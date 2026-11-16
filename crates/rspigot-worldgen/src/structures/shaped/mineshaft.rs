use std::sync::atomic::{AtomicBool, Ordering};

use rspigot_blocks::{BlockKind as Kind, Face, Support};
use rspigot_domain::BlockStateId;
use serde_json::json;

use super::frame::{
    Frame, Guard, Wood, cave_air, dir_name, offset, plain, replaceable_by_structures, with,
};
use crate::legacy::JavaRandom;
use crate::overworld::features::place::Run;
use crate::structures::bbox::{Bbox, Pos};
use crate::structures::blueprint::Dir;
use crate::structures::jigsaw::{Ground, Heightmap};
use crate::structures::placement::{BiomeProbe, large_feature_seed};

const LOOT: &str = "minecraft:chests/abandoned_mineshaft";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Type {
    Normal,
    Mesa,
}

impl Type {
    fn wood(self) -> Wood {
        match self {
            Self::Normal => Wood {
                planks: Kind::OakPlanks,
                log: Kind::OakLog,
                fence: Kind::OakFence,
            },
            Self::Mesa => Wood {
                planks: Kind::DarkOakPlanks,
                log: Kind::DarkOakLog,
                fence: Kind::DarkOakFence,
            },
        }
    }

    pub(crate) fn parse(name: &str) -> Self {
        if name == "mesa" {
            Self::Mesa
        } else {
            Self::Normal
        }
    }

    fn id(self) -> i32 {
        match self {
            Self::Normal => 0,
            Self::Mesa => 1,
        }
    }
}

#[derive(Debug)]
pub(crate) enum Part {
    Room {
        entrances: Vec<Bbox>,
    },
    Corridor {
        rails: bool,
        spider: bool,
        sections: i32,
        placed: AtomicBool,
    },
    Crossing {
        two_floored: bool,
        direction: Dir,
    },
    Stairs,
}

#[derive(Debug)]
pub(crate) struct Piece {
    pub(crate) part: Part,
    pub(crate) bbox: Bbox,
    pub(crate) facing: Option<Dir>,
    pub(crate) depth: i32,
    pub(crate) ty: Type,
}

impl Piece {
    #[cfg(test)]
    pub(crate) fn class(&self) -> &'static str {
        match self.part {
            Part::Room { .. } => "MineShaftRoom",
            Part::Corridor { .. } => "MineShaftCorridor",
            Part::Crossing { .. } => "MineShaftCrossing",
            Part::Stairs => "MineShaftStairs",
        }
    }

    pub(crate) fn id(&self) -> &'static str {
        match self.part {
            Part::Room { .. } => "MSRoom",
            Part::Corridor { .. } => "MSCorridor",
            Part::Crossing { .. } => "MSCrossing",
            Part::Stairs => "MSStairs",
        }
    }

    pub(crate) fn extra(&self) -> Vec<(&'static str, i32)> {
        let mut out = vec![("MST", self.ty.id())];
        match &self.part {
            Part::Corridor {
                rails,
                spider,
                sections,
                placed,
            } => {
                out.push(("hr", i32::from(*rails)));
                out.push(("sc", i32::from(*spider)));
                out.push(("hps", i32::from(placed.load(Ordering::Relaxed))));
                out.push(("Num", *sections));
            }
            Part::Crossing { two_floored, .. } => out.push(("tf", i32::from(*two_floored))),
            _ => {}
        }
        out
    }
}

fn shifted(bbox: (i32, i32, i32, i32, i32, i32), (x, y, z): Pos) -> Bbox {
    Bbox {
        min: (bbox.0 + x, bbox.1 + y, bbox.2 + z),
        max: (bbox.3 + x, bbox.4 + y, bbox.5 + z),
    }
}

fn collides(pieces: &[Piece], bbox: &Bbox) -> bool {
    pieces.iter().any(|piece| piece.bbox.intersects(bbox))
}

