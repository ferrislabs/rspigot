use rand::Rng;
use rspigot_blocks::infested::{host_state_by_infested, is_infested};
use rspigot_domain::entity::save;
use rspigot_domain::item::ItemStack;
use rspigot_domain::random::{LegacyRandom, RandomSource};
use rspigot_domain::{BlockPos, BlockStateId, EntityId};
use rspigot_entities::EntityKind as Kind;
use rspigot_nbt::Compound;

use crate::loot::{self, LootContext};
use crate::mobs::loot_facts_of;
use crate::plugins::events::CreatureSpawnReason;
use crate::plugins::steps::{Birth, Grief, Spawned};
use crate::ports::ClientLink;
use crate::server::Server;

const BLOCK_BREAK_EVENT: i32 = 2001;
const ENCHANTMENTS: &str = "minecraft:enchantment";
const PREVENTS_INFESTED_SPAWNS: &str = "minecraft:prevents_infested_spawns";
const FRIEND_RANGE_Y: i32 = 5;
const FRIEND_RANGE_HORIZONTAL: i32 = 10;
const Y_SPAN: u32 = 2 * FRIEND_RANGE_Y as u32 + 1;
const HORIZONTAL_SPAN: u32 = 2 * FRIEND_RANGE_HORIZONTAL as u32 + 1;
pub(crate) const FRIEND_CELLS: u32 = Y_SPAN * HORIZONTAL_SPAN * HORIZONTAL_SPAN;

fn alternating(step: u32) -> i32 {
    let half = step.div_ceil(2) as i32;
    if step % 2 == 1 { half } else { -half }
}

pub(crate) fn friend_offset(index: u32) -> (i32, i32, i32) {
    let z = index % HORIZONTAL_SPAN;
    let x = index / HORIZONTAL_SPAN % HORIZONTAL_SPAN;
    let y = index / (HORIZONTAL_SPAN * HORIZONTAL_SPAN);
    (alternating(x), alternating(y), alternating(z))
}

