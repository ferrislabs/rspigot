use std::sync::OnceLock;

use rand::Rng;
use rspigot_ai::{GoalSettings, MobAi};
use rspigot_data::spawns::DimensionSpawns;
use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::difficulty::{DifficultyInstance, moon_brightness};
use rspigot_domain::entity::spawn::{SpawnEnvironment, finalize_spawn};
use rspigot_domain::entity::species::{Species, ocelot};
use rspigot_domain::entity::{
    Aabb, BlockView, Entity, EntityBase, EntityState, RemovalReason, save,
};
use rspigot_domain::player::GameMode;
use rspigot_domain::random::{LegacyRandom, RandomSource};
use rspigot_domain::spawn::despawn::{DespawnFacts, check_despawn};
use rspigot_domain::spawn::generation::{GenerationHost, spawn_mobs_for_chunk_generation};
use rspigot_domain::spawn::natural::{NaturalSpawnHost, shuffle, spawn_category_for_chunk};
use rspigot_domain::spawn::rules::rules_ported;
use rspigot_domain::spawn::state::{
    SPAWN_DISTANCE_CHUNK, SpawnPlayer, SpawnState, any_player_close_for_spawning,
    effective_mob_spawn_range, spawn_friendlies,
};
use rspigot_domain::spawn::{
    SpawnGroupData, SpawnLevel, SpawnReason, category_spawners, column_top, noise_biome,
    within_border, zoomed_biome,
};
use rspigot_domain::world::{Difficulty, Dimension, WorldHeight};
use rspigot_domain::{BiomeId, BlockPos, BlockStateId, ChunkPos, EntityId, Vec3};
use rspigot_entities::{EntityKind, MobCategory, SpawnHeightmap};
use rspigot_nbt::Compound;
use rustc_hash::FxHashSet;

use crate::chunks::{ChunkManager, MobGeneration};
use crate::config::hashed_seed;
use crate::environment::{is_halloween, today};
use crate::lighting::light_level;
use crate::mob_actions::new_mob;
use crate::mobs::{ServerRandom, sky_darken};
use crate::plugins::events::{CreatureSpawnReason, EntityRemoveCause};
use crate::plugins::{Birth, Continuation};
use crate::ports::ClientLink;
use crate::server::Server;

pub(crate) const LEASH: &str = "leash";
const PERSISTENCE_REQUIRED: &str = "PersistenceRequired";
const OVERWORLD: &str = "minecraft:overworld";
const PLAYER_WIDTH: f32 = 0.6;

#[derive(Debug, Clone)]
pub(crate) struct Spawning {
    zoom_seed: i64,
    dimension: &'static DimensionSpawns,
    sky_darken: i32,
    players: Vec<SpawnPlayer>,
    pub(crate) patrol_next_tick: i32,
    pub(crate) phantom: crate::phantom_spawner::PhantomSpawner,
    pub(crate) trader: crate::trader_spawner::TraderSpawner,
}

impl Spawning {
    pub(crate) fn has_ceiling(&self) -> bool {
        self.dimension.has_ceiling
    }

    pub(crate) fn new(seed: i64, dimension: &Dimension) -> Self {
        Self {
            zoom_seed: hashed_seed(seed),
            dimension: dimension_spawns(dimension),
            sky_darken: 0,
            players: Vec::new(),
            patrol_next_tick: 0,
            phantom: crate::phantom_spawner::PhantomSpawner::default(),
            trader: crate::trader_spawner::TraderSpawner::default(),
        }
    }
}

pub(crate) fn dimension_spawns(dimension: &Dimension) -> &'static DimensionSpawns {
    rspigot_data::spawns::dimension(dimension.dimension_type.as_str())
        .or_else(|| rspigot_data::spawns::dimension(OVERWORLD))
        .unwrap_or(&OVERWORLD_FALLBACK)
}

static OVERWORLD_FALLBACK: DimensionSpawns = DimensionSpawns {
    monster_spawn_light: rspigot_data::spawns::SpawnCount::Uniform { min: 0, max: 7 },
    monster_spawn_block_light_limit: 0,
    ambient_light: 0.0,
    has_ceiling: false,
};

