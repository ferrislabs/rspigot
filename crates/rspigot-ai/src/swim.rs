use rspigot_blocks::{BlockKind, StateFlags, StateId};
use rspigot_domain::BlockPos;

use crate::PathType;
use crate::classify::{BlockClass, class_of};
use crate::view::BlockView;
use crate::walk::Walker;

const DIRECTIONS: [(i32, i32, i32); 6] = [
    (0, -1, 0),
    (0, 1, 0),
    (0, 0, -1),
    (0, 0, 1),
    (-1, 0, 0),
    (1, 0, 0),
];
const HORIZONTAL_CLOCKWISE: [(usize, usize); 4] = [(2, 5), (5, 3), (3, 4), (4, 2)];
const NO_FLUID_COST: f32 = 8.0;

pub(crate) fn is_water_pathfindable(state: Option<StateId>) -> bool {
    let Some(state) = state else {
        return false;
    };
    match state.kind() {
        Some(kind) if class_of(kind) == BlockClass::Liquid => kind != BlockKind::Lava,
        _ => state.flags().contains(StateFlags::WATER),
    }
}

fn has_fluid(state: Option<StateId>) -> bool {
    state.is_some_and(|state| state.flags().has_fluid())
}

impl<V: BlockView> Walker<'_, V> {
    pub(crate) fn swim_type(&mut self, pos: BlockPos) -> PathType {
        let mut last = pos;
        for dx in 0..self.width {
            for dy in 0..self.height {
                for dz in 0..self.width {
                    last = pos.offset(dx, dy, dz);
                    let state = self.view.state(last);
                    let below = self.view.state(last.offset(0, -1, 0));
                    let air = state.is_none_or(StateId::is_air);
                    if !has_fluid(state) && is_water_pathfindable(below) && air {
                        return PathType::Breach;
                    }
                    if !state.is_some_and(|state| state.flags().contains(StateFlags::WATER)) {
                        return PathType::Blocked;
                    }
                }
            }
        }
        if is_water_pathfindable(self.view.state(last)) {
            PathType::Water
        } else {
            PathType::Blocked
        }
    }

    pub(crate) fn swim_start(&mut self) -> usize {
        let area = self.profile.bounding_box(self.mob.position);
        let pos = BlockPos::new(
            area.min_x.floor() as i32,
            (area.min_y + 0.5).floor() as i32,
            area.min_z.floor() as i32,
        );
        self.node(pos)
    }

    fn swim_accept(&mut self, pos: BlockPos) -> Option<usize> {
        let kind = self.mob_type(pos);
        let breaching = self.profile.allows_breaching() && kind == PathType::Breach;
        if !breaching && kind != PathType::Water {
            return None;
        }
        let malus = self.malus(kind);
        if malus < 0.0 {
            return None;
        }
        let empty = !has_fluid(self.view.state(pos));
        let node = self.node(pos);
        let entry = &mut self.nodes[node];
        entry.path_type = kind;
        entry.cost_malus = entry.cost_malus.max(malus);
        if empty {
            entry.cost_malus += NO_FLUID_COST;
        }
        Some(node)
    }

    fn swim_open(&self, node: Option<usize>) -> Option<usize> {
        node.filter(|node| !self.nodes[*node].closed)
    }

    fn swim_has_malus(&self, node: Option<usize>) -> bool {
        node.is_some_and(|node| self.nodes[node].cost_malus >= 0.0)
    }

    pub(crate) fn swim_neighbors(&mut self, current: usize, out: &mut Vec<usize>) {
        let pos = self.nodes[current].pos;
        let mut around: [Option<usize>; 6] = [None; 6];
        for (index, (dx, dy, dz)) in DIRECTIONS.iter().enumerate() {
            let node = self.swim_accept(pos.offset(*dx, *dy, *dz));
            around[index] = node;
            if let Some(node) = self.swim_open(node) {
                out.push(node);
            }
        }
        for (side, clockwise) in HORIZONTAL_CLOCKWISE {
            if !self.swim_has_malus(around[side]) || !self.swim_has_malus(around[clockwise]) {
                continue;
            }
            let (sx, _, sz) = DIRECTIONS[side];
            let (cx, _, cz) = DIRECTIONS[clockwise];
            let node = self.swim_accept(pos.offset(sx + cx, 0, sz + cz));
            if let Some(node) = self.swim_open(node) {
                out.push(node);
            }
        }
    }
}
