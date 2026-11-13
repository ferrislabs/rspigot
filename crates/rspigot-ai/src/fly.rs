use rspigot_domain::BlockPos;

use crate::PathType;
use crate::random::{MobRandom, RandomSource};
use crate::view::{BlockView, is_water};
use crate::walk::Walker;

const START_CANDIDATES: i32 = 10;
const SMALL_MOB_SIZE: f64 = 1.0;
const SMALL_MOB_INFLATE: f64 = 1.1;
const ORTHOGONAL: [(i32, i32, i32); 6] = [
    (0, 0, 1),
    (-1, 0, 0),
    (1, 0, 0),
    (0, 0, -1),
    (0, 1, 0),
    (0, -1, 0),
];
const EDGES: [((i32, i32, i32), [usize; 2]); 12] = [
    ((0, 1, 1), [0, 4]),
    ((-1, 1, 0), [1, 4]),
    ((1, 1, 0), [2, 4]),
    ((0, 1, -1), [3, 4]),
    ((0, -1, 1), [0, 5]),
    ((-1, -1, 0), [1, 5]),
    ((1, -1, 0), [2, 5]),
    ((0, -1, -1), [3, 5]),
    ((1, 0, -1), [3, 2]),
    ((1, 0, 1), [0, 2]),
    ((-1, 0, -1), [3, 1]),
    ((-1, 0, 1), [0, 1]),
];
const CORNERS: [((i32, i32, i32), [Side; 6]); 8] = [
    (
        (1, 1, -1),
        [
            Side::Edge(8),
            Side::Orthogonal(3),
            Side::Orthogonal(2),
            Side::Orthogonal(4),
            Side::Edge(3),
            Side::Edge(2),
        ],
    ),
    (
        (1, 1, 1),
        [
            Side::Edge(9),
            Side::Orthogonal(0),
            Side::Orthogonal(2),
            Side::Orthogonal(4),
            Side::Edge(0),
            Side::Edge(2),
        ],
    ),
    (
        (-1, 1, -1),
        [
            Side::Edge(10),
            Side::Orthogonal(3),
            Side::Orthogonal(1),
            Side::Orthogonal(4),
            Side::Edge(3),
            Side::Edge(1),
        ],
    ),
    (
        (-1, 1, 1),
        [
            Side::Edge(11),
            Side::Orthogonal(0),
            Side::Orthogonal(1),
            Side::Orthogonal(4),
            Side::Edge(0),
            Side::Edge(1),
        ],
    ),
    (
        (1, -1, -1),
        [
            Side::Edge(8),
            Side::Orthogonal(3),
            Side::Orthogonal(2),
            Side::Orthogonal(5),
            Side::Edge(7),
            Side::Edge(6),
        ],
    ),
    (
        (1, -1, 1),
        [
            Side::Edge(9),
            Side::Orthogonal(0),
            Side::Orthogonal(2),
            Side::Orthogonal(5),
            Side::Edge(4),
            Side::Edge(6),
        ],
    ),
    (
        (-1, -1, -1),
        [
            Side::Edge(10),
            Side::Orthogonal(3),
            Side::Orthogonal(1),
            Side::Orthogonal(5),
            Side::Edge(7),
            Side::Edge(5),
        ],
    ),
    (
        (-1, -1, 1),
        [
            Side::Edge(11),
            Side::Orthogonal(0),
            Side::Orthogonal(1),
            Side::Orthogonal(5),
            Side::Edge(4),
            Side::Edge(5),
        ],
    ),
];

#[derive(Debug, Clone, Copy)]
enum Side {
    Orthogonal(usize),
    Edge(usize),
}

