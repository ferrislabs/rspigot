use rspigot_blocks::{BlockKind, StateFlags, StateId};
use rspigot_domain::{BlockPos, ChunkPos, Vec3};

use crate::classify::is_cauldron;
use crate::view::{floor_level, is_air, is_water};
use crate::{BlockView, MobProfile, MobState, Path, PathFinder, PathType};

const STUCK_CHECK_INTERVAL: i32 = 100;
const STUCK_THRESHOLD_DISTANCE_FACTOR: f32 = 0.25;
const MAX_VERTICAL_DISTANCE_TO_WAYPOINT: f64 = 1.0;
const SURFACE_SCAN: i32 = 16;
const MAX_TIME_RECOMPUTE: i64 = 20;
const REQUIRED_PATH_LENGTH: f32 = 16.0;
const BLOCK_REGION_OFFSET: f32 = 8.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MobTick {
    pub state: MobState,
    pub in_liquid: bool,
    pub passenger: bool,
    pub speed: f32,
    pub game_time: i64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MoveTarget {
    pub position: Vec3,
    pub speed_modifier: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Navigation {
    path: Option<Path>,
    speed_modifier: f64,
    tick: i32,
    last_stuck_check: i32,
    last_stuck_check_pos: Vec3,
    timeout_cached_node: BlockPos,
    timeout_timer: i64,
    last_timeout_check: i64,
    timeout_limit: f64,
    max_distance_to_waypoint: f32,
    stuck: bool,
    target_pos: Option<BlockPos>,
    reach_range: i32,
    has_delayed_recomputation: bool,
    time_last_recompute: i64,
    max_visited_nodes: i32,
    max_visited_nodes_multiplier: f32,
    required_path_length: f32,
    climber: bool,
    path_to_position: Option<BlockPos>,
    avoid_sun: bool,
    path_to_targets_below_surface: bool,
}

impl Default for Navigation {
    fn default() -> Self {
        Self::new()
    }
}

pub fn waypoint_distance(width: f32) -> f32 {
    if width > 0.75 {
        width / 2.0
    } else {
        0.75 - width / 2.0
    }
}

pub fn can_cut_corner(kind: PathType) -> bool {
    !matches!(
        kind,
        PathType::FireInNeighbor | PathType::DamagingInNeighbor | PathType::WalkableDoor
    )
}

impl Navigation {
    pub fn new() -> Self {
        Self {
            path: None,
            speed_modifier: 0.0,
            tick: 0,
            last_stuck_check: 0,
            last_stuck_check_pos: Vec3::ZERO,
            timeout_cached_node: BlockPos::new(0, 0, 0),
            timeout_timer: 0,
            last_timeout_check: 0,
            timeout_limit: 0.0,
            max_distance_to_waypoint: 0.5,
            stuck: false,
            target_pos: None,
            reach_range: 0,
            has_delayed_recomputation: false,
            time_last_recompute: 0,
            max_visited_nodes: (MobProfile::DEFAULT_FOLLOW_RANGE * 16.0).floor() as i32,
            max_visited_nodes_multiplier: 1.0,
            required_path_length: REQUIRED_PATH_LENGTH,
            climber: false,
            path_to_position: None,
            avoid_sun: false,
            path_to_targets_below_surface: false,
        }
    }

    pub fn set_path_to_targets_below_surface(&mut self, enabled: bool) {
        self.path_to_targets_below_surface = enabled;
    }

    pub fn path_to_targets_below_surface(&self) -> bool {
        self.path_to_targets_below_surface
    }

    pub fn set_climber(&mut self, climber: bool) {
        self.climber = climber;
    }

    pub fn is_climber(&self) -> bool {
        self.climber
    }

    pub fn path_to_position(&self) -> Option<BlockPos> {
        self.path_to_position
    }

    pub fn with_follow_range(base_follow_range: f64) -> Self {
        Self {
            max_visited_nodes: (base_follow_range * 16.0).floor() as i32,
            ..Self::new()
        }
    }

    pub fn set_required_path_length(&mut self, length: f32, follow_range: f64) {
        self.required_path_length = length;
        self.update_max_visited_nodes(follow_range);
    }

    pub fn max_path_length(&self, follow_range: f64) -> f32 {
        (follow_range as f32).max(self.required_path_length)
    }

    pub fn update_max_visited_nodes(&mut self, follow_range: f64) {
        self.max_visited_nodes = (self.max_path_length(follow_range) * 16.0).floor() as i32;
    }

    pub fn max_visited_nodes(&self) -> i32 {
        self.max_visited_nodes
    }

    pub fn target_pos(&self) -> Option<BlockPos> {
        self.target_pos
    }

    pub fn has_delayed_recomputation(&self) -> bool {
        self.has_delayed_recomputation
    }

    pub fn can_update_path(mob: &MobTick) -> bool {
        mob.state.on_ground || mob.in_liquid || mob.passenger
    }

    pub fn create_path(
        &mut self,
        view: &impl BlockView,
        profile: &MobProfile,
        mob: &MobTick,
        target: BlockPos,
        reach: i32,
        follow_range: f64,
    ) -> Option<Path> {
        let max_path_length = self.max_path_length(follow_range);
        self.create_path_limited(view, profile, mob, target, reach, max_path_length)
    }

    pub fn create_path_limited(
        &mut self,
        view: &impl BlockView,
        profile: &MobProfile,
        mob: &MobTick,
        target: BlockPos,
        reach: i32,
        max_path_length: f32,
    ) -> Option<Path> {
        if self.climber {
            self.path_to_position = Some(target);
        }
        if !view.chunk_loaded(target.chunk()) {
            return None;
        }
        let target = if self.path_to_targets_below_surface {
            target
        } else {
            find_surface_position(view, target)
        };
        if mob.state.position.y < f64::from(view.min_y()) || !Self::can_update_path(mob) {
            return None;
        }
        if let Some(path) = self.path.as_ref()
            && !path.is_done()
            && self.target_pos == Some(target)
        {
            return Some(path.clone());
        }
        let start = mob.state.block_pos();
        let radius = (max_path_length + BLOCK_REGION_OFFSET) as i32;
        let region = Region {
            view,
            min: start.offset(-radius, -radius, -radius).chunk(),
            max: start.offset(radius, radius, radius).chunk(),
        };
        let mut finder = PathFinder::new(profile.clone());
        finder.set_max_visited_nodes(self.max_visited_nodes);
        finder.set_max_path_length(max_path_length);
        finder.set_visited_multiplier(self.max_visited_nodes_multiplier);
        let path = finder.find_path(&region, &mob.state, &[target], reach);
        if let Some(path) = path.as_ref() {
            self.target_pos = Some(path.target());
            self.reach_range = reach;
            self.reset_stuck_timeout();
        }
        path
    }

    pub fn create_path_to_any(
        &mut self,
        view: &impl BlockView,
        profile: &MobProfile,
        mob: &MobTick,
        targets: &[BlockPos],
        reach: i32,
        follow_range: f64,
    ) -> Option<Path> {
        if targets.is_empty() {
            return None;
        }
        if mob.state.position.y < f64::from(view.min_y()) || !Self::can_update_path(mob) {
            return None;
        }
        if let Some(path) = self.path.as_ref()
            && !path.is_done()
            && self
                .target_pos
                .is_some_and(|target| targets.contains(&target))
        {
            return Some(path.clone());
        }
        let max_path_length = self.max_path_length(follow_range);
        let start = mob.state.block_pos();
        let radius = (max_path_length + BLOCK_REGION_OFFSET) as i32;
        let region = Region {
            view,
            min: start.offset(-radius, -radius, -radius).chunk(),
            max: start.offset(radius, radius, radius).chunk(),
        };
        let mut finder = PathFinder::new(profile.clone());
        finder.set_max_visited_nodes(self.max_visited_nodes);
        finder.set_max_path_length(max_path_length);
        finder.set_visited_multiplier(self.max_visited_nodes_multiplier);
        let path = finder.find_path(&region, &mob.state, targets, reach);
        if let Some(path) = path.as_ref() {
            self.target_pos = Some(path.target());
            self.reach_range = reach;
            self.reset_stuck_timeout();
        }
        path
    }

    pub fn move_to_pos(
        &mut self,
        view: &impl BlockView,
        profile: &MobProfile,
        mob: &MobTick,
        position: Vec3,
        speed_modifier: f64,
        follow_range: f64,
    ) -> bool {
        let path = self.create_path(view, profile, mob, position.block(), 1, follow_range);
        self.move_to(path, speed_modifier, view, profile, mob)
    }

    pub fn move_to_entity(
        &mut self,
        view: &impl BlockView,
        profile: &MobProfile,
        mob: &MobTick,
        position: Vec3,
        speed_modifier: f64,
        follow_range: f64,
    ) -> bool {
        let reach = if self.climber { 0 } else { 1 };
        let target = position.block();
        let path = self.create_path(view, profile, mob, target, reach, follow_range);
        if self.climber && path.is_none() {
            self.path_to_position = Some(target);
            self.speed_modifier = speed_modifier;
            return true;
        }
        path.is_some() && self.move_to(path, speed_modifier, view, profile, mob)
    }

    fn climb_towards(&mut self, profile: &MobProfile, mob: &MobTick) -> Option<MoveTarget> {
        let target = self.path_to_position?;
        let position = mob.state.position;
        let width = f64::from(profile.width);
        let level = BlockPos::new(target.x, position.y.floor() as i32, target.z);
        if closer_to_center(target, position, width)
            || (position.y > f64::from(target.y) && closer_to_center(level, position, width))
        {
            self.path_to_position = None;
            return None;
        }
        Some(MoveTarget {
            position: Vec3::new(
                f64::from(target.x),
                f64::from(target.y),
                f64::from(target.z),
            ),
            speed_modifier: self.speed_modifier,
        })
    }

    pub fn recompute_path(
        &mut self,
        view: &impl BlockView,
        profile: &MobProfile,
        mob: &MobTick,
        follow_range: f64,
    ) {
        if mob.game_time - self.time_last_recompute > MAX_TIME_RECOMPUTE
            && Self::can_update_path(mob)
        {
            if let Some(target) = self.target_pos {
                self.path = None;
                let reach = self.reach_range;
                self.path = self.create_path(view, profile, mob, target, reach, follow_range);
                self.time_last_recompute = mob.game_time;
                self.has_delayed_recomputation = false;
            }
        } else {
            self.has_delayed_recomputation = true;
        }
    }

    pub fn should_recompute_path(&self, pos: BlockPos, mob_position: Vec3) -> bool {
        if self.has_delayed_recomputation {
            return false;
        }
        let Some(path) = self.path.as_ref().filter(|path| !path.is_done()) else {
            return false;
        };
        let Some(end) = path.end_node() else {
            return false;
        };
        let middle = Vec3::new(
            (f64::from(end.pos.x) + mob_position.x) / 2.0,
            (f64::from(end.pos.y) + mob_position.y) / 2.0,
            (f64::from(end.pos.z) + mob_position.z) / 2.0,
        );
        let reach = (path.node_count() - path.next_node_index()) as f64;
        bottom_center(pos).distance_squared(Vec3::new(middle.x, middle.y - 0.5, middle.z))
            < reach * reach
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_ref()
    }

    pub fn is_done(&self) -> bool {
        self.path.as_ref().is_none_or(Path::is_done)
    }

    pub fn is_in_progress(&self) -> bool {
        !self.is_done()
    }

    pub fn stop(&mut self) {
        self.path = None;
    }

    pub fn is_stuck(&self) -> bool {
        self.stuck
    }

    pub fn max_distance_to_waypoint(&self) -> f32 {
        self.max_distance_to_waypoint
    }

    pub fn speed_modifier(&self) -> f64 {
        self.speed_modifier
    }

    pub fn set_speed_modifier(&mut self, speed_modifier: f64) {
        self.speed_modifier = speed_modifier;
    }

    pub fn reset_stuck_timeout(&mut self) {
        self.timeout_cached_node = BlockPos::new(0, 0, 0);
        self.timeout_timer = 0;
        self.timeout_limit = 0.0;
        self.stuck = false;
    }

    pub fn move_to(
        &mut self,
        path: Option<Path>,
        speed_modifier: f64,
        view: &impl BlockView,
        profile: &MobProfile,
        mob: &MobTick,
    ) -> bool {
        let Some(path) = path else {
            self.path = None;
            return false;
        };
        if !path.same_as(self.path.as_ref()) {
            self.path = Some(path);
        }
        if self.is_done() {
            return false;
        }
        self.trim(view, mob.state.position);
        if self.path.as_ref().is_none_or(|path| path.node_count() == 0) {
            return false;
        }
        self.speed_modifier = speed_modifier;
        self.last_stuck_check = self.tick;
        self.last_stuck_check_pos = temp_mob_pos(view, profile, &mob.state);
        true
    }

    pub fn set_avoid_sun(&mut self, avoid_sun: bool) {
        self.avoid_sun = avoid_sun;
    }

    pub fn avoids_sun(&self) -> bool {
        self.avoid_sun
    }

    fn trim(&mut self, view: &impl BlockView, position: Vec3) {
        self.trim_cauldrons(view);
        if !self.avoid_sun {
            return;
        }
        if view.can_see_sky(Vec3::new(position.x, position.y + 0.5, position.z).block()) {
            return;
        }
        let Some(path) = self.path.as_mut() else {
            return;
        };
        if let Some(index) = (0..path.node_count()).find(|i| view.can_see_sky(path.node(*i).pos)) {
            path.truncate_nodes(index);
        }
    }

    fn trim_cauldrons(&mut self, view: &impl BlockView) {
        let Some(path) = self.path.as_mut() else {
            return;
        };
        for index in 0..path.node_count() {
            let node = *path.node(index);
            if !view.state(node.pos).is_some_and(is_cauldron) {
                continue;
            }
            let raised = node.pos.offset(0, 1, 0);
            path.replace_node(index, node.moved(raised));
            if let Some(next) = path.nodes().get(index + 1).copied()
                && node.pos.y >= next.pos.y
            {
                let lifted = BlockPos::new(next.pos.x, raised.y, next.pos.z);
                path.replace_node(index + 1, node.moved(lifted));
            }
        }
    }

    pub fn tick(
        &mut self,
        view: &impl BlockView,
        profile: &MobProfile,
        mob: &MobTick,
    ) -> Option<MoveTarget> {
        if self.climber && self.is_done() {
            return self.climb_towards(profile, mob);
        }
        self.tick += 1;
        if self.is_done() {
            return None;
        }
        if mob.state.on_ground || mob.in_liquid || mob.passenger {
            self.follow(view, profile, mob);
        } else if let Some(path) = self.path.as_mut().filter(|path| !path.is_done()) {
            let temp = temp_mob_pos(view, profile, &mob.state);
            let next = path.next_entity_pos(profile.width);
            if temp.y > next.y
                && !mob.state.on_ground
                && temp.x.floor() as i32 == next.x.floor() as i32
                && temp.z.floor() as i32 == next.z.floor() as i32
            {
                path.advance();
            }
        }
        if self.is_done() {
            return None;
        }
        let next = self.path.as_ref()?.next_entity_pos(profile.width);
        Some(MoveTarget {
            position: Vec3::new(next.x, ground_y(view, next), next.z),
            speed_modifier: self.speed_modifier,
        })
    }

    fn follow(&mut self, view: &impl BlockView, profile: &MobProfile, mob: &MobTick) {
        let temp = temp_mob_pos(view, profile, &mob.state);
        self.max_distance_to_waypoint = waypoint_distance(profile.width);
        let Some(path) = self.path.as_mut() else {
            return;
        };
        let next = path.next_node_pos();
        let position = mob.state.position;
        let reach = f64::from(self.max_distance_to_waypoint);
        let reached = (position.x - (f64::from(next.x) + 0.5)).abs() < reach
            && (position.z - (f64::from(next.z) + 0.5)).abs() < reach
            && (position.y - f64::from(next.y)).abs() < MAX_VERTICAL_DISTANCE_TO_WAYPOINT;
        if reached || (can_cut_corner(path.next_node().path_type) && aims_past(path, temp)) {
            path.advance();
        }
        self.stuck_detection(temp, mob);
    }

    fn stuck_detection(&mut self, temp: Vec3, mob: &MobTick) {
        if self.tick - self.last_stuck_check > STUCK_CHECK_INTERVAL {
            let speed = if mob.speed >= 1.0 {
                mob.speed
            } else {
                mob.speed * mob.speed
            };
            let threshold = speed * 100.0 * STUCK_THRESHOLD_DISTANCE_FACTOR;
            if temp.distance_squared(self.last_stuck_check_pos) < f64::from(threshold * threshold) {
                self.stuck = true;
                self.stop();
            } else {
                self.stuck = false;
            }
            self.last_stuck_check = self.tick;
            self.last_stuck_check_pos = temp;
        }
        let Some(next) = self
            .path
            .as_ref()
            .filter(|path| !path.is_done())
            .map(Path::next_node_pos)
        else {
            return;
        };
        let time = mob.game_time;
        if next == self.timeout_cached_node {
            self.timeout_timer += time - self.last_timeout_check;
        } else {
            self.timeout_cached_node = next;
            let distance = temp.distance_squared(bottom_center(next)).sqrt();
            self.timeout_limit = if mob.speed > 0.0 {
                distance / f64::from(mob.speed) * 20.0
            } else {
                0.0
            };
        }
        if self.timeout_limit > 0.0 && self.timeout_timer as f64 > self.timeout_limit * 3.0 {
            self.reset_stuck_timeout();
            self.stop();
        }
        self.last_timeout_check = time;
    }
}

fn aims_past(path: &Path, temp: Vec3) -> bool {
    let index = path.next_node_index();
    if index + 1 >= path.node_count() {
        return false;
    }
    let next = bottom_center(path.next_node_pos());
    if temp.distance_squared(next) >= 4.0 {
        return false;
    }
    let following = bottom_center(path.node_pos(index + 1));
    let toward_next = next - temp;
    let toward_following = following - temp;
    let next_length = length_squared(toward_next);
    let following_length = length_squared(toward_following);
    if following_length < next_length || next_length < 0.5 {
        dot(normalize(toward_following), normalize(toward_next)) < 0.0
    } else {
        false
    }
}

fn closer_to_center(pos: BlockPos, position: Vec3, distance: f64) -> bool {
    let center = Vec3::new(
        f64::from(pos.x) + 0.5,
        f64::from(pos.y) + 0.5,
        f64::from(pos.z) + 0.5,
    );
    center.distance_squared(position) < distance * distance
}

fn bottom_center(pos: BlockPos) -> Vec3 {
    Vec3::new(
        f64::from(pos.x) + 0.5,
        f64::from(pos.y),
        f64::from(pos.z) + 0.5,
    )
}

fn length_squared(vec: Vec3) -> f64 {
    vec.x * vec.x + vec.y * vec.y + vec.z * vec.z
}

fn dot(left: Vec3, right: Vec3) -> f64 {
    left.x * right.x + left.y * right.y + left.z * right.z
}

fn normalize(vec: Vec3) -> Vec3 {
    let length = length_squared(vec).sqrt();
    if length < f64::from(1.0e-5_f32) {
        Vec3::ZERO
    } else {
        Vec3::new(vec.x / length, vec.y / length, vec.z / length)
    }
}

struct Region<'a, V> {
    view: &'a V,
    min: ChunkPos,
    max: ChunkPos,
}

impl<V: BlockView> BlockView for Region<'_, V> {
    fn state(&self, pos: BlockPos) -> Option<StateId> {
        let chunk = pos.chunk();
        if chunk.x < self.min.x
            || chunk.z < self.min.z
            || chunk.x > self.max.x
            || chunk.z > self.max.z
            || !self.view.chunk_loaded(chunk)
        {
            return Some(BlockKind::VoidAir.default_state());
        }
        Some(self.view.state(pos).unwrap_or(StateId::AIR))
    }

    fn min_y(&self) -> i32 {
        self.view.min_y()
    }

    fn max_y(&self) -> i32 {
        self.view.max_y()
    }

    fn chunk_loaded(&self, chunk: ChunkPos) -> bool {
        self.view.chunk_loaded(chunk)
    }
}

