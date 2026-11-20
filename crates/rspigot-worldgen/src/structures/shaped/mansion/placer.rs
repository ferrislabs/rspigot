use super::grid::{
    BLOCKED, Face, Grid, Mansion, ORIGIN, ROOM_1X1, ROOM_1X2, ROOM_2X2, ROOM_ID, ROOM_KIND, STAIRS,
    STAIRS_UP, is_house,
};
use super::names::{self, Floor};
use crate::legacy::JavaRandom;
use crate::overworld::features::structure::Rotation;
use crate::structures::bbox::Pos;
use crate::structures::shaped::tpl::Mirror;

#[derive(Debug, Clone, Copy)]
pub(crate) struct Spec {
    pub(crate) name: &'static str,
    pub(crate) pos: Pos,
    pub(crate) rotation: Rotation,
    pub(crate) mirror: Mirror,
}

#[derive(Debug, Clone, Copy)]
enum Exit {
    None,
    Up,
    Side(Face),
}

#[derive(Debug, Clone, Copy)]
struct Cursor {
    pos: Pos,
    rotation: Rotation,
    wall: &'static str,
}

fn turns(rotation: Rotation) -> i32 {
    match rotation {
        Rotation::None => 0,
        Rotation::Clockwise90 => 1,
        Rotation::Clockwise180 => 2,
        Rotation::Counterclockwise90 => 3,
    }
}

fn combine(first: Rotation, second: Rotation) -> Rotation {
    Rotation::ALL[((turns(first) + turns(second)) & 3) as usize]
}

fn facing(rotation: Rotation, face: Face) -> Face {
    match rotation {
        Rotation::None => face,
        Rotation::Clockwise90 => face.clockwise(),
        Rotation::Clockwise180 => face.opposite(),
        Rotation::Counterclockwise90 => face.counterclockwise(),
    }
}

fn shift(pos: Pos, face: Face, amount: i32) -> Pos {
    let (dx, dz) = face.step();
    (pos.0 + dx * amount, pos.1, pos.2 + dz * amount)
}

fn up(pos: Pos, amount: i32) -> Pos {
    (pos.0, pos.1 + amount, pos.2)
}

fn zero_offset(rotation: Rotation, size: i32) -> (i32, i32) {
    let edge = size - 1;
    match rotation {
        Rotation::Counterclockwise90 => (0, edge),
        Rotation::Clockwise90 => (edge, 0),
        Rotation::Clockwise180 => (edge, edge),
        Rotation::None => (0, 0),
    }
}

pub(crate) struct Placer<'a> {
    random: &'a mut JavaRandom,
    start_x: i32,
    start_y: i32,
    out: Vec<Spec>,
}