fn prevents_infested_spawns(tool: Option<&ItemStack>) -> bool {
    let Some(members) = rspigot_data::tags::tag(ENCHANTMENTS, PREVENTS_INFESTED_SPAWNS) else {
        return false;
    };
    tool.is_some_and(|stack| {
        stack.enchantments().iter().any(|(name, level)| {
            *level > 0
                && rspigot_data::registries::synced_index(ENCHANTMENTS, name)
                    .is_some_and(|id| members.binary_search(&id).is_ok())
        })
    })
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn spawn_after_break(
        &mut self,
        pos: BlockPos,
        state: BlockStateId,
        tool: Option<&ItemStack>,
    ) {
        if is_infested(state) && self.config.rules.block_drops && !prevents_infested_spawns(tool) {
            self.spawn_infestation(pos);
        }
    }

    fn spawn_infestation(&mut self, pos: BlockPos) {
        let id = self.allocate_entity_id();
        let uuid = self.random_uuid();
        let compound = Compound::new().with("id", Kind::Silverfish.name());
        let Ok(mut entity) = save::load_entity(&compound, id, uuid) else {
            return;
        };
        entity.base.snap_to(pos.center(), 0.0, 0.0);
        let own = LegacyRandom::new(self.rng.random());
        self.spawn_entity(
            entity,
            Birth::of(CreatureSpawnReason::SilverfishBlock)
                .with_random(own)
                .then(Spawned::Poof),
        );
    }

    pub(crate) fn destroy_block_dropping(&mut self, pos: BlockPos, breaker: Option<EntityId>) {
        let Some(state) = self.world.chunks.block(pos).filter(|state| !state.is_air()) else {
            return;
        };
        self.level_event(BLOCK_BREAK_EVENT, pos, i32::from(state.0));
        let facts = breaker
            .and_then(|id| self.entities.store.get(id))
            .map(loot_facts_of);
        let context = LootContext::block(state, None, facts.as_ref());
        if self.config.rules.block_drops {
            for stack in loot::block_drops(&context, &mut self.rng) {
                self.spawn_item(pos, stack);
            }
        }
        self.spawn_after_break(pos, state, None);
        self.set_block(pos, BlockStateId::AIR);
    }

    pub(crate) fn scan_friends(&mut self, silverfish: EntityId, origin: BlockPos, from: u32) {
        for index in from..FRIEND_CELLS {
            let (dx, dy, dz) = friend_offset(index);
            let pos = origin.offset(dx, dy, dz);
            if !self.world.chunks.block(pos).is_some_and(is_infested) {
                continue;
            }
            let Some(view) = self.entity_view(silverfish) else {
                return;
            };
            let change = Grief::WakeFriends {
                silverfish,
                origin,
                next: index + 1,
                pos,
            };
            self.change_block(view, pos, BlockStateId::AIR, change, false);
            return;
        }
    }

    pub(crate) fn finish_wake_friend(
        &mut self,
        silverfish: EntityId,
        origin: BlockPos,
        next: u32,
        pos: BlockPos,
        cancelled: bool,
    ) {
        if !self.living_entity_alive(silverfish) {
            return;
        }
        if !cancelled {
            if self.config.rules.mob_griefing {
                self.destroy_block_dropping(pos, Some(silverfish));
            } else if let Some(host) = self
                .world
                .chunks
                .block(pos)
                .and_then(host_state_by_infested)
            {
                self.set_block(pos, host);
            }
            let mut random = self.mob_random(silverfish);
            let stop = random.next_boolean();
            self.store_mob_random(silverfish, random);
            if stop {
                return;
            }
        }
        self.scan_friends(silverfish, origin, next);
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU8;

    use rspigot_domain::ItemId;

    use super::*;

    fn pick(enchantments: &[(&str, u8)]) -> ItemStack {
        let item = rspigot_items::ItemKind::DiamondPickaxe.id();
        let mut stack = ItemStack::new(ItemId(item.0), NonZeroU8::MIN);
        let levels: Vec<(String, u8)> = enchantments
            .iter()
            .map(|(name, level)| ((*name).to_owned(), *level))
            .collect();
        stack.set_enchantments(&levels);
        stack
    }

    #[test]
    fn silk_touch_prevents_the_infestation() {
        assert!(prevents_infested_spawns(Some(&pick(&[(
            "minecraft:silk_touch",
            1
        )]))));
    }

    #[test]
    fn other_tools_do_not() {
        assert!(!prevents_infested_spawns(None));
        assert!(!prevents_infested_spawns(Some(&pick(&[]))));
        assert!(!prevents_infested_spawns(Some(&pick(&[(
            "minecraft:efficiency",
            5
        )]))));
        assert!(!prevents_infested_spawns(Some(&pick(&[(
            "minecraft:fortune",
            3
        )]))));
    }

    #[test]
    fn friend_offsets_follow_the_vanilla_loop_order() {
        assert_eq!(friend_offset(0), (0, 0, 0));
        assert_eq!(friend_offset(1), (0, 0, 1));
        assert_eq!(friend_offset(2), (0, 0, -1));
        assert_eq!(friend_offset(20), (0, 0, -10));
        assert_eq!(friend_offset(21), (1, 0, 0));
        assert_eq!(friend_offset(42), (-1, 0, 0));
        assert_eq!(friend_offset(441), (0, 1, 0));
        assert_eq!(friend_offset(FRIEND_CELLS - 1), (-10, -5, -10));
    }

    #[test]
    fn friend_offsets_cover_the_cube_once() {
        let mut seen = std::collections::HashSet::new();
        for index in 0..FRIEND_CELLS {
            let (x, y, z) = friend_offset(index);
            assert!(x.abs() <= 10 && y.abs() <= 5 && z.abs() <= 10);
            assert!(seen.insert((x, y, z)));
        }
        assert_eq!(seen.len(), 21 * 11 * 21);
    }
}
