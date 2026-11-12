use std::collections::VecDeque;

use rspigot_data::spawns::DimensionSpawns;
use rspigot_entities::{EntityKind, SpawnHeightmap};
use rustc_hash::FxHashMap;

use super::natural::NaturalSpawnHost;
use super::{SpawnGroupData, SpawnLevel, SpawnReason};
use crate::entity::{Aabb, BlockView};
use crate::random::{LegacyRandom, RandomSource};
use crate::world::{Difficulty, WorldHeight};
use crate::{BiomeId, BlockPos, BlockStateId, ChunkPos, Vec3};

pub struct Level {
    pub floor: Option<(BlockStateId, i32)>,
    pub blocks: FxHashMap<BlockPos, BlockStateId>,
    pub sky: i32,
    pub block: i32,
    pub sky_darken: i32,
    pub thundering: bool,
    pub difficulty: Difficulty,
    pub dimension: DimensionSpawns,
    pub biome: BiomeId,
    pub obstacles: Vec<Aabb>,
    pub players: Vec<Vec3>,
    pub respawn: Option<BlockPos>,
    pub spawned: Vec<(EntityKind, Vec3, f32)>,
    rolls: VecDeque<f64>,
    fallback: LegacyRandom,
    used: usize,
}

impl Level {
    pub fn floor(state: BlockStateId, y: i32) -> Self {
        Self {
            floor: Some((state, y)),
            blocks: FxHashMap::default(),
            sky: 15,
            block: 0,
            sky_darken: 0,
            thundering: false,
            difficulty: Difficulty::Normal,
            dimension: *rspigot_data::spawns::dimension("minecraft:overworld")
                .expect("overworld spawn settings"),
            biome: BiomeId(rspigot_data::registries::biome_id("minecraft:plains").unwrap_or(0)),
            obstacles: Vec::new(),
            players: Vec::new(),
            respawn: None,
            spawned: Vec::new(),
            rolls: VecDeque::new(),
            fallback: LegacyRandom::new(0),
            used: 0,
        }
    }

    pub fn seeded(mut self, seed: i64) -> Self {
        self.fallback = LegacyRandom::new(seed);
        self
    }

    pub fn set(&mut self, pos: BlockPos, state: BlockStateId) {
        self.blocks.insert(pos, state);
    }

    pub fn rolls(&mut self, values: &[i32]) {
        self.rolls = values.iter().map(|value| f64::from(*value)).collect();
        self.used = 0;
    }

    pub fn used(&self) -> usize {
        self.used
    }

    fn scripted(&mut self) -> Option<f64> {
        self.used += 1;
        self.rolls.pop_front()
    }
}

impl BlockView for Level {
    fn block(&self, pos: BlockPos) -> Option<BlockStateId> {
        if let Some(state) = self.blocks.get(&pos) {
            return Some(*state);
        }
        Some(match self.floor {
            Some((state, y)) if y == pos.y => state,
            _ => BlockStateId::AIR,
        })
    }
}

impl SpawnLevel for Level {
    fn height(&self) -> WorldHeight {
        WorldHeight::OVERWORLD
    }

    fn sea_level(&self) -> i32 {
        63
    }

    fn sky_light(&self, _: BlockPos) -> i32 {
        self.sky
    }

    fn block_light(&self, _: BlockPos) -> i32 {
        self.block
    }

    fn sky_darken(&self) -> i32 {
        self.sky_darken
    }

    fn thundering(&self) -> bool {
        self.thundering
    }

    fn difficulty(&self) -> Difficulty {
        self.difficulty
    }

    fn dimension(&self) -> &DimensionSpawns {
        &self.dimension
    }

    fn biome(&self, _: BlockPos) -> BiomeId {
        self.biome
    }

    fn top_height(&self, _: SpawnHeightmap, x: i32, z: i32) -> i32 {
        let floor = self
            .floor
            .map_or(WorldHeight::OVERWORLD.min_y, |(_, y)| y + 1);
        self.blocks
            .iter()
            .filter(|(pos, state)| pos.x == x && pos.z == z && !state.is_air())
            .map(|(pos, _)| pos.y + 1)
            .fold(floor, i32::max)
    }

    fn is_unobstructed(&self, area: &Aabb) -> bool {
        !self
            .obstacles
            .iter()
            .any(|obstacle| obstacle.intersects(area))
    }

    fn no_entity_collision(&self, _: &Aabb) -> bool {
        true
    }

    fn survival_player_within(&self, center: Vec3, distance: f64) -> bool {
        self.players
            .iter()
            .any(|player| player.distance_squared(center) < distance * distance)
    }
}

impl RandomSource for Level {
    fn next_float(&mut self) -> f32 {
        match self.scripted() {
            Some(value) => value as f32,
            None => self.fallback.next_float(),
        }
    }

    fn next_int(&mut self, bound: i32) -> i32 {
        match self.scripted() {
            Some(value) => {
                let value = value as i32;
                assert!(value < bound.max(1), "roll {value} out of bound {bound}");
                value
            }
            None => self.fallback.next_int(bound),
        }
    }

    fn next_double(&mut self) -> f64 {
        match self.scripted() {
            Some(value) => value,
            None => self.fallback.next_double(),
        }
    }

    fn next_boolean(&mut self) -> bool {
        match self.scripted() {
            Some(value) => value != 0.0,
            None => self.fallback.next_boolean(),
        }
    }

    fn next_gaussian(&mut self) -> f64 {
        match self.scripted() {
            Some(value) => value,
            None => self.fallback.next_gaussian(),
        }
    }
}

impl NaturalSpawnHost for Level {
    fn nearest_player_distance_sqr(&self, position: Vec3) -> Option<f64> {
        self.players
            .iter()
            .map(|player| player.distance_squared(position))
            .reduce(f64::min)
    }

    fn respawn_pos(&self) -> Option<BlockPos> {
        self.respawn
    }

    fn can_spawn_entities_in_chunk(&self, _: ChunkPos) -> bool {
        true
    }

    fn naturally_spawns(&self, kind: EntityKind) -> bool {
        matches!(
            kind,
            EntityKind::Cow
                | EntityKind::Pig
                | EntityKind::Sheep
                | EntityKind::Chicken
                | EntityKind::Zombie
                | EntityKind::Skeleton
                | EntityKind::Creeper
                | EntityKind::Spider
        )
    }

    fn spawn_mob(
        &mut self,
        kind: EntityKind,
        position: Vec3,
        yaw: f32,
        _: SpawnReason,
        group: Option<SpawnGroupData>,
    ) -> (bool, Option<SpawnGroupData>) {
        self.spawned.push((kind, position, yaw));
        let dimensions = kind.dimensions();
        self.obstacles
            .push(Aabb::of_size(position, dimensions.width, dimensions.height));
        (true, group)
    }
}