fn corridor_size(pieces: &[Piece], random: &mut JavaRandom, at: Pos, dir: Dir) -> Option<Bbox> {
    let mut length = random.next_int(3) + 2;
    while length >= 1 {
        let n = length * 5;
        let raw = match dir {
            Dir::South => (0, 0, 0, 2, 2, n - 1),
            Dir::West => (-(n - 1), 0, 0, 0, 2, 2),
            Dir::East => (0, 0, 0, n - 1, 2, 2),
            _ => (0, 0, -(n - 1), 2, 2, 0),
        };
        let bbox = shifted(raw, at);
        if !collides(pieces, &bbox) {
            return Some(bbox);
        }
        length -= 1;
    }
    None
}

fn crossing_size(pieces: &[Piece], random: &mut JavaRandom, at: Pos, dir: Dir) -> Option<Bbox> {
    let height = if random.next_int(4) == 0 { 6 } else { 2 };
    let raw = match dir {
        Dir::South => (-1, 0, 0, 3, height, 4),
        Dir::West => (-4, 0, -1, 0, height, 3),
        Dir::East => (0, 0, -1, 4, height, 3),
        _ => (-1, 0, -4, 3, height, 0),
    };
    let bbox = shifted(raw, at);
    (!collides(pieces, &bbox)).then_some(bbox)
}

fn stairs_size(pieces: &[Piece], at: Pos, dir: Dir) -> Option<Bbox> {
    let raw = match dir {
        Dir::South => (0, -5, 0, 2, 2, 8),
        Dir::West => (-8, -5, 0, 0, 2, 2),
        Dir::East => (0, -5, 0, 8, 2, 2),
        _ => (0, -5, -8, 2, 2, 0),
    };
    let bbox = shifted(raw, at);
    (!collides(pieces, &bbox)).then_some(bbox)
}

fn create(
    pieces: &[Piece],
    random: &mut JavaRandom,
    at: Pos,
    dir: Dir,
    depth: i32,
    ty: Type,
) -> Option<Piece> {
    let roll = random.next_int(100);
    if roll >= 80 {
        let bbox = crossing_size(pieces, random, at, dir)?;
        let two_floored = bbox.y_span() > 3;
        return Some(Piece {
            part: Part::Crossing {
                two_floored,
                direction: dir,
            },
            bbox,
            facing: None,
            depth,
            ty,
        });
    }
    if roll < 70 {
        let bbox = corridor_size(pieces, random, at, dir)?;
        let rails = random.next_int(3) == 0;
        let spider = !rails && random.next_int(23) == 0;
        let span = if matches!(dir, Dir::North | Dir::South) {
            bbox.max.2 - bbox.min.2 + 1
        } else {
            bbox.max.0 - bbox.min.0 + 1
        };
        return Some(Piece {
            part: Part::Corridor {
                rails,
                spider,
                sections: span / 5,
                placed: AtomicBool::new(false),
            },
            bbox,
            facing: Some(dir),
            depth,
            ty,
        });
    }
    let bbox = stairs_size(pieces, at, dir)?;
    Some(Piece {
        part: Part::Stairs,
        bbox,
        facing: Some(dir),
        depth,
        ty,
    })
}

struct Builder {
    pieces: Vec<Piece>,
    origin: Bbox,
    ty: Type,
}

impl Builder {
    fn spawn(&mut self, random: &mut JavaRandom, at: Pos, dir: Dir, depth: i32) -> Option<usize> {
        if depth > 8 {
            return None;
        }
        if (at.0 - self.origin.min.0).abs() > 80 || (at.2 - self.origin.min.2).abs() > 80 {
            return None;
        }
        let piece = create(&self.pieces, random, at, dir, depth + 1, self.ty)?;
        self.pieces.push(piece);
        let index = self.pieces.len() - 1;
        self.expand(index, random);
        Some(index)
    }

    fn expand(&mut self, index: usize, random: &mut JavaRandom) {
        let bbox = self.pieces[index].bbox;
        let depth = self.pieces[index].depth;
        let facing = self.pieces[index].facing;
        match &self.pieces[index].part {
            Part::Room { .. } => self.expand_room(index, random, bbox, depth),
            Part::Corridor { .. } => self.expand_corridor(random, bbox, depth, facing),
            Part::Crossing { direction, .. } => {
                let direction = *direction;
                self.expand_crossing(random, bbox, depth, direction);
            }
            Part::Stairs => {
                let (at, dir) = match facing.unwrap_or(Dir::North) {
                    Dir::South => ((bbox.min.0, bbox.min.1, bbox.max.2 + 1), Dir::South),
                    Dir::West => ((bbox.min.0 - 1, bbox.min.1, bbox.min.2), Dir::West),
                    Dir::East => ((bbox.max.0 + 1, bbox.min.1, bbox.min.2), Dir::East),
                    _ => ((bbox.min.0, bbox.min.1, bbox.min.2 - 1), Dir::North),
                };
                self.spawn(random, at, dir, depth);
            }
        }
    }

