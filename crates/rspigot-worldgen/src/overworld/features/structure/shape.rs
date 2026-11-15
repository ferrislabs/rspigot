use rspigot_blocks::{BlockKind as Kind, Face, Support};
use rspigot_domain::BlockStateId;

use super::super::place::Run;
use super::super::prov::Pos;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Dir {
    West,
    East,
    North,
    South,
    Down,
    Up,
}

pub(super) const UPDATE_ORDER: [Dir; 6] = [
    Dir::West,
    Dir::East,
    Dir::North,
    Dir::South,
    Dir::Down,
    Dir::Up,
];

impl Dir {
    pub(super) fn offset(self, (x, y, z): Pos, steps: i32) -> Pos {
        match self {
            Self::West => (x - steps, y, z),
            Self::East => (x + steps, y, z),
            Self::North => (x, y, z - steps),
            Self::South => (x, y, z + steps),
            Self::Down => (x, y - steps, z),
            Self::Up => (x, y + steps, z),
        }
    }

    pub(super) fn opposite(self) -> Self {
        match self {
            Self::West => Self::East,
            Self::East => Self::West,
            Self::North => Self::South,
            Self::South => Self::North,
            Self::Down => Self::Up,
            Self::Up => Self::Down,
        }
    }

    pub(super) fn along(axis: usize, positive: bool) -> Self {
        match (axis, positive) {
            (0, false) => Self::West,
            (0, true) => Self::East,
            (1, false) => Self::Down,
            (1, true) => Self::Up,
            (_, false) => Self::North,
            (_, true) => Self::South,
        }
    }
}

fn speleothem(state: BlockStateId) -> bool {
    matches!(
        state.kind(),
        Some(Kind::PointedDripstone | Kind::SulfurSpike)
    )
}

fn tip_direction(state: BlockStateId) -> Option<Dir> {
    match state.property("vertical_direction") {
        Some("up") => Some(Dir::Up),
        Some("down") => Some(Dir::Down),
        _ => None,
    }
}

fn pointing(state: BlockStateId, dir: Dir) -> bool {
    speleothem(state) && tip_direction(state) == Some(dir)
}

fn survives(run: &mut Run, state: BlockStateId, pos: Pos, tip: Dir) -> bool {
    let behind = tip.offset(pos, -1);
    let held = run.region.get(behind.0, behind.1, behind.2);
    let support = match tip {
        Dir::Down => Face::Down,
        _ => Face::Up,
    };
    held.is_face_sturdy(support, Support::Full)
        || (pointing(held, tip) && held.kind() == state.kind())
}

fn thickness(run: &mut Run, state: BlockStateId, pos: Pos, tip: Dir) -> &'static str {
    let merging = state.property("thickness") == Some("tip_merge");
    let ahead = tip.offset(pos, 1);
    let front = run.region.get(ahead.0, ahead.1, ahead.2);
    if pointing(front, tip.opposite()) && front.kind() == state.kind() {
        return if merging || front.property("thickness") == Some("tip_merge") {
            "tip_merge"
        } else {
            "tip"
        };
    }
    if !pointing(front, tip) {
        return "tip";
    }
    if matches!(front.property("thickness"), Some("tip" | "tip_merge")) {
        return "frustum";
    }
    let behind = tip.offset(pos, -1);
    let held = run.region.get(behind.0, behind.1, behind.2);
    if pointing(held, tip) {
        "middle"
    } else {
        "base"
    }
}

pub(super) fn update_shape(run: &mut Run, state: BlockStateId, pos: Pos, dir: Dir) -> BlockStateId {
    if !speleothem(state) || !matches!(dir, Dir::Up | Dir::Down) {
        return state;
    }
    let Some(tip) = tip_direction(state) else {
        return state;
    };
    if dir == tip.opposite() && !survives(run, state, pos, tip) {
        return state;
    }
    let next = thickness(run, state, pos, tip);
    state.with_property("thickness", next).unwrap_or(state)
}

