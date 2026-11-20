use rspigot_blocks::BlockKind as Kind;
use rspigot_domain::BlockStateId;
use rspigot_nbt::Tag;

use super::frame::{Frame, Guard, plain, with};
use super::start::{random, valid_at};
use super::stronghold::orient_box;
use crate::legacy::JavaRandom;
use crate::overworld::features::place::Run;
use crate::structures::bbox::{Bbox, Pos};
use crate::structures::blueprint::Dir;
use crate::structures::placement::BiomeProbe;

const NETHER_BRIDGE: &str = "minecraft:chests/nether_bridge";
const START_Y: i32 = 64;
const REACH: i32 = 113;
const DEPTH_LIMIT: i32 = 30;
const LOWEST: i32 = 48;
const HIGHEST: i32 = 70;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Part {
    Start,
    BridgeCrossing,
    BridgeStraight,
    RoomCrossing,
    StairsRoom,
    MonsterThrone,
    CastleEntrance,
    SmallCorridor,
    SmallCrossing,
    RightTurn { chest: bool },
    LeftTurn { chest: bool },
    CorridorStairs,
    TBalcony,
    StalkRoom,
    EndFiller { seed: i32 },
}

#[derive(Debug)]
pub(crate) struct Piece {
    pub(crate) part: Part,
    pub(crate) bbox: Bbox,
    pub(crate) facing: Dir,
    pub(crate) depth: i32,
}