pub(crate) fn naturally_spawns(kind: EntityKind) -> bool {
    static SPAWNS: OnceLock<Vec<bool>> = OnceLock::new();
    SPAWNS.get_or_init(|| {
        EntityKind::ALL
            .iter()
            .map(|kind| {
                kind.extends("Mob")
                    && save::is_simulated(*kind)
                    && rules_ported(*kind)
                    && has_ai(*kind)
            })
            .collect()
    })[usize::from(kind.id().0)]
}

fn has_ai(kind: EntityKind) -> bool {
    MobAi::new(kind, 0, GoalSettings::default()).has_ai()
}

fn blocks_building(kind: EntityKind) -> bool {
    kind.living()
        || matches!(
            kind,
            EntityKind::FallingBlock | EntityKind::Tnt | EntityKind::EndCrystal
        )
        || kind.extends("AbstractBoat")
        || kind.extends("AbstractMinecart")
}

fn collides_with_spawns(kind: EntityKind) -> bool {
    kind == EntityKind::Shulker || kind.extends("AbstractBoat") || kind.extends("AbstractMinecart")
}

fn chunk_noise_biome(chunks: &ChunkManager, origin: ChunkPos, x: i32, y: i32, z: i32) -> BiomeId {
    let at = ChunkPos::new(x >> 2, z >> 2);
    match chunks.get(at) {
        Some(chunk) => noise_biome(chunk, x, y, z),
        None => chunks.get(origin).map_or(BiomeId(0), |chunk| {
            let min_x = origin.min_block_x() >> 2;
            let min_z = origin.min_block_z() >> 2;
            noise_biome(
                chunk,
                x.clamp(min_x, min_x + 3),
                y,
                z.clamp(min_z, min_z + 3),
            )
        }),
    }
}

fn is_bucketed_fish(entity: &Entity) -> bool {
    matches!(
        entity
            .living()
            .and_then(|living| living.mob.as_ref())
            .map(|mob| mob.species),
        Some(Species::Fish(_))
    ) && rspigot_domain::entity::species::fish::from_bucket(&entity.base)
}

fn is_persistent(entity: &Entity) -> bool {
    match &entity.state {
        EntityState::Living(living) => {
            living
                .mob
                .as_ref()
                .is_some_and(|mob| mob.persistence_required)
                || entity.extra.contains_key(LEASH)
                || rspigot_domain::entity::species::enderman::carries_block(&entity.base)
                || is_bucketed_fish(entity)
        }
        EntityState::Inert(nbt) => {
            nbt.get_bool(PERSISTENCE_REQUIRED).unwrap_or(false) || nbt.contains_key(LEASH)
        }
        _ => false,
    }
}

impl<L: ClientLink> BlockView for Server<L> {
    fn block(&self, pos: BlockPos) -> Option<BlockStateId> {
        self.world.chunks.block(pos)
    }
}

impl<L: ClientLink> SpawnLevel for Server<L> {
    fn height(&self) -> WorldHeight {
        self.world.dimension.height
    }

    fn sea_level(&self) -> i32 {
        self.world.dimension.sea_level
    }

    fn sky_light(&self, pos: BlockPos) -> i32 {
        if !self.world.dimension.has_skylight {
            return 0;
        }
        self.world
            .chunks
            .get(pos.chunk())
            .map_or(0, |chunk| light_level(chunk, pos, true))
    }

    fn block_light(&self, pos: BlockPos) -> i32 {
        self.world
            .chunks
            .get(pos.chunk())
            .map_or(0, |chunk| light_level(chunk, pos, false))
    }

    fn sky_darken(&self) -> i32 {
        self.spawning.sky_darken
    }

    fn thundering(&self) -> bool {
        self.world.dimension.has_skylight
            && !self.spawning.dimension.has_ceiling
            && self.world.level.raining
            && self.world.level.thundering
    }

    fn difficulty(&self) -> Difficulty {
        self.world.difficulty
    }

    fn dimension(&self) -> &DimensionSpawns {
        self.spawning.dimension
    }

    fn biome(&self, pos: BlockPos) -> BiomeId {
        let chunks = &self.world.chunks;
        let origin = pos.chunk();
        zoomed_biome(self.spawning.zoom_seed, pos.x, pos.y, pos.z, |x, y, z| {
            chunk_noise_biome(chunks, origin, x, y, z)
        })
    }

    fn top_height(&self, heightmap: SpawnHeightmap, x: i32, z: i32) -> i32 {
        let pos = BlockPos::new(x, 0, z);
        self.world
            .chunks
            .get(pos.chunk())
            .map_or(self.world.dimension.height.min_y, |chunk| {
                column_top(chunk, heightmap, x, z)
            })
    }

