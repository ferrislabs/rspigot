use super::ellipsoid::carve_ellipsoid;
use super::provider::{Anchor, FloatProvider, HeightProvider, IntProvider};
use super::*;
use crate::buffer::ChunkBuffer;
use crate::resources::{BlockKind, Palette};
use rspigot_domain::world::WorldHeight;

const OVERWORLD: Context = Context {
    min_y: -64,
    depth: 384,
};
const NETHER: Context = Context {
    min_y: 0,
    depth: 128,
};

fn cave(carvers: &Carvers, index: usize) -> &Cave {
    match carvers.get(index) {
        Some(Carver::Cave(cave)) => cave,
        other => panic!("expected a cave carver, got {other:?}"),
    }
}

#[test]
fn overworld_carvers_follow_the_vanilla_biome_list() {
    let carvers = Carvers::overworld();
    assert_eq!(carvers.len(), 3);
    let probabilities: Vec<f32> = (0..3)
        .map(|i| carvers.get(i).map(Carver::probability).unwrap_or_default())
        .collect();
    assert_eq!(probabilities, vec![0.15, 0.07, 0.01]);
    assert!(matches!(carvers.get(2), Some(Carver::Canyon(_))));
}

#[test]
fn cave_ranges_match_the_vanilla_data() {
    let carvers = Carvers::overworld();
    let main = cave(&carvers, 0);
    assert_eq!(
        main.y,
        HeightProvider::Uniform(Anchor::AboveBottom(8), Anchor::Absolute(180))
    );
    assert_eq!(main.count, IntProvider::VeryBiasedToBottom(0, 14));
    assert!(main.weird_thickness_bias);
    assert_eq!(main.floor_level, FloatProvider::Uniform(-1.0, -0.4));
    assert_eq!(main.start_vertical_radius, FloatProvider::Constant(1.0));
    let deep = cave(&carvers, 1);
    assert_eq!(
        deep.y,
        HeightProvider::Uniform(Anchor::AboveBottom(8), Anchor::Absolute(47))
    );
    match carvers.get(2) {
        Some(Carver::Canyon(canyon)) => {
            assert_eq!(
                canyon.y,
                HeightProvider::Uniform(Anchor::Absolute(10), Anchor::Absolute(67))
            );
            assert_eq!(canyon.width_smoothness, 3);
            assert_eq!(canyon.y_scale, FloatProvider::Constant(3.0));
        }
        other => panic!("expected a canyon, got {other:?}"),
    }
}

#[test]
fn nether_cave_matches_the_vanilla_data() {
    let carvers = Carvers::nether();
    assert_eq!(carvers.len(), 1);
    let nether = cave(&carvers, 0);
    assert_eq!(nether.probability, 0.2);
    assert_eq!(nether.count, IntProvider::VeryBiasedToBottom(0, 9));
    assert_eq!(nether.floor_level, FloatProvider::Constant(-0.7));
    assert_eq!(nether.start_vertical_radius, FloatProvider::Constant(5.0));
    assert_eq!(nether.room_vertical_radius, FloatProvider::Constant(0.5));
    assert_eq!(
        nether.y,
        HeightProvider::Uniform(Anchor::Absolute(0), Anchor::BelowTop(1))
    );
}

#[test]
fn anchors_resolve_against_the_generation_height() {
    assert_eq!(Anchor::AboveBottom(8).resolve(-64, 384), -56);
    assert_eq!(Anchor::BelowTop(1).resolve(0, 128), 126);
    assert_eq!(Anchor::Absolute(180).resolve(-64, 384), 180);
}

#[test]
fn masks_leave_the_bottom_layer_and_the_top_margin_alone() {
    let overworld = Carvers::overworld().mask(1, OVERWORLD, (0, 0));
    assert_eq!((overworld.min_y(), overworld.max_y()), (-63, 312));
    let nether = Carvers::nether().mask(1, NETHER, (0, 0));
    assert_eq!((nether.min_y(), nether.max_y()), (1, 120));
}

#[test]
fn trapezoid_floats_stay_in_range() {
    let provider = FloatProvider::Trapezoid {
        min: 0.0,
        max: 3.0,
        plateau: 1.0,
    };
    let mut random = crate::legacy::JavaRandom::new(7);
    for _ in 0..1000 {
        let value = provider.sample(&mut random);
        assert!((0.0..=3.0).contains(&value));
    }
}

#[test]
fn very_biased_counts_stay_in_range_and_favour_zero() {
    let provider = IntProvider::VeryBiasedToBottom(0, 14);
    let mut random = crate::legacy::JavaRandom::new(11);
    let samples: Vec<i32> = (0..2000).map(|_| provider.sample(&mut random)).collect();
    assert!(samples.iter().all(|n| (0..=14).contains(n)));
    let low = samples.iter().filter(|n| **n <= 2).count();
    assert!(low * 2 > samples.len());
}

#[test]
fn ellipsoid_carves_inside_and_not_outside() {
    let mut mask = Mask::new(-63, 312);
    carve_ellipsoid(
        (0, 0),
        (8.0, 0.0, 8.0),
        (3.0, 3.0),
        &mut mask,
        &|_, _, _, _| false,
    );
    assert!(mask.contains(8, 0, 8));
    assert!(!mask.contains(14, 0, 8));
    assert!(!mask.contains(8, 6, 8));
    assert!(!mask.contains(8, -6, 8));
}

