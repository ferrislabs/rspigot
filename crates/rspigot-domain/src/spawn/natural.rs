use rspigot_data::spawns::Spawner;
use rspigot_entities::{EntityKind, MobCategory, SpawnHeightmap};

use super::despawn::{FarAwayFacts, remove_when_far_away};
use super::placement::{is_redstone_conductor, is_spawn_position_ok};
use super::rules::{
    check_spawn_obstruction, check_spawn_rules, is_max_group_size_reached, max_spawn_cluster_size,
    mob_check_spawn_rules, no_collision,
};
use super::state::SpawnState;
use super::{
    SpawnGroupData, SpawnLevel, SpawnReason, category_spawners, pick_spawner, sample_count,
    spawn_aabb, spawner_kind,
};
use crate::entity::Aabb;
use crate::random::{RandomSource, random_between_inclusive};
use crate::{BlockPos, ChunkPos, Vec3};

pub const MIN_SPAWN_DISTANCE_SQR: f64 = 576.0;
pub const SPAWN_POINT_DISTANCE: f64 = 24.0;
const GROUPS: i32 = 3;
const GROUP_SPREAD: i32 = 6;
const REDUCED_WATER_AMBIENT_SPAWNS: &str = "minecraft:reduce_water_ambient_spawns";
const REDUCED_WATER_AMBIENT_CHANCE: f32 = 0.98;
const BIOME_TAGS: &str = "minecraft:worldgen/biome";

pub trait NaturalSpawnHost: SpawnLevel + RandomSource {
    fn nearest_player_distance_sqr(&self, position: Vec3) -> Option<f64>;
    fn respawn_pos(&self) -> Option<BlockPos>;
    fn can_spawn_entities_in_chunk(&self, chunk: ChunkPos) -> bool;
    fn naturally_spawns(&self, kind: EntityKind) -> bool;
    fn spawn_mob(
        &mut self,
        kind: EntityKind,
        position: Vec3,
        yaw: f32,
        reason: SpawnReason,
        group: Option<SpawnGroupData>,
    ) -> (bool, Option<SpawnGroupData>);
}

pub fn shuffle<T>(items: &mut [T], random: &mut impl RandomSource) {
    let mut index = items.len();
    while index > 1 {
        let swap = random.next_int(index as i32) as usize;
        items.swap(index - 1, swap);
        index -= 1;
    }
}

pub fn ceil(value: f32) -> i32 {
    let truncated = value as i32;
    if value > truncated as f32 {
        truncated + 1
    } else {
        truncated
    }
}

pub fn random_pos_within(host: &mut impl NaturalSpawnHost, chunk: ChunkPos) -> BlockPos {
    let x = chunk.min_block_x() + host.next_int(16);
    let z = chunk.min_block_z() + host.next_int(16);
    let top = host.top_height(SpawnHeightmap::WorldSurface, x, z);
    let min_y = host.height().min_y;
    let y = random_between_inclusive(host, min_y, top);
    BlockPos::new(x, y, z)
}

pub fn spawn_category_for_chunk(
    host: &mut impl NaturalSpawnHost,
    state: &mut SpawnState,
    category: MobCategory,
    chunk: ChunkPos,
) {
    let pos = random_pos_within(host, chunk);
    if pos.y > host.height().min_y {
        spawn_category_for_position(host, state, category, chunk, pos);
    }
}

fn closer_to_center_than(pos: BlockPos, target: Vec3, distance: f64) -> bool {
    let dx = f64::from(pos.x) + 0.5 - target.x;
    let dy = f64::from(pos.y) + 0.5 - target.y;
    let dz = f64::from(pos.z) + 0.5 - target.z;
    dx * dx + dy * dy + dz * dz < distance * distance
}

fn is_right_distance_to_player_and_spawn_point(
    host: &impl NaturalSpawnHost,
    chunk: ChunkPos,
    pos: BlockPos,
    distance: f64,
) -> bool {
    if distance <= MIN_SPAWN_DISTANCE_SQR {
        return false;
    }
    let center = Vec3::new(
        f64::from(pos.x) + 0.5,
        f64::from(pos.y),
        f64::from(pos.z) + 0.5,
    );
    if host
        .respawn_pos()
        .is_some_and(|respawn| closer_to_center_than(respawn, center, SPAWN_POINT_DISTANCE))
    {
        return false;
    }
    let at = pos.chunk();
    at == chunk || host.can_spawn_entities_in_chunk(at)
}

