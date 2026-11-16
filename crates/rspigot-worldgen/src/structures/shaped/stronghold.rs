use std::sync::atomic::{AtomicBool, Ordering};

use rspigot_blocks::BlockKind as Kind;
use rspigot_domain::BlockStateId;

use super::frame::{Frame, Guard, cave_air, plain, with};
use crate::legacy::JavaRandom;
use crate::overworld::features::place::Run;
use crate::structures::bbox::{Bbox, Pos};
use crate::structures::blueprint::Dir;
use crate::structures::jigsaw::Ground;
use crate::structures::placement::{BiomeProbe, large_feature_seed};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Id {
    Straight,
    PrisonHall,
    LeftTurn,
    RightTurn,
    RoomCrossing,
    StraightStairsDown,
    StairsDown,
    FiveCrossing,
    ChestCorridor,
    Library,
    PortalRoom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Door {
    Opening,
    Wood,
    Grates,
    Iron,
}

#[derive(Debug)]
pub(crate) enum Part {
    Straight {
        left: bool,
        right: bool,
    },
    PrisonHall,
    LeftTurn,
    RightTurn,
    RoomCrossing {
        variant: i32,
    },
    StraightStairsDown,
    StairsDown {
        source: bool,
    },
    FiveCrossing {
        left_low: bool,
        left_high: bool,
        right_low: bool,
        right_high: bool,
    },
    ChestCorridor {
        placed: AtomicBool,
    },
    Library {
        tall: bool,
    },
    PortalRoom {
        placed: AtomicBool,
    },
    Filler {
        steps: i32,
    },
}

#[derive(Debug)]
pub(crate) struct Piece {
    pub(crate) part: Part,
    pub(crate) bbox: Bbox,
    pub(crate) facing: Dir,
    pub(crate) depth: i32,
    pub(crate) door: Door,
}

impl Piece {
    #[cfg(test)]
    pub(crate) fn class(&self) -> &'static str {
        match self.part {
            Part::Straight { .. } => "Straight",
            Part::PrisonHall => "PrisonHall",
            Part::LeftTurn => "LeftTurn",
            Part::RightTurn => "RightTurn",
            Part::RoomCrossing { .. } => "RoomCrossing",
            Part::StraightStairsDown => "StraightStairsDown",
            Part::StairsDown { source: true } => "StartPiece",
            Part::StairsDown { .. } => "StairsDown",
            Part::FiveCrossing { .. } => "FiveCrossing",
            Part::ChestCorridor { .. } => "ChestCorridor",
            Part::Library { .. } => "Library",
            Part::PortalRoom { .. } => "PortalRoom",
            Part::Filler { .. } => "FillerCorridor",
        }
    }

    pub(crate) fn id(&self) -> &'static str {
        match self.part {
            Part::Straight { .. } => "shstraight",
            Part::PrisonHall => "shph",
            Part::LeftTurn => "shlt",
            Part::RightTurn => "shrt",
            Part::RoomCrossing { .. } => "shrc",
            Part::StraightStairsDown => "shssd",
            Part::StairsDown { source: true } => "shstart",
            Part::StairsDown { .. } => "shsd",
            Part::FiveCrossing { .. } => "shfc",
            Part::ChestCorridor { .. } => "shcc",
            Part::Library { .. } => "shli",
            Part::PortalRoom { .. } => "shpr",
            Part::Filler { .. } => "shfc2",
        }
    }

    pub(crate) fn extra(&self) -> Vec<(&'static str, i32)> {
        let door = match self.door {
            Door::Opening => 0,
            Door::Wood => 1,
            Door::Grates => 2,
            Door::Iron => 3,
        };
        let mut out = vec![("EntryDoor", door)];
        match &self.part {
            Part::Straight { left, right } => {
                out.push(("Left", i32::from(*left)));
                out.push(("Right", i32::from(*right)));
            }
            Part::RoomCrossing { variant } => out.push(("Type", *variant)),
            Part::StairsDown { source } => out.push(("Source", i32::from(*source))),
            Part::ChestCorridor { placed } => {
                out.push(("Chest", i32::from(placed.load(Ordering::Relaxed))));
            }
            Part::Library { tall } => out.push(("Tall", i32::from(*tall))),
            Part::PortalRoom { placed } => {
                out.push(("Mob", i32::from(placed.load(Ordering::Relaxed))))
            }
            Part::Filler { steps } => out.push(("Steps", *steps)),
            _ => {}
        }
        out
    }
}

pub(crate) fn orient_box(foot: Pos, off: Pos, size: Pos, dir: Dir) -> Bbox {
    let (fx, fy, fz) = foot;
    let (ox, oy, oz) = off;
    let (w, h, d) = size;
    let (min, max) = match dir {
        Dir::North => (
            (fx + ox, fy + oy, fz - d + 1 + oz),
            (fx + w - 1 + ox, fy + h - 1 + oy, fz + oz),
        ),
        Dir::West => (
            (fx - d + 1 + oz, fy + oy, fz + ox),
            (fx + oz, fy + h - 1 + oy, fz + w - 1 + ox),
        ),
        Dir::East => (
            (fx + oz, fy + oy, fz + ox),
            (fx + d - 1 + oz, fy + h - 1 + oy, fz + w - 1 + ox),
        ),
        _ => (
            (fx + ox, fy + oy, fz + oz),
            (fx + w - 1 + ox, fy + h - 1 + oy, fz + d - 1 + oz),
        ),
    };
    Bbox { min, max }
}

