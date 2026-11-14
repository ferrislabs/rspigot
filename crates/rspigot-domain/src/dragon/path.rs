use std::f32::consts::{FRAC_PI_4, FRAC_PI_8, PI};

use crate::BlockPos;
use crate::mth;

pub const NODE_COUNT: usize = 24;
pub const OUTER_RING: usize = 12;
const MIN_NODE_HEIGHT: i32 = 73;
const CLOSEST_START: f32 = 10_000.0;
const ADJACENCY: [i32; NODE_COUNT] = [
    0x1802, 0x2005, 0x200a, 0x4014, 0x8028, 0x8050, 0x100a0, 0x20140, 0x20280, 0x40500, 0x80a00,
    0x80401, 0x182001, 0x305006, 0x20a008, 0x614030, 0x428040, 0xc50180, 0x8a0200, 0x941c00,
    0xe83000, 0xd0e000, 0xb38000, 0x7e0000,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DragonNode {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl DragonNode {
    pub const fn new(x: i32, y: i32, z: i32) -> Self {
        Self { x, y, z }
    }

    pub fn block(self) -> BlockPos {
        BlockPos::new(self.x, self.y, self.z)
    }

    pub fn distance_to(self, other: Self) -> f32 {
        self.distance_to_sqr(other).sqrt()
    }

    pub fn distance_to_sqr(self, other: Self) -> f32 {
        let dx = (other.x - self.x) as f32;
        let dy = (other.y - self.y) as f32;
        let dz = (other.z - self.z) as f32;
        dx * dx + dy * dy + dz * dz
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct Slot {
    node: DragonNode,
    closed: bool,
    f: f32,
    g: f32,
    h: f32,
    came_from: Option<usize>,
    heap_index: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct DragonPath {
    nodes: Vec<DragonNode>,
    next: usize,
}

impl DragonPath {
    pub fn new(nodes: Vec<DragonNode>) -> Self {
        Self { nodes, next: 0 }
    }

    pub fn advance(&mut self) {
        self.next += 1;
    }

    pub fn is_done(&self) -> bool {
        self.next >= self.nodes.len()
    }

    pub fn next_node_pos(&self) -> Option<BlockPos> {
        self.nodes.get(self.next).map(|node| node.block())
    }

    pub fn nodes(&self) -> &[DragonNode] {
        &self.nodes
    }

    pub fn next_index(&self) -> usize {
        self.next
    }
}

#[derive(Debug, Default)]
struct Heap {
    items: Vec<usize>,
}

impl Heap {
    fn clear(&mut self) {
        self.items.clear();
    }

    fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    fn insert(&mut self, slots: &mut [Slot], node: usize) {
        let index = self.items.len();
        self.items.push(node);
        slots[node].heap_index = Some(index);
        self.up(slots, index);
    }

    fn pop(&mut self, slots: &mut [Slot]) -> Option<usize> {
        let last = self.items.pop()?;
        let top = match self.items.first_mut() {
            Some(first) => std::mem::replace(first, last),
            None => last,
        };
        if !self.items.is_empty() {
            self.down(slots, 0);
        }
        slots[top].heap_index = None;
        Some(top)
    }

    fn change_cost(&mut self, slots: &mut [Slot], node: usize, cost: f32) {
        let old = slots[node].f;
        slots[node].f = cost;
        let Some(index) = slots[node].heap_index else {
            return;
        };
        if cost < old {
            self.up(slots, index);
        } else {
            self.down(slots, index);
        }
    }

    fn place(&mut self, slots: &mut [Slot], index: usize, node: usize) {
        self.items[index] = node;
        slots[node].heap_index = Some(index);
    }

    fn up(&mut self, slots: &mut [Slot], mut index: usize) {
        let node = self.items[index];
        let cost = slots[node].f;
        while index > 0 {
            let parent = (index - 1) >> 1;
            let above = self.items[parent];
            if cost >= slots[above].f {
                break;
            }
            self.place(slots, index, above);
            index = parent;
        }
        self.place(slots, index, node);
    }

    fn down(&mut self, slots: &mut [Slot], mut index: usize) {
        let node = self.items[index];
        let cost = slots[node].f;
        loop {
            let left = 1 + (index << 1);
            let right = left + 1;
            let Some(&left_node) = self.items.get(left) else {
                break;
            };
            let left_cost = slots[left_node].f;
            let (right_node, right_cost) = match self.items.get(right) {
                Some(&right_node) => (Some(right_node), slots[right_node].f),
                None => (None, f32::INFINITY),
            };
            if left_cost < right_cost {
                if left_cost >= cost {
                    break;
                }
                self.place(slots, index, left_node);
                index = left;
            } else {
                let Some(right_node) = right_node else {
                    break;
                };
                if right_cost >= cost {
                    break;
                }
                self.place(slots, index, right_node);
                index = right;
            }
        }
        self.place(slots, index, node);
    }
}

#[derive(Debug, Default)]
pub struct DragonNodes {
    slots: Vec<Slot>,
    open: Heap,
}

impl DragonNodes {
    pub fn is_built(&self) -> bool {
        !self.slots.is_empty()
    }

    pub fn node(&self, index: usize) -> Option<DragonNode> {
        self.slots.get(index).map(|slot| slot.node)
    }

    pub fn build(&mut self, mut ground: impl FnMut(i32, i32) -> i32) {
        self.slots = (0..NODE_COUNT)
            .map(|index| {
                let (x, z, lift) = node_xz(index);
                let height = MIN_NODE_HEIGHT.max(ground(x, z) + lift);
                Slot {
                    node: DragonNode::new(x, height, z),
                    ..Slot::default()
                }
            })
            .collect();
    }

    pub fn find_closest_node(
        &mut self,
        position: (f64, f64, f64),
        crystals: Option<i32>,
        ground: impl FnMut(i32, i32) -> i32,
    ) -> usize {
        if !self.is_built() {
            self.build(ground);
        }
        self.closest_to(position, crystals)
    }

    pub fn closest_to(&self, position: (f64, f64, f64), crystals: Option<i32>) -> usize {
        let probe = DragonNode::new(
            mth::floor(position.0),
            mth::floor(position.1),
            mth::floor(position.2),
        );
        let mut best = CLOSEST_START;
        let mut closest = 0;
        for index in first_neighbour(crystals)..NODE_COUNT {
            let distance = self.slots[index].node.distance_to_sqr(probe);
            if distance < best {
                best = distance;
                closest = index;
            }
        }
        closest
    }

    pub fn find_path(
        &mut self,
        start: usize,
        end: usize,
        final_node: Option<DragonNode>,
        crystals: Option<i32>,
    ) -> Option<DragonPath> {
        if !self.is_built() || start >= NODE_COUNT || end >= NODE_COUNT {
            return None;
        }
        self.slots.truncate(NODE_COUNT);
        for slot in &mut self.slots {
            slot.closed = false;
            slot.f = 0.0;
            slot.g = 0.0;
            slot.h = 0.0;
            slot.came_from = None;
            slot.heap_index = None;
        }
        let final_slot = final_node.map(|node| {
            self.slots.push(Slot {
                node,
                ..Slot::default()
            });
            self.slots.len() - 1
        });
        let target = self.slots[end].node;
        let origin = self.slots[start].node;
        self.slots[start].g = 0.0;
        self.slots[start].h = origin.distance_to(target);
        self.slots[start].f = self.slots[start].h;
        self.open.clear();
        self.open.insert(&mut self.slots, start);
        let mut closest = start;
        let first = first_neighbour(crystals);
        loop {
            if self.open.is_empty() {
                if closest == start {
                    return None;
                }
                return Some(self.reconstruct(closest, final_slot));
            }
            let current = self.open.pop(&mut self.slots)?;
            let current_node = self.slots[current].node;
            if current_node == target {
                return Some(self.reconstruct(end, final_slot));
            }
            let distance = current_node.distance_to(target);
            if distance < self.slots[closest].node.distance_to(target) {
                closest = current;
            }
            self.slots[current].closed = true;
            for neighbour in first..NODE_COUNT {
                if ADJACENCY[current] & (1 << neighbour) <= 0 || self.slots[neighbour].closed {
                    continue;
                }
                let cost =
                    self.slots[current].g + current_node.distance_to(self.slots[neighbour].node);
                let open = self.slots[neighbour].heap_index.is_some();
                if open && cost >= self.slots[neighbour].g {
                    continue;
                }
                self.slots[neighbour].came_from = Some(current);
                self.slots[neighbour].g = cost;
                self.slots[neighbour].h = self.slots[neighbour].node.distance_to(target);
                let total = self.slots[neighbour].g + self.slots[neighbour].h;
                if open {
                    self.open.change_cost(&mut self.slots, neighbour, total);
                } else {
                    self.slots[neighbour].f = total;
                    self.open.insert(&mut self.slots, neighbour);
                }
            }
        }
    }

    fn reconstruct(&mut self, end: usize, final_slot: Option<usize>) -> DragonPath {
        let mut last = end;
        if let Some(final_slot) = final_slot {
            self.slots[final_slot].came_from = Some(end);
            last = final_slot;
        }
        let mut nodes = vec![self.slots[last].node];
        let mut at = last;
        while let Some(previous) = self.slots[at].came_from {
            nodes.insert(0, self.slots[previous].node);
            at = previous;
        }
        DragonPath::new(nodes)
    }
}

pub fn first_neighbour(crystals: Option<i32>) -> usize {
    if crystals.is_none_or(|alive| alive == 0) {
        OUTER_RING
    } else {
        0
    }
}

fn node_xz(index: usize) -> (i32, i32, i32) {
    let (step, count, radius, lift) = if index < OUTER_RING {
        (0.261_799_4_f32, index, 60.0_f32, 5)
    } else if index < 20 {
        (FRAC_PI_8, index - OUTER_RING, 40.0_f32, 15)
    } else {
        (FRAC_PI_4, index - 20, 20.0_f32, 5)
    };
    let angle = (count as f32 * step + -PI) * 2.0_f32;
    let x = mth::floor(f64::from(mth::cos(f64::from(angle)) * radius));
    let z = mth::floor(f64::from(mth::sin(f64::from(angle)) * radius));
    (x, z, lift)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn built() -> DragonNodes {
        let mut nodes = DragonNodes::default();
        nodes.build(|_, _| 64);
        nodes
    }

    #[test]
    fn the_outer_ring_has_twelve_nodes_sixty_blocks_out_five_above_the_ground() {
        let nodes = built();
        let first = nodes.node(0).unwrap();
        assert_eq!((first.x, first.z), (60, 0));
        assert_eq!(first.y, 73);
        for index in 0..12 {
            let node = nodes.node(index).unwrap();
            let radius = f64::from(node.x).hypot(f64::from(node.z));
            assert!((radius - 60.0).abs() < 1.5, "{index} {radius}");
        }
    }

    #[test]
    fn the_middle_ring_sits_fifteen_above_the_ground_and_the_inner_ring_five() {
        let nodes = built();
        assert_eq!(nodes.node(12).unwrap().y, 79);
        assert_eq!(nodes.node(20).unwrap().y, 73);
        let middle = nodes.node(12).unwrap();
        assert_eq!((middle.x, middle.z), (40, 0));
        let inner = nodes.node(20).unwrap();
        assert_eq!((inner.x, inner.z), (20, 0));
    }

    #[test]
    fn nodes_never_sit_below_seventy_three() {
        let mut nodes = DragonNodes::default();
        nodes.build(|_, _| 0);
        assert!((0..NODE_COUNT).all(|index| nodes.node(index).unwrap().y == 73));
    }

    #[test]
    fn the_closest_node_ignores_the_outer_ring_when_no_crystal_is_left() {
        let nodes = built();
        assert_eq!(nodes.closest_to((60.0, 73.0, 0.0), Some(3)), 0);
        assert_eq!(nodes.closest_to((60.0, 73.0, 0.0), Some(0)), 12);
        assert_eq!(nodes.closest_to((60.0, 73.0, 0.0), None), 12);
    }

    #[test]
    fn the_closest_node_defaults_to_the_first_one_when_all_are_far() {
        let nodes = built();
        assert_eq!(nodes.closest_to((5000.0, 70.0, 5000.0), Some(1)), 0);
        assert_eq!(nodes.closest_to((5000.0, 70.0, 5000.0), None), 0);
    }

    #[test]
    fn a_path_between_neighbouring_outer_nodes_is_direct() {
        let mut nodes = built();
        let path = nodes.find_path(0, 1, None, Some(1)).unwrap();
        assert_eq!(path.nodes().len(), 2);
        assert_eq!(path.nodes()[0], nodes.node(0).unwrap());
        assert_eq!(path.nodes()[1], nodes.node(1).unwrap());
    }

    #[test]
    fn a_path_to_a_far_outer_node_goes_around_the_ring() {
        let mut nodes = built();
        let path = nodes.find_path(0, 6, None, Some(1)).unwrap();
        assert!(path.nodes().len() > 2);
        assert_eq!(path.nodes().first().copied(), nodes.node(0));
        assert_eq!(path.nodes().last().copied(), nodes.node(6));
    }

    #[test]
    fn a_path_to_the_start_node_has_a_single_node() {
        let mut nodes = built();
        let path = nodes.find_path(3, 3, None, Some(1)).unwrap();
        assert_eq!(path.nodes().len(), 1);
    }

    #[test]
    fn a_final_node_is_appended_to_the_path() {
        let mut nodes = built();
        let extra = DragonNode::new(500, 90, 500);
        let path = nodes.find_path(0, 1, Some(extra), Some(1)).unwrap();
        assert_eq!(path.nodes().last().copied(), Some(extra));
        assert_eq!(path.nodes().len(), 3);
    }

    #[test]
    fn the_path_cursor_advances_until_done() {
        let mut nodes = built();
        let mut path = nodes.find_path(0, 1, None, Some(1)).unwrap();
        assert!(!path.is_done());
        assert_eq!(path.next_node_pos(), Some(nodes.node(0).unwrap().block()));
        path.advance();
        path.advance();
        assert!(path.is_done());
        assert_eq!(path.next_node_pos(), None);
    }

    #[test]
    fn paths_between_unbuilt_nodes_do_not_exist() {
        let mut nodes = DragonNodes::default();
        assert!(nodes.find_path(0, 1, None, None).is_none());
    }

    #[test]
    fn every_adjacency_is_symmetric() {
        for (a, row) in ADJACENCY.iter().enumerate() {
            for (b, other) in ADJACENCY.iter().enumerate() {
                let forward = row & (1 << b) > 0;
                let backward = other & (1 << a) > 0;
                assert_eq!(forward, backward, "{a} {b}");
            }
        }
    }

    #[test]
    fn without_crystals_only_the_middle_and_inner_rings_are_walked() {
        let mut nodes = built();
        let path = nodes.find_path(12, 17, None, Some(0)).unwrap();
        assert!(
            path.nodes()
                .iter()
                .all(|node| { (12..NODE_COUNT).any(|index| nodes.node(index) == Some(*node)) })
        );
        assert_eq!(path.nodes().first().copied(), nodes.node(12));
    }

    #[test]
    fn an_unreachable_goal_leads_to_the_closest_node_found() {
        let mut nodes = built();
        let path = nodes.find_path(0, 6, None, Some(0)).unwrap();
        assert_ne!(path.nodes().last().copied(), nodes.node(6));
        assert!(path.nodes().len() > 1);
    }

    #[test]
    fn a_goal_next_to_nothing_reachable_has_no_path() {
        let mut nodes = built();
        assert!(nodes.find_path(12, 6, None, Some(0)).is_some());
        assert!(nodes.find_path(3, 3, None, Some(0)).is_some());
        assert!(nodes.find_path(3, 25, None, Some(0)).is_none());
    }
}
