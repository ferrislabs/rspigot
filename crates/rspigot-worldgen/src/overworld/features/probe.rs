use rspigot_domain::{BlockStateId, ChunkPos};

use super::tests::Bench;
use crate::overworld::region::Region;

pub(super) const GRASS: &str = "68:stone,69:grass_block";

fn block(name: &str) -> BlockStateId {
    rspigot_blocks::BlockKind::from_name(name)
        .map_or(BlockStateId::AIR, rspigot_blocks::BlockKind::default_state)
}

fn span(text: &str) -> std::ops::RangeInclusive<i32> {
    let (low, high) = text.split_once("..").unwrap_or((text, text));
    low.parse().unwrap_or(0)..=high.parse().unwrap_or(0)
}

pub(super) fn world<'a>(bench: &'a Bench, spec: &str) -> Region<'a> {
    let mut sections = spec.split(';');
    let layers: Vec<(i32, BlockStateId)> = sections
        .next()
        .unwrap_or("")
        .split(',')
        .filter_map(|part| {
            let (top, name) = part.split_once(':')?;
            Some((top.parse().ok()?, block(name)))
        })
        .collect();
    let mut region = Region::fixture(
        &bench.resources,
        &bench.dimension,
        ChunkPos::new(0, 0),
        0,
        |_, buffer| {
            let mut from = -64;
            for (top, state) in &layers {
                for y in from..=*top {
                    buffer.fill_layer(y, *state);
                }
                from = top + 1;
            }
        },
    );
    for fill in sections {
        let Some((axes, name)) = fill.split_once('=') else {
            continue;
        };
        let axes: Vec<&str> = axes.split('/').collect();
        let state = block(name);
        for x in span(axes[0]) {
            for y in span(axes[1]) {
                for z in span(axes[2]) {
                    region.set(x, y, z, state);
                }
            }
        }
    }
    region
}

fn describe(state: BlockStateId) -> String {
    let mut text = state
        .kind()
        .map_or("air", rspigot_blocks::BlockKind::name)
        .trim_start_matches("minecraft:")
        .to_owned();
    let mut properties: Vec<_> = state.properties().collect();
    properties.sort_unstable();
    for (name, value) in properties {
        text.push_str(&format!(" {name}={value}"));
    }
    text
}

fn cells(region: &mut Region, (y0, y1): (i32, i32)) -> Vec<BlockStateId> {
    let mut states = Vec::new();
    for x in -24..40 {
        for z in -24..40 {
            for y in y0..y1 {
                states.push(region.get(x, y, z));
            }
        }
    }
    states
}

fn changes(
    bench: &Bench,
    spec: &str,
    band: (i32, i32),
    act: impl FnOnce(&mut Region),
) -> Vec<String> {
    let mut region = world(bench, spec);
    let before = cells(&mut region, band);
    act(&mut region);
    let after = cells(&mut region, band);
    let mut lines = Vec::new();
    let mut index = 0;
    for x in -24..40 {
        for z in -24..40 {
            for y in band.0..band.1 {
                if before[index] != after[index] {
                    lines.push(format!("{x},{y},{z} {}", describe(after[index])));
                }
                index += 1;
            }
        }
    }
    lines.sort();
    lines
}

pub(super) fn lines(
    bench: &Bench,
    spec: &str,
    feature: &str,
    origin: (i32, i32, i32),
    seed: i64,
    band: (i32, i32),
) -> Vec<String> {
    changes(bench, spec, band, |region| {
        bench.place(region, feature, origin, seed);
    })
}

fn hashed(lines: &[String]) -> (usize, u64) {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for line in lines {
        for byte in line.bytes().chain(std::iter::once(b'\n')) {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0100_0000_01b3);
        }
    }
    (lines.len(), hash)
}

pub(super) fn scatter_fingerprint(
    bench: &Bench,
    spec: &str,
    feature: &str,
    area: [i32; 6],
    count: usize,
    seed: i64,
    band: (i32, i32),
) -> (usize, u64) {
    hashed(&changes(bench, spec, band, |region| {
        bench.scatter(region, feature, area, count, seed);
    }))
}

pub(super) fn fingerprint(
    bench: &Bench,
    spec: &str,
    feature: &str,
    origin: (i32, i32, i32),
    seed: i64,
    band: (i32, i32),
) -> (usize, u64) {
    hashed(&lines(bench, spec, feature, origin, seed, band))
}