struct Weight {
    id: Id,
    weight: i32,
    max: i32,
    count: i32,
}

impl Weight {
    fn can_place(&self, depth: i32) -> bool {
        let base = self.max == 0 || self.count < self.max;
        match self.id {
            Id::Library => base && depth > 4,
            Id::PortalRoom => base && depth > 5,
            _ => base,
        }
    }

    fn valid(&self) -> bool {
        self.max == 0 || self.count < self.max
    }
}

fn weights() -> Vec<Weight> {
    [
        (Id::Straight, 40, 0),
        (Id::PrisonHall, 5, 5),
        (Id::LeftTurn, 20, 0),
        (Id::RightTurn, 20, 0),
        (Id::RoomCrossing, 10, 6),
        (Id::StraightStairsDown, 5, 5),
        (Id::StairsDown, 5, 5),
        (Id::FiveCrossing, 5, 4),
        (Id::ChestCorridor, 5, 4),
        (Id::Library, 10, 2),
        (Id::PortalRoom, 20, 1),
    ]
    .into_iter()
    .map(|(id, weight, max)| Weight {
        id,
        weight,
        max,
        count: 0,
    })
    .collect()
}

fn random_door(random: &mut JavaRandom) -> Door {
    match random.next_int(5) {
        2 => Door::Wood,
        3 => Door::Grates,
        4 => Door::Iron,
        _ => Door::Opening,
    }
}

fn ok_box(bbox: &Bbox) -> bool {
    bbox.min.1 > 10
}

fn free(pieces: &[Piece], bbox: &Bbox) -> bool {
    ok_box(bbox) && !pieces.iter().any(|piece| piece.bbox.intersects(bbox))
}

struct Builder {
    pieces: Vec<Piece>,
    pending: Vec<usize>,
    weights: Vec<Weight>,
    previous: Option<Id>,
    imposed: Option<Id>,
    origin: Bbox,
}

struct Anchor {
    bbox: Bbox,
    facing: Dir,
    depth: i32,
}

impl Builder {
    fn create(
        &self,
        id: Id,
        random: &mut JavaRandom,
        foot: Pos,
        dir: Dir,
        depth: i32,
    ) -> Option<Piece> {
        let (off, size) = match id {
            Id::Straight | Id::ChestCorridor => ((-1, -1, 0), (5, 5, 7)),
            Id::PrisonHall => ((-1, -1, 0), (9, 5, 11)),
            Id::LeftTurn | Id::RightTurn => ((-1, -1, 0), (5, 5, 5)),
            Id::RoomCrossing => ((-4, -1, 0), (11, 7, 11)),
            Id::StraightStairsDown => ((-1, -7, 0), (5, 11, 8)),
            Id::StairsDown => ((-1, -7, 0), (5, 11, 5)),
            Id::FiveCrossing => ((-4, -3, 0), (10, 9, 11)),
            Id::Library => ((-4, -1, 0), (14, 11, 15)),
            Id::PortalRoom => ((-4, -1, 0), (11, 8, 16)),
        };
        let mut bbox = orient_box(foot, off, size, dir);
        if !free(&self.pieces, &bbox) {
            if id != Id::Library {
                return None;
            }
            bbox = orient_box(foot, off, (14, 6, 15), dir);
            if !free(&self.pieces, &bbox) {
                return None;
            }
        }
        let (part, door) = match id {
            Id::Straight => {
                let door = random_door(random);
                let left = random.next_int(2) == 0;
                let right = random.next_int(2) == 0;
                (Part::Straight { left, right }, door)
            }
            Id::PrisonHall => (Part::PrisonHall, random_door(random)),
            Id::LeftTurn => (Part::LeftTurn, random_door(random)),
            Id::RightTurn => (Part::RightTurn, random_door(random)),
            Id::RoomCrossing => {
                let door = random_door(random);
                (
                    Part::RoomCrossing {
                        variant: random.next_int(5),
                    },
                    door,
                )
            }
            Id::StraightStairsDown => (Part::StraightStairsDown, random_door(random)),
            Id::StairsDown => (Part::StairsDown { source: false }, random_door(random)),
            Id::FiveCrossing => {
                let door = random_door(random);
                let left_low = random.next_boolean();
                let left_high = random.next_boolean();
                let right_low = random.next_boolean();
                let right_high = random.next_int(3) > 0;
                (
                    Part::FiveCrossing {
                        left_low,
                        left_high,
                        right_low,
                        right_high,
                    },
                    door,
                )
            }
            Id::ChestCorridor => (
                Part::ChestCorridor {
                    placed: AtomicBool::new(false),
                },
                random_door(random),
            ),
            Id::Library => {
                let door = random_door(random);
                (
                    Part::Library {
                        tall: bbox.y_span() > 6,
                    },
                    door,
                )
            }
            Id::PortalRoom => (
                Part::PortalRoom {
                    placed: AtomicBool::new(false),
                },
                Door::Opening,
            ),
        };
        Some(Piece {
            part,
            bbox,
            facing: dir,
            depth,
            door,
        })
    }

