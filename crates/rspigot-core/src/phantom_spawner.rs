use rand::Rng;
use rspigot_domain::entity::spawn::finalize_spawn;
use rspigot_domain::player::GameMode;
use rspigot_domain::random::{LegacyRandom, RandomSource};
use rspigot_domain::spawn::SpawnReason;
use rspigot_domain::spawn::placement::is_valid_empty_spawn_block;
use rspigot_domain::spawn::rules::can_see_sky;
use rspigot_domain::world::Difficulty;
use rspigot_domain::{BlockPos, Vec3};
use rspigot_entities::EntityKind;

use crate::mob_actions::new_mob;
use crate::mobs::{ServerRandom, sky_darken};
use crate::plugins::Birth;
use crate::plugins::events::CreatureSpawnReason;
use crate::ports::ClientLink;
use crate::server::Server;

const INTERVAL_BASE_SECONDS: i32 = 60;
const INTERVAL_RANDOM_SECONDS: i32 = 60;
const TICKS_PER_SECOND: i32 = 20;
const MAX_SKY_DARKEN: i32 = 5;
const DIFFICULTY_ROLL: f32 = 3.0;
const INSOMNIA_THRESHOLD: i32 = 72_000;
const ABOVE_BASE: i32 = 20;
const ABOVE_SPREAD: i32 = 15;
const SIDE_SPREAD: i32 = 21;
const SIDE_OFFSET: i32 = 10;

#[derive(Debug, Clone, Default)]
pub(crate) struct PhantomSpawner {
    next_tick: i32,
    rest: rustc_hash::FxHashMap<rspigot_domain::player::PlayerId, i32>,
}

impl PhantomSpawner {
    pub(crate) fn rest_ticks(&self, player: rspigot_domain::player::PlayerId) -> i32 {
        self.rest.get(&player).copied().unwrap_or(0)
    }
}

impl<L: ClientLink> Server<L> {
    pub fn set_ticks_since_rest(&mut self, player: rspigot_domain::player::PlayerId, ticks: i32) {
        self.spawning.phantom.rest.insert(player, ticks);
    }

    pub fn ticks_since_rest(&self, player: rspigot_domain::player::PlayerId) -> i32 {
        self.spawning.phantom.rest_ticks(player)
    }

    pub(crate) fn tick_phantom_spawner(&mut self) {
        let alive: Vec<_> = self.players.keys().copied().collect();
        let spawner = &mut self.spawning.phantom;
        spawner.rest.retain(|player, _| alive.contains(player));
        for player in &alive {
            let ticks = spawner.rest.entry(*player).or_insert(0);
            *ticks = ticks.saturating_add(1);
        }
        let spawn_enemies = self.world.level.spawn_monsters
            && self.world.level.spawn_mobs
            && self.world.difficulty != Difficulty::Peaceful;
        if !spawn_enemies || !self.config.rules.spawn_phantoms {
            return;
        }
        self.spawning.phantom.next_tick -= 1;
        if self.spawning.phantom.next_tick >= 1 {
            return;
        }
        let mut random = ServerRandom(&mut self.rng);
        let before = self.spawning.phantom.next_tick;
        self.spawning.phantom.next_tick = before
            + (random.next_int(INTERVAL_RANDOM_SECONDS) + INTERVAL_BASE_SECONDS) * TICKS_PER_SECOND;
        let skylight = self.world.dimension.has_skylight;
        if sky_darken(&self.world.level) < MAX_SKY_DARKEN && skylight {
            return;
        }
        let mut players: Vec<_> = self
            .players
            .iter()
            .map(|(id, player)| (*id, player.entity_id, player.position, player.game_mode))
            .collect();
        players.sort_by_key(|(_, entity, ..)| entity.0);
        for (id, _, position, mode) in players {
            if mode == GameMode::Spectator {
                continue;
            }
            let pos = position.block();
            if skylight && (pos.y < self.world.dimension.sea_level || !can_see_sky(&*self, pos)) {
                continue;
            }
            let difficulty = self.difficulty_at(pos);
            let mut random = ServerRandom(&mut self.rng);
            if !difficulty.is_harder_than(random.next_float() * DIFFICULTY_ROLL) {
                continue;
            }
            let rest = self.spawning.phantom.rest_ticks(id).max(1);
            let mut random = ServerRandom(&mut self.rng);
            if random.next_int(rest) < INSOMNIA_THRESHOLD {
                continue;
            }
            let up = ABOVE_BASE + random.next_int(ABOVE_SPREAD);
            let east = random.next_int(SIDE_SPREAD) - SIDE_OFFSET;
            let south = random.next_int(SIDE_SPREAD) - SIDE_OFFSET;
            let spawn = pos.offset(east, up, south);
            let state = self.world.chunks.block(spawn).unwrap_or_default();
            if !is_valid_empty_spawn_block(state, EntityKind::Phantom) {
                continue;
            }
            let count = 1 + random.next_int(i32::from(self.world.difficulty.id()) + 1);
            let mut group = None;
            for _ in 0..count {
                group = self.spawn_phantom(spawn, group);
            }
        }
    }

    fn spawn_phantom(
        &mut self,
        pos: BlockPos,
        group: Option<rspigot_domain::spawn::SpawnGroupData>,
    ) -> Option<rspigot_domain::spawn::SpawnGroupData> {
        let mut entity = new_mob(self.new_entity_base(EntityKind::Phantom));
        let position = Vec3::new(
            f64::from(pos.x) + 0.5,
            f64::from(pos.y),
            f64::from(pos.z) + 0.5,
        );
        entity.base.snap_to(position, 0.0, 0.0);
        let mut own = LegacyRandom::new(self.rng.random());
        let environment = self.spawn_environment(position);
        let group = finalize_spawn(
            &mut entity,
            &environment,
            SpawnReason::Natural,
            group,
            &mut ServerRandom(&mut self.rng),
            &mut own,
        );
        self.spawn_entity(
            entity,
            Birth::of(CreatureSpawnReason::Natural).with_random(own),
        );
        group
    }
}
