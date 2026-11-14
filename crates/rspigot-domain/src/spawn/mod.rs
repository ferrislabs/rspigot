mod bat;
mod biome;
pub mod despawn;
pub mod endermite;
pub mod fox;
pub mod generation;
mod ghast;
pub mod natural;
mod piglin;
pub mod placement;
mod polar_bear;
pub mod rules;
pub mod silverfish;
pub mod slime;
pub mod state;
mod water;
pub mod wolf;
mod zombified_piglin;

use rspigot_data::spawns::{BiomeSpawns, DimensionSpawns, SpawnCount, Spawner};
use rspigot_entities::{EntityKind, MobCategory, SpawnHeightmap};

use crate::chunk::ChunkColumn;
use crate::effect::EffectId;
use crate::entity::ageable::DEFAULT_BABY_CHANCE;
use crate::entity::physics::has_tag;
use crate::entity::{Aabb, BlockView};
use crate::random::{RandomSource, random_between_inclusive};
use crate::world::{Difficulty, WorldHeight};
use crate::{BiomeId, BlockPos, BlockStateId, ChunkPos, Vec3};

pub use biome::{noise_biome, zoomed_biome};

pub const WORLD_BORDER: f64 = 29_999_984.0;
const BRIGHTNESS_LIMIT: i32 = 30_000_000;
const MAX_BRIGHTNESS: i32 = 15;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SpawnReason {
    Natural,
    ChunkGeneration,
    Spawner,
    Structure,
    Breeding,
    MobSummoned,
    Jockey,
    Event,
    Conversion,
    Reinforcement,
    Triggered,
    Bucket,
    SpawnItemUse,
    Command,
    Dispenser,
    Patrol,
    TrialSpawner,
    Load,
    DimensionTravel,
}

impl SpawnReason {
    pub const fn is_spawner(self) -> bool {
        matches!(self, Self::Spawner | Self::TrialSpawner)
    }

