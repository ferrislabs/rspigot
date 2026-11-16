use std::collections::HashMap;

use serde_json::Value;

use super::data::short;
use super::jigsaw::{Heightmap, JigsawConfig};
use super::library::Library;

#[derive(Debug, Clone)]
pub(crate) struct JigsawDef {
    pub(crate) start_pool: String,
    pub(crate) start_jigsaw: Option<String>,
    pub(crate) size: i32,
    pub(crate) expansion_hack: bool,
    pub(crate) project: Option<Heightmap>,
    pub(crate) max_horizontal: i32,
    pub(crate) max_vertical: i32,
    pub(crate) padding: (i32, i32),
    pub(crate) start_height: HeightAnchor,
    pub(crate) aliased: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HeightAnchor {
    Absolute(i32),
    AboveBottom(i32),
    BelowTop(i32),
    Unsupported,
}

impl HeightAnchor {
    pub(crate) fn resolve(self, min_y: i32, max_y: i32) -> Option<i32> {
        match self {
            Self::Absolute(y) => Some(y),
            Self::AboveBottom(offset) => Some(min_y + offset),
            Self::BelowTop(offset) => Some(max_y + 1 - offset),
            Self::Unsupported => None,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) enum Kind {
    Jigsaw(JigsawDef),
    Other,
}

#[derive(Debug, Clone)]
pub(crate) struct StructureDef {
    pub(crate) name: String,
    pub(crate) kind: Kind,
    pub(crate) biomes: Vec<String>,
    pub(crate) step: String,
    pub(crate) terrain_adaptation: String,
}

#[derive(Debug)]
pub(crate) struct Structures {
    defs: HashMap<String, StructureDef>,
}

fn int(value: &Value, key: &str, default: i32) -> i32 {
    value
        .get(key)
        .and_then(Value::as_i64)
        .and_then(|n| i32::try_from(n).ok())
        .unwrap_or(default)
}

fn pair(value: Option<&Value>, first: &str, second: &str) -> (i32, i32) {
    match value {
        Some(Value::Number(number)) => {
            let n = number
                .as_i64()
                .and_then(|n| i32::try_from(n).ok())
                .unwrap_or(0);
            (n, n)
        }
        Some(object @ Value::Object(_)) => (int(object, first, 0), int(object, second, 0)),
        _ => (0, 0),
    }
}

fn anchor(value: Option<&Value>) -> HeightAnchor {
    let Some(value) = value else {
        return HeightAnchor::Unsupported;
    };
    if value.get("type").is_some() {
        return HeightAnchor::Unsupported;
    }
    if let Some(y) = value.get("absolute").and_then(Value::as_i64) {
        return HeightAnchor::Absolute(y as i32);
    }
    if let Some(y) = value.get("above_bottom").and_then(Value::as_i64) {
        return HeightAnchor::AboveBottom(y as i32);
    }
    if let Some(y) = value.get("below_top").and_then(Value::as_i64) {
        return HeightAnchor::BelowTop(y as i32);
    }
    HeightAnchor::Unsupported
}

fn jigsaw(value: &Value) -> JigsawDef {
    let (horizontal, vertical) = match value.get("max_distance_from_center") {
        Some(Value::Number(number)) => {
            let n = number.as_i64().unwrap_or(0) as i32;
            (n, n)
        }
        other => pair(other, "horizontal", "vertical"),
    };
    JigsawDef {
        start_pool: value
            .get("start_pool")
            .and_then(Value::as_str)
            .unwrap_or("minecraft:empty")
            .to_owned(),
        start_jigsaw: value
            .get("start_jigsaw_name")
            .and_then(Value::as_str)
            .map(str::to_owned),
        size: int(value, "size", 0),
        expansion_hack: value
            .get("use_expansion_hack")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        project: match value
            .get("project_start_to_heightmap")
            .and_then(Value::as_str)
        {
            Some("WORLD_SURFACE_WG") => Some(Heightmap::WorldSurfaceWg),
            Some("OCEAN_FLOOR_WG") => Some(Heightmap::OceanFloorWg),
            _ => None,
        },
        max_horizontal: horizontal,
        max_vertical: vertical,
        padding: pair(value.get("dimension_padding"), "bottom", "top"),
        start_height: anchor(value.get("start_height")),
        aliased: value
            .get("pool_aliases")
            .and_then(Value::as_array)
            .is_some_and(|aliases| !aliases.is_empty()),
    }
}

impl JigsawDef {
    pub(crate) fn config(&self) -> JigsawConfig<'_> {
        JigsawConfig {
            start_pool: &self.start_pool,
            start_jigsaw: self.start_jigsaw.as_deref(),
            size: self.size,
            expansion_hack: self.expansion_hack,
            project: self.project,
            max_horizontal: self.max_horizontal,
            max_vertical: self.max_vertical,
            padding: self.padding,
        }
    }
}

impl Structures {
    pub(crate) fn load(library: &Library) -> Self {
        let mut defs = HashMap::new();
        {
            for (name, value) in library.structures() {
                let kind_name = short(value.get("type").and_then(Value::as_str).unwrap_or(""));
                let kind = if kind_name == "jigsaw" {
                    Kind::Jigsaw(jigsaw(value))
                } else {
                    Kind::Other
                };
                defs.insert(
                    name.clone(),
                    StructureDef {
                        name: name.clone(),
                        kind,
                        biomes: value
                            .get("biomes")
                            .and_then(Value::as_array)
                            .map(|names| {
                                names
                                    .iter()
                                    .filter_map(Value::as_str)
                                    .map(|name| short(name).to_owned())
                                    .collect()
                            })
                            .unwrap_or_default(),
                        step: value
                            .get("step")
                            .and_then(Value::as_str)
                            .unwrap_or("surface_structures")
                            .to_owned(),
                        terrain_adaptation: value
                            .get("terrain_adaptation")
                            .and_then(Value::as_str)
                            .unwrap_or("none")
                            .to_owned(),
                    },
                );
            }
        }
        Self { defs }
    }

