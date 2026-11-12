use rspigot_blocks::StateFlags;
use rspigot_domain::{BlockPos, Vec3, mth};

use crate::behavior::control::{Behavior, BehaviorCx};
use crate::behavior::memory::{Condition, MemoryModuleType, MemoryStatus, MemoryValue, WalkTarget};
use crate::behavior::support::{
    UniformInt, dist_sqr, get_walk_target, manhattan_distance, set_walk_target,
};
use crate::path::Path;
use crate::random::RandomSource;
use crate::random_pos;
use crate::scan::manhattan_ordered;
use crate::world::AiWorld;

const FLUID_JUMP_THRESHOLD: f64 = 0.4;
const SMALL_EYE_HEIGHT: f64 = 0.4;
const WATER_SEARCH_RANGE: i32 = 5;
const WATER_SEARCH_HEIGHT: i32 = 1;
const PANIC_HORIZONTAL: i32 = 5;
const PANIC_VERTICAL: i32 = 4;
const PANIC_CAUSES: &str = "minecraft:panic_causes";
const RETARGET_DISTANCE_SQR: f64 = 4.0;
const FALLBACK_HORIZONTAL: i32 = 10;
const FALLBACK_VERTICAL: i32 = 7;
const FALLBACK_RADIANS: f64 = std::f64::consts::FRAC_PI_2;
const STUCK_COOLDOWN_BOUND: i32 = 40;
const PANIC_MIN_DURATION: i32 = 100;
const PANIC_MAX_DURATION: i32 = 120;
const MOVE_MIN_TIMEOUT: i32 = 150;
const MOVE_MAX_TIMEOUT: i32 = 250;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Swim {
    chance: f32,
    requires_can_move: bool,
}

impl Swim {
    pub fn new(chance: f32) -> Self {
        Self {
            chance,
            requires_can_move: false,
        }
    }

    pub fn creaking(chance: f32) -> Self {
        Self {
            chance,
            requires_can_move: true,
        }
    }

    fn in_floatable_fluid<W: AiWorld>(&self, cx: &BehaviorCx<'_, '_, W>) -> bool {
        let mob = cx.ai.mob;
        let threshold = if f64::from(mob.eye_height) < SMALL_EYE_HEIGHT {
            0.0
        } else {
            FLUID_JUMP_THRESHOLD
        };
        mob.fluid.water_height.max(mob.fluid.lava_height) > threshold || mob.fluid.in_lava()
    }
}

impl Behavior for Swim {
    fn entry_condition(&self) -> &[Condition] {
        &[]
    }

