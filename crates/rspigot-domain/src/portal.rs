use rspigot_blocks::BlockKind;

use crate::BlockPos;
use crate::ids::BlockStateId as StateId;

pub const MIN_WIDTH: i32 = 2;
pub const MAX_WIDTH: i32 = 21;
pub const MIN_HEIGHT: i32 = 3;
pub const MAX_HEIGHT: i32 = 21;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Axis {
    X,
    Z,
}

impl Axis {
    pub const fn other(self) -> Self {
        match self {
            Self::X => Self::Z,
            Self::Z => Self::X,
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::X => "x",
            Self::Z => "z",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "x" => Some(Self::X),
            "z" => Some(Self::Z),
            _ => None,
        }
    }

    const fn right(self) -> (i32, i32) {
        match self {
            Self::X => (-1, 0),
            Self::Z => (0, 1),
        }
    }
}

fn is_frame(state: Option<StateId>) -> bool {
    state.is_some_and(|state| state.kind() == Some(BlockKind::Obsidian))
}

fn is_empty(state: Option<StateId>) -> bool {
    state.is_some_and(|state| {
        state.is_air()
            || matches!(
                state.kind(),
                Some(BlockKind::Fire | BlockKind::SoulFire | BlockKind::NetherPortal)
            )
    })
}

fn is_portal(state: Option<StateId>) -> bool {
    state.is_some_and(|state| state.kind() == Some(BlockKind::NetherPortal))
}

#[derive(Debug, Clone, Default)]
struct Recorder(Vec<BlockPos>);

impl Recorder {
    fn record(&mut self, pos: BlockPos) {
        self.0.retain(|seen| *seen != pos);
        self.0.push(pos);
    }
}

#[derive(Debug, Clone)]
pub struct PortalShape {
    pub axis: Axis,
    pub portal_blocks: i32,
    pub bottom_left: BlockPos,
    pub width: i32,
    pub height: i32,
    right: (i32, i32),
    recorded: Vec<BlockPos>,
}

fn step(pos: BlockPos, (dx, dz): (i32, i32), count: i32) -> BlockPos {
    pos.offset(dx * count, 0, dz * count)
}

fn distance_until_edge(
    block: &impl Fn(BlockPos) -> Option<StateId>,
    pos: BlockPos,
    direction: (i32, i32),
    recorder: &mut Recorder,
) -> i32 {
    for i in 0..=21 {
        let at = step(pos, direction, i);
        if !is_empty(block(at)) {
            return if is_frame(block(at)) {
                recorder.record(at);
                i
            } else {
                0
            };
        }
        let below = at.offset(0, -1, 0);
        if !is_frame(block(below)) {
            return 0;
        }
        recorder.record(below);
    }
    0
}

fn bottom_left(
    block: &impl Fn(BlockPos) -> Option<StateId>,
    min_y: i32,
    right: (i32, i32),
    start: BlockPos,
    recorder: &mut Recorder,
) -> Option<BlockPos> {
    let mut pos = start;
    let limit = min_y.max(start.y - 21);
    while pos.y > limit && is_empty(block(pos.offset(0, -1, 0))) {
        pos = pos.offset(0, -1, 0);
    }
    let left = (-right.0, -right.1);
    let distance = distance_until_edge(block, pos, left, recorder) - 1;
    (distance >= 0).then(|| step(pos, left, distance))
}

fn distance_until_top(
    block: &impl Fn(BlockPos) -> Option<StateId>,
    bottom_left: BlockPos,
    right: (i32, i32),
    width: i32,
    portal_blocks: &mut i32,
    recorder: &mut Recorder,
) -> i32 {
    for height in 0..21 {
        let row = bottom_left.offset(0, height, 0);
        let left_edge = step(row, right, -1);
        if !is_frame(block(left_edge)) {
            return height;
        }
        let right_edge = step(row, right, width);
        if !is_frame(block(right_edge)) {
            return height;
        }
        for i in 0..width {
            let cell = block(step(row, right, i));
            if !is_empty(cell) {
                return height;
            }
            if is_portal(cell) {
                *portal_blocks += 1;
            }
        }
        recorder.record(left_edge);
        recorder.record(right_edge);
    }
    21
}