fn mobs_at(
    host: &impl NaturalSpawnHost,
    category: MobCategory,
    pos: BlockPos,
) -> &'static [Spawner] {
    category_spawners(host.spawns_at(pos), category)
}

fn random_spawn_mob_at(
    host: &mut impl NaturalSpawnHost,
    category: MobCategory,
    pos: BlockPos,
) -> Option<&'static Spawner> {
    if category == MobCategory::WaterAmbient {
        let biome = u32::from(host.biome(pos).0);
        let reduced = rspigot_data::tags::tag(BIOME_TAGS, REDUCED_WATER_AMBIENT_SPAWNS)
            .is_some_and(|ids| ids.binary_search(&biome).is_ok());
        if reduced && host.next_float() < REDUCED_WATER_AMBIENT_CHANCE {
            return None;
        }
    }
    let spawners = mobs_at(host, category, pos);
    pick_spawner(spawners, host)
}

fn can_spawn_mob_at(
    host: &impl NaturalSpawnHost,
    category: MobCategory,
    spawner: &Spawner,
    pos: BlockPos,
) -> bool {
    mobs_at(host, category, pos)
        .iter()
        .any(|candidate| candidate.entity == spawner.entity && candidate.count == spawner.count)
}

fn is_valid_spawn_position_for_type(
    host: &mut impl NaturalSpawnHost,
    category: MobCategory,
    spawner: &Spawner,
    pos: BlockPos,
    distance: f64,
) -> bool {
    let Some(kind) = spawner_kind(spawner) else {
        return false;
    };
    let kind_category = kind.category();
    if kind_category == MobCategory::Misc || !host.naturally_spawns(kind) {
        return false;
    }
    let despawn = f64::from(i32::from(kind_category.despawn_distance()).pow(2));
    if !kind.can_spawn_far_from_player() && distance > despawn {
        return false;
    }
    if !(kind.summonable() && can_spawn_mob_at(host, category, spawner, pos)) {
        return false;
    }
    if !is_spawn_position_ok(host, kind, pos) {
        return false;
    }
    if !check_spawn_rules(host, kind, SpawnReason::Natural, pos) {
        return false;
    }
    let area = spawn_aabb(
        kind,
        f64::from(pos.x) + 0.5,
        f64::from(pos.y),
        f64::from(pos.z) + 0.5,
    );
    no_collision(host, &area)
}

pub fn is_valid_position_for_mob(
    level: &impl SpawnLevel,
    kind: EntityKind,
    position: Vec3,
    distance: f64,
) -> bool {
    let despawn = f64::from(i32::from(kind.category().despawn_distance()).pow(2));
    if distance > despawn && remove_when_far_away(kind, FarAwayFacts::default()).unwrap_or(false) {
        return false;
    }
    let dimensions = kind.dimensions();
    let bounds = Aabb::of_size(position, dimensions.width, dimensions.height);
    mob_check_spawn_rules(level, kind, position) && check_spawn_obstruction(level, kind, &bounds)
}

