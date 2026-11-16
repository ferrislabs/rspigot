use std::sync::Arc;

use rspigot_blocks::BlockKind as Kind;
use rspigot_domain::BlockStateId;
use serde_json::json;

use super::frame::{Frame, Guard, plain, world_of};
use super::scatter::make_bbox;
use super::start;
use crate::legacy::JavaRandom;
use crate::overworld::features::place::Run;
use crate::structures::bbox::{Bbox, Pos};
use crate::structures::blueprint::Dir;
use crate::structures::jigsaw::{Ground, Heightmap as Map};
use crate::structures::placement::BiomeProbe;

const DOWN: usize = 0;
const UP: usize = 1;
const NORTH: usize = 2;
const SOUTH: usize = 3;
const WEST: usize = 4;
const EAST: usize = 5;

const SURROUNDING: [&str; 11] = [
    "deep_frozen_ocean",
    "deep_cold_ocean",
    "deep_ocean",
    "deep_lukewarm_ocean",
    "frozen_ocean",
    "ocean",
    "cold_ocean",
    "lukewarm_ocean",
    "warm_ocean",
    "river",
    "frozen_river",
];

const HORIZONTAL: [Dir; 4] = [Dir::North, Dir::East, Dir::South, Dir::West];

#[derive(Debug, Clone)]
struct Room {
    index: i32,
    connections: [Option<usize>; 6],
    open: [bool; 6],
    claimed: bool,
    source: bool,
    scan: i32,
}

#[derive(Debug, Default)]
pub(crate) struct Graph {
    rooms: Vec<Room>,
}

fn room_index(x: i32, y: i32, z: i32) -> i32 {
    y * 25 + z * 5 + x
}

const STEPS: [(i32, i32, i32); 6] = [
    (0, -1, 0),
    (0, 1, 0),
    (0, 0, -1),
    (0, 0, 1),
    (-1, 0, 0),
    (1, 0, 0),
];

impl Graph {
    fn add(&mut self, index: i32) -> usize {
        self.rooms.push(Room {
            index,
            connections: [None; 6],
            open: [false; 6],
            claimed: false,
            source: false,
            scan: 0,
        });
        self.rooms.len() - 1
    }

    fn connect(&mut self, from: usize, dir: usize, to: usize) {
        self.rooms[from].connections[dir] = Some(to);
        self.rooms[to].connections[dir ^ 1] = Some(from);
    }

    fn update_openings(&mut self, room: usize) {
        for dir in 0..6 {
            self.rooms[room].open[dir] = self.rooms[room].connections[dir].is_some();
        }
    }

    fn find_source(&mut self, room: usize, scan: i32) -> bool {
        if self.rooms[room].source {
            return true;
        }
        self.rooms[room].scan = scan;
        for dir in 0..6 {
            if let Some(next) = self.rooms[room].connections[dir]
                && self.rooms[room].open[dir]
                && self.rooms[next].scan != scan
                && self.find_source(next, scan)
            {
                return true;
            }
        }
        false
    }

    fn special(&self, room: usize) -> bool {
        self.rooms[room].index >= 75
    }

    fn count_openings(&self, room: usize) -> i32 {
        self.rooms[room].open.iter().filter(|open| **open).count() as i32
    }

    fn open(&self, room: usize, dir: usize) -> bool {
        self.rooms[room].open[dir]
    }

    fn linked(&self, room: usize, dir: usize) -> bool {
        self.rooms[room].connections[dir].is_some()
    }

    fn near(&self, room: usize, dir: usize) -> usize {
        self.rooms[room].connections[dir].unwrap_or(room)
    }

    fn index(&self, room: usize) -> i32 {
        self.rooms[room].index
    }

    fn claim(&mut self, room: usize) {
        self.rooms[room].claimed = true;
    }

    fn claimed_near(&self, room: usize, dir: usize) -> bool {
        self.rooms[room].connections[dir].is_some_and(|next| self.rooms[next].claimed)
    }
}

struct Layout {
    graph: Graph,
    source: usize,
    core: usize,
    order: Vec<usize>,
}

fn shuffle(list: &mut [usize], random: &mut JavaRandom) {
    for i in (2..=list.len()).rev() {
        let j = random.next_int(i as i32) as usize;
        list.swap(i - 1, j);
    }
}