impl Piece {
    #[cfg(test)]
    pub(crate) fn class(&self) -> &'static str {
        match self.part {
            Part::Start => "StartPiece",
            Part::BridgeCrossing => "BridgeCrossing",
            Part::BridgeStraight => "BridgeStraight",
            Part::RoomCrossing => "RoomCrossing",
            Part::StairsRoom => "StairsRoom",
            Part::MonsterThrone => "MonsterThrone",
            Part::CastleEntrance => "CastleEntrance",
            Part::SmallCorridor => "CastleSmallCorridorPiece",
            Part::SmallCrossing => "CastleSmallCorridorCrossingPiece",
            Part::RightTurn { .. } => "CastleSmallCorridorRightTurnPiece",
            Part::LeftTurn { .. } => "CastleSmallCorridorLeftTurnPiece",
            Part::CorridorStairs => "CastleCorridorStairsPiece",
            Part::TBalcony => "CastleCorridorTBalconyPiece",
            Part::StalkRoom => "CastleStalkRoom",
            Part::EndFiller { .. } => "BridgeEndFiller",
        }
    }

    pub(crate) fn id(&self) -> &'static str {
        match self.part {
            Part::Start => "NeStart",
            Part::BridgeCrossing => "NeBCr",
            Part::BridgeStraight => "NeBS",
            Part::RoomCrossing => "NeRC",
            Part::StairsRoom => "NeSR",
            Part::MonsterThrone => "NeMT",
            Part::CastleEntrance => "NeCE",
            Part::SmallCorridor => "NeSC",
            Part::SmallCrossing => "NeSCSC",
            Part::RightTurn { .. } => "NeSCRT",
            Part::LeftTurn { .. } => "NeSCLT",
            Part::CorridorStairs => "NeCCS",
            Part::TBalcony => "NeCTB",
            Part::StalkRoom => "NeCSR",
            Part::EndFiller { .. } => "NeBEF",
        }
    }

    pub(crate) fn extra(&self) -> Vec<(&'static str, Tag)> {
        match self.part {
            Part::MonsterThrone => vec![("Mob", Tag::from(false))],
            Part::RightTurn { chest } | Part::LeftTurn { chest } => {
                vec![("Chest", Tag::from(chest))]
            }
            Part::EndFiller { seed } => vec![("Seed", Tag::from(seed))],
            _ => Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Class {
    BridgeStraight,
    BridgeCrossing,
    RoomCrossing,
    StairsRoom,
    MonsterThrone,
    CastleEntrance,
    SmallCorridor,
    SmallCrossing,
    RightTurn,
    LeftTurn,
    CorridorStairs,
    TBalcony,
    StalkRoom,
}

#[derive(Debug, Clone, Copy)]
struct Weight {
    class: Class,
    weight: i32,
    max: i32,
    placed: i32,
    in_row: bool,
}

impl Weight {
    fn new(class: Class, weight: i32, max: i32, in_row: bool) -> Self {
        Self {
            class,
            weight,
            max,
            placed: 0,
            in_row,
        }
    }

    fn open(&self) -> bool {
        self.max == 0 || self.placed < self.max
    }
}

fn bridge_weights() -> Vec<Weight> {
    vec![
        Weight::new(Class::BridgeStraight, 30, 0, true),
        Weight::new(Class::BridgeCrossing, 10, 4, false),
        Weight::new(Class::RoomCrossing, 10, 4, false),
        Weight::new(Class::StairsRoom, 10, 3, false),
        Weight::new(Class::MonsterThrone, 5, 2, false),
        Weight::new(Class::CastleEntrance, 5, 1, false),
    ]
}

fn castle_weights() -> Vec<Weight> {
    vec![
        Weight::new(Class::SmallCorridor, 25, 0, true),
        Weight::new(Class::SmallCrossing, 15, 5, false),
        Weight::new(Class::RightTurn, 5, 10, false),
        Weight::new(Class::LeftTurn, 5, 10, false),
        Weight::new(Class::CorridorStairs, 10, 3, true),
        Weight::new(Class::TBalcony, 7, 2, false),
        Weight::new(Class::StalkRoom, 5, 2, false),
    ]
}

fn dimensions(class: Class) -> (Pos, Pos) {
    match class {
        Class::BridgeStraight => ((-1, -3, 0), (5, 10, 19)),
        Class::BridgeCrossing => ((-8, -3, 0), (19, 10, 19)),
        Class::RoomCrossing => ((-2, 0, 0), (7, 9, 7)),
        Class::StairsRoom => ((-2, 0, 0), (7, 11, 7)),
        Class::MonsterThrone => ((-2, 0, 0), (7, 8, 9)),
        Class::CastleEntrance | Class::StalkRoom => ((-5, -3, 0), (13, 14, 13)),
        Class::SmallCorridor | Class::SmallCrossing | Class::RightTurn | Class::LeftTurn => {
            ((-1, 0, 0), (5, 7, 5))
        }
        Class::CorridorStairs => ((-1, -7, 0), (5, 14, 10)),
        Class::TBalcony => ((-3, 0, 0), (9, 7, 9)),
    }
}

fn ok_box(bbox: &Bbox) -> bool {
    bbox.min.1 > 10
}

struct Builder {
    pieces: Vec<Piece>,
    pending: Vec<usize>,
    bridge: Vec<Weight>,
    castle: Vec<Weight>,
    previous: Option<Class>,
    origin: Bbox,
}

struct Anchor {
    bbox: Bbox,
    facing: Dir,
    depth: i32,
}

impl Builder {
    fn free(&self, bbox: &Bbox) -> bool {
        ok_box(bbox) && !self.pieces.iter().any(|piece| piece.bbox.intersects(bbox))
    }

    fn create(
        &self,
        class: Class,
        random: &mut JavaRandom,
        foot: Pos,
        dir: Dir,
        depth: i32,
    ) -> Option<Piece> {
        let (off, size) = dimensions(class);
        let bbox = orient_box(foot, off, size, dir);
        if !self.free(&bbox) {
            return None;
        }
        let part = match class {
            Class::BridgeStraight => Part::BridgeStraight,
            Class::BridgeCrossing => Part::BridgeCrossing,
            Class::RoomCrossing => Part::RoomCrossing,
            Class::StairsRoom => Part::StairsRoom,
            Class::MonsterThrone => Part::MonsterThrone,
            Class::CastleEntrance => Part::CastleEntrance,
            Class::SmallCorridor => Part::SmallCorridor,
            Class::SmallCrossing => Part::SmallCrossing,
            Class::RightTurn => Part::RightTurn {
                chest: random.next_int(3) == 0,
            },
            Class::LeftTurn => Part::LeftTurn {
                chest: random.next_int(3) == 0,
            },
            Class::CorridorStairs => Part::CorridorStairs,
            Class::TBalcony => Part::TBalcony,
            Class::StalkRoom => Part::StalkRoom,
        };
        Some(Piece {
            part,
            bbox,
            facing: dir,
            depth,
        })
    }

    fn filler(&self, random: &mut JavaRandom, foot: Pos, dir: Dir, depth: i32) -> Option<Piece> {
        let bbox = orient_box(foot, (-1, -3, 0), (5, 10, 8), dir);
        if !self.free(&bbox) {
            return None;
        }
        Some(Piece {
            part: Part::EndFiller {
                seed: random.next_i32(),
            },
            bbox,
            facing: dir,
            depth,
        })
    }

    fn total(&self, castle: bool) -> i32 {
        let list = if castle { &self.castle } else { &self.bridge };
        let limited = list.iter().any(|w| w.max > 0 && w.placed < w.max);
        if limited {
            list.iter().map(|w| w.weight).sum()
        } else {
            -1
        }
    }

    fn pick(
        &mut self,
        random: &mut JavaRandom,
        castle: bool,
        foot: Pos,
        dir: Dir,
        depth: i32,
    ) -> Option<Piece> {
        let total = self.total(castle);
        if total >= 1 && depth <= DEPTH_LIMIT {
            for _ in 0..5 {
                let mut roll = random.next_int(total);
                let count = if castle {
                    self.castle.len()
                } else {
                    self.bridge.len()
                };
                for index in 0..count {
                    let weight = if castle {
                        self.castle[index]
                    } else {
                        self.bridge[index]
                    };
                    roll -= weight.weight;
                    if roll >= 0 {
                        continue;
                    }
                    if !weight.open() || (self.previous == Some(weight.class) && !weight.in_row) {
                        break;
                    }
                    if let Some(piece) = self.create(weight.class, random, foot, dir, depth) {
                        let list = if castle {
                            &mut self.castle
                        } else {
                            &mut self.bridge
                        };
                        list[index].placed += 1;
                        self.previous = Some(weight.class);
                        if !list[index].open() {
                            list.remove(index);
                        }
                        return Some(piece);
                    }
                }
            }
        }
        self.filler(random, foot, dir, depth)
    }

    fn spawn(&mut self, random: &mut JavaRandom, foot: Pos, dir: Dir, depth: i32, castle: bool) {
        if (foot.0 - self.origin.min.0).abs() < REACH && (foot.2 - self.origin.min.2).abs() < REACH
        {
            if let Some(piece) = self.pick(random, castle, foot, dir, depth + 1) {
                self.pieces.push(piece);
                self.pending.push(self.pieces.len() - 1);
            }
        } else {
            self.filler(random, foot, dir, depth);
        }
    }

    fn forward(
        &mut self,
        random: &mut JavaRandom,
        at: &Anchor,
        x_off: i32,
        y_off: i32,
        castle: bool,
    ) {
        let b = at.bbox;
        let y = b.min.1 + y_off;
        let foot = match at.facing {
            Dir::North => (b.min.0 + x_off, y, b.min.2 - 1),
            Dir::South => (b.min.0 + x_off, y, b.max.2 + 1),
            Dir::West => (b.min.0 - 1, y, b.min.2 + x_off),
            Dir::East => (b.max.0 + 1, y, b.min.2 + x_off),
            _ => return,
        };
        self.spawn(random, foot, at.facing, at.depth, castle);
    }

    fn left(&mut self, random: &mut JavaRandom, at: &Anchor, y_off: i32, z_off: i32, castle: bool) {
        let b = at.bbox;
        let y = b.min.1 + y_off;
        let (foot, dir) = match at.facing {
            Dir::North | Dir::South => ((b.min.0 - 1, y, b.min.2 + z_off), Dir::West),
            Dir::West | Dir::East => ((b.min.0 + z_off, y, b.min.2 - 1), Dir::North),
            _ => return,
        };
        self.spawn(random, foot, dir, at.depth, castle);
    }

    fn right(
        &mut self,
        random: &mut JavaRandom,
        at: &Anchor,
        y_off: i32,
        z_off: i32,
        castle: bool,
    ) {
        let b = at.bbox;
        let y = b.min.1 + y_off;
        let (foot, dir) = match at.facing {
            Dir::North | Dir::South => ((b.max.0 + 1, y, b.min.2 + z_off), Dir::East),
            Dir::West | Dir::East => ((b.min.0 + z_off, y, b.max.2 + 1), Dir::South),
            _ => return,
        };
        self.spawn(random, foot, dir, at.depth, castle);
    }

    fn expand(&mut self, index: usize, random: &mut JavaRandom) {
        let piece = &self.pieces[index];
        let at = Anchor {
            bbox: piece.bbox,
            facing: piece.facing,
            depth: piece.depth,
        };
        match piece.part {
            Part::Start | Part::BridgeCrossing => {
                self.forward(random, &at, 8, 3, false);
                self.left(random, &at, 3, 8, false);
                self.right(random, &at, 3, 8, false);
            }
            Part::BridgeStraight => self.forward(random, &at, 1, 3, false),
            Part::RoomCrossing => {
                self.forward(random, &at, 2, 0, false);
                self.left(random, &at, 0, 2, false);
                self.right(random, &at, 0, 2, false);
            }
            Part::StairsRoom => self.right(random, &at, 6, 2, false),
            Part::CastleEntrance => self.forward(random, &at, 5, 3, true),
            Part::SmallCorridor | Part::CorridorStairs => self.forward(random, &at, 1, 0, true),
            Part::SmallCrossing => {
                self.forward(random, &at, 1, 0, true);
                self.left(random, &at, 0, 1, true);
                self.right(random, &at, 0, 1, true);
            }
            Part::RightTurn { .. } => self.right(random, &at, 0, 1, true),
            Part::LeftTurn { .. } => self.left(random, &at, 0, 1, true),
            Part::TBalcony => {
                let z = if matches!(at.facing, Dir::West | Dir::North) {
                    5
                } else {
                    1
                };
                let left = random.next_int(8) > 0;
                self.left(random, &at, 0, z, left);
                let right = random.next_int(8) > 0;
                self.right(random, &at, 0, z, right);
            }
            Part::StalkRoom => {
                self.forward(random, &at, 5, 3, true);
                self.forward(random, &at, 5, 11, true);
            }
            Part::MonsterThrone | Part::EndFiller { .. } => {}
        }
    }
}

pub(crate) fn generate(
    valid: &[u8],
    probe: &impl BiomeProbe,
    seed: i64,
    chunk: (i32, i32),
) -> Option<Vec<Piece>> {
    if !valid_at(valid, probe, (chunk.0 * 16, START_Y, chunk.1 * 16)) {
        return None;
    }
    let mut random = random(seed, chunk);
    let facing = [Dir::North, Dir::East, Dir::South, Dir::West][random.next_int(4) as usize];
    let origin = orient_box(
        (chunk.0 * 16 + 2, START_Y, chunk.1 * 16 + 2),
        (0, 0, 0),
        (19, 10, 19),
        facing,
    );
    let mut builder = Builder {
        pieces: vec![Piece {
            part: Part::Start,
            bbox: origin,
            facing,
            depth: 0,
        }],
        pending: Vec::new(),
        bridge: bridge_weights(),
        castle: castle_weights(),
        previous: None,
        origin,
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
    let room = HIGHEST - LOWEST + 1 - whole.y_span();
    let low = if room > 1 {
        LOWEST + random.next_int(room)
    } else {
        LOWEST
    };
    let dy = low - whole.min.1;
    for piece in &mut pieces {
        piece.bbox = piece.bbox.moved(0, dy, 0);
    }
    Some(pieces)
}

fn brick() -> BlockStateId {
    plain(Kind::NetherBricks)
}

fn air() -> BlockStateId {
    BlockStateId::AIR
}

fn fence(sides: &[&str]) -> BlockStateId {
    let properties: Vec<(&str, &str)> = sides.iter().map(|side| (*side, "true")).collect();
    with(Kind::NetherBrickFence, &properties)
}

fn fill(f: &mut Frame, from: Pos, to: Pos, state: BlockStateId) {
    f.bounds(from, to, state, state, false);
}

fn bricks(f: &mut Frame, from: Pos, to: Pos) {
    fill(f, from, to, brick());
}

fn clear(f: &mut Frame, from: Pos, to: Pos) {
    fill(f, from, to, air());
}

fn columns(f: &mut Frame, xs: std::ops::RangeInclusive<i32>, zs: std::ops::RangeInclusive<i32>) {
    for x in xs {
        for z in zs.clone() {
            f.column_down(brick(), x, -1, z);
        }
    }
}

impl Piece {
    pub(crate) fn place(&self, run: &mut Run, write: &Bbox) {
        let mut f = Frame::new(run, self.bbox, Some(self.facing), *write, Guard::Open);
        match self.part {
            Part::Start | Part::BridgeCrossing => bridge_crossing(&mut f),
            Part::BridgeStraight => bridge_straight(&mut f),
            Part::RoomCrossing => room_crossing(&mut f),
            Part::StairsRoom => stairs_room(&mut f),
            Part::MonsterThrone => monster_throne(&mut f),
            Part::CastleEntrance => castle_hall(&mut f, false),
            Part::StalkRoom => castle_hall(&mut f, true),
            Part::SmallCorridor => small_corridor(&mut f),
            Part::SmallCrossing => small_crossing(&mut f),
            Part::RightTurn { chest } => turn(&mut f, chest, true),
            Part::LeftTurn { chest } => turn(&mut f, chest, false),
            Part::CorridorStairs => corridor_stairs(&mut f),
            Part::TBalcony => t_balcony(&mut f),
            Part::EndFiller { seed } => end_filler(&mut f, seed),
        }
        f.finish();
    }
}

fn bridge_straight(f: &mut Frame) {
    bricks(f, (0, 3, 0), (4, 4, 18));
    clear(f, (1, 5, 0), (3, 7, 18));
    bricks(f, (0, 5, 0), (0, 5, 18));
    bricks(f, (4, 5, 0), (4, 5, 18));
    bricks(f, (0, 2, 0), (4, 2, 5));
    bricks(f, (0, 2, 13), (4, 2, 18));
    bricks(f, (0, 0, 0), (4, 1, 3));
    bricks(f, (0, 0, 15), (4, 1, 18));
    for x in 0..=4 {
        for z in 0..=2 {
            f.column_down(brick(), x, -1, z);
            f.column_down(brick(), x, -1, 18 - z);
        }
    }
    let east = fence(&["north", "south", "east"]);
    let west = fence(&["north", "south", "west"]);
    for (x, state) in [(0, east), (4, west)] {
        fill(f, (x, 1, 1), (x, 4, 1), state);
        fill(f, (x, 3, 4), (x, 4, 4), state);
        fill(f, (x, 3, 14), (x, 4, 14), state);
        fill(f, (x, 1, 17), (x, 4, 17), state);
    }
}

fn bridge_crossing(f: &mut Frame) {
    bricks(f, (7, 3, 0), (11, 4, 18));
    bricks(f, (0, 3, 7), (18, 4, 11));
    clear(f, (8, 5, 0), (10, 7, 18));
    clear(f, (0, 5, 8), (18, 7, 10));
    bricks(f, (7, 5, 0), (7, 5, 7));
    bricks(f, (7, 5, 11), (7, 5, 18));
    bricks(f, (11, 5, 0), (11, 5, 7));
    bricks(f, (11, 5, 11), (11, 5, 18));
    bricks(f, (0, 5, 7), (7, 5, 7));
    bricks(f, (11, 5, 7), (18, 5, 7));
    bricks(f, (0, 5, 11), (7, 5, 11));
    bricks(f, (11, 5, 11), (18, 5, 11));
    bricks(f, (7, 2, 0), (11, 2, 5));
    bricks(f, (7, 2, 13), (11, 2, 18));
    bricks(f, (7, 0, 0), (11, 1, 3));
    bricks(f, (7, 0, 15), (11, 1, 18));
    for x in 7..=11 {
        for z in 0..=2 {
            f.column_down(brick(), x, -1, z);
            f.column_down(brick(), x, -1, 18 - z);
        }
    }
    bricks(f, (0, 2, 7), (5, 2, 11));
    bricks(f, (13, 2, 7), (18, 2, 11));
    bricks(f, (0, 0, 7), (3, 1, 11));
    bricks(f, (15, 0, 7), (18, 1, 11));
    for x in 0..=2 {
        for z in 7..=11 {
            f.column_down(brick(), x, -1, z);
            f.column_down(brick(), 18 - x, -1, z);
        }
    }
}

fn room_crossing(f: &mut Frame) {
    bricks(f, (0, 0, 0), (6, 1, 6));
    clear(f, (0, 2, 0), (6, 7, 6));
    bricks(f, (0, 2, 0), (1, 6, 0));
    bricks(f, (0, 2, 6), (1, 6, 6));
    bricks(f, (5, 2, 0), (6, 6, 0));
    bricks(f, (5, 2, 6), (6, 6, 6));
    bricks(f, (0, 2, 0), (0, 6, 1));
    bricks(f, (0, 2, 5), (0, 6, 6));
    bricks(f, (6, 2, 0), (6, 6, 1));
    bricks(f, (6, 2, 5), (6, 6, 6));
    let west_east = fence(&["west", "east"]);
    let north_south = fence(&["north", "south"]);
    bricks(f, (2, 6, 0), (4, 6, 0));
    fill(f, (2, 5, 0), (4, 5, 0), west_east);
    bricks(f, (2, 6, 6), (4, 6, 6));
    fill(f, (2, 5, 6), (4, 5, 6), west_east);
    bricks(f, (0, 6, 2), (0, 6, 4));
    fill(f, (0, 5, 2), (0, 5, 4), north_south);
    bricks(f, (6, 6, 2), (6, 6, 4));
    fill(f, (6, 5, 2), (6, 5, 4), north_south);
    columns(f, 0..=6, 0..=6);
}

fn stairs_room(f: &mut Frame) {
    bricks(f, (0, 0, 0), (6, 1, 6));
    clear(f, (0, 2, 0), (6, 10, 6));
    bricks(f, (0, 2, 0), (1, 8, 0));
    bricks(f, (5, 2, 0), (6, 8, 0));
    bricks(f, (0, 2, 1), (0, 8, 6));
    bricks(f, (6, 2, 1), (6, 8, 6));
    bricks(f, (1, 2, 6), (5, 8, 6));
    let north_south = fence(&["north", "south"]);
    fill(f, (0, 3, 2), (0, 5, 4), north_south);
    fill(f, (6, 3, 2), (6, 5, 2), north_south);
    fill(f, (6, 3, 4), (6, 5, 4), north_south);
    f.place(brick(), 5, 2, 5);
    bricks(f, (4, 2, 5), (4, 3, 5));
    bricks(f, (3, 2, 5), (3, 4, 5));
    bricks(f, (2, 2, 5), (2, 5, 5));
    bricks(f, (1, 2, 5), (1, 6, 5));
    bricks(f, (1, 7, 1), (5, 7, 4));
    clear(f, (6, 8, 2), (6, 8, 4));
    bricks(f, (2, 6, 0), (4, 8, 0));
    fill(f, (2, 5, 0), (4, 5, 0), fence(&["west", "east"]));
    columns(f, 0..=6, 0..=6);
}

fn monster_throne(f: &mut Frame) {
    clear(f, (0, 2, 0), (6, 7, 7));
    bricks(f, (1, 0, 0), (5, 1, 7));
    bricks(f, (1, 2, 1), (5, 2, 7));
    bricks(f, (1, 3, 2), (5, 3, 7));
    bricks(f, (1, 4, 3), (5, 4, 7));
    bricks(f, (1, 2, 0), (1, 4, 2));
    bricks(f, (5, 2, 0), (5, 4, 2));
    bricks(f, (1, 5, 2), (1, 5, 3));
    bricks(f, (5, 5, 2), (5, 5, 3));
    bricks(f, (0, 5, 3), (0, 5, 8));
    bricks(f, (6, 5, 3), (6, 5, 8));
    bricks(f, (1, 5, 8), (5, 5, 8));
    f.place(fence(&["west"]), 1, 6, 3);
    f.place(fence(&["east"]), 5, 6, 3);
    f.place(fence(&["east", "north"]), 0, 6, 3);
    f.place(fence(&["west", "north"]), 6, 6, 3);
    let north_south = fence(&["north", "south"]);
    fill(f, (0, 6, 4), (0, 6, 7), north_south);
    fill(f, (6, 6, 4), (6, 6, 7), north_south);
    f.place(fence(&["east", "south"]), 0, 6, 8);
    f.place(fence(&["west", "south"]), 6, 6, 8);
    let west_east = fence(&["west", "east"]);
    fill(f, (1, 6, 8), (5, 6, 8), west_east);
    f.place(fence(&["east"]), 1, 7, 8);
    fill(f, (2, 7, 8), (4, 7, 8), west_east);
    f.place(fence(&["west"]), 5, 7, 8);
    f.place(fence(&["east"]), 2, 8, 8);
    f.place(west_east, 3, 8, 8);
    f.place(fence(&["west"]), 4, 8, 8);
    let at = f.world_pos(3, 5, 5);
    if f.chunk.contains(at) {
        f.spawner(at, "minecraft:blaze");
    }
    columns(f, 0..=6, 0..=6);
}

fn hall_walls(f: &mut Frame, entrance: bool) {
    bricks(f, (0, 3, 0), (12, 4, 12));
    clear(f, (0, 5, 0), (12, 13, 12));
    bricks(f, (0, 5, 0), (1, 12, 12));
    bricks(f, (11, 5, 0), (12, 12, 12));
    bricks(f, (2, 5, 11), (4, 12, 12));
    bricks(f, (8, 5, 11), (10, 12, 12));
    bricks(f, (5, 9, 11), (7, 12, 12));
    bricks(f, (2, 5, 0), (4, 12, 1));
    bricks(f, (8, 5, 0), (10, 12, 1));
    bricks(f, (5, 9, 0), (7, 12, 1));
    bricks(f, (2, 11, 2), (10, 12, 10));
    if entrance {
        fill(f, (5, 8, 0), (7, 8, 0), fence(&[]));
    }
    let west_east = fence(&["west", "east"]);
    let north_south = fence(&["north", "south"]);
    for i in (1..=11).step_by(2) {
        fill(f, (i, 10, 0), (i, 11, 0), west_east);
        fill(f, (i, 10, 12), (i, 11, 12), west_east);
        fill(f, (0, 10, i), (0, 11, i), north_south);
        fill(f, (12, 10, i), (12, 11, i), north_south);
        f.place(brick(), i, 13, 0);
        f.place(brick(), i, 13, 12);
        f.place(brick(), 0, 13, i);
        f.place(brick(), 12, 13, i);
        if i != 11 {
            f.place(west_east, i + 1, 13, 0);
            f.place(west_east, i + 1, 13, 12);
            f.place(north_south, 0, 13, i + 1);
            f.place(north_south, 12, 13, i + 1);
        }
    }
    f.place(fence(&["north", "east"]), 0, 13, 0);
    f.place(fence(&["south", "east"]), 0, 13, 12);
    f.place(fence(&["south", "west"]), 12, 13, 12);
    f.place(fence(&["north", "west"]), 12, 13, 0);
    let north_south_west = fence(&["north", "south", "west"]);
    let north_south_east = fence(&["north", "south", "east"]);
    for i in (3..=9).step_by(2) {
        fill(f, (1, 7, i), (1, 8, i), north_south_west);
        fill(f, (11, 7, i), (11, 8, i), north_south_east);
    }
}

fn hall_base(f: &mut Frame) {
    bricks(f, (4, 2, 0), (8, 2, 12));
    bricks(f, (0, 2, 4), (12, 2, 8));
    bricks(f, (4, 0, 0), (8, 1, 3));
    bricks(f, (4, 0, 9), (8, 1, 12));
    bricks(f, (0, 0, 4), (3, 1, 8));
    bricks(f, (9, 0, 4), (12, 1, 8));
    for x in 4..=8 {
        for z in 0..=2 {
            f.column_down(brick(), x, -1, z);
            f.column_down(brick(), x, -1, 12 - z);
        }
    }
    for x in 0..=2 {
        for z in 4..=8 {
            f.column_down(brick(), x, -1, z);
            f.column_down(brick(), 12 - x, -1, z);
        }
    }
}

fn castle_hall(f: &mut Frame, stalk: bool) {
    hall_walls(f, !stalk);
    if !stalk {
        hall_base(f);
        bricks(f, (5, 5, 5), (7, 5, 7));
        clear(f, (6, 1, 6), (6, 4, 6));
        f.place(brick(), 6, 0, 6);
        f.place(plain(Kind::Lava), 6, 5, 6);
        return;
    }
    let stairs = with(Kind::NetherBrickStairs, &[("facing", "north")]);
    for i in 0..=6 {
        let z = i + 4;
        for x in 5..=7 {
            f.place(stairs, x, 5 + i, z);
        }
        if (5..=8).contains(&z) {
            bricks(f, (5, 5, z), (7, i + 4, z));
        } else if (9..=10).contains(&z) {
            bricks(f, (5, 8, z), (7, i + 4, z));
        }
        if i >= 1 {
            clear(f, (5, 6 + i, z), (7, 9 + i, z));
        }
    }
    for x in 5..=7 {
        f.place(stairs, x, 12, 11);
    }
    fill(f, (5, 6, 7), (5, 7, 7), fence(&["north", "south", "east"]));
    fill(f, (7, 6, 7), (7, 7, 7), fence(&["north", "south", "west"]));
    clear(f, (5, 13, 12), (7, 13, 12));
    bricks(f, (2, 5, 2), (3, 5, 3));
    bricks(f, (2, 5, 9), (3, 5, 10));
    bricks(f, (2, 5, 4), (2, 5, 8));
    bricks(f, (9, 5, 2), (10, 5, 3));
    bricks(f, (9, 5, 9), (10, 5, 10));
    bricks(f, (10, 5, 4), (10, 5, 8));
    let post = fence(&["north", "west"]);
    for (x, z) in [
        (4, 2),
        (4, 3),
        (4, 9),
        (4, 10),
        (8, 2),
        (8, 3),
        (8, 9),
        (8, 10),
    ] {
        f.place(post, x, 5, z);
    }
    let sand = plain(Kind::SoulSand);
    let wart = plain(Kind::NetherWart);
    fill(f, (3, 4, 4), (4, 4, 8), sand);
    fill(f, (8, 4, 4), (9, 4, 8), sand);
    fill(f, (3, 5, 4), (4, 5, 8), wart);
    fill(f, (8, 5, 4), (9, 5, 8), wart);
    hall_base(f);
}

fn small_corridor(f: &mut Frame) {
    bricks(f, (0, 0, 0), (4, 1, 4));
    clear(f, (0, 2, 0), (4, 5, 4));
    bricks(f, (0, 2, 0), (0, 5, 4));
    bricks(f, (4, 2, 0), (4, 5, 4));
    let north_south = fence(&["north", "south"]);
    fill(f, (0, 3, 1), (0, 4, 1), north_south);
    fill(f, (0, 3, 3), (0, 4, 3), north_south);
    fill(f, (4, 3, 1), (4, 4, 1), north_south);
    fill(f, (4, 3, 3), (4, 4, 3), north_south);
    bricks(f, (0, 6, 0), (4, 6, 4));
    columns(f, 0..=4, 0..=4);
}

fn small_crossing(f: &mut Frame) {
    bricks(f, (0, 0, 0), (4, 1, 4));
    clear(f, (0, 2, 0), (4, 5, 4));
    bricks(f, (0, 2, 0), (0, 5, 0));
    bricks(f, (4, 2, 0), (4, 5, 0));
    bricks(f, (0, 2, 4), (0, 5, 4));
    bricks(f, (4, 2, 4), (4, 5, 4));
    bricks(f, (0, 6, 0), (4, 6, 4));
    columns(f, 0..=4, 0..=4);
}

fn turn(f: &mut Frame, chest: bool, right: bool) {
    bricks(f, (0, 0, 0), (4, 1, 4));
    clear(f, (0, 2, 0), (4, 5, 4));
    let north_south = fence(&["north", "south"]);
    let west_east = fence(&["west", "east"]);
    if right {
        bricks(f, (0, 2, 0), (0, 5, 4));
        fill(f, (0, 3, 1), (0, 4, 1), north_south);
        fill(f, (0, 3, 3), (0, 4, 3), north_south);
        bricks(f, (4, 2, 0), (4, 5, 0));
        bricks(f, (1, 2, 4), (4, 5, 4));
    } else {
        bricks(f, (4, 2, 0), (4, 5, 4));
        fill(f, (4, 3, 1), (4, 4, 1), north_south);
        fill(f, (4, 3, 3), (4, 4, 3), north_south);
        bricks(f, (0, 2, 0), (0, 5, 0));
        bricks(f, (0, 2, 4), (3, 5, 4));
    }
    fill(f, (1, 3, 4), (1, 4, 4), west_east);
    fill(f, (3, 3, 4), (3, 4, 4), west_east);
    if chest {
        f.chest(if right { 1 } else { 3 }, 2, 3, NETHER_BRIDGE);
    }
    bricks(f, (0, 6, 0), (4, 6, 4));
    columns(f, 0..=4, 0..=4);
}

fn corridor_stairs(f: &mut Frame) {
    let stairs = with(Kind::NetherBrickStairs, &[("facing", "south")]);
    let north_south = fence(&["north", "south"]);
    for z in 0..=9 {
        let floor = 1.max(7 - z);
        let ceiling = (floor + 5).max(14 - z).min(13);
        bricks(f, (0, 0, z), (4, floor, z));
        clear(f, (1, floor + 1, z), (3, ceiling - 1, z));
        if z <= 6 {
            for x in 1..=3 {
                f.place(stairs, x, floor + 1, z);
            }
        }
        bricks(f, (0, ceiling, z), (4, ceiling, z));
        bricks(f, (0, floor + 1, z), (0, ceiling - 1, z));
        bricks(f, (4, floor + 1, z), (4, ceiling - 1, z));
        if z & 1 == 0 {
            fill(f, (0, floor + 2, z), (0, floor + 3, z), north_south);
            fill(f, (4, floor + 2, z), (4, floor + 3, z), north_south);
        }
        for x in 0..=4 {
            f.column_down(brick(), x, -1, z);
        }
    }
}

fn t_balcony(f: &mut Frame) {
    bricks(f, (0, 0, 0), (8, 1, 8));
    clear(f, (0, 2, 0), (8, 5, 8));
    bricks(f, (0, 6, 0), (8, 6, 5));
    bricks(f, (0, 2, 0), (2, 5, 0));
    bricks(f, (6, 2, 0), (8, 5, 0));
    let west_east = fence(&["west", "east"]);
    let north_south = fence(&["north", "south"]);
    fill(f, (1, 3, 0), (1, 4, 0), west_east);
    fill(f, (7, 3, 0), (7, 4, 0), west_east);
    bricks(f, (0, 2, 4), (8, 2, 8));
    clear(f, (1, 1, 4), (2, 2, 4));
    clear(f, (6, 1, 4), (7, 2, 4));
    fill(f, (1, 3, 8), (7, 3, 8), west_east);
    f.place(fence(&["east", "south"]), 0, 3, 8);
    f.place(fence(&["west", "south"]), 8, 3, 8);
    fill(f, (0, 3, 6), (0, 3, 7), north_south);
    fill(f, (8, 3, 6), (8, 3, 7), north_south);
    bricks(f, (0, 3, 4), (0, 5, 5));
    bricks(f, (8, 3, 4), (8, 5, 5));
    bricks(f, (1, 3, 5), (2, 5, 5));
    bricks(f, (6, 3, 5), (7, 5, 5));
    fill(f, (1, 4, 5), (1, 5, 5), west_east);
    fill(f, (7, 4, 5), (7, 5, 5), west_east);
    columns(f, 0..=8, 0..=5);
}

fn end_filler(f: &mut Frame, seed: i32) {
    let mut random = JavaRandom::new(i64::from(seed));
    for x in 0..=4 {
        for y in 3..=4 {
            let depth = random.next_int(8);
            bricks(f, (x, y, 0), (x, y, depth));
        }
    }
    let depth = random.next_int(8);
    bricks(f, (0, 5, 0), (0, 5, depth));
    let depth = random.next_int(8);
    bricks(f, (4, 5, 0), (4, 5, depth));
    for x in 0..=4 {
        let depth = random.next_int(5);
        bricks(f, (x, 2, 0), (x, 2, depth));
    }
    for x in 0..=4 {
        for y in 0..=1 {
            let depth = random.next_int(3);
            bricks(f, (x, y, 0), (x, y, depth));
        }
    }
}