    fn check_extra_start_conditions<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>) -> bool {
        (!self.requires_can_move || cx.ai.brain.creaking.can_move) && self.in_floatable_fluid(cx)
    }

    fn can_still_use<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        self.in_floatable_fluid(cx)
    }

    fn tick<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        if cx.ai.brain.random.next_float() < self.chance {
            cx.ai.brain.jump.jump();
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LookAtTargetSink;

impl LookAtTargetSink {
    const CONDITION: [Condition; 1] = [(MemoryModuleType::LookTarget, MemoryStatus::ValuePresent)];
}

impl Behavior for LookAtTargetSink {
    fn entry_condition(&self) -> &[Condition] {
        &Self::CONDITION
    }

    fn can_still_use<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        let Some(target) = cx
            .brain
            .memories
            .get(MemoryModuleType::LookTarget)
            .and_then(MemoryValue::as_tracker)
            .copied()
        else {
            return false;
        };
        target.is_visible_by(cx.ai, &cx.brain.memories)
    }

    fn stop<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        cx.brain.memories.erase(MemoryModuleType::LookTarget);
    }

    fn tick<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        let target = cx
            .brain
            .memories
            .get(MemoryModuleType::LookTarget)
            .and_then(MemoryValue::as_tracker)
            .copied();
        if let Some(position) = target.and_then(|target| target.current_position(cx.ai.world)) {
            cx.ai.brain.look.look_at(position);
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MoveToTargetSink {
    path: Option<Path>,
    last_target_pos: Option<BlockPos>,
    speed_modifier: f32,
    remaining_cooldown: i32,
}

impl Default for MoveToTargetSink {
    fn default() -> Self {
        Self::new()
    }
}

impl MoveToTargetSink {
    const CONDITION: [Condition; 3] = [
        (
            MemoryModuleType::CantReachWalkTargetSince,
            MemoryStatus::Registered,
        ),
        (MemoryModuleType::Path, MemoryStatus::ValueAbsent),
        (MemoryModuleType::WalkTarget, MemoryStatus::ValuePresent),
    ];

    pub fn new() -> Self {
        Self {
            path: None,
            last_target_pos: None,
            speed_modifier: 0.0,
            remaining_cooldown: 0,
        }
    }

    pub const fn min_timeout() -> i32 {
        MOVE_MIN_TIMEOUT
    }

    pub const fn max_timeout() -> i32 {
        MOVE_MAX_TIMEOUT
    }

    fn reached_target<W: AiWorld>(cx: &BehaviorCx<'_, '_, W>, walk_target: &WalkTarget) -> bool {
        let Some(target) = walk_target.target.current_block_position(cx.ai.world) else {
            return false;
        };
        manhattan_distance(target, cx.ai.mob.block_pos()) <= walk_target.close_enough_dist
    }

    fn is_walk_target_spectator<W: AiWorld>(
        cx: &BehaviorCx<'_, '_, W>,
        walk_target: &WalkTarget,
    ) -> bool {
        match walk_target.target {
            crate::behavior::PositionTracker::Entity { id, .. } => cx
                .ai
                .world
                .living(id)
                .is_some_and(|living| living.spectator),
            crate::behavior::PositionTracker::Block { .. } => false,
        }
    }

    fn try_compute_path<W: AiWorld>(
        &mut self,
        cx: &mut BehaviorCx<'_, '_, W>,
        walk_target: &WalkTarget,
        now: i64,
    ) -> bool {
        let Some(target_pos) = walk_target.target.current_block_position(cx.ai.world) else {
            return false;
        };
        self.path = cx.ai.create_path_to(target_pos, 0);
        self.speed_modifier = walk_target.speed_modifier;
        if Self::reached_target(cx, walk_target) {
            cx.brain
                .memories
                .erase(MemoryModuleType::CantReachWalkTargetSince);
            return false;
        }
        let can_reach = self.path.as_ref().is_some_and(Path::can_reach);
        if can_reach {
            cx.brain
                .memories
                .erase(MemoryModuleType::CantReachWalkTargetSince);
        } else if !cx
            .brain
            .memories
            .has_value(MemoryModuleType::CantReachWalkTargetSince)
        {
            cx.brain.memories.set(
                MemoryModuleType::CantReachWalkTargetSince,
                MemoryValue::Long(now),
            );
        }
        if self.path.is_some() {
            return true;
        }
        let towards = Vec3::new(
            f64::from(target_pos.x) + 0.5,
            f64::from(target_pos.y),
            f64::from(target_pos.z) + 0.5,
        );
        let Some(position) = random_pos::pos_towards(
            cx.ai,
            FALLBACK_HORIZONTAL,
            FALLBACK_VERTICAL,
            towards,
            FALLBACK_RADIANS,
        ) else {
            return false;
        };
        self.path = cx.ai.create_path_to(position.block(), 0);
        self.path.is_some()
    }

    fn publish_path<W: AiWorld>(cx: &mut BehaviorCx<'_, '_, W>, path: Option<&Path>) {
        cx.brain.memories.set_option(
            MemoryModuleType::Path,
            path.map(|path| MemoryValue::Path(path.clone())),
        );
    }
}

impl Behavior for MoveToTargetSink {
    fn entry_condition(&self) -> &[Condition] {
        &Self::CONDITION
    }

    fn check_extra_start_conditions<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>) -> bool {
        if self.remaining_cooldown > 0 {
            self.remaining_cooldown -= 1;
            return false;
        }
        let Some(walk_target) = get_walk_target(cx) else {
            return false;
        };
        let reached = Self::reached_target(cx, &walk_target);
        let now = cx.ai.world.game_time();
        if !reached && self.try_compute_path(cx, &walk_target, now) {
            self.last_target_pos = walk_target.target.current_block_position(cx.ai.world);
            return true;
        }
        cx.brain.memories.erase(MemoryModuleType::WalkTarget);
        if reached {
            cx.brain
                .memories
                .erase(MemoryModuleType::CantReachWalkTargetSince);
        }
        false
    }

    fn start<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        Self::publish_path(cx, self.path.as_ref());
        cx.ai
            .move_along(self.path.clone(), f64::from(self.speed_modifier));
    }

    fn can_still_use<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        if self.path.is_none() || self.last_target_pos.is_none() {
            return false;
        }
        let Some(walk_target) = get_walk_target(cx) else {
            return false;
        };
        let spectator = Self::is_walk_target_spectator(cx, &walk_target);
        !cx.ai.brain.navigation.is_done() && !Self::reached_target(cx, &walk_target) && !spectator
    }

    fn tick<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, now: i64) {
        let navigation_path = cx.ai.brain.navigation.path().cloned();
        let changed = match (&self.path, &navigation_path) {
            (Some(ours), theirs) => !ours.same_as(theirs.as_ref()),
            (None, theirs) => theirs.is_some(),
        };
        if changed {
            self.path = navigation_path.clone();
            Self::publish_path(cx, navigation_path.as_ref());
        }
        if navigation_path.is_none() {
            return;
        }
        let Some(last) = self.last_target_pos else {
            return;
        };
        let Some(walk_target) = get_walk_target(cx) else {
            return;
        };
        let Some(current) = walk_target.target.current_block_position(cx.ai.world) else {
            return;
        };
        if dist_sqr(current, last) > RETARGET_DISTANCE_SQR
            && self.try_compute_path(cx, &walk_target, now)
        {
            self.last_target_pos = walk_target.target.current_block_position(cx.ai.world);
            self.start(cx, now);
        }
    }

    fn stop<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        if let Some(walk_target) = get_walk_target(cx)
            && !Self::reached_target(cx, &walk_target)
            && cx.ai.brain.navigation.is_stuck()
        {
            self.remaining_cooldown = cx.ai.brain.random.next_int(STUCK_COOLDOWN_BOUND);
        }
        cx.ai.stop_navigation();
        cx.brain.memories.erase(MemoryModuleType::WalkTarget);
        cx.brain.memories.erase(MemoryModuleType::Path);
        self.path = None;
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AnimalPanic {
    speed_multiplier: f32,
    panic_causes: &'static str,
}