fn layout(random: &mut JavaRandom) -> Layout {
    let mut graph = Graph::default();
    let mut grid: [Option<usize>; 75] = [None; 75];
    let mut make = |graph: &mut Graph, x: i32, y: i32, z: i32| {
        let index = room_index(x, y, z);
        grid[index as usize] = Some(graph.add(index));
    };
    for x in 0..5 {
        for z in 0..4 {
            make(&mut graph, x, 0, z);
        }
    }
    for x in 0..5 {
        for z in 0..4 {
            make(&mut graph, x, 1, z);
        }
    }
    for x in 1..4 {
        for z in 0..2 {
            make(&mut graph, x, 2, z);
        }
    }
    let at = |x: i32, y: i32, z: i32| grid[room_index(x, y, z) as usize];
    let source = at(2, 0, 0).unwrap_or(0);
    for x in 0..5 {
        for z in 0..5 {
            for y in 0..3 {
                let Some(room) = at(x, y, z) else { continue };
                for (dir, (dx, dy, dz)) in STEPS.iter().enumerate() {
                    let (nx, ny, nz) = (x + dx, y + dy, z + dz);
                    if !(0..5).contains(&nx) || !(0..5).contains(&nz) || !(0..3).contains(&ny) {
                        continue;
                    }
                    let Some(next) = at(nx, ny, nz) else { continue };
                    if nz == z {
                        graph.connect(room, dir, next);
                    } else {
                        graph.connect(room, dir ^ 1, next);
                    }
                }
            }
        }
    }
    let top = graph.add(1003);
    let left = graph.add(1001);
    let right = graph.add(1002);
    graph.connect(at(2, 2, 0).unwrap_or(0), UP, top);
    graph.connect(at(0, 1, 0).unwrap_or(0), SOUTH, left);
    graph.connect(at(4, 1, 0).unwrap_or(0), SOUTH, right);
    for room in [top, left, right] {
        graph.claim(room);
    }
    graph.rooms[source].source = true;
    let core = at(random.next_int(4), 0, 2).unwrap_or(0);
    graph.claim(core);
    let east = graph.near(core, EAST);
    let north = graph.near(core, NORTH);
    graph.claim(east);
    graph.claim(north);
    let east_north = graph.near(east, NORTH);
    graph.claim(east_north);
    let up = graph.near(core, UP);
    graph.claim(up);
    let east_up = graph.near(east, UP);
    graph.claim(east_up);
    let north_up = graph.near(north, UP);
    graph.claim(north_up);
    let east_north_up = graph.near(east_north, UP);
    graph.claim(east_north_up);
    let mut order = Vec::new();
    for room in grid.iter().flatten() {
        graph.update_openings(*room);
        order.push(*room);
    }
    graph.update_openings(top);
    shuffle(&mut order, random);
    let mut scan = 1;
    for &room in &order {
        let mut done = 0;
        let mut tries = 0;
        while done < 2 && tries < 5 {
            tries += 1;
            let dir = random.next_int(6) as usize;
            if !graph.rooms[room].open[dir] {
                continue;
            }
            let opposite = dir ^ 1;
            let Some(next) = graph.rooms[room].connections[dir] else {
                continue;
            };
            graph.rooms[room].open[dir] = false;
            graph.rooms[next].open[opposite] = false;
            scan += 1;
            if graph.find_source(room, scan) {
                scan += 1;
                if graph.find_source(next, scan) {
                    done += 1;
                    continue;
                }
            }
            graph.rooms[room].open[dir] = true;
            graph.rooms[next].open[opposite] = true;
        }
    }
    order.push(top);
    order.push(left);
    order.push(right);
    Layout {
        graph,
        source,
        core,
        order,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Part {
    Entry,
    Core,
    SimpleTop,
    Simple(i32),
    DoubleX,
    DoubleXY,
    DoubleY,
    DoubleYZ,
    DoubleZ,
    Wing(i32),
    Penthouse,
}

impl Part {
    #[cfg(test)]
    fn class(self) -> &'static str {
        match self {
            Self::Entry => "OceanMonumentEntryRoom",
            Self::Core => "OceanMonumentCoreRoom",
            Self::SimpleTop => "OceanMonumentSimpleTopRoom",
            Self::Simple(_) => "OceanMonumentSimpleRoom",
            Self::DoubleX => "OceanMonumentDoubleXRoom",
            Self::DoubleXY => "OceanMonumentDoubleXYRoom",
            Self::DoubleY => "OceanMonumentDoubleYRoom",
            Self::DoubleYZ => "OceanMonumentDoubleYZRoom",
            Self::DoubleZ => "OceanMonumentDoubleZRoom",
            Self::Wing(_) => "OceanMonumentWingRoom",
            Self::Penthouse => "OceanMonumentPenthouse",
        }
    }

    fn size(self) -> (i32, i32, i32) {
        match self {
            Self::DoubleX => (2, 1, 1),
            Self::DoubleXY => (2, 2, 1),
            Self::DoubleY => (1, 2, 1),
            Self::DoubleYZ => (1, 2, 2),
            Self::DoubleZ => (1, 1, 2),
            Self::Core => (2, 2, 2),
            _ => (1, 1, 1),
        }
    }
}

#[derive(Debug)]
pub(crate) struct Child {
    part: Part,
    room: usize,
    bbox: Bbox,
}

#[derive(Debug)]
pub(crate) struct Piece {
    pub(crate) bbox: Bbox,
    pub(crate) facing: Dir,
    graph: Arc<Graph>,
    children: Vec<Child>,
}

fn room_bbox(facing: Dir, index: i32, size: (i32, i32, i32)) -> Bbox {
    let (xs, ys, zs) = size;
    let (j, k, l) = (index % 5, index / 5 % 5, index / 25);
    let base = make_bbox((0, 0, 0), facing, (xs * 8, ys * 4, zs * 8));
    match facing {
        Dir::North => base.moved(j * 8, l * 4, -(k + zs) * 8 + 1),
        Dir::South => base.moved(j * 8, l * 4, k * 8),
        Dir::West => base.moved(-(k + zs) * 8 + 1, l * 4, j * 8),
        _ => base.moved(k * 8, l * 4, j * 8),
    }
}

fn fit(graph: &mut Graph, room: usize, random: &mut JavaRandom) -> (Part, usize) {
    let free = |graph: &Graph, room: usize, dir: usize| {
        graph.open(room, dir) && !graph.claimed_near(room, dir)
    };
    let chain = |graph: &Graph, room: usize, first: usize, second: usize| {
        if !(free(graph, room, first) && free(graph, room, second)) {
            return false;
        }
        let side = graph.near(room, first);
        free(graph, side, second)
    };
    if chain(graph, room, EAST, UP) {
        let east = graph.near(room, EAST);
        let up = graph.near(room, UP);
        let east_up = graph.near(east, UP);
        for claimed in [room, east, up, east_up] {
            graph.claim(claimed);
        }
        return (Part::DoubleXY, room);
    }
    if chain(graph, room, NORTH, UP) {
        let north = graph.near(room, NORTH);
        let up = graph.near(room, UP);
        let north_up = graph.near(north, UP);
        for claimed in [room, north, up, north_up] {
            graph.claim(claimed);
        }
        return (Part::DoubleYZ, room);
    }
    if free(graph, room, NORTH) {
        let mut base = room;
        if !graph.open(room, NORTH) || graph.claimed_near(room, NORTH) {
            base = graph.near(room, SOUTH);
        }
        graph.claim(base);
        let north = graph.near(base, NORTH);
        graph.claim(north);
        return (Part::DoubleZ, base);
    }
    if free(graph, room, EAST) {
        let east = graph.near(room, EAST);
        graph.claim(room);
        graph.claim(east);
        return (Part::DoubleX, room);
    }
    if free(graph, room, UP) {
        let up = graph.near(room, UP);
        graph.claim(room);
        graph.claim(up);
        return (Part::DoubleY, room);
    }
    if !graph.open(room, WEST)
        && !graph.open(room, EAST)
        && !graph.open(room, NORTH)
        && !graph.open(room, SOUTH)
        && !graph.open(room, UP)
    {
        graph.claim(room);
        return (Part::SimpleTop, room);
    }
    graph.claim(room);
    (Part::Simple(random.next_int(3)), room)
}

fn surroundings<G: Ground>(ground: &G, probe: &impl BiomeProbe, center: (i32, i32)) -> bool {
    let sea = ground.sea_level();
    let allowed: Vec<u8> = SURROUNDING
        .iter()
        .filter_map(|name| probe.id(name))
        .collect();
    let radius = 29;
    for qz in (center.1 - radius) >> 2..=(center.1 + radius) >> 2 {
        for qx in (center.0 - radius) >> 2..=(center.0 + radius) >> 2 {
            for qy in (sea - radius) >> 2..=(sea + radius) >> 2 {
                if !allowed.contains(&probe.at(qx, qy, qz)) {
                    return false;
                }
            }
        }
    }
    true
}

pub(crate) fn generate<G: Ground>(
    ground: &G,
    valid: &[u8],
    probe: &impl BiomeProbe,
    seed: i64,
    chunk: (i32, i32),
) -> Option<Piece> {
    let center = (chunk.0 * 16 + 9, chunk.1 * 16 + 9);
    if !surroundings(ground, probe, center) {
        return None;
    }
    start::on_top(ground, valid, probe, Map::OceanFloorWg, chunk)?;
    let mut random = start::random(seed, chunk);
    let (west, north) = (chunk.0 * 16 - 29, chunk.1 * 16 - 29);
    let facing = HORIZONTAL[random.next_int(4) as usize];
    let bbox = make_bbox((west, 39, north), facing, (58, 23, 58));
    let Layout {
        mut graph,
        source,
        core,
        order,
    } = layout(&mut random);
    graph.claim(source);
    let mut parts: Vec<(Part, usize)> = vec![(Part::Entry, source), (Part::Core, core)];
    for room in order {
        if graph.rooms[room].claimed || graph.special(room) {
            continue;
        }
        parts.push(fit(&mut graph, room, &mut random));
    }
    let shift = world_of(&bbox, Some(facing), (9, 0, 22));
    let mut children: Vec<Child> = parts
        .into_iter()
        .map(|(part, room)| Child {
            part,
            room,
            bbox: room_bbox(facing, graph.index(room), part.size())
                .moved(shift.0, shift.1, shift.2),
        })
        .collect();
    let corner = |a: Pos, b: Pos| {
        Bbox::from_corners(
            world_of(&bbox, Some(facing), a),
            world_of(&bbox, Some(facing), b),
        )
    };
    let left = corner((1, 1, 1), (23, 8, 21));
    let right = corner((34, 1, 1), (56, 8, 21));
    let penthouse = corner((22, 13, 22), (35, 17, 35));
    let mut design = random.next_i32();
    design = design.wrapping_add(1);
    children.push(Child {
        part: Part::Wing(design & 1),
        room: 0,
        bbox: left,
    });
    design = design.wrapping_add(1);
    children.push(Child {
        part: Part::Wing(design & 1),
        room: 0,
        bbox: right,
    });
    children.push(Child {
        part: Part::Penthouse,
        room: 0,
        bbox: penthouse,
    });
    Some(Piece {
        bbox,
        facing,
        graph: Arc::new(graph),
        children,
    })
}

fn water(f: &mut Frame, x1: i32, y1: i32, z1: i32, x2: i32, y2: i32, z2: i32) {
    let sea = f.run.region.sea_level();
    for y in y1..=y2 {
        for x in x1..=x2 {
            for z in z1..=z2 {
                let state = f.block(x, y, z);
                let keep = matches!(
                    state.kind(),
                    Some(Kind::Ice | Kind::PackedIce | Kind::BlueIce | Kind::Water)
                );
                if keep {
                    continue;
                }
                if f.world_y(y) >= sea && state != plain(Kind::Water) {
                    f.place(BlockStateId::AIR, x, y, z);
                } else {
                    f.place(plain(Kind::Water), x, y, z);
                }
            }
        }
    }
}

fn fill_only(f: &mut Frame, from: Pos, to: Pos, state: BlockStateId) {
    for y in from.1..=to.1 {
        for x in from.0..=to.0 {
            for z in from.2..=to.2 {
                if f.block(x, y, z) == plain(Kind::Water) {
                    f.place(state, x, y, z);
                }
            }
        }
    }
}

fn floor(f: &mut Frame, x: i32, z: i32, open: bool) {
    let gray = plain(Kind::Prismarine);
    let light = plain(Kind::PrismarineBricks);
    if open {
        f.bounds((x, 0, z), (x + 2, 0, z + 7), gray, gray, false);
        f.bounds((x + 5, 0, z), (x + 7, 0, z + 7), gray, gray, false);
        f.bounds((x + 3, 0, z), (x + 4, 0, z + 2), gray, gray, false);
        f.bounds((x + 3, 0, z + 5), (x + 4, 0, z + 7), gray, gray, false);
        f.bounds((x + 3, 0, z + 2), (x + 4, 0, z + 2), light, light, false);
        f.bounds((x + 3, 0, z + 5), (x + 4, 0, z + 5), light, light, false);
        f.bounds((x + 2, 0, z + 3), (x + 2, 0, z + 4), light, light, false);
        f.bounds((x + 5, 0, z + 3), (x + 5, 0, z + 4), light, light, false);
    } else {
        f.bounds((x, 0, z), (x + 7, 0, z + 7), gray, gray, false);
    }
}

fn hits(f: &mut Frame, x1: i32, z1: i32, x2: i32, z2: i32) -> bool {
    let (ax, az) = (f.world_x(x1, z1), f.world_z(x1, z1));
    let (bx, bz) = (f.world_x(x2, z2), f.world_z(x2, z2));
    f.chunk.max.0 >= ax.min(bx)
        && f.chunk.min.0 <= ax.max(bx)
        && f.chunk.max.2 >= az.min(bz)
        && f.chunk.min.2 <= az.max(bz)
}

fn elder(f: &mut Frame, x: i32, y: i32, z: i32) {
    let at = f.world_pos(x, y, z);
    if !f.chunk.contains(at) {
        return;
    }
    f.run.region.entities.push(json!({
        "id": "minecraft:elder_guardian",
        "Pos": [f64::from(at.0) + 0.5, f64::from(at.1), f64::from(at.2) + 0.5],
        "Motion": [0.0, 0.0, 0.0],
        "Rotation": [0.0f32, 0.0f32],
    }));
}

impl Piece {
    pub(crate) fn id(&self) -> &'static str {
        "OMB"
    }

    #[cfg(test)]
    pub(crate) fn child_lines(&self) -> Vec<String> {
        self.children
            .iter()
            .map(|child| {
                let b = child.bbox;
                format!(
                    "CHILD {} {} {} {} {} {} {}",
                    child.part.class(),
                    b.min.0,
                    b.min.1,
                    b.min.2,
                    b.max.0,
                    b.max.1,
                    b.max.2
                )
            })
            .collect()
    }

    pub(crate) fn place(&self, run: &mut Run, write: &Bbox) {
        {
            let mut f = Frame::new(run, self.bbox, Some(self.facing), *write, Guard::Open);
            let top = f.run.region.sea_level().max(64) - self.bbox.min.1;
            water(&mut f, 0, 0, 0, 58, top, 58);
            wing(&mut f, 0, 0);
            wing(&mut f, 1, 33);
            entrance_archs(&mut f);
            entrance_wall(&mut f);
            roof_piece(&mut f);
            lower_wall(&mut f);
            middle_wall(&mut f);
            upper_wall(&mut f);
            for x in 0..7 {
                let mut z = 0;
                while z < 7 {
                    if z == 0 && x == 3 {
                        z = 6;
                    }
                    let (bx, bz) = (x * 9, z * 9);
                    for dx in 0..4 {
                        for dz in 0..4 {
                            f.place(plain(Kind::PrismarineBricks), bx + dx, 0, bz + dz);
                            f.column_down(plain(Kind::PrismarineBricks), bx + dx, -1, bz + dz);
                        }
                    }
                    if x != 0 && x != 6 {
                        z += 6;
                    } else {
                        z += 1;
                    }
                }
            }
            for i in 0..5 {
                water(&mut f, -1 - i, i * 2, -1 - i, -1 - i, 23, 58 + i);
                water(&mut f, 58 + i, i * 2, -1 - i, 58 + i, 23, 58 + i);
                water(&mut f, -i, i * 2, -1 - i, 57 + i, 23, -1 - i);
                water(&mut f, -i, i * 2, 58 + i, 57 + i, 23, 58 + i);
            }
            f.finish();
        }
        for child in &self.children {
            if !child.bbox.intersects(write) {
                continue;
            }
            let mut f = Frame::new(run, child.bbox, Some(self.facing), *write, Guard::Open);
            let g = &*self.graph;
            let room = child.room;
            match child.part {
                Part::Entry => entry(&mut f, g, room),
                Part::Core => core(&mut f),
                Part::SimpleTop => simple_top(&mut f, g, room),
                Part::Simple(design) => simple(&mut f, g, room, design),
                Part::DoubleX => double_x(&mut f, g, room),
                Part::DoubleXY => double_xy(&mut f, g, room),
                Part::DoubleY => double_y(&mut f, g, room),
                Part::DoubleYZ => double_yz(&mut f, g, room),
                Part::DoubleZ => double_z(&mut f, g, room),
                Part::Wing(design) => room_wing_room(&mut f, design),
                Part::Penthouse => penthouse(&mut f),
            }
            f.finish();
        }
    }
}