    fn is_unobstructed(&self, area: &Aabb) -> bool {
        let blocked_by_player = self.players.values().any(|player| {
            !player.vitals.removed
                && player.game_mode != GameMode::Spectator
                && Aabb::of_size(player.position, PLAYER_WIDTH, player.height() as f32)
                    .intersects(area)
        });
        !blocked_by_player && !self.entity_in(area, blocks_building)
    }

    fn no_entity_collision(&self, area: &Aabb) -> bool {
        !self.entity_in(area, collides_with_spawns)
    }

    fn survival_player_within(&self, center: Vec3, distance: f64) -> bool {
        self.players.values().any(|player| {
            !player.vitals.removed
                && player.game_mode != GameMode::Spectator
                && player.game_mode != GameMode::Creative
                && player.position.distance_squared(center) < distance * distance
        })
    }
}

impl<L: ClientLink> RandomSource for Server<L> {
    fn next_float(&mut self) -> f32 {
        ServerRandom(&mut self.rng).next_float()
    }

    fn next_int(&mut self, bound: i32) -> i32 {
        ServerRandom(&mut self.rng).next_int(bound)
    }

    fn next_double(&mut self) -> f64 {
        ServerRandom(&mut self.rng).next_double()
    }

    fn next_boolean(&mut self) -> bool {
        ServerRandom(&mut self.rng).next_boolean()
    }

    fn next_gaussian(&mut self) -> f64 {
        ServerRandom(&mut self.rng).next_gaussian()
    }
}

impl<L: ClientLink> NaturalSpawnHost for Server<L> {
    fn nearest_player_distance_sqr(&self, position: Vec3) -> Option<f64> {
        nearest_distance_sqr(&self.spawning.players, position)
    }

    fn respawn_pos(&self) -> Option<BlockPos> {
        Some(self.world.spawn.block())
    }

    fn can_spawn_entities_in_chunk(&self, chunk: ChunkPos) -> bool {
        self.entity_ticking(chunk) && chunk_within_border(chunk)
    }

    fn naturally_spawns(&self, kind: EntityKind) -> bool {
        naturally_spawns(kind)
    }

    fn spawn_mob(
        &mut self,
        kind: EntityKind,
        position: Vec3,
        yaw: f32,
        reason: SpawnReason,
        group: Option<SpawnGroupData>,
    ) -> (bool, Option<SpawnGroupData>) {
        let mut entity = new_mob(self.new_entity_base(kind));
        entity.base.snap_to(position, yaw, 0.0);
        let environment = self.spawn_environment(position);
        let mut own = LegacyRandom::new(self.rng.random());
        let group = finalize_spawn(
            &mut entity,
            &environment,
            reason,
            group,
            &mut ServerRandom(&mut self.rng),
            &mut own,
        );
        let id = entity.base.id;
        let follows = crate::kinds::fish_facts_of(&entity)
            .0
            .filter(|leader| *leader != id);
        self.spawn_entity(
            entity,
            Birth::of(CreatureSpawnReason::Natural).with_random(own),
        );
        if let Some(leader) = follows {
            self.fish_joined_school(leader);
        }
        (self.spawning_now(id), group)
    }
}

fn nearest_distance_sqr(players: &[SpawnPlayer], position: Vec3) -> Option<f64> {
    players
        .iter()
        .filter(|player| !player.spectator)
        .map(|player| player.position.distance_squared(position))
        .reduce(|best, distance| if distance < best { distance } else { best })
}

fn chunk_within_border(chunk: ChunkPos) -> bool {
    let (x0, z0) = (
        f64::from(chunk.min_block_x()),
        f64::from(chunk.min_block_z()),
    );
    within_border(x0, z0) && within_border(x0 + 15.0, z0 + 15.0)
}

fn spawn_candidates(players: &[SpawnPlayer]) -> Vec<ChunkPos> {
    let mut chunks = FxHashSet::default();
    for player in players {
        let center = player.position.chunk();
        for dx in -SPAWN_DISTANCE_CHUNK..=SPAWN_DISTANCE_CHUNK {
            for dz in -SPAWN_DISTANCE_CHUNK..=SPAWN_DISTANCE_CHUNK {
                chunks.insert(ChunkPos::new(center.x + dx, center.z + dz));
            }
        }
    }
    let mut chunks: Vec<ChunkPos> = chunks.into_iter().collect();
    chunks.sort_unstable();
    chunks
}

