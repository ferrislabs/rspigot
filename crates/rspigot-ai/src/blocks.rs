use rspigot_blocks::{BlockKind, StateId};
use rspigot_domain::particle::{Particle, ParticleData};
use rspigot_domain::poi::Occupancy;
use rspigot_domain::{BlockPos, Vec3};
use rspigot_entities::SoundSource;
use rspigot_items::ItemKind;

use crate::action::MobAction;
use crate::brain::AiContext;
use crate::goal::reduced_tick_delay;
use crate::path::Path;
use crate::random::RandomSource;
use crate::random_pos::{HALF_PI, land_pos_scored, pos_towards};
use crate::scan::closer_to_center_than;
use crate::world::{AiWorld, Hand};

const GIVE_UP_TICKS: i32 = 1200;
const SEARCH_RANGE: i32 = 24;
const EGG_ACCEPTED_DISTANCE: f64 = 1.14;
const RECALCULATE_PERIOD: i32 = 40;
const FOUND_DELAY: i32 = 20;
const BREAK_AFTER: i32 = 60;
const BOUNCE: f64 = 0.3;
const EGG_PARTICLE_SPREAD: f64 = 0.08;
const PARTICLE_SPEED: f64 = 0.150_000_005_960_464_48;
const POOF_COUNT: usize = 20;
const POOF_SPREAD: f64 = 0.02;
const DESTROY_EGG: &str = "minecraft:entity.zombie.destroy_egg";
const DOOR_BREAK_TIME: i32 = 240;
const DOOR_REACH_SQR: f64 = 2.25;
const DOOR_KEEP_DISTANCE: f64 = 2.0;
const DOOR_KNOCK_CHANCE: i32 = 20;
pub const ZOMBIE_ATTACK_DOOR_EVENT: i32 = 1019;
pub const ZOMBIE_BREAK_DOOR_EVENT: i32 = 1021;
const HARD_DIFFICULTY: i32 = 3;

fn is_turtle_egg(state: Option<StateId>) -> bool {
    state.and_then(StateId::kind) == Some(BlockKind::TurtleEgg)
}

