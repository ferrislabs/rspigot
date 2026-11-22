use rspigot_blocks::BlockKind;
use rspigot_domain::world::WorldHeight;
use rspigot_domain::{BlockPos, ChunkPos};
use rspigot_nbt::{Compound, Tag};

use super::*;
use crate::chunk::from_nbt;
use crate::datafix::{DataKind, upgrade_checked};

fn name(value: &str) -> Tag {
    Tag::Compound(Compound::new().with("Name", value))
}

fn palette(names: &[&str]) -> Tag {
    Tag::List(names.iter().map(|value| name(value)).collect())
}

fn context(dimension: &str) -> Compound {
    Compound::new()
        .with("dimension", dimension)
        .with("generator", "minecraft:noise")
}

fn old_section(y: i8, names: &[&str], states: Vec<i64>) -> Tag {
    Tag::Compound(
        Compound::new()
            .with("Y", Tag::Byte(y))
            .with("Palette", palette(names))
            .with("BlockStates", Tag::LongArray(states))
            .with("SkyLight", Tag::ByteArray(vec![0xff; 2048])),
    )
}

fn lists(count: usize) -> Tag {
    Tag::List(vec![Tag::List(Vec::new()); count])
}

fn old_chunk(sections: Vec<Tag>, biomes: Vec<i32>, status: &str) -> Compound {
    let level = Compound::new()
        .with("xPos", 0)
        .with("zPos", 0)
        .with("LastUpdate", 5i64)
        .with("InhabitedTime", 7i64)
        .with("Status", status)
        .with("isLightOn", Tag::Byte(1))
        .with("Biomes", Tag::IntArray(biomes))
        .with(
            "Heightmaps",
            Compound::new().with("MOTION_BLOCKING", Tag::LongArray(vec![1 | (3 << 9)])),
        )
        .with("Sections", Tag::List(sections))
        .with("TileEntities", Tag::List(Vec::new()))
        .with("Entities", Tag::List(Vec::new()))
        .with("TileTicks", Tag::List(Vec::new()))
        .with("LiquidTicks", Tag::List(Vec::new()))
        .with("LiquidsToBeTicked", lists(16))
        .with("ToBeTicked", lists(16))
        .with("PostProcessing", lists(16))
        .with(
            "Structures",
            Compound::new()
                .with("Starts", Compound::new())
                .with("References", Compound::new()),
        );
    Compound::new()
        .with("DataVersion", 2586)
        .with("Level", level)
        .with("__context", context("minecraft:overworld"))
}

fn section_y(section: &Tag) -> i64 {
    section.as_compound().unwrap().get_i64("Y").unwrap()
}

fn sections_of(root: &Compound) -> &[Tag] {
    root.get_list("sections").unwrap()
}

fn section_at(root: &Compound, y: i64) -> &Compound {
    sections_of(root)
        .iter()
        .find(|section| section_y(section) == y)
        .and_then(Tag::as_compound)
        .unwrap()
}

fn stone_floor(y: i8) -> Tag {
    let mut states = vec![0i64; 256];
    states[0] = 1;
    old_section(y, &["minecraft:air", "minecraft:stone"], states)
}

fn bedrock_floor() -> Tag {
    old_section(0, &["minecraft:bedrock", "minecraft:air"], vec![0i64; 256])
}

fn plains(count: usize) -> Vec<i32> {
    vec![1; count]
}

#[test]
fn the_log_of_a_count_rounds_up_like_the_vanilla_helper() {
    assert_eq!(ceil_log2(0), 0);
    assert_eq!(ceil_log2(1), 0);
    assert_eq!(ceil_log2(2), 1);
    assert_eq!(ceil_log2(3), 2);
    assert_eq!(ceil_log2(5), 3);
    for power in 0..=12 {
        assert_eq!(ceil_log2(1usize << power), power);
    }
}

#[test]
fn a_uniform_biome_section_collapses_to_one_palette_entry() {
    let container = biome_container(|_| 1);
    assert_eq!(container.get_list("palette").unwrap().len(), 1);
    assert_eq!(container.get_str("data"), None);
    assert!(!container.contains_key("data"));
}

