use crate::legacy::JavaRandom;
use crate::structures::pool::shuffle;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Face {
    North,
    East,
    South,
    West,
}

impl Face {
    pub(crate) const HORIZONTAL: [Self; 4] = [Self::North, Self::East, Self::South, Self::West];

    pub(crate) fn step(self) -> (i32, i32) {
        match self {
            Self::North => (0, -1),
            Self::East => (1, 0),
            Self::South => (0, 1),
            Self::West => (-1, 0),
        }
    }

    pub(crate) fn clockwise(self) -> Self {
        match self {
            Self::North => Self::East,
            Self::East => Self::South,
            Self::South => Self::West,
            Self::West => Self::North,
        }
    }

    pub(crate) fn counterclockwise(self) -> Self {
        match self {
            Self::North => Self::West,
            Self::West => Self::South,
            Self::South => Self::East,
            Self::East => Self::North,
        }
    }

    pub(crate) fn opposite(self) -> Self {
        self.clockwise().clockwise()
    }

    fn from_2d(value: i32) -> Self {
        match value & 3 {
            0 => Self::South,
            1 => Self::West,
            2 => Self::North,
            _ => Self::East,
        }
    }
}

pub(crate) const ROOM_ID: i32 = 0xFFFF;
pub(crate) const ROOM_KIND: i32 = 0xF_0000;
pub(crate) const ROOM_1X1: i32 = 0x1_0000;
pub(crate) const ROOM_1X2: i32 = 0x2_0000;
pub(crate) const ROOM_2X2: i32 = 0x4_0000;
pub(crate) const ORIGIN: i32 = 0x10_0000;
pub(crate) const STAIRS: i32 = 0x20_0000;
pub(crate) const STAIRS_UP: i32 = 0x40_0000;
pub(crate) const BLOCKED: i32 = 0x80_0000;

#[derive(Debug, Clone)]
pub(crate) struct Grid {
    pub(crate) width: i32,
    pub(crate) height: i32,
    outside: i32,
    cells: Vec<i32>,
}

impl Grid {
    pub(crate) fn new(width: i32, height: i32, outside: i32) -> Self {
        Self {
            width,
            height,
            outside,
            cells: vec![0; (width * height) as usize],
        }
    }

    fn index(&self, x: i32, y: i32) -> Option<usize> {
        (x >= 0 && x < self.width && y >= 0 && y < self.height)
            .then(|| (x * self.height + y) as usize)
    }

    pub(crate) fn get(&self, x: i32, y: i32) -> i32 {
        self.index(x, y).map_or(self.outside, |i| self.cells[i])
    }

    pub(crate) fn set(&mut self, x: i32, y: i32, value: i32) {
        if let Some(i) = self.index(x, y) {
            self.cells[i] = value;
        }
    }

    pub(crate) fn fill(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, value: i32) {
        for y in y0..=y1 {
            for x in x0..=x1 {
                self.set(x, y, value);
            }
        }
    }

    fn set_if(&mut self, x: i32, y: i32, only: i32, value: i32) {
        if self.get(x, y) == only {
            self.set(x, y, value);
        }
    }

    fn edges_to(&self, x: i32, y: i32, value: i32) -> bool {
        self.get(x - 1, y) == value
            || self.get(x + 1, y) == value
            || self.get(x, y + 1) == value
            || self.get(x, y - 1) == value
    }
}

pub(crate) fn is_house(grid: &Grid, x: i32, y: i32) -> bool {
    matches!(grid.get(x, y), 1..=4)
}

#[derive(Debug)]
pub(crate) struct Mansion {
    pub(crate) base: Grid,
    pub(crate) rooms: [Grid; 3],
    pub(crate) third: Grid,
    pub(crate) entrance_x: i32,
    pub(crate) entrance_y: i32,
}

