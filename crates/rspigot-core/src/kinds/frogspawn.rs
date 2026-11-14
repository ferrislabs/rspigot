use rand::Rng;
use rspigot_blocks::BlockKind;
use rspigot_domain::entity::Entity;
use rspigot_domain::random::LegacyRandom;
use rspigot_domain::{BlockPos, BlockStateId, Vec3};
use rspigot_entities::{EntityKind as Kind, SoundSource};

use crate::mob_actions::new_mob;
use crate::plugins::Birth;
use crate::plugins::events::CreatureSpawnReason;
use crate::ports::ClientLink;
use crate::server::Server;

pub(crate) const MIN_HATCH_DELAY: u64 = 3600;
pub(crate) const MAX_HATCH_DELAY: u64 = 12_000;
const MIN_TADPOLES: i32 = 2;
const MAX_TADPOLES: i32 = 6;
const OFFSET_MIN: f64 = 0.200_000_002_980_232_24;
const OFFSET_MAX: f64 = 0.799_999_997_019_767_8;
const SPAWN_DROP: f64 = 0.5;
const HATCH_SOUND: &str = "minecraft:block.frogspawn.hatch";
const SUPPORTS_FROGSPAWN: &str = "minecraft:supports_frogspawn";
const BLOCK_BREAK_EVENT: i32 = 2001;

fn is_frogspawn(state: BlockStateId) -> bool {
    state.kind() == Some(BlockKind::Frogspawn)
}

impl<L: ClientLink> Server<L> {
    fn frog_block_at(&self, pos: BlockPos) -> BlockStateId {
        self.world.chunks.block(pos).unwrap_or(BlockStateId::AIR)
    }

    pub(crate) fn frogspawn_may_place_on(&self, pos: BlockPos) -> bool {
        let state = self.frog_block_at(pos);
        let above = self.frog_block_at(pos.offset(0, 1, 0));
        let supports = rspigot_ai::fluid_in_tag(Some(state), SUPPORTS_FROGSPAWN)
            || rspigot_ai::block_in_tag(Some(state), SUPPORTS_FROGSPAWN);
        supports && rspigot_ai::fluid_id(Some(above)) == 0
    }

    pub(crate) fn frogspawn_survives(&self, pos: BlockPos) -> bool {
        self.frogspawn_may_place_on(pos.offset(0, -1, 0))
    }

    pub(crate) fn frogspawn_placed(&mut self) -> u64 {
        self.rng.random_range(MIN_HATCH_DELAY..MAX_HATCH_DELAY)
    }

    pub(crate) fn frogspawn_neighbour_changed(&mut self, pos: BlockPos) {
        if is_frogspawn(self.frog_block_at(pos)) && !self.frogspawn_survives(pos) {
            self.set_block(pos, BlockStateId::AIR);
        }
    }

    pub(crate) fn frogspawn_tick(&mut self, pos: BlockPos) {
        if !is_frogspawn(self.frog_block_at(pos)) {
            return;
        }
        if !self.frogspawn_survives(pos) {
            self.destroy_frogspawn(pos);
            return;
        }
        self.destroy_frogspawn(pos);
        self.play_sound(
            None,
            Vec3::new(
                f64::from(pos.x) + 0.5,
                f64::from(pos.y) + 0.5,
                f64::from(pos.z) + 0.5,
            ),
            HATCH_SOUND,
            SoundSource::Blocks,
            1.0,
            1.0,
        );
        self.spawn_tadpoles(pos);
    }

    pub(crate) fn frogspawn_crushed(&mut self, pos: BlockPos) {
        if is_frogspawn(self.frog_block_at(pos)) {
            self.destroy_frogspawn(pos);
        }
    }

    fn destroy_frogspawn(&mut self, pos: BlockPos) {
        let state = self.frog_block_at(pos);
        self.level_event(BLOCK_BREAK_EVENT, pos, i32::from(state.0));
        self.set_block(pos, BlockStateId::AIR);
    }

    fn spawn_tadpoles(&mut self, pos: BlockPos) {
        let count = self.rng.random_range(MIN_TADPOLES..MAX_TADPOLES);
        for _ in 0..count {
            let offset_x = self.rng.random::<f64>().clamp(OFFSET_MIN, OFFSET_MAX);
            let offset_z = self.rng.random::<f64>().clamp(OFFSET_MIN, OFFSET_MAX);
            let yaw = self.rng.random_range(1..361) as f32;
            let mut tadpole: Entity = new_mob(self.new_entity_base(Kind::Tadpole));
            tadpole.base.snap_to(
                Vec3::new(
                    f64::from(pos.x) + offset_x,
                    f64::from(pos.y) - SPAWN_DROP,
                    f64::from(pos.z) + offset_z,
                ),
                yaw,
                0.0,
            );
            if let Some(mob) = tadpole.living_mut().and_then(|living| living.mob.as_mut()) {
                mob.persistence_required = true;
            }
            let own = LegacyRandom::new(self.rng.random());
            self.spawn_entity(
                tadpole,
                Birth::of(CreatureSpawnReason::Breeding).with_random(own),
            );
        }
    }
}