fn is_air(state: Option<StateId>) -> bool {
    state.is_some_and(StateId::is_air)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RemoveTurtleEggGoal {
    speed_modifier: f64,
    vertical_range: i32,
    next_start_tick: i32,
    try_ticks: i32,
    max_stay_ticks: i32,
    block_pos: BlockPos,
    reached_target: bool,
    ticks_since_reached_goal: i32,
}

impl RemoveTurtleEggGoal {
    pub fn new(speed_modifier: f64, vertical_range: i32) -> Self {
        Self {
            speed_modifier,
            vertical_range,
            next_start_tick: 0,
            try_ticks: 0,
            max_stay_ticks: 0,
            block_pos: BlockPos::new(0, 0, 0),
            reached_target: false,
            ticks_since_reached_goal: 0,
        }
    }

    fn is_valid_target<W: AiWorld>(context: &AiContext<'_, W>, pos: BlockPos) -> bool {
        let world = context.world;
        world.chunk_loaded(pos.chunk())
            && is_turtle_egg(world.state(pos))
            && is_air(world.state(pos.offset(0, 1, 0)))
            && is_air(world.state(pos.offset(0, 2, 0)))
    }

    fn find_nearest_block<W: AiWorld>(&mut self, context: &AiContext<'_, W>) -> bool {
        let origin = context.mob.block_pos();
        let mut dy = 0;
        while dy <= self.vertical_range {
            for radius in 0..SEARCH_RANGE {
                let mut dx = 0;
                while dx <= radius {
                    let mut dz = if dx < radius && dx > -radius {
                        radius
                    } else {
                        0
                    };
                    while dz <= radius {
                        let pos = origin.offset(dx, dy - 1, dz);
                        if context.mob.is_within_home(pos) && Self::is_valid_target(context, pos) {
                            self.block_pos = pos;
                            return true;
                        }
                        dz = if dz > 0 { -dz } else { 1 - dz };
                    }
                    dx = if dx > 0 { -dx } else { 1 - dx };
                }
            }
            dy = if dy > 0 { -dy } else { 1 - dy };
        }
        false
    }

    pub(crate) fn can_use<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) -> bool {
        if !context.world.mob_griefing() {
            return false;
        }
        if self.next_start_tick > 0 {
            self.next_start_tick -= 1;
            return false;
        }
        if self.find_nearest_block(context) {
            self.next_start_tick = reduced_tick_delay(FOUND_DELAY);
            return true;
        }
        self.next_start_tick = reduced_tick_delay(200 + context.brain.random.next_int(200));
        false
    }

    pub(crate) fn can_continue_to_use<W: AiWorld>(
        &mut self,
        context: &mut AiContext<'_, W>,
    ) -> bool {
        self.try_ticks >= -self.max_stay_ticks
            && self.try_ticks <= GIVE_UP_TICKS
            && Self::is_valid_target(context, self.block_pos)
    }

    pub(crate) fn start<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        let pos = self.block_pos;
        let at = Vec3::new(
            f64::from(pos.x) + 0.5,
            f64::from(pos.y + 1),
            f64::from(pos.z) + 0.5,
        );
        context.move_to_pos(at, self.speed_modifier);
        self.try_ticks = 0;
        let inner = context.brain.random.next_int(1200) + 1200;
        self.max_stay_ticks = context.brain.random.next_int(inner) + 1200;
        self.ticks_since_reached_goal = 0;
    }

    pub(crate) fn stop<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        context.body.fall_distance = 1.0;
    }

    fn pos_with_block<W: AiWorld>(context: &AiContext<'_, W>, pos: BlockPos) -> Option<BlockPos> {
        let world = context.world;
        if is_turtle_egg(world.state(pos)) {
            return Some(pos);
        }
        [
            pos.offset(0, -1, 0),
            pos.offset(-1, 0, 0),
            pos.offset(1, 0, 0),
            pos.offset(0, 0, -1),
            pos.offset(0, 0, 1),
            pos.offset(0, -2, 0),
        ]
        .into_iter()
        .find(|candidate| is_turtle_egg(world.state(*candidate)))
    }

    pub(crate) fn tick<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        let target = self.block_pos.offset(0, 1, 0);
        if closer_to_center_than(target, context.mob.position, EGG_ACCEPTED_DISTANCE) {
            self.reached_target = true;
            self.try_ticks -= 1;
        } else {
            self.reached_target = false;
            self.try_ticks += 1;
            if self.try_ticks % RECALCULATE_PERIOD == 0 {
                let at = Vec3::new(
                    f64::from(target.x) + 0.5,
                    f64::from(target.y),
                    f64::from(target.z) + 0.5,
                );
                context.move_to_pos(at, self.speed_modifier);
            }
        }
        let Some(egg) = Self::pos_with_block(context, context.mob.block_pos()) else {
            return;
        };
        if !self.reached_target {
            return;
        }
        let center = Vec3::new(
            f64::from(egg.x) + 0.5,
            f64::from(egg.y),
            f64::from(egg.z) + 0.5,
        );
        if self.ticks_since_reached_goal > 0 {
            let v = context.body.velocity;
            context.body.velocity = Vec3::new(v.x, BOUNCE, v.z);
            let random = &mut context.brain.random;
            let spread = Vec3::new(
                (f64::from(random.next_float()) - 0.5) * EGG_PARTICLE_SPREAD,
                (f64::from(random.next_float()) - 0.5) * EGG_PARTICLE_SPREAD,
                (f64::from(random.next_float()) - 0.5) * EGG_PARTICLE_SPREAD,
            );
            context.act(MobAction::Particle(Particle {
                kind: "minecraft:item",
                data: ParticleData::Item(ItemKind::Egg.id()),
                position: Vec3::new(center.x, center.y + 0.7, center.z),
                count: 3,
                spread,
                speed: PARTICLE_SPEED,
            }));
        }
        if self.ticks_since_reached_goal % 2 == 0 {
            let v = context.body.velocity;
            context.body.velocity = Vec3::new(v.x, -BOUNCE, v.z);
            if self.ticks_since_reached_goal % 6 == 0 {
                let pitch = 0.9 + context.brain.random.next_float() * 0.2;
                context.act(MobAction::BlockSound {
                    sound: DESTROY_EGG,
                    source: SoundSource::Hostile,
                    pos: self.block_pos,
                    volume: 0.5,
                    pitch,
                });
            }
        }
        if self.ticks_since_reached_goal > BREAK_AFTER {
            context.act(MobAction::RemoveTurtleEgg(egg));
            return;
        }
        self.ticks_since_reached_goal += 1;
    }
}