impl<L: ClientLink> Server<L> {
    fn entity_in(&self, area: &Aabb, matches: fn(EntityKind) -> bool) -> bool {
        let from = Vec3::new(area.min_x - 2.0, 0.0, area.min_z - 2.0).chunk();
        let to = Vec3::new(area.max_x + 2.0, 0.0, area.max_z + 2.0).chunk();
        (from.x..=to.x).any(|x| {
            (from.z..=to.z).any(|z| {
                self.entities
                    .store
                    .in_chunk(ChunkPos::new(x, z))
                    .iter()
                    .filter_map(|id| self.entities.store.get(*id))
                    .any(|entity| {
                        !entity.base.is_removed()
                            && matches(entity.base.kind)
                            && entity.base.bounding_box().intersects(area)
                    })
            })
        })
    }

    pub(crate) fn ticking_chunks_near_players(&self) -> Vec<ChunkPos> {
        spawn_candidates(&self.spawning.players)
            .into_iter()
            .filter(|chunk| self.chunk_ticking(*chunk))
            .collect()
    }

    fn chunk_ticking(&self, chunk: ChunkPos) -> bool {
        (-1..=1).all(|dx| {
            (-1..=1).all(|dz| {
                self.world
                    .chunks
                    .is_loaded(ChunkPos::new(chunk.x + dx, chunk.z + dz))
            })
        })
    }

    fn spawn_players(&self) -> Vec<SpawnPlayer> {
        let mut players: Vec<(EntityId, SpawnPlayer)> = self
            .players
            .values()
            .filter(|player| !player.vitals.removed)
            .map(|player| {
                (
                    player.entity_id,
                    SpawnPlayer {
                        position: player.position,
                        spectator: player.game_mode == GameMode::Spectator,
                    },
                )
            })
            .collect();
        players.sort_by_key(|(id, _)| *id);
        players.into_iter().map(|(_, player)| player).collect()
    }

    pub(crate) fn update_spawning(&mut self) {
        self.spawning.sky_darken = sky_darken(&self.world.level);
        self.spawning.players = self.spawn_players();
        let level = &self.world.level;
        self.world
            .chunks
            .set_mob_generation(level.spawn_mobs.then_some(MobGeneration {
                seed: level.seed,
                zoom_seed: self.spawning.zoom_seed,
                difficulty: self.world.difficulty,
            }));
    }

    fn count_existing(&mut self, state: &mut SpawnState) {
        let reserved: Vec<(EntityKind, BlockPos)> = self
            .decisions
            .waiting()
            .filter_map(|then| match then {
                Continuation::Spawn(step) if step.birth.reason == CreatureSpawnReason::Natural => {
                    Some((step.entity.base.kind, step.entity.base.position.block()))
                }
                _ => None,
            })
            .collect();
        for (kind, pos) in reserved {
            let id = kind.id().0;
            let charge = if rspigot_data::spawns::has_spawn_cost(id) {
                self.spawns_at(pos).cost(id).map(|cost| cost.charge)
            } else {
                None
            };
            state.add_existing(kind, pos, charge, true);
        }
        for entity in self.entities.store.iter() {
            let kind = entity.base.kind;
            if kind.category() == MobCategory::Misc
                || !self.world.chunks.is_loaded(entity.base.chunk())
                || (kind.extends("Mob") && is_persistent(entity))
            {
                continue;
            }
            let pos = entity.base.position.block();
            let id = kind.id().0;
            let charge = if rspigot_data::spawns::has_spawn_cost(id) {
                self.spawns_at(pos).cost(id).map(|cost| cost.charge)
            } else {
                None
            };
            state.add_existing(kind, pos, charge, kind.extends("Mob"));
        }
    }

