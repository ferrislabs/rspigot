use super::start;
use crate::overworld::features::place::Run;
use crate::overworld::region::Heightmap;
use crate::structures::bbox::Bbox;
use crate::structures::blueprint::Dir;
use crate::structures::jigsaw::{Ground, Heightmap as Map};
use crate::structures::placement::BiomeProbe;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Temple {
    Desert,
    Jungle,
    SwampHut,
}

impl Temple {
    pub(crate) fn size(self) -> (i32, i32, i32) {
        match self {
            Self::Desert => (21, 15, 21),
            Self::Jungle => (12, 10, 15),
            Self::SwampHut => (7, 7, 9),
        }
    }

    pub(crate) fn id(self) -> &'static str {
        match self {
            Self::Desert => "TeDP",
            Self::Jungle => "TeJP",
            Self::SwampHut => "TeSH",
        }
    }
}

#[derive(Debug)]
pub(crate) struct Piece {
    pub(crate) temple: Temple,
    pub(crate) bbox: Bbox,
    pub(crate) facing: Dir,
    pub(crate) drop: i32,
}

const HORIZONTAL: [Dir; 4] = [Dir::North, Dir::East, Dir::South, Dir::West];

pub(crate) fn make_bbox(at: (i32, i32, i32), facing: Dir, size: (i32, i32, i32)) -> Bbox {
    let (width, height, depth) = size;
    let (dx, dz) = if matches!(facing, Dir::North | Dir::South) {
        (width, depth)
    } else {
        (depth, width)
    };
    Bbox {
        min: at,
        max: (at.0 + dx - 1, at.1 + height - 1, at.2 + dz - 1),
    }
}

pub(crate) fn generate<G: Ground>(
    temple: Temple,
    ground: &G,
    valid: &[u8],
    probe: &impl BiomeProbe,
    seed: i64,
    chunk: (i32, i32),
) -> Option<Piece> {
    let mut random = start::random(seed, chunk);
    let (width, _, depth) = temple.size();
    if temple != Temple::SwampHut {
        let (x, z) = (chunk.0 * 16 + 8, chunk.1 * 16 + 8);
        if !start::could_exist(ground, valid, probe, x, z) {
            return None;
        }
        if start::lowest_y(ground, chunk.0 * 16, width, chunk.1 * 16, depth) < ground.sea_level() {
            return None;
        }
    }
    start::on_top(ground, valid, probe, Map::WorldSurfaceWg, chunk)?;
    let facing = HORIZONTAL[random.next_int(4) as usize];
    let at = (chunk.0 * 16, 64, chunk.1 * 16);
    Some(Piece {
        temple,
        bbox: make_bbox(at, facing, temple.size()),
        facing,
        drop: random.next_int(3),
    })
}

impl Piece {
    pub(crate) fn place(&self, run: &mut Run, write: &Bbox) {
        match self.temple {
            Temple::Desert => self.place_desert(run, write),
            Temple::Jungle | Temple::SwampHut => {}
        }
    }

    pub(crate) fn id(&self) -> &'static str {
        self.temple.id()
    }

    pub(crate) fn lowest(&self, run: &mut Run) -> i32 {
        let mut lowest = run.region.height().max_y() + 1;
        for z in self.bbox.min.2..=self.bbox.max.2 {
            for x in self.bbox.min.0..=self.bbox.max.0 {
                let height = run
                    .region
                    .height_at(Heightmap::MotionBlockingNoLeaves, x, z);
                lowest = lowest.min(height);
            }
        }
        lowest
    }

    pub(crate) fn average(&self, run: &mut Run) -> Option<i32> {
        let home = (self.bbox.min.0 >> 4, self.bbox.min.2 >> 4);
        let mut total = 0;
        let mut count = 0;
        for z in self.bbox.min.2..=self.bbox.max.2 {
            for x in self.bbox.min.0..=self.bbox.max.0 {
                if x >> 4 == home.0 && z >> 4 == home.1 {
                    total += run
                        .region
                        .height_at(Heightmap::MotionBlockingNoLeaves, x, z);
                    count += 1;
                }
            }
        }
        (count > 0).then(|| total / count)
    }

    pub(crate) fn moved(&self, height: i32, offset: i32) -> Bbox {
        self.bbox.moved(0, height - self.bbox.min.1 + offset, 0)
    }
}