    fn filler(&self, foot: Pos, dir: Dir) -> Option<Bbox> {
        let probe = orient_box(foot, (-1, -1, 0), (5, 5, 4), dir);
        let hit = self
            .pieces
            .iter()
            .find(|piece| piece.bbox.intersects(&probe))?;
        if hit.bbox.min.1 != probe.min.1 {
            return None;
        }
        for depth in (1..=2).rev() {
            let shorter = orient_box(foot, (-1, -1, 0), (5, 5, depth), dir);
            if !hit.bbox.intersects(&shorter) {
                return Some(orient_box(foot, (-1, -1, 0), (5, 5, depth + 1), dir));
            }
        }
        None
    }

    fn pick_piece(
        &mut self,
        random: &mut JavaRandom,
        foot: Pos,
        dir: Dir,
        depth: i32,
    ) -> Option<Piece> {
        let mut total = 0;
        let mut any = false;
        for weight in &self.weights {
            if weight.max > 0 && weight.count < weight.max {
                any = true;
            }
            total += weight.weight;
        }
        if !any {
            return None;
        }
        if let Some(id) = self.imposed.take()
            && let Some(piece) = self.create(id, random, foot, dir, depth)
        {
            return Some(piece);
        }
        for _ in 0..5 {
            let mut roll = random.next_int(total);
            for index in 0..self.weights.len() {
                roll -= self.weights[index].weight;
                if roll < 0 {
                    let id = self.weights[index].id;
                    if !self.weights[index].can_place(depth) || self.previous == Some(id) {
                        break;
                    }
                    if let Some(piece) = self.create(id, random, foot, dir, depth) {
                        self.weights[index].count += 1;
                        self.previous = Some(id);
                        if !self.weights[index].valid() {
                            self.weights.remove(index);
                        }
                        return Some(piece);
                    }
                }
            }
        }
        let bbox = self.filler(foot, dir)?;
        if bbox.min.1 < 2 {
            return None;
        }
        let steps = if matches!(dir, Dir::North | Dir::South) {
            bbox.max.2 - bbox.min.2 + 1
        } else {
            bbox.max.0 - bbox.min.0 + 1
        };
        Some(Piece {
            part: Part::Filler { steps },
            bbox,
            facing: dir,
            depth,
            door: Door::Opening,
        })
    }

    fn spawn(&mut self, random: &mut JavaRandom, foot: Pos, dir: Dir, depth: i32) {
        if depth > 50 {
            return;
        }
        if (foot.0 - self.origin.min.0).abs() >= 113 || (foot.2 - self.origin.min.2).abs() >= 113 {
            return;
        }
        if let Some(piece) = self.pick_piece(random, foot, dir, depth + 1) {
            self.pieces.push(piece);
            self.pending.push(self.pieces.len() - 1);
        }
    }

    fn forward(&mut self, random: &mut JavaRandom, at: &Anchor, x_off: i32, y_off: i32) {
        let b = at.bbox;
        let y = b.min.1 + y_off;
        let (foot, dir) = match at.facing {
            Dir::North => ((b.min.0 + x_off, y, b.min.2 - 1), at.facing),
            Dir::South => ((b.min.0 + x_off, y, b.max.2 + 1), at.facing),
            Dir::West => ((b.min.0 - 1, y, b.min.2 + x_off), at.facing),
            Dir::East => ((b.max.0 + 1, y, b.min.2 + x_off), at.facing),
            _ => return,
        };
        self.spawn(random, foot, dir, at.depth);
    }

    fn left(&mut self, random: &mut JavaRandom, at: &Anchor, y_off: i32, z_off: i32) {
        let b = at.bbox;
        let y = b.min.1 + y_off;
        let (foot, dir) = match at.facing {
            Dir::North | Dir::South => ((b.min.0 - 1, y, b.min.2 + z_off), Dir::West),
            Dir::West | Dir::East => ((b.min.0 + z_off, y, b.min.2 - 1), Dir::North),
            _ => return,
        };
        self.spawn(random, foot, dir, at.depth);
    }

    fn right(&mut self, random: &mut JavaRandom, at: &Anchor, y_off: i32, z_off: i32) {
        let b = at.bbox;
        let y = b.min.1 + y_off;
        let (foot, dir) = match at.facing {
            Dir::North | Dir::South => ((b.max.0 + 1, y, b.min.2 + z_off), Dir::East),
            Dir::West | Dir::East => ((b.min.0 + z_off, y, b.max.2 + 1), Dir::South),
            _ => return,
        };
        self.spawn(random, foot, dir, at.depth);
    }