    fn expand_room(&mut self, index: usize, random: &mut JavaRandom, bbox: Bbox, depth: i32) {
        let x_span = bbox.max.0 - bbox.min.0 + 1;
        let z_span = bbox.max.2 - bbox.min.2 + 1;
        let height = (bbox.y_span() - 4).max(1);
        let mut found: Vec<Bbox> = Vec::new();
        let mut pos = 0;
        while pos < x_span {
            pos += random.next_int(x_span);
            if pos + 3 > x_span {
                break;
            }
            let y = bbox.min.1 + random.next_int(height) + 1;
            let child = self.spawn(
                random,
                (bbox.min.0 + pos, y, bbox.min.2 - 1),
                Dir::North,
                depth,
            );
            if let Some(child) = child {
                let c = self.pieces[child].bbox;
                found.push(Bbox {
                    min: (c.min.0, c.min.1, bbox.min.2),
                    max: (c.max.0, c.max.1, bbox.min.2 + 1),
                });
            }
            pos += 4;
        }
        pos = 0;
        while pos < x_span {
            pos += random.next_int(x_span);
            if pos + 3 > x_span {
                break;
            }
            let y = bbox.min.1 + random.next_int(height) + 1;
            let child = self.spawn(
                random,
                (bbox.min.0 + pos, y, bbox.max.2 + 1),
                Dir::South,
                depth,
            );
            if let Some(child) = child {
                let c = self.pieces[child].bbox;
                found.push(Bbox {
                    min: (c.min.0, c.min.1, bbox.max.2 - 1),
                    max: (c.max.0, c.max.1, bbox.max.2),
                });
            }
            pos += 4;
        }
        pos = 0;
        while pos < z_span {
            pos += random.next_int(z_span);
            if pos + 3 > z_span {
                break;
            }
            let y = bbox.min.1 + random.next_int(height) + 1;
            let child = self.spawn(
                random,
                (bbox.min.0 - 1, y, bbox.min.2 + pos),
                Dir::West,
                depth,
            );
            if let Some(child) = child {
                let c = self.pieces[child].bbox;
                found.push(Bbox {
                    min: (bbox.min.0, c.min.1, c.min.2),
                    max: (bbox.min.0 + 1, c.max.1, c.max.2),
                });
            }
            pos += 4;
        }
        pos = 0;
        while pos < z_span {
            pos += random.next_int(z_span);
            if pos + 3 > z_span {
                break;
            }
            let y = bbox.min.1 + random.next_int(height) + 1;
            let child = self.spawn(
                random,
                (bbox.max.0 + 1, y, bbox.min.2 + pos),
                Dir::East,
                depth,
            );
            if let Some(child) = child {
                let c = self.pieces[child].bbox;
                found.push(Bbox {
                    min: (bbox.max.0 - 1, c.min.1, c.min.2),
                    max: (bbox.max.0, c.max.1, c.max.2),
                });
            }
            pos += 4;
        }
        if let Part::Room { entrances } = &mut self.pieces[index].part {
            entrances.extend(found);
        }
    }

