use rspigot_blocks::BlockKind as Kind;
use rspigot_domain::BlockStateId;

use super::frame::{Frame, Guard, offset, plain};
use crate::overworld::features::place::Run;
use crate::overworld::region::Heightmap;
use crate::structures::bbox::{Bbox, Pos};
use crate::structures::blueprint::Dir;
use crate::structures::jigsaw::{Ground, Heightmap as Map};
use crate::structures::placement::BiomeProbe;

#[derive(Debug)]
pub(crate) struct Piece {
    pub(crate) bbox: Bbox,
}

pub(crate) fn generate<G: Ground>(
    ground: &G,
    valid: &[u8],
    probe: &impl BiomeProbe,
    chunk: (i32, i32),
) -> Option<Piece> {
    super::start::on_top(ground, valid, probe, Map::OceanFloorWg, chunk)?;
    let at = (chunk.0 * 16 + 9, 90, chunk.1 * 16 + 9);
    Some(Piece {
        bbox: Bbox { min: at, max: at },
    })
}

fn liquid(state: BlockStateId) -> bool {
    matches!(state.kind(), Some(Kind::Water | Kind::Lava))
}

fn anchor(state: BlockStateId) -> bool {
    matches!(
        state.kind(),
        Some(Kind::Sandstone | Kind::Stone | Kind::Andesite | Kind::Granite | Kind::Diorite)
    )
}

const DIRECTIONS: [Dir; 6] = [
    Dir::Down,
    Dir::Up,
    Dir::North,
    Dir::South,
    Dir::West,
    Dir::East,
];

impl Piece {
    pub(crate) fn id(&self) -> &'static str {
        "BTP"
    }

    pub(crate) fn place(&self, run: &mut Run, write: &Bbox) {
        let (x, z) = (self.bbox.min.0, self.bbox.min.2);
        let min_y = run.region.height().min_y;
        let mut at: Pos = (x, run.region.height_at(Heightmap::OceanFloor, x, z), z);
        let (state, below) = loop {
            if at.1 <= min_y {
                return;
            }
            let state = run.region.get(at.0, at.1, at.2);
            let below = run.region.get(at.0, at.1 - 1, at.2);
            if anchor(below) {
                break (state, below);
            }
            at.1 -= 1;
        };
        let state = if state.is_air() || liquid(state) {
            plain(Kind::Sand)
        } else {
            state
        };
        for dir in DIRECTIONS {
            let near = offset(at, dir);
            let current = run.region.get(near.0, near.1, near.2);
            if !(current.is_air() || liquid(current)) {
                continue;
            }
            let under = run.region.get(near.0, near.1 - 1, near.2);
            let solid = !under.is_air() && !liquid(under);
            let chosen = if solid || dir == Dir::Up {
                state
            } else {
                below
            };
            run.region.set(near.0, near.1, near.2, chosen);
        }
        let mut frame = Frame::new(run, self.bbox, None, *write, Guard::Open);
        frame.chest(at.0, at.1, at.2, "minecraft:chests/buried_treasure");
        frame.finish();
    }
}