    fn expand(&mut self, index: usize, random: &mut JavaRandom) {
        let piece = &self.pieces[index];
        let at = Anchor {
            bbox: piece.bbox,
            facing: piece.facing,
            depth: piece.depth,
        };
        let north_east = matches!(at.facing, Dir::North | Dir::East);
        match &piece.part {
            Part::Straight { left, right } => {
                let (left, right) = (*left, *right);
                self.forward(random, &at, 1, 1);
                if left {
                    self.left(random, &at, 1, 2);
                }
                if right {
                    self.right(random, &at, 1, 2);
                }
            }
            Part::PrisonHall | Part::ChestCorridor { .. } | Part::StraightStairsDown => {
                self.forward(random, &at, 1, 1);
            }
            Part::LeftTurn => {
                if north_east {
                    self.left(random, &at, 1, 1);
                } else {
                    self.right(random, &at, 1, 1);
                }
            }
            Part::RightTurn => {
                if north_east {
                    self.right(random, &at, 1, 1);
                } else {
                    self.left(random, &at, 1, 1);
                }
            }
            Part::RoomCrossing { .. } => {
                self.forward(random, &at, 4, 1);
                self.left(random, &at, 1, 4);
                self.right(random, &at, 1, 4);
            }
            Part::StairsDown { source } => {
                if *source {
                    self.imposed = Some(Id::FiveCrossing);
                }
                self.forward(random, &at, 1, 1);
            }
            Part::FiveCrossing {
                left_low,
                left_high,
                right_low,
                right_high,
            } => {
                let (ll, lh, rl, rh) = (*left_low, *left_high, *right_low, *right_high);
                let (mut a, mut b) = (3, 5);
                if matches!(at.facing, Dir::West | Dir::North) {
                    a = 8 - a;
                    b = 8 - b;
                }
                self.forward(random, &at, 5, 1);
                if ll {
                    self.left(random, &at, a, 1);
                }
                if lh {
                    self.left(random, &at, b, 7);
                }
                if rl {
                    self.right(random, &at, a, 1);
                }
                if rh {
                    self.right(random, &at, b, 7);
                }
            }
            Part::Library { .. } | Part::PortalRoom { .. } | Part::Filler { .. } => {}
        }
    }
}

pub(crate) fn generate<G: Ground>(
    ground: &G,
    valid: &[u8],
    probe: &impl BiomeProbe,
    seed: i64,
    chunk: (i32, i32),
) -> Option<Vec<Piece>> {
    let at = (chunk.0 * 16, 0, chunk.1 * 16);
    if !valid.contains(&probe.at(at.0 >> 2, at.1 >> 2, at.2 >> 2)) {
        return None;
    }
    let mut random = JavaRandom::new(0);
    let mut tries = 0;
    loop {
        large_feature_seed(&mut random, seed.wrapping_add(tries), chunk.0, chunk.1);
        tries += 1;
        let dir = [Dir::North, Dir::East, Dir::South, Dir::West][random.next_int(4) as usize];
        let (west, north) = (chunk.0 * 16 + 2, chunk.1 * 16 + 2);
        let bbox = Bbox {
            min: (west, 64, north),
            max: (west + 4, 74, north + 4),
        };
        let mut builder = Builder {
            pieces: vec![Piece {
                part: Part::StairsDown { source: true },
                bbox,
                facing: dir,
                depth: 0,
                door: Door::Opening,
            }],
            pending: Vec::new(),
            weights: weights(),
            previous: None,
            imposed: None,
            origin: bbox,
        };
        builder.expand(0, &mut random);
        while !builder.pending.is_empty() {
            let pick = random.next_int(builder.pending.len() as i32) as usize;
            let index = builder.pending.remove(pick);
            builder.expand(index, &mut random);
        }
        let mut pieces = builder.pieces;
        let mut whole = pieces[0].bbox;
        for piece in &pieces {
            whole.encapsulate(&piece.bbox);
        }
        let limit = ground.sea_level() - 10;
        let mut low = whole.y_span() + ground.min_y() + 1;
        if low < limit {
            low += random.next_int(limit - low);
        }
        let dy = low - whole.max.1;
        for piece in &mut pieces {
            piece.bbox = piece.bbox.moved(0, dy, 0);
        }
        if pieces
            .iter()
            .any(|piece| matches!(piece.part, Part::PortalRoom { .. }))
        {
            return Some(pieces);
        }
    }
}

const STRONGHOLD_CORRIDOR: &str = "minecraft:chests/stronghold_corridor";
const STRONGHOLD_CROSSING: &str = "minecraft:chests/stronghold_crossing";
const STRONGHOLD_LIBRARY: &str = "minecraft:chests/stronghold_library";

fn stone_state(rng: &mut crate::climate::Xoroshiro, edge: bool) -> BlockStateId {
    if !edge {
        return cave_air();
    }
    let roll = rng.next_float();
    if roll < 0.2 {
        plain(Kind::CrackedStoneBricks)
    } else if roll < 0.5 {
        plain(Kind::MossyStoneBricks)
    } else if roll < 0.55 {
        plain(Kind::InfestedStoneBricks)
    } else {
        plain(Kind::StoneBricks)
    }
}

fn solid(f: &mut Frame, from: Pos, to: Pos, skip_air: bool) {
    f.selected(from, to, skip_air, stone_state);
}

fn air(f: &mut Frame, from: Pos, to: Pos) {
    f.bounds(from, to, cave_air(), cave_air(), false);
}

fn fill(f: &mut Frame, from: Pos, to: Pos, state: BlockStateId) {
    f.bounds(from, to, state, state, false);
}