pub fn egg_poof(random: &mut impl RandomSource, egg: BlockPos) -> Vec<Particle> {
    let center = Vec3::new(
        f64::from(egg.x) + 0.5,
        f64::from(egg.y),
        f64::from(egg.z) + 0.5,
    );
    (0..POOF_COUNT)
        .map(|_| {
            let spread = Vec3::new(
                random.next_gaussian() * POOF_SPREAD,
                random.next_gaussian() * POOF_SPREAD,
                random.next_gaussian() * POOF_SPREAD,
            );
            Particle {
                kind: "minecraft:poof",
                data: ParticleData::None,
                position: center,
                count: 1,
                spread,
                speed: PARTICLE_SPEED,
            }
        })
        .collect()
}

fn is_wooden_door(state: Option<StateId>) -> bool {
    state.and_then(StateId::kind).is_some_and(|kind| {
        kind != BlockKind::IronDoor
            && rspigot_data::tags::tag("minecraft:block", "minecraft:doors")
                .is_some_and(|ids| ids.binary_search(&u32::from(kind.id().0)).is_ok())
    })
}

fn is_open(state: Option<StateId>) -> bool {
    state.and_then(|state| state.property("open")) == Some("true")
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BreakDoorGoal {
    normal_ok: bool,
    door_pos: BlockPos,
    has_door: bool,
    passed: bool,
    door_open_dir: (f32, f32),
    break_time: i32,
    last_break_progress: i32,
}

impl Default for BreakDoorGoal {
    fn default() -> Self {
        Self {
            normal_ok: false,
            door_pos: BlockPos::new(0, 0, 0),
            has_door: false,
            passed: false,
            door_open_dir: (0.0, 0.0),
            break_time: 0,
            last_break_progress: -1,
        }
    }
}

const NORMAL_DIFFICULTY: i32 = 2;

impl BreakDoorGoal {
    pub fn vindicator() -> Self {
        Self {
            normal_ok: true,
            ..Self::default()
        }
    }

    pub(crate) fn door_is_open<W: AiWorld>(&mut self, context: &AiContext<'_, W>) -> bool {
        if !self.has_door {
            return false;
        }
        let state = context.world.state(self.door_pos);
        if !state.and_then(StateId::kind).is_some_and(|kind| {
            rspigot_data::tags::tag("minecraft:block", "minecraft:doors")
                .is_some_and(|ids| ids.binary_search(&u32::from(kind.id().0)).is_ok())
        }) {
            self.has_door = false;
            return false;
        }
        is_open(state)
    }

    fn valid_difficulty<W: AiWorld>(&self, context: &AiContext<'_, W>) -> bool {
        let difficulty = context.world.difficulty();
        difficulty == HARD_DIFFICULTY || (self.normal_ok && difficulty == NORMAL_DIFFICULTY)
    }

    pub(crate) fn find_door<W: AiWorld>(&mut self, context: &AiContext<'_, W>) -> bool {
        if !context.mob.horizontal_collision {
            return false;
        }
        let Some(path) = context
            .brain
            .navigation
            .path()
            .filter(|path| !path.is_done())
        else {
            return false;
        };
        let limit = (path.next_node_index() + 2).min(path.node_count());
        for index in 0..limit {
            let node = path.node(index).pos;
            self.door_pos = node.offset(0, 1, 0);
            let mob = context.mob.position;
            let dx = f64::from(self.door_pos.x) - mob.x;
            let dz = f64::from(self.door_pos.z) - mob.z;
            if dx * dx + dz * dz <= DOOR_REACH_SQR {
                self.has_door = is_wooden_door(context.world.state(self.door_pos));
                if self.has_door {
                    return true;
                }
            }
        }
        self.door_pos = context.mob.block_pos().offset(0, 1, 0);
        self.has_door = is_wooden_door(context.world.state(self.door_pos));
        self.has_door
    }

    pub(crate) fn can_use<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) -> bool {
        self.find_door(context)
            && context.world.mob_griefing()
            && self.valid_difficulty(context)
            && !self.door_is_open(context)
    }

    pub(crate) fn can_continue_to_use<W: AiWorld>(
        &mut self,
        context: &mut AiContext<'_, W>,
    ) -> bool {
        self.break_time <= DOOR_BREAK_TIME
            && !self.door_is_open(context)
            && closer_to_center_than(self.door_pos, context.mob.position, DOOR_KEEP_DISTANCE)
            && self.valid_difficulty(context)
    }

    pub(crate) fn interact_start<W: AiWorld>(&mut self, context: &AiContext<'_, W>) {
        self.passed = false;
        let mob = context.mob.position;
        self.door_open_dir = (
            (f64::from(self.door_pos.x) + 0.5 - mob.x) as f32,
            (f64::from(self.door_pos.z) + 0.5 - mob.z) as f32,
        );
    }

    pub(crate) fn interact_tick<W: AiWorld>(&mut self, context: &AiContext<'_, W>) {
        let mob = context.mob.position;
        let dir = (
            (f64::from(self.door_pos.x) + 0.5 - mob.x) as f32,
            (f64::from(self.door_pos.z) + 0.5 - mob.z) as f32,
        );
        if self.door_open_dir.0 * dir.0 + self.door_open_dir.1 * dir.1 < 0.0 {
            self.passed = true;
        }
    }

    pub(crate) fn door_pos(&self) -> BlockPos {
        self.door_pos
    }

    pub(crate) fn has_door(&self) -> bool {
        self.has_door
    }

    pub(crate) fn start<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        self.interact_start(context);
        self.break_time = 0;
    }

    pub(crate) fn stop<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        context.act(MobAction::DestroyProgress {
            pos: self.door_pos,
            stage: -1,
        });
    }

    pub(crate) fn tick<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        self.interact_tick(context);
        if context.brain.random.next_int(DOOR_KNOCK_CHANCE) == 0 {
            context.act(MobAction::LevelEvent {
                event: ZOMBIE_ATTACK_DOOR_EVENT,
                pos: self.door_pos,
                data: 0,
            });
            if !context.mob.swinging {
                context.act(MobAction::Swing(Hand::Main));
            }
        }
        self.break_time += 1;
        let progress = (self.break_time as f32 / DOOR_BREAK_TIME as f32 * 10.0) as i32;
        if progress != self.last_break_progress {
            context.act(MobAction::DestroyProgress {
                pos: self.door_pos,
                stage: progress,
            });
            self.last_break_progress = progress;
        }
        if self.break_time == DOOR_BREAK_TIME && self.valid_difficulty(context) {
            context.act(MobAction::BreakDoor(self.door_pos));
        }
    }

    pub(crate) fn passed(&self) -> bool {
        self.passed
    }
}

