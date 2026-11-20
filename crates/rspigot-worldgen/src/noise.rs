use rspigot_core::ports::{ChunkGenerator, PlaceFailure, PlaceRequest, PlacedBlock};
use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::world::Dimension;
use rspigot_domain::{ChunkPos, Vec3};
use rspigot_nbt::Compound;

use crate::dimension::DimensionKind;
use crate::end::End;
use crate::nether::Nether;
use crate::overworld::Overworld;
use crate::resources::Resources;
use rspigot_data::multi_noise::ClimateVariant;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NoiseOptions {
    pub biome_scale: f64,
    pub amplitude: f64,
}

impl NoiseOptions {
    pub const NORMAL: Self = Self {
        biome_scale: 1.0,
        amplitude: 1.0,
    };
    pub const LARGE_BIOMES: Self = Self {
        biome_scale: 4.0,
        amplitude: 1.0,
    };
    pub const AMPLIFIED: Self = Self {
        biome_scale: 1.0,
        amplitude: 2.0,
    };
}

#[derive(Debug, Clone)]
struct NoiseState {
    resources: Resources,
    overworld: Overworld,
    nether: Nether,
    end: End,
}

#[derive(Debug, Clone)]
pub struct NoiseGenerator {
    state: Box<NoiseState>,
}

impl NoiseGenerator {
    pub fn new(seed: i64) -> Self {
        Self::with_options(seed, NoiseOptions::NORMAL)
    }

    pub fn with_options(seed: i64, options: NoiseOptions) -> Self {
        let seed = seed as u64;
        let resources = Resources::resolve();
        let variant = if options.biome_scale > 1.0 {
            ClimateVariant::LargeBiomes
        } else if options.amplitude > 1.0 {
            ClimateVariant::Amplified
        } else {
            ClimateVariant::Normal
        };
        Self {
            state: Box::new(NoiseState {
                overworld: Overworld::new(seed, variant, &resources.blocks),
                nether: Nether::new(seed, &resources.blocks),
                end: End::new(seed, &resources.blocks),
                resources,
            }),
        }
    }
}

impl NoiseGenerator {
    #[must_use]
    pub fn without_structures(mut self) -> Self {
        self.state.overworld.disable_structures();
        self.state.nether.disable_structures();
        self
    }
}

impl ChunkGenerator for NoiseGenerator {
    fn generate(&self, dimension: &Dimension, pos: ChunkPos) -> ChunkColumn {
        match DimensionKind::of(dimension) {
            DimensionKind::Overworld => {
                self.state
                    .overworld
                    .generate(&self.state.resources, dimension, pos)
            }
            DimensionKind::Nether => {
                self.state
                    .nether
                    .generate(&self.state.resources, dimension, pos)
            }
            DimensionKind::End => self
                .state
                .end
                .generate(&self.state.resources, dimension, pos),
        }
    }

    fn spawn_point(&self, dimension: &Dimension) -> Vec3 {
        match DimensionKind::of(dimension) {
            DimensionKind::Overworld => self
                .state
                .overworld
                .spawn_point(&self.state.resources, dimension),
            DimensionKind::Nether => self
                .state
                .nether
                .spawn_point(&self.state.resources, dimension),
            DimensionKind::End => self.state.end.spawn_point(&self.state.resources, dimension),
        }
    }

    fn spawns_original_mobs(&self, dimension: &Dimension) -> bool {
        DimensionKind::of(dimension) != DimensionKind::End
    }

    fn structure_entities(&self, dimension: &Dimension, pos: ChunkPos) -> Vec<Compound> {
        match DimensionKind::of(dimension) {
            DimensionKind::End => self.state.end.pillars().crystals(pos),
            DimensionKind::Overworld => self.state.overworld.structure_entities(dimension, pos),
            DimensionKind::Nether => self.state.nether.structure_entities(dimension, pos),
        }
    }

    fn nearest_structure(
        &self,
        dimension: &Dimension,
        from: Vec3,
        structure_id: &str,
    ) -> Option<(String, Vec3)> {
        self.nearest_structures(dimension, from, &[structure_id.to_owned()])
    }

    fn nearest_structures(
        &self,
        dimension: &Dimension,
        from: Vec3,
        structure_ids: &[String],
    ) -> Option<(String, Vec3)> {
        match DimensionKind::of(dimension) {
            DimensionKind::Overworld => {
                self.state
                    .overworld
                    .nearest_structures(dimension, from, structure_ids)
            }
            DimensionKind::Nether => {
                self.state
                    .nether
                    .nearest_structures(dimension, from, structure_ids)
            }
            DimensionKind::End => None,
        }
    }