fn bars() -> BlockStateId {
    plain(Kind::IronBars)
}

fn small_door(f: &mut Frame, door: Door, x: i32, y: i32, z: i32) {
    let brick = plain(Kind::StoneBricks);
    let frame_blocks = [
        (x, y, z),
        (x, y + 1, z),
        (x, y + 2, z),
        (x + 1, y + 2, z),
        (x + 2, y + 2, z),
        (x + 2, y + 1, z),
        (x + 2, y, z),
    ];
    match door {
        Door::Opening => air(f, (x, y, z), (x + 2, y + 2, z)),
        Door::Wood => {
            for (px, py, pz) in frame_blocks {
                f.place(brick, px, py, pz);
            }
            f.place(plain(Kind::OakDoor), x + 1, y, z);
            f.place(with(Kind::OakDoor, &[("half", "upper")]), x + 1, y + 1, z);
        }
        Door::Grates => {
            f.place(cave_air(), x + 1, y, z);
            f.place(cave_air(), x + 1, y + 1, z);
            for (px, py, pz) in frame_blocks {
                f.place(bars(), px, py, pz);
            }
        }
        Door::Iron => {
            for (px, py, pz) in frame_blocks {
                f.place(brick, px, py, pz);
            }
            f.place(plain(Kind::IronDoor), x + 1, y, z);
            f.place(with(Kind::IronDoor, &[("half", "upper")]), x + 1, y + 1, z);
            f.place(
                with(Kind::StoneButton, &[("facing", "north")]),
                x + 2,
                y + 1,
                z + 1,
            );
            f.place(
                with(Kind::StoneButton, &[("facing", "south")]),
                x + 2,
                y + 1,
                z - 1,
            );
        }
    }
}

fn torch(facing: &str) -> BlockStateId {
    with(Kind::WallTorch, &[("facing", facing)])
}

fn slab() -> BlockStateId {
    plain(Kind::SmoothStoneSlab)
}

impl Piece {
    pub(crate) fn place(&self, run: &mut Run, write: &Bbox) {
        let mut f = Frame::new(run, self.bbox, Some(self.facing), *write, Guard::Open);
        let north_east = matches!(self.facing, Dir::North | Dir::East);
        match &self.part {
            Part::Straight { left, right } => {
                solid(&mut f, (0, 0, 0), (4, 4, 6), true);
                small_door(&mut f, self.door, 1, 1, 0);
                small_door(&mut f, Door::Opening, 1, 1, 6);
                let east = torch("east");
                let west = torch("west");
                f.maybe_block(0.1, (1, 2, 1), east);
                f.maybe_block(0.1, (3, 2, 1), west);
                f.maybe_block(0.1, (1, 2, 5), east);
                f.maybe_block(0.1, (3, 2, 5), west);
                if *left {
                    air(&mut f, (0, 1, 2), (0, 3, 4));
                }
                if *right {
                    air(&mut f, (4, 1, 2), (4, 3, 4));
                }
            }
            Part::PrisonHall => prison(&mut f, self.door),
            Part::LeftTurn | Part::RightTurn => {
                solid(&mut f, (0, 0, 0), (4, 4, 4), true);
                small_door(&mut f, self.door, 1, 1, 0);
                let left = matches!(self.part, Part::LeftTurn);
                if north_east == left {
                    air(&mut f, (0, 1, 1), (0, 3, 3));
                } else {
                    air(&mut f, (4, 1, 1), (4, 3, 3));
                }
            }
            Part::RoomCrossing { variant } => room_crossing(&mut f, self.door, *variant),
            Part::StraightStairsDown => {
                solid(&mut f, (0, 0, 0), (4, 10, 7), true);
                small_door(&mut f, self.door, 1, 7, 0);
                small_door(&mut f, Door::Opening, 1, 1, 7);
                let stairs = with(Kind::CobblestoneStairs, &[("facing", "south")]);
                let brick = plain(Kind::StoneBricks);
                for i in 0..6 {
                    for x in 1..=3 {
                        f.place(stairs, x, 6 - i, i + 1);
                        if i < 5 {
                            f.place(brick, x, 5 - i, i + 1);
                        }
                    }
                }
            }
            Part::StairsDown { .. } => stairs_down(&mut f, self.door),
            Part::FiveCrossing {
                left_low,
                left_high,
                right_low,
                right_high,
            } => five_crossing(
                &mut f,
                self.door,
                [*left_low, *left_high, *right_low, *right_high],
            ),
            Part::ChestCorridor { placed } => {
                solid(&mut f, (0, 0, 0), (4, 4, 6), true);
                small_door(&mut f, self.door, 1, 1, 0);
                small_door(&mut f, Door::Opening, 1, 1, 6);
                let brick = plain(Kind::StoneBricks);
                f.bounds((3, 1, 2), (3, 1, 4), brick, brick, false);
                let half = plain(Kind::StoneBrickSlab);
                f.place(half, 3, 1, 1);
                f.place(half, 3, 1, 5);
                f.place(half, 3, 2, 2);
                f.place(half, 3, 2, 4);
                for z in 2..=4 {
                    f.place(half, 2, 1, z);
                }
                if !placed.load(Ordering::Relaxed) {
                    let at = f.world_pos(3, 2, 3);
                    if f.chunk.contains(at) {
                        placed.store(true, Ordering::Relaxed);
                        f.chest(3, 2, 3, STRONGHOLD_CORRIDOR);
                    }
                }
            }
            Part::Library { tall } => library(&mut f, self.door, *tall),
            Part::PortalRoom { placed } => portal_room(&mut f, placed),
            Part::Filler { steps } => {
                let brick = plain(Kind::StoneBricks);
                for z in 0..*steps {
                    for x in 0..=4 {
                        f.place(brick, x, 0, z);
                    }
                    for y in 1..=3 {
                        f.place(brick, 0, y, z);
                        f.place(cave_air(), 1, y, z);
                        f.place(cave_air(), 2, y, z);
                        f.place(cave_air(), 3, y, z);
                        f.place(brick, 4, y, z);
                    }
                    for x in 0..=4 {
                        f.place(brick, x, 4, z);
                    }
                }
            }
        }
        f.finish();
    }
}