    pub(crate) fn tick_spawning(&mut self) {
        self.update_spawning();
        let players = self.spawning.players.clone();
        if players.is_empty() || !self.world.level.spawn_mobs {
            return;
        }
        let candidates = spawn_candidates(&players);
        let mut state = SpawnState::new(candidates.len() as i32, players.clone());
        self.count_existing(&mut state);
        let game_time = self.world.level.game_time;
        let settings = self.config.spawning.categories;
        let spawn_enemies = self.world.level.spawn_monsters;
        let friendlies = spawn_friendlies(&settings, game_time);
        let categories = state.filtered_categories(&settings, spawn_enemies, friendlies, game_time);
        let mut chunks: Vec<ChunkPos> = candidates
            .into_iter()
            .filter(|chunk| {
                self.chunk_ticking(*chunk) && any_player_close_for_spawning(&players, *chunk, None)
            })
            .collect();
        shuffle(&mut chunks, self);
        if categories.is_empty() {
            return;
        }
        let range = effective_mob_spawn_range(
            self.config.spawning.mob_spawn_range,
            i32::from(self.config.view_distance),
        );
        for chunk in chunks {
            if !self.can_spawn_entities_in_chunk(chunk)
                || !any_player_close_for_spawning(&players, chunk, Some(range))
            {
                continue;
            }
            for category in &categories {
                if state.can_spawn_for_category_local(*category, chunk) {
                    spawn_category_for_chunk(self, &mut state, *category, chunk);
                }
            }
        }
    }

    pub(crate) fn check_mob_despawn(&mut self, id: EntityId) -> bool {
        let Some(entity) = self.entities.store.get(id) else {
            return false;
        };
        let Some(living) = entity.living().filter(|living| living.is_mob()) else {
            return false;
        };
        let facts = DespawnFacts {
            kind: entity.base.kind,
            persistence_required: living
                .mob
                .as_ref()
                .is_some_and(|mob| mob.persistence_required),
            requires_custom_persistence: entity.extra.contains_key(LEASH)
                || rspigot_domain::entity::species::enderman::carries_block(&entity.base)
                || is_bucketed_fish(entity),
            from_bucket: is_bucketed_fish(entity),
            named: entity.base.custom_name().is_some(),
            no_action_time: living.no_action_time,
            chicken_jockey: matches!(
                living.mob.as_ref().map(|mob| &mob.species),
                Some(Species::Chicken(chicken)) if chicken.jockey
            ),
            trusting: matches!(
                living.mob.as_ref().map(|mob| &mob.species),
                Some(Species::Ocelot(_))
            ) && ocelot::is_trusting(&entity.base),
            tame: entity.base.kind.extends("TamableAnimal")
                && rspigot_domain::entity::tamable::is_tame(&entity.base),
            tick_count: entity.base.tick_count,
            villager_progress: matches!(
                living.mob.as_ref().map(|mob| &mob.species),
                Some(Species::Zombie(zombie))
                    if zombie.villager.is_some_and(|villager| villager.keeps_when_far(&entity.base))
            ),
            patrolling: living
                .mob
                .as_ref()
                .and_then(|mob| mob.species.raider())
                .is_some_and(|raider| raider.patrolling),
        };
        let distance = nearest_distance_sqr(&self.spawning.players, entity.base.position);
        let difficulty = self.world.difficulty;
        let outcome = match self.entities.mobs.entries.get_mut(&id) {
            Some(entry) => check_despawn(
                &facts,
                difficulty,
                distance,
                &mut entry.ai.brain_mut().random,
            ),
            None => check_despawn(
                &facts,
                difficulty,
                distance,
                &mut ServerRandom(&mut self.rng),
            ),
        };
        if outcome.discard {
            self.remove_entity(
                id,
                RemovalReason::Discarded,
                Some(EntityRemoveCause::Despawn),
            );
            return true;
        }
        if outcome.reset_no_action_time
            && let Some(living) = self.entities.store.get_mut(id).and_then(Entity::living_mut)
        {
            living.no_action_time = 0;
        }
        false
    }
}

struct Generation<'a> {
    chunk: &'a ChunkColumn,
    has_skylight: bool,
    sea_level: i32,
    dimension: &'static DimensionSpawns,
    zoom_seed: i64,
    difficulty: Difficulty,
    random: LegacyRandom,
    spawned: Vec<Compound>,
}

impl BlockView for Generation<'_> {
    fn block(&self, pos: BlockPos) -> Option<BlockStateId> {
        self.chunk.block(pos)
    }
}