fn surface_air(view: &impl BlockView, pos: BlockPos) -> bool {
    view.state(pos).is_none_or(StateId::is_air)
}

fn surface_solid(view: &impl BlockView, pos: BlockPos) -> bool {
    view.state(pos)
        .is_some_and(|state| state.flags().contains(StateFlags::LEGACY_SOLID))
}

pub fn find_surface_position(view: &impl BlockView, pos: BlockPos) -> BlockPos {
    let mut pos = pos;
    if surface_air(view, pos) {
        let mut below = pos.offset(0, -1, 0);
        while below.y >= view.min_y() && surface_air(view, below) {
            below = below.offset(0, -1, 0);
        }
        if below.y >= view.min_y() {
            return below.offset(0, 1, 0);
        }
        let mut above = BlockPos::new(pos.x, pos.y + 1, pos.z);
        while above.y <= view.max_y() && surface_air(view, above) {
            above = above.offset(0, 1, 0);
        }
        pos = above;
    }
    if !surface_solid(view, pos) {
        return pos;
    }
    let mut above = pos.offset(0, 1, 0);
    while above.y <= view.max_y() && surface_solid(view, above) {
        above = above.offset(0, 1, 0);
    }
    above
}

pub fn temp_mob_pos(view: &impl BlockView, profile: &MobProfile, mob: &MobState) -> Vec3 {
    let position = mob.position;
    Vec3::new(
        position.x,
        f64::from(surface_y(view, profile, mob)),
        position.z,
    )
}

fn surface_y(view: &impl BlockView, profile: &MobProfile, mob: &MobState) -> i32 {
    let position = mob.position;
    if !mob.in_floatable_fluid || !profile.can_float {
        return (position.y + 0.5).floor() as i32;
    }
    let block = mob.block_pos();
    let mut y = block.y;
    let mut scanned = 0;
    while is_water(view.state(BlockPos::new(block.x, y, block.z))) {
        y += 1;
        scanned += 1;
        if scanned > SURFACE_SCAN {
            return block.y;
        }
    }
    y
}

pub fn ground_y(view: &impl BlockView, position: Vec3) -> f64 {
    let pos = position.block();
    if is_air(view.state(pos.offset(0, -1, 0))) {
        position.y
    } else {
        floor_level(view, pos)
    }
}