fn has_top_frame(
    block: &impl Fn(BlockPos) -> Option<StateId>,
    bottom_left: BlockPos,
    right: (i32, i32),
    width: i32,
    height: i32,
    recorder: &mut Recorder,
) -> bool {
    for i in 0..width {
        let at = step(bottom_left.offset(0, height, 0), right, i);
        if !is_frame(block(at)) {
            return false;
        }
        recorder.record(at);
    }
    true
}

impl PortalShape {
    pub fn find_any(
        block: &impl Fn(BlockPos) -> Option<StateId>,
        min_y: i32,
        pos: BlockPos,
        axis: Axis,
    ) -> Self {
        let right = axis.right();
        let mut recorder = Recorder::default();
        let empty = |anchor: BlockPos, recorder: Recorder| Self {
            axis,
            portal_blocks: 0,
            bottom_left: anchor,
            width: 0,
            height: 0,
            right,
            recorded: recorder.0,
        };
        let Some(corner) = bottom_left(block, min_y, right, pos, &mut recorder) else {
            return empty(pos, recorder);
        };
        let width = distance_until_edge(block, corner, right, &mut recorder);
        if !(MIN_WIDTH..=MAX_WIDTH).contains(&width) {
            return empty(corner, recorder);
        }
        let mut portal_blocks = 0;
        let top = distance_until_top(
            block,
            corner,
            right,
            width,
            &mut portal_blocks,
            &mut recorder,
        );
        let height = if top > 2
            && top < 22
            && has_top_frame(block, corner, right, width, top, &mut recorder)
        {
            top
        } else {
            0
        };
        Self {
            axis,
            portal_blocks,
            bottom_left: corner,
            width,
            height,
            right,
            recorded: recorder.0,
        }
    }

    pub fn find_empty(
        block: &impl Fn(BlockPos) -> Option<StateId>,
        min_y: i32,
        pos: BlockPos,
        preferred: Axis,
    ) -> Option<Self> {
        [preferred, preferred.other()]
            .into_iter()
            .map(|axis| Self::find_any(block, min_y, pos, axis))
            .find(|shape| shape.is_valid() && shape.portal_blocks == 0)
    }

    pub fn is_valid(&self) -> bool {
        (MIN_WIDTH..=MAX_WIDTH).contains(&self.width)
            && (MIN_HEIGHT..=MAX_HEIGHT).contains(&self.height)
    }

    pub fn is_complete(&self) -> bool {
        self.is_valid() && self.portal_blocks == self.width * self.height
    }

    pub fn interior(&self) -> Vec<BlockPos> {
        let far = step(
            self.bottom_left.offset(0, self.height - 1, 0),
            self.right,
            self.width - 1,
        );
        let (min_x, max_x) = (self.bottom_left.x.min(far.x), self.bottom_left.x.max(far.x));
        let (min_z, max_z) = (self.bottom_left.z.min(far.z), self.bottom_left.z.max(far.z));
        let min_y = self.bottom_left.y;
        let mut cells = Vec::new();
        for z in min_z..=max_z {
            for y in min_y..min_y + self.height {
                for x in min_x..=max_x {
                    cells.push(BlockPos::new(x, y, z));
                }
            }
        }
        cells
    }

    pub fn frame(&self) -> &[BlockPos] {
        &self.recorded
    }