#[test]
fn mixed_biomes_are_packed_in_first_seen_order() {
    let container = biome_container(|cell| if cell < 63 { 2 } else { 1 });
    let palette: Vec<&str> = container
        .get_list("palette")
        .unwrap()
        .iter()
        .filter_map(Tag::as_str)
        .collect();
    assert_eq!(palette, ["minecraft:desert", "minecraft:plains"]);
    let data = container.get_long_array("data").unwrap();
    assert_eq!(data, [i64::MIN]);
}

#[test]
fn the_1_16_chunk_gets_24_sections_with_replicated_biome_layers() {
    let mut biomes = plains(1024);
    for slot in biomes.iter_mut().take(16) {
        *slot = 2;
    }
    for slot in biomes.iter_mut().skip(1008) {
        *slot = 4;
    }
    let mut root = old_chunk(vec![stone_floor(3)], biomes, "full");
    height_and_biome_2832(&mut root);
    let level = root.get_compound("Level").unwrap();
    let sections = level.get_list("Sections").unwrap();
    assert_eq!(sections.len(), 24);
    let ys: std::collections::BTreeSet<i64> = sections.iter().map(section_y).collect();
    assert_eq!(ys, (-4..=19).collect());
    let biomes_of = |y: i64| -> Vec<String> {
        sections
            .iter()
            .find(|section| section_y(section) == y)
            .and_then(Tag::as_compound)
            .and_then(|section| section.get_compound("biomes"))
            .unwrap()
            .get_list("palette")
            .unwrap()
            .iter()
            .filter_map(Tag::as_str)
            .map(str::to_owned)
            .collect()
    };
    assert_eq!(biomes_of(-4), ["minecraft:desert"]);
    assert_eq!(biomes_of(-1), ["minecraft:desert"]);
    assert_eq!(biomes_of(19), ["minecraft:forest"].map(str::to_owned));
    assert_eq!(biomes_of(8), ["minecraft:plains"]);
    assert!(!level.contains_key("Biomes"));
    let section = sections
        .iter()
        .find(|section| section_y(section) == 3)
        .and_then(Tag::as_compound)
        .unwrap();
    assert!(!section.contains_key("Palette"));
    assert!(!section.contains_key("BlockStates"));
    assert!(section.get_compound("block_states").is_some());
}

#[test]
fn old_block_state_arrays_keep_their_width_by_padding_the_palette() {
    let mut root = old_chunk(vec![stone_floor(3)], plains(1024), "full");
    height_and_biome_2832(&mut root);
    let level = root.get_compound("Level").unwrap();
    let states = level
        .get_list("Sections")
        .unwrap()
        .iter()
        .filter_map(Tag::as_compound)
        .find(|section| section.get_i64("Y") == Some(3))
        .and_then(|section| section.get_compound("block_states"))
        .unwrap();
    assert_eq!(states.get_list("palette").unwrap().len(), 9);
    assert_eq!(states.get_long_array("data").unwrap().len(), 256);
}

#[test]
fn heightmaps_gain_64_per_entry_and_carving_masks_move_up_four_sections() {
    let mut root = old_chunk(Vec::new(), plains(1024), "carvers");
    let level = compound_mut(&mut root, "Level").unwrap();
    level.insert(
        "CarvingMasks",
        Compound::new().with("AIR", Tag::ByteArray(vec![1, 0, 0, 0, 0, 0, 0, 0, 2])),
    );
    height_and_biome_2832(&mut root);
    let level = root.get_compound("Level").unwrap();
    let heightmap = level
        .get_compound("Heightmaps")
        .unwrap()
        .get_long_array("MOTION_BLOCKING")
        .unwrap()[0];
    assert_eq!(heightmap & 511, 65);
    assert_eq!((heightmap >> 9) & 511, 67);
    assert_eq!((heightmap >> 18) & 511, 0);
    let air = level
        .get_compound("CarvingMasks")
        .unwrap()
        .get_long_array("AIR")
        .unwrap();
    assert_eq!(air.len(), 64 * 24);
    assert_eq!(air[256], 1);
    assert_eq!(air[257], 2);
    assert_eq!(air[0], 0);
}