const OPEN_DOOR_FORGET_TICKS: i32 = 20;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct OpenDoorGoal {
    door: BreakDoorGoal,
    forget_time: i32,
    close_after: bool,
}

impl OpenDoorGoal {
    fn set_open<W: AiWorld>(&self, context: &mut AiContext<'_, W>, open: bool) {
        if self.door.has_door() {
            context.act(MobAction::SetDoorOpen {
                pos: self.door.door_pos(),
                open,
                interact: false,
            });
        }
    }

    pub(crate) fn can_use<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) -> bool {
        self.door.find_door(context)
    }

    pub(crate) fn can_continue_to_use(&self) -> bool {
        self.close_after && 0 < self.forget_time && !self.door.passed()
    }

    pub(crate) fn start<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        self.door.interact_start(context);
        self.forget_time = OPEN_DOOR_FORGET_TICKS;
        self.set_open(context, true);
    }

    pub(crate) fn stop<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        self.set_open(context, false);
    }

    pub(crate) fn tick<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        self.forget_time -= 1;
        self.door.interact_tick(context);
    }
}

const VILLAGE_FAR_SECTIONS: i32 = 6;
const VILLAGE_POI_RANGE: i32 = 10;
const VISITED_LIMIT: usize = 15;

#[derive(Debug, Clone, PartialEq)]
pub struct MoveThroughVillageGoal {
    speed_modifier: f64,
    only_at_night: bool,
    distance_to_poi: i32,
    path: Option<Path>,
    poi_pos: BlockPos,
    visited: Vec<BlockPos>,
}

