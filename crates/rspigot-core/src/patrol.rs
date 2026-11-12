use rand::Rng;
use rspigot_ai::AiWorld;
use rspigot_domain::entity::spawn::finalize_spawn;
use rspigot_domain::player::GameMode;
use rspigot_domain::random::{LegacyRandom, RandomSource};
use rspigot_domain::spawn::SpawnReason;
use rspigot_domain::spawn::placement::is_valid_empty_spawn_block;
use rspigot_domain::spawn::rules::check_patrolling_monster_spawn_rules;
use rspigot_domain::world::Difficulty;
use rspigot_domain::{BlockPos, Vec3};
use rspigot_entities::EntityKind;

use crate::mob_actions::new_mob;
use crate::mobs::{ServerRandom, sky_darken};
use crate::plugins::Birth;
use crate::plugins::events::CreatureSpawnReason;
use crate::ports::ClientLink;
use crate::server::Server;

const COOLDOWN_BASE: i32 = 12_000;
const COOLDOWN_JITTER: i32 = 1_200;
const SPAWN_ODDS: i32 = 5;
const OFFSET_BASE: i32 = 24;
const OFFSET_SPREAD: i32 = 24;
const VILLAGE_SECTIONS: i32 = 2;
const CHUNK_MARGIN: i32 = 10;
const SCATTER: i32 = 5;
const NO_PATROL_BIOME: &str = "minecraft:mushroom_fields";
const PATROL_CLOCK_START: i64 = 120_000;

pub(crate) fn patrol_size(effective_difficulty: f32) -> i32 {
    f64::from(effective_difficulty).ceil() as i32 + 1
}