    fn knows_structure(&self, dimension: &Dimension, structure_id: &str) -> bool {
        match DimensionKind::of(dimension) {
            DimensionKind::Overworld => self.state.overworld.knows_structure(structure_id),
            DimensionKind::Nether => self.state.nether.knows_structure(structure_id),
            DimensionKind::End => false,
        }
    }

    fn nearest_biome(
        &self,
        dimension: &Dimension,
        from: Vec3,
        biomes: &[String],
    ) -> Option<(String, Vec3)> {
        match DimensionKind::of(dimension) {
            DimensionKind::Overworld => self.state.overworld.nearest_biome(dimension, from, biomes),
            DimensionKind::Nether => self.state.nether.nearest_biome(dimension, from, biomes),
            DimensionKind::End => self.state.end.nearest_biome(dimension, from, biomes),
        }
    }

    fn place(
        &self,
        dimension: &Dimension,
        request: &PlaceRequest,
    ) -> Result<Vec<PlacedBlock>, PlaceFailure> {
        match (request, DimensionKind::of(dimension)) {
            (PlaceRequest::Structure { id, pos }, DimensionKind::Overworld) => {
                self.state.overworld.place_structure(dimension, id, *pos)
            }
            (PlaceRequest::Structure { id, pos }, DimensionKind::Nether) => {
                self.state.nether.place_structure(dimension, id, *pos)
            }
            _ => crate::placing::standalone(dimension, request),
        }
    }