#[test]
fn tick_and_post_processing_lists_are_padded_to_24_and_upgrade_data_shifts() {
    let mut root = old_chunk(Vec::new(), plains(1024), "full");
    compound_mut(&mut root, "Level").unwrap().insert(
        "UpgradeData",
        Compound::new().with(
            "Indices",
            Compound::new()
                .with("0", Tag::IntArray(vec![1]))
                .with("15", Tag::IntArray(vec![2])),
        ),
    );
    height_and_biome_2832(&mut root);
    let level = root.get_compound("Level").unwrap();
    for key in ["LiquidsToBeTicked", "PostProcessing", "ToBeTicked"] {
        assert_eq!(level.get_list(key).unwrap().len(), 24, "{key}");
    }
    let indices = level
        .get_compound("UpgradeData")
        .unwrap()
        .get_compound("Indices")
        .unwrap();
    assert_eq!(indices.get_int_array("4"), Some(&[1][..]));
    assert_eq!(indices.get_int_array("19"), Some(&[2][..]));
    assert!(!indices.contains_key("0"));
}

#[test]
fn a_noise_chunk_with_bedrock_at_zero_becomes_a_below_zero_retrogen_chunk() {
    let mut root = old_chunk(vec![bedrock_floor()], plains(1024), "full");
    height_and_biome_2832(&mut root);
    let level = root.get_compound("Level").unwrap();
    assert_eq!(level.get_str("Status"), Some("empty"));
    let retrogen = level.get_compound("below_zero_retrogen").unwrap();
    assert_eq!(retrogen.get_str("target_status"), Some("heightmaps"));
    assert_eq!(retrogen.get_long_array("missing_bedrock"), Some(&[][..]));
    assert_eq!(
        level
            .get_compound("blending_data")
            .unwrap()
            .get_bool("old_noise"),
        Some(true)
    );
    assert_eq!(level.get_i64("isLightOn"), Some(0));
}

#[test]
fn a_chunk_without_bedrock_keeps_its_status_but_loses_its_light_flag() {
    let mut root = old_chunk(vec![stone_floor(3)], plains(1024), "full");
    height_and_biome_2832(&mut root);
    let level = root.get_compound("Level").unwrap();
    assert!(level.get_compound("below_zero_retrogen").is_none());
    assert_eq!(level.get_str("Status"), Some("full"));
}

#[test]
fn only_the_overworld_is_extended_downwards() {
    let mut root = old_chunk(vec![stone_floor(3)], plains(1024), "full");
    root.insert("__context", context("minecraft:the_nether"));
    height_and_biome_2832(&mut root);
    let level = root.get_compound("Level").unwrap();
    let sections = level.get_list("Sections").unwrap();
    assert_eq!(sections.len(), 16);
    assert_eq!(level.get_list("LiquidsToBeTicked").unwrap().len(), 16);
    assert!(level.get_compound("CarvingMasks").is_some());
    assert!(!level.contains_key("blending_data"));
}

#[test]
fn a_1536_biome_array_keeps_the_height_and_skips_the_rest() {
    let mut root = old_chunk(Vec::new(), plains(1536), "full");
    height_and_biome_2832(&mut root);
    let level = root.get_compound("Level").unwrap();
    assert_eq!(level.get_list("Sections").unwrap().len(), 24);
    assert_eq!(level.get_list("PostProcessing").unwrap().len(), 16);
    assert!(!level.contains_key("blending_data"));
}

#[test]
fn a_missing_biome_array_fills_plains() {
    let mut root = old_chunk(Vec::new(), plains(7), "full");
    height_and_biome_2832(&mut root);
    let level = root.get_compound("Level").unwrap();
    let section = level.get_list("Sections").unwrap()[0]
        .as_compound()
        .unwrap();
    let biomes = section.get_compound("biomes").unwrap();
    assert_eq!(biomes.get_list("palette").unwrap().len(), 1);
}

#[test]
fn the_status_is_advanced_when_the_blocks_prove_a_later_stage() {
    let mut names: HashSet<String> = ["minecraft:air", "minecraft:stone"]
        .map(str::to_owned)
        .into();
    assert_eq!(predicted_status("empty", &mut names), "noise");
    let mut names: HashSet<String> = ["minecraft:air", "minecraft:oak_log"]
        .map(str::to_owned)
        .into();
    assert_eq!(predicted_status("noise", &mut names), "liquid_carvers");
    let mut names: HashSet<String> = ["minecraft:air"].map(str::to_owned).into();
    assert_eq!(
        predicted_status("biomes", &mut names),
        "structure_references"
    );
    let mut names: HashSet<String> = ["minecraft:oak_log"].map(str::to_owned).into();
    assert_eq!(predicted_status("full", &mut names), "full");
}

