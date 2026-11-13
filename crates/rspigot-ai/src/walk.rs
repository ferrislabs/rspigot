use std::collections::HashMap;

use rspigot_blocks::Aabb;
use rspigot_domain::BlockPos;

use crate::classify::{is_pathfindable, path_type_of};
use crate::view::{BlockView, collides, floor_level, is_air, is_water};
use crate::{MobProfile, MobState, PathType};

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Node {
    pub(crate) pos: BlockPos,
    pub(crate) heap_index: Option<usize>,
    pub(crate) g: f32,
    pub(crate) h: f32,
    pub(crate) f: f32,
    pub(crate) came_from: Option<usize>,
    pub(crate) closed: bool,
    pub(crate) walked_distance: f32,
    pub(crate) cost_malus: f32,
    pub(crate) path_type: PathType,
}

impl Node {
    pub(crate) fn new(pos: BlockPos) -> Self {
        Self {
            pos,
            heap_index: None,
            g: 0.0,
            h: 0.0,
            f: 0.0,
            came_from: None,
            closed: false,
            walked_distance: 0.0,
            cost_malus: 0.0,
            path_type: PathType::Blocked,
        }
    }

    pub(crate) fn in_open_set(&self) -> bool {
        self.heap_index.is_some()
    }
}

pub(crate) fn distance(from: BlockPos, to: BlockPos) -> f32 {
    let dx = (to.x - from.x) as f32;
    let dy = (to.y - from.y) as f32;
    let dz = (to.z - from.z) as f32;
    (dx * dx + dy * dy + dz * dz).sqrt()
}

pub(crate) fn distance_xz(from: BlockPos, to: BlockPos) -> f32 {
    let dx = (to.x - from.x) as f32;
    let dz = (to.z - from.z) as f32;
    (dx * dx + dz * dz).sqrt()
}

const HORIZONTAL: [(i32, i32); 4] = [(0, -1), (1, 0), (0, 1), (-1, 0)];

pub(crate) struct Walker<'a, V> {
    pub(crate) view: &'a V,
    pub(crate) profile: &'a MobProfile,
    pub(crate) mob: MobState,
    pub(crate) width: i32,
    pub(crate) height: i32,
    pub(crate) min_y: i32,
    pub(crate) nodes: Vec<Node>,
    index: HashMap<BlockPos, usize>,
    block_types: HashMap<BlockPos, PathType>,
    mob_types: HashMap<BlockPos, PathType>,
}

impl<'a, V: BlockView> Walker<'a, V> {
    pub(crate) fn new(view: &'a V, profile: &'a MobProfile, mob: MobState) -> Self {
        let (width, height) = profile.cells();
        Self {
            view,
            profile,
            mob,
            width,
            height,
            min_y: view.min_y(),
            nodes: Vec::new(),
            index: HashMap::new(),
            block_types: HashMap::new(),
            mob_types: HashMap::new(),
        }
    }

    pub(crate) fn malus(&self, kind: PathType) -> f32 {
        self.profile.malus(kind)
    }

    pub(crate) fn node(&mut self, pos: BlockPos) -> usize {
        let nodes = &mut self.nodes;
        *self.index.entry(pos).or_insert_with(|| {
            nodes.push(Node::new(pos));
            nodes.len() - 1
        })
    }

    pub(crate) fn block_type(&mut self, pos: BlockPos) -> PathType {
        let view = self.view;
        *self
            .block_types
            .entry(pos)
            .or_insert_with(|| path_type_of(view.state(pos)))
    }

    pub(crate) fn static_type(&mut self, pos: BlockPos) -> PathType {
        if self.profile.flying {
            return self.fly_type(pos);
        }
        let kind = self.block_type(pos);
        if kind != PathType::Open || pos.y < self.min_y + 1 {
            return kind;
        }
        match self.block_type(pos.offset(0, -1, 0)) {
            PathType::Open | PathType::Water | PathType::Lava | PathType::Walkable => {
                PathType::Open
            }
            PathType::Fire => PathType::Fire,
            PathType::Damaging => PathType::Damaging,
            PathType::StickyHoney => PathType::StickyHoney,
            PathType::PowderSnow => PathType::OnTopOfPowderSnow,
            PathType::DamageCautious => PathType::DamageCautious,
            PathType::Trapdoor => PathType::OnTopOfTrapdoor,
            _ => self.neighbour_type(pos, PathType::Walkable),
        }
    }

