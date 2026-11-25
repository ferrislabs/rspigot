use rspigot_blocks::BlockKind;
use rspigot_domain::campfire::{Campfire, SLOTS};
use rspigot_domain::chunk::BlockEntity;
use rspigot_domain::item::ItemStack;
use rspigot_domain::player::{GameMode, PlayerId};
use rspigot_domain::{BlockPos, BlockStateId, Identifier};
use rspigot_nbt::{Compound, Tag};

use crate::containers::{read_items, write_items};
use crate::events::{Animation, ClientEvent, Hand};
use crate::plugins::events::{BlockCookPatch, CampfireStart, CampfireStartPatch};
use crate::plugins::steps::{CampfireStep, CookStep};
use crate::plugins::{Continuation, EventPayload, Lane, spec_of, stack_of};
use crate::ports::ClientLink;
use crate::server::{Audience, Server};

const ENTITY: &str = "campfire";
const COOKING_TIMES: &str = "CookingTimes";
const COOKING_TOTAL_TIMES: &str = "CookingTotalTimes";
const LIT: &str = "lit";
const WATERLOGGED: &str = "waterlogged";

pub(crate) fn is_campfire(state: BlockStateId) -> bool {
    matches!(
        state.kind(),
        Some(BlockKind::Campfire | BlockKind::SoulCampfire)
    )
}

pub(crate) fn is_lit(state: BlockStateId) -> bool {
    is_campfire(state) && state.property(LIT) == Some("true")
}

pub(crate) fn is_waterlogged(state: BlockStateId) -> bool {
    is_campfire(state) && state.property(WATERLOGGED) == Some("true")
}

pub(crate) fn can_waterlog(state: BlockStateId) -> bool {
    is_campfire(state) && state.property(WATERLOGGED) == Some("false")
}

pub(crate) fn doused(state: BlockStateId) -> BlockStateId {
    if is_lit(state) && is_waterlogged(state) {
        state.with_property(LIT, "false").unwrap_or(state)
    } else {
        state
    }
}

pub(crate) fn placed(state: BlockStateId, replaced: BlockStateId) -> BlockStateId {
    if !is_campfire(state) {
        return state;
    }
    let water =
        replaced.kind() == Some(BlockKind::Water) && replaced.property("level") == Some("0");
    let (lit, waterlogged) = if water {
        ("false", "true")
    } else {
        ("true", "false")
    };
    state
        .with_property(LIT, lit)
        .and_then(|state| state.with_property(WATERLOGGED, waterlogged))
        .unwrap_or(state)
}

fn read_counters(data: &Compound, key: &str) -> [i32; SLOTS] {
    let mut counters = [0; SLOTS];
    if let Some(values) = data.get_int_array(key) {
        for (counter, value) in counters.iter_mut().zip(values) {
            *counter = *value;
        }
    }
    counters
}

pub(crate) fn read_campfire(data: &Compound) -> Campfire {
    let mut items = read_items(data, SLOTS).into_iter();
    Campfire {
        items: std::array::from_fn(|_| items.next().flatten()),
        progress: read_counters(data, COOKING_TIMES),
        total: read_counters(data, COOKING_TOTAL_TIMES),
    }
}

pub(crate) fn write_campfire(campfire: &Campfire, data: &mut Compound) {
    write_items(data, &campfire.items);
    data.insert(COOKING_TIMES, Tag::IntArray(campfire.progress.to_vec()));
    data.insert(COOKING_TOTAL_TIMES, Tag::IntArray(campfire.total.to_vec()));
}