#[test]
fn proto_chunk_ticks_are_rebuilt_from_the_to_be_ticked_lists() {
    let mut root = old_chunk(vec![stone_floor(3)], plains(1024), "carvers");
    let level = compound_mut(&mut root, "Level").unwrap();
    level.remove("TileTicks");
    level.remove("LiquidTicks");
    level.insert("xPos", 2);
    level.insert("zPos", -1);
    let mut to_be = vec![Tag::List(Vec::new()); 16];
    to_be[3] = Tag::List(vec![Tag::Short(0), Tag::Short(1 | (2 << 4) | (3 << 8))]);
    level.insert("ToBeTicked", Tag::List(to_be));
    level.insert("LiquidsToBeTicked", lists(16));
    height_and_biome_2832(&mut root);
    proto_ticks_2841(&mut root);
    let level = root.get_compound("Level").unwrap();
    assert!(!level.contains_key("ToBeTicked"));
    let ticks = level.get_list("TileTicks").unwrap();
    assert_eq!(ticks.len(), 1);
    let tick = ticks[0].as_compound().unwrap();
    assert_eq!(tick.get_str("i"), Some("minecraft:air"));
    assert_eq!(tick.get_i32("x"), Some(2 * 16 + 1));
    assert_eq!(tick.get_i32("y"), Some(3 * 16 + 2));
    assert_eq!(tick.get_i32("z"), Some(-16 + 3));
    assert_eq!(level.get_list("fluid_ticks").unwrap().len(), 0);
}

#[test]
fn full_chunks_only_rename_their_liquid_ticks() {
    let mut root = old_chunk(Vec::new(), plains(1024), "full");
    compound_mut(&mut root, "Level").unwrap().insert(
        "LiquidTicks",
        Tag::List(vec![Tag::Compound(
            Compound::new().with("i", "minecraft:water"),
        )]),
    );
    proto_ticks_2841(&mut root);
    let level = root.get_compound("Level").unwrap();
    assert!(!level.contains_key("LiquidTicks"));
    assert_eq!(level.get_list("fluid_ticks").unwrap().len(), 1);
    assert!(level.contains_key("LiquidsToBeTicked"));
}

#[test]
fn the_level_wrapper_is_flattened_with_the_vanilla_renames() {
    let mut root = old_chunk(Vec::new(), plains(1024), "full");
    compound_mut(&mut root, "Level").unwrap().insert(
        "Structures",
        Compound::new()
            .with(
                "Starts",
                Compound::new().with("Village", Compound::new().with("id", "Village")),
            )
            .with("References", Compound::new()),
    );
    chunk_renames_2842(&mut root);
    assert!(!root.contains_key("Level"));
    for old in [
        "TileEntities",
        "TileTicks",
        "Entities",
        "Sections",
        "Structures",
    ] {
        assert!(!root.contains_key(old), "{old}");
    }
    for new in [
        "block_entities",
        "block_ticks",
        "entities",
        "sections",
        "structures",
    ] {
        assert!(root.contains_key(new), "{new}");
    }
    assert_eq!(root.get_i64("InhabitedTime"), Some(7));
    assert!(
        root.get_compound("structures")
            .unwrap()
            .contains_key("starts")
    );
    assert!(root.contains_key("LiquidTicks"));
}

#[test]
fn ticks_saved_in_a_neighbour_chunk_are_copied_to_the_upgrade_data() {
    let tick = |x: i32, z: i32| {
        Tag::Compound(
            Compound::new()
                .with("x", x)
                .with("z", z)
                .with("i", "minecraft:stone"),
        )
    };
    let mut root = Compound::new()
        .with("xPos", 0)
        .with("zPos", 0)
        .with(
            "block_ticks",
            Tag::List(vec![tick(3, 3), tick(20, 3), tick(40, 0)]),
        )
        .with("fluid_ticks", Tag::List(vec![tick(-1, -5)]));
    overreaching_ticks_2843(&mut root);
    let upgrade = root.get_compound("UpgradeData").unwrap();
    assert_eq!(upgrade.get_list("neighbor_block_ticks").unwrap().len(), 1);
    assert_eq!(upgrade.get_list("neighbor_fluid_ticks").unwrap().len(), 1);
    assert_eq!(root.get_list("block_ticks").unwrap().len(), 3);
}

