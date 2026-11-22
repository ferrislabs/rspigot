use serde_json::json;

use super::super::probe::{GRASS, world};
use super::super::tests::Bench;

fn pile() -> serde_json::Value {
    json!({
        "type": "minecraft:block_pile",
        "state_provider": {"type": "minecraft:simple_state_provider", "state": {"Name": "minecraft:hay_block"}},
    })
}

#[test]
fn a_block_pile_lays_hay_on_solid_ground_within_its_footprint() {
    let bench = Bench::new();
    let mut region = world(&bench, GRASS);
    assert!(bench.place_config(&mut region, &pile(), (8, 70, 8), 3));
    let mut placed = 0;
    for x in -4..20 {
        for y in 60..75 {
            for z in -4..20 {
                let state = region.get(x, y, z);
                if state.kind() == Some(rspigot_blocks::BlockKind::HayBlock) {
                    placed += 1;
                    assert!((70..=71).contains(&y), "{x},{y},{z}");
                    assert!(
                        (5..=11).contains(&x) && (5..=11).contains(&z),
                        "{x},{y},{z}"
                    );
                }
            }
        }
    }
    assert!(placed > 0);
}

#[test]
fn a_block_pile_needs_five_blocks_above_the_world_floor() {
    let bench = Bench::new();
    let mut region = world(&bench, GRASS);
    assert!(!bench.place_config(&mut region, &pile(), (8, -61, 8), 3));
}

#[test]
fn a_bonus_chest_is_placed_in_the_chunk_with_its_loot_table_and_torches() {
    let bench = Bench::new();
    let mut region = world(&bench, GRASS);
    let config = json!({"type": "minecraft:bonus_chest"});
    assert!(bench.place_config(&mut region, &config, (8, 70, 8), 5));
    let ((x, y, z), entity) = region.block_entities[0].clone();
    assert_eq!(entity["LootTable"], "minecraft:chests/spawn_bonus_chest");
    assert!((0..16).contains(&x) && (0..16).contains(&z));
    assert_eq!(y, 70);
    assert_eq!(
        region.get(x, y, z).kind(),
        Some(rspigot_blocks::BlockKind::Chest)
    );
    let torches = [(0, -1), (1, 0), (0, 1), (-1, 0)]
        .iter()
        .filter(|(dx, dz)| {
            region.get(x + dx, y, z + dz).kind() == Some(rspigot_blocks::BlockKind::Torch)
        })
        .count();
    assert!(torches >= 1);
}

#[test]
fn the_void_start_platform_is_a_stone_square_with_a_cobblestone_centre() {
    let bench = Bench::new();
    let mut region = world(&bench, "-60:stone");
    let config = json!({"type": "minecraft:void_start_platform"});
    assert!(bench.place_config(&mut region, &config, (8, 10, 8), 1));
    assert_eq!(
        region.get(8, 13, 8).kind(),
        Some(rspigot_blocks::BlockKind::Cobblestone)
    );
    for (x, z) in [(0, 0), (15, 15), (3, 12)] {
        assert_eq!(
            region.get(x, 13, z).kind(),
            Some(rspigot_blocks::BlockKind::Stone),
            "{x},{z}"
        );
    }
    assert!(region.get(8, 14, 8).is_air());
}

#[test]
fn the_void_start_platform_skips_chunks_away_from_the_origin() {
    let bench = Bench::new();
    let mut region = world(&bench, "-60:stone");
    let config = json!({"type": "minecraft:void_start_platform"});
    assert!(bench.place_config(&mut region, &config, (40, 10, 8), 1));
    assert!(region.get(40, 13, 8).is_air());
}

#[test]
fn vanilla_weighted_state_providers_drive_a_block_pile() {
    let bench = Bench::new();
    let mut region = world(&bench, GRASS);
    let config = json!({
        "type": "minecraft:block_pile",
        "state_provider": {
            "type": "minecraft:weighted_state_provider",
            "entries": [{"data": {"Name": "minecraft:hay_block"}, "weight": 1}],
        },
    });
    assert!(bench.place_config(&mut region, &config, (8, 70, 8), 3));
    let hay = (0..16)
        .flat_map(|x| (0..16).map(move |z| (x, z)))
        .filter(|(x, z)| region.get(*x, 70, *z).kind() == Some(rspigot_blocks::BlockKind::HayBlock))
        .count();
    assert!(hay > 0);
}
