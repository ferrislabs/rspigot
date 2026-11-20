use rand::Rng;
use rspigot_blocks::BlockKind;
use rspigot_domain::entity::physics::has_tag;
use rspigot_domain::game_event::GameEvent;
use rspigot_domain::item::ItemStack;
use rspigot_domain::player::{GameMode, PlayerId};
use rspigot_domain::{BlockPos, BlockStateId, EntityId};
use rspigot_entities::SoundSource;
use rspigot_items::ItemKind;

use crate::block_tools::ToolUse;
use crate::campfire::is_campfire;
use crate::events::{Direction, Hand};
use crate::game_events::GameEventContext;
use crate::plugins::events::{BlockIgnite, BlockIgniteCause};
use crate::plugins::{Continuation, EventPayload};
use crate::ports::ClientLink;
use crate::server::Server;

const FLINT_SOUND: &str = "minecraft:item.flintandsteel.use";
const CHARGE_SOUND: &str = "minecraft:item.firecharge.use";
const CANDLE_SOUND: &str = "minecraft:block.candle.extinguish";
const CANDLES: &str = "minecraft:candles";
const CANDLE_CAKES: &str = "minecraft:candle_cakes";
const SHOVELS: &str = "minecraft:shovels";
const CAMPFIRE_DOUSE_EVENT: i32 = 1009;
const LIT: &str = "lit";
const WATERLOGGED: &str = "waterlogged";
const CAKE_CANDLE_HEIGHT: f64 = 0.5;

#[derive(Debug)]
pub(crate) struct Striker {
    pub(crate) player: PlayerId,
    pub(crate) hand: Hand,
    pub(crate) held: ItemStack,
    pub(crate) sequence: i32,
    pub(crate) flint: bool,
}

#[derive(Debug)]
pub(crate) enum Spark {
    Player(Striker),
    Projectile(EntityId),
}

#[derive(Debug)]
pub(crate) struct LightStep {
    pub(crate) pos: BlockPos,
    pub(crate) before: BlockStateId,
    pub(crate) state: BlockStateId,
    pub(crate) spark: Spark,
}

fn flag(state: BlockStateId, name: &str) -> Option<bool> {
    state.property(name).map(|value| value == "true")
}

pub(crate) fn can_light(state: BlockStateId) -> bool {
    let unlit = flag(state, LIT) == Some(false);
    if is_campfire(state) || has_tag(state, CANDLES) {
        unlit && flag(state, WATERLOGGED) == Some(false)
    } else {
        unlit && has_tag(state, CANDLE_CAKES)
    }
}

fn is_lit_candle(state: BlockStateId) -> bool {
    flag(state, LIT) == Some(true) && has_tag(state, CANDLES)
}

fn is_lit_candle_cake(state: BlockStateId) -> bool {
    flag(state, LIT) == Some(true) && has_tag(state, CANDLE_CAKES)
}

fn is_lit_campfire(state: BlockStateId) -> bool {
    flag(state, LIT) == Some(true) && is_campfire(state)
}

fn is_shovel(stack: &ItemStack) -> bool {
    let item = u32::from(stack.item.0);
    rspigot_data::tags::tag("minecraft:item", SHOVELS)
        .is_some_and(|ids| ids.binary_search(&item).is_ok())
}

fn builds(game_mode: GameMode) -> bool {
    !matches!(game_mode, GameMode::Adventure | GameMode::Spectator)
}

impl<L: ClientLink> Server<L> {
    fn fire_placement(&self, target: BlockPos) -> Option<BlockStateId> {
        if !self.world.chunks.block(target)?.is_air() {
            return None;
        }
        let state = self.fire_state_with_age(target, 0);
        (state.kind() == Some(BlockKind::SoulFire) || self.fire_survives(target)).then_some(state)
    }