#[test]
fn cauldrons_dirt_paths_and_biomes_are_renamed() {
    let section = Tag::Compound(Compound::new().with("Y", Tag::Byte(0)).with(
        "Palette",
        Tag::List(vec![
            Tag::Compound(
                Compound::new()
                    .with("Name", "minecraft:cauldron")
                    .with("Properties", Compound::new().with("level", "0")),
            ),
            Tag::Compound(
                Compound::new()
                    .with("Name", "minecraft:cauldron")
                    .with("Properties", Compound::new().with("level", "2")),
            ),
            name("minecraft:grass_path"),
        ]),
    ));
    let mut root = old_chunk(vec![section], plains(1024), "full");
    cauldron_2679(&mut root);
    dirt_path_blocks_2680(&mut root);
    let entries = root
        .get_compound("Level")
        .unwrap()
        .get_list("Sections")
        .unwrap()[0]
        .as_compound()
        .unwrap()
        .get_list("Palette")
        .unwrap();
    let first = entries[0].as_compound().unwrap();
    assert_eq!(first.get_str("Name"), Some("minecraft:cauldron"));
    assert!(!first.contains_key("Properties"));
    let second = entries[1].as_compound().unwrap();
    assert_eq!(second.get_str("Name"), Some("minecraft:water_cauldron"));
    assert_eq!(
        second.get_compound("Properties").unwrap().get_str("level"),
        Some("2")
    );
    assert_eq!(
        entries[2].as_compound().unwrap().get_str("Name"),
        Some("minecraft:dirt_path")
    );
}

#[test]
fn old_biome_names_follow_the_caves_and_cliffs_renames() {
    let mut biomes = vec![0; 64];
    biomes[0] = 3;
    biomes[1] = 12;
    biomes[2] = 1;
    let mut root = old_chunk(vec![stone_floor(3)], Vec::new(), "full");
    compound_mut(&mut root, "Level")
        .unwrap()
        .insert("Biomes", Tag::IntArray([biomes, vec![1; 960]].concat()));
    height_and_biome_2832(&mut root);
    caves_and_cliffs_biomes_2838(&mut root);
    let section = root
        .get_compound("Level")
        .unwrap()
        .get_list("Sections")
        .unwrap()
        .iter()
        .filter_map(Tag::as_compound)
        .find(|section| section.get_i64("Y") == Some(0))
        .unwrap();
    let palette: Vec<&str> = section
        .get_compound("biomes")
        .unwrap()
        .get_list("palette")
        .unwrap()
        .iter()
        .filter_map(Tag::as_str)
        .collect();
    assert!(
        palette.contains(&"minecraft:windswept_hills"),
        "{palette:?}"
    );
    assert!(palette.contains(&"minecraft:snowy_plains"), "{palette:?}");
    assert!(palette.contains(&"minecraft:plains"), "{palette:?}");
}

#[test]
fn deep_warm_ocean_becomes_warm_ocean() {
    let mut root = Compound::new().with(
        "sections",
        Tag::List(vec![Tag::Compound(Compound::new().with(
            "biomes",
            Compound::new().with(
                "palette",
                Tag::List(vec![Tag::String("minecraft:deep_warm_ocean".into())]),
            ),
        ))]),
    );
    deep_warm_ocean_2843(&mut root);
    let palette = root.get_list("sections").unwrap()[0]
        .as_compound()
        .unwrap()
        .get_compound("biomes")
        .unwrap()
        .get_list("palette")
        .unwrap();
    assert_eq!(palette[0].as_str(), Some("minecraft:warm_ocean"));
}