    fn expand_corridor(
        &mut self,
        random: &mut JavaRandom,
        bbox: Bbox,
        depth: i32,
        facing: Option<Dir>,
    ) {
        let choice = random.next_int(4);
        let Some(dir) = facing else {
            return;
        };
        let (b, y) = (bbox, bbox.min.1);
        match dir {
            Dir::South => {
                let r = random.next_int(3);
                if choice < 2 {
                    self.spawn(random, (b.min.0, y - 1 + r, b.max.2 + 1), dir, depth);
                } else if choice == 2 {
                    self.spawn(
                        random,
                        (b.min.0 - 1, y - 1 + r, b.max.2 - 3),
                        Dir::West,
                        depth,
                    );
                } else {
                    self.spawn(
                        random,
                        (b.max.0 + 1, y - 1 + r, b.max.2 - 3),
                        Dir::East,
                        depth,
                    );
                }
            }
            Dir::West => {
                let r = random.next_int(3);
                if choice < 2 {
                    self.spawn(random, (b.min.0 - 1, y - 1 + r, b.min.2), dir, depth);
                } else if choice == 2 {
                    self.spawn(random, (b.min.0, y - 1 + r, b.min.2 - 1), Dir::North, depth);
                } else {
                    self.spawn(random, (b.min.0, y - 1 + r, b.max.2 + 1), Dir::South, depth);
                }
            }
            Dir::East => {
                let r = random.next_int(3);
                if choice < 2 {
                    self.spawn(random, (b.max.0 + 1, y - 1 + r, b.min.2), dir, depth);
                } else if choice == 2 {
                    self.spawn(
                        random,
                        (b.max.0 - 3, y - 1 + r, b.min.2 - 1),
                        Dir::North,
                        depth,
                    );
                } else {
                    self.spawn(
                        random,
                        (b.max.0 - 3, y - 1 + r, b.max.2 + 1),
                        Dir::South,
                        depth,
                    );
                }
            }
            _ => {
                let r = random.next_int(3);
                if choice < 2 {
                    self.spawn(random, (b.min.0, y - 1 + r, b.min.2 - 1), dir, depth);
                } else if choice == 2 {
                    self.spawn(random, (b.min.0 - 1, y - 1 + r, b.min.2), Dir::West, depth);
                } else {
                    self.spawn(random, (b.max.0 + 1, y - 1 + r, b.min.2), Dir::East, depth);
                }
            }
        }
        if depth < 8 {
            if matches!(dir, Dir::North | Dir::South) {
                let mut z = b.min.2 + 3;
                while z + 3 <= b.max.2 {
                    match random.next_int(5) {
                        0 => {
                            self.spawn(random, (b.min.0 - 1, y, z), Dir::West, depth + 1);
                        }
                        1 => {
                            self.spawn(random, (b.max.0 + 1, y, z), Dir::East, depth + 1);
                        }
                        _ => {}
                    }
                    z += 5;
                }
            } else {
                let mut x = b.min.0 + 3;
                while x + 3 <= b.max.0 {
                    match random.next_int(5) {
                        0 => {
                            self.spawn(random, (x, y, b.min.2 - 1), Dir::North, depth + 1);
                        }
                        1 => {
                            self.spawn(random, (x, y, b.max.2 + 1), Dir::South, depth + 1);
                        }
                        _ => {}
                    }
                    x += 5;
                }
            }
        }
    }

    fn expand_crossing(&mut self, random: &mut JavaRandom, b: Bbox, depth: i32, dir: Dir) {
        let y = b.min.1;
        let sides: [(Pos, Dir); 3] = match dir {
            Dir::South => [
                ((b.min.0 + 1, y, b.max.2 + 1), Dir::South),
                ((b.min.0 - 1, y, b.min.2 + 1), Dir::West),
                ((b.max.0 + 1, y, b.min.2 + 1), Dir::East),
            ],
            Dir::West => [
                ((b.min.0 + 1, y, b.min.2 - 1), Dir::North),
                ((b.min.0 + 1, y, b.max.2 + 1), Dir::South),
                ((b.min.0 - 1, y, b.min.2 + 1), Dir::West),
            ],
            Dir::East => [
                ((b.min.0 + 1, y, b.min.2 - 1), Dir::North),
                ((b.min.0 + 1, y, b.max.2 + 1), Dir::South),
                ((b.max.0 + 1, y, b.min.2 + 1), Dir::East),
            ],
            _ => [
                ((b.min.0 + 1, y, b.min.2 - 1), Dir::North),
                ((b.min.0 - 1, y, b.min.2 + 1), Dir::West),
                ((b.max.0 + 1, y, b.min.2 + 1), Dir::East),
            ],
        };
        for (at, dir) in sides {
            self.spawn(random, at, dir, depth);
        }
        if b.y_span() > 3 {
            let upper: [(Pos, Dir); 4] = [
                ((b.min.0 + 1, y + 4, b.min.2 - 1), Dir::North),
                ((b.min.0 - 1, y + 4, b.min.2 + 1), Dir::West),
                ((b.max.0 + 1, y + 4, b.min.2 + 1), Dir::East),
                ((b.min.0 + 1, y + 4, b.max.2 + 1), Dir::South),
            ];
            for (at, dir) in upper {
                if random.next_boolean() {
                    self.spawn(random, at, dir, depth);
                }
            }
        }
    }
}