    pub const fn ignores_light_requirements(self) -> bool {
        matches!(self, Self::TrialSpawner)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SpawnGroupData {
    Ageable {
        group_size: i32,
        should_spawn_baby: bool,
        baby_chance: f32,
    },
    Zombie {
        baby: bool,
        can_spawn_jockey: bool,
    },
    SpiderEffects {
        effect: Option<EffectId>,
    },
    Fox {
        variant: crate::entity::species::fox::Variant,
        group_size: i32,
    },
    WolfPack {
        variant: u32,
        group_size: i32,
    },
    Horse {
        variant: i32,
        group_size: i32,
    },
    Llama {
        variant: crate::entity::species::llama::Variant,
        group_size: i32,
    },
    Rabbit {
        group_size: i32,
        variant: i32,
    },
    Axolotl {
        variants: [u8; 2],
        group_size: i32,
    },
    School {
        leader: crate::EntityId,
    },
    TropicalSchool {
        leader: crate::EntityId,
        variant: i32,
    },
}

impl SpawnGroupData {
    pub const fn ageable(should_spawn_baby: bool) -> Self {
        Self::Ageable {
            group_size: 0,
            should_spawn_baby,
            baby_chance: DEFAULT_BABY_CHANCE,
        }
    }
}

pub fn within_border(x: f64, z: f64) -> bool {
    (-WORLD_BORDER..WORLD_BORDER).contains(&x) && (-WORLD_BORDER..WORLD_BORDER).contains(&z)
}

pub fn block_within_border(pos: BlockPos) -> bool {
    within_border(f64::from(pos.x), f64::from(pos.z))
}

pub trait SpawnLevel: BlockView {
    fn height(&self) -> WorldHeight;
    fn sea_level(&self) -> i32;
    fn sky_light(&self, pos: BlockPos) -> i32;
    fn block_light(&self, pos: BlockPos) -> i32;
    fn sky_darken(&self) -> i32;
    fn thundering(&self) -> bool;
    fn difficulty(&self) -> Difficulty;
    fn dimension(&self) -> &DimensionSpawns;
    fn biome(&self, pos: BlockPos) -> BiomeId;
    fn top_height(&self, heightmap: SpawnHeightmap, x: i32, z: i32) -> i32;
    fn is_unobstructed(&self, area: &Aabb) -> bool;
    fn no_entity_collision(&self, area: &Aabb) -> bool;
    fn survival_player_within(&self, center: Vec3, distance: f64) -> bool;
    fn is_slime_chunk(&self, chunk: ChunkPos) -> bool;
    fn surface_slime_spawn_chance(&self) -> f32;

    fn raw_brightness(&self, pos: BlockPos, darken: i32) -> i32 {
        (self.sky_light(pos) - darken).max(self.block_light(pos))
    }

    fn max_local_raw_brightness(&self, pos: BlockPos, darken: i32) -> i32 {
        if pos.x >= -BRIGHTNESS_LIMIT
            && pos.z >= -BRIGHTNESS_LIMIT
            && pos.x < BRIGHTNESS_LIMIT
            && pos.z < BRIGHTNESS_LIMIT
        {
            self.raw_brightness(pos, darken)
        } else {
            MAX_BRIGHTNESS
        }
    }

    fn light_level_dependent_magic_value(&self, pos: BlockPos) -> f32 {
        let level = self.max_local_raw_brightness(pos, self.sky_darken()) as f32 / 15.0;
        let curve = level / (4.0 - 3.0 * level);
        let ambient = self.dimension().ambient_light;
        ambient + curve * (1.0 - ambient)
    }

    fn spawns_at(&self, pos: BlockPos) -> &'static BiomeSpawns {
        rspigot_data::spawns::biome(self.biome(pos).0)
    }
}

const MOTION_BLOCKING_NO_LEAVES: &str = "minecraft:blocks_motion_in_heightmap_no_leaves";

fn in_heightmap(heightmap: SpawnHeightmap, state: BlockStateId) -> bool {
    let flags = state.flags();
    match heightmap {
        SpawnHeightmap::WorldSurface | SpawnHeightmap::WorldSurfaceWg => !state.is_air(),
        SpawnHeightmap::OceanFloor | SpawnHeightmap::OceanFloorWg => {
            flags.contains(rspigot_blocks::StateFlags::BLOCKS_MOTION)
        }
        SpawnHeightmap::MotionBlocking => {
            flags.contains(rspigot_blocks::StateFlags::BLOCKS_MOTION) || flags.has_fluid()
        }
        SpawnHeightmap::MotionBlockingNoLeaves => {
            has_tag(state, MOTION_BLOCKING_NO_LEAVES) || flags.has_fluid()
        }
    }
}

pub fn column_top(chunk: &ChunkColumn, heightmap: SpawnHeightmap, x: i32, z: i32) -> i32 {
    chunk
        .highest_block((x & 15) as usize, (z & 15) as usize, |state| {
            in_heightmap(heightmap, state)
        })
        .map_or(chunk.height().min_y, |y| y + 1)
}

pub fn category_spawners(
    spawns: &'static BiomeSpawns,
    category: MobCategory,
) -> &'static [Spawner] {
    match category {
        MobCategory::Monster => spawns.monster,
        MobCategory::Creature => spawns.creature,
        MobCategory::Ambient => spawns.ambient,
        MobCategory::Axolotls => spawns.axolotls,
        MobCategory::UndergroundWaterCreature => spawns.underground_water_creature,
        MobCategory::WaterCreature => spawns.water_creature,
        MobCategory::WaterAmbient => spawns.water_ambient,
        MobCategory::Misc => spawns.misc,
    }
}

pub fn sample_count(count: SpawnCount, random: &mut impl RandomSource) -> i32 {
    match count {
        SpawnCount::Constant(value) => value,
        SpawnCount::Uniform { min, max } => random_between_inclusive(random, min, max),
    }
}

pub fn pick_spawner(
    spawners: &'static [Spawner],
    random: &mut impl RandomSource,
) -> Option<&'static Spawner> {
    let total: i32 = spawners.iter().map(|spawner| spawner.weight).sum();
    if spawners.is_empty() {
        return None;
    }
    let mut index = random.next_int(total);
    for spawner in spawners {
        index -= spawner.weight;
        if index < 0 {
            return Some(spawner);
        }
    }
    None
}

pub fn spawner_kind(spawner: &Spawner) -> Option<EntityKind> {
    EntityKind::from_id(rspigot_entities::EntityTypeId(spawner.entity))
}

pub fn spawn_aabb(kind: EntityKind, x: f64, y: f64, z: f64) -> Aabb {
    let scale = kind
        .mob_spawn()
        .map_or(1.0, |spawn| spawn.spawn_dimensions_scale);
    let dimensions = kind.dimensions();
    let half = f64::from(scale * dimensions.width / 2.0);
    let height = f64::from(scale * dimensions.height);
    Aabb {
        min_x: x - half,
        min_y: y,
        min_z: z - half,
        max_x: x + half,
        max_y: y + height,
        max_z: z + half,
    }
}

#[cfg(test)]
pub(crate) mod testing;
