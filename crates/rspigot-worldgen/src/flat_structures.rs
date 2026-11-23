use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use rspigot_data::multi_noise::overworld_biomes;
use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::world::Dimension;
use rspigot_domain::{BiomeId, BlockStateId, ChunkPos};
use rspigot_nbt::Compound;

use crate::climate::Xoroshiro;
use crate::overworld::features::place::Run;
use crate::overworld::region::{FlatTerrain, Region};
use crate::overworld::surface_rules::SurfaceSystem;
use crate::resources::Resources;
use crate::structures::{
    BiomeProbe, Engine, Ground, Heightmap, Library, STEP_NAMES, Start, attach, chunk_entities,
    place_start, spawned_entities,
};

pub(crate) struct FlatProbe {
    biome: u8,
}

impl BiomeProbe for FlatProbe {
    fn at(&self, _: i32, _: i32, _: i32) -> u8 {
        self.biome
    }

    fn id(&self, name: &str) -> Option<u8> {
        overworld_biomes()
            .iter()
            .position(|candidate| candidate.strip_prefix("minecraft:") == Some(name))
            .map(|index| index as u8)
    }
}

pub(crate) struct FlatGround<'a> {
    table: &'a [u16],
    min_y: i32,
    max_y: i32,
    sea_level: i32,
}

impl FlatGround<'_> {
    fn top(&self) -> i32 {
        self.table
            .iter()
            .rposition(|state| *state != 0)
            .map_or(self.min_y, |index| self.min_y + index as i32 + 1)
    }
}

impl Ground for FlatGround<'_> {
    fn first_free_height(&self, _: i32, _: i32, _: Heightmap) -> i32 {
        self.top()
    }

    fn min_y(&self) -> i32 {
        self.min_y
    }

    fn max_y(&self) -> i32 {
        self.max_y
    }

    fn sea_level(&self) -> i32 {
        self.sea_level
    }

    fn base(&self, _: i32, y: i32, _: i32) -> crate::structures::Base {
        use crate::structures::Base;
        let Some(state) = usize::try_from(y - self.min_y)
            .ok()
            .and_then(|index| self.table.get(index))
        else {
            return Base::Air;
        };
        let state = BlockStateId(*state);
        match state.kind() {
            _ if state.is_air() => Base::Air,
            Some(rspigot_blocks::BlockKind::Water) => Base::Water,
            Some(rspigot_blocks::BlockKind::Lava) => Base::Lava,
            _ => Base::Solid,
        }
    }
}

pub(crate) struct FlatStructures {
    engine: Engine,
    resources: Resources,
    surface: SurfaceSystem,
    seed: i64,
    spawned: Mutex<HashMap<(i32, i32), Vec<Compound>>>,
}

impl std::fmt::Debug for FlatStructures {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("FlatStructures")
            .finish_non_exhaustive()
    }
}

impl FlatStructures {
    pub(crate) fn new(seed: i64, biome: u8, overrides: Option<&[String]>) -> Self {
        let resources = Resources::resolve();
        let mut engine = Engine::new(seed, &FlatProbe { biome }, Library::current());
        if let Some(names) = overrides {
            engine.restrict_to(names);
        }
        Self {
            engine,
            surface: SurfaceSystem::with_rule(seed, &resources.blocks, None),
            resources,
            seed,
            spawned: Mutex::default(),
        }
    }

    fn starts(
        &self,
        terrain: &FlatTerrain,
        dimension: &Dimension,
        pos: ChunkPos,
    ) -> Vec<Arc<Start>> {
        let ground = FlatGround {
            table: &terrain.table,
            min_y: dimension.height.min_y,
            max_y: dimension.height.max_y(),
            sea_level: dimension.sea_level,
        };
        self.engine.starts_near(
            &ground,
            &FlatProbe {
                biome: terrain.biome,
            },
            (pos.x, pos.z),
        )
    }

    pub(crate) fn entities(
        &self,
        terrain: &FlatTerrain,
        dimension: &Dimension,
        pos: ChunkPos,
    ) -> Vec<Compound> {
        let starts = self.starts(terrain, dimension, pos);
        let mut entities = chunk_entities(&starts, (pos.x, pos.z));
        if let Ok(mut pending) = self.spawned.lock()
            && let Some(extra) = pending.remove(&(pos.x, pos.z))
        {
            entities.extend(extra);
        }
        entities
    }

    pub(crate) fn column(
        &self,
        terrain: &FlatTerrain,
        dimension: &Dimension,
        pos: ChunkPos,
        base: ChunkColumn,
        biome: BiomeId,
    ) -> ChunkColumn {
        let starts = self.starts(terrain, dimension, pos);
        if starts.is_empty() {
            return base;
        }
        let height = dimension.height;
        let min_y = height.min_y;
        let mut region = Region::for_flat(terrain, &self.resources, dimension, pos);
        region.set_random(
            Xoroshiro::from_seed(self.seed)
                .fork_positional()
                .named("minecraft:worldgen_region_random")
                .fork_positional()
                .at(pos.x * 16, 0, pos.z * 16),
        );
        let (ox, oz) = (pos.x * 16, pos.z * 16);
        let mut rng = Xoroshiro::from_seed(0);
        rng.reseed(self.seed);
        let a = (rng.next_long() | 1) as i64;
        let b = (rng.next_long() | 1) as i64;
        let population = (i64::from(ox)
            .wrapping_mul(a)
            .wrapping_add(i64::from(oz).wrapping_mul(b)))
            ^ self.seed;
        let write = crate::structures::Bbox {
            min: (ox, min_y + 1, oz),
            max: (ox + 15, min_y + height.height as i32 - 1, oz + 15),
        };
        for step in 0..STEP_NAMES.len() {
            for (index, name) in self.engine.structures_at(step).iter().enumerate() {
                rng.reseed(
                    population
                        .wrapping_add(index as i64)
                        .wrapping_add(10_000 * step as i64),
                );
                let mut run = Run {
                    region: &mut region,
                    rng: &mut rng,
                    surface: &self.surface,
                    feature_id: usize::MAX,
                    seed: self.seed,
                    biome_has: &[],
                    universal: &[],
                    interest: None,
                };
                for start in starts.iter().filter(|start| start.name == *name) {
                    place_start(&self.engine, &mut run, start, &write);
                }
            }
        }
        let block_entities = std::mem::take(&mut region.block_entities);
        let spawned = spawned_entities(std::mem::take(&mut region.entities));
        let (buffer, indices) = region.finish();
        let stored = vec![biome; indices.len()];
        let mut column = buffer.finish_layered(pos, &stored, &self.resources.classifier);
        attach(&mut column, &block_entities, &starts, pos);
        if !spawned.is_empty()
            && let Ok(mut pending) = self.spawned.lock()
        {
            pending.insert((pos.x, pos.z), spawned);
        }
        column
    }
}
