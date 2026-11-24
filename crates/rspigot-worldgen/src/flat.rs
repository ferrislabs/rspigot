use std::sync::{Arc, OnceLock};

use rspigot_blocks::BlockKind;
use rspigot_core::ports::ChunkGenerator;
use rspigot_data::multi_noise::overworld_biomes;
use rspigot_data::registries;
use rspigot_domain::chunk::{ChunkColumn, ChunkSection, PalettedContainer};
use rspigot_domain::world::{Dimension, WorldHeight};
use rspigot_domain::{BiomeId, BlockStateId, ChunkPos, Vec3};
use rspigot_nbt::Compound;
use serde::Deserialize;

use crate::buffer::{LAYER, SECTION, block_container};
use crate::classify::Classifier;
use crate::dimension::DimensionKind;
use crate::flat_structures::FlatStructures;
use crate::noise::LazyNoise;
use crate::overworld::region::FlatTerrain;
use crate::resources::biome_id;

#[derive(Debug, thiserror::Error)]
pub enum FlatSettingsError {
    #[error("invalid flat generator settings: {0}")]
    Json(#[from] serde_json::Error),
    #[error("unknown block {0}")]
    UnknownBlock(String),
    #[error("unknown biome {0}")]
    UnknownBiome(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct FlatLayer {
    pub block: String,
    pub height: u32,
}

fn default_biome() -> String {
    "minecraft:plains".to_owned()
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum StructureOverrides {
    One(String),
    Many(Vec<String>),
}

impl StructureOverrides {
    fn names(&self) -> Vec<String> {
        match self {
            Self::One(name) => vec![name.clone()],
            Self::Many(names) => names.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct FlatSettings {
    #[serde(default = "default_biome")]
    pub biome: String,
    pub layers: Vec<FlatLayer>,
    #[serde(default)]
    pub structure_overrides: Option<StructureOverrides>,
}

impl FlatSettings {
    pub fn vanilla() -> Self {
        let layer = |block: &str, height| FlatLayer {
            block: block.to_owned(),
            height,
        };
        Self {
            biome: default_biome(),
            layers: vec![
                layer("minecraft:bedrock", 1),
                layer("minecraft:dirt", 2),
                layer("minecraft:grass_block", 1),
            ],
            structure_overrides: Some(StructureOverrides::Many(vec![
                "minecraft:strongholds".to_owned(),
                "minecraft:villages".to_owned(),
            ])),
        }
    }

    pub fn from_json(json: &str) -> Result<Self, FlatSettingsError> {
        Ok(serde_json::from_str(json)?)
    }
}

fn qualified(name: &str) -> String {
    if name.contains(':') {
        name.to_owned()
    } else {
        format!("minecraft:{name}")
    }
}

#[derive(Debug, Clone)]
pub struct FlatGenerator {
    layers: Vec<(BlockStateId, u32)>,
    biome: BiomeId,
    classifier: Classifier,
    other_dimensions: LazyNoise,
    every_dimension: bool,
    seed: i64,
    overrides: Option<Vec<String>>,
    structures: bool,
    world: Arc<OnceLock<FlatStructures>>,
}

impl FlatGenerator {
    pub fn new(settings: &FlatSettings, seed: i64) -> Result<Self, FlatSettingsError> {
        let biome_name = qualified(&settings.biome);
        let biome = registries::biome_id(&biome_name)
            .map(BiomeId)
            .ok_or(FlatSettingsError::UnknownBiome(biome_name))?;
        let layers = settings
            .layers
            .iter()
            .map(|layer| {
                let name = qualified(&layer.block);
                BlockKind::from_name(&name)
                    .map(|kind| (kind.default_state(), layer.height))
                    .ok_or(FlatSettingsError::UnknownBlock(name))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut generator = Self::assemble(layers, biome, seed);
        generator.overrides = settings
            .structure_overrides
            .as_ref()
            .map(StructureOverrides::names);
        generator.structures = true;
        Ok(generator)
    }

    pub fn from_json(json: &str, seed: i64) -> Result<Self, FlatSettingsError> {
        Self::new(&FlatSettings::from_json(json)?, seed)
    }

    pub fn vanilla(seed: i64) -> Self {
        let layers = FlatSettings::vanilla()
            .layers
            .iter()
            .map(|layer| {
                let state = BlockKind::from_name(&layer.block)
                    .map_or(BlockStateId::AIR, BlockKind::default_state);
                (state, layer.height)
            })
            .collect();
        let mut generator = Self::assemble(layers, biome_id("minecraft:plains"), seed);
        generator.overrides = FlatSettings::vanilla()
            .structure_overrides
            .as_ref()
            .map(StructureOverrides::names);
        generator.structures = true;
        generator
    }

    #[must_use]
    pub fn without_structures(mut self) -> Self {
        self.structures = false;
        self
    }

    #[must_use]
    pub fn in_every_dimension(mut self) -> Self {
        self.every_dimension = true;
        self
    }

    fn assemble(layers: Vec<(BlockStateId, u32)>, biome: BiomeId, seed: i64) -> Self {
        Self {
            layers,
            biome,
            classifier: Classifier::new(),
            other_dimensions: LazyNoise::new(seed),
            every_dimension: false,
            seed,
            overrides: None,
            structures: false,
            world: Arc::new(OnceLock::new()),
        }
    }

    fn terrain(&self, dimension: &Dimension) -> FlatTerrain {
        FlatTerrain {
            table: self.column_table(dimension.height),
            biome: self.biome_index(),
        }
    }

    fn biome_index(&self) -> u8 {
        let plains = overworld_biomes()
            .iter()
            .position(|name| *name == "minecraft:plains")
            .unwrap_or(0);
        overworld_biomes()
            .iter()
            .position(|name| registries::biome_id(name) == Some(self.biome.0))
            .unwrap_or(plains) as u8
    }

    fn structures_world(&self) -> &FlatStructures {
        self.world.get_or_init(|| {
            FlatStructures::new(self.seed, self.biome_index(), self.overrides.as_deref())
        })
    }

    fn column_table(&self, height: WorldHeight) -> Vec<u16> {
        let mut table = vec![0u16; height.height as usize];
        let mut cursor = 0usize;
        for (state, count) in &self.layers {
            let end = (cursor + *count as usize).min(table.len());
            table[cursor..end].fill(state.0);
            cursor = end;
        }
        table
    }

    fn flat_column(&self, dimension: &Dimension, pos: ChunkPos) -> ChunkColumn {
        let height = dimension.height;
        let table = self.column_table(height);
        let sections: Vec<ChunkSection> = table
            .chunks(16)
            .map(|slice| {
                let first = slice[0];
                if slice.iter().all(|v| *v == first) {
                    let mut section = ChunkSection::empty(self.biome);
                    section.fill(BlockStateId(first));
                    return section;
                }
                let mut values = [0u16; SECTION];
                for (layer, state) in values.chunks_mut(LAYER).zip(slice) {
                    layer.fill(*state);
                }
                let (blocks, _) = block_container(&values);
                ChunkSection::from_parts(blocks, PalettedContainer::single(self.biome))
            })
            .collect();
        let mut column = ChunkColumn::from_sections(pos, height, sections)
            .unwrap_or_else(|| ChunkColumn::empty(pos, height, self.biome));
        column.heightmaps = self.classifier.heightmaps_1d(&table);
        column
    }

    fn flat_spawn(&self, dimension: &Dimension) -> Vec3 {
        let table = self.column_table(dimension.height);
        let top = table
            .iter()
            .rposition(|state| self.classifier.blocks_motion(BlockStateId(*state)))
            .map_or(0, |index| index + 1);
        Vec3::new(0.5, f64::from(dimension.height.min_y) + top as f64, 0.5)
    }
}

impl ChunkGenerator for FlatGenerator {
    fn generate(&self, dimension: &Dimension, pos: ChunkPos) -> ChunkColumn {
        if self.every_dimension || DimensionKind::of(dimension) == DimensionKind::Overworld {
            let column = self.flat_column(dimension, pos);
            if !self.structures {
                return column;
            }
            self.structures_world().column(
                &self.terrain(dimension),
                dimension,
                pos,
                column,
                self.biome,
            )
        } else {
            self.other_dimensions.get().generate(dimension, pos)
        }
    }

    fn spawn_point(&self, dimension: &Dimension) -> Vec3 {
        if self.every_dimension || DimensionKind::of(dimension) == DimensionKind::Overworld {
            self.flat_spawn(dimension)
        } else {
            self.other_dimensions.get().spawn_point(dimension)
        }
    }

    fn structure_entities(&self, dimension: &Dimension, pos: ChunkPos) -> Vec<Compound> {
        if self.every_dimension || DimensionKind::of(dimension) == DimensionKind::Overworld {
            if !self.structures {
                return Vec::new();
            }
            return self
                .structures_world()
                .entities(&self.terrain(dimension), dimension, pos);
        }
        self.other_dimensions
            .get()
            .structure_entities(dimension, pos)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_flat_world_builds_its_other_dimensions_only_when_one_is_asked_for() {
        let generator = FlatGenerator::vanilla(1);
        let overworld = Dimension::overworld();
        generator.generate(&overworld, ChunkPos::new(0, 0));
        generator.spawn_point(&overworld);
        assert!(
            generator
                .structure_entities(&overworld, ChunkPos::new(3, 3))
                .is_empty()
        );
        assert!(!generator.other_dimensions.is_built());
        generator.spawn_point(&Dimension::nether());
        assert!(generator.other_dimensions.is_built());
    }
    use rspigot_domain::BlockPos;
    use rspigot_domain::chunk::HeightmapKind;

    fn state(name: &str) -> BlockStateId {
        BlockKind::from_name(name).unwrap().default_state()
    }

    #[test]
    fn parses_vanilla_preset_json() {
        let json = r#"{"biome":"minecraft:plains","features":false,"lakes":false,
            "layers":[{"block":"minecraft:bedrock","height":1},{"block":"minecraft:dirt","height":2},
            {"block":"minecraft:grass_block","height":1}],"structure_overrides":["minecraft:strongholds","minecraft:villages"]}"#;
        assert_eq!(
            FlatSettings::from_json(json).unwrap(),
            FlatSettings::vanilla()
        );
    }

    #[test]
    fn biome_defaults_to_plains() {
        let settings = FlatSettings::from_json(r#"{"layers":[]}"#).unwrap();
        assert_eq!(settings.biome, "minecraft:plains");
        assert!(settings.layers.is_empty());
    }

    #[test]
    fn rejects_bad_settings() {
        assert!(matches!(
            FlatSettings::from_json("not json"),
            Err(FlatSettingsError::Json(_))
        ));
        assert!(matches!(
            FlatGenerator::from_json(r#"{"layers":[{"block":"minecraft:nope","height":1}]}"#, 0),
            Err(FlatSettingsError::UnknownBlock(_))
        ));
        assert!(matches!(
            FlatGenerator::from_json(r#"{"biome":"minecraft:nope","layers":[]}"#, 0),
            Err(FlatSettingsError::UnknownBiome(_))
        ));
    }

    #[test]
    fn bare_names_are_qualified() {
        let generator = FlatGenerator::from_json(
            r#"{"biome":"desert","layers":[{"block":"stone","height":2}]}"#,
            0,
        )
        .unwrap();
        let column = generator.generate(&Dimension::overworld(), ChunkPos::new(0, 0));
        assert_eq!(
            column.block(BlockPos::new(1, -64, 1)),
            Some(state("minecraft:stone"))
        );
        assert_eq!(
            column.sections()[0].biome(0, 0, 0),
            biome_id("minecraft:desert")
        );
    }

    #[test]
    fn layers_are_truncated_at_world_top() {
        let generator = FlatGenerator::from_json(
            r#"{"layers":[{"block":"minecraft:stone","height":100000}]}"#,
            0,
        )
        .unwrap();
        let column = generator.generate(&Dimension::overworld(), ChunkPos::new(2, 2));
        assert_eq!(
            column.block(BlockPos::new(32, 319, 32)),
            Some(state("minecraft:stone"))
        );
        assert_eq!(
            column
                .heightmaps
                .get(HeightmapKind::WorldSurface)
                .unwrap()
                .get(0, 0),
            384
        );
    }

    fn village_chunk(generator: &FlatGenerator) -> Option<ChunkPos> {
        let dimension = Dimension::overworld();
        for radius in 0..40_i32 {
            for cx in -radius..=radius {
                for cz in -radius..=radius {
                    if cx.abs().max(cz.abs()) != radius {
                        continue;
                    }
                    let pos = ChunkPos::new(cx, cz);
                    let column = generator.generate(&dimension, pos);
                    let found = column
                        .extra
                        .get_compound("structures")
                        .and_then(|tag| tag.get_compound("starts"))
                        .is_some_and(|starts| starts.contains_key("minecraft:village_plains"));
                    if found {
                        return Some(pos);
                    }
                }
            }
        }
        None
    }

    #[test]
    fn the_default_superflat_builds_villages_and_without_structures_it_does_not() {
        let with = FlatGenerator::vanilla(1);
        let at = village_chunk(&with).expect("a village in range");
        let dimension = Dimension::overworld();
        let mut buildings = 0;
        for dx in -2..=2 {
            for dz in -2..=2 {
                let pos = ChunkPos::new(at.x + dx, at.z + dz);
                let column = with.generate(&dimension, pos);
                for y in -61..-55 {
                    for z in 0..16 {
                        for x in 0..16 {
                            let block =
                                BlockPos::new(pos.min_block_x() + x, y, pos.min_block_z() + z);
                            if column.block(block).is_some_and(|state| {
                                matches!(
                                    state.kind(),
                                    Some(BlockKind::OakPlanks | BlockKind::Cobblestone)
                                )
                            }) {
                                buildings += 1;
                            }
                        }
                    }
                }
            }
        }
        assert!(buildings > 50, "only {buildings} village blocks");
        let without = FlatGenerator::vanilla(1).without_structures();
        let column = without.generate(&dimension, at);
        assert!(
            column
                .extra
                .get_compound("structures")
                .and_then(|tag| tag.get_compound("starts"))
                .is_none_or(|starts| starts.is_empty())
        );
    }

    #[test]
    fn structure_overrides_pick_which_sets_generate() {
        let json = r#"{"layers":[{"block":"minecraft:bedrock","height":1},
            {"block":"minecraft:dirt","height":2},{"block":"minecraft:grass_block","height":1}],
            "structure_overrides":"minecraft:strongholds"}"#;
        let generator = FlatGenerator::from_json(json, 1).unwrap();
        assert!(village_chunk(&generator).is_none());
    }
}