pub(crate) fn generate<G: Ground>(
    ty: Type,
    ground: &G,
    valid: &[u8],
    probe: &impl BiomeProbe,
    seed: i64,
    chunk: (i32, i32),
) -> Option<Vec<Piece>> {
    let mut random = JavaRandom::new(0);
    large_feature_seed(&mut random, seed, chunk.0, chunk.1);
    random.next_double();
    let (x, z) = (chunk.0 * 16 + 2, chunk.1 * 16 + 2);
    let room = Bbox {
        min: (x, 50, z),
        max: (0, 0, 0),
    };
    let rx = random.next_int(6);
    let ry = random.next_int(6);
    let rz = random.next_int(6);
    let room = Bbox {
        max: (x + 7 + rx, ry + 54, z + 7 + rz),
        ..room
    };
    let mut builder = Builder {
        pieces: vec![Piece {
            part: Part::Room {
                entrances: Vec::new(),
            },
            bbox: room,
            facing: None,
            depth: 0,
            ty,
        }],
        origin: room,
        ty,
    };
    builder.expand(0, &mut random);
    let mut pieces = builder.pieces;
    let mut whole = pieces[0].bbox;
    for piece in &pieces {
        whole.encapsulate(&piece.bbox);
    }
    let sea = ground.sea_level();
    let dy = match ty {
        Type::Normal => {
            let limit = sea - 10;
            let mut low = whole.y_span() + ground.min_y() + 1;
            if low < limit {
                low += random.next_int(limit - low);
            }
            low - whole.max.1
        }
        Type::Mesa => {
            let cx = whole.min.0 + (whole.max.0 - whole.min.0 + 1) / 2;
            let cz = whole.min.2 + (whole.max.2 - whole.min.2 + 1) / 2;
            let cy = whole.min.1 + (whole.max.1 - whole.min.1 + 1) / 2;
            let top = ground.first_free_height(cx, cz, Heightmap::WorldSurfaceWg);
            let mut level = sea;
            if level < top {
                level = random.next_int(top - level + 1) + level;
            }
            level - cy
        }
    };
    for piece in &mut pieces {
        piece.bbox = piece.bbox.moved(0, dy, 0);
        if let Part::Room { entrances } = &mut piece.part {
            for entrance in entrances {
                *entrance = entrance.moved(0, dy, 0);
            }
        }
    }
    let at = (chunk.0 * 16 + 8, 50 + dy, chunk.1 * 16);
    valid
        .contains(&probe.at(at.0 >> 2, at.1 >> 2, at.2 >> 2))
        .then_some(pieces)
}

fn is_falling(kind: Kind) -> bool {
    let name = kind.name();
    matches!(name, "sand" | "red_sand" | "gravel" | "dragon_egg")
        || name.ends_with("concrete_powder")
        || name.ends_with("anvil")
}

impl Piece {
    pub(crate) fn place(&self, run: &mut Run, write: &Bbox) {
        let wood = self.ty.wood();
        let mut frame = Frame::new(run, self.bbox, self.facing, *write, Guard::Wood(wood));
        if !self.invalid(&mut frame) {
            match &self.part {
                Part::Room { entrances } => room(&mut frame, entrances),
                Part::Corridor {
                    rails,
                    spider,
                    sections,
                    placed,
                } => corridor(&mut frame, wood, *rails, *spider, *sections, placed),
                Part::Crossing { two_floored, .. } => crossing(&mut frame, wood, *two_floored),
                Part::Stairs => stairs(&mut frame),
            }
        }
        frame.finish();
    }