    pub(crate) fn use_fire_starter(
        &mut self,
        id: PlayerId,
        hand: Hand,
        held: &ItemStack,
        pos: BlockPos,
        face: Direction,
        sequence: i32,
    ) -> bool {
        let flint = held.item == ItemKind::FlintAndSteel.id();
        if !flint && held.item != ItemKind::FireCharge.id() {
            return false;
        }
        let Some(player) = self.players.get(&id) else {
            return false;
        };
        if !builds(player.game_mode) {
            return false;
        }
        let Some(clicked) = self.world.chunks.block(pos) else {
            return false;
        };
        let (target, before, state) = if can_light(clicked) {
            let Some(lit) = clicked.with_property(LIT, "true") else {
                return false;
            };
            (pos, clicked, lit)
        } else {
            let (dx, dy, dz) = face.offset();
            let target = pos.offset(dx, dy, dz);
            let Some(fire) = self.fire_placement(target) else {
                return false;
            };
            (target, BlockStateId::AIR, fire)
        };
        if !self.within_reach(id, target) || self.is_protected(id, target) {
            self.refuse_block(id, target, sequence);
            return true;
        }
        let cause = if flint {
            BlockIgniteCause::FlintAndSteel
        } else {
            BlockIgniteCause::Fireball
        };
        let step = LightStep {
            pos: target,
            before,
            state,
            spark: Spark::Player(Striker {
                player: id,
                hand,
                held: held.clone(),
                sequence,
                flint,
            }),
        };
        self.fire(
            crate::plugins::Lane::Player(id),
            Continuation::Light(Box::new(step)),
            move |server| {
                let entity = server.players.get(&id)?.entity_id;
                Some(EventPayload::BlockIgnite(BlockIgnite {
                    block: target,
                    cause,
                    igniting_block: None,
                    igniting_entity: server.subject_view(entity),
                    player: server.snapshot_of(id),
                }))
            },
        );
        true
    }

    pub(crate) fn arrow_lights(&mut self, arrow: EntityId, pos: BlockPos, state: BlockStateId) {
        if !can_light(state) {
            return;
        }
        let Some(lit) = state.with_property(LIT, "true") else {
            return;
        };
        let Some(view) = self.entity_view(arrow) else {
            return;
        };
        let lane = self.lane_for(None, arrow);
        let step = LightStep {
            pos,
            before: state,
            state: lit,
            spark: Spark::Projectile(arrow),
        };
        self.fire(lane, Continuation::Light(Box::new(step)), move |_| {
            Some(EventPayload::BlockIgnite(BlockIgnite {
                block: pos,
                cause: BlockIgniteCause::Arrow,
                igniting_block: None,
                igniting_entity: Some(view),
                player: None,
            }))
        });
    }

    pub(crate) fn finish_light(&mut self, step: LightStep, cancelled: bool) {
        let LightStep {
            pos,
            before,
            state,
            spark,
            ..
        } = step;
        let intact = !cancelled && self.world.chunks.block(pos) == Some(before);
        match spark {
            Spark::Projectile(arrow) => {
                if intact && self.entities.store.get(arrow).is_some() {
                    self.set_block(pos, state);
                    self.block_game_event(
                        GameEvent::BlockChange,
                        pos,
                        GameEventContext::of(Some(arrow)),
                    );
                }
            }
            Spark::Player(striker) => self.finish_strike(pos, state, striker, intact),
        }
    }

    fn finish_strike(
        &mut self,
        pos: BlockPos,
        state: BlockStateId,
        striker: Striker,
        intact: bool,
    ) {
        let Striker {
            player,
            hand,
            held,
            sequence,
            flint,
        } = striker;
        let holding = self.held(player, hand).as_ref() == Some(&held);
        let Some(entity) = self.players.get(&player).map(|p| p.entity_id) else {
            return;
        };
        if !intact || !holding {
            self.refuse_block(player, pos, sequence);
            self.send_inventory(player);
            return;
        }
        self.set_block(pos, state);
        let (sound, source, pitch) = if flint {
            (
                FLINT_SOUND,
                SoundSource::Players,
                self.rng.random::<f32>() * 0.4 + 0.8,
            )
        } else {
            (
                CHARGE_SOUND,
                SoundSource::Blocks,
                (self.rng.random::<f32>() - self.rng.random::<f32>()) * 0.2 + 1.0,
            )
        };
        self.play_sound(Some(player), pos.center(), sound, source, 1.0, pitch);
        self.block_game_event(
            GameEvent::BlockChange,
            pos,
            GameEventContext::of(Some(entity)),
        );
        if flint {
            self.hurt_held(player, hand, 1);
        } else {
            self.consume_held(player, hand, 1);
        }
        self.refresh_hands(player);
        self.player_swing(player, hand, false);
        self.ack_block(player, sequence);
    }