impl Placer<'_> {
    fn add(&mut self, name: &'static str, pos: Pos, rotation: Rotation) {
        self.add_mirrored(name, pos, rotation, Mirror::None);
    }

    fn add_mirrored(&mut self, name: &'static str, pos: Pos, rotation: Rotation, mirror: Mirror) {
        self.out.push(Spec {
            name,
            pos,
            rotation,
            mirror,
        });
    }

    fn cell(&self, rotation: Rotation, origin: Pos, x: i32, y: i32) -> Pos {
        let south = shift(
            origin,
            facing(rotation, Face::South),
            8 + (y - self.start_y) * 8,
        );
        shift(south, facing(rotation, Face::East), (x - self.start_x) * 8)
    }

    fn entrance(&mut self, cursor: &mut Cursor) {
        let at = shift(cursor.pos, facing(cursor.rotation, Face::West), 9);
        self.add(names::ENTRANCE, at, cursor.rotation);
        cursor.pos = shift(cursor.pos, facing(cursor.rotation, Face::South), 16);
    }

    fn wall_piece(&mut self, cursor: &mut Cursor) {
        let at = shift(cursor.pos, facing(cursor.rotation, Face::East), 7);
        self.add(cursor.wall, at, cursor.rotation);
        cursor.pos = shift(cursor.pos, facing(cursor.rotation, Face::South), 8);
    }

    fn turn(&mut self, cursor: &mut Cursor) {
        cursor.pos = shift(cursor.pos, facing(cursor.rotation, Face::South), -1);
        self.add(names::WALL_CORNER, cursor.pos, cursor.rotation);
        cursor.pos = shift(cursor.pos, facing(cursor.rotation, Face::South), -7);
        cursor.pos = shift(cursor.pos, facing(cursor.rotation, Face::West), -6);
        cursor.rotation = combine(cursor.rotation, Rotation::Clockwise90);
    }

    fn inner_turn(&mut self, cursor: &mut Cursor) {
        cursor.pos = shift(cursor.pos, facing(cursor.rotation, Face::South), 6);
        cursor.pos = shift(cursor.pos, facing(cursor.rotation, Face::East), 8);
        cursor.rotation = combine(cursor.rotation, Rotation::Counterclockwise90);
    }

    fn outer_walls(
        &mut self,
        cursor: &mut Cursor,
        grid: &Grid,
        heading: Face,
        start: (i32, i32),
        end: (i32, i32),
    ) {
        let first = heading;
        let mut heading = heading;
        let (mut x, mut y) = start;
        loop {
            let (hx, hz) = heading.step();
            if !is_house(grid, x + hx, y + hz) {
                self.turn(cursor);
                heading = heading.clockwise();
                if (x, y) != end || first != heading {
                    self.wall_piece(cursor);
                }
            } else {
                let (lx, lz) = heading.counterclockwise().step();
                if is_house(grid, x + hx + lx, y + hz + lz) {
                    self.inner_turn(cursor);
                    x += hx;
                    y += hz;
                    heading = heading.counterclockwise();
                } else {
                    x += hx;
                    y += hz;
                    if (x, y) != end || first != heading {
                        self.wall_piece(cursor);
                    }
                }
            }
            if (x, y) == end && first == heading {
                return;
            }
        }
    }

    fn roof(&mut self, origin: Pos, rotation: Rotation, grid: &Grid, above: Option<&Grid>) {
        let covered = |x: i32, y: i32| above.is_some_and(|above| is_house(above, x, y));
        let rot = |extra: Rotation| combine(rotation, extra);
        for y in 0..grid.height {
            for x in 0..grid.width {
                let p = self.cell(rotation, origin, x, y);
                if !is_house(grid, x, y) || covered(x, y) {
                    continue;
                }
                self.add(names::ROOF, up(p, 3), rotation);
                if !is_house(grid, x + 1, y) {
                    let at = shift(p, facing(rotation, Face::East), 6);
                    self.add(names::ROOF_FRONT, at, rotation);
                }
                if !is_house(grid, x - 1, y) {
                    let at = shift(p, facing(rotation, Face::South), 7);
                    self.add(names::ROOF_FRONT, at, rot(Rotation::Clockwise180));
                }
                if !is_house(grid, x, y - 1) {
                    let at = shift(p, facing(rotation, Face::West), 1);
                    self.add(names::ROOF_FRONT, at, rot(Rotation::Counterclockwise90));
                }
                if !is_house(grid, x, y + 1) {
                    let at = shift(
                        shift(p, facing(rotation, Face::East), 6),
                        facing(rotation, Face::South),
                        6,
                    );
                    self.add(names::ROOF_FRONT, at, rot(Rotation::Clockwise90));
                }
            }
        }
        if above.is_some() {
            for y in 0..grid.height {
                for x in 0..grid.width {
                    let p = self.cell(rotation, origin, x, y);
                    if !is_house(grid, x, y) || !covered(x, y) {
                        continue;
                    }
                    let east = facing(rotation, Face::East);
                    let west = facing(rotation, Face::West);
                    let north = facing(rotation, Face::North);
                    let south = facing(rotation, Face::South);
                    let open_east = !is_house(grid, x + 1, y);
                    let open_west = !is_house(grid, x - 1, y);
                    let open_north = !is_house(grid, x, y - 1);
                    let open_south = !is_house(grid, x, y + 1);
                    if open_east {
                        self.add(names::SMALL_WALL, shift(p, east, 7), rotation);
                    }
                    if open_west {
                        let at = shift(shift(p, west, 1), south, 6);
                        self.add(names::SMALL_WALL, at, rot(Rotation::Clockwise180));
                    }
                    if open_north {
                        let at = shift(shift(p, west, 0), north, 1);
                        self.add(names::SMALL_WALL, at, rot(Rotation::Counterclockwise90));
                    }
                    if open_south {
                        let at = shift(shift(p, east, 6), south, 7);
                        self.add(names::SMALL_WALL, at, rot(Rotation::Clockwise90));
                    }
                    if open_east {
                        if open_north {
                            let at = shift(shift(p, east, 7), north, 2);
                            self.add(names::SMALL_WALL_CORNER, at, rotation);
                        }
                        if open_south {
                            let at = shift(shift(p, east, 8), south, 7);
                            self.add(names::SMALL_WALL_CORNER, at, rot(Rotation::Clockwise90));
                        }
                    }
                    if open_west {
                        if open_north {
                            let at = shift(shift(p, west, 2), north, 1);
                            self.add(
                                names::SMALL_WALL_CORNER,
                                at,
                                rot(Rotation::Counterclockwise90),
                            );
                        }
                        if open_south {
                            let at = shift(shift(p, west, 1), south, 8);
                            self.add(names::SMALL_WALL_CORNER, at, rot(Rotation::Clockwise180));
                        }
                    }
                }
            }
        }
        for y in 0..grid.height {
            for x in 0..grid.width {
                let p = self.cell(rotation, origin, x, y);
                if !is_house(grid, x, y) || covered(x, y) {
                    continue;
                }
                let east = facing(rotation, Face::East);
                let north = facing(rotation, Face::North);
                let south = facing(rotation, Face::South);
                let west = facing(rotation, Face::West);
                if !is_house(grid, x + 1, y) {
                    let q = shift(p, east, 6);
                    if !is_house(grid, x, y + 1) {
                        self.add(names::ROOF_CORNER, shift(q, south, 6), rotation);
                    } else if is_house(grid, x + 1, y + 1) {
                        self.add(names::ROOF_INNER_CORNER, shift(q, south, 5), rotation);
                    }
                    if !is_house(grid, x, y - 1) {
                        self.add(names::ROOF_CORNER, q, rot(Rotation::Counterclockwise90));
                    } else if is_house(grid, x + 1, y - 1) {
                        let at = shift(shift(q, east, 3), north, 2);
                        self.add(names::ROOF_INNER_CORNER, at, rot(Rotation::Clockwise90));
                    }
                }
                if !is_house(grid, x - 1, y) {
                    if !is_house(grid, x, y + 1) {
                        self.add(
                            names::ROOF_CORNER,
                            shift(p, south, 6),
                            rot(Rotation::Clockwise90),
                        );
                    } else if is_house(grid, x - 1, y + 1) {
                        let at = shift(shift(p, south, 8), west, 3);
                        self.add(
                            names::ROOF_INNER_CORNER,
                            at,
                            rot(Rotation::Counterclockwise90),
                        );
                    }
                    if !is_house(grid, x, y - 1) {
                        self.add(names::ROOF_CORNER, p, rot(Rotation::Clockwise180));
                    } else if is_house(grid, x - 1, y - 1) {
                        self.add(
                            names::ROOF_INNER_CORNER,
                            shift(p, south, 1),
                            rot(Rotation::Clockwise180),
                        );
                    }
                }
            }
        }
    }

    fn room_1x1(&mut self, at: Pos, rotation: Rotation, exit: Exit, floor: Floor) {
        let mut turn = Rotation::None;
        let mut name = floor.one_by_one(self.random);
        match exit {
            Exit::Side(Face::East) => {}
            Exit::Side(Face::North) => turn = combine(turn, Rotation::Counterclockwise90),
            Exit::Side(Face::West) => turn = combine(turn, Rotation::Clockwise180),
            Exit::Side(Face::South) => turn = combine(turn, Rotation::Clockwise90),
            Exit::Up | Exit::None => name = floor.one_by_one_secret(self.random),
        }
        let (zx, zz) = zero_offset(turn, 7);
        let turned = combine(turn, rotation);
        let (rx, _, rz) = rotation.transform((1 + zx, 0, zz));
        self.add(name, (at.0 + rx, at.1, at.2 + rz), turned);
    }

    fn room_1x2(
        &mut self,
        at: Pos,
        rotation: Rotation,
        room: Face,
        exit: Exit,
        floor: Floor,
        stairs: bool,
    ) {
        use Face::{East, North, South, West};
        let rel = |east: i32, south: i32| {
            shift(
                shift(at, facing(rotation, East), east),
                facing(rotation, South),
                south,
            )
        };
        let r = |extra: Rotation| combine(rotation, extra);
        let side = |this: &mut Self| floor.side_entrance(this.random, stairs);
        let front = |this: &mut Self| floor.front_entrance(this.random, stairs);
        match (exit, room) {
            (Exit::Side(East), South) => {
                let name = side(self);
                self.add(name, rel(1, 0), rotation);
            }
            (Exit::Side(East), North) => {
                let name = side(self);
                self.add_mirrored(name, rel(1, 6), rotation, Mirror::LeftRight);
            }
            (Exit::Side(West), North) => {
                let name = side(self);
                self.add(name, rel(7, 6), r(Rotation::Clockwise180));
            }
            (Exit::Side(West), South) => {
                let name = side(self);
                self.add_mirrored(name, rel(7, 0), rotation, Mirror::FrontBack);
            }
            (Exit::Side(South), East) => {
                let name = side(self);
                self.add_mirrored(name, rel(1, 0), r(Rotation::Clockwise90), Mirror::LeftRight);
            }
            (Exit::Side(South), West) => {
                let name = side(self);
                self.add(name, rel(7, 0), r(Rotation::Clockwise90));
            }
            (Exit::Side(North), West) => {
                let name = side(self);
                self.add_mirrored(name, rel(7, 6), r(Rotation::Clockwise90), Mirror::FrontBack);
            }
            (Exit::Side(North), East) => {
                let name = side(self);
                self.add(name, rel(1, 6), r(Rotation::Counterclockwise90));
            }
            (Exit::Side(South), North) => {
                let name = front(self);
                self.add(name, rel(1, -8), rotation);
            }
            (Exit::Side(North), South) => {
                let name = front(self);
                self.add(name, rel(7, 14), r(Rotation::Clockwise180));
            }
            (Exit::Side(West), East) => {
                let name = front(self);
                self.add(name, rel(15, 0), r(Rotation::Clockwise90));
            }
            (Exit::Side(East), West) => {
                let name = front(self);
                self.add(name, rel(-7, 6), r(Rotation::Counterclockwise90));
            }
            (Exit::Up, East) => {
                let name = floor.one_by_two_secret(self.random);
                self.add(name, rel(15, 0), r(Rotation::Clockwise90));
            }
            (Exit::Up, South) => {
                let name = floor.one_by_two_secret(self.random);
                self.add(name, rel(1, 0), rotation);
            }
            _ => {}
        }
    }

    fn room_2x2(&mut self, at: Pos, rotation: Rotation, room: Face, exit: Exit, floor: Floor) {
        use Face::{East, North, South, West};
        let mut east = 0;
        let mut south = 0;
        let mut turned = rotation;
        let mut mirror = Mirror::None;
        match (exit, room) {
            (Exit::Side(East), South) => east = -7,
            (Exit::Side(East), North) => {
                east = -7;
                south = 6;
                mirror = Mirror::LeftRight;
            }
            (Exit::Side(North), East) => {
                east = 1;
                south = 14;
                turned = combine(rotation, Rotation::Counterclockwise90);
            }
            (Exit::Side(North), West) => {
                east = 7;
                south = 14;
                turned = combine(rotation, Rotation::Counterclockwise90);
                mirror = Mirror::LeftRight;
            }
            (Exit::Side(South), West) => {
                east = 7;
                south = -8;
                turned = combine(rotation, Rotation::Clockwise90);
            }
            (Exit::Side(South), East) => {
                east = 1;
                south = -8;
                turned = combine(rotation, Rotation::Clockwise90);
                mirror = Mirror::LeftRight;
            }
            (Exit::Side(West), North) => {
                east = 15;
                south = 6;
                turned = combine(rotation, Rotation::Clockwise180);
            }
            (Exit::Side(West), South) => {
                east = 15;
                mirror = Mirror::FrontBack;
            }
            _ => {}
        }
        let pos = shift(
            shift(at, facing(rotation, East), east),
            facing(rotation, South),
            south,
        );
        let name = floor.two_by_two(self.random);
        self.add_mirrored(name, pos, turned, mirror);
    }

    fn room_2x2_secret(&mut self, at: Pos, rotation: Rotation) {
        let pos = shift(at, facing(rotation, Face::East), 1);
        self.add(names::SECRET_2X2, pos, rotation);
    }

    fn floors(&mut self, mansion: &Mansion, origin: Pos, rotation: Rotation) {
        for level in 0..3usize {
            let floor_origin = up(origin, 8 * level as i32 + if level == 2 { 3 } else { 0 });
            let rooms = &mansion.rooms[level];
            let walls = if level == 2 {
                &mansion.third
            } else {
                &mansion.base
            };
            let tier = usize::from(level != 0);
            self.corridors(walls, rooms, floor_origin, rotation, tier);
            self.partitions(mansion, walls, floor_origin, rotation, level);
        }
    }

    fn corridors(
        &mut self,
        walls: &Grid,
        rooms: &Grid,
        origin: Pos,
        rotation: Rotation,
        tier: usize,
    ) {
        let blocked = |x: i32, y: i32| rooms.get(x, y) & BLOCKED == BLOCKED;
        for y in 0..walls.height {
            for x in 0..walls.width {
                if walls.get(x, y) != 1 {
                    continue;
                }
                let at = self.cell(rotation, origin, x, y);
                self.add(names::CORRIDOR, at, rotation);
                if walls.get(x, y - 1) == 1 || blocked(x, y - 1) {
                    let pos = up(shift(at, facing(rotation, Face::East), 1), 1);
                    self.add(names::CARPET_NORTH, pos, rotation);
                }
                if walls.get(x + 1, y) == 1 || blocked(x + 1, y) {
                    let pos = up(
                        shift(
                            shift(at, facing(rotation, Face::South), 1),
                            facing(rotation, Face::East),
                            5,
                        ),
                        1,
                    );
                    self.add(names::CARPET_EAST, pos, rotation);
                }
                if walls.get(x, y + 1) == 1 || blocked(x, y + 1) {
                    let pos = shift(
                        shift(at, facing(rotation, Face::South), 5),
                        facing(rotation, Face::West),
                        1,
                    );
                    self.add(names::CARPETS_SOUTH[tier], pos, rotation);
                }
                if walls.get(x - 1, y) == 1 || blocked(x - 1, y) {
                    let pos = shift(
                        shift(at, facing(rotation, Face::West), 1),
                        facing(rotation, Face::North),
                        1,
                    );
                    self.add(names::CARPETS_WEST[tier], pos, rotation);
                }
            }
        }
    }

    fn partitions(
        &mut self,
        mansion: &Mansion,
        walls: &Grid,
        origin: Pos,
        rotation: Rotation,
        level: usize,
    ) {
        let tier = usize::from(level != 0);
        let floor = Floor::of(level);
        let rooms = &mansion.rooms[level];
        let wall = names::INDOOR_WALLS[tier];
        let door = names::INDOOR_DOORS[tier];
        for y in 0..walls.height {
            for x in 0..walls.width {
                let top_stairs = level == 2 && walls.get(x, y) == 3;
                if walls.get(x, y) != 2 && !top_stairs {
                    continue;
                }
                let value = rooms.get(x, y);
                let kind = value & ROOM_KIND;
                let id = value & ROOM_ID;
                let blocked_stairs = top_stairs && value & BLOCKED == BLOCKED;
                let mut candidates = Vec::new();
                if value & STAIRS == STAIRS {
                    for face in Face::HORIZONTAL {
                        let (dx, dz) = face.step();
                        if walls.get(x + dx, y + dz) == 1 {
                            candidates.push(face);
                        }
                    }
                }
                let exit = if !candidates.is_empty() {
                    Exit::Side(candidates[self.random.next_int(candidates.len() as i32) as usize])
                } else if value & ORIGIN == ORIGIN {
                    Exit::Up
                } else {
                    Exit::None
                };
                let base = shift(
                    shift(
                        origin,
                        facing(rotation, Face::South),
                        8 + (y - self.start_y) * 8,
                    ),
                    facing(rotation, Face::East),
                    -1 + (x - self.start_x) * 8,
                );
                let pick = |face: Face| {
                    if matches!(exit, Exit::Side(chosen) if chosen == face) {
                        door
                    } else {
                        wall
                    }
                };
                if is_house(walls, x - 1, y) && !mansion.is_room(level, x - 1, y, id) {
                    self.add(pick(Face::West), base, rotation);
                }
                if walls.get(x + 1, y) == 1 && !blocked_stairs {
                    let at = shift(base, facing(rotation, Face::East), 8);
                    self.add(pick(Face::East), at, rotation);
                }
                if is_house(walls, x, y + 1) && !mansion.is_room(level, x, y + 1, id) {
                    let at = shift(
                        shift(base, facing(rotation, Face::South), 7),
                        facing(rotation, Face::East),
                        7,
                    );
                    self.add(
                        pick(Face::South),
                        at,
                        combine(rotation, Rotation::Clockwise90),
                    );
                }
                if walls.get(x, y - 1) == 1 && !blocked_stairs {
                    let at = shift(
                        shift(base, facing(rotation, Face::North), 1),
                        facing(rotation, Face::East),
                        7,
                    );
                    self.add(
                        pick(Face::North),
                        at,
                        combine(rotation, Rotation::Clockwise90),
                    );
                }
                if kind == ROOM_1X1 {
                    self.room_1x1(base, rotation, exit, floor);
                } else if kind == ROOM_1X2 && !matches!(exit, Exit::None) {
                    if let Some(room) = mansion.pair_direction(level, x, y, id) {
                        let stairs = value & STAIRS_UP == STAIRS_UP;
                        self.room_1x2(base, rotation, room, exit, floor, stairs);
                    }
                } else if kind == ROOM_2X2 {
                    match exit {
                        Exit::Side(side) => {
                            let mut room = side.clockwise();
                            let (dx, dz) = room.step();
                            if !mansion.is_room(level, x + dx, y + dz, id) {
                                room = room.opposite();
                            }
                            self.room_2x2(base, rotation, room, exit, floor);
                        }
                        Exit::Up => self.room_2x2_secret(base, rotation),
                        Exit::None => {}
                    }
                }
            }
        }
    }
}