    pub fn blocks_touched(&self) -> Vec<BlockPos> {
        let mut list = self.recorded.clone();
        for cell in self.interior() {
            list.retain(|seen| *seen != cell);
            list.push(cell);
        }
        list
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rectangle {
    pub min_corner: BlockPos,
    pub axis1_size: i32,
    pub axis2_size: i32,
}

pub fn largest_rectangle_around(
    pos: BlockPos,
    axis: Axis,
    max1: i32,
    max2: i32,
    test: impl Fn(BlockPos) -> bool,
) -> Rectangle {
    let (w, h) = (2 * max1 - 1, 2 * max2 - 1);
    let at = |a: i32, y: i32| match axis {
        Axis::X => pos.offset(a, y, 0),
        Axis::Z => pos.offset(0, y, a),
    };
    let mut sums = vec![0i32; ((w + 1) * (h + 1)) as usize];
    let index = |i: i32, j: i32| (j * (w + 1) + i) as usize;
    for j in 0..h {
        for i in 0..w {
            let pass = i32::from(test(at(i - (max1 - 1), j - (max2 - 1))));
            sums[index(i + 1, j + 1)] =
                pass + sums[index(i, j + 1)] + sums[index(i + 1, j)] - sums[index(i, j)];
        }
    }
    let full = |i0: i32, i1: i32, j0: i32, j1: i32| {
        let count = sums[index(i1 + 1, j1 + 1)] - sums[index(i0, j1 + 1)] - sums[index(i1 + 1, j0)]
            + sums[index(i0, j0)];
        count == (i1 - i0 + 1) * (j1 - j0 + 1)
    };
    let mut best = Rectangle {
        min_corner: pos,
        axis1_size: 0,
        axis2_size: 0,
    };
    let mut best_area = 0;
    let (c1, c2) = (max1 - 1, max2 - 1);
    for i0 in 0..=c1 {
        for i1 in c1..w.min(i0 + max1) {
            for j0 in 0..=c2 {
                for j1 in c2..h.min(j0 + max2) {
                    let area = (i1 - i0 + 1) * (j1 - j0 + 1);
                    if area > best_area && full(i0, i1, j0, j1) {
                        best_area = area;
                        best = Rectangle {
                            min_corner: at(i0 - c1, j0 - c2),
                            axis1_size: i1 - i0 + 1,
                            axis2_size: j1 - j0 + 1,
                        };
                    }
                }
            }
        }
    }
    best
}

pub struct Terrain<'a> {
    pub block: &'a dyn Fn(BlockPos) -> Option<StateId>,
    pub replaceable: &'a dyn Fn(StateId) -> bool,
    pub solid: &'a dyn Fn(StateId) -> bool,
    pub motion_blocking_height: &'a dyn Fn(i32, i32) -> i32,
    pub within_border: &'a dyn Fn(i32, i32) -> bool,
    pub clamp_to_border: &'a dyn Fn(BlockPos) -> BlockPos,
    pub min_y: i32,
    pub top_y: i32,
}

#[derive(Debug, Clone)]
pub struct CreatedPortal {
    pub changes: Vec<(BlockPos, StateId)>,
    pub exit: BlockPos,
}

impl Terrain<'_> {
    fn can_replace(&self, pos: BlockPos) -> bool {
        (self.block)(pos).is_some_and(|state| (self.replaceable)(state))
    }

    fn can_host_frame(&self, origin: BlockPos, direction: (i32, i32), offset: i32) -> bool {
        let clockwise = (-direction.1, direction.0);
        for i in -1..=2 {
            for j in -1..=3 {
                let at = origin.offset(
                    direction.0 * i + clockwise.0 * offset,
                    j,
                    direction.1 * i + clockwise.1 * offset,
                );
                if j < 0 && !(self.block)(at).is_some_and(|state| (self.solid)(state)) {
                    return false;
                }
                if j >= 0 && !self.can_replace(at) {
                    return false;
                }
            }
        }
        true
    }
}

fn positive(axis: Axis) -> (i32, i32) {
    match axis {
        Axis::X => (1, 0),
        Axis::Z => (0, 1),
    }
}

fn spiral(origin: BlockPos, radius: i32) -> Vec<(i32, i32)> {
    let mut cells = Vec::new();
    let (mut x, mut z) = (0i32, 0i32);
    let (mut dx, mut dz) = (1i32, 0i32);
    let limit = (2 * radius + 1) * (2 * radius + 1);
    let mut leg = 1;
    let mut walked = 0;
    let mut turns = 0;
    cells.push((origin.x, origin.z));
    while (cells.len() as i32) < limit {
        x += dx;
        z += dz;
        if x.abs() <= radius && z.abs() <= radius {
            cells.push((origin.x + x, origin.z + z));
        }
        walked += 1;
        if walked == leg {
            walked = 0;
            (dx, dz) = (-dz, dx);
            turns += 1;
            if turns % 2 == 0 {
                leg += 1;
            }
        }
        if leg > 2 * radius + 2 {
            break;
        }
    }
    cells
}

