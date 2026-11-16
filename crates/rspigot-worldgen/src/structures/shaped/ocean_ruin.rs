use rspigot_blocks::BlockKind as Kind;
use rspigot_domain::BlockStateId;
use serde_json::json;

use super::frame::{plain, with};
use super::start;
use super::tpl::{Archaeology, Mirror, Tpl, transform};
use crate::legacy::JavaRandom;
use crate::overworld::features::place::Run;
use crate::overworld::features::structure::Rotation;
use crate::overworld::region::Heightmap;
use crate::structures::bbox::{Bbox, Pos};
use crate::structures::jigsaw::{Ground, Heightmap as Map};
use crate::structures::library::Library;
use crate::structures::placement::BiomeProbe;
use crate::structures::processors::Step;

const WARM: [&str; 8] = [
    "minecraft:underwater_ruin/warm_1",
    "minecraft:underwater_ruin/warm_2",
    "minecraft:underwater_ruin/warm_3",
    "minecraft:underwater_ruin/warm_4",
    "minecraft:underwater_ruin/warm_5",
    "minecraft:underwater_ruin/warm_6",
    "minecraft:underwater_ruin/warm_7",
    "minecraft:underwater_ruin/warm_8",
];
const BRICK: [&str; 8] = [
    "minecraft:underwater_ruin/brick_1",
    "minecraft:underwater_ruin/brick_2",
    "minecraft:underwater_ruin/brick_3",
    "minecraft:underwater_ruin/brick_4",
    "minecraft:underwater_ruin/brick_5",
    "minecraft:underwater_ruin/brick_6",
    "minecraft:underwater_ruin/brick_7",
    "minecraft:underwater_ruin/brick_8",
];
const CRACKED: [&str; 8] = [
    "minecraft:underwater_ruin/cracked_1",
    "minecraft:underwater_ruin/cracked_2",
    "minecraft:underwater_ruin/cracked_3",
    "minecraft:underwater_ruin/cracked_4",
    "minecraft:underwater_ruin/cracked_5",
    "minecraft:underwater_ruin/cracked_6",
    "minecraft:underwater_ruin/cracked_7",
    "minecraft:underwater_ruin/cracked_8",
];
const MOSSY: [&str; 8] = [
    "minecraft:underwater_ruin/mossy_1",
    "minecraft:underwater_ruin/mossy_2",
    "minecraft:underwater_ruin/mossy_3",
    "minecraft:underwater_ruin/mossy_4",
    "minecraft:underwater_ruin/mossy_5",
    "minecraft:underwater_ruin/mossy_6",
    "minecraft:underwater_ruin/mossy_7",
    "minecraft:underwater_ruin/mossy_8",
];
const BIG_WARM: [&str; 4] = [
    "minecraft:underwater_ruin/big_warm_4",
    "minecraft:underwater_ruin/big_warm_5",
    "minecraft:underwater_ruin/big_warm_6",
    "minecraft:underwater_ruin/big_warm_7",
];
const BIG_BRICK: [&str; 4] = [
    "minecraft:underwater_ruin/big_brick_1",
    "minecraft:underwater_ruin/big_brick_2",
    "minecraft:underwater_ruin/big_brick_3",
    "minecraft:underwater_ruin/big_brick_8",
];
const BIG_MOSSY: [&str; 4] = [
    "minecraft:underwater_ruin/big_mossy_1",
    "minecraft:underwater_ruin/big_mossy_2",
    "minecraft:underwater_ruin/big_mossy_3",
    "minecraft:underwater_ruin/big_mossy_8",
];
const BIG_CRACKED: [&str; 4] = [
    "minecraft:underwater_ruin/big_cracked_1",
    "minecraft:underwater_ruin/big_cracked_2",
    "minecraft:underwater_ruin/big_cracked_3",
    "minecraft:underwater_ruin/big_cracked_8",
];

#[derive(Debug, Clone, Copy)]
pub(crate) struct Params {
    pub(crate) warm: bool,
    pub(crate) large: f32,
    pub(crate) cluster: f32,
}

#[derive(Debug)]
pub(crate) struct Piece {
    pub(crate) tpl: Tpl,
    pub(crate) bbox: Bbox,
    pub(crate) integrity: f32,
    pub(crate) warm: bool,
    pub(crate) large: bool,
}

struct Builder<'a> {
    library: &'static Library,
    params: Params,
    pieces: &'a mut Vec<Piece>,
}