    pub(crate) fn all(&self) -> impl Iterator<Item = &StructureDef> {
        self.defs.values()
    }

    pub(crate) fn get(&self, name: &str) -> Option<&StructureDef> {
        self.defs.get(name)
    }
}

pub(crate) fn valid_biomes(
    def: &StructureDef,
    probe: &impl super::placement::BiomeProbe,
) -> Vec<u8> {
    def.biomes
        .iter()
        .filter_map(|name| probe.id(name))
        .collect()
}

pub(crate) fn generate_jigsaw<'a, G: super::jigsaw::Ground>(
    def: &JigsawDef,
    valid: &[u8],
    pools: &'a super::pool::Pools,
    ground: &G,
    probe: &impl super::placement::BiomeProbe,
    seed: i64,
    chunk: (i32, i32),
) -> Option<super::jigsaw::Assembly<'a>> {
    let mut random = crate::legacy::JavaRandom::new(0);
    super::placement::large_feature_seed(&mut random, seed, chunk.0, chunk.1);
    if def.aliased {
        return None;
    }
    let y = def.start_height.resolve(ground.min_y(), ground.max_y())?;
    let position = (chunk.0 * 16, y, chunk.1 * 16);
    let could_exist = |x: i32, z: i32| {
        (ground.min_y() >> 2..=ground.max_y() >> 2)
            .any(|qy| valid.contains(&probe.at(x >> 2, qy, z >> 2)))
    };
    let assembly = super::jigsaw::assemble(
        pools,
        ground,
        &def.config(),
        &mut random,
        position,
        could_exist,
    )?;
    let at = assembly.position;
    valid
        .contains(&probe.at(at.0 >> 2, at.1 >> 2, at.2 >> 2))
        .then_some(assembly)
}