impl SpawnLevel for Generation<'_> {
    fn height(&self) -> WorldHeight {
        self.chunk.height()
    }

    fn sea_level(&self) -> i32 {
        self.sea_level
    }

    fn sky_light(&self, pos: BlockPos) -> i32 {
        if self.has_skylight && pos.chunk() == self.chunk.pos() {
            light_level(self.chunk, pos, true)
        } else {
            0
        }
    }

    fn block_light(&self, pos: BlockPos) -> i32 {
        if pos.chunk() == self.chunk.pos() {
            light_level(self.chunk, pos, false)
        } else {
            0
        }
    }

    fn sky_darken(&self) -> i32 {
        0
    }

    fn thundering(&self) -> bool {
        false
    }

    fn difficulty(&self) -> Difficulty {
        self.difficulty
    }

    fn dimension(&self) -> &DimensionSpawns {
        self.dimension
    }

    fn biome(&self, pos: BlockPos) -> BiomeId {
        let chunk = self.chunk;
        let (min_x, min_z) = (
            chunk.pos().min_block_x() >> 2,
            chunk.pos().min_block_z() >> 2,
        );
        zoomed_biome(self.zoom_seed, pos.x, pos.y, pos.z, |x, y, z| {
            noise_biome(
                chunk,
                x.clamp(min_x, min_x + 3),
                y,
                z.clamp(min_z, min_z + 3),
            )
        })
    }

    fn top_height(&self, heightmap: SpawnHeightmap, x: i32, z: i32) -> i32 {
        column_top(self.chunk, heightmap, x, z)
    }

    fn is_unobstructed(&self, _: &Aabb) -> bool {
        true
    }

    fn no_entity_collision(&self, _: &Aabb) -> bool {
        true
    }

    fn survival_player_within(&self, _: Vec3, _: f64) -> bool {
        false
    }
}

impl RandomSource for Generation<'_> {
    fn next_float(&mut self) -> f32 {
        self.random.next_float()
    }

    fn next_int(&mut self, bound: i32) -> i32 {
        self.random.next_int(bound)
    }

    fn next_double(&mut self) -> f64 {
        self.random.next_double()
    }

    fn next_boolean(&mut self) -> bool {
        self.random.next_boolean()
    }

    fn next_gaussian(&mut self) -> f64 {
        self.random.next_gaussian()
    }
}

impl GenerationHost for Generation<'_> {
    fn naturally_spawns(&self, kind: EntityKind) -> bool {
        naturally_spawns(kind)
    }

    fn spawn_mob(
        &mut self,
        kind: EntityKind,
        position: Vec3,
        yaw: f32,
        group: Option<SpawnGroupData>,
    ) -> Option<SpawnGroupData> {
        let uuid = uuid::Builder::from_random_bytes(rand::rng().random()).into_uuid();
        let mut entity = new_mob(EntityBase::new(EntityId(0), uuid, kind));
        entity.base.snap_to(position, yaw, 0.0);
        let environment = SpawnEnvironment {
            biome: self.biome(position.block()),
            difficulty: DifficultyInstance::new(
                self.difficulty,
                0,
                0,
                moon_brightness(0, self.has_skylight),
            ),
            halloween: is_halloween(today()),
            moon_brightness: moon_brightness(0, self.has_skylight),
        };
        let mut own = LegacyRandom::new(rand::rng().random());
        let group = finalize_spawn(
            &mut entity,
            &environment,
            SpawnReason::ChunkGeneration,
            group,
            &mut self.random,
            &mut own,
        );
        if let Some(nbt) = save::save_entity(&entity, None) {
            self.spawned.push(nbt);
        }
        group
    }
}