    pub(crate) fn neighbour_type(&mut self, pos: BlockPos, fallback: PathType) -> PathType {
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    if dx == 0 && dz == 0 {
                        continue;
                    }
                    match self.block_type(pos.offset(dx, dy, dz)) {
                        PathType::Damaging => return PathType::DamagingInNeighbor,
                        PathType::Fire | PathType::Lava => return PathType::FireInNeighbor,
                        PathType::Water => return PathType::WaterBorder,
                        PathType::DamageCautious => return PathType::DamageCautious,
                        _ => {}
                    }
                }
            }
        }
        fallback
    }

    fn types_within(&mut self, pos: BlockPos) -> u32 {
        let mob = self.mob.block_pos();
        let can_pass = self.profile.can_pass_doors;
        let mut set = 0;
        for dx in 0..self.width {
            for dy in 0..self.height {
                for dz in 0..self.width {
                    let mut kind = self.static_type(pos.offset(dx, dy, dz));
                    if kind == PathType::DoorWoodClosed && self.profile.can_open_doors && can_pass {
                        kind = PathType::WalkableDoor;
                    }
                    if kind == PathType::DoorOpen && !can_pass {
                        kind = PathType::Blocked;
                    }
                    if kind == PathType::Rail
                        && self.static_type(mob) != PathType::Rail
                        && self.static_type(mob.offset(0, -1, 0)) != PathType::Rail
                    {
                        kind = PathType::UnpassableRail;
                    }
                    set |= kind.bit();
                }
            }
        }
        set
    }

    pub(crate) fn mob_type(&mut self, pos: BlockPos) -> PathType {
        if let Some(kind) = self.mob_types.get(&pos) {
            return *kind;
        }
        let kind = match self.profile.creaking_home {
            Some(home) if crate::creaking::blocks_path(home, pos, self.mob.position.block()) => {
                PathType::Blocked
            }
            _ => self.compute_mob_type(pos),
        };
        self.mob_types.insert(pos, kind);
        kind
    }

    fn compute_mob_type(&mut self, pos: BlockPos) -> PathType {
        let set = self.types_within(pos);
        let mut members = PathType::ALL
            .into_iter()
            .filter(|kind| set & kind.bit() != 0);
        if set.count_ones() == 1 {
            return members.next().unwrap_or(PathType::Blocked);
        }
        if set & PathType::Fence.bit() != 0 {
            return PathType::Fence;
        }
        if set & PathType::UnpassableRail.bit() != 0 {
            return PathType::UnpassableRail;
        }
        let mut best = PathType::Blocked;
        let mut best_malus = self.malus(best);
        for kind in members {
            let malus = self.malus(kind);
            if malus < 0.0 {
                return kind;
            }
            if malus >= best_malus {
                best_malus = malus;
                best = kind;
            }
        }
        let here = self.static_type(pos);
        if self.width > 1 {
            let close = self.malus(here) < best_malus;
            if close && self.malus(PathType::BigMobsCloseToDanger) < best_malus {
                return PathType::BigMobsCloseToDanger;
            }
            return best;
        }
        if here == PathType::Open && best != PathType::Open && best_malus == 0.0 {
            return PathType::Open;
        }
        best
    }

    fn floor(&self, pos: BlockPos) -> f64 {
        if self.profile.can_float && is_water(self.view.state(pos)) {
            return f64::from(pos.y) + 0.5;
        }
        floor_level(self.view, pos)
    }

    pub(crate) fn start(&mut self) -> usize {
        if self.profile.flying {
            return self.fly_start();
        }
        let position = self.mob.position;
        let column =
            |y: i32| BlockPos::new(position.x.floor() as i32, y, position.z.floor() as i32);
        let mut y = position.y.floor() as i32;
        if self.profile.can_float && self.mob.in_floatable_fluid {
            while is_water(self.view.state(column(y))) {
                y += 1;
            }
            y -= 1;
        } else if self.mob.on_ground {
            y = (position.y + 0.5).floor() as i32;
        } else {
            let mut probe = (position.y + 1.0).floor() as i32;
            while probe > self.min_y {
                y = probe;
                probe -= 1;
                let state = self.view.state(column(probe));
                if !is_air(state) && !state.is_some_and(is_pathfindable) {
                    break;
                }
            }
        }
        let block = self.mob.block_pos();
        let origin = BlockPos::new(block.x, y, block.z);
        if !self.can_start_at(origin) {
            let area = self.profile.bounding_box(position);
            let corners = [
                (area.min_x, area.min_z),
                (area.min_x, area.max_z),
                (area.max_x, area.min_z),
                (area.max_x, area.max_z),
            ];
            for (x, z) in corners {
                let corner = BlockPos::new(x.floor() as i32, y, z.floor() as i32);
                if self.can_start_at(corner) {
                    return self.start_node(corner);
                }
            }
        }
        self.start_node(origin)
    }

    fn can_start_at(&mut self, pos: BlockPos) -> bool {
        let kind = self.mob_type(pos);
        kind != PathType::Open && self.malus(kind) >= 0.0
    }

    pub(crate) fn start_node(&mut self, pos: BlockPos) -> usize {
        let node = self.node(pos);
        let kind = self.mob_type(pos);
        self.nodes[node].path_type = kind;
        self.nodes[node].cost_malus = self.malus(kind);
        node
    }

    pub(crate) fn neighbors(&mut self, current: usize, out: &mut Vec<usize>) {
        out.clear();
        if self.profile.flying {
            self.fly_neighbors(current, out);
            return;
        }
        let pos = self.nodes[current].pos;
        let above = self.mob_type(pos.offset(0, 1, 0));
        let here = self.mob_type(pos);
        let max_up = if self.malus(above) >= 0.0 && here != PathType::StickyHoney {
            1.0_f32.max(self.profile.step_height).floor() as i32
        } else {
            0
        };
        let floor = self.floor(pos);
        let mut sides = [None; 4];
        for (side, (dx, dz)) in HORIZONTAL.into_iter().enumerate() {
            let node = self.accept(pos.offset(dx, 0, dz), max_up, floor, (dx, dz), here);
            sides[side] = node;
            if let Some(node) = node.filter(|node| self.neighbor_valid(*node, current)) {
                out.push(node);
            }
        }
        for side in 0..HORIZONTAL.len() {
            let clockwise = (side + 1) % HORIZONTAL.len();
            if !self.diagonal_valid(current, sides[side], sides[clockwise]) {
                continue;
            }
            let (dx, dz) = HORIZONTAL[side];
            let (cx, cz) = HORIZONTAL[clockwise];
            let node = self.accept(
                pos.offset(dx + cx, 0, dz + cz),
                max_up,
                floor,
                (dx, dz),
                here,
            );
            if let Some(node) = node.filter(|node| self.diagonal_node_valid(*node)) {
                out.push(node);
            }
        }
    }

    fn neighbor_valid(&self, node: usize, current: usize) -> bool {
        let node = &self.nodes[node];
        !node.closed && (node.cost_malus >= 0.0 || self.nodes[current].cost_malus < 0.0)
    }

    fn diagonal_valid(&self, current: usize, first: Option<usize>, second: Option<usize>) -> bool {
        let (Some(first), Some(second)) = (first, second) else {
            return false;
        };
        let current = &self.nodes[current];
        let first = &self.nodes[first];
        let second = &self.nodes[second];
        if second.pos.y > current.pos.y || first.pos.y > current.pos.y {
            return false;
        }
        if first.path_type == PathType::WalkableDoor || second.path_type == PathType::WalkableDoor {
            return false;
        }
        if self.profile.width > 1.0 && (first.cost_malus > 0.0 || second.cost_malus > 0.0) {
            return false;
        }
        let around = second.path_type == PathType::Fence
            && first.path_type == PathType::Fence
            && self.profile.width < 0.5;
        let passable = |side: &Node| side.pos.y < current.pos.y || side.cost_malus >= 0.0 || around;
        passable(second) && passable(first)
    }

    fn diagonal_node_valid(&self, node: usize) -> bool {
        let node = &self.nodes[node];
        !node.closed && node.path_type != PathType::WalkableDoor && node.cost_malus >= 0.0
    }

    fn accept(
        &mut self,
        pos: BlockPos,
        max_up: i32,
        floor: f64,
        direction: (i32, i32),
        current: PathType,
    ) -> Option<usize> {
        if self.floor(pos) - floor > self.profile.jump_height() {
            return None;
        }
        let kind = self.mob_type(pos);
        let malus = self.malus(kind);
        let mut node = (malus >= 0.0).then(|| self.raise_cost(pos, kind, malus));
        if current.has_partial_collision()
            && node.is_some_and(|node| {
                self.nodes[node].cost_malus >= 0.0 && !self.reach_without_collision(node)
            })
        {
            node = None;
        }
        if kind == PathType::Walkable {
            return node;
        }
        let stuck = node.is_none_or(|node| self.nodes[node].cost_malus < 0.0);
        if stuck
            && max_up > 0
            && (kind != PathType::Fence || self.profile.can_walk_over_fences)
            && !matches!(
                kind,
                PathType::UnpassableRail | PathType::Trapdoor | PathType::PowderSnow
            )
        {
            self.jump_on(pos, max_up, floor, direction, current)
        } else if kind == PathType::Water && !self.profile.can_float {
            self.first_non_water_below(pos, node)
        } else if kind == PathType::Open {
            Some(self.first_ground_below(pos))
        } else if kind.has_partial_collision() && node.is_none() {
            Some(self.closed_node(pos, kind))
        } else {
            node
        }
    }

    pub(crate) fn raise_cost(&mut self, pos: BlockPos, kind: PathType, malus: f32) -> usize {
        let index = self.node(pos);
        let node = &mut self.nodes[index];
        node.path_type = kind;
        node.cost_malus = node.cost_malus.max(malus);
        index
    }

    fn blocked_node(&mut self, pos: BlockPos) -> usize {
        let index = self.node(pos);
        let node = &mut self.nodes[index];
        node.path_type = PathType::Blocked;
        node.cost_malus = -1.0;
        index
    }

    fn closed_node(&mut self, pos: BlockPos, kind: PathType) -> usize {
        let index = self.node(pos);
        let node = &mut self.nodes[index];
        node.closed = true;
        node.path_type = kind;
        node.cost_malus = kind.malus();
        index
    }

    fn jump_on(
        &mut self,
        pos: BlockPos,
        max_up: i32,
        floor: f64,
        direction: (i32, i32),
        current: PathType,
    ) -> Option<usize> {
        let node = self.accept(pos.offset(0, 1, 0), max_up - 1, floor, direction, current)?;
        if self.profile.width >= 1.0 {
            return Some(node);
        }
        let kind = self.nodes[node].path_type;
        if kind != PathType::Open && kind != PathType::Walkable {
            return Some(node);
        }
        let x = f64::from(pos.x - direction.0) + 0.5;
        let z = f64::from(pos.z - direction.1) + 0.5;
        let half = f64::from(self.profile.width) / 2.0;
        let origin = BlockPos::new(x.floor() as i32, pos.y + 1, z.floor() as i32);
        let bottom = self.floor(origin) + 0.001;
        let top = f64::from(self.profile.height) + self.floor(self.nodes[node].pos) - 0.002;
        let area = Aabb::new(x - half, bottom, z - half, x + half, top, z + half);
        (!collides(self.view, &area)).then_some(node)
    }

    fn first_non_water_below(&mut self, pos: BlockPos, mut node: Option<usize>) -> Option<usize> {
        let mut y = pos.y - 1;
        while y > self.min_y {
            let below = BlockPos::new(pos.x, y, pos.z);
            let kind = self.mob_type(below);
            if kind != PathType::Water {
                return node;
            }
            node = Some(self.raise_cost(below, kind, self.malus(kind)));
            y -= 1;
        }
        node
    }

    fn first_ground_below(&mut self, pos: BlockPos) -> usize {
        let mut y = pos.y - 1;
        while y >= self.min_y {
            let below = BlockPos::new(pos.x, y, pos.z);
            if pos.y - y > self.profile.max_fall_distance {
                return self.blocked_node(below);
            }
            let kind = self.mob_type(below);
            let malus = self.malus(kind);
            if kind != PathType::Open {
                return if malus >= 0.0 {
                    self.raise_cost(below, kind, malus)
                } else {
                    self.blocked_node(below)
                };
            }
            y -= 1;
        }
        self.blocked_node(pos)
    }

    fn reach_without_collision(&self, node: usize) -> bool {
        let position = self.mob.position;
        let mut area = self.profile.bounding_box(position);
        let target = self.nodes[node].pos;
        let size_x = area.max_x - area.min_x;
        let size_y = area.max_y - area.min_y;
        let size_z = area.max_z - area.min_z;
        let step_x = f64::from(target.x) - position.x + size_x / 2.0;
        let step_y = f64::from(target.y) - position.y + size_y / 2.0;
        let step_z = f64::from(target.z) - position.z + size_z / 2.0;
        let length = (step_x * step_x + step_y * step_y + step_z * step_z).sqrt();
        let steps = (length / ((size_x + size_y + size_z) / 3.0)).ceil() as i32;
        let scale = f64::from(1.0_f32 / steps as f32);
        let (step_x, step_y, step_z) = (step_x * scale, step_y * scale, step_z * scale);
        for _ in 0..steps {
            area = Aabb::new(
                area.min_x + step_x,
                area.min_y + step_y,
                area.min_z + step_z,
                area.max_x + step_x,
                area.max_y + step_y,
                area.max_z + step_z,
            );
            if collides(self.view, &area) {
                return false;
            }
        }
        true
    }
}