#[test]
fn spawner_data_is_wrapped_into_the_weighted_entry_format() {
    let spawner = Compound::new()
        .with("id", "minecraft:mob_spawner")
        .with("SpawnData", Compound::new().with("id", "minecraft:zombie"))
        .with(
            "SpawnPotentials",
            Tag::List(vec![Tag::Compound(Compound::new().with("Weight", 3).with(
                "Entity",
                Compound::new().with("id", "minecraft:skeleton"),
            ))]),
        );
    let mut root = old_chunk(Vec::new(), plains(1024), "full");
    compound_mut(&mut root, "Level")
        .unwrap()
        .insert("TileEntities", Tag::List(vec![Tag::Compound(spawner)]));
    spawner_data_2831(&mut root);
    let spawner = root
        .get_compound("Level")
        .unwrap()
        .get_list("TileEntities")
        .unwrap()[0]
        .as_compound()
        .unwrap();
    assert_eq!(
        spawner
            .get_compound("SpawnData")
            .unwrap()
            .get_compound("entity")
            .unwrap()
            .get_str("id"),
        Some("minecraft:zombie")
    );
    let entry = spawner.get_list("SpawnPotentials").unwrap()[0]
        .as_compound()
        .unwrap();
    assert_eq!(entry.get_i32("weight"), Some(3));
    assert!(!entry.contains_key("Weight"));
    assert!(!entry.contains_key("Entity"));
    assert_eq!(
        entry
            .get_compound("data")
            .unwrap()
            .get_compound("entity")
            .unwrap()
            .get_str("id"),
        Some("minecraft:skeleton")
    );
}

#[test]
fn a_synthetic_1_16_chunk_loads_through_the_whole_registry() {
    let spawner = Compound::new()
        .with("id", "minecraft:mob_spawner")
        .with("x", 4)
        .with("y", 60)
        .with("z", 5)
        .with("SpawnData", Compound::new().with("id", "minecraft:zombie"));
    let mut root = old_chunk(vec![bedrock_floor(), stone_floor(3)], plains(1024), "full");
    let level = compound_mut(&mut root, "Level").unwrap();
    level.insert("TileEntities", Tag::List(vec![Tag::Compound(spawner)]));
    level.insert(
        "Structures",
        Compound::new()
            .with(
                "Starts",
                Compound::new().with("Village", Compound::new().with("id", "Village")),
            )
            .with("References", Compound::new()),
    );
    let upgraded = upgrade_checked(DataKind::Chunk, root.clone()).unwrap();
    assert_eq!(upgraded.from_version, 2586);
    let fixed = &upgraded.compound;
    assert!(!fixed.contains_key("Level"));
    assert_eq!(sections_of(fixed).len(), 24);
    assert!(section_at(fixed, 3).get_compound("block_states").is_some());
    let starts = fixed
        .get_compound("structures")
        .unwrap()
        .get_compound("starts")
        .unwrap();
    assert!(starts.contains_key("minecraft:village_plains"));
    let loaded = from_nbt(root, ChunkPos::new(0, 0), Some(WorldHeight::OVERWORLD))
        .unwrap()
        .unwrap();
    assert_eq!(
        loaded.block(BlockPos::new(0, 48, 0)),
        Some(BlockKind::Stone.default_state())
    );
    assert_eq!(
        loaded.block(BlockPos::new(3, 0, 3)),
        Some(BlockKind::Bedrock.default_state())
    );
    assert!(loaded.block(BlockPos::new(0, 49, 0)).unwrap().is_air());
    assert_eq!(loaded.status, rspigot_domain::chunk::ChunkStatus::Full);
    let spawner = loaded
        .block_entities
        .iter()
        .find(|entity| entity.kind.to_string() == "minecraft:mob_spawner")
        .unwrap();
    assert!(spawner.data.get_compound("SpawnData").is_some());
}

#[test]
fn a_1_16_nether_chunk_loads_at_its_own_height() {
    let mut root = old_chunk(vec![stone_floor(3)], plains(1024), "full");
    root.insert("__context", context("minecraft:the_nether"));
    let nether = WorldHeight::NETHER;
    let loaded = from_nbt(root, ChunkPos::new(0, 0), Some(nether))
        .unwrap()
        .unwrap();
    assert_eq!(
        loaded.block(BlockPos::new(0, 48, 0)),
        Some(BlockKind::Stone.default_state())
    );
}