impl<V: BlockView> Walker<'_, V> {
    pub(crate) fn fly_type(&mut self, pos: BlockPos) -> PathType {
        let mut kind = self.block_type(pos);
        if kind == PathType::Open && pos.y > self.min_y {
            let below_pos = pos.offset(0, -1, 0);
            kind = match self.block_type(below_pos) {
                PathType::Fire | PathType::Lava => PathType::Fire,
                PathType::Damaging => PathType::Damaging,
                PathType::Cocoa => PathType::Cocoa,
                PathType::Fence if below_pos == self.mob.block_pos() => PathType::Open,
                PathType::Fence => PathType::Fence,
                PathType::Walkable | PathType::Open | PathType::Water => PathType::Open,
                _ => PathType::Walkable,
            };
        }
        if matches!(kind, PathType::Walkable | PathType::Open) {
            kind = self.neighbour_type(pos, kind);
        }
        kind
    }

    pub(crate) fn fly_start(&mut self) -> usize {
        let position = self.mob.position;
        let column =
            |y: i32| BlockPos::new(position.x.floor() as i32, y, position.z.floor() as i32);
        let y = if self.profile.can_float && self.mob.in_floatable_fluid {
            let mut y = position.y.floor() as i32;
            while is_water(self.view.state(column(y))) {
                y += 1;
            }
            y
        } else {
            (position.y + 0.5).floor() as i32
        };
        let origin = column(y);
        if !self.fly_can_start_at(origin) {
            for candidate in self.fly_start_candidates() {
                if self.fly_can_start_at(candidate) {
                    return self.start_node(candidate);
                }
            }
        }
        self.start_node(origin)
    }

    fn fly_can_start_at(&mut self, pos: BlockPos) -> bool {
        let kind = self.mob_type(pos);
        self.malus(kind) >= 0.0
    }

    fn fly_start_candidates(&self) -> Vec<BlockPos> {
        let position = self.mob.position;
        let area = self.profile.bounding_box(position);
        let size_x = area.max_x - area.min_x;
        let size_y = area.max_y - area.min_y;
        let size_z = area.max_z - area.min_z;
        let block_y = position.y.floor() as i32;
        if (size_x + size_y + size_z) / 3.0 >= SMALL_MOB_SIZE {
            let corner =
                |x: f64, z: f64| BlockPos::new(x.floor() as i32, block_y, z.floor() as i32);
            return vec![
                corner(area.min_x, area.min_z),
                corner(area.min_x, area.max_z),
                corner(area.max_x, area.min_z),
                corner(area.max_x, area.max_z),
            ];
        }
        let grow = |size: f64| (SMALL_MOB_INFLATE - size).max(0.0);
        let (gx, gy, gz) = (grow(size_x), grow(size_y), grow(size_z));
        let low = BlockPos::new(
            (area.min_x - gx).floor() as i32,
            (area.min_y - gy).floor() as i32,
            (area.min_z - gz).floor() as i32,
        );
        let high = BlockPos::new(
            (area.max_x + gx).floor() as i32,
            (area.max_y + gy).floor() as i32,
            (area.max_z + gz).floor() as i32,
        );
        let mut random = MobRandom::new(
            i64::from(low.x) ^ i64::from(low.y).rotate_left(21) ^ i64::from(low.z).rotate_left(42),
        );
        (0..START_CANDIDATES)
            .map(|_| {
                BlockPos::new(
                    random.next_int(high.x - low.x + 1) + low.x,
                    random.next_int(high.y - low.y + 1) + low.y,
                    random.next_int(high.z - low.z + 1) + low.z,
                )
            })
            .collect()
    }

    fn fly_accept(&mut self, pos: BlockPos) -> Option<usize> {
        let kind = self.mob_type(pos);
        let malus = self.malus(kind);
        if malus < 0.0 {
            return None;
        }
        let node = self.node(pos);
        let entry = &mut self.nodes[node];
        entry.path_type = kind;
        entry.cost_malus = entry.cost_malus.max(malus);
        if kind == PathType::Walkable {
            entry.cost_malus += 1.0;
        }
        Some(node)
    }

    fn fly_open(&self, node: Option<usize>) -> Option<usize> {
        node.filter(|node| !self.nodes[*node].closed)
    }

    fn fly_has_malus(&self, node: Option<usize>) -> bool {
        node.is_some_and(|node| self.nodes[node].cost_malus >= 0.0)
    }

    pub(crate) fn fly_neighbors(&mut self, current: usize, out: &mut Vec<usize>) {
        let pos = self.nodes[current].pos;
        let orthogonal: Vec<Option<usize>> = ORTHOGONAL
            .iter()
            .map(|(dx, dy, dz)| self.fly_accept(pos.offset(*dx, *dy, *dz)))
            .collect();
        for node in &orthogonal {
            if let Some(node) = self.fly_open(*node) {
                out.push(node);
            }
        }
        let mut edges: Vec<Option<usize>> = Vec::with_capacity(EDGES.len());
        for ((dx, dy, dz), needs) in &EDGES {
            let node = self.fly_accept(pos.offset(*dx, *dy, *dz));
            edges.push(node);
            let supported = needs
                .iter()
                .all(|index| self.fly_has_malus(orthogonal[*index]));
            if let Some(node) = self.fly_open(node).filter(|_| supported) {
                out.push(node);
            }
        }
        for ((dx, dy, dz), needs) in &CORNERS {
            let node = self.fly_accept(pos.offset(*dx, *dy, *dz));
            let supported = needs.iter().all(|side| match side {
                Side::Orthogonal(index) => self.fly_has_malus(orthogonal[*index]),
                Side::Edge(index) => self.fly_has_malus(edges[*index]),
            });
            if let Some(node) = self.fly_open(node).filter(|_| supported) {
                out.push(node);
            }
        }
    }
}