fn corridor(random: &mut JavaRandom, grid: &mut Grid, x: i32, y: i32, heading: Face, depth: i32) {
    if depth < 1 {
        return;
    }
    let (hx, hz) = heading.step();
    grid.set(x, y, 1);
    grid.set_if(x + hx, y + hz, 0, 1);
    for _ in 0..8 {
        let dir = Face::from_2d(random.next_int(4));
        if dir == heading.opposite() || (dir == Face::East && random.next_boolean()) {
            continue;
        }
        let (dx, dz) = dir.step();
        let (j, k) = (x + hx, y + hz);
        if grid.get(j + dx, k + dz) == 0 && grid.get(j + dx * 2, k + dz * 2) == 0 {
            corridor(random, grid, x + hx + dx, y + hz + dz, dir, depth - 1);
            break;
        }
    }
    let cw = heading.clockwise();
    let ccw = heading.counterclockwise();
    let (cx, cz) = cw.step();
    let (wx, wz) = ccw.step();
    grid.set_if(x + cx, y + cz, 0, 2);
    grid.set_if(x + wx, y + wz, 0, 2);
    grid.set_if(x + hx + cx, y + hz + cz, 0, 2);
    grid.set_if(x + hx + wx, y + hz + wz, 0, 2);
    grid.set_if(x + hx * 2, y + hz * 2, 0, 2);
    grid.set_if(x + cx * 2, y + cz * 2, 0, 2);
    grid.set_if(x + wx * 2, y + wz * 2, 0, 2);
}

fn clean_edges(grid: &mut Grid) -> bool {
    let mut changed = false;
    for y in 0..grid.height {
        for x in 0..grid.width {
            if grid.get(x, y) != 0 {
                continue;
            }
            let near = i32::from(is_house(grid, x + 1, y))
                + i32::from(is_house(grid, x - 1, y))
                + i32::from(is_house(grid, x, y + 1))
                + i32::from(is_house(grid, x, y - 1));
            if near >= 3 {
                grid.set(x, y, 2);
                changed = true;
            } else if near == 2 {
                let diagonal = i32::from(is_house(grid, x + 1, y + 1))
                    + i32::from(is_house(grid, x - 1, y + 1))
                    + i32::from(is_house(grid, x + 1, y - 1))
                    + i32::from(is_house(grid, x - 1, y - 1));
                if diagonal <= 1 {
                    grid.set(x, y, 2);
                    changed = true;
                }
            }
        }
    }
    changed
}

fn identify_rooms(random: &mut JavaRandom, from: &Grid, rooms: &mut Grid) {
    let mut cells = Vec::new();
    for y in 0..from.height {
        for x in 0..from.width {
            if from.get(x, y) == 2 {
                cells.push((x, y));
            }
        }
    }
    shuffle(&mut cells, random);
    let mut id = 10;
    for (x, y) in cells {
        if rooms.get(x, y) != 0 {
            continue;
        }
        let (mut min_x, mut max_x, mut min_y, mut max_y) = (x, x, y, y);
        let mut kind = ROOM_1X1;
        let open =
            |dx: i32, dy: i32| rooms.get(x + dx, y + dy) == 0 && from.get(x + dx, y + dy) == 2;
        if open(1, 0) && open(0, 1) && open(1, 1) {
            max_x += 1;
            max_y += 1;
            kind = ROOM_2X2;
        } else if open(-1, 0) && open(0, 1) && open(-1, 1) {
            min_x -= 1;
            max_y += 1;
            kind = ROOM_2X2;
        } else if open(-1, 0) && open(0, -1) && open(-1, -1) {
            min_x -= 1;
            min_y -= 1;
            kind = ROOM_2X2;
        } else if open(1, 0) {
            max_x += 1;
            kind = ROOM_1X2;
        } else if open(0, 1) {
            max_y += 1;
            kind = ROOM_1X2;
        } else if open(-1, 0) {
            min_x -= 1;
            kind = ROOM_1X2;
        } else if open(0, -1) {
            min_y -= 1;
            kind = ROOM_1X2;
        }
        let mut px = if random.next_boolean() { min_x } else { max_x };
        let mut py = if random.next_boolean() { min_y } else { max_y };
        let mut stairs = STAIRS;
        if !from.edges_to(px, py, 1) {
            px = if px == min_x { max_x } else { min_x };
            py = if py == min_y { max_y } else { min_y };
            if !from.edges_to(px, py, 1) {
                py = if py == min_y { max_y } else { min_y };
                if !from.edges_to(px, py, 1) {
                    px = if px == min_x { max_x } else { min_x };
                    py = if py == min_y { max_y } else { min_y };
                    if !from.edges_to(px, py, 1) {
                        stairs = 0;
                        px = min_x;
                        py = min_y;
                    }
                }
            }
        }
        for cy in min_y..=max_y {
            for cx in min_x..=max_x {
                if cx == px && cy == py {
                    rooms.set(cx, cy, stairs | ORIGIN | kind | id);
                } else {
                    rooms.set(cx, cy, kind | id);
                }
            }
        }
        id += 1;
    }
}

