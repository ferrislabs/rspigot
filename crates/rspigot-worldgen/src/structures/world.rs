use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use super::bbox::Bbox;
use super::jigsaw::{Ground, Piece};
use super::library::Library;
use super::placement::{BiomeProbe, large_feature_seed};
use super::pool::{Element, Pools};
use super::sets::{Chunks, StructureSet, StructureSets};
use super::shaped::{Shaped, mineshaft};
use super::structure::{Kind, StructureDef, Structures, generate_jigsaw, valid_biomes};
use crate::legacy::JavaRandom;
use crate::overworld::features::compile_placed;
use crate::overworld::features::place::Placed;

pub(crate) const STEP_NAMES: [&str; 11] = [
    "raw_generation",
    "lakes",
    "local_modifications",
    "underground_structures",
    "surface_structures",
    "strongholds",
    "underground_ores",
    "underground_decoration",
    "fluid_springs",
    "vegetal_decoration",
    "top_layer_modification",
];

const REACH: i32 = 8;
const ADAPTATION_MARGIN: i32 = 12;
const CACHE_CAPACITY: usize = 4096;

#[derive(Debug)]
pub(crate) struct Start {
    pub(crate) name: String,
    pub(crate) chunk: (i32, i32),
    pub(crate) pieces: Vec<Piece<'static>>,
    pub(crate) shapes: Vec<Shaped>,
    pub(crate) bbox: Bbox,
}

#[derive(Debug, Default)]
struct StartCache {
    map: HashMap<(i32, i32), Arc<Vec<Arc<Start>>>>,
    order: std::collections::VecDeque<(i32, i32)>,
}

#[derive(Debug)]
pub(crate) struct Engine {
    seed: i64,
    sets: StructureSets,
    defs: Structures,
    library: &'static Library,
    pools: &'static Pools,
    valid: HashMap<String, Vec<u8>>,
    active: Vec<usize>,
    features: HashMap<String, Arc<Placed>>,
    order: Vec<Vec<String>>,
    cache: Mutex<StartCache>,
}

fn step_index(name: &str) -> usize {
    STEP_NAMES
        .iter()
        .position(|candidate| *candidate == name)
        .unwrap_or(4)
}

fn supported(def: &StructureDef, pools: &Pools) -> bool {
    let Kind::Jigsaw(jigsaw) = &def.kind else {
        return matches!(def.kind, Kind::Mineshaft(_));
    };
    if jigsaw.aliased {
        return false;
    }
    let Some(pool) = pools.get(&jigsaw.start_pool) else {
        return false;
    };
    pool.elements.iter().all(|(element, _)| match element {
        Element::Single { blueprint, .. } => blueprint.is_some(),
        _ => true,
    })
}