fn signed_offset(random: &mut impl RandomSource) -> i32 {
    let magnitude = random.next_int(OFFSET_SPREAD) + OFFSET_BASE;
    if random.next_boolean() {
        -magnitude
    } else {
        magnitude
    }
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn tick_patrol_spawner(&mut self) {
        let spawn_enemies = self.world.level.spawn_monsters
            && self.world.level.spawn_mobs
            && self.world.difficulty != Difficulty::Peaceful;
        if !spawn_enemies || !self.config.rules.spawn_patrols {
            return;
        }
        self.spawning.patrol_next_tick -= 1;
        if self.spawning.patrol_next_tick >= 1 {
            return;
        }
        let mut random = ServerRandom(&mut self.rng);
        let before = self.spawning.patrol_next_tick;
        self.spawning.patrol_next_tick = before + random.next_int(COOLDOWN_JITTER) + COOLDOWN_BASE;
        let bright = self
            .mob_world(sky_darken(&self.world.level))
            .bright_outside();
        let mut random = ServerRandom(&mut self.rng);
        if !bright || random.next_int(SPAWN_ODDS) != 0 {
            return;
        }
        let mut players: Vec<_> = self
            .players
            .values()
            .map(|player| (player.entity_id, player.position, player.game_mode))
            .collect();
        if players.is_empty() {
            return;
        }
        players.sort_by_key(|(id, ..)| id.0);
        let mut random = ServerRandom(&mut self.rng);
        let (_, position, mode) = players[random.next_int(players.len() as i32) as usize];
        if mode == GameMode::Spectator {
            return;
        }
        let here = position.block();
        let world = self.mob_world(sky_darken(&self.world.level));
        if world.is_close_to_village(here, VILLAGE_SECTIONS) {
            return;
        }
        let mut random = ServerRandom(&mut self.rng);
        let dx = signed_offset(&mut random);
        let dz = signed_offset(&mut random);
        let mut pos = here.offset(dx, 0, dz);
        if !self.chunks_around_loaded(pos) || !self.can_pillager_patrol_spawn(pos) {
            return;
        }
        let count = patrol_size(self.difficulty_at(pos).effective);
        for index in 0..count {
            let surface = self
                .mob_world(sky_darken(&self.world.level))
                .motion_blocking_no_leaves_pos(pos);
            pos = BlockPos::new(pos.x, surface.y, pos.z);
            if index == 0 {
                if !self.spawn_patrol_member(pos, true) {
                    return;
                }
            } else {
                self.spawn_patrol_member(pos, false);
            }
            let mut random = ServerRandom(&mut self.rng);
            let x = pos.x + random.next_int(SCATTER) - random.next_int(SCATTER);
            let z = pos.z + random.next_int(SCATTER) - random.next_int(SCATTER);
            pos = BlockPos::new(x, pos.y, z);
        }
    }

    fn chunks_around_loaded(&self, pos: BlockPos) -> bool {
        let low = BlockPos::new(pos.x - CHUNK_MARGIN, 0, pos.z - CHUNK_MARGIN).chunk();
        let high = BlockPos::new(pos.x + CHUNK_MARGIN, 0, pos.z + CHUNK_MARGIN).chunk();
        (low.x..=high.x).all(|x| {
            (low.z..=high.z).all(|z| {
                self.world
                    .chunks
                    .is_loaded(rspigot_domain::ChunkPos::new(x, z))
            })
        })
    }

    fn can_pillager_patrol_spawn(&self, pos: BlockPos) -> bool {
        let mushrooms = rspigot_data::registries::biome_id(NO_PATROL_BIOME);
        self.world.level.day_time >= PATROL_CLOCK_START && Some(self.biome_at(pos).0) != mushrooms
    }

    fn spawn_patrol_member(&mut self, pos: BlockPos, leader: bool) -> bool {
        let state = self.world.chunks.block(pos).unwrap_or_default();
        if !is_valid_empty_spawn_block(state, EntityKind::Pillager) {
            return false;
        }
        let rules = check_patrolling_monster_spawn_rules(
            &*self,
            EntityKind::Pillager,
            SpawnReason::Patrol,
            pos,
        );
        if !rules {
            return false;
        }
        let mut entity = new_mob(self.new_entity_base(EntityKind::Pillager));
        let mut own = LegacyRandom::new(self.rng.random());
        if leader
            && let Some(raider) = entity
                .living_mut()
                .and_then(|living| living.mob.as_mut())
                .and_then(|mob| mob.species.raider_mut())
        {
            raider.patrol_leader = true;
            raider.patrolling = true;
            raider.find_patrol_target(BlockPos::new(0, 0, 0), &mut own);
        }
        let position = Vec3::new(f64::from(pos.x), f64::from(pos.y), f64::from(pos.z));
        entity.base.snap_to(position, 0.0, 0.0);
        let environment = self.spawn_environment(position);
        finalize_spawn(
            &mut entity,
            &environment,
            SpawnReason::Patrol,
            None,
            &mut ServerRandom(&mut self.rng),
            &mut own,
        );
        self.spawn_entity(
            entity,
            Birth::of(CreatureSpawnReason::Patrol).with_random(own),
        );
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_patrol_has_one_more_member_than_the_rounded_up_difficulty() {
        assert_eq!(patrol_size(0.0), 1);
        assert_eq!(patrol_size(0.1), 2);
        assert_eq!(patrol_size(2.0), 3);
        assert_eq!(patrol_size(2.4), 4);
        assert_eq!(patrol_size(6.75), 8);
    }

    #[test]
    fn the_offsets_are_between_twenty_four_and_forty_seven_blocks_either_way() {
        let mut random = LegacyRandom::new(3);
        let offsets: Vec<i32> = (0..200).map(|_| signed_offset(&mut random)).collect();
        assert!(
            offsets
                .iter()
                .all(|offset| (24..48).contains(&offset.abs()))
        );
        assert!(offsets.iter().any(|offset| *offset < 0));
        assert!(offsets.iter().any(|offset| *offset > 0));
    }

    #[test]
    fn the_offset_draws_the_magnitude_before_the_sign() {
        let mut random = LegacyRandom::new(9);
        let mut mirror = LegacyRandom::new(9);
        let magnitude = mirror.next_int(24) + 24;
        let negative = mirror.next_boolean();
        assert_eq!(
            signed_offset(&mut random),
            if negative { -magnitude } else { magnitude }
        );
    }
}
