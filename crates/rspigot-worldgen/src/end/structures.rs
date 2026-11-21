use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use rspigot_domain::world::Dimension;
use rspigot_domain::{BlockPos, ChunkPos, Vec3};
use rspigot_nbt::Compound;

use super::islands::EndIslands;
use super::{END_BIOMES, End, islands_biome};
use crate::resources::Resources;
use crate::structures::{BiomeProbe, Ground, Heightmap, Start};

pub(super) type Spawned = HashMap<(i32, i32), Vec<Compound>>;

pub(super) struct EndProbe<'a> {
    pub(super) islands: &'a EndIslands,
}

impl BiomeProbe for EndProbe<'_> {
    fn at(&self, qx: i32, _qy: i32, qz: i32) -> u8 {
        islands_biome(self.islands, qx, qz)
    }

    fn id(&self, name: &str) -> Option<u8> {
        END_BIOMES
            .iter()
            .position(|candidate| candidate.strip_prefix("minecraft:") == Some(name))
            .map(|index| index as u8)
    }
}

pub(super) struct EndGround<'a> {
    end: &'a End,
    resources: &'a Resources,
    dimension: &'a Dimension,
    tops: RefCell<HashMap<(i32, i32), Vec<i32>>>,
}

impl EndGround<'_> {
    fn column_tops(&self, chunk: (i32, i32)) -> Vec<i32> {
        let terrain = self.end.terrain_cached(
            self.resources,
            self.dimension,
            ChunkPos::new(chunk.0, chunk.1),
        );
        let (min_y, max_y) = (self.min_y(), self.max_y());
        let mut tops = Vec::with_capacity(256);
        for z in 0..16usize {
            for x in 0..16usize {
                let mut top = min_y;
                for y in (min_y..=max_y).rev() {
                    if !terrain.buffer.get(x, y, z).is_air() {
                        top = y + 1;
                        break;
                    }
                }
                tops.push(top);
            }
        }
        tops
    }
}

impl Ground for EndGround<'_> {
    fn first_free_height(&self, x: i32, z: i32, _heightmap: Heightmap) -> i32 {
        let chunk = (x >> 4, z >> 4);
        let mut cache = self.tops.borrow_mut();
        let tops = cache
            .entry(chunk)
            .or_insert_with(|| self.column_tops(chunk));
        tops[(((z & 15) << 4) | (x & 15)) as usize]
    }

    fn min_y(&self) -> i32 {
        self.dimension.height.min_y
    }

    fn max_y(&self) -> i32 {
        self.dimension.height.max_y()
    }

    fn sea_level(&self) -> i32 {
        self.dimension.sea_level
    }
}

impl End {
    fn ground<'a>(&'a self, resources: &'a Resources, dimension: &'a Dimension) -> EndGround<'a> {
        EndGround {
            end: self,
            resources,
            dimension,
            tops: RefCell::default(),
        }
    }

    fn probe(&self) -> EndProbe<'_> {
        EndProbe {
            islands: &self.islands,
        }
    }

    pub(crate) fn starts_near(
        &self,
        resources: &Resources,
        dimension: &Dimension,
        pos: ChunkPos,
    ) -> Vec<Arc<Start>> {
        if !self.structures_on {
            return Vec::new();
        }
        let ground = self.ground(resources, dimension);
        self.structures
            .starts_near(&ground, &self.probe(), (pos.x, pos.z))
    }

    pub(super) fn remember_spawned(&self, pos: ChunkPos, entities: Vec<serde_json::Value>) {
        let spawned = crate::structures::spawned_entities(entities);
        if !spawned.is_empty()
            && let Ok(mut pending) = self.spawned.lock()
        {
            pending.insert((pos.x, pos.z), spawned);
        }
    }

    pub(crate) fn structure_entities(
        &self,
        resources: &Resources,
        dimension: &Dimension,
        pos: ChunkPos,
    ) -> Vec<Compound> {
        let starts = self.starts_near(resources, dimension, pos);
        let mut entities = crate::structures::chunk_entities(&starts, (pos.x, pos.z));
        if let Ok(mut pending) = self.spawned.lock()
            && let Some(extra) = pending.remove(&(pos.x, pos.z))
        {
            entities.extend(extra);
        }
        entities
    }

    pub(crate) fn nearest_structures(
        &self,
        resources: &Resources,
        dimension: &Dimension,
        from: Vec3,
        ids: &[String],
    ) -> Option<(String, Vec3)> {
        if !self.structures_on {
            return None;
        }
        let wanted: Vec<String> = ids
            .iter()
            .map(|id| id.strip_prefix("minecraft:").unwrap_or(id).to_owned())
            .collect();
        let ground = self.ground(resources, dimension);
        let (name, at) = self.structures.nearest_start(
            &ground,
            &|| self.probe(),
            &wanted,
            (from.x.floor() as i32, from.z.floor() as i32),
            100,
        )?;
        Some((
            format!("minecraft:{name}"),
            Vec3::new(f64::from(at.0), f64::from(at.1), f64::from(at.2)),
        ))
    }

    pub(crate) fn knows_structure(&self, id: &str) -> bool {
        self.structures_on
            && self
                .structures
                .knows(id.strip_prefix("minecraft:").unwrap_or(id))
    }

    pub(crate) fn place_structure(
        &self,
        resources: &Resources,
        dimension: &Dimension,
        id: &str,
        pos: BlockPos,
    ) -> Result<Vec<rspigot_core::ports::PlacedBlock>, rspigot_core::ports::PlaceFailure> {
        use rspigot_core::ports::PlaceFailure;
        let name = id.strip_prefix("minecraft:").unwrap_or(id);
        if !self.structures_on || !self.structures.knows(name) {
            return Err(PlaceFailure::Failed);
        }
        let ground = self.ground(resources, dimension);
        let start = self
            .structures
            .start_at(&ground, &self.probe(), name, (pos.x >> 4, pos.z >> 4))
            .ok_or(PlaceFailure::Failed)?;
        crate::placing::placed(dimension, crate::structures::start_blocks(&start))
    }
}