fn feature_names(pools: &Pools) -> Vec<String> {
    fn walk(element: &Element, out: &mut Vec<String>) {
        match element {
            Element::Feature { feature, .. } => out.push(feature.clone()),
            Element::List { elements, .. } => elements.iter().for_each(|inner| walk(inner, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    for pool in pools.all() {
        for (element, _) in &pool.elements {
            walk(element, &mut out);
        }
    }
    out.sort();
    out.dedup();
    out
}

impl Engine {
    pub(crate) fn new(seed: i64, probe: &impl BiomeProbe, library: &'static Library) -> Self {
        let sets = StructureSets::load(library);
        let defs = Structures::load(library);
        let pools = library.pools();
        let mut valid = HashMap::new();
        let mut order: Vec<Vec<String>> = vec![Vec::new(); STEP_NAMES.len()];
        let mut names: Vec<&StructureDef> = defs.all().collect();
        names.sort_by(|a, b| a.name.cmp(&b.name));
        for def in names {
            let ids = valid_biomes(def, probe);
            if !ids.is_empty() && supported(def, pools) {
                order[step_index(&def.step)].push(def.name.clone());
                valid.insert(def.name.clone(), ids);
            }
        }
        let active = sets
            .all()
            .iter()
            .enumerate()
            .filter(|(_, set)| {
                set.entries
                    .iter()
                    .any(|entry| valid.contains_key(&entry.structure))
            })
            .map(|(index, _)| index)
            .collect();
        Self {
            seed,
            sets,
            defs,
            library,
            pools,
            valid,
            active,
            features: compile_placed(&feature_names(pools)),
            order,
            cache: Mutex::new(StartCache::default()),
        }
    }

    pub(crate) fn library(&self) -> &'static Library {
        self.library
    }

    pub(crate) fn feature(&self, name: &str) -> Option<&Arc<Placed>> {
        self.features.get(name)
    }

    pub(crate) fn structures_at(&self, step: usize) -> &[String] {
        &self.order[step]
    }

    fn generate<G: Ground>(
        &self,
        def: &StructureDef,
        ground: &G,
        probe: &impl BiomeProbe,
        chunk: (i32, i32),
    ) -> Option<Start> {
        let valid = self.valid.get(&def.name)?;
        let jigsaw = match &def.kind {
            Kind::Jigsaw(jigsaw) => jigsaw,
            Kind::Mineshaft(ty) => {
                let pieces = mineshaft::generate(*ty, ground, valid, probe, self.seed, chunk)?;
                let mut bbox = pieces.first()?.bbox;
                for piece in &pieces {
                    bbox.encapsulate(&piece.bbox);
                }
                return Some(Start {
                    name: def.name.clone(),
                    chunk,
                    pieces: Vec::new(),
                    shapes: pieces.into_iter().map(Shaped::Mine).collect(),
                    bbox,
                });
            }
            _ => return None,
        };
        let assembly = generate_jigsaw(jigsaw, valid, self.pools, ground, probe, self.seed, chunk)?;
        let mut pieces = assembly.pieces;
        let first = pieces.first()?;
        let mut bbox = first.bbox;
        for piece in &pieces {
            bbox.encapsulate(&piece.bbox);
        }
        if def.terrain_adaptation != "none" {
            bbox = Bbox {
                min: (
                    bbox.min.0 - ADAPTATION_MARGIN,
                    bbox.min.1,
                    bbox.min.2 - ADAPTATION_MARGIN,
                ),
                max: (
                    bbox.max.0 + ADAPTATION_MARGIN,
                    bbox.max.1,
                    bbox.max.2 + ADAPTATION_MARGIN,
                ),
            };
        }
        pieces.shrink_to_fit();
        Some(Start {
            name: def.name.clone(),
            chunk,
            pieces,
            shapes: Vec::new(),
            bbox,
        })
    }

    fn try_set<G: Ground, B: BiomeProbe>(
        &self,
        set: &StructureSet,
        ground: &G,
        probe: &B,
        chunk: (i32, i32),
    ) -> Option<Start> {
        if set.entries.len() == 1 {
            return self.generate(
                self.defs.get(&set.entries[0].structure)?,
                ground,
                probe,
                chunk,
            );
        }
        let mut random = JavaRandom::new(0);
        large_feature_seed(&mut random, self.seed, chunk.0, chunk.1);
        let mut total: i32 = set.entries.iter().map(|entry| entry.weight).sum();
        let mut remaining: Vec<&super::sets::Entry> = set.entries.iter().collect();
        while !remaining.is_empty() {
            let mut roll = random.next_int(total);
            let mut index = 0;
            for entry in &remaining {
                roll -= entry.weight;
                if roll < 0 {
                    break;
                }
                index += 1;
            }
            let chosen = remaining.remove(index.min(remaining.len() - 1));
            total -= chosen.weight;
            if let Some(def) = self.defs.get(&chosen.structure)
                && let Some(start) = self.generate(def, ground, probe, chunk)
            {
                return Some(start);
            }
        }
        None
    }

    fn starts_in_chunk<G: Ground, B: BiomeProbe>(
        &self,
        ground: &G,
        probe: &B,
        chunk: (i32, i32),
    ) -> Arc<Vec<Arc<Start>>> {
        if let Some(found) = self
            .cache
            .lock()
            .ok()
            .and_then(|cache| cache.map.get(&chunk).cloned())
        {
            return found;
        }
        let world = Chunks {
            sets: &self.sets,
            seed: self.seed,
            biomes: probe,
        };
        let mut found = Vec::new();
        for &index in &self.active {
            let set = &self.sets.all()[index];
            if !world.is_structure_chunk(set, chunk.0, chunk.1) {
                continue;
            }
            if let Some(start) = self.try_set(set, ground, probe, chunk) {
                found.push(Arc::new(start));
            }
        }
        let found = Arc::new(found);
        if let Ok(mut cache) = self.cache.lock()
            && cache.map.insert(chunk, found.clone()).is_none()
        {
            cache.order.push_back(chunk);
            while cache.order.len() > CACHE_CAPACITY {
                if let Some(old) = cache.order.pop_front() {
                    cache.map.remove(&old);
                }
            }
        }
        found
    }

    pub(crate) fn starts_near<G: Ground, B: BiomeProbe>(
        &self,
        ground: &G,
        probe: &B,
        chunk: (i32, i32),
    ) -> Vec<Arc<Start>> {
        let area = Bbox {
            min: (chunk.0 * 16, i32::MIN, chunk.1 * 16),
            max: (chunk.0 * 16 + 15, i32::MAX, chunk.1 * 16 + 15),
        };
        let mut out = Vec::new();
        for sx in chunk.0 - REACH..=chunk.0 + REACH {
            for sz in chunk.1 - REACH..=chunk.1 + REACH {
                for start in self.starts_in_chunk(ground, probe, (sx, sz)).iter() {
                    if start.bbox.intersects(&area) {
                        out.push(start.clone());
                    }
                }
            }
        }
        out
    }
}