fn prison(f: &mut Frame, door: Door) {
    solid(f, (0, 0, 0), (8, 4, 10), true);
    small_door(f, door, 1, 1, 0);
    air(f, (1, 1, 10), (3, 3, 10));
    solid(f, (4, 1, 1), (4, 3, 1), false);
    solid(f, (4, 1, 3), (4, 3, 3), false);
    solid(f, (4, 1, 7), (4, 3, 7), false);
    solid(f, (4, 1, 9), (4, 3, 9), false);
    for y in 1..=3 {
        for (x, z) in [(4, 4), (4, 5), (4, 6), (5, 5), (6, 5), (7, 5)] {
            f.place(bars(), x, y, z);
        }
    }
    f.place(bars(), 4, 3, 2);
    f.place(bars(), 4, 3, 8);
    let lower = with(Kind::IronDoor, &[("facing", "west")]);
    let upper = with(Kind::IronDoor, &[("facing", "west"), ("half", "upper")]);
    f.place(lower, 4, 1, 2);
    f.place(upper, 4, 2, 2);
    f.place(lower, 4, 1, 8);
    f.place(upper, 4, 2, 8);
}

fn room_crossing(f: &mut Frame, door: Door, variant: i32) {
    solid(f, (0, 0, 0), (10, 6, 10), true);
    small_door(f, door, 4, 1, 0);
    air(f, (4, 1, 10), (6, 3, 10));
    air(f, (0, 1, 4), (0, 3, 6));
    air(f, (10, 1, 4), (10, 3, 6));
    let brick = plain(Kind::StoneBricks);
    match variant {
        0 => {
            for y in 1..=3 {
                f.place(brick, 5, y, 5);
            }
            f.place(torch("west"), 4, 3, 5);
            f.place(torch("east"), 6, 3, 5);
            f.place(torch("south"), 5, 3, 4);
            f.place(torch("north"), 5, 3, 6);
            for (x, z) in [
                (4, 4),
                (4, 5),
                (4, 6),
                (6, 4),
                (6, 5),
                (6, 6),
                (5, 4),
                (5, 6),
            ] {
                f.place(slab(), x, 1, z);
            }
        }
        1 => {
            for i in 0..5 {
                f.place(brick, 3, 1, 3 + i);
                f.place(brick, 7, 1, 3 + i);
                f.place(brick, 3 + i, 1, 3);
                f.place(brick, 3 + i, 1, 7);
            }
            for y in 1..=3 {
                f.place(brick, 5, y, 5);
            }
            f.place(plain(Kind::Water), 5, 4, 5);
        }
        2 => {
            let cobble = plain(Kind::Cobblestone);
            for i in 1..9 {
                f.place(cobble, 1, 3, i);
                f.place(cobble, 9, 3, i);
            }
            for i in 1..9 {
                f.place(cobble, i, 3, 1);
                f.place(cobble, i, 3, 9);
            }
            for (x, y, z) in [
                (5, 1, 4),
                (5, 1, 6),
                (5, 3, 4),
                (5, 3, 6),
                (4, 1, 5),
                (6, 1, 5),
                (4, 3, 5),
                (6, 3, 5),
            ] {
                f.place(cobble, x, y, z);
            }
            for i in 1..4 {
                f.place(cobble, 4, i, 4);
                f.place(cobble, 6, i, 4);
                f.place(cobble, 4, i, 6);
                f.place(cobble, 6, i, 6);
            }
            f.place(plain(Kind::WallTorch), 5, 3, 5);
            let planks = plain(Kind::OakPlanks);
            for i in 2..9 {
                f.place(planks, 2, 3, i);
                f.place(planks, 3, 3, i);
                if !(4..=6).contains(&i) {
                    f.place(planks, 4, 3, i);
                    f.place(planks, 5, 3, i);
                    f.place(planks, 6, 3, i);
                }
                f.place(planks, 7, 3, i);
                f.place(planks, 8, 3, i);
            }
            let ladder = with(Kind::Ladder, &[("facing", "west")]);
            for y in 1..=3 {
                f.place(ladder, 9, y, 3);
            }
            f.chest(3, 4, 8, STRONGHOLD_CROSSING);
        }
        _ => {}
    }
}