fn entry(f: &mut Frame, g: &Graph, room: usize) {
    f.bounds(
        (0, 3, 0),
        (2, 3, 7),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (5, 3, 0),
        (7, 3, 7),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (0, 2, 0),
        (1, 2, 7),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (6, 2, 0),
        (7, 2, 7),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (0, 1, 0),
        (0, 1, 7),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (7, 1, 0),
        (7, 1, 7),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (0, 1, 7),
        (7, 3, 7),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (1, 1, 0),
        (2, 3, 0),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (5, 1, 0),
        (6, 3, 0),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    if g.open(room, NORTH) {
        water(f, 3, 1, 7, 4, 2, 7);
    }
    if g.open(room, WEST) {
        water(f, 0, 1, 3, 1, 2, 4);
    }
    if g.open(room, EAST) {
        water(f, 6, 1, 3, 7, 2, 4);
    }
}

fn simple(f: &mut Frame, g: &Graph, room: usize, design: i32) {
    if g.index(room) / 25 > 0 {
        floor(f, 0, 0, g.open(room, DOWN));
    }
    if !g.linked(room, UP) {
        fill_only(f, (1, 4, 1), (6, 4, 6), plain(Kind::Prismarine));
    }
    let v8 = if design == 0
        || !f.run.rng.next_boolean()
        || g.open(room, DOWN)
        || g.open(room, UP)
        || g.count_openings(room) <= 1
    {
        0
    } else {
        1
    };
    if design == 0 {
        f.bounds(
            (0, 1, 0),
            (2, 1, 2),
            plain(Kind::PrismarineBricks),
            plain(Kind::PrismarineBricks),
            false,
        );
        f.bounds(
            (0, 3, 0),
            (2, 3, 2),
            plain(Kind::PrismarineBricks),
            plain(Kind::PrismarineBricks),
            false,
        );
        f.bounds(
            (0, 2, 0),
            (0, 2, 2),
            plain(Kind::Prismarine),
            plain(Kind::Prismarine),
            false,
        );
        f.bounds(
            (1, 2, 0),
            (2, 2, 0),
            plain(Kind::Prismarine),
            plain(Kind::Prismarine),
            false,
        );
        f.place(plain(Kind::SeaLantern), 1, 2, 1);
        f.bounds(
            (5, 1, 0),
            (7, 1, 2),
            plain(Kind::PrismarineBricks),
            plain(Kind::PrismarineBricks),
            false,
        );
        f.bounds(
            (5, 3, 0),
            (7, 3, 2),
            plain(Kind::PrismarineBricks),
            plain(Kind::PrismarineBricks),
            false,
        );
        f.bounds(
            (7, 2, 0),
            (7, 2, 2),
            plain(Kind::Prismarine),
            plain(Kind::Prismarine),
            false,
        );
        f.bounds(
            (5, 2, 0),
            (6, 2, 0),
            plain(Kind::Prismarine),
            plain(Kind::Prismarine),
            false,
        );
        f.place(plain(Kind::SeaLantern), 6, 2, 1);
        f.bounds(
            (0, 1, 5),
            (2, 1, 7),
            plain(Kind::PrismarineBricks),
            plain(Kind::PrismarineBricks),
            false,
        );
        f.bounds(
            (0, 3, 5),
            (2, 3, 7),
            plain(Kind::PrismarineBricks),
            plain(Kind::PrismarineBricks),
            false,
        );
        f.bounds(
            (0, 2, 5),
            (0, 2, 7),
            plain(Kind::Prismarine),
            plain(Kind::Prismarine),
            false,
        );
        f.bounds(
            (1, 2, 7),
            (2, 2, 7),
            plain(Kind::Prismarine),
            plain(Kind::Prismarine),
            false,
        );
        f.place(plain(Kind::SeaLantern), 1, 2, 6);
        f.bounds(
            (5, 1, 5),
            (7, 1, 7),
            plain(Kind::PrismarineBricks),
            plain(Kind::PrismarineBricks),
            false,
        );
        f.bounds(
            (5, 3, 5),
            (7, 3, 7),
            plain(Kind::PrismarineBricks),
            plain(Kind::PrismarineBricks),
            false,
        );
        f.bounds(
            (7, 2, 5),
            (7, 2, 7),
            plain(Kind::Prismarine),
            plain(Kind::Prismarine),
            false,
        );
        f.bounds(
            (5, 2, 7),
            (6, 2, 7),
            plain(Kind::Prismarine),
            plain(Kind::Prismarine),
            false,
        );
        f.place(plain(Kind::SeaLantern), 6, 2, 6);
        if g.open(room, SOUTH) {
            f.bounds(
                (3, 3, 0),
                (4, 3, 0),
                plain(Kind::PrismarineBricks),
                plain(Kind::PrismarineBricks),
                false,
            );
        } else {
            f.bounds(
                (3, 3, 0),
                (4, 3, 1),
                plain(Kind::PrismarineBricks),
                plain(Kind::PrismarineBricks),
                false,
            );
            f.bounds(
                (3, 2, 0),
                (4, 2, 0),
                plain(Kind::Prismarine),
                plain(Kind::Prismarine),
                false,
            );
            f.bounds(
                (3, 1, 0),
                (4, 1, 1),
                plain(Kind::PrismarineBricks),
                plain(Kind::PrismarineBricks),
                false,
            );
        }
        if g.open(room, NORTH) {
            f.bounds(
                (3, 3, 7),
                (4, 3, 7),
                plain(Kind::PrismarineBricks),
                plain(Kind::PrismarineBricks),
                false,
            );
        } else {
            f.bounds(
                (3, 3, 6),
                (4, 3, 7),
                plain(Kind::PrismarineBricks),
                plain(Kind::PrismarineBricks),
                false,
            );
            f.bounds(
                (3, 2, 7),
                (4, 2, 7),
                plain(Kind::Prismarine),
                plain(Kind::Prismarine),
                false,
            );
            f.bounds(
                (3, 1, 6),
                (4, 1, 7),
                plain(Kind::PrismarineBricks),
                plain(Kind::PrismarineBricks),
                false,
            );
        }
        if g.open(room, WEST) {
            f.bounds(
                (0, 3, 3),
                (0, 3, 4),
                plain(Kind::PrismarineBricks),
                plain(Kind::PrismarineBricks),
                false,
            );
        } else {
            f.bounds(
                (0, 3, 3),
                (1, 3, 4),
                plain(Kind::PrismarineBricks),
                plain(Kind::PrismarineBricks),
                false,
            );
            f.bounds(
                (0, 2, 3),
                (0, 2, 4),
                plain(Kind::Prismarine),
                plain(Kind::Prismarine),
                false,
            );
            f.bounds(
                (0, 1, 3),
                (1, 1, 4),
                plain(Kind::PrismarineBricks),
                plain(Kind::PrismarineBricks),
                false,
            );
        }
        if g.open(room, EAST) {
            f.bounds(
                (7, 3, 3),
                (7, 3, 4),
                plain(Kind::PrismarineBricks),
                plain(Kind::PrismarineBricks),
                false,
            );
        } else {
            f.bounds(
                (6, 3, 3),
                (7, 3, 4),
                plain(Kind::PrismarineBricks),
                plain(Kind::PrismarineBricks),
                false,
            );
            f.bounds(
                (7, 2, 3),
                (7, 2, 4),
                plain(Kind::Prismarine),
                plain(Kind::Prismarine),
                false,
            );
            f.bounds(
                (6, 1, 3),
                (7, 1, 4),
                plain(Kind::PrismarineBricks),
                plain(Kind::PrismarineBricks),
                false,
            );
        }
    } else {
        if design == 1 {
            f.bounds(
                (2, 1, 2),
                (2, 3, 2),
                plain(Kind::PrismarineBricks),
                plain(Kind::PrismarineBricks),
                false,
            );
            f.bounds(
                (2, 1, 5),
                (2, 3, 5),
                plain(Kind::PrismarineBricks),
                plain(Kind::PrismarineBricks),
                false,
            );
            f.bounds(
                (5, 1, 5),
                (5, 3, 5),
                plain(Kind::PrismarineBricks),
                plain(Kind::PrismarineBricks),
                false,
            );
            f.bounds(
                (5, 1, 2),
                (5, 3, 2),
                plain(Kind::PrismarineBricks),
                plain(Kind::PrismarineBricks),
                false,
            );
            f.place(plain(Kind::SeaLantern), 2, 2, 2);
            f.place(plain(Kind::SeaLantern), 2, 2, 5);
            f.place(plain(Kind::SeaLantern), 5, 2, 5);
            f.place(plain(Kind::SeaLantern), 5, 2, 2);
            f.bounds(
                (0, 1, 0),
                (1, 3, 0),
                plain(Kind::PrismarineBricks),
                plain(Kind::PrismarineBricks),
                false,
            );
            f.bounds(
                (0, 1, 1),
                (0, 3, 1),
                plain(Kind::PrismarineBricks),
                plain(Kind::PrismarineBricks),
                false,
            );
            f.bounds(
                (0, 1, 7),
                (1, 3, 7),
                plain(Kind::PrismarineBricks),
                plain(Kind::PrismarineBricks),
                false,
            );
            f.bounds(
                (0, 1, 6),
                (0, 3, 6),
                plain(Kind::PrismarineBricks),
                plain(Kind::PrismarineBricks),
                false,
            );
            f.bounds(
                (6, 1, 7),
                (7, 3, 7),
                plain(Kind::PrismarineBricks),
                plain(Kind::PrismarineBricks),
                false,
            );
            f.bounds(
                (7, 1, 6),
                (7, 3, 6),
                plain(Kind::PrismarineBricks),
                plain(Kind::PrismarineBricks),
                false,
            );
            f.bounds(
                (6, 1, 0),
                (7, 3, 0),
                plain(Kind::PrismarineBricks),
                plain(Kind::PrismarineBricks),
                false,
            );
            f.bounds(
                (7, 1, 1),
                (7, 3, 1),
                plain(Kind::PrismarineBricks),
                plain(Kind::PrismarineBricks),
                false,
            );
            f.place(plain(Kind::Prismarine), 1, 2, 0);
            f.place(plain(Kind::Prismarine), 0, 2, 1);
            f.place(plain(Kind::Prismarine), 1, 2, 7);
            f.place(plain(Kind::Prismarine), 0, 2, 6);
            f.place(plain(Kind::Prismarine), 6, 2, 7);
            f.place(plain(Kind::Prismarine), 7, 2, 6);
            f.place(plain(Kind::Prismarine), 6, 2, 0);
            f.place(plain(Kind::Prismarine), 7, 2, 1);
            if !g.open(room, SOUTH) {
                f.bounds(
                    (1, 3, 0),
                    (6, 3, 0),
                    plain(Kind::PrismarineBricks),
                    plain(Kind::PrismarineBricks),
                    false,
                );
                f.bounds(
                    (1, 2, 0),
                    (6, 2, 0),
                    plain(Kind::Prismarine),
                    plain(Kind::Prismarine),
                    false,
                );
                f.bounds(
                    (1, 1, 0),
                    (6, 1, 0),
                    plain(Kind::PrismarineBricks),
                    plain(Kind::PrismarineBricks),
                    false,
                );
            }
            if !g.open(room, NORTH) {
                f.bounds(
                    (1, 3, 7),
                    (6, 3, 7),
                    plain(Kind::PrismarineBricks),
                    plain(Kind::PrismarineBricks),
                    false,
                );
                f.bounds(
                    (1, 2, 7),
                    (6, 2, 7),
                    plain(Kind::Prismarine),
                    plain(Kind::Prismarine),
                    false,
                );
                f.bounds(
                    (1, 1, 7),
                    (6, 1, 7),
                    plain(Kind::PrismarineBricks),
                    plain(Kind::PrismarineBricks),
                    false,
                );
            }
            if !g.open(room, WEST) {
                f.bounds(
                    (0, 3, 1),
                    (0, 3, 6),
                    plain(Kind::PrismarineBricks),
                    plain(Kind::PrismarineBricks),
                    false,
                );
                f.bounds(
                    (0, 2, 1),
                    (0, 2, 6),
                    plain(Kind::Prismarine),
                    plain(Kind::Prismarine),
                    false,
                );
                f.bounds(
                    (0, 1, 1),
                    (0, 1, 6),
                    plain(Kind::PrismarineBricks),
                    plain(Kind::PrismarineBricks),
                    false,
                );
            }
            if !g.open(room, EAST) {
                f.bounds(
                    (7, 3, 1),
                    (7, 3, 6),
                    plain(Kind::PrismarineBricks),
                    plain(Kind::PrismarineBricks),
                    false,
                );
                f.bounds(
                    (7, 2, 1),
                    (7, 2, 6),
                    plain(Kind::Prismarine),
                    plain(Kind::Prismarine),
                    false,
                );
                f.bounds(
                    (7, 1, 1),
                    (7, 1, 6),
                    plain(Kind::PrismarineBricks),
                    plain(Kind::PrismarineBricks),
                    false,
                );
            }
        } else {
            if design == 2 {
                f.bounds(
                    (0, 1, 0),
                    (0, 1, 7),
                    plain(Kind::PrismarineBricks),
                    plain(Kind::PrismarineBricks),
                    false,
                );
                f.bounds(
                    (7, 1, 0),
                    (7, 1, 7),
                    plain(Kind::PrismarineBricks),
                    plain(Kind::PrismarineBricks),
                    false,
                );
                f.bounds(
                    (1, 1, 0),
                    (6, 1, 0),
                    plain(Kind::PrismarineBricks),
                    plain(Kind::PrismarineBricks),
                    false,
                );
                f.bounds(
                    (1, 1, 7),
                    (6, 1, 7),
                    plain(Kind::PrismarineBricks),
                    plain(Kind::PrismarineBricks),
                    false,
                );
                f.bounds(
                    (0, 2, 0),
                    (0, 2, 7),
                    plain(Kind::DarkPrismarine),
                    plain(Kind::DarkPrismarine),
                    false,
                );
                f.bounds(
                    (7, 2, 0),
                    (7, 2, 7),
                    plain(Kind::DarkPrismarine),
                    plain(Kind::DarkPrismarine),
                    false,
                );
                f.bounds(
                    (1, 2, 0),
                    (6, 2, 0),
                    plain(Kind::DarkPrismarine),
                    plain(Kind::DarkPrismarine),
                    false,
                );
                f.bounds(
                    (1, 2, 7),
                    (6, 2, 7),
                    plain(Kind::DarkPrismarine),
                    plain(Kind::DarkPrismarine),
                    false,
                );
                f.bounds(
                    (0, 3, 0),
                    (0, 3, 7),
                    plain(Kind::PrismarineBricks),
                    plain(Kind::PrismarineBricks),
                    false,
                );
                f.bounds(
                    (7, 3, 0),
                    (7, 3, 7),
                    plain(Kind::PrismarineBricks),
                    plain(Kind::PrismarineBricks),
                    false,
                );
                f.bounds(
                    (1, 3, 0),
                    (6, 3, 0),
                    plain(Kind::PrismarineBricks),
                    plain(Kind::PrismarineBricks),
                    false,
                );
                f.bounds(
                    (1, 3, 7),
                    (6, 3, 7),
                    plain(Kind::PrismarineBricks),
                    plain(Kind::PrismarineBricks),
                    false,
                );
                f.bounds(
                    (0, 1, 3),
                    (0, 2, 4),
                    plain(Kind::DarkPrismarine),
                    plain(Kind::DarkPrismarine),
                    false,
                );
                f.bounds(
                    (7, 1, 3),
                    (7, 2, 4),
                    plain(Kind::DarkPrismarine),
                    plain(Kind::DarkPrismarine),
                    false,
                );
                f.bounds(
                    (3, 1, 0),
                    (4, 2, 0),
                    plain(Kind::DarkPrismarine),
                    plain(Kind::DarkPrismarine),
                    false,
                );
                f.bounds(
                    (3, 1, 7),
                    (4, 2, 7),
                    plain(Kind::DarkPrismarine),
                    plain(Kind::DarkPrismarine),
                    false,
                );
                if g.open(room, SOUTH) {
                    water(f, 3, 1, 0, 4, 2, 0);
                }
                if g.open(room, NORTH) {
                    water(f, 3, 1, 7, 4, 2, 7);
                }
                if g.open(room, WEST) {
                    water(f, 0, 1, 3, 0, 2, 4);
                }
                if g.open(room, EAST) {
                    water(f, 7, 1, 3, 7, 2, 4);
                }
            }
        }
    }
    if v8 != 0 {
        f.bounds(
            (3, 1, 3),
            (4, 1, 4),
            plain(Kind::PrismarineBricks),
            plain(Kind::PrismarineBricks),
            false,
        );
        f.bounds(
            (3, 2, 3),
            (4, 2, 4),
            plain(Kind::Prismarine),
            plain(Kind::Prismarine),
            false,
        );
        f.bounds(
            (3, 3, 3),
            (4, 3, 4),
            plain(Kind::PrismarineBricks),
            plain(Kind::PrismarineBricks),
            false,
        );
    }
}

fn simple_top(f: &mut Frame, g: &Graph, room: usize) {
    if g.index(room) / 25 > 0 {
        floor(f, 0, 0, g.open(room, DOWN));
    }
    if !g.linked(room, UP) {
        fill_only(f, (1, 4, 1), (6, 4, 6), plain(Kind::Prismarine));
    }
    let mut v8 = 1;
    while v8 <= 6 {
        let mut v9 = 1;
        while v9 <= 6 {
            if f.run.rng.below(3) != 0 {
                let v10 = 2 + if f.run.rng.below(4) != 0 { 1 } else { 0 };
                let v11 = plain(Kind::WetSponge);
                f.bounds((v8, v10, v9), (v8, 3, v9), v11, v11, false);
            }
            v9 += 1;
        }
        v8 += 1;
    }
    f.bounds(
        (0, 1, 0),
        (0, 1, 7),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (7, 1, 0),
        (7, 1, 7),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (1, 1, 0),
        (6, 1, 0),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (1, 1, 7),
        (6, 1, 7),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (0, 2, 0),
        (0, 2, 7),
        plain(Kind::DarkPrismarine),
        plain(Kind::DarkPrismarine),
        false,
    );
    f.bounds(
        (7, 2, 0),
        (7, 2, 7),
        plain(Kind::DarkPrismarine),
        plain(Kind::DarkPrismarine),
        false,
    );
    f.bounds(
        (1, 2, 0),
        (6, 2, 0),
        plain(Kind::DarkPrismarine),
        plain(Kind::DarkPrismarine),
        false,
    );
    f.bounds(
        (1, 2, 7),
        (6, 2, 7),
        plain(Kind::DarkPrismarine),
        plain(Kind::DarkPrismarine),
        false,
    );
    f.bounds(
        (0, 3, 0),
        (0, 3, 7),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (7, 3, 0),
        (7, 3, 7),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (1, 3, 0),
        (6, 3, 0),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (1, 3, 7),
        (6, 3, 7),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (0, 1, 3),
        (0, 2, 4),
        plain(Kind::DarkPrismarine),
        plain(Kind::DarkPrismarine),
        false,
    );
    f.bounds(
        (7, 1, 3),
        (7, 2, 4),
        plain(Kind::DarkPrismarine),
        plain(Kind::DarkPrismarine),
        false,
    );
    f.bounds(
        (3, 1, 0),
        (4, 2, 0),
        plain(Kind::DarkPrismarine),
        plain(Kind::DarkPrismarine),
        false,
    );
    f.bounds(
        (3, 1, 7),
        (4, 2, 7),
        plain(Kind::DarkPrismarine),
        plain(Kind::DarkPrismarine),
        false,
    );
    if g.open(room, SOUTH) {
        water(f, 3, 1, 0, 4, 2, 0);
    }
}

fn double_x(f: &mut Frame, g: &Graph, room: usize) {
    let v8 = g.near(room, EAST);
    let v9 = room;
    if g.index(room) / 25 > 0 {
        floor(f, 8, 0, g.open(v8, DOWN));
        floor(f, 0, 0, g.open(v9, DOWN));
    }
    if !g.linked(v9, UP) {
        fill_only(f, (1, 4, 1), (7, 4, 6), plain(Kind::Prismarine));
    }
    if !g.linked(v8, UP) {
        fill_only(f, (8, 4, 1), (14, 4, 6), plain(Kind::Prismarine));
    }
    f.bounds(
        (0, 3, 0),
        (0, 3, 7),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (15, 3, 0),
        (15, 3, 7),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (1, 3, 0),
        (15, 3, 0),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (1, 3, 7),
        (14, 3, 7),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (0, 2, 0),
        (0, 2, 7),
        plain(Kind::Prismarine),
        plain(Kind::Prismarine),
        false,
    );
    f.bounds(
        (15, 2, 0),
        (15, 2, 7),
        plain(Kind::Prismarine),
        plain(Kind::Prismarine),
        false,
    );
    f.bounds(
        (1, 2, 0),
        (15, 2, 0),
        plain(Kind::Prismarine),
        plain(Kind::Prismarine),
        false,
    );
    f.bounds(
        (1, 2, 7),
        (14, 2, 7),
        plain(Kind::Prismarine),
        plain(Kind::Prismarine),
        false,
    );
    f.bounds(
        (0, 1, 0),
        (0, 1, 7),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (15, 1, 0),
        (15, 1, 7),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (1, 1, 0),
        (15, 1, 0),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (1, 1, 7),
        (14, 1, 7),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (5, 1, 0),
        (10, 1, 4),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (6, 2, 0),
        (9, 2, 3),
        plain(Kind::Prismarine),
        plain(Kind::Prismarine),
        false,
    );
    f.bounds(
        (5, 3, 0),
        (10, 3, 4),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.place(plain(Kind::SeaLantern), 6, 2, 3);
    f.place(plain(Kind::SeaLantern), 9, 2, 3);
    if g.open(v9, SOUTH) {
        water(f, 3, 1, 0, 4, 2, 0);
    }
    if g.open(v9, NORTH) {
        water(f, 3, 1, 7, 4, 2, 7);
    }
    if g.open(v9, WEST) {
        water(f, 0, 1, 3, 0, 2, 4);
    }
    if g.open(v8, SOUTH) {
        water(f, 11, 1, 0, 12, 2, 0);
    }
    if g.open(v8, NORTH) {
        water(f, 11, 1, 7, 12, 2, 7);
    }
    if g.open(v8, EAST) {
        water(f, 15, 1, 3, 15, 2, 4);
    }
}

fn double_xy(f: &mut Frame, g: &Graph, room: usize) {
    let v8 = g.near(room, EAST);
    let v9 = room;
    let v10 = g.near(v9, UP);
    let v11 = g.near(v8, UP);
    if g.index(room) / 25 > 0 {
        floor(f, 8, 0, g.open(v8, DOWN));
        floor(f, 0, 0, g.open(v9, DOWN));
    }
    if !g.linked(v10, UP) {
        fill_only(f, (1, 8, 1), (7, 8, 6), plain(Kind::Prismarine));
    }
    if !g.linked(v11, UP) {
        fill_only(f, (8, 8, 1), (14, 8, 6), plain(Kind::Prismarine));
    }
    let mut v12 = 1;
    while v12 <= 7 {
        let mut v13 = plain(Kind::PrismarineBricks);
        if v12 == 2 || v12 == 6 {
            v13 = plain(Kind::Prismarine);
        }
        f.bounds((0, v12, 0), (0, v12, 7), v13, v13, false);
        f.bounds((15, v12, 0), (15, v12, 7), v13, v13, false);
        f.bounds((1, v12, 0), (15, v12, 0), v13, v13, false);
        f.bounds((1, v12, 7), (14, v12, 7), v13, v13, false);
        v12 += 1;
    }
    f.bounds(
        (2, 1, 3),
        (2, 7, 4),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (3, 1, 2),
        (4, 7, 2),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (3, 1, 5),
        (4, 7, 5),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (13, 1, 3),
        (13, 7, 4),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (11, 1, 2),
        (12, 7, 2),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (11, 1, 5),
        (12, 7, 5),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (5, 1, 3),
        (5, 3, 4),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (10, 1, 3),
        (10, 3, 4),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (5, 7, 2),
        (10, 7, 5),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (5, 5, 2),
        (5, 7, 2),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (10, 5, 2),
        (10, 7, 2),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (5, 5, 5),
        (5, 7, 5),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (10, 5, 5),
        (10, 7, 5),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.place(plain(Kind::PrismarineBricks), 6, 6, 2);
    f.place(plain(Kind::PrismarineBricks), 9, 6, 2);
    f.place(plain(Kind::PrismarineBricks), 6, 6, 5);
    f.place(plain(Kind::PrismarineBricks), 9, 6, 5);
    f.bounds(
        (5, 4, 3),
        (6, 4, 4),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (9, 4, 3),
        (10, 4, 4),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.place(plain(Kind::SeaLantern), 5, 4, 2);
    f.place(plain(Kind::SeaLantern), 5, 4, 5);
    f.place(plain(Kind::SeaLantern), 10, 4, 2);
    f.place(plain(Kind::SeaLantern), 10, 4, 5);
    if g.open(v9, SOUTH) {
        water(f, 3, 1, 0, 4, 2, 0);
    }
    if g.open(v9, NORTH) {
        water(f, 3, 1, 7, 4, 2, 7);
    }
    if g.open(v9, WEST) {
        water(f, 0, 1, 3, 0, 2, 4);
    }
    if g.open(v8, SOUTH) {
        water(f, 11, 1, 0, 12, 2, 0);
    }
    if g.open(v8, NORTH) {
        water(f, 11, 1, 7, 12, 2, 7);
    }
    if g.open(v8, EAST) {
        water(f, 15, 1, 3, 15, 2, 4);
    }
    if g.open(v10, SOUTH) {
        water(f, 3, 5, 0, 4, 6, 0);
    }
    if g.open(v10, NORTH) {
        water(f, 3, 5, 7, 4, 6, 7);
    }
    if g.open(v10, WEST) {
        water(f, 0, 5, 3, 0, 6, 4);
    }
    if g.open(v11, SOUTH) {
        water(f, 11, 5, 0, 12, 6, 0);
    }
    if g.open(v11, NORTH) {
        water(f, 11, 5, 7, 12, 6, 7);
    }
    if g.open(v11, EAST) {
        water(f, 15, 5, 3, 15, 6, 4);
    }
}

fn double_y(f: &mut Frame, g: &Graph, room: usize) {
    if g.index(room) / 25 > 0 {
        floor(f, 0, 0, g.open(room, DOWN));
    }
    let v8 = g.near(room, UP);
    if !g.linked(v8, UP) {
        fill_only(f, (1, 8, 1), (6, 8, 6), plain(Kind::Prismarine));
    }
    f.bounds(
        (0, 4, 0),
        (0, 4, 7),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (7, 4, 0),
        (7, 4, 7),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (1, 4, 0),
        (6, 4, 0),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (1, 4, 7),
        (6, 4, 7),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (2, 4, 1),
        (2, 4, 2),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (1, 4, 2),
        (1, 4, 2),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (5, 4, 1),
        (5, 4, 2),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (6, 4, 2),
        (6, 4, 2),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (2, 4, 5),
        (2, 4, 6),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (1, 4, 5),
        (1, 4, 5),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (5, 4, 5),
        (5, 4, 6),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (6, 4, 5),
        (6, 4, 5),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    let mut v9 = room;
    let mut v10 = 1;
    while v10 <= 5 {
        let mut v11 = 0;
        if g.open(v9, SOUTH) {
            f.bounds(
                (2, v10, v11),
                (2, v10 + 2, v11),
                plain(Kind::PrismarineBricks),
                plain(Kind::PrismarineBricks),
                false,
            );
            f.bounds(
                (5, v10, v11),
                (5, v10 + 2, v11),
                plain(Kind::PrismarineBricks),
                plain(Kind::PrismarineBricks),
                false,
            );
            f.bounds(
                (3, v10 + 2, v11),
                (4, v10 + 2, v11),
                plain(Kind::PrismarineBricks),
                plain(Kind::PrismarineBricks),
                false,
            );
        } else {
            f.bounds(
                (0, v10, v11),
                (7, v10 + 2, v11),
                plain(Kind::PrismarineBricks),
                plain(Kind::PrismarineBricks),
                false,
            );
            f.bounds(
                (0, v10 + 1, v11),
                (7, v10 + 1, v11),
                plain(Kind::Prismarine),
                plain(Kind::Prismarine),
                false,
            );
        }
        v11 = 7;
        if g.open(v9, NORTH) {
            f.bounds(
                (2, v10, v11),
                (2, v10 + 2, v11),
                plain(Kind::PrismarineBricks),
                plain(Kind::PrismarineBricks),
                false,
            );
            f.bounds(
                (5, v10, v11),
                (5, v10 + 2, v11),
                plain(Kind::PrismarineBricks),
                plain(Kind::PrismarineBricks),
                false,
            );
            f.bounds(
                (3, v10 + 2, v11),
                (4, v10 + 2, v11),
                plain(Kind::PrismarineBricks),
                plain(Kind::PrismarineBricks),
                false,
            );
        } else {
            f.bounds(
                (0, v10, v11),
                (7, v10 + 2, v11),
                plain(Kind::PrismarineBricks),
                plain(Kind::PrismarineBricks),
                false,
            );
            f.bounds(
                (0, v10 + 1, v11),
                (7, v10 + 1, v11),
                plain(Kind::Prismarine),
                plain(Kind::Prismarine),
                false,
            );
        }
        let mut v12 = 0;
        if g.open(v9, WEST) {
            f.bounds(
                (v12, v10, 2),
                (v12, v10 + 2, 2),
                plain(Kind::PrismarineBricks),
                plain(Kind::PrismarineBricks),
                false,
            );
            f.bounds(
                (v12, v10, 5),
                (v12, v10 + 2, 5),
                plain(Kind::PrismarineBricks),
                plain(Kind::PrismarineBricks),
                false,
            );
            f.bounds(
                (v12, v10 + 2, 3),
                (v12, v10 + 2, 4),
                plain(Kind::PrismarineBricks),
                plain(Kind::PrismarineBricks),
                false,
            );
        } else {
            f.bounds(
                (v12, v10, 0),
                (v12, v10 + 2, 7),
                plain(Kind::PrismarineBricks),
                plain(Kind::PrismarineBricks),
                false,
            );
            f.bounds(
                (v12, v10 + 1, 0),
                (v12, v10 + 1, 7),
                plain(Kind::Prismarine),
                plain(Kind::Prismarine),
                false,
            );
        }
        v12 = 7;
        if g.open(v9, EAST) {
            f.bounds(
                (v12, v10, 2),
                (v12, v10 + 2, 2),
                plain(Kind::PrismarineBricks),
                plain(Kind::PrismarineBricks),
                false,
            );
            f.bounds(
                (v12, v10, 5),
                (v12, v10 + 2, 5),
                plain(Kind::PrismarineBricks),
                plain(Kind::PrismarineBricks),
                false,
            );
            f.bounds(
                (v12, v10 + 2, 3),
                (v12, v10 + 2, 4),
                plain(Kind::PrismarineBricks),
                plain(Kind::PrismarineBricks),
                false,
            );
        } else {
            f.bounds(
                (v12, v10, 0),
                (v12, v10 + 2, 7),
                plain(Kind::PrismarineBricks),
                plain(Kind::PrismarineBricks),
                false,
            );
            f.bounds(
                (v12, v10 + 1, 0),
                (v12, v10 + 1, 7),
                plain(Kind::Prismarine),
                plain(Kind::Prismarine),
                false,
            );
        }
        v9 = v8;
        v10 += 4;
    }
}

fn double_yz(f: &mut Frame, g: &Graph, room: usize) {
    let v8 = g.near(room, NORTH);
    let v9 = room;
    let v10 = g.near(v8, UP);
    let v11 = g.near(v9, UP);
    if g.index(room) / 25 > 0 {
        floor(f, 0, 8, g.open(v8, DOWN));
        floor(f, 0, 0, g.open(v9, DOWN));
    }
    if !g.linked(v11, UP) {
        fill_only(f, (1, 8, 1), (6, 8, 7), plain(Kind::Prismarine));
    }
    if !g.linked(v10, UP) {
        fill_only(f, (1, 8, 8), (6, 8, 14), plain(Kind::Prismarine));
    }
    let mut v12 = 1;
    while v12 <= 7 {
        let mut v13 = plain(Kind::PrismarineBricks);
        if v12 == 2 || v12 == 6 {
            v13 = plain(Kind::Prismarine);
        }
        f.bounds((0, v12, 0), (0, v12, 15), v13, v13, false);
        f.bounds((7, v12, 0), (7, v12, 15), v13, v13, false);
        f.bounds((1, v12, 0), (6, v12, 0), v13, v13, false);
        f.bounds((1, v12, 15), (6, v12, 15), v13, v13, false);
        v12 += 1;
    }
    v12 = 1;
    while v12 <= 7 {
        let mut v13 = plain(Kind::DarkPrismarine);
        if v12 == 2 || v12 == 6 {
            v13 = plain(Kind::SeaLantern);
        }
        f.bounds((3, v12, 7), (4, v12, 8), v13, v13, false);
        v12 += 1;
    }
    if g.open(v9, SOUTH) {
        water(f, 3, 1, 0, 4, 2, 0);
    }
    if g.open(v9, EAST) {
        water(f, 7, 1, 3, 7, 2, 4);
    }
    if g.open(v9, WEST) {
        water(f, 0, 1, 3, 0, 2, 4);
    }
    if g.open(v8, NORTH) {
        water(f, 3, 1, 15, 4, 2, 15);
    }
    if g.open(v8, WEST) {
        water(f, 0, 1, 11, 0, 2, 12);
    }
    if g.open(v8, EAST) {
        water(f, 7, 1, 11, 7, 2, 12);
    }
    if g.open(v11, SOUTH) {
        water(f, 3, 5, 0, 4, 6, 0);
    }
    if g.open(v11, EAST) {
        water(f, 7, 5, 3, 7, 6, 4);
        f.bounds(
            (5, 4, 2),
            (6, 4, 5),
            plain(Kind::PrismarineBricks),
            plain(Kind::PrismarineBricks),
            false,
        );
        f.bounds(
            (6, 1, 2),
            (6, 3, 2),
            plain(Kind::PrismarineBricks),
            plain(Kind::PrismarineBricks),
            false,
        );
        f.bounds(
            (6, 1, 5),
            (6, 3, 5),
            plain(Kind::PrismarineBricks),
            plain(Kind::PrismarineBricks),
            false,
        );
    }
    if g.open(v11, WEST) {
        water(f, 0, 5, 3, 0, 6, 4);
        f.bounds(
            (1, 4, 2),
            (2, 4, 5),
            plain(Kind::PrismarineBricks),
            plain(Kind::PrismarineBricks),
            false,
        );
        f.bounds(
            (1, 1, 2),
            (1, 3, 2),
            plain(Kind::PrismarineBricks),
            plain(Kind::PrismarineBricks),
            false,
        );
        f.bounds(
            (1, 1, 5),
            (1, 3, 5),
            plain(Kind::PrismarineBricks),
            plain(Kind::PrismarineBricks),
            false,
        );
    }
    if g.open(v10, NORTH) {
        water(f, 3, 5, 15, 4, 6, 15);
    }
    if g.open(v10, WEST) {
        water(f, 0, 5, 11, 0, 6, 12);
        f.bounds(
            (1, 4, 10),
            (2, 4, 13),
            plain(Kind::PrismarineBricks),
            plain(Kind::PrismarineBricks),
            false,
        );
        f.bounds(
            (1, 1, 10),
            (1, 3, 10),
            plain(Kind::PrismarineBricks),
            plain(Kind::PrismarineBricks),
            false,
        );
        f.bounds(
            (1, 1, 13),
            (1, 3, 13),
            plain(Kind::PrismarineBricks),
            plain(Kind::PrismarineBricks),
            false,
        );
    }
    if g.open(v10, EAST) {
        water(f, 7, 5, 11, 7, 6, 12);
        f.bounds(
            (5, 4, 10),
            (6, 4, 13),
            plain(Kind::PrismarineBricks),
            plain(Kind::PrismarineBricks),
            false,
        );
        f.bounds(
            (6, 1, 10),
            (6, 3, 10),
            plain(Kind::PrismarineBricks),
            plain(Kind::PrismarineBricks),
            false,
        );
        f.bounds(
            (6, 1, 13),
            (6, 3, 13),
            plain(Kind::PrismarineBricks),
            plain(Kind::PrismarineBricks),
            false,
        );
    }
}

fn double_z(f: &mut Frame, g: &Graph, room: usize) {
    let v8 = g.near(room, NORTH);
    let v9 = room;
    if g.index(room) / 25 > 0 {
        floor(f, 0, 8, g.open(v8, DOWN));
        floor(f, 0, 0, g.open(v9, DOWN));
    }
    if !g.linked(v9, UP) {
        fill_only(f, (1, 4, 1), (6, 4, 7), plain(Kind::Prismarine));
    }
    if !g.linked(v8, UP) {
        fill_only(f, (1, 4, 8), (6, 4, 14), plain(Kind::Prismarine));
    }
    f.bounds(
        (0, 3, 0),
        (0, 3, 15),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (7, 3, 0),
        (7, 3, 15),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (1, 3, 0),
        (7, 3, 0),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (1, 3, 15),
        (6, 3, 15),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (0, 2, 0),
        (0, 2, 15),
        plain(Kind::Prismarine),
        plain(Kind::Prismarine),
        false,
    );
    f.bounds(
        (7, 2, 0),
        (7, 2, 15),
        plain(Kind::Prismarine),
        plain(Kind::Prismarine),
        false,
    );
    f.bounds(
        (1, 2, 0),
        (7, 2, 0),
        plain(Kind::Prismarine),
        plain(Kind::Prismarine),
        false,
    );
    f.bounds(
        (1, 2, 15),
        (6, 2, 15),
        plain(Kind::Prismarine),
        plain(Kind::Prismarine),
        false,
    );
    f.bounds(
        (0, 1, 0),
        (0, 1, 15),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (7, 1, 0),
        (7, 1, 15),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (1, 1, 0),
        (7, 1, 0),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (1, 1, 15),
        (6, 1, 15),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (1, 1, 1),
        (1, 1, 2),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (6, 1, 1),
        (6, 1, 2),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (1, 3, 1),
        (1, 3, 2),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (6, 3, 1),
        (6, 3, 2),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (1, 1, 13),
        (1, 1, 14),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (6, 1, 13),
        (6, 1, 14),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (1, 3, 13),
        (1, 3, 14),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (6, 3, 13),
        (6, 3, 14),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (2, 1, 6),
        (2, 3, 6),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (5, 1, 6),
        (5, 3, 6),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (2, 1, 9),
        (2, 3, 9),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (5, 1, 9),
        (5, 3, 9),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (3, 2, 6),
        (4, 2, 6),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (3, 2, 9),
        (4, 2, 9),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (2, 2, 7),
        (2, 2, 8),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (5, 2, 7),
        (5, 2, 8),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.place(plain(Kind::SeaLantern), 2, 2, 5);
    f.place(plain(Kind::SeaLantern), 5, 2, 5);
    f.place(plain(Kind::SeaLantern), 2, 2, 10);
    f.place(plain(Kind::SeaLantern), 5, 2, 10);
    f.place(plain(Kind::PrismarineBricks), 2, 3, 5);
    f.place(plain(Kind::PrismarineBricks), 5, 3, 5);
    f.place(plain(Kind::PrismarineBricks), 2, 3, 10);
    f.place(plain(Kind::PrismarineBricks), 5, 3, 10);
    if g.open(v9, SOUTH) {
        water(f, 3, 1, 0, 4, 2, 0);
    }
    if g.open(v9, EAST) {
        water(f, 7, 1, 3, 7, 2, 4);
    }
    if g.open(v9, WEST) {
        water(f, 0, 1, 3, 0, 2, 4);
    }
    if g.open(v8, NORTH) {
        water(f, 3, 1, 15, 4, 2, 15);
    }
    if g.open(v8, WEST) {
        water(f, 0, 1, 11, 0, 2, 12);
    }
    if g.open(v8, EAST) {
        water(f, 7, 1, 11, 7, 2, 12);
    }
}

fn core(f: &mut Frame) {
    fill_only(f, (1, 8, 0), (14, 8, 14), plain(Kind::Prismarine));
    let mut v9 = plain(Kind::PrismarineBricks);
    f.bounds((0, 7, 0), (0, 7, 15), v9, v9, false);
    f.bounds((15, 7, 0), (15, 7, 15), v9, v9, false);
    f.bounds((1, 7, 0), (15, 7, 0), v9, v9, false);
    f.bounds((1, 7, 15), (14, 7, 15), v9, v9, false);
    let mut v8 = 1;
    while v8 <= 6 {
        v9 = plain(Kind::PrismarineBricks);
        if v8 == 2 || v8 == 6 {
            v9 = plain(Kind::Prismarine);
        }
        let mut v10 = 0;
        while v10 <= 15 {
            f.bounds((v10, v8, 0), (v10, v8, 1), v9, v9, false);
            f.bounds((v10, v8, 6), (v10, v8, 9), v9, v9, false);
            f.bounds((v10, v8, 14), (v10, v8, 15), v9, v9, false);
            v10 += 15;
        }
        f.bounds((1, v8, 0), (1, v8, 0), v9, v9, false);
        f.bounds((6, v8, 0), (9, v8, 0), v9, v9, false);
        f.bounds((14, v8, 0), (14, v8, 0), v9, v9, false);
        f.bounds((1, v8, 15), (14, v8, 15), v9, v9, false);
        v8 += 1;
    }
    f.bounds(
        (6, 3, 6),
        (9, 6, 9),
        plain(Kind::DarkPrismarine),
        plain(Kind::DarkPrismarine),
        false,
    );
    f.bounds(
        (7, 4, 7),
        (8, 5, 8),
        plain(Kind::GoldBlock),
        plain(Kind::GoldBlock),
        false,
    );
    v8 = 3;
    while v8 <= 6 {
        let mut v9_1 = 6;
        while v9_1 <= 9 {
            f.place(plain(Kind::SeaLantern), v9_1, v8, 6);
            f.place(plain(Kind::SeaLantern), v9_1, v8, 9);
            v9_1 += 3;
        }
        v8 += 3;
    }
    f.bounds(
        (5, 1, 6),
        (5, 2, 6),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (5, 1, 9),
        (5, 2, 9),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (10, 1, 6),
        (10, 2, 6),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (10, 1, 9),
        (10, 2, 9),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (6, 1, 5),
        (6, 2, 5),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (9, 1, 5),
        (9, 2, 5),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (6, 1, 10),
        (6, 2, 10),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (9, 1, 10),
        (9, 2, 10),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (5, 2, 5),
        (5, 6, 5),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (5, 2, 10),
        (5, 6, 10),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (10, 2, 5),
        (10, 6, 5),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (10, 2, 10),
        (10, 6, 10),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (5, 7, 1),
        (5, 7, 6),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (10, 7, 1),
        (10, 7, 6),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (5, 7, 9),
        (5, 7, 14),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (10, 7, 9),
        (10, 7, 14),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (1, 7, 5),
        (6, 7, 5),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (1, 7, 10),
        (6, 7, 10),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (9, 7, 5),
        (14, 7, 5),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (9, 7, 10),
        (14, 7, 10),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (2, 1, 2),
        (2, 1, 3),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (3, 1, 2),
        (3, 1, 2),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (13, 1, 2),
        (13, 1, 3),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (12, 1, 2),
        (12, 1, 2),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (2, 1, 12),
        (2, 1, 13),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (3, 1, 13),
        (3, 1, 13),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (13, 1, 12),
        (13, 1, 13),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (12, 1, 13),
        (12, 1, 13),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
}

fn room_wing_room(f: &mut Frame, design: i32) {
    if design == 0 {
        let mut v8 = 0;
        while v8 < 4 {
            f.bounds(
                (10 - v8, 3 - v8, 20 - v8),
                (12 + v8, 3 - v8, 20),
                plain(Kind::PrismarineBricks),
                plain(Kind::PrismarineBricks),
                false,
            );
            v8 += 1;
        }
        f.bounds(
            (7, 0, 6),
            (15, 0, 16),
            plain(Kind::PrismarineBricks),
            plain(Kind::PrismarineBricks),
            false,
        );
        f.bounds(
            (6, 0, 6),
            (6, 3, 20),
            plain(Kind::PrismarineBricks),
            plain(Kind::PrismarineBricks),
            false,
        );
        f.bounds(
            (16, 0, 6),
            (16, 3, 20),
            plain(Kind::PrismarineBricks),
            plain(Kind::PrismarineBricks),
            false,
        );
        f.bounds(
            (7, 1, 7),
            (7, 1, 20),
            plain(Kind::PrismarineBricks),
            plain(Kind::PrismarineBricks),
            false,
        );
        f.bounds(
            (15, 1, 7),
            (15, 1, 20),
            plain(Kind::PrismarineBricks),
            plain(Kind::PrismarineBricks),
            false,
        );
        f.bounds(
            (7, 1, 6),
            (9, 3, 6),
            plain(Kind::PrismarineBricks),
            plain(Kind::PrismarineBricks),
            false,
        );
        f.bounds(
            (13, 1, 6),
            (15, 3, 6),
            plain(Kind::PrismarineBricks),
            plain(Kind::PrismarineBricks),
            false,
        );
        f.bounds(
            (8, 1, 7),
            (9, 1, 7),
            plain(Kind::PrismarineBricks),
            plain(Kind::PrismarineBricks),
            false,
        );
        f.bounds(
            (13, 1, 7),
            (14, 1, 7),
            plain(Kind::PrismarineBricks),
            plain(Kind::PrismarineBricks),
            false,
        );
        f.bounds(
            (9, 0, 5),
            (13, 0, 5),
            plain(Kind::PrismarineBricks),
            plain(Kind::PrismarineBricks),
            false,
        );
        f.bounds(
            (10, 0, 7),
            (12, 0, 7),
            plain(Kind::DarkPrismarine),
            plain(Kind::DarkPrismarine),
            false,
        );
        f.bounds(
            (8, 0, 10),
            (8, 0, 12),
            plain(Kind::DarkPrismarine),
            plain(Kind::DarkPrismarine),
            false,
        );
        f.bounds(
            (14, 0, 10),
            (14, 0, 12),
            plain(Kind::DarkPrismarine),
            plain(Kind::DarkPrismarine),
            false,
        );
        v8 = 18;
        while v8 >= 7 {
            f.place(plain(Kind::SeaLantern), 6, 3, v8);
            f.place(plain(Kind::SeaLantern), 16, 3, v8);
            v8 += -3;
        }
        f.place(plain(Kind::SeaLantern), 10, 0, 10);
        f.place(plain(Kind::SeaLantern), 12, 0, 10);
        f.place(plain(Kind::SeaLantern), 10, 0, 12);
        f.place(plain(Kind::SeaLantern), 12, 0, 12);
        f.place(plain(Kind::SeaLantern), 8, 3, 6);
        f.place(plain(Kind::SeaLantern), 14, 3, 6);
        f.place(plain(Kind::PrismarineBricks), 4, 2, 4);
        f.place(plain(Kind::SeaLantern), 4, 1, 4);
        f.place(plain(Kind::PrismarineBricks), 4, 0, 4);
        f.place(plain(Kind::PrismarineBricks), 18, 2, 4);
        f.place(plain(Kind::SeaLantern), 18, 1, 4);
        f.place(plain(Kind::PrismarineBricks), 18, 0, 4);
        f.place(plain(Kind::PrismarineBricks), 4, 2, 18);
        f.place(plain(Kind::SeaLantern), 4, 1, 18);
        f.place(plain(Kind::PrismarineBricks), 4, 0, 18);
        f.place(plain(Kind::PrismarineBricks), 18, 2, 18);
        f.place(plain(Kind::SeaLantern), 18, 1, 18);
        f.place(plain(Kind::PrismarineBricks), 18, 0, 18);
        f.place(plain(Kind::PrismarineBricks), 9, 7, 20);
        f.place(plain(Kind::PrismarineBricks), 13, 7, 20);
        f.bounds(
            (6, 0, 21),
            (7, 4, 21),
            plain(Kind::PrismarineBricks),
            plain(Kind::PrismarineBricks),
            false,
        );
        f.bounds(
            (15, 0, 21),
            (16, 4, 21),
            plain(Kind::PrismarineBricks),
            plain(Kind::PrismarineBricks),
            false,
        );
        elder(f, 11, 2, 16);
    } else {
        if design == 1 {
            f.bounds(
                (9, 3, 18),
                (13, 3, 20),
                plain(Kind::PrismarineBricks),
                plain(Kind::PrismarineBricks),
                false,
            );
            f.bounds(
                (9, 0, 18),
                (9, 2, 18),
                plain(Kind::PrismarineBricks),
                plain(Kind::PrismarineBricks),
                false,
            );
            f.bounds(
                (13, 0, 18),
                (13, 2, 18),
                plain(Kind::PrismarineBricks),
                plain(Kind::PrismarineBricks),
                false,
            );
            let mut v8 = 9;
            let mut v11 = 0;
            while v11 < 2 {
                f.place(plain(Kind::PrismarineBricks), v8, 6, 20);
                f.place(plain(Kind::SeaLantern), v8, 5, 20);
                f.place(plain(Kind::PrismarineBricks), v8, 4, 20);
                v8 = 13;
                v11 += 1;
            }
            f.bounds(
                (7, 3, 7),
                (15, 3, 14),
                plain(Kind::PrismarineBricks),
                plain(Kind::PrismarineBricks),
                false,
            );
            v8 = 10;
            v11 = 0;
            while v11 < 2 {
                f.bounds(
                    (v8, 0, 10),
                    (v8, 6, 10),
                    plain(Kind::PrismarineBricks),
                    plain(Kind::PrismarineBricks),
                    false,
                );
                f.bounds(
                    (v8, 0, 12),
                    (v8, 6, 12),
                    plain(Kind::PrismarineBricks),
                    plain(Kind::PrismarineBricks),
                    false,
                );
                f.place(plain(Kind::SeaLantern), v8, 0, 10);
                f.place(plain(Kind::SeaLantern), v8, 0, 12);
                f.place(plain(Kind::SeaLantern), v8, 4, 10);
                f.place(plain(Kind::SeaLantern), v8, 4, 12);
                v8 = 12;
                v11 += 1;
            }
            v8 = 8;
            v11 = 0;
            while v11 < 2 {
                f.bounds(
                    (v8, 0, 7),
                    (v8, 2, 7),
                    plain(Kind::PrismarineBricks),
                    plain(Kind::PrismarineBricks),
                    false,
                );
                f.bounds(
                    (v8, 0, 14),
                    (v8, 2, 14),
                    plain(Kind::PrismarineBricks),
                    plain(Kind::PrismarineBricks),
                    false,
                );
                v8 = 14;
                v11 += 1;
            }
            f.bounds(
                (8, 3, 8),
                (8, 3, 13),
                plain(Kind::DarkPrismarine),
                plain(Kind::DarkPrismarine),
                false,
            );
            f.bounds(
                (14, 3, 8),
                (14, 3, 13),
                plain(Kind::DarkPrismarine),
                plain(Kind::DarkPrismarine),
                false,
            );
            elder(f, 11, 5, 13);
        }
    }
}

fn penthouse(f: &mut Frame) {
    f.bounds(
        (2, -1, 2),
        (11, -1, 11),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (0, -1, 0),
        (1, -1, 11),
        plain(Kind::Prismarine),
        plain(Kind::Prismarine),
        false,
    );
    f.bounds(
        (12, -1, 0),
        (13, -1, 11),
        plain(Kind::Prismarine),
        plain(Kind::Prismarine),
        false,
    );
    f.bounds(
        (2, -1, 0),
        (11, -1, 1),
        plain(Kind::Prismarine),
        plain(Kind::Prismarine),
        false,
    );
    f.bounds(
        (2, -1, 12),
        (11, -1, 13),
        plain(Kind::Prismarine),
        plain(Kind::Prismarine),
        false,
    );
    f.bounds(
        (0, 0, 0),
        (0, 0, 13),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (13, 0, 0),
        (13, 0, 13),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (1, 0, 0),
        (12, 0, 0),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (1, 0, 13),
        (12, 0, 13),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    let mut v8 = 2;
    while v8 <= 11 {
        f.place(plain(Kind::SeaLantern), 0, 0, v8);
        f.place(plain(Kind::SeaLantern), 13, 0, v8);
        f.place(plain(Kind::SeaLantern), v8, 0, 0);
        v8 += 3;
    }
    f.bounds(
        (2, 0, 3),
        (4, 0, 9),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (9, 0, 3),
        (11, 0, 9),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (4, 0, 9),
        (9, 0, 11),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.place(plain(Kind::PrismarineBricks), 5, 0, 8);
    f.place(plain(Kind::PrismarineBricks), 8, 0, 8);
    f.place(plain(Kind::PrismarineBricks), 10, 0, 10);
    f.place(plain(Kind::PrismarineBricks), 3, 0, 10);
    f.bounds(
        (3, 0, 3),
        (3, 0, 7),
        plain(Kind::DarkPrismarine),
        plain(Kind::DarkPrismarine),
        false,
    );
    f.bounds(
        (10, 0, 3),
        (10, 0, 7),
        plain(Kind::DarkPrismarine),
        plain(Kind::DarkPrismarine),
        false,
    );
    f.bounds(
        (6, 0, 10),
        (7, 0, 10),
        plain(Kind::DarkPrismarine),
        plain(Kind::DarkPrismarine),
        false,
    );
    v8 = 3;
    let mut v9 = 0;
    while v9 < 2 {
        let mut v10 = 2;
        while v10 <= 8 {
            f.bounds(
                (v8, 0, v10),
                (v8, 2, v10),
                plain(Kind::PrismarineBricks),
                plain(Kind::PrismarineBricks),
                false,
            );
            v10 += 3;
        }
        v8 = 10;
        v9 += 1;
    }
    f.bounds(
        (5, 0, 10),
        (5, 2, 10),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (8, 0, 10),
        (8, 2, 10),
        plain(Kind::PrismarineBricks),
        plain(Kind::PrismarineBricks),
        false,
    );
    f.bounds(
        (6, -1, 7),
        (7, -1, 8),
        plain(Kind::DarkPrismarine),
        plain(Kind::DarkPrismarine),
        false,
    );
    water(f, 6, -1, 3, 7, -1, 4);
    elder(f, 6, 1, 6);
}

fn wing(f: &mut Frame, v1: i32, v2: i32) {
    if hits(f, v2, 0, v2 + 23, 20) {
        f.bounds(
            (v2, 0, 0),
            (v2 + 24, 0, 20),
            plain(Kind::Prismarine),
            plain(Kind::Prismarine),
            false,
        );
        water(f, v2, 1, 0, v2 + 24, 10, 20);
        let mut v7 = 0;
        while v7 < 4 {
            f.bounds(
                (v2 + v7, v7 + 1, v7),
                (v2 + v7, v7 + 1, 20),
                plain(Kind::PrismarineBricks),
                plain(Kind::PrismarineBricks),
                false,
            );
            f.bounds(
                ((v2 + v7) + 7, v7 + 5, v7 + 7),
                ((v2 + v7) + 7, v7 + 5, 20),
                plain(Kind::PrismarineBricks),
                plain(Kind::PrismarineBricks),
                false,
            );
            f.bounds(
                ((v2 + 17) - v7, v7 + 5, v7 + 7),
                ((v2 + 17) - v7, v7 + 5, 20),
                plain(Kind::PrismarineBricks),
                plain(Kind::PrismarineBricks),
                false,
            );
            f.bounds(
                ((v2 + 24) - v7, v7 + 1, v7),
                ((v2 + 24) - v7, v7 + 1, 20),
                plain(Kind::PrismarineBricks),
                plain(Kind::PrismarineBricks),
                false,
            );
            f.bounds(
                ((v2 + v7) + 1, v7 + 1, v7),
                ((v2 + 23) - v7, v7 + 1, v7),
                plain(Kind::PrismarineBricks),
                plain(Kind::PrismarineBricks),
                false,
            );
            f.bounds(
                ((v2 + v7) + 8, v7 + 5, v7 + 7),
                ((v2 + 16) - v7, v7 + 5, v7 + 7),
                plain(Kind::PrismarineBricks),
                plain(Kind::PrismarineBricks),
                false,
            );
            v7 += 1;
        }
        f.bounds(
            (v2 + 4, 4, 4),
            (v2 + 6, 4, 20),
            plain(Kind::Prismarine),
            plain(Kind::Prismarine),
            false,
        );
        f.bounds(
            (v2 + 7, 4, 4),
            (v2 + 17, 4, 6),
            plain(Kind::Prismarine),
            plain(Kind::Prismarine),
            false,
        );
        f.bounds(
            (v2 + 18, 4, 4),
            (v2 + 20, 4, 20),
            plain(Kind::Prismarine),
            plain(Kind::Prismarine),
            false,
        );
        f.bounds(
            (v2 + 11, 8, 11),
            (v2 + 13, 8, 20),
            plain(Kind::Prismarine),
            plain(Kind::Prismarine),
            false,
        );
        f.place(plain(Kind::PrismarineBricks), v2 + 12, 9, 12);
        f.place(plain(Kind::PrismarineBricks), v2 + 12, 9, 15);
        f.place(plain(Kind::PrismarineBricks), v2 + 12, 9, 18);
        v7 = v2 + if v1 == 0 { 5 } else { 19 };
        let v8 = v2 + if v1 == 0 { 19 } else { 5 };
        let mut v9 = 20;
        while v9 >= 5 {
            f.place(plain(Kind::PrismarineBricks), v7, 5, v9);
            v9 += -3;
        }
        v9 = 19;
        while v9 >= 7 {
            f.place(plain(Kind::PrismarineBricks), v8, 5, v9);
            v9 += -3;
        }
        v9 = 0;
        while v9 < 4 {
            let v10 = if v1 == 0 {
                (v2 + 17) - (v9 * 3)
            } else {
                (v2 + 24) - (17 - (v9 * 3))
            };
            f.place(plain(Kind::PrismarineBricks), v10, 5, 5);
            v9 += 1;
        }
        f.place(plain(Kind::PrismarineBricks), v8, 5, 5);
        f.bounds(
            (v2 + 11, 1, 12),
            (v2 + 13, 7, 12),
            plain(Kind::Prismarine),
            plain(Kind::Prismarine),
            false,
        );
        f.bounds(
            (v2 + 12, 1, 11),
            (v2 + 12, 7, 13),
            plain(Kind::Prismarine),
            plain(Kind::Prismarine),
            false,
        );
    }
}

fn entrance_archs(f: &mut Frame) {
    if hits(f, 22, 5, 35, 17) {
        water(f, 25, 0, 0, 32, 8, 20);
        let mut v4 = 0;
        while v4 < 4 {
            f.bounds(
                (24, 2, 5 + (v4 * 4)),
                (24, 4, 5 + (v4 * 4)),
                plain(Kind::PrismarineBricks),
                plain(Kind::PrismarineBricks),
                false,
            );
            f.bounds(
                (22, 4, 5 + (v4 * 4)),
                (23, 4, 5 + (v4 * 4)),
                plain(Kind::PrismarineBricks),
                plain(Kind::PrismarineBricks),
                false,
            );
            f.place(plain(Kind::PrismarineBricks), 25, 5, 5 + (v4 * 4));
            f.place(plain(Kind::PrismarineBricks), 26, 6, 5 + (v4 * 4));
            f.place(plain(Kind::SeaLantern), 26, 5, 5 + (v4 * 4));
            f.bounds(
                (33, 2, 5 + (v4 * 4)),
                (33, 4, 5 + (v4 * 4)),
                plain(Kind::PrismarineBricks),
                plain(Kind::PrismarineBricks),
                false,
            );
            f.bounds(
                (34, 4, 5 + (v4 * 4)),
                (35, 4, 5 + (v4 * 4)),
                plain(Kind::PrismarineBricks),
                plain(Kind::PrismarineBricks),
                false,
            );
            f.place(plain(Kind::PrismarineBricks), 32, 5, 5 + (v4 * 4));
            f.place(plain(Kind::PrismarineBricks), 31, 6, 5 + (v4 * 4));
            f.place(plain(Kind::SeaLantern), 31, 5, 5 + (v4 * 4));
            f.bounds(
                (27, 6, 5 + (v4 * 4)),
                (30, 6, 5 + (v4 * 4)),
                plain(Kind::Prismarine),
                plain(Kind::Prismarine),
                false,
            );
            v4 += 1;
        }
    }
}

fn entrance_wall(f: &mut Frame) {
    if hits(f, 15, 20, 42, 21) {
        f.bounds(
            (15, 0, 21),
            (42, 0, 21),
            plain(Kind::Prismarine),
            plain(Kind::Prismarine),
            false,
        );
        water(f, 26, 1, 21, 31, 3, 21);
        f.bounds(
            (21, 12, 21),
            (36, 12, 21),
            plain(Kind::Prismarine),
            plain(Kind::Prismarine),
            false,
        );
        f.bounds(
            (17, 11, 21),
            (40, 11, 21),
            plain(Kind::Prismarine),
            plain(Kind::Prismarine),
            false,
        );
        f.bounds(
            (16, 10, 21),
            (41, 10, 21),
            plain(Kind::Prismarine),
            plain(Kind::Prismarine),
            false,
        );
        f.bounds(
            (15, 7, 21),
            (42, 9, 21),
            plain(Kind::Prismarine),
            plain(Kind::Prismarine),
            false,
        );
        f.bounds(
            (16, 6, 21),
            (41, 6, 21),
            plain(Kind::Prismarine),
            plain(Kind::Prismarine),
            false,
        );
        f.bounds(
            (17, 5, 21),
            (40, 5, 21),
            plain(Kind::Prismarine),
            plain(Kind::Prismarine),
            false,
        );
        f.bounds(
            (21, 4, 21),
            (36, 4, 21),
            plain(Kind::Prismarine),
            plain(Kind::Prismarine),
            false,
        );
        f.bounds(
            (22, 3, 21),
            (26, 3, 21),
            plain(Kind::Prismarine),
            plain(Kind::Prismarine),
            false,
        );
        f.bounds(
            (31, 3, 21),
            (35, 3, 21),
            plain(Kind::Prismarine),
            plain(Kind::Prismarine),
            false,
        );
        f.bounds(
            (23, 2, 21),
            (25, 2, 21),
            plain(Kind::Prismarine),
            plain(Kind::Prismarine),
            false,
        );
        f.bounds(
            (32, 2, 21),
            (34, 2, 21),
            plain(Kind::Prismarine),
            plain(Kind::Prismarine),
            false,
        );
        f.bounds(
            (28, 4, 20),
            (29, 4, 21),
            plain(Kind::PrismarineBricks),
            plain(Kind::PrismarineBricks),
            false,
        );
        f.place(plain(Kind::PrismarineBricks), 27, 3, 21);
        f.place(plain(Kind::PrismarineBricks), 30, 3, 21);
        f.place(plain(Kind::PrismarineBricks), 26, 2, 21);
        f.place(plain(Kind::PrismarineBricks), 31, 2, 21);
        f.place(plain(Kind::PrismarineBricks), 25, 1, 21);
        f.place(plain(Kind::PrismarineBricks), 32, 1, 21);
        let mut v4 = 0;
        while v4 < 7 {
            f.place(plain(Kind::DarkPrismarine), 28 - v4, 6 + v4, 21);
            f.place(plain(Kind::DarkPrismarine), 29 + v4, 6 + v4, 21);
            v4 += 1;
        }
        v4 = 0;
        while v4 < 4 {
            f.place(plain(Kind::DarkPrismarine), 28 - v4, 9 + v4, 21);
            f.place(plain(Kind::DarkPrismarine), 29 + v4, 9 + v4, 21);
            v4 += 1;
        }
        f.place(plain(Kind::DarkPrismarine), 28, 12, 21);
        f.place(plain(Kind::DarkPrismarine), 29, 12, 21);
        v4 = 0;
        while v4 < 3 {
            f.place(plain(Kind::DarkPrismarine), 22 - (v4 * 2), 8, 21);
            f.place(plain(Kind::DarkPrismarine), 22 - (v4 * 2), 9, 21);
            f.place(plain(Kind::DarkPrismarine), 35 + (v4 * 2), 8, 21);
            f.place(plain(Kind::DarkPrismarine), 35 + (v4 * 2), 9, 21);
            v4 += 1;
        }
        water(f, 15, 13, 21, 42, 15, 21);
        water(f, 15, 1, 21, 15, 6, 21);
        water(f, 16, 1, 21, 16, 5, 21);
        water(f, 17, 1, 21, 20, 4, 21);
        water(f, 21, 1, 21, 21, 3, 21);
        water(f, 22, 1, 21, 22, 2, 21);
        water(f, 23, 1, 21, 24, 1, 21);
        water(f, 42, 1, 21, 42, 6, 21);
        water(f, 41, 1, 21, 41, 5, 21);
        water(f, 37, 1, 21, 40, 4, 21);
        water(f, 36, 1, 21, 36, 3, 21);
        water(f, 33, 1, 21, 34, 1, 21);
        water(f, 35, 1, 21, 35, 2, 21);
    }
}

fn roof_piece(f: &mut Frame) {
    if hits(f, 21, 21, 36, 36) {
        f.bounds(
            (21, 0, 22),
            (36, 0, 36),
            plain(Kind::Prismarine),
            plain(Kind::Prismarine),
            false,
        );
        water(f, 21, 1, 22, 36, 23, 36);
        let mut v4 = 0;
        while v4 < 4 {
            f.bounds(
                (21 + v4, 13 + v4, 21 + v4),
                (36 - v4, 13 + v4, 21 + v4),
                plain(Kind::PrismarineBricks),
                plain(Kind::PrismarineBricks),
                false,
            );
            f.bounds(
                (21 + v4, 13 + v4, 36 - v4),
                (36 - v4, 13 + v4, 36 - v4),
                plain(Kind::PrismarineBricks),
                plain(Kind::PrismarineBricks),
                false,
            );
            f.bounds(
                (21 + v4, 13 + v4, 22 + v4),
                (21 + v4, 13 + v4, 35 - v4),
                plain(Kind::PrismarineBricks),
                plain(Kind::PrismarineBricks),
                false,
            );
            f.bounds(
                (36 - v4, 13 + v4, 22 + v4),
                (36 - v4, 13 + v4, 35 - v4),
                plain(Kind::PrismarineBricks),
                plain(Kind::PrismarineBricks),
                false,
            );
            v4 += 1;
        }
        f.bounds(
            (25, 16, 25),
            (32, 16, 32),
            plain(Kind::Prismarine),
            plain(Kind::Prismarine),
            false,
        );
        f.bounds(
            (25, 17, 25),
            (25, 19, 25),
            plain(Kind::PrismarineBricks),
            plain(Kind::PrismarineBricks),
            false,
        );
        f.bounds(
            (32, 17, 25),
            (32, 19, 25),
            plain(Kind::PrismarineBricks),
            plain(Kind::PrismarineBricks),
            false,
        );
        f.bounds(
            (25, 17, 32),
            (25, 19, 32),
            plain(Kind::PrismarineBricks),
            plain(Kind::PrismarineBricks),
            false,
        );
        f.bounds(
            (32, 17, 32),
            (32, 19, 32),
            plain(Kind::PrismarineBricks),
            plain(Kind::PrismarineBricks),
            false,
        );
        f.place(plain(Kind::PrismarineBricks), 26, 20, 26);
        f.place(plain(Kind::PrismarineBricks), 27, 21, 27);
        f.place(plain(Kind::SeaLantern), 27, 20, 27);
        f.place(plain(Kind::PrismarineBricks), 26, 20, 31);
        f.place(plain(Kind::PrismarineBricks), 27, 21, 30);
        f.place(plain(Kind::SeaLantern), 27, 20, 30);
        f.place(plain(Kind::PrismarineBricks), 31, 20, 31);
        f.place(plain(Kind::PrismarineBricks), 30, 21, 30);
        f.place(plain(Kind::SeaLantern), 30, 20, 30);
        f.place(plain(Kind::PrismarineBricks), 31, 20, 26);
        f.place(plain(Kind::PrismarineBricks), 30, 21, 27);
        f.place(plain(Kind::SeaLantern), 30, 20, 27);
        f.bounds(
            (28, 21, 27),
            (29, 21, 27),
            plain(Kind::Prismarine),
            plain(Kind::Prismarine),
            false,
        );
        f.bounds(
            (27, 21, 28),
            (27, 21, 29),
            plain(Kind::Prismarine),
            plain(Kind::Prismarine),
            false,
        );
        f.bounds(
            (28, 21, 30),
            (29, 21, 30),
            plain(Kind::Prismarine),
            plain(Kind::Prismarine),
            false,
        );
        f.bounds(
            (30, 21, 28),
            (30, 21, 29),
            plain(Kind::Prismarine),
            plain(Kind::Prismarine),
            false,
        );
    }
}

fn lower_wall(f: &mut Frame) {
    if hits(f, 0, 21, 6, 58) {
        f.bounds(
            (0, 0, 21),
            (6, 0, 57),
            plain(Kind::Prismarine),
            plain(Kind::Prismarine),
            false,
        );
        water(f, 0, 1, 21, 6, 7, 57);
        f.bounds(
            (4, 4, 21),
            (6, 4, 53),
            plain(Kind::Prismarine),
            plain(Kind::Prismarine),
            false,
        );
        let mut v4 = 0;
        while v4 < 4 {
            f.bounds(
                (v4, v4 + 1, 21),
                (v4, v4 + 1, 57 - v4),
                plain(Kind::PrismarineBricks),
                plain(Kind::PrismarineBricks),
                false,
            );
            v4 += 1;
        }
        v4 = 23;
        while v4 < 53 {
            f.place(plain(Kind::PrismarineBricks), 5, 5, v4);
            v4 += 3;
        }
        f.place(plain(Kind::PrismarineBricks), 5, 5, 52);
        v4 = 0;
        while v4 < 4 {
            f.bounds(
                (v4, v4 + 1, 21),
                (v4, v4 + 1, 57 - v4),
                plain(Kind::PrismarineBricks),
                plain(Kind::PrismarineBricks),
                false,
            );
            v4 += 1;
        }
        f.bounds(
            (4, 1, 52),
            (6, 3, 52),
            plain(Kind::Prismarine),
            plain(Kind::Prismarine),
            false,
        );
        f.bounds(
            (5, 1, 51),
            (5, 3, 53),
            plain(Kind::Prismarine),
            plain(Kind::Prismarine),
            false,
        );
    }
    if hits(f, 51, 21, 58, 58) {
        f.bounds(
            (51, 0, 21),
            (57, 0, 57),
            plain(Kind::Prismarine),
            plain(Kind::Prismarine),
            false,
        );
        water(f, 51, 1, 21, 57, 7, 57);
        f.bounds(
            (51, 4, 21),
            (53, 4, 53),
            plain(Kind::Prismarine),
            plain(Kind::Prismarine),
            false,
        );
        let mut v4 = 0;
        while v4 < 4 {
            f.bounds(
                (57 - v4, v4 + 1, 21),
                (57 - v4, v4 + 1, 57 - v4),
                plain(Kind::PrismarineBricks),
                plain(Kind::PrismarineBricks),
                false,
            );
            v4 += 1;
        }
        v4 = 23;
        while v4 < 53 {
            f.place(plain(Kind::PrismarineBricks), 52, 5, v4);
            v4 += 3;
        }
        f.place(plain(Kind::PrismarineBricks), 52, 5, 52);
        f.bounds(
            (51, 1, 52),
            (53, 3, 52),
            plain(Kind::Prismarine),
            plain(Kind::Prismarine),
            false,
        );
        f.bounds(
            (52, 1, 51),
            (52, 3, 53),
            plain(Kind::Prismarine),
            plain(Kind::Prismarine),
            false,
        );
    }
    if hits(f, 0, 51, 57, 57) {
        f.bounds(
            (7, 0, 51),
            (50, 0, 57),
            plain(Kind::Prismarine),
            plain(Kind::Prismarine),
            false,
        );
        water(f, 7, 1, 51, 50, 10, 57);
        let mut v4 = 0;
        while v4 < 4 {
            f.bounds(
                (v4 + 1, v4 + 1, 57 - v4),
                (56 - v4, v4 + 1, 57 - v4),
                plain(Kind::PrismarineBricks),
                plain(Kind::PrismarineBricks),
                false,
            );
            v4 += 1;
        }
    }
}

fn middle_wall(f: &mut Frame) {
    if hits(f, 7, 21, 13, 50) {
        f.bounds(
            (7, 0, 21),
            (13, 0, 50),
            plain(Kind::Prismarine),
            plain(Kind::Prismarine),
            false,
        );
        water(f, 7, 1, 21, 13, 10, 50);
        f.bounds(
            (11, 8, 21),
            (13, 8, 53),
            plain(Kind::Prismarine),
            plain(Kind::Prismarine),
            false,
        );
        let mut v4 = 0;
        while v4 < 4 {
            f.bounds(
                (v4 + 7, v4 + 5, 21),
                (v4 + 7, v4 + 5, 54),
                plain(Kind::PrismarineBricks),
                plain(Kind::PrismarineBricks),
                false,
            );
            v4 += 1;
        }
        v4 = 21;
        while v4 <= 45 {
            f.place(plain(Kind::PrismarineBricks), 12, 9, v4);
            v4 += 3;
        }
    }
    if hits(f, 44, 21, 50, 54) {
        f.bounds(
            (44, 0, 21),
            (50, 0, 50),
            plain(Kind::Prismarine),
            plain(Kind::Prismarine),
            false,
        );
        water(f, 44, 1, 21, 50, 10, 50);
        f.bounds(
            (44, 8, 21),
            (46, 8, 53),
            plain(Kind::Prismarine),
            plain(Kind::Prismarine),
            false,
        );
        let mut v4 = 0;
        while v4 < 4 {
            f.bounds(
                (50 - v4, v4 + 5, 21),
                (50 - v4, v4 + 5, 54),
                plain(Kind::PrismarineBricks),
                plain(Kind::PrismarineBricks),
                false,
            );
            v4 += 1;
        }
        v4 = 21;
        while v4 <= 45 {
            f.place(plain(Kind::PrismarineBricks), 45, 9, v4);
            v4 += 3;
        }
    }
    if hits(f, 8, 44, 49, 54) {
        f.bounds(
            (14, 0, 44),
            (43, 0, 50),
            plain(Kind::Prismarine),
            plain(Kind::Prismarine),
            false,
        );
        water(f, 14, 1, 44, 43, 10, 50);
        let mut v4 = 12;
        while v4 <= 45 {
            f.place(plain(Kind::PrismarineBricks), v4, 9, 45);
            f.place(plain(Kind::PrismarineBricks), v4, 9, 52);
            if v4 == 12 || v4 == 18 || v4 == 24 || v4 == 33 || v4 == 39 || v4 == 45 {
                f.place(plain(Kind::PrismarineBricks), v4, 9, 47);
                f.place(plain(Kind::PrismarineBricks), v4, 9, 50);
                f.place(plain(Kind::PrismarineBricks), v4, 10, 45);
                f.place(plain(Kind::PrismarineBricks), v4, 10, 46);
                f.place(plain(Kind::PrismarineBricks), v4, 10, 51);
                f.place(plain(Kind::PrismarineBricks), v4, 10, 52);
                f.place(plain(Kind::PrismarineBricks), v4, 11, 47);
                f.place(plain(Kind::PrismarineBricks), v4, 11, 50);
                f.place(plain(Kind::PrismarineBricks), v4, 12, 48);
                f.place(plain(Kind::PrismarineBricks), v4, 12, 49);
            }
            v4 += 3;
        }
        v4 = 0;
        while v4 < 3 {
            f.bounds(
                (8 + v4, 5 + v4, 54),
                (49 - v4, 5 + v4, 54),
                plain(Kind::Prismarine),
                plain(Kind::Prismarine),
                false,
            );
            v4 += 1;
        }
        f.bounds(
            (11, 8, 54),
            (46, 8, 54),
            plain(Kind::PrismarineBricks),
            plain(Kind::PrismarineBricks),
            false,
        );
        f.bounds(
            (14, 8, 44),
            (43, 8, 53),
            plain(Kind::Prismarine),
            plain(Kind::Prismarine),
            false,
        );
    }
}

fn upper_wall(f: &mut Frame) {
    if hits(f, 14, 21, 20, 43) {
        f.bounds(
            (14, 0, 21),
            (20, 0, 43),
            plain(Kind::Prismarine),
            plain(Kind::Prismarine),
            false,
        );
        water(f, 14, 1, 22, 20, 14, 43);
        f.bounds(
            (18, 12, 22),
            (20, 12, 39),
            plain(Kind::Prismarine),
            plain(Kind::Prismarine),
            false,
        );
        f.bounds(
            (18, 12, 21),
            (20, 12, 21),
            plain(Kind::PrismarineBricks),
            plain(Kind::PrismarineBricks),
            false,
        );
        let mut v4 = 0;
        while v4 < 4 {
            f.bounds(
                (v4 + 14, v4 + 9, 21),
                (v4 + 14, v4 + 9, 43 - v4),
                plain(Kind::PrismarineBricks),
                plain(Kind::PrismarineBricks),
                false,
            );
            v4 += 1;
        }
        v4 = 23;
        while v4 <= 39 {
            f.place(plain(Kind::PrismarineBricks), 19, 13, v4);
            v4 += 3;
        }
    }
    if hits(f, 37, 21, 43, 43) {
        f.bounds(
            (37, 0, 21),
            (43, 0, 43),
            plain(Kind::Prismarine),
            plain(Kind::Prismarine),
            false,
        );
        water(f, 37, 1, 22, 43, 14, 43);
        f.bounds(
            (37, 12, 22),
            (39, 12, 39),
            plain(Kind::Prismarine),
            plain(Kind::Prismarine),
            false,
        );
        f.bounds(
            (37, 12, 21),
            (39, 12, 21),
            plain(Kind::PrismarineBricks),
            plain(Kind::PrismarineBricks),
            false,
        );
        let mut v4 = 0;
        while v4 < 4 {
            f.bounds(
                (43 - v4, v4 + 9, 21),
                (43 - v4, v4 + 9, 43 - v4),
                plain(Kind::PrismarineBricks),
                plain(Kind::PrismarineBricks),
                false,
            );
            v4 += 1;
        }
        v4 = 23;
        while v4 <= 39 {
            f.place(plain(Kind::PrismarineBricks), 38, 13, v4);
            v4 += 3;
        }
    }
    if hits(f, 15, 37, 42, 43) {
        f.bounds(
            (21, 0, 37),
            (36, 0, 43),
            plain(Kind::Prismarine),
            plain(Kind::Prismarine),
            false,
        );
        water(f, 21, 1, 37, 36, 14, 43);
        f.bounds(
            (21, 12, 37),
            (36, 12, 39),
            plain(Kind::Prismarine),
            plain(Kind::Prismarine),
            false,
        );
        let mut v4 = 0;
        while v4 < 4 {
            f.bounds(
                (15 + v4, v4 + 9, 43 - v4),
                (42 - v4, v4 + 9, 43 - v4),
                plain(Kind::PrismarineBricks),
                plain(Kind::PrismarineBricks),
                false,
            );
            v4 += 1;
        }
        v4 = 21;
        while v4 <= 36 {
            f.place(plain(Kind::PrismarineBricks), v4, 13, 38);
            v4 += 3;
        }
    }
}