impl Default for MoveThroughVillageGoal {
    fn default() -> Self {
        Self::new(1.0, true, 4)
    }
}

fn dist_sqr(a: BlockPos, b: BlockPos) -> f64 {
    let dx = f64::from(a.x - b.x);
    let dy = f64::from(a.y - b.y);
    let dz = f64::from(a.z - b.z);
    dx * dx + dy * dy + dz * dz
}

impl MoveThroughVillageGoal {
    pub fn new(speed_modifier: f64, only_at_night: bool, distance_to_poi: i32) -> Self {
        Self {
            speed_modifier,
            only_at_night,
            distance_to_poi,
            path: None,
            poi_pos: BlockPos::new(0, 0, 0),
            visited: Vec::new(),
        }
    }

    #[cfg(test)]
    pub(crate) fn is_ready(&self) -> bool {
        self.path.is_some()
    }

    #[cfg(test)]
    pub(crate) fn visited(&self) -> &[BlockPos] {
        &self.visited
    }

    #[cfg(test)]
    pub(crate) fn mark_visited(&mut self, pos: BlockPos) {
        self.visited.push(pos);
    }

    fn has_not_visited(visited: &[BlockPos], poi: BlockPos) -> bool {
        !visited.contains(&poi)
    }

    fn find_unvisited_poi<W: AiWorld>(
        world: &W,
        visited: &[BlockPos],
        center: BlockPos,
    ) -> Option<BlockPos> {
        world
            .pois_in_range(center, VILLAGE_POI_RANGE, Occupancy::IsOccupied)
            .into_iter()
            .filter(|record| record.kind().is_village())
            .map(|record| record.pos())
            .find(|pos| Self::has_not_visited(visited, *pos))
    }

    fn create_path<W: AiWorld>(context: &mut AiContext<'_, W>, pos: BlockPos) -> Option<Path> {
        context.brain.profile.can_open_doors = context.brain.can_break_doors;
        let path = context.create_path_to(pos, 0);
        context.brain.profile.can_open_doors = true;
        path
    }

    pub(crate) fn can_use<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) -> bool {
        if self.visited.len() > VISITED_LIMIT {
            self.visited.remove(0);
        }
        if self.only_at_night && context.world.bright_outside() {
            return false;
        }
        let pos = context.mob.block_pos();
        if !context.world.is_close_to_village(pos, VILLAGE_FAR_SECTIONS) {
            return false;
        }
        let visited = self.visited.clone();
        let Some(land) = land_pos_scored(context, 15, 7, |context, candidate| {
            if !context.world.is_village(candidate) {
                return f64::NEG_INFINITY;
            }
            Self::find_unvisited_poi(context.world, &visited, candidate)
                .map_or(f64::NEG_INFINITY, |poi| -dist_sqr(poi, pos))
        }) else {
            return false;
        };
        let Some(target) = Self::find_unvisited_poi(context.world, &self.visited, land.block())
        else {
            return false;
        };
        self.poi_pos = target;
        let mut path = Self::create_path(context, target);
        if path.is_none() {
            let Some(partial) = pos_towards(context, 10, 7, target.center(), HALF_PI) else {
                return false;
            };
            path = Self::create_path(context, partial.block());
            if path.is_none() {
                return false;
            }
        }
        if let Some(found) = &path {
            for index in 0..found.node_count() {
                let node = found.node(index).pos;
                if is_wooden_door(context.world.state(node.offset(0, 1, 0))) {
                    path = context.create_path_to(node, 0);
                    break;
                }
            }
        }
        self.path = path;
        self.path.is_some()
    }

    pub(crate) fn can_continue_to_use<W: AiWorld>(
        &mut self,
        context: &mut AiContext<'_, W>,
    ) -> bool {
        if context.brain.navigation.is_done() {
            return false;
        }
        let reach = f64::from(context.mob.width + self.distance_to_poi as f32);
        !closer_to_center_than(self.poi_pos, context.mob.position, reach)
    }

    pub(crate) fn start<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        context.move_along(self.path.clone(), self.speed_modifier);
    }

    pub(crate) fn stop<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        let reach = f64::from(self.distance_to_poi);
        if context.brain.navigation.is_done()
            || closer_to_center_than(self.poi_pos, context.mob.position, reach)
        {
            self.visited.push(self.poi_pos);
        }
    }
}