impl Mansion {
    pub(crate) fn is_room(&self, floor: usize, x: i32, y: i32, id: i32) -> bool {
        self.rooms[floor].get(x, y) & ROOM_ID == id
    }

    pub(crate) fn pair_direction(&self, floor: usize, x: i32, y: i32, id: i32) -> Option<Face> {
        Face::HORIZONTAL.into_iter().find(|face| {
            let (dx, dz) = face.step();
            self.is_room(floor, x + dx, y + dz, id)
        })
    }

    pub(crate) fn generate(random: &mut JavaRandom) -> Self {
        let (ex, ey) = (7, 4);
        let mut base = Grid::new(11, 11, 5);
        base.fill(ex, ey, ex + 1, ey + 1, 3);
        base.fill(ex - 1, ey, ex - 1, ey + 1, 2);
        base.fill(ex + 2, ey - 2, ex + 3, ey + 3, 5);
        base.fill(ex + 1, ey - 2, ex + 1, ey - 1, 1);
        base.fill(ex + 1, ey + 2, ex + 1, ey + 3, 1);
        base.set(ex - 1, ey - 1, 1);
        base.set(ex - 1, ey + 2, 1);
        base.fill(0, 0, 11, 1, 5);
        base.fill(0, 9, 11, 11, 5);
        corridor(random, &mut base, ex, ey - 2, Face::West, 6);
        corridor(random, &mut base, ex, ey + 3, Face::West, 6);
        corridor(random, &mut base, ex - 2, ey - 1, Face::West, 3);
        corridor(random, &mut base, ex - 2, ey + 2, Face::West, 3);
        while clean_edges(&mut base) {}
        let mut rooms = [
            Grid::new(11, 11, 5),
            Grid::new(11, 11, 5),
            Grid::new(11, 11, 5),
        ];
        identify_rooms(random, &base, &mut rooms[0]);
        identify_rooms(random, &base, &mut rooms[1]);
        rooms[0].fill(ex + 1, ey, ex + 1, ey + 1, BLOCKED);
        rooms[1].fill(ex + 1, ey, ex + 1, ey + 1, BLOCKED);
        let third = Grid::new(base.width, base.height, 5);
        let mut mansion = Self {
            base,
            rooms,
            third,
            entrance_x: ex,
            entrance_y: ey,
        };
        mansion.setup_third(random);
        let third = mansion.third.clone();
        identify_rooms(random, &third, &mut mansion.rooms[2]);
        mansion
    }

    fn clear_third(&mut self) {
        let (w, h) = (self.third.width, self.third.height);
        self.third.fill(0, 0, w, h, 5);
    }

    fn setup_third(&mut self, random: &mut JavaRandom) {
        let mut stairs = Vec::new();
        for y in 0..self.third.height {
            for x in 0..self.third.width {
                let value = self.rooms[1].get(x, y);
                if value & ROOM_KIND == ROOM_1X2 && value & STAIRS == STAIRS {
                    stairs.push((x, y));
                }
            }
        }
        if stairs.is_empty() {
            self.clear_third();
            return;
        }
        let (px, py) = stairs[random.next_int(stairs.len() as i32) as usize];
        let value = self.rooms[1].get(px, py);
        self.rooms[1].set(px, py, value | STAIRS_UP);
        let Some(toward) = self.pair_direction(1, px, py, value & ROOM_ID) else {
            self.clear_third();
            self.rooms[1].set(px, py, value);
            return;
        };
        let (tx, tz) = toward.step();
        let (qx, qy) = (px + tx, py + tz);
        for y in 0..self.third.height {
            for x in 0..self.third.width {
                if !is_house(&self.base, x, y) {
                    self.third.set(x, y, 5);
                } else if x == px && y == py {
                    self.third.set(x, y, 3);
                } else if x == qx && y == qy {
                    self.third.set(x, y, 3);
                    self.rooms[2].set(x, y, BLOCKED);
                }
            }
        }
        let open: Vec<Face> = Face::HORIZONTAL
            .into_iter()
            .filter(|face| {
                let (dx, dz) = face.step();
                self.third.get(qx + dx, qy + dz) == 0
            })
            .collect();
        if open.is_empty() {
            self.clear_third();
            self.rooms[1].set(px, py, value);
            return;
        }
        let heading = open[random.next_int(open.len() as i32) as usize];
        let (hx, hz) = heading.step();
        corridor(random, &mut self.third, qx + hx, qy + hz, heading, 4);
        while clean_edges(&mut self.third) {}
    }
}
