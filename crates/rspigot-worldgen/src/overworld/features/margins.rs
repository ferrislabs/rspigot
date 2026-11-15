use super::probe::world;
use super::tests::Bench;

const WORLDS: [&str; 3] = [
    "68:stone,69:grass_block",
    "40:stone,41:sand,66:water",
    "70:stone;0..15/60..66/0..15=air",
];
const SEEDS: i64 = 8;
const ORIGIN: (i32, i32, i32) = (8, 70, 8);

fn extent(bench: &Bench, name: &str, spec: &str, seed: i64) -> i32 {
    let mut region = world(bench, spec);
    region.changed.clear();
    bench.place(&mut region, name, ORIGIN, seed);
    region
        .changed
        .iter()
        .map(|(x, z)| (x - ORIGIN.0).abs().max((z - ORIGIN.2).abs()))
        .max()
        .unwrap_or(0)
}

#[test]
fn reach_covers_every_write_of_every_configured_feature() {
    let bench = Bench::new();
    let mut failures = Vec::new();
    for full in bench.configured_names() {
        let name = full.strip_prefix("minecraft:").unwrap_or(&full).to_owned();
        let reach = bench.reach_of(&full);
        for spec in WORLDS {
            for seed in 0..SEEDS {
                let wrote = extent(&bench, &name, spec, seed);
                if wrote > reach {
                    failures.push(format!("{name} seed {seed} wrote {wrote} > reach {reach}"));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