    fn nearest_stronghold(&self, dimension: &Dimension, from: Vec3) -> Option<Vec3> {
        match DimensionKind::of(dimension) {
            DimensionKind::Overworld => self.state.overworld.nearest_stronghold(from),
            DimensionKind::Nether | DimensionKind::End => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use rspigot_core::ports::{PlaceRequest, TemplateMirror, TemplateRotation};
    use rspigot_domain::BlockPos;

    use super::*;
    use crate::structures::BiomeProbe;

    fn generator() -> NoiseGenerator {
        NoiseGenerator::new(77)
    }

    #[test]
    fn locating_a_stronghold_returns_the_nearest_ring_chunk_corner() {
        let generator = generator();
        let dimension = Dimension::overworld();
        let (id, at) = generator
            .nearest_structure(&dimension, Vec3::ZERO, "minecraft:stronghold")
            .expect("a stronghold");
        assert_eq!(id, "minecraft:stronghold");
        assert_eq!((at.x as i32 % 16, at.z as i32 % 16), (0, 0));
        let overworld = &generator.state.overworld;
        let rings = overworld
            .engine()
            .stronghold_chunks_parallel(&|| overworld.biome_probe())
            .to_vec();
        let chunk = ((at.x as i32).div_euclid(16), (at.z as i32).div_euclid(16));
        assert!(rings.contains(&chunk));
        let distance = |(x, z): (i32, i32)| {
            i64::from(x * 16) * i64::from(x * 16) + i64::from(z * 16) * i64::from(z * 16)
        };
        assert!(
            rings
                .iter()
                .all(|&other| distance(chunk) <= distance(other))
        );
    }

    #[test]
    fn locating_a_random_spread_structure_agrees_with_the_placement_function() {
        let generator = generator();
        let dimension = Dimension::overworld();
        let (id, at) = generator
            .nearest_structure(&dimension, Vec3::ZERO, "minecraft:desert_pyramid")
            .expect("a desert pyramid");
        assert_eq!(id, "minecraft:desert_pyramid");
        let chunk = ((at.x as i32).div_euclid(16), (at.z as i32).div_euclid(16));
        assert_eq!(
            generator
                .state
                .overworld
                .engine()
                .potential_chunk("desert_pyramids", chunk.0, chunk.1),
            Some(chunk)
        );
        assert_eq!((at.x as i32 % 16, at.y, at.z as i32 % 16), (0, 0.0, 0));
    }

    #[test]
    fn a_structure_the_dimension_cannot_hold_is_not_found() {
        let generator = generator();
        let dimension = Dimension::overworld();
        assert_eq!(
            generator.nearest_structure(&dimension, Vec3::ZERO, "minecraft:fortress"),
            None
        );
        assert_eq!(
            generator.nearest_structure(&dimension, Vec3::ZERO, "minecraft:nonsense"),
            None
        );
    }

    #[test]
    fn the_closest_biome_is_sampled_at_the_returned_position() {
        let generator = generator();
        let dimension = Dimension::overworld();
        let wanted = vec!["minecraft:desert".to_owned(), "minecraft:plains".to_owned()];
        let (name, at) = generator
            .nearest_biome(&dimension, Vec3::new(0.0, 64.0, 0.0), &wanted)
            .expect("a biome");
        let overworld = &generator.state.overworld;
        let probe = overworld.biome_probe();
        let index = probe.at((at.x as i32) >> 2, (at.y as i32) >> 2, (at.z as i32) >> 2);
        assert_eq!(
            rspigot_data::multi_noise::overworld_biomes()[usize::from(index)],
            name
        );
        assert_eq!(
            at.x as i32 % 32 + at.z as i32 % 32,
            0,
            "samples lie on the 32 block lattice"
        );
    }

    #[test]
    fn placing_a_template_writes_its_blocks_at_the_origin() {
        let generator = generator();
        let dimension = Dimension::overworld();
        let request = |rotation| PlaceRequest::Template {
            id: "minecraft:igloo/top".to_owned(),
            pos: BlockPos::new(100, 70, -40),
            rotation,
            mirror: TemplateMirror::None,
            integrity: 1.0,
            seed: 0,
        };
        let plain = generator
            .place(&dimension, &request(TemplateRotation::None))
            .expect("blocks");
        assert!(plain.len() > 20);
        assert!(
            plain
                .iter()
                .all(|block| block.pos.x >= 100 && block.pos.z <= -40 + 16)
        );
        let turned = generator
            .place(&dimension, &request(TemplateRotation::Clockwise90))
            .expect("blocks");
        assert_eq!(plain.len(), turned.len());
        assert_ne!(plain, turned);
        let missing = PlaceRequest::Template {
            id: "minecraft:nope".to_owned(),
            pos: BlockPos::new(0, 70, 0),
            rotation: TemplateRotation::None,
            mirror: TemplateMirror::None,
            integrity: 1.0,
            seed: 0,
        };
        assert_eq!(
            generator.place(&dimension, &missing),
            Err(rspigot_core::ports::PlaceFailure::Unknown)
        );
    }

    #[test]
    fn integrity_removes_blocks_deterministically() {
        let generator = generator();
        let dimension = Dimension::overworld();
        let request = |integrity| PlaceRequest::Template {
            id: "minecraft:igloo/top".to_owned(),
            pos: BlockPos::new(0, 70, 0),
            rotation: TemplateRotation::None,
            mirror: TemplateMirror::None,
            integrity,
            seed: 5,
        };
        let full = generator.place(&dimension, &request(1.0)).unwrap();
        let half = generator.place(&dimension, &request(0.5)).unwrap();
        assert!(half.len() < full.len());
        assert_eq!(generator.place(&dimension, &request(0.5)).unwrap(), half);
    }

    #[test]
    fn placing_a_jigsaw_assembles_pieces_from_the_pool() {
        let generator = generator();
        let dimension = Dimension::overworld();
        let request = PlaceRequest::Jigsaw {
            pool: "minecraft:village/plains/town_centers".to_owned(),
            target: "minecraft:empty".to_owned(),
            max_depth: 3,
            pos: BlockPos::new(0, 70, 0),
        };
        let blocks = generator.place(&dimension, &request).expect("blocks");
        assert!(blocks.len() > 100);
        let unknown = PlaceRequest::Jigsaw {
            pool: "minecraft:nope".to_owned(),
            target: "minecraft:empty".to_owned(),
            max_depth: 3,
            pos: BlockPos::new(0, 70, 0),
        };
        assert_eq!(
            generator.place(&dimension, &unknown),
            Err(rspigot_core::ports::PlaceFailure::Unknown)
        );
    }

    #[test]
    fn placing_a_structure_generates_its_start_in_the_chunk_of_the_position() {
        let generator = NoiseGenerator::new(1);
        let dimension = Dimension::overworld();
        let request = |id: &str, x, z| PlaceRequest::Structure {
            id: id.to_owned(),
            pos: BlockPos::new(x, 70, z),
        };
        let village = generator
            .place(
                &dimension,
                &request("minecraft:village_plains", 40 * 16, 51 * 16),
            )
            .expect("the village of the reference fixture");
        assert!(village.len() > 200);
        assert!(
            village
                .iter()
                .all(|block| dimension.height.contains(block.pos.y))
        );
        assert_eq!(
            generator.place(&dimension, &request("minecraft:stronghold", 0, 0)),
            Err(rspigot_core::ports::PlaceFailure::Failed)
        );
    }
}