pub fn spawn_category_for_position(
    host: &mut impl NaturalSpawnHost,
    state: &mut SpawnState,
    category: MobCategory,
    chunk: ChunkPos,
    start: BlockPos,
) {
    let y = start.y;
    if is_redstone_conductor(host.block_or_air(start)) {
        return;
    }
    let mut spawned = 0;
    for _ in 0..GROUPS {
        let mut x = start.x;
        let mut z = start.z;
        let mut spawner: Option<&'static Spawner> = None;
        let mut group: Option<SpawnGroupData> = None;
        let mut max = ceil(host.next_float() * 4.0);
        let mut in_group = 0;
        let mut attempt = 0;
        while attempt < max {
            x += host.next_int(GROUP_SPREAD) - host.next_int(GROUP_SPREAD);
            z += host.next_int(GROUP_SPREAD) - host.next_int(GROUP_SPREAD);
            attempt += 1;
            let pos = BlockPos::new(x, y, z);
            let position = Vec3::new(f64::from(x) + 0.5, f64::from(y), f64::from(z) + 0.5);
            let Some(distance) = host.nearest_player_distance_sqr(position) else {
                continue;
            };
            if !is_right_distance_to_player_and_spawn_point(host, chunk, pos, distance) {
                continue;
            }
            let picked = match spawner {
                Some(picked) => picked,
                None => {
                    let Some(picked) = random_spawn_mob_at(host, category, pos) else {
                        break;
                    };
                    spawner = Some(picked);
                    max = sample_count(picked.count, host);
                    picked
                }
            };
            if !is_valid_spawn_position_for_type(host, category, picked, pos, distance) {
                continue;
            }
            let Some(kind) = spawner_kind(picked) else {
                continue;
            };
            let cost = host
                .spawns_at(pos)
                .cost(picked.entity)
                .map(|cost| (cost.energy_budget, cost.charge));
            if !state.can_spawn(kind, pos, cost) {
                continue;
            }
            let yaw = host.next_float() * 360.0;
            if !is_valid_position_for_mob(host, kind, position, distance) {
                continue;
            }
            let (added, next) = host.spawn_mob(kind, position, yaw, SpawnReason::Natural, group);
            group = next;
            if added {
                spawned += 1;
                in_group += 1;
                state.after_spawn(kind, pos, || {
                    host.spawns_at(pos)
                        .cost(picked.entity)
                        .map_or(0.0, |cost| cost.charge)
                });
            }
            if spawned >= max_spawn_cluster_size(kind) {
                return;
            }
            if is_max_group_size_reached(kind, in_group) {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use rspigot_blocks::{Block, GrassBlock, Stone};

    use super::*;
    use crate::spawn::state::SpawnPlayer;
    use crate::spawn::testing::Level;

    fn state(level: &Level) -> SpawnState {
        let players = level
            .players
            .iter()
            .map(|position| SpawnPlayer {
                position: *position,
                spectator: false,
            })
            .collect();
        SpawnState::new(289, players)
    }

    fn run(level: &mut Level, category: MobCategory, start: BlockPos, rounds: i64) {
        let mut spawn_state = state(level);
        for seed in 0..rounds {
            *level = std::mem::replace(level, Level::floor(Stone::DEFAULT_STATE, 0)).seeded(seed);
            spawn_category_for_position(level, &mut spawn_state, category, start.chunk(), start);
        }
    }

    #[test]
    fn shuffle_swaps_from_the_end_like_util_shuffle() {
        let mut level = Level::floor(Stone::DEFAULT_STATE, 0);
        level.rolls(&[0, 0, 0]);
        let mut items = [1, 2, 3, 4];
        shuffle(&mut items, &mut level);
        assert_eq!(items, [2, 3, 4, 1]);
        assert_eq!(level.used(), 3);
        let mut single = [7];
        shuffle(&mut single, &mut level);
        assert_eq!(level.used(), 3);
    }

    #[test]
    fn ceil_matches_mth_ceil() {
        assert_eq!(ceil(0.0), 0);
        assert_eq!(ceil(0.1), 1);
        assert_eq!(ceil(3.0), 3);
        assert_eq!(ceil(3.9), 4);
        assert_eq!(ceil(-0.5), 0);
    }

    #[test]
    fn lit_grass_spawns_simulated_farm_animals_away_from_players() {
        let mut level = Level::floor(GrassBlock::DEFAULT_STATE, 63);
        level.players.push(Vec3::new(8.0, 64.0, -40.0));
        let start = BlockPos::new(8, 64, 8);
        run(&mut level, MobCategory::Creature, start, 30);
        assert!(!level.spawned.is_empty());
        for (kind, position, _) in &level.spawned {
            assert!(matches!(
                kind,
                EntityKind::Cow | EntityKind::Pig | EntityKind::Sheep | EntityKind::Chicken
            ));
            assert_eq!(position.y, 64.0);
            assert!(
                position.distance_squared(Vec3::new(8.0, 64.0, -40.0)) > MIN_SPAWN_DISTANCE_SQR
            );
            assert_eq!(position.x - position.x.floor(), 0.5);
        }
    }

    #[test]
    fn darkness_spawns_simulated_monsters() {
        let mut level = Level::floor(Stone::DEFAULT_STATE, 63);
        level.sky = 0;
        level.players.push(Vec3::new(8.0, 64.0, 40.0));
        let start = BlockPos::new(8, 64, 8);
        run(&mut level, MobCategory::Monster, start, 30);
        assert!(!level.spawned.is_empty());
        assert!(level.spawned.iter().all(|(kind, _, _)| matches!(
            kind,
            EntityKind::Zombie | EntityKind::Skeleton | EntityKind::Creeper | EntityKind::Spider
        )));
        let mut bright = Level::floor(Stone::DEFAULT_STATE, 63);
        bright.players.push(Vec3::new(8.0, 64.0, 40.0));
        run(&mut bright, MobCategory::Monster, start, 30);
        assert!(bright.spawned.is_empty());
    }

    #[test]
    fn players_spawn_points_and_conductors_block_spawning() {
        let start = BlockPos::new(8, 64, 8);
        let mut close = Level::floor(GrassBlock::DEFAULT_STATE, 63);
        close.players.push(Vec3::new(8.0, 64.0, 8.0));
        run(&mut close, MobCategory::Creature, start, 30);
        assert!(close.spawned.is_empty());
        let mut alone = Level::floor(GrassBlock::DEFAULT_STATE, 63);
        run(&mut alone, MobCategory::Creature, start, 30);
        assert!(alone.spawned.is_empty());
        let mut respawn = Level::floor(GrassBlock::DEFAULT_STATE, 63);
        respawn.players.push(Vec3::new(8.0, 64.0, -40.0));
        respawn.respawn = Some(start);
        let mut spawn_state = state(&respawn);
        spawn_category_for_position(
            &mut respawn,
            &mut spawn_state,
            MobCategory::Creature,
            start.chunk(),
            start,
        );
        assert!(respawn.spawned.iter().all(|(_, position, _)| {
            !closer_to_center_than(start, *position, SPAWN_POINT_DISTANCE)
        }));
        let mut conductor = Level::floor(GrassBlock::DEFAULT_STATE, 63);
        conductor.players.push(Vec3::new(8.0, 64.0, -40.0));
        conductor.set(start, Stone::DEFAULT_STATE);
        let used = conductor.used();
        let mut spawn_state = state(&conductor);
        spawn_category_for_position(
            &mut conductor,
            &mut spawn_state,
            MobCategory::Creature,
            start.chunk(),
            start,
        );
        assert!(conductor.spawned.is_empty());
        assert_eq!(conductor.used(), used);
    }

    #[test]
    fn group_size_is_capped_by_the_cluster_size() {
        let mut level = Level::floor(GrassBlock::DEFAULT_STATE, 63);
        level.players.push(Vec3::new(8.0, 64.0, -40.0));
        let start = BlockPos::new(8, 64, 8);
        for seed in 0..30 {
            let mut fresh = Level::floor(GrassBlock::DEFAULT_STATE, 63).seeded(seed);
            fresh.players.push(Vec3::new(8.0, 64.0, -40.0));
            let mut spawn_state = state(&fresh);
            spawn_category_for_position(
                &mut fresh,
                &mut spawn_state,
                MobCategory::Creature,
                start.chunk(),
                start,
            );
            assert!(fresh.spawned.len() <= 4);
            assert_eq!(
                spawn_state.count(MobCategory::Creature),
                fresh.spawned.len() as i32
            );
        }
        let mut spawn_state = state(&level);
        let pos = random_pos_within(&mut level, ChunkPos::new(2, 3));
        assert_eq!(pos.chunk(), ChunkPos::new(2, 3));
        assert!(pos.y <= 64 && pos.y >= -64);
        spawn_category_for_chunk(
            &mut level,
            &mut spawn_state,
            MobCategory::Creature,
            ChunkPos::new(2, 3),
        );
    }
}