fn update_tag(campfire: &Campfire) -> Compound {
    let mut tag = Compound::new();
    write_items(&mut tag, &campfire.items);
    tag
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn campfire_at(&self, pos: BlockPos) -> Option<Campfire> {
        self.world
            .chunks
            .get(pos.chunk())
            .and_then(|chunk| chunk.block_entity(pos))
            .map(|entity| read_campfire(&entity.data))
    }

    fn store_campfire(&mut self, pos: BlockPos, campfire: &Campfire) {
        let Some(chunk) = self.world.chunks.chunk_mut(pos.chunk()) else {
            return;
        };
        let (kind, mut data) = chunk.block_entity(pos).map_or_else(
            || (Identifier::minecraft(ENTITY), Compound::new()),
            |existing| (existing.kind.clone(), existing.data.clone()),
        );
        write_campfire(campfire, &mut data);
        chunk.set_block_entity(BlockEntity { pos, kind, data });
    }

    fn send_campfire_update(&mut self, pos: BlockPos, campfire: &Campfire) {
        self.push(
            Audience::ChunkViewers(pos.chunk()),
            ClientEvent::BlockEntityUpdate {
                pos,
                kind: ENTITY,
                data: update_tag(campfire),
            },
        );
    }

    pub(crate) fn has_block_entity(&self, pos: BlockPos) -> bool {
        self.world
            .chunks
            .get(pos.chunk())
            .is_some_and(|chunk| chunk.block_entity(pos).is_some())
    }

    pub(crate) fn block_entity_update(&self, pos: BlockPos) -> Option<ClientEvent> {
        let entity = self.world.chunks.get(pos.chunk())?.block_entity(pos)?;
        if entity.kind.as_str() != "minecraft:campfire" {
            return None;
        }
        let campfire = read_campfire(&entity.data);
        Some(ClientEvent::BlockEntityUpdate {
            pos,
            kind: ENTITY,
            data: update_tag(&campfire),
        })
    }

    pub(crate) fn campfire_items_lost_by(
        &self,
        pos: BlockPos,
        old: BlockStateId,
        new: BlockStateId,
    ) -> Vec<ItemStack> {
        if old.kind() == new.kind() || !is_campfire(old) {
            return Vec::new();
        }
        self.campfire_at(pos)
            .map(|campfire| campfire.items.into_iter().flatten().collect())
            .unwrap_or_default()
    }

    pub(crate) fn tick_campfires(&mut self) {
        let mut found: Vec<BlockPos> = Vec::new();
        for chunk_pos in self
            .world
            .chunks
            .chunks_with_block_entities(|kind| kind.as_str() == "minecraft:campfire")
        {
            let Some(chunk) = self.world.chunks.get(chunk_pos) else {
                continue;
            };
            found.extend(
                chunk
                    .block_entities
                    .iter()
                    .filter(|entity| entity.kind.as_str() == "minecraft:campfire")
                    .map(|entity| entity.pos),
            );
        }
        for pos in found {
            self.tick_campfire(pos);
        }
    }

    fn tick_campfire(&mut self, pos: BlockPos) {
        let Some(state) = self.world.chunks.block(pos).filter(|s| is_campfire(*s)) else {
            return;
        };
        let Some(mut campfire) = self.campfire_at(pos) else {
            return;
        };
        if !is_lit(state) {
            if campfire.cool() {
                self.store_campfire(pos, &campfire);
            }
            return;
        }
        let mut changed = false;
        for slot in 0..SLOTS {
            if self.cooking(pos, slot) {
                continue;
            }
            changed |= campfire.items[slot].is_some();
            let Some((source, result)) = campfire.advance(slot) else {
                continue;
            };
            self.store_campfire(pos, &campfire);
            self.ask_cook(CookStep {
                pos,
                slot,
                source,
                result,
            });
            let Some(current) = self.campfire_at(pos) else {
                return;
            };
            campfire = current;
            changed = false;
        }
        if changed {
            self.store_campfire(pos, &campfire);
        }
    }

    pub(crate) fn finish_cook(&mut self, step: CookStep, patch: BlockCookPatch) {
        let pos = step.pos;
        let lit = self
            .world
            .chunks
            .block(pos)
            .is_some_and(|state| is_campfire(state) && is_lit(state));
        let Some(mut campfire) = self.campfire_at(pos).filter(|_| lit) else {
            return;
        };
        if campfire.items[step.slot].as_ref() != Some(&step.source) {
            return;
        }
        let result = match patch.result {
            Some(spec) => stack_of(&spec).unwrap_or(Some(step.result)),
            None => Some(step.result),
        };
        campfire.items[step.slot] = None;
        self.store_campfire(pos, &campfire);
        self.send_campfire_update(pos, &campfire);
        self.drop_contents(pos, result.into_iter().collect());
    }

    pub(crate) fn place_food_on_campfire(
        &mut self,
        id: PlayerId,
        hand: Hand,
        slot: usize,
        pos: BlockPos,
        clicked: BlockStateId,
        sequence: i32,
    ) -> bool {
        if !is_campfire(clicked) {
            return false;
        }
        let Some(player) = self.players.get(&id) else {
            return false;
        };
        let game_mode = player.game_mode;
        let Some(held) = player
            .inventory
            .get(slot)
            .filter(|stack| Campfire::can_cook(stack))
            .cloned()
        else {
            return false;
        };
        if game_mode == GameMode::Spectator {
            return false;
        }
        self.ensure_container_entity(pos, clicked);
        let found = self
            .campfire_at(pos)
            .and_then(|campfire| Some((campfire.free_slot()?, Campfire::recipe(&held)?)));
        let Some((cooking_slot, recipe)) = found else {
            self.ack_block(id, sequence);
            return true;
        };
        let total = i32::from(recipe.cooking_time);
        let recipe_id = recipe.id;
        let step = CampfireStep {
            player: id,
            hand,
            slot,
            pos,
            sequence,
            held,
            game_mode,
            cooking_slot,
            total,
        };
        self.fire(
            id,
            Continuation::CampfireStart(Box::new(step)),
            move |server| {
                let held = server.players.get(&id)?.inventory.get(slot)?;
                Some(EventPayload::CampfireStart(CampfireStart {
                    block: pos,
                    recipe: recipe_id.to_owned(),
                    source: spec_of(held),
                    total_cook_time: total,
                }))
            },
        );
        true
    }

    pub(crate) fn finish_campfire_start(&mut self, step: CampfireStep, patch: CampfireStartPatch) {
        let id = step.player;
        let holding = self.players.get(&id).is_some_and(|player| {
            player.game_mode == step.game_mode
                && player.inventory.get(step.slot) == Some(&step.held)
        });
        let still = self.world.chunks.block(step.pos).is_some_and(is_campfire);
        let free = self
            .campfire_at(step.pos)
            .filter(|campfire| campfire.items[step.cooking_slot].is_none());
        let (true, true, Some(mut campfire)) = (holding, still, free) else {
            self.ack_block(id, step.sequence);
            return;
        };
        let total = patch.total_cook_time.unwrap_or(step.total);
        campfire.put(step.cooking_slot, &step.held, total);
        self.store_campfire(step.pos, &campfire);
        self.send_campfire_update(step.pos, &campfire);
        self.award_custom(id, crate::stats::custom::INTERACT_WITH_CAMPFIRE, 1);
        if step.game_mode != GameMode::Creative {
            self.consume_one(id, step.slot, step.held);
        }
        if let Some(entity) = self.players.get(&id).map(|player| player.entity_id) {
            let swing = ClientEvent::EntityAnimation {
                entity,
                animation: match step.hand {
                    Hand::Main => Animation::SwingMainArm,
                    Hand::Off => Animation::SwingOffHand,
                },
            };
            self.push(Audience::One(id), swing.clone());
            self.push(Audience::Trackers(entity), swing);
        }
        self.ack_block(id, step.sequence);
    }

    fn cooking(&self, pos: BlockPos, slot: usize) -> bool {
        self.decisions.in_flight(
            Lane::Block(pos),
            |then| matches!(then, Continuation::Cook(step) if step.slot == slot),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn campfire(lit: &str, waterlogged: &str) -> BlockStateId {
        BlockKind::Campfire
            .state_with(&[("lit", lit), ("waterlogged", waterlogged)])
            .expect("a valid state")
    }

    #[test]
    fn a_lit_waterlogged_campfire_is_put_out() {
        let state = doused(campfire("true", "true"));
        assert_eq!(state.property("lit"), Some("false"));
        assert_eq!(state.property("waterlogged"), Some("true"));
    }

    #[test]
    fn a_dry_lit_campfire_is_left_alone() {
        let state = campfire("true", "false");
        assert_eq!(doused(state), state);
    }

    #[test]
    fn placing_in_a_water_source_waterlogs_and_leaves_it_unlit() {
        let water = BlockKind::Water.default_state();
        let state = placed(campfire("true", "false"), water);
        assert_eq!(state.property("lit"), Some("false"));
        assert_eq!(state.property("waterlogged"), Some("true"));
    }

    #[test]
    fn placing_in_air_lights_it() {
        let state = placed(campfire("false", "false"), BlockStateId::AIR);
        assert_eq!(state.property("lit"), Some("true"));
        assert_eq!(state.property("waterlogged"), Some("false"));
    }
}