impl AnimalPanic {
    const CONDITION: [Condition; 2] = [
        (MemoryModuleType::IsPanicking, MemoryStatus::Registered),
        (MemoryModuleType::HurtBy, MemoryStatus::Registered),
    ];

    pub fn new(speed_multiplier: f32) -> Self {
        Self::with_causes(speed_multiplier, PANIC_CAUSES)
    }

    pub fn with_causes(speed_multiplier: f32, panic_causes: &'static str) -> Self {
        Self {
            speed_multiplier,
            panic_causes,
        }
    }

    pub const fn min_duration() -> i32 {
        PANIC_MIN_DURATION
    }

    pub const fn max_duration() -> i32 {
        PANIC_MAX_DURATION
    }

    fn is_water<W: AiWorld>(cx: &BehaviorCx<'_, '_, W>, pos: BlockPos) -> bool {
        cx.ai
            .world
            .state(pos)
            .is_some_and(|state| state.flags().contains(StateFlags::WATER))
    }

    fn look_for_water<W: AiWorld>(cx: &BehaviorCx<'_, '_, W>) -> Option<BlockPos> {
        let origin = cx.ai.mob.block_pos();
        if cx
            .ai
            .world
            .state(origin)
            .is_none_or(|state| !state.collision_shape().is_empty())
        {
            return None;
        }
        let wide = cx.ai.mob.width.ceil() as i32 == 2;
        manhattan_ordered(
            origin,
            WATER_SEARCH_RANGE,
            WATER_SEARCH_HEIGHT,
            WATER_SEARCH_RANGE,
        )
        .filter(|pos| Self::is_water(cx, *pos))
        .find(|pos| {
            !wide
                || (Self::is_water(cx, pos.offset(0, 0, 1))
                    && Self::is_water(cx, pos.offset(1, 0, 0))
                    && Self::is_water(cx, pos.offset(1, 0, 1)))
        })
    }