    fn invalid(&self, frame: &mut Frame) -> bool {
        let (b, c) = (&self.bbox, &frame.chunk);
        let lo = (
            (b.min.0 - 1).max(c.min.0),
            (b.min.1 - 1).max(c.min.1),
            (b.min.2 - 1).max(c.min.2),
        );
        let hi = (
            (b.max.0 + 1).min(c.max.0),
            (b.max.1 + 1).min(c.max.1),
            (b.max.2 + 1).min(c.max.2),
        );
        let center = ((lo.0 + hi.0) / 2, (lo.1 + hi.1) / 2, (lo.2 + hi.2) / 2);
        let biome = frame.run.region.biome(center.0, center.1, center.2);
        if rspigot_data::multi_noise::overworld_biomes()
            .get(usize::from(biome))
            .is_some_and(|name| name.strip_prefix("minecraft:") == Some("deep_dark"))
        {
            return true;
        }
        let wet = |frame: &mut Frame, at: Pos| {
            let state = frame.state_at(at);
            state.flags().has_fluid() || state.kind() == Some(Kind::BubbleColumn)
        };
        for x in lo.0..=hi.0 {
            for y in lo.1..=hi.1 {
                if wet(frame, (x, y, lo.2)) || wet(frame, (x, y, hi.2)) {
                    return true;
                }
            }
            for z in lo.2..=hi.2 {
                if wet(frame, (x, lo.1, z)) || wet(frame, (x, hi.1, z)) {
                    return true;
                }
            }
        }
        for z in lo.2..=hi.2 {
            for y in lo.1..=hi.1 {
                if wet(frame, (lo.0, y, z)) || wet(frame, (hi.0, y, z)) {
                    return true;
                }
            }
        }
        false
    }
}

fn set_planks(frame: &mut Frame, planks: BlockStateId, x: i32, y: i32, z: i32) {
    if frame.is_interior(x, y, z) {
        let at = frame.world_pos(x, y, z);
        let held = frame.state_at(at);
        if !held.is_face_sturdy(Face::Up, Support::Full) {
            frame.set_raw(at, planks);
        }
    }
}

fn room(frame: &mut Frame, entrances: &[Bbox]) {
    let b = frame.bbox;
    let air = cave_air();
    frame.bounds(
        (b.min.0, b.min.1 + 1, b.min.2),
        (b.max.0, (b.min.1 + 3).min(b.max.1), b.max.2),
        air,
        air,
        false,
    );
    for e in entrances {
        frame.bounds(
            (e.min.0, e.max.1 - 2, e.min.2),
            (e.max.0, e.max.1, e.max.2),
            air,
            air,
            false,
        );
    }
    frame.upper_half_sphere(
        (b.min.0, b.min.1 + 4, b.min.2),
        (b.max.0, b.max.1, b.max.2),
        air,
        false,
    );
}

fn stairs(frame: &mut Frame) {
    let air = cave_air();
    frame.bounds((0, 5, 0), (2, 7, 1), air, air, false);
    frame.bounds((0, 0, 7), (2, 2, 8), air, air, false);
    for i in 0..5 {
        let low = 5 - i - i32::from(i < 4);
        frame.bounds((0, low, 2 + i), (2, 7 - i, 2 + i), air, air, false);
    }
}

fn crossing(frame: &mut Frame, wood: Wood, two_floored: bool) {
    let b = frame.bbox;
    let air = cave_air();
    let planks = plain(wood.planks);
    if !two_floored {
        frame.bounds(
            (b.min.0 + 1, b.min.1, b.min.2),
            (b.max.0 - 1, b.max.1, b.max.2),
            air,
            air,
            false,
        );
        frame.bounds(
            (b.min.0, b.min.1, b.min.2 + 1),
            (b.max.0, b.max.1, b.max.2 - 1),
            air,
            air,
            false,
        );
    } else {
        frame.bounds(
            (b.min.0 + 1, b.min.1, b.min.2),
            (b.max.0 - 1, b.min.1 + 2, b.max.2),
            air,
            air,
            false,
        );
        frame.bounds(
            (b.min.0, b.min.1, b.min.2 + 1),
            (b.max.0, b.min.1 + 2, b.max.2 - 1),
            air,
            air,
            false,
        );
        frame.bounds(
            (b.min.0 + 1, b.max.1 - 2, b.min.2),
            (b.max.0 - 1, b.max.1, b.max.2),
            air,
            air,
            false,
        );
        frame.bounds(
            (b.min.0, b.max.1 - 2, b.min.2 + 1),
            (b.max.0, b.max.1, b.max.2 - 1),
            air,
            air,
            false,
        );
        frame.bounds(
            (b.min.0 + 1, b.min.1 + 3, b.min.2 + 1),
            (b.max.0 - 1, b.min.1 + 3, b.max.2 - 1),
            air,
            air,
            false,
        );
    }
    let pillars = [
        (b.min.0 + 1, b.min.2 + 1),
        (b.min.0 + 1, b.max.2 - 1),
        (b.max.0 - 1, b.min.2 + 1),
        (b.max.0 - 1, b.max.2 - 1),
    ];
    for (x, z) in pillars {
        if !frame.block(x, b.max.1 + 1, z).is_air() {
            frame.bounds((x, b.min.1, z), (x, b.max.1, z), planks, air, false);
        }
    }
    let y = b.min.1 - 1;
    for x in b.min.0..=b.max.0 {
        for z in b.min.2..=b.max.2 {
            set_planks(frame, planks, x, y, z);
        }
    }
}