fn stairs_down(f: &mut Frame, door: Door) {
    solid(f, (0, 0, 0), (4, 10, 4), true);
    small_door(f, door, 1, 7, 0);
    small_door(f, Door::Opening, 1, 1, 4);
    let brick = plain(Kind::StoneBricks);
    let half = slab();
    for (x, y, z, is_slab) in [
        (2, 6, 1, false),
        (1, 5, 1, false),
        (1, 6, 1, true),
        (1, 5, 2, false),
        (1, 4, 3, false),
        (1, 5, 3, true),
        (2, 4, 3, false),
        (3, 3, 3, false),
        (3, 4, 3, true),
        (3, 3, 2, false),
        (3, 2, 1, false),
        (3, 3, 1, true),
        (2, 2, 1, false),
        (1, 1, 1, false),
        (1, 2, 1, true),
        (1, 1, 2, false),
        (1, 1, 3, true),
    ] {
        f.place(if is_slab { half } else { brick }, x, y, z);
    }
}

fn five_crossing(f: &mut Frame, door: Door, open: [bool; 4]) {
    solid(f, (0, 0, 0), (9, 8, 10), true);
    small_door(f, door, 4, 3, 0);
    let [left_low, left_high, right_low, right_high] = open;
    if left_low {
        air(f, (0, 3, 1), (0, 5, 3));
    }
    if right_low {
        air(f, (9, 3, 1), (9, 5, 3));
    }
    if left_high {
        air(f, (0, 5, 7), (0, 7, 9));
    }
    if right_high {
        air(f, (9, 5, 7), (9, 7, 9));
    }
    air(f, (5, 1, 10), (7, 3, 10));
    solid(f, (1, 2, 1), (8, 2, 6), false);
    solid(f, (4, 1, 5), (4, 4, 9), false);
    solid(f, (8, 1, 5), (8, 4, 9), false);
    solid(f, (1, 4, 7), (3, 4, 9), false);
    solid(f, (1, 3, 5), (3, 3, 6), false);
    fill(f, (1, 3, 4), (3, 3, 4), slab());
    fill(f, (1, 4, 6), (3, 4, 6), slab());
    solid(f, (5, 1, 7), (7, 1, 8), false);
    fill(f, (5, 1, 9), (7, 1, 9), slab());
    fill(f, (5, 2, 7), (7, 2, 7), slab());
    fill(f, (4, 5, 7), (4, 5, 9), slab());
    fill(f, (8, 5, 7), (8, 5, 9), slab());
    fill(
        f,
        (5, 5, 7),
        (7, 5, 9),
        with(Kind::SmoothStoneSlab, &[("type", "double")]),
    );
    f.place(torch("south"), 6, 5, 6);
}

fn portal_room(f: &mut Frame, placed: &AtomicBool) {
    solid(f, (0, 0, 0), (10, 7, 15), false);
    small_door(f, Door::Grates, 4, 1, 0);
    solid(f, (1, 6, 1), (1, 6, 14), false);
    solid(f, (9, 6, 1), (9, 6, 14), false);
    solid(f, (2, 6, 1), (8, 6, 2), false);
    solid(f, (2, 6, 14), (8, 6, 14), false);
    solid(f, (1, 1, 1), (2, 1, 4), false);
    solid(f, (8, 1, 1), (9, 1, 4), false);
    let lava = plain(Kind::Lava);
    fill(f, (1, 1, 1), (1, 1, 3), lava);
    fill(f, (9, 1, 1), (9, 1, 3), lava);
    solid(f, (3, 1, 8), (7, 1, 12), false);
    fill(f, (4, 1, 9), (6, 1, 11), lava);
    let mut z = 3;
    while z < 14 {
        fill(f, (0, 3, z), (0, 4, z), bars());
        fill(f, (10, 3, z), (10, 4, z), bars());
        z += 2;
    }
    let mut x = 2;
    while x < 9 {
        fill(f, (x, 3, 15), (x, 4, 15), bars());
        x += 2;
    }
    let stairs = with(Kind::StoneBrickStairs, &[("facing", "north")]);
    solid(f, (4, 1, 5), (6, 1, 7), false);
    solid(f, (4, 2, 6), (6, 2, 7), false);
    solid(f, (4, 3, 7), (6, 3, 7), false);
    for x in 4..=6 {
        f.place(stairs, x, 1, 4);
        f.place(stairs, x, 2, 5);
        f.place(stairs, x, 3, 6);
    }
    let mut eyes = [false; 12];
    let mut all = true;
    for eye in &mut eyes {
        *eye = f.run.rng.next_float() > 0.9;
        all &= *eye;
    }
    let frame = |facing: &str, eye: bool| {
        with(
            Kind::EndPortalFrame,
            &[
                ("facing", facing),
                ("eye", if eye { "true" } else { "false" }),
            ],
        )
    };
    let spots: [(&str, (i32, i32, i32)); 12] = [
        ("north", (4, 3, 8)),
        ("north", (5, 3, 8)),
        ("north", (6, 3, 8)),
        ("south", (4, 3, 12)),
        ("south", (5, 3, 12)),
        ("south", (6, 3, 12)),
        ("east", (3, 3, 9)),
        ("east", (3, 3, 10)),
        ("east", (3, 3, 11)),
        ("west", (7, 3, 9)),
        ("west", (7, 3, 10)),
        ("west", (7, 3, 11)),
    ];
    for (index, (facing, (x, y, z))) in spots.into_iter().enumerate() {
        f.place(frame(facing, eyes[index]), x, y, z);
    }
    if all {
        for z in 9..=11 {
            for x in 4..=6 {
                f.place(plain(Kind::EndPortal), x, 3, z);
            }
        }
    }
    if !placed.load(Ordering::Relaxed) {
        let at = f.world_pos(5, 3, 6);
        if f.chunk.contains(at) {
            placed.store(true, Ordering::Relaxed);
            f.spawner(at, "minecraft:silverfish");
        }
    }
}