    pub(crate) fn snuff_candle(
        &mut self,
        id: PlayerId,
        held: Option<&ItemStack>,
        pos: BlockPos,
        clicked: BlockStateId,
        cursor_y: f64,
        sequence: i32,
    ) -> bool {
        let snuffable = is_lit_candle(clicked)
            || (is_lit_candle_cake(clicked) && cursor_y > CAKE_CANDLE_HEIGHT);
        let Some(player) = self.players.get(&id) else {
            return false;
        };
        if held.is_some() || !snuffable || !builds(player.game_mode) {
            return false;
        }
        let entity = player.entity_id;
        let Some(unlit) = clicked.with_property(LIT, "false") else {
            return false;
        };
        self.set_block(pos, unlit);
        self.play_sound(
            None,
            pos.center(),
            CANDLE_SOUND,
            SoundSource::Blocks,
            1.0,
            1.0,
        );
        self.block_game_event(
            GameEvent::BlockChange,
            pos,
            GameEventContext::of(Some(entity)),
        );
        self.ack_block(id, sequence);
        true
    }

    pub(crate) fn douse_with_shovel(
        &mut self,
        id: PlayerId,
        held: &ItemStack,
        usage: &ToolUse,
        sequence: i32,
    ) -> bool {
        let (hand, pos, face, clicked) = (usage.hand, usage.pos, usage.face, usage.clicked);
        if face == Direction::Down || !is_lit_campfire(clicked) || !is_shovel(held) {
            return false;
        }
        let Some(player) = self.players.get(&id) else {
            return false;
        };
        let entity = player.entity_id;
        if !builds(player.game_mode) || !self.within_reach(id, pos) || self.is_protected(id, pos) {
            return false;
        }
        let Some(unlit) = clicked.with_property(LIT, "false") else {
            return false;
        };
        self.level_event(CAMPFIRE_DOUSE_EVENT, pos, 0);
        self.set_block(pos, unlit);
        self.block_game_event(
            GameEvent::BlockChange,
            pos,
            GameEventContext::of(Some(entity)),
        );
        self.hurt_held(id, hand, 1);
        self.refresh_hands(id);
        self.player_swing(id, hand, false);
        self.ack_block(id, sequence);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(kind: BlockKind, properties: &[(&str, &str)]) -> BlockStateId {
        kind.state_with(properties).expect("a valid state")
    }

    #[test]
    fn an_unlit_dry_campfire_can_be_lit() {
        let campfire = state(
            BlockKind::Campfire,
            &[("lit", "false"), ("waterlogged", "false")],
        );
        assert!(can_light(campfire));
    }

    #[test]
    fn a_lit_or_waterlogged_campfire_cannot_be_lit() {
        let lit = state(
            BlockKind::Campfire,
            &[("lit", "true"), ("waterlogged", "false")],
        );
        let wet = state(
            BlockKind::SoulCampfire,
            &[("lit", "false"), ("waterlogged", "true")],
        );
        assert!(!can_light(lit));
        assert!(!can_light(wet));
    }

    #[test]
    fn an_unlit_candle_cake_can_be_lit_and_stone_cannot() {
        let cake = state(BlockKind::WhiteCandleCake, &[("lit", "false")]);
        assert!(can_light(cake));
        assert!(!can_light(BlockKind::Stone.default_state()));
    }
}