fn has_sturdy_neighbours(frame: &mut Frame, x: i32, y: i32, z: i32, count: i32) -> bool {
    let at = frame.world_pos(x, y, z);
    let mut found = 0;
    for dir in [
        Dir::Down,
        Dir::Up,
        Dir::North,
        Dir::South,
        Dir::West,
        Dir::East,
    ] {
        let near = offset(at, dir);
        if frame.chunk.contains(near) && frame.sturdy(near, dir.opposite()) {
            found += 1;
            if found >= count {
                return true;
            }
        }
    }
    false
}

fn cobweb(frame: &mut Frame, probability: f32, x: i32, y: i32, z: i32) {
    if frame.is_interior(x, y, z)
        && frame.run.rng.next_float() < probability
        && has_sturdy_neighbours(frame, x, y, z, 2)
    {
        frame.place(plain(Kind::Cobweb), x, y, z);
    }
}

fn support(frame: &mut Frame, wood: Wood, x0: i32, y0: i32, z: i32, y1: i32, x1: i32) {
    for x in x0..=x1 {
        if frame.block(x, y1 + 1, z).is_air() {
            return;
        }
    }
    let planks = plain(wood.planks);
    let air = cave_air();
    frame.bounds(
        (x0, y0, z),
        (x0, y1 - 1, z),
        with(wood.fence, &[("west", "true")]),
        air,
        false,
    );
    frame.bounds(
        (x1, y0, z),
        (x1, y1 - 1, z),
        with(wood.fence, &[("east", "true")]),
        air,
        false,
    );
    if frame.run.rng.below(4) == 0 {
        frame.bounds((x0, y1, z), (x0, y1, z), planks, air, false);
        frame.bounds((x1, y1, z), (x1, y1, z), planks, air, false);
    } else {
        frame.bounds((x0, y1, z), (x1, y1, z), planks, air, false);
        let south = with(Kind::WallTorch, &[("facing", dir_name(Dir::South))]);
        frame.maybe_block(0.05, (x0 + 1, y1, z - 1), south);
        let north = with(Kind::WallTorch, &[("facing", dir_name(Dir::North))]);
        frame.maybe_block(0.05, (x0 + 1, y1, z + 1), north);
    }
}

fn minecart(frame: &mut Frame, x: i32, y: i32, z: i32) {
    let at = frame.world_pos(x, y, z);
    if !frame.chunk.contains(at) || !frame.state_at(at).is_air() {
        return;
    }
    if frame.state_at(offset(at, Dir::Down)).is_air() {
        return;
    }
    let shape = if frame.run.rng.next_boolean() {
        "north_south"
    } else {
        "east_west"
    };
    frame.place(with(Kind::Rail, &[("shape", shape)]), x, y, z);
    let seed = frame.loot_seed();
    frame.run.region.entities.push(json!({
        "id": "minecraft:chest_minecart",
        "Pos": [f64::from(at.0) + 0.5, f64::from(at.1) + 0.5, f64::from(at.2) + 0.5],
        "Motion": [0.0, 0.0, 0.0],
        "Rotation": [0.0f32, 0.0f32],
        "LootTable": LOOT,
        "LootTableSeed": seed,
    }));
}