pub(super) fn update_from_neighbours(run: &mut Run, state: BlockStateId, pos: Pos) -> BlockStateId {
    UPDATE_ORDER
        .iter()
        .fold(state, |current, dir| update_shape(run, current, pos, *dir))
}

pub(super) struct Shape {
    low: Pos,
    size: [i32; 3],
    cells: Vec<bool>,
}

fn pick(cycle: usize, (x, y, z): Pos, axis: usize) -> i32 {
    let triple = match cycle {
        0 => [x, y, z],
        1 => [z, x, y],
        _ => [y, z, x],
    };
    triple[axis]
}

fn cycled(cycle: usize, axis: usize) -> usize {
    match cycle {
        0 => axis,
        1 => (axis + 1) % 3,
        _ => (axis + 2) % 3,
    }
}

impl Shape {
    pub(super) fn of(placed: &[Pos]) -> Option<Self> {
        let first = *placed.first()?;
        let (mut low, mut high) = (first, first);
        for &(x, y, z) in placed {
            low = (low.0.min(x), low.1.min(y), low.2.min(z));
            high = (high.0.max(x), high.1.max(y), high.2.max(z));
        }
        let size = [high.0 - low.0 + 1, high.1 - low.1 + 1, high.2 - low.2 + 1];
        let mut cells = vec![false; (size[0] * size[1] * size[2]) as usize];
        for &(x, y, z) in placed {
            let at = ((x - low.0) * size[1] + (y - low.1)) * size[2] + (z - low.2);
            cells[at as usize] = true;
        }
        Some(Self { low, size, cells })
    }

    fn full(&self, (x, y, z): Pos) -> bool {
        let at = (x * self.size[1] + y) * self.size[2] + z;
        self.cells[at as usize]
    }

    pub(super) fn faces(&self) -> Vec<(Dir, Pos)> {
        let mut faces = Vec::new();
        for cycle in 0..3 {
            let inverse = match cycle {
                0 => 0,
                1 => 2,
                _ => 1,
            };
            let world = |x: i32, y: i32, z: i32| {
                (
                    pick(inverse, (x, y, z), 0),
                    pick(inverse, (x, y, z), 1),
                    pick(inverse, (x, y, z), 2),
                )
            };
            let axis_z = cycled(inverse, 2);
            let size_x = self.size[cycled(inverse, 0)];
            let size_y = self.size[cycled(inverse, 1)];
            let size_z = self.size[axis_z];
            for x in 0..size_x {
                for y in 0..size_y {
                    let mut was_full = false;
                    for z in 0..=size_z {
                        let is_full = z != size_z && self.full(world(x, y, z));
                        if !was_full && is_full {
                            faces.push((Dir::along(axis_z, false), self.absolute(world(x, y, z))));
                        }
                        if was_full && !is_full {
                            faces.push((
                                Dir::along(axis_z, true),
                                self.absolute(world(x, y, z - 1)),
                            ));
                        }
                        was_full = is_full;
                    }
                }
            }
        }
        faces
    }

    fn absolute(&self, (x, y, z): Pos) -> Pos {
        (self.low.0 + x, self.low.1 + y, self.low.2 + z)
    }
}

pub(super) fn settle(run: &mut Run, placed: &[Pos]) {
    let Some(shape) = Shape::of(placed) else {
        return;
    };
    for (dir, pos) in shape.faces() {
        let near = dir.offset(pos, 1);
        let state = run.region.get(pos.0, pos.1, pos.2);
        let across = run.region.get(near.0, near.1, near.2);
        let next = update_shape(run, state, pos, dir);
        if next != state {
            run.region.set(pos.0, pos.1, pos.2, next);
        }
        let across_next = update_shape(run, across, near, dir.opposite());
        if across_next != across {
            run.region.set(near.0, near.1, near.2, across_next);
        }
    }
    for &pos in placed {
        let state = run.region.get(pos.0, pos.1, pos.2);
        let next = update_from_neighbours(run, state, pos);
        if next != state {
            run.region.set(pos.0, pos.1, pos.2, next);
        }
    }
}