pub fn block_path_type(view: &impl BlockView, pos: BlockPos) -> PathType {
    let profile = MobProfile::new(0.0, 0.0);
    let mob = MobState {
        position: pos.center(),
        on_ground: true,
        in_floatable_fluid: false,
    };
    Walker::new(view, &profile, mob).static_type(pos)
}

pub fn mob_path_type(
    view: &impl BlockView,
    profile: &MobProfile,
    mob: MobState,
    pos: BlockPos,
) -> PathType {
    Walker::new(view, profile, mob).mob_type(pos)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rspigot_blocks::{BlockKind, StateId};
    use rspigot_domain::Vec3;

    struct World(HashMap<BlockPos, StateId>);

    impl BlockView for World {
        fn state(&self, pos: BlockPos) -> Option<StateId> {
            Some(self.0.get(&pos).copied().unwrap_or(StateId::AIR))
        }

        fn min_y(&self) -> i32 {
            0
        }
    }

    impl World {
        fn floor() -> Self {
            let mut blocks = HashMap::new();
            for x in -6..=6 {
                for z in -6..=6 {
                    blocks.insert(BlockPos::new(x, 63, z), BlockKind::Stone.default_state());
                }
            }
            Self(blocks)
        }

        fn put(&mut self, x: i32, y: i32, z: i32, state: StateId) {
            self.0.insert(BlockPos::new(x, y, z), state);
        }
    }

    fn mob(x: f64, y: f64, z: f64, on_ground: bool) -> MobState {
        MobState {
            position: Vec3::new(x, y, z),
            on_ground,
            in_floatable_fluid: false,
        }
    }

    #[test]
    fn airborne_start_drops_to_ground() {
        let world = World::floor();
        let profile = MobProfile::new(0.6, 1.95);
        let mut walker = Walker::new(&world, &profile, mob(0.5, 70.3, 0.5, false));
        let start = walker.start();
        assert_eq!(walker.nodes[start].pos, BlockPos::new(0, 64, 0));
        assert_eq!(walker.nodes[start].path_type, PathType::Walkable);
    }

    #[test]
    fn floating_start_rises_to_surface() {
        let mut world = World::floor();
        for y in 64..=66 {
            world.put(0, y, 0, BlockKind::Water.default_state());
        }
        let mut profile = MobProfile::new(0.6, 1.95);
        profile.can_float = true;
        let mut state = mob(0.5, 64.2, 0.5, false);
        state.in_floatable_fluid = true;
        let mut walker = Walker::new(&world, &profile, state);
        let start = walker.start();
        assert_eq!(walker.nodes[start].pos, BlockPos::new(0, 66, 0));
        assert_eq!(walker.floor(BlockPos::new(0, 65, 0)), 65.5);
    }

    #[test]
    fn rails_are_unpassable_unless_standing_on_one() {
        let mut world = World::floor();
        world.put(2, 64, 0, BlockKind::Rail.default_state());
        let profile = MobProfile::new(0.6, 1.95);
        let mut walker = Walker::new(&world, &profile, mob(0.5, 64.0, 0.5, true));
        assert_eq!(
            walker.mob_type(BlockPos::new(2, 64, 0)),
            PathType::UnpassableRail
        );
        world.put(0, 64, 0, BlockKind::Rail.default_state());
        let mut walker = Walker::new(&world, &profile, mob(0.5, 64.0, 0.5, true));
        assert_eq!(walker.mob_type(BlockPos::new(2, 64, 0)), PathType::Rail);
    }

    #[test]
    fn big_mobs_near_danger_get_their_own_type() {
        let mut world = World::floor();
        world.put(2, 64, 0, BlockKind::Cactus.default_state());
        let spider = MobProfile::new(1.4, 0.9);
        let mut walker = Walker::new(&world, &spider, mob(0.5, 64.0, 0.5, true));
        assert_eq!(
            walker.mob_type(BlockPos::new(-1, 64, 0)),
            PathType::Walkable
        );
        assert_eq!(
            walker.mob_type(BlockPos::new(0, 64, 0)),
            PathType::BigMobsCloseToDanger
        );
        assert_eq!(walker.mob_type(BlockPos::new(1, 64, 0)), PathType::Damaging);
        let zombie = MobProfile::new(0.6, 1.95);
        let mut walker = Walker::new(&world, &zombie, mob(0.5, 64.0, 0.5, true));
        assert_eq!(
            walker.mob_type(BlockPos::new(1, 64, 0)),
            PathType::DamagingInNeighbor
        );
    }

    fn neighbours_of_start(world: &World) -> Vec<BlockPos> {
        let profile = MobProfile::new(0.6, 1.95);
        let mut walker = Walker::new(world, &profile, mob(0.5, 64.0, 0.5, true));
        let start = walker.start();
        let mut out = Vec::new();
        walker.neighbors(start, &mut out);
        out.iter().map(|node| walker.nodes[*node].pos).collect()
    }

    #[test]
    fn honey_prevents_stepping_up() {
        let mut world = World::floor();
        world.put(1, 64, 0, BlockKind::Stone.default_state());
        assert!(neighbours_of_start(&world).contains(&BlockPos::new(1, 65, 0)));
        world.put(0, 63, 0, BlockKind::HoneyBlock.default_state());
        let neighbours = neighbours_of_start(&world);
        assert!(!neighbours.contains(&BlockPos::new(1, 65, 0)));
        assert!(neighbours.contains(&BlockPos::new(-1, 64, 0)));
    }

    #[test]
    fn low_ceiling_blocks_the_jump() {
        let mut world = World::floor();
        world.put(1, 64, 0, BlockKind::Stone.default_state());
        world.put(0, 66, 0, BlockKind::Stone.default_state());
        assert!(!neighbours_of_start(&world).contains(&BlockPos::new(1, 65, 0)));
    }

    #[test]
    fn trapdoors_are_never_jumped() {
        let mut world = World::floor();
        let closed = BlockKind::OakTrapdoor.state_with(&[("open", "false"), ("half", "top")]);
        world.put(1, 64, 0, closed.unwrap_or_default());
        let zombie = MobProfile::new(0.6, 1.95);
        let mut walker = Walker::new(&world, &zombie, mob(0.5, 64.0, 0.5, true));
        assert_eq!(
            walker.static_type(BlockPos::new(1, 64, 0)),
            PathType::Trapdoor
        );
        assert_eq!(
            walker.mob_type(BlockPos::new(1, 64, 0)),
            PathType::OnTopOfTrapdoor
        );
        let wary = MobProfile::new(0.6, 0.9).with_malus(PathType::Trapdoor, -1.0);
        let mut walker = Walker::new(&world, &wary, mob(0.5, 64.0, 0.5, true));
        let start = walker.start();
        let mut out = Vec::new();
        walker.neighbors(start, &mut out);
        let reached: Vec<_> = out.iter().map(|node| walker.nodes[*node].pos).collect();
        assert!(!reached.iter().any(|pos| pos.x == 1 && pos.z == 0));
        assert!(reached.contains(&BlockPos::new(-1, 64, 0)));
    }
}