impl Terrain<'_> {
    pub fn create_portal(
        &self,
        origin: BlockPos,
        axis: Axis,
        radius: i32,
    ) -> Option<CreatedPortal> {
        let direction = positive(axis);
        let (mut best, mut best_distance) = (None, -1.0f64);
        let (mut fallback, mut fallback_distance) = (None, -1.0f64);
        let dist = |a: BlockPos, b: BlockPos| {
            let (dx, dy, dz) = (
                f64::from(a.x - b.x),
                f64::from(a.y - b.y),
                f64::from(a.z - b.z),
            );
            dx * dx + dy * dy + dz * dz
        };
        for (x, z) in spiral(origin, radius) {
            let top = self.top_y.min((self.motion_blocking_height)(x, z));
            if !(self.within_border)(x, z)
                || !(self.within_border)(x + direction.0, z + direction.1)
            {
                continue;
            }
            let mut y = top;
            while y >= self.min_y {
                let at = BlockPos::new(x, y, z);
                if self.can_replace(at) {
                    let start = y;
                    while y > self.min_y && self.can_replace(BlockPos::new(x, y - 1, z)) {
                        y -= 1;
                    }
                    if y + 4 <= self.top_y {
                        let gap = start - y;
                        if gap <= 0 || gap >= 3 {
                            let floor = BlockPos::new(x, y, z);
                            if self.can_host_frame(floor, direction, 0) {
                                let distance = dist(origin, floor);
                                if self.can_host_frame(floor, direction, -1)
                                    && self.can_host_frame(floor, direction, 1)
                                    && (best_distance == -1.0 || best_distance > distance)
                                {
                                    best_distance = distance;
                                    best = Some(floor);
                                }
                                if best_distance == -1.0
                                    && (fallback_distance == -1.0 || fallback_distance > distance)
                                {
                                    fallback_distance = distance;
                                    fallback = Some(floor);
                                }
                            }
                        }
                    }
                }
                y -= 1;
            }
        }
        let mut position = best;
        if best_distance == -1.0 && fallback_distance != -1.0 {
            position = fallback;
        }
        let mut changes: Vec<(BlockPos, StateId)> = Vec::new();
        let mut put = |pos: BlockPos, state: StateId| {
            changes.retain(|(seen, _)| *seen != pos);
            changes.push((pos, state));
        };
        let obsidian = BlockKind::Obsidian.default_state();
        let air = BlockKind::Air.default_state();
        let base = match position {
            Some(found) => found,
            None => {
                let lowest = (self.min_y + 1).max(70);
                let highest = self.top_y - 9;
                if highest < lowest {
                    return None;
                }
                let base = (self.clamp_to_border)(BlockPos::new(
                    origin.x - direction.0,
                    origin.y.clamp(lowest, highest),
                    origin.z - direction.1,
                ));
                let clockwise = (-direction.1, direction.0);
                for side in -1..2 {
                    for depth in 0..2 {
                        for height in -1..3 {
                            let at = base.offset(
                                depth * direction.0 + side * clockwise.0,
                                height,
                                depth * direction.1 + side * clockwise.1,
                            );
                            put(at, if height < 0 { obsidian } else { air });
                        }
                    }
                }
                base
            }
        };
        for i in -1..3 {
            for j in -1..4 {
                if i == -1 || i == 2 || j == -1 || j == 3 {
                    put(base.offset(direction.0 * i, j, direction.1 * i), obsidian);
                }
            }
        }
        let portal = BlockKind::NetherPortal
            .state_with(&[("axis", axis.name())])
            .unwrap_or_else(|| BlockKind::NetherPortal.default_state());
        for i in 0..2 {
            for j in 0..3 {
                put(base.offset(direction.0 * i, j, direction.1 * i), portal);
            }
        }
        Some(CreatedPortal {
            changes,
            exit: base,
        })
    }
}

#[cfg(test)]
mod tests;