pub(crate) fn mansion_pieces(
    origin: Pos,
    rotation: Rotation,
    random: &mut JavaRandom,
) -> Vec<Spec> {
    let mansion = Mansion::generate(random);
    let mut placer = Placer {
        random,
        start_x: mansion.entrance_x + 1,
        start_y: mansion.entrance_y + 1,
        out: Vec::new(),
    };
    let mut ground = Cursor {
        pos: origin,
        rotation,
        wall: names::WALL_FLAT,
    };
    placer.entrance(&mut ground);
    let mut upper = Cursor {
        pos: up(ground.pos, 8),
        rotation: ground.rotation,
        wall: names::WALL_WINDOW,
    };
    let start = (placer.start_x, placer.start_y);
    let end = (mansion.entrance_x + 1, mansion.entrance_y);
    placer.outer_walls(&mut ground, &mansion.base, Face::South, start, end);
    placer.outer_walls(&mut upper, &mansion.base, Face::South, start, end);
    let mut top = Cursor {
        pos: up(ground.pos, 19),
        rotation: ground.rotation,
        wall: names::WALL_WINDOW,
    };
    let mut found = false;
    let mut y = 0;
    while y < mansion.third.height && !found {
        let mut x = mansion.third.width - 1;
        while x >= 0 && !found {
            if is_house(&mansion.third, x, y) {
                top.pos = shift(
                    top.pos,
                    facing(rotation, Face::South),
                    8 + (y - placer.start_y) * 8,
                );
                top.pos = shift(
                    top.pos,
                    facing(rotation, Face::East),
                    (x - placer.start_x) * 8,
                );
                placer.wall_piece(&mut top);
                placer.outer_walls(&mut top, &mansion.third, Face::South, (x, y), (x, y));
                found = true;
            }
            x -= 1;
        }
        y += 1;
    }
    placer.roof(
        up(origin, 16),
        rotation,
        &mansion.base,
        Some(&mansion.third),
    );
    placer.roof(up(origin, 27), rotation, &mansion.third, None);
    placer.floors(&mansion, origin, rotation);
    placer.out
}
