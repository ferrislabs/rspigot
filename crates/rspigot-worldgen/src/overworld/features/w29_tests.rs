use rspigot_blocks::BlockKind;
use serde_json::json;

use super::prov::Pred;
use super::tests::Bench;
use crate::resources::BlockKind as Slot;

fn count(region: &mut super::super::region::Region, kind: BlockKind) -> usize {
    let mut found = 0;
    for x in -8..24 {
        for z in -8..24 {
            for y in 60..140 {
                if region.get(x, y, z).kind() == Some(kind) {
                    found += 1;
                }
            }
        }
    }
    found
}

#[test]
fn a_basalt_pillar_hangs_down_to_the_ground_and_caps_itself() {
    let bench = Bench::new();
    let mut region = bench.floor(Slot::Stone, 64);
    assert!(bench.place(&mut region, "basalt_pillar", (8, 100, 8), 5));
    for y in 65..=100 {
        assert_eq!(
            region.get(8, y, 8).kind(),
            Some(BlockKind::Basalt),
            "y = {y}"
        );
    }
    assert!(count(&mut region, BlockKind::Basalt) > 36);
}

#[test]
fn basalt_columns_grow_in_a_cluster() {
    let bench = Bench::new();
    let mut region = bench.floor(Slot::Stone, 64);
    let mut total = 0;
    for seed in 1..=6 {
        total += usize::from(bench.place(&mut region, "large_basalt_columns", (8, 65, 8), seed));
    }
    assert!(total > 0);
    let basalt = count(&mut region, BlockKind::Basalt);
    assert!(basalt > 30, "only {basalt} basalt blocks");
    let tallest = (65..140)
        .filter(|y| {
            (-8..24)
                .any(|x| (-8..24).any(|z| region.get(x, *y, z).kind() == Some(BlockKind::Basalt)))
        })
        .max()
        .unwrap_or(0);
    assert!(tallest >= 69, "tallest column reaches {tallest}");
}

#[test]
fn the_height_range_block_predicate_is_relative_to_the_sea_level() {
    let bench = Bench::new();
    let mut region = bench.floor(Slot::Stone, 64);
    let predicate = Pred::compile(&json!({
        "type": "minecraft:height_range",
        "min_inclusive": { "above_bottom": 0 },
        "max_inclusive": { "relative_to_sea_level": 0 }
    }));
    assert!(predicate.test(&mut region, (0, -64, 0)));
    assert!(predicate.test(&mut region, (0, 63, 0)));
    assert!(!predicate.test(&mut region, (0, 64, 0)));
    assert!(!predicate.test(&mut region, (0, 200, 0)));
}

#[test]
fn a_template_rule_with_append_loot_writes_the_loot_table() {
    let bench = Bench::new();
    let mut region = bench.floor(Slot::Stone, 64);
    let config = json!({
        "type": "minecraft:template",
        "processors": { "processors": [{
            "processor_type": "minecraft:rule",
            "rules": [{
                "block_entity_modifier": {
                    "type": "minecraft:append_loot",
                    "loot_table": "minecraft:archaeology/desert_well"
                },
                "input_predicate": {
                    "block": "minecraft:suspicious_sand",
                    "predicate_type": "minecraft:block_match"
                },
                "location_predicate": { "predicate_type": "minecraft:always_true" },
                "output_state": "minecraft:suspicious_sand"
            }]
        }]},
        "templates": [{ "data": { "id": "minecraft:desert_well/suspicious_sand" }, "weight": 1 }]
    });
    assert!(bench.place_config(&mut region, &config, (8, 70, 8), 3));
    assert!(!region.block_entities.is_empty());
    for (_, nbt) in &region.block_entities {
        assert_eq!(nbt["LootTable"], "minecraft:archaeology/desert_well");
        assert!(nbt["LootTableSeed"]["$l"].is_i64());
    }
}

#[test]
fn fill_layer_fills_the_air_of_one_layer_above_the_world_floor() {
    let bench = Bench::new();
    let mut region = bench.floor(Slot::Stone, 64);
    let config = json!({
        "type": "minecraft:fill_layer",
        "height": 130,
        "state": { "Name": "minecraft:glass" }
    });
    assert!(bench.place_config(&mut region, &config, (0, 0, 0), 1));
    assert_eq!(region.get(5, 66, 5).kind(), Some(BlockKind::Glass));
    assert_eq!(region.get(15, 66, 15).kind(), Some(BlockKind::Glass));
    assert!(region.get(5, 67, 5).is_air());
    assert_eq!(region.get(5, 64, 5).kind(), Some(BlockKind::Stone));
}

#[test]
fn replace_single_block_swaps_the_first_matching_target() {
    let bench = Bench::new();
    let mut region = bench.floor(Slot::Stone, 64);
    let config = json!({
        "type": "minecraft:replace_single_block",
        "targets": [{
            "target": {
                "predicate_type": "minecraft:block_match",
                "block": "minecraft:stone"
            },
            "state": { "Name": "minecraft:diamond_block" }
        }]
    });
    assert!(bench.place_config(&mut region, &config, (3, 50, 3), 1));
    assert_eq!(region.get(3, 50, 3).kind(), Some(BlockKind::DiamondBlock));
    assert_eq!(region.get(4, 50, 3).kind(), Some(BlockKind::Stone));
    assert!(!bench.place_config(&mut region, &config, (3, 80, 3), 1));
}