fn library(f: &mut Frame, door: Door, tall: bool) {
    let height = if tall { 11 } else { 6 };
    solid(f, (0, 0, 0), (13, height - 1, 14), true);
    small_door(f, door, 4, 1, 0);
    f.maybe_bounds(
        0.07,
        (2, 1, 1),
        (11, 4, 13),
        plain(Kind::Cobweb),
        plain(Kind::Cobweb),
        false,
        false,
    );
    let planks = plain(Kind::OakPlanks);
    let shelf = plain(Kind::Bookshelf);
    for i in 1..=13 {
        let wall = if (i - 1) % 4 == 0 { planks } else { shelf };
        fill(f, (1, 1, i), (1, 4, i), wall);
        fill(f, (12, 1, i), (12, 4, i), wall);
        if (i - 1) % 4 == 0 {
            f.place(torch("east"), 2, 3, i);
            f.place(torch("west"), 11, 3, i);
        }
        if tall {
            fill(f, (1, 6, i), (1, 9, i), wall);
            fill(f, (12, 6, i), (12, 9, i), wall);
        }
    }
    let mut i = 3;
    while i < 12 {
        fill(f, (3, 1, i), (4, 3, i), shelf);
        fill(f, (6, 1, i), (7, 3, i), shelf);
        fill(f, (9, 1, i), (10, 3, i), shelf);
        i += 2;
    }
    if tall {
        fill(f, (1, 5, 1), (3, 5, 13), planks);
        fill(f, (10, 5, 1), (12, 5, 13), planks);
        fill(f, (4, 5, 1), (9, 5, 2), planks);
        fill(f, (4, 5, 12), (9, 5, 13), planks);
        f.place(planks, 9, 5, 11);
        f.place(planks, 8, 5, 11);
        f.place(planks, 9, 5, 10);
        let sides = |names: &[&str]| {
            let props: Vec<(&str, &str)> = names.iter().map(|name| (*name, "true")).collect();
            with(Kind::OakFence, &props)
        };
        let west_east = sides(&["west", "east"]);
        let north_south = sides(&["north", "south"]);
        fill(f, (3, 6, 3), (3, 6, 11), north_south);
        fill(f, (10, 6, 3), (10, 6, 9), north_south);
        fill(f, (4, 6, 2), (9, 6, 2), west_east);
        fill(f, (4, 6, 12), (7, 6, 12), west_east);
        f.place(sides(&["north", "east"]), 3, 6, 2);
        f.place(sides(&["south", "east"]), 3, 6, 12);
        f.place(sides(&["north", "west"]), 10, 6, 2);
        for i in 0..=2 {
            f.place(sides(&["south", "west"]), 8 + i, 6, 12 - i);
            if i != 2 {
                f.place(sides(&["north", "east"]), 8 + i, 6, 11 - i);
            }
        }
        let ladder = with(Kind::Ladder, &[("facing", "south")]);
        for y in 1..=7 {
            f.place(ladder, 10, y, 13);
        }
        let east = sides(&["east"]);
        let west = sides(&["west"]);
        f.place(east, 6, 9, 7);
        f.place(west, 7, 9, 7);
        f.place(east, 6, 8, 7);
        f.place(west, 7, 8, 7);
        f.place(west_east, 6, 7, 7);
        f.place(west_east, 7, 7, 7);
        f.place(east, 5, 7, 7);
        f.place(west, 8, 7, 7);
        f.place(sides(&["east", "north"]), 6, 7, 6);
        f.place(sides(&["east", "south"]), 6, 7, 8);
        f.place(sides(&["west", "north"]), 7, 7, 6);
        f.place(sides(&["west", "south"]), 7, 7, 8);
        let lamp = plain(Kind::Torch);
        for (x, y, z) in [
            (5, 8, 7),
            (8, 8, 7),
            (6, 8, 6),
            (6, 8, 8),
            (7, 8, 6),
            (7, 8, 8),
        ] {
            f.place(lamp, x, y, z);
        }
    }
    f.chest(3, 3, 5, STRONGHOLD_LIBRARY);
    if tall {
        f.place(cave_air(), 12, 9, 1);
        f.chest(12, 8, 1, STRONGHOLD_LIBRARY);
    }
}
