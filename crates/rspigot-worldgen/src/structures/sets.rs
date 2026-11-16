use serde_json::Value;

use super::data::{short, structure_data};
use super::placement::{BiomeProbe, Neighbours, Placement};

#[derive(Debug)]
pub(crate) struct Entry {
    pub(crate) structure: String,
    pub(crate) weight: i32,
}

#[derive(Debug)]
pub(crate) struct StructureSet {
    pub(crate) name: String,
    pub(crate) placement: Placement,
    pub(crate) entries: Vec<Entry>,
}

#[derive(Debug)]
pub(crate) struct StructureSets {
    sets: Vec<StructureSet>,
}

pub(crate) struct Chunks<'a, B> {
    pub(crate) sets: &'a StructureSets,
    pub(crate) seed: i64,
    pub(crate) biomes: &'a B,
}

impl StructureSets {
    pub(crate) fn load() -> Self {
        let mut sets = Vec::new();
        if let Some(Value::Object(map)) = structure_data().get("sets") {
            for (name, value) in map {
                let Some(placement) = value.get("placement").and_then(Placement::parse) else {
                    continue;
                };
                let entries = value
                    .get("structures")
                    .and_then(Value::as_array)
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(|item| {
                                Some(Entry {
                                    structure: short(item.get("structure")?.as_str()?).to_owned(),
                                    weight: i32::try_from(item.get("weight")?.as_i64()?).ok()?,
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                sets.push(StructureSet {
                    name: name.clone(),
                    placement,
                    entries,
                });
            }
        }
        Self { sets }
    }

    pub(crate) fn all(&self) -> &[StructureSet] {
        &self.sets
    }

    pub(crate) fn named(&self, name: &str) -> Option<&StructureSet> {
        self.sets.iter().find(|set| set.name == name)
    }
}

impl<B: BiomeProbe> Chunks<'_, B> {
    pub(crate) fn is_structure_chunk(&self, set: &StructureSet, x: i32, z: i32) -> bool {
        set.placement
            .is_structure_chunk(self.seed, self.biomes, self, x, z)
    }
}

impl<B: BiomeProbe> Neighbours for Chunks<'_, B> {
    fn has_structure_chunk(&self, set: &str, x: i32, z: i32, range: i32) -> bool {
        let Some(other) = self.sets.named(set) else {
            return false;
        };
        (x - range..=x + range).any(|cx| {
            (z - range..=z + range).any(|cz| self.is_structure_chunk(other, cx, cz))
        })
    }
}