#[test]
fn ellipsoid_skip_rule_keeps_the_floor() {
    let mut mask = Mask::new(-63, 312);
    let floor = |_: f64, yd: f64, _: f64, _: i32| yd <= -0.5;
    carve_ellipsoid((0, 0), (8.0, 0.0, 8.0), (4.0, 4.0), &mut mask, &floor);
    assert!(mask.contains(8, 1, 8));
    assert!(!mask.contains(8, -3, 8));
}

#[test]
fn same_seed_carves_the_same_cells() {
    let carvers = Carvers::overworld();
    for chunk in [(0, 0), (-5, 9)] {
        let first = carvers.mask(42, OVERWORLD, chunk);
        let second = carvers.mask(42, OVERWORLD, chunk);
        assert_eq!(first.len(), second.len());
        assert_eq!(first.segments(3, 4), second.segments(3, 4));
        for x in 0..16 {
            for z in 0..16 {
                assert_eq!(first.segments(x, z), second.segments(x, z));
            }
        }
    }
}

#[test]
fn different_seeds_carve_differently() {
    let carvers = Carvers::overworld();
    let total = |seed: i64| -> usize {
        (0..6)
            .map(|cx| carvers.mask(seed, OVERWORLD, (cx, 0)).len())
            .sum()
    };
    assert_ne!(total(1), total(2));
}

fn carved_below(carvers: &Carvers, context: Context, seed: i64, limit: i32) -> (usize, usize) {
    let mut chunks_with_air = 0;
    let mut cells = 0;
    for cx in 0..16 {
        for cz in 0..16 {
            let mask = carvers.mask(seed, context, (cx, cz));
            let mut here = 0;
            for x in 0..16 {
                for z in 0..16 {
                    for (low, high) in mask.segments(x, z) {
                        if low < limit {
                            here += (high.min(limit - 1) - low + 1) as usize;
                        }
                    }
                }
            }
            cells += here;
            chunks_with_air += usize::from(here > 0);
        }
    }
    (chunks_with_air, cells)
}

#[test]
fn a_chunk_neighbourhood_has_carved_air_below_sea_level() {
    let (chunks, cells) = carved_below(&Carvers::overworld(), OVERWORLD, 20_240_601, 60);
    assert!(cells > 1000, "only {cells} carved cells");
    assert!(chunks > 100, "only {chunks} of 256 chunks carved");
}

#[test]
fn canyons_alone_carve_a_connected_share_of_chunks() {
    let canyons = Carvers::from_ids(
        &["minecraft:canyon"],
        &crate::datapack::worldgen()["carvers"],
    )
    .unwrap_or_default();
    let (chunks, cells) = carved_below(&canyons, OVERWORLD, 99, 70);
    assert!(cells > 0);
    assert!(chunks > 0 && chunks < 256);
}

#[test]
fn nether_caves_carve_inside_the_noise_range() {
    let (chunks, cells) = carved_below(&Carvers::nether(), NETHER, 7, 121);
    assert!(cells > 1000, "only {cells} carved cells");
    assert!(chunks > 50);
}

#[test]
fn a_zero_probability_carver_never_starts() {
    let mut data = crate::datapack::worldgen()["carvers"]["minecraft:cave"].clone();
    data["probability"] = serde_json::json!(0.0);
    let carver = Carver::parse(&data).unwrap_or_else(|e| panic!("{e}"));
    let carvers = Carvers::from_list(vec![carver]);
    assert_eq!(carvers.mask(5, OVERWORLD, (0, 0)).len(), 0);
}

#[test]
fn unsupported_carver_types_are_rejected() {
    let error = Carver::parse(&serde_json::json!({"type": "minecraft:nether"}));
    assert!(error.is_err());
}

#[test]
fn applying_a_mask_spares_bedrock_and_restores_grass_over_dirt() {
    let palette = Palette::resolve();
    let height = WorldHeight::new(-64, 384).unwrap_or_else(|e| panic!("{e}"));
    let mut buffer = ChunkBuffer::new(height);
    let mut mask = Mask::new(-63, 312);
    for y in [-64, 62, 64] {
        buffer.set(1, y, 1, palette[BlockKind::Stone]);
    }
    buffer.set(1, -64, 1, palette[BlockKind::Bedrock]);
    buffer.set(1, 64, 1, palette[BlockKind::GrassBlock]);
    buffer.set(1, 63, 1, palette[BlockKind::Dirt]);
    mask.carve(1, 64, 1);
    mask.carve(1, 62, 1);
    super::apply(&mask, &mut buffer, (0, 0), |_, _, _| {
        Some(palette[BlockKind::Air])
    });
    assert!(buffer.get(1, 64, 1).is_air());
    assert_eq!(buffer.get(1, 63, 1), palette[BlockKind::GrassBlock]);
    assert!(buffer.get(1, 62, 1).is_air());
    assert_eq!(buffer.get(1, -64, 1), palette[BlockKind::Bedrock]);
}

#[test]
fn applying_keeps_blocks_the_substance_rule_refuses_to_replace() {
    let palette = Palette::resolve();
    let height = WorldHeight::new(-64, 384).unwrap_or_else(|e| panic!("{e}"));
    let mut buffer = ChunkBuffer::new(height);
    let mut mask = Mask::new(-63, 312);
    buffer.set(2, 10, 2, palette[BlockKind::Stone]);
    mask.carve(2, 10, 2);
    super::apply(&mask, &mut buffer, (0, 0), |_, _, _| None);
    assert_eq!(buffer.get(2, 10, 2), palette[BlockKind::Stone]);
}
