use rspigot_core::ports::ChunkGenerator;
use rspigot_domain::world::Dimension;
use rspigot_domain::{BiomeId, ChunkPos};

use super::WorldGenerator;

#[test]
fn the_debug_level_type_builds_the_debug_generator() {
    let generator = WorldGenerator::from_level_type("minecraft:debug_all_block_states", 1, "");
    assert!(matches!(generator, WorldGenerator::Debug(_)));
    let generator = WorldGenerator::from_level_type("DEBUG_ALL_BLOCK_STATES", 1, "");
    assert!(matches!(generator, WorldGenerator::Debug(_)));
}

#[test]
fn single_biome_surface_fills_the_overworld_with_one_biome_over_normal_terrain() {
    let generator = WorldGenerator::from_level_type("minecraft:single_biome_surface", 1, "");
    assert!(matches!(generator, WorldGenerator::Noise(_)));
    let dimension = Dimension::overworld();
    let desert = rspigot_data::registries::biome_id("minecraft:desert").expect("desert");
    let custom = WorldGenerator::from_level_type(
        "single_biome_surface",
        1,
        "{\"biome\":\"minecraft:desert\"}",
    );
    let plains = BiomeId(rspigot_data::registries::biome_id("minecraft:plains").expect("plains"));
    for (generator, expected) in [(generator, plains), (custom, BiomeId(desert))] {
        let column = generator.generate(&dimension, ChunkPos::new(7, -3));
        for section in column.sections() {
            for (x, y, z) in [(0, 0, 0), (7, 9, 3), (15, 15, 15)] {
                assert_eq!(section.biome(x, y, z), expected);
            }
        }
        let solid = column
            .sections()
            .iter()
            .filter(|section| !section.is_empty())
            .count();
        assert!(solid > 4, "terrain still generates ({solid} sections)");
    }
}

#[test]
fn the_generator_lists_templates_and_reads_their_size() {
    let generator = WorldGenerator::from_level_type("void", 1, "");
    let names = generator.template_names();
    assert!(names.iter().any(|name| name == "minecraft:igloo/top"));
    assert!(names.windows(2).all(|pair| pair[0] < pair[1]));
    let info = generator
        .template_info("minecraft:igloo/top")
        .expect("a template");
    assert!(info.size.iter().all(|side| *side > 0));
    assert!(info.blocks > 0);
    assert!(generator.template_info("minecraft:nope").is_none());
}