pub(crate) fn original_mobs(
    chunk: &ChunkColumn,
    dimension: &Dimension,
    mobs: MobGeneration,
) -> Vec<Compound> {
    let pos = chunk.pos();
    let height = chunk.height();
    let mut decoration = LegacyRandom::decoration(mobs.seed, pos.min_block_x(), pos.min_block_z());
    let region_seed = decoration.next_long();
    let mut decoration = LegacyRandom::decoration(mobs.seed, pos.min_block_x(), pos.min_block_z());
    let mut host = Generation {
        chunk,
        has_skylight: dimension.has_skylight,
        sea_level: dimension.sea_level,
        dimension: dimension_spawns(dimension),
        zoom_seed: mobs.zoom_seed,
        difficulty: mobs.difficulty,
        random: LegacyRandom::new(region_seed),
        spawned: Vec::new(),
    };
    let top = BlockPos::new(pos.min_block_x(), height.max_y(), pos.min_block_z());
    let spawns = host.spawns_at(top);
    let probability =
        rspigot_data::spawns::biome(noise_biome(chunk, top.x >> 2, top.y >> 2, top.z >> 2).0)
            .creature_probability;
    spawn_mobs_for_chunk_generation(
        &mut host,
        pos,
        category_spawners(spawns, MobCategory::Creature),
        probability,
        &mut decoration,
    );
    host.spawned
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mobs_with_goals_spawn_naturally() {
        let spawning: Vec<EntityKind> = EntityKind::ALL
            .into_iter()
            .filter(|kind| naturally_spawns(*kind))
            .collect();
        assert_eq!(
            spawning,
            vec![
                EntityKind::Allay,
                EntityKind::Armadillo,
                EntityKind::Axolotl,
                EntityKind::Bat,
                EntityKind::Bee,
                EntityKind::Blaze,
                EntityKind::Bogged,
                EntityKind::Breeze,
                EntityKind::Cat,
                EntityKind::CaveSpider,
                EntityKind::Chicken,
                EntityKind::Cod,
                EntityKind::CopperGolem,
                EntityKind::Cow,
                EntityKind::Creaking,
                EntityKind::Creeper,
                EntityKind::Dolphin,
                EntityKind::Drowned,
                EntityKind::ElderGuardian,
                EntityKind::Enderman,
                EntityKind::Endermite,
                EntityKind::Fox,
                EntityKind::Frog,
                EntityKind::Ghast,
                EntityKind::GlowSquid,
                EntityKind::Goat,
                EntityKind::Guardian,
                EntityKind::Hoglin,
                EntityKind::Husk,
                EntityKind::IronGolem,
                EntityKind::Mooshroom,
                EntityKind::Nautilus,
                EntityKind::Ocelot,
                EntityKind::Panda,
                EntityKind::Parched,
                EntityKind::Parrot,
                EntityKind::Phantom,
                EntityKind::Pig,
                EntityKind::Piglin,
                EntityKind::PiglinBrute,
                EntityKind::Pillager,
                EntityKind::PolarBear,
                EntityKind::Pufferfish,
                EntityKind::Rabbit,
                EntityKind::Salmon,
                EntityKind::Sheep,
                EntityKind::Shulker,
                EntityKind::Silverfish,
                EntityKind::Skeleton,
                EntityKind::Sniffer,
                EntityKind::SnowGolem,
                EntityKind::Spider,
                EntityKind::Squid,
                EntityKind::Stray,
                EntityKind::Tadpole,
                EntityKind::TropicalFish,
                EntityKind::Turtle,
                EntityKind::Vex,
                EntityKind::Villager,
                EntityKind::WanderingTrader,
                EntityKind::Warden,
                EntityKind::Witch,
                EntityKind::Wither,
                EntityKind::WitherSkeleton,
                EntityKind::Wolf,
                EntityKind::Zoglin,
                EntityKind::Zombie,
                EntityKind::ZombieNautilus,
                EntityKind::ZombieVillager,
                EntityKind::ZombifiedPiglin,
            ]
        );
        for kind in EntityKind::ALL {
            if has_ai(kind) {
                assert!(
                    naturally_spawns(kind),
                    "{kind:?} has an AI but no ported spawn rules"
                );
            }
        }
    }

    #[test]
    fn a_carried_block_makes_an_enderman_persistent() {
        use rspigot_domain::entity::species::enderman;
        use rspigot_domain::entity::{Entity, EntityBase, LivingState};
        let mut base = EntityBase::new(EntityId(1), uuid::Uuid::from_u128(1), EntityKind::Enderman);
        let living = LivingState::new(&mut base, 0.0);
        let mut entity = Entity::new(base, EntityState::Living(Box::new(living)));
        assert!(!is_persistent(&entity));
        enderman::set_carried_block(
            &mut entity.base,
            Some(rspigot_blocks::BlockKind::Dirt.default_state()),
        );
        assert!(is_persistent(&entity));
    }

    #[test]
    fn spawn_candidates_cover_seventeen_chunks_square_per_player() {
        let at = |x: f64, z: f64| SpawnPlayer {
            position: Vec3::new(x, 64.0, z),
            spectator: false,
        };
        assert_eq!(spawn_candidates(&[at(0.0, 0.0)]).len(), 289);
        assert_eq!(spawn_candidates(&[at(0.0, 0.0), at(8.0, 8.0)]).len(), 289);
        assert_eq!(
            spawn_candidates(&[at(0.0, 0.0), at(16.0 * 17.0, 0.0)]).len(),
            578
        );
    }
}