impl Builder<'_> {
    fn add(
        &mut self,
        name: &'static str,
        at: Pos,
        rotation: Rotation,
        integrity: f32,
        large: bool,
    ) -> Option<()> {
        let tpl = Tpl::new(self.library, name, at, rotation, Mirror::None, (0, 0))?;
        let bbox = tpl.bbox();
        self.pieces.push(Piece {
            tpl,
            bbox,
            integrity,
            warm: self.params.warm,
            large,
        });
        Some(())
    }

    fn piece(
        &mut self,
        random: &mut JavaRandom,
        at: Pos,
        rotation: Rotation,
        large: bool,
        base: f32,
    ) -> Option<()> {
        if self.params.warm {
            let name = if large {
                BIG_WARM[random.next_int(4) as usize]
            } else {
                WARM[random.next_int(8) as usize]
            };
            return self.add(name, at, rotation, base, large);
        }
        let (bricks, cracked, mossy): (&[&'static str], &[&'static str], &[&'static str]) = if large
        {
            (&BIG_BRICK, &BIG_CRACKED, &BIG_MOSSY)
        } else {
            (&BRICK, &CRACKED, &MOSSY)
        };
        let index = random.next_int(bricks.len() as i32) as usize;
        self.add(bricks[index], at, rotation, base, large)?;
        self.add(cracked[index], at, rotation, 0.7, large)?;
        self.add(mossy[index], at, rotation, 0.5, large)
    }
}

fn between(random: &mut JavaRandom, low: i32, high: i32) -> i32 {
    if low >= high {
        low
    } else {
        random.next_int(high - low + 1) + low
    }
}

fn positions(random: &mut JavaRandom, origin: Pos) -> Vec<Pos> {
    const SPEC: [((i32, i32), (i32, i32), (i32, i32)); 8] = [
        ((1, 8), (1, 7), (-16, 16)),
        ((1, 8), (1, 7), (-16, 0)),
        ((1, 8), (4, 8), (-16, -16)),
        ((1, 7), (1, 7), (0, 16)),
        ((1, 7), (4, 6), (0, -16)),
        ((1, 7), (3, 8), (16, 16)),
        ((1, 7), (1, 7), (16, 0)),
        ((1, 7), (4, 8), (16, -16)),
    ];
    SPEC.iter()
        .map(|&((xl, xh), (zl, zh), (dx, dz))| {
            let x = between(random, xl, xh);
            let z = between(random, zl, zh);
            (origin.0 + x + dx, origin.1, origin.2 + z + dz)
        })
        .collect()
}

fn span(at: Pos, size: Pos, rotation: Rotation) -> Bbox {
    let far = transform(size, Mirror::None, rotation, (0, 0));
    Bbox::from_corners(at, (far.0 + at.0, far.1 + at.1, far.2 + at.2))
}

fn cluster(
    builder: &mut Builder,
    random: &mut JavaRandom,
    rotation: Rotation,
    at: Pos,
) -> Option<()> {
    let base = (at.0, 90, at.2);
    let area = span(base, (15, 0, 15), rotation);
    let corner = transform((15, 0, 15), Mirror::None, rotation, (0, 0));
    let corner = (corner.0 + base.0, corner.1 + base.1, corner.2 + base.2);
    let origin = (base.0.min(corner.0), base.1, base.2.min(corner.2));
    let mut spots = positions(random, origin);
    let count = between(random, 4, 8);
    for _ in 0..count {
        if spots.is_empty() {
            continue;
        }
        let index = random.next_int(spots.len() as i32) as usize;
        let spot = spots.remove(index);
        let turn = Rotation::ALL[random.next_int(4) as usize];
        if !span(spot, (5, 0, 6), turn).intersects(&area) {
            builder.piece(random, spot, turn, false, 0.8)?;
        }
    }
    Some(())
}

pub(crate) fn generate<G: Ground>(
    params: Params,
    library: &'static Library,
    ground: &G,
    valid: &[u8],
    probe: &impl BiomeProbe,
    seed: i64,
    chunk: (i32, i32),
) -> Option<Vec<Piece>> {
    let mut random = start::random(seed, chunk);
    start::on_top(ground, valid, probe, Map::OceanFloorWg, chunk)?;
    let at = (chunk.0 * 16, 90, chunk.1 * 16);
    let rotation = Rotation::ALL[random.next_int(4) as usize];
    let mut pieces = Vec::new();
    let mut builder = Builder {
        library,
        params,
        pieces: &mut pieces,
    };
    let large = random.next_float() <= params.large;
    let base = if large { 0.9 } else { 0.8 };
    builder.piece(&mut random, at, rotation, large, base)?;
    if large && random.next_float() <= params.cluster {
        cluster(&mut builder, &mut random, rotation, at)?;
    }
    Some(pieces)
}

fn is_water(state: BlockStateId) -> bool {
    state.kind() == Some(Kind::Water)
        || state.property("waterlogged") == Some("true")
        || matches!(
            state.kind(),
            Some(
                Kind::Seagrass
                    | Kind::TallSeagrass
                    | Kind::Kelp
                    | Kind::KelpPlant
                    | Kind::BubbleColumn
            )
        )
}

fn is_ice(state: BlockStateId) -> bool {
    matches!(
        state.kind(),
        Some(Kind::Ice | Kind::PackedIce | Kind::BlueIce | Kind::FrostedIce)
    )
}

impl Piece {
    pub(crate) fn id(&self) -> &'static str {
        "ORP"
    }

    fn surface(&self, run: &mut Run, at: Pos, corner: Pos) -> i32 {
        let min_y = run.region.height().min_y;
        let mut lowest = 512;
        let mut count = 0;
        let below = at.1 - 1;
        for x in at.0.min(corner.0)..=at.0.max(corner.0) {
            for z in at.2.min(corner.2)..=at.2.max(corner.2) {
                let mut y = at.1 - 1;
                loop {
                    let state = run.region.get(x, y, z);
                    if (state.is_air() || is_water(state) || is_ice(state)) && y > min_y + 1 {
                        y -= 1;
                    } else {
                        break;
                    }
                }
                lowest = lowest.min(y);
                if y < below - 2 {
                    count += 1;
                }
            }
        }
        let width = (at.0 - corner.0).abs();
        if below - lowest > 2 && width - 2 < count {
            lowest + 1
        } else {
            at.1
        }
    }

    pub(crate) fn place(&self, run: &mut Run, write: &Bbox) {
        let at = self.tpl.pos;
        let floor = run.region.height_at(Heightmap::OceanFloor, at.0, at.2);
        let size = self.tpl.size();
        let far = transform(
            (size.0 - 1, 0, size.2 - 1),
            Mirror::None,
            self.tpl.rotation,
            (0, 0),
        );
        let corner = (far.0 + at.0, floor + far.1, far.2 + at.2);
        let y = self.surface(run, (at.0, floor, at.2), corner);
        let tpl = self.tpl.at((at.0, y, at.2));
        let (input, output, table) = if self.warm {
            (
                Kind::Sand,
                Kind::SuspiciousSand,
                "minecraft:archaeology/ocean_ruin_warm",
            )
        } else {
            (
                Kind::Gravel,
                Kind::SuspiciousGravel,
                "minecraft:archaeology/ocean_ruin_cold",
            )
        };
        let steps = [
            Step::BlockRot {
                integrity: self.integrity,
                rottable: None,
            },
            Step::IgnoreAir,
        ];
        let archaeology = Archaeology {
            input,
            output,
            table,
            limit: 5,
        };
        let markers = tpl.place_with(run, write, &steps, true, Some(&archaeology));
        for marker in markers {
            let at = marker.at;
            match marker.meta.as_str() {
                "chest" => {
                    let wet = is_water(run.region.get(at.0, at.1, at.2));
                    let chest = with(
                        Kind::Chest,
                        &[("waterlogged", if wet { "true" } else { "false" })],
                    );
                    run.region.set(at.0, at.1, at.2, chest);
                    let seed = run.rng.next_long() as i64;
                    let table = if self.large {
                        "minecraft:chests/underwater_ruin_big"
                    } else {
                        "minecraft:chests/underwater_ruin_small"
                    };
                    run.region.block_entities.retain(|(pos, _)| *pos != at);
                    run.region.block_entities.push((
                        at,
                        json!({ "id": "minecraft:chest", "LootTable": table, "LootTableSeed": { "$l": seed } }),
                    ));
                }
                "drowned" => {
                    run.region.entities.push(json!({
                        "id": "minecraft:drowned",
                        "Pos": [f64::from(at.0) + 0.5, f64::from(at.1), f64::from(at.2) + 0.5],
                        "Motion": [0.0, 0.0, 0.0],
                        "Rotation": [0.0f32, 0.0f32],
                        "PersistenceRequired": { "$b": 1 },
                    }));
                    let fill = if at.1 > run.region.sea_level() {
                        BlockStateId::AIR
                    } else {
                        plain(Kind::Water)
                    };
                    run.region.set(at.0, at.1, at.2, fill);
                }
                _ => {}
            }
        }
    }
}