    fn panic_position<W: AiWorld>(&self, cx: &mut BehaviorCx<'_, '_, W>) -> Option<Vec3> {
        if cx.ai.mob.on_fire
            && let Some(water) = Self::look_for_water(cx)
        {
            return Some(Vec3::new(
                f64::from(water.x) + 0.5,
                f64::from(water.y),
                f64::from(water.z) + 0.5,
            ));
        }
        random_pos::land_pos(cx.ai, PANIC_HORIZONTAL, PANIC_VERTICAL)
    }
}

impl Behavior for AnimalPanic {
    fn entry_condition(&self) -> &[Condition] {
        &Self::CONDITION
    }

    fn check_extra_start_conditions<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>) -> bool {
        let hurt = cx
            .brain
            .memories
            .get(MemoryModuleType::HurtBy)
            .and_then(|value| match value {
                MemoryValue::HurtBy(damage) => Some(damage.kind.in_tag(self.panic_causes)),
                _ => None,
            })
            .unwrap_or(false);
        hurt || cx.brain.memories.has_value(MemoryModuleType::IsPanicking)
    }

    fn can_still_use<W: AiWorld>(&mut self, _cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        true
    }

    fn start<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        cx.brain
            .memories
            .set(MemoryModuleType::IsPanicking, MemoryValue::Bool(true));
        cx.brain.memories.erase(MemoryModuleType::WalkTarget);
        cx.ai.stop_navigation();
    }

    fn stop<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        cx.brain.memories.erase(MemoryModuleType::IsPanicking);
    }

    fn tick<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        if !cx.ai.brain.navigation.is_done() {
            return;
        }
        if let Some(position) = self.panic_position(cx) {
            set_walk_target(cx, WalkTarget::position(position, self.speed_multiplier, 0));
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RandomLookAround {
    interval: UniformInt,
    max_yaw: f32,
    min_pitch: f32,
    pitch_range: f32,
}

impl RandomLookAround {
    const CONDITION: [Condition; 2] = [
        (MemoryModuleType::LookTarget, MemoryStatus::ValueAbsent),
        (
            MemoryModuleType::GazeCooldownTicks,
            MemoryStatus::ValueAbsent,
        ),
    ];

    pub fn new(interval: UniformInt, max_yaw: f32, min_pitch: f32, max_pitch: f32) -> Self {
        assert!(
            max_pitch >= min_pitch,
            "Minimum pitch is larger than maximum pitch! {min_pitch} > {max_pitch}"
        );
        Self {
            interval,
            max_yaw,
            min_pitch,
            pitch_range: max_pitch - min_pitch,
        }
    }
}

impl Behavior for RandomLookAround {
    fn entry_condition(&self) -> &[Condition] {
        &Self::CONDITION
    }

    fn start<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        let pitch = mth::clamp(
            cx.ai.brain.random.next_float() * self.pitch_range + self.min_pitch,
            -90.0,
            90.0,
        );
        let yaw = mth::wrap_degrees(
            (cx.ai.mob.yaw + cx.ai.brain.random.next_float() * 2.0 * self.max_yaw) - self.max_yaw,
        );
        let direction = mth::view_vector(pitch, yaw);
        let eyes = cx.ai.mob.eye_position();
        cx.brain.memories.set(
            MemoryModuleType::LookTarget,
            MemoryValue::Tracker(crate::behavior::PositionTracker::of_position(
                eyes + direction,
            )),
        );
        let cooldown = self.interval.sample(&mut cx.ai.brain.random);
        cx.brain.memories.set(
            MemoryModuleType::GazeCooldownTicks,
            MemoryValue::Int(cooldown),
        );
    }
}
