use rand::Rng;
use rspigot_blocks::BlockKind;
use rspigot_domain::entity::{EntityState, ageable};
use rspigot_domain::game_event::GameEvent;
use rspigot_domain::random::LegacyRandom;
use rspigot_domain::{BlockPos, BlockStateId, Vec3};
use rspigot_entities::{EntityKind as Kind, SoundSource};

use crate::game_events::GameEventContext;
use crate::mob_actions::new_mob;
use crate::plugins::Birth;
use crate::plugins::events::CreatureSpawnReason;
use crate::ports::ClientLink;
use crate::server::Server;

pub(crate) const HYDRATION_TICK_DELAY: u64 = 5000;
const MAX_HYDRATION: i32 = 3;
const TRANSITION_SOUND: &str = "minecraft:block.dried_ghast.transition";
const SPAWN_SOUND: &str = "minecraft:entity.ghastling.spawn";

fn hydration(state: BlockStateId) -> i32 {
    state
        .property("hydration")
        .and_then(|level| level.parse().ok())
        .unwrap_or(0)
}

fn waterlogged(state: BlockStateId) -> bool {
    state.property("waterlogged") == Some("true")
}

fn yaw_of(state: BlockStateId) -> f32 {
    match state.property("facing") {
        Some("west") => 90.0,
        Some("north") => 180.0,
        Some("east") => 270.0,
        _ => 0.0,
    }
}

fn is_dried_ghast(state: BlockStateId) -> bool {
    state.kind() == Some(BlockKind::DriedGhast)
}

impl<L: ClientLink> Server<L> {
    fn dried_ghast_at(&self, pos: BlockPos) -> Option<BlockStateId> {
        self.world
            .chunks
            .block(pos)
            .filter(|state| is_dried_ghast(*state))
    }

    pub(crate) fn dried_ghast_random_tick(&mut self, pos: BlockPos) {
        let Some(state) = self.dried_ghast_at(pos) else {
            return;
        };
        if waterlogged(state) || hydration(state) > 0 {
            self.schedule(pos, BlockKind::DriedGhast, HYDRATION_TICK_DELAY);
        }
    }

    pub(crate) fn dried_ghast_tick(&mut self, pos: BlockPos) {
        let Some(state) = self.dried_ghast_at(pos) else {
            return;
        };
        let level = hydration(state);
        let centre = Vec3::new(
            f64::from(pos.x) + 0.5,
            f64::from(pos.y) + 0.5,
            f64::from(pos.z) + 0.5,
        );
        if !waterlogged(state) {
            if level > 0 {
                self.change_hydration(pos, state, level - 1);
            }
            return;
        }
        if level < MAX_HYDRATION {
            self.play_sound(
                None,
                centre,
                TRANSITION_SOUND,
                SoundSource::Blocks,
                1.0,
                1.0,
            );
            self.change_hydration(pos, state, level + 1);
        } else {
            self.spawn_ghastling(pos, state);
        }
    }

    fn change_hydration(&mut self, pos: BlockPos, state: BlockStateId, level: i32) {
        let Some(next) = state.with_property("hydration", &level.to_string()) else {
            return;
        };
        self.set_block(pos, next);
        self.block_game_event(GameEvent::BlockChange, pos, GameEventContext::block(state));
    }

    fn spawn_ghastling(&mut self, pos: BlockPos, state: BlockStateId) {
        self.set_block(pos, BlockStateId::AIR);
        let yaw = yaw_of(state);
        let at = Vec3::new(
            f64::from(pos.x) + 0.5,
            f64::from(pos.y),
            f64::from(pos.z) + 0.5,
        );
        let mut ghast = new_mob(self.new_entity_base(Kind::HappyGhast));
        if let EntityState::Living(living) = &mut ghast.state
            && let Some(age) = living.mob.as_mut().and_then(|mob| mob.ageable.as_mut())
        {
            ageable::set_baby(&mut ghast.base, age, true);
        }
        ghast.base.snap_to(at, yaw, 0.0);
        ghast.base.head_yaw = yaw;
        let own = LegacyRandom::new(self.rng.random());
        self.spawn_entity(
            ghast,
            Birth::of(CreatureSpawnReason::Breeding).with_random(own),
        );
        self.play_sound(None, at, SPAWN_SOUND, SoundSource::Blocks, 1.0, 1.0);
    }
}