fn pillar(frame: &mut Frame, wood: Wood, x: i32, y: i32, z: i32) {
    let mut at = frame.world_pos(x, y, z);
    if !frame.chunk.contains(at) {
        return;
    }
    let base = at.1;
    let (floor, ceiling) = (
        frame.run.region.height().min_y,
        frame.run.region.height().max_y(),
    );
    let mut down = true;
    let mut up = true;
    let mut i = 1;
    while down || up {
        if down {
            at.1 = base - i;
            let held = frame.state_at(at);
            down = replaceable_by_structures(held) && held.kind() != Some(Kind::Lava);
            if !down && held.is_face_sturdy(Face::Up, Support::Full) {
                for yy in (base - i + 1)..base {
                    frame.set_raw((at.0, yy, at.2), plain(wood.log));
                }
                return;
            }
            down = down && i < 21 && floor + 1 < at.1;
        }
        if up {
            at.1 = base + i;
            let held = frame.state_at(at);
            let free = replaceable_by_structures(held);
            if !free
                && held.is_face_sturdy(Face::Down, Support::Center)
                && !held.kind().is_some_and(is_falling)
            {
                frame.set_raw((at.0, base + 1, at.2), plain(wood.fence));
                for yy in (base + 2)..(base + i) {
                    frame.set_raw((at.0, yy, at.2), plain(Kind::IronChain));
                }
                return;
            }
            up = free && i < 51 && at.1 < ceiling;
        }
        i += 1;
    }
}

fn double_support(frame: &mut Frame, wood: Wood, x: i32, y: i32, z: i32) {
    for dx in [0, 2] {
        if frame.block(x + dx, y, z).kind() == Some(wood.planks) {
            pillar(frame, wood, x + dx, y, z);
        }
    }
}

fn corridor(
    frame: &mut Frame,
    wood: Wood,
    rails: bool,
    spider: bool,
    sections: i32,
    placed: &AtomicBool,
) {
    let air = cave_air();
    let planks = plain(wood.planks);
    let length = sections * 5 - 1;
    frame.bounds((0, 0, 0), (2, 1, length), air, air, false);
    frame.maybe_bounds(0.8, (0, 2, 0), (2, 2, length), air, air, false, false);
    if spider {
        frame.maybe_bounds(
            0.6,
            (0, 0, 0),
            (2, 1, length),
            plain(Kind::Cobweb),
            air,
            false,
            true,
        );
    }
    for i in 0..sections {
        let z = i * 5 + 2;
        support(frame, wood, 0, 0, z, 2, 2);
        for (p, x, dz) in [
            (0.1, 0, -1),
            (0.1, 2, -1),
            (0.1, 0, 1),
            (0.1, 2, 1),
            (0.05, 0, -2),
            (0.05, 2, -2),
            (0.05, 0, 2),
            (0.05, 2, 2),
        ] {
            cobweb(frame, p, x, 2, z + dz);
        }
        if frame.run.rng.below(100) == 0 {
            minecart(frame, 2, 0, z - 1);
        }
        if frame.run.rng.below(100) == 0 {
            minecart(frame, 0, 0, z + 1);
        }
        if spider && !placed.load(Ordering::Relaxed) {
            let zz = z - 1 + frame.run.rng.below(3);
            let at = frame.world_pos(1, 0, zz);
            if frame.chunk.contains(at) && frame.is_interior(1, 0, zz) {
                placed.store(true, Ordering::Relaxed);
                frame.spawner(at, "minecraft:cave_spider");
            }
        }
    }
    for x in 0..3 {
        for z in 0..=length {
            set_planks(frame, planks, x, -1, z);
        }
    }
    double_support(frame, wood, 0, -1, 2);
    if sections > 1 {
        double_support(frame, wood, 0, -1, length - 2);
    }
    if rails {
        let rail = with(Kind::Rail, &[("shape", "north_south")]);
        for z in 0..=length {
            let below = frame.block(1, -1, z);
            if !below.is_air()
                && below
                    .flags()
                    .contains(rspigot_blocks::StateFlags::SOLID_RENDER)
            {
                let chance = if frame.is_interior(1, 0, z) { 0.7 } else { 0.9 };
                frame.maybe_block(chance, (1, 0, z), rail);
            }
        }
    }
}
