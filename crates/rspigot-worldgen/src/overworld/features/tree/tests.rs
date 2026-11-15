use super::*;
use crate::overworld::features::tests::Bench;
use crate::overworld::region::Region;
use crate::resources::BlockKind;
use rspigot_blocks::BlockKind as Kind;

fn cells(region: &mut Region) -> Vec<(Pos, BlockStateId)> {
    let mut states = Vec::new();
    for x in -24..40 {
        for z in -24..40 {
            for y in 40..130 {
                states.push(((x, y, z), region.get(x, y, z)));
            }
        }
    }
    states
}

fn describe(state: BlockStateId) -> String {
    let mut text = state
        .kind()
        .map_or("air", Kind::name)
        .trim_start_matches("minecraft:")
        .to_owned();
    for key in [
        "age",
        "axis",
        "creaking_heart_state",
        "distance",
        "facing",
        "hanging",
        "natural",
        "persistent",
        "segment_amount",
        "tip",
        "waterlogged",
    ] {
        if let Some(value) = state.property(key) {
            text.push_str(&format!(" {key}={value}"));
        }
    }
    text
}

fn fingerprint(bench: &Bench, feature: &str, seed: i64) -> (usize, u64) {
    let mut region = bench.floor(BlockKind::GrassBlock, 69);
    let before = cells(&mut region);
    bench.place(&mut region, feature, (8, 70, 8), seed);
    let after = cells(&mut region);
    let mut lines: Vec<String> = before
        .iter()
        .zip(&after)
        .filter(|(old, new)| old.1 != new.1)
        .map(|(_, new)| {
            let (x, y, z) = new.0;
            format!("{x},{y},{z} {}", describe(new.1))
        })
        .collect();
    lines.sort();
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for line in &lines {
        for byte in line.bytes().chain(std::iter::once(b'\n')) {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0100_0000_01b3);
        }
    }
    (lines.len(), hash)
}

const REFERENCE: [(&str, i64, usize, u64); 42] = [
    ("acacia", 1, 92, 0xc2385bb24eb84a74),
    ("acacia", 2, 94, 0x1f983ae25b632b84),
    ("birch", 1, 60, 0xce6936f52009cf57),
    ("birch", 2, 64, 0x2db97713236867a9),
    ("birch_bees_0002", 1, 60, 0xce6936f52009cf57),
    ("birch_bees_0002", 2, 64, 0x2db97713236867a9),
    ("birch_leaf_litter", 1, 89, 0xf246f39cfa6250dd),
    ("birch_leaf_litter", 2, 87, 0x466d5c00851c862f),
    ("dark_oak", 1, 180, 0xd52d87f162cd438b),
    ("dark_oak", 2, 157, 0x7e973d7ad7ebcc20),
    ("dark_oak_leaf_litter", 1, 205, 0xbe4d6d97aae48738),
    ("dark_oak_leaf_litter", 2, 184, 0x1f9e5cb4deb4f5ae),
    ("fancy_oak", 1, 78, 0x134e2d7d2febc3f0),
    ("fancy_oak", 2, 162, 0x83e174d93b34b6d7),
    ("fancy_oak_bees_005", 1, 78, 0x134e2d7d2febc3f0),
    ("fancy_oak_bees_005", 2, 162, 0x83e174d93b34b6d7),
    ("jungle_bush", 1, 30, 0x74e8dbd6eb649533),
    ("jungle_bush", 2, 31, 0xf21670d1fb309f56),
    ("jungle_tree", 1, 119, 0x850b4593e1296de9),
    ("jungle_tree", 2, 129, 0x11a339ae2cd4fcfd),
    ("jungle_tree_no_vine", 1, 61, 0xfa96cffa95cc0f8b),
    ("jungle_tree_no_vine", 2, 66, 0x5e53c6d9309e9d79),
    ("mega_jungle_tree", 1, 475, 0x1b724c207ec04cdf),
    ("mega_jungle_tree", 2, 471, 0x6595409aac66ec6c),
    ("mega_pine", 1, 243, 0x5b5314969fbd62ee),
    ("mega_pine", 2, 246, 0x6dfdbc278bfa1113),
    ("mega_spruce", 1, 463, 0x639111099d27bed7),
    ("mega_spruce", 2, 474, 0x104f27ab08986180),
    ("oak", 1, 59, 0xdce38fcff4c1916e),
    ("oak", 2, 63, 0xe2f7cbedb9e061b2),
    ("oak_bees_005", 1, 59, 0xdce38fcff4c1916e),
    ("oak_bees_005", 2, 63, 0xe2f7cbedb9e061b2),
    ("oak_leaf_litter", 1, 88, 0x945aad59729a1098),
    ("oak_leaf_litter", 2, 86, 0xed4648de52f48a40),
    ("pine", 1, 39, 0xcfdf72ad2d3333c7),
    ("pine", 2, 39, 0xcfdf72ad2d3333c7),
    ("spruce", 1, 60, 0x96ffaff31ca002e7),
    ("spruce", 2, 60, 0x96ffaff31ca002e7),
    ("super_birch_bees", 1, 62, 0x41640715ed8bdd5),
    ("super_birch_bees", 2, 65, 0x36b2df5cac9b73e4),
    ("swamp_oak", 1, 197, 0xcf4e2bd9c4d1f051),
    ("swamp_oak", 2, 205, 0xdc99b0626fca1648),
];

#[test]
fn trees_match_the_vanilla_feature_cell_for_cell() {
    let bench = Bench::new();
    for (feature, seed, count, hash) in REFERENCE {
        assert_eq!(
            fingerprint(&bench, feature, seed),
            (count, hash),
            "{feature} seed {seed}"
        );
    }
}
