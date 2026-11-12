use rspigot_blocks::{Aabb, BlockKind, StateFlags};
use rspigot_domain::entity::Pose;
use rspigot_domain::entity::ageable::dimensions_in_pose;
use rspigot_domain::{BlockPos, Vec3};
use rspigot_entities::EntityDimensions;

use crate::action::MobAction;
use crate::behavior::control::{Behavior, BehaviorCx};
use crate::behavior::memory::{Condition, MemoryModuleType, MemoryStatus, MemoryValue};
use crate::behavior::support::{UniformInt, dist_sqr};
use crate::behavior::tracker::PositionTracker;
use crate::path::Path;
use crate::random::RandomSource;
use crate::view::collides;
use crate::world::AiWorld;

pub const FIND_JUMP_TRIES: i32 = 20;
pub const PREPARE_JUMP_DURATION: i64 = 40;
pub const MIN_PATHFIND_DISTANCE_TO_VALID_JUMP: i32 = 8;
pub const LONG_JUMP_TIME_OUT: i32 = 200;
pub const MID_JUMP_TIME_OUT: i32 = 100;
pub const ALLOWED_ANGLES: [i32; 4] = [65, 70, 75, 80];
const JUMP_SPEED_SCALE: f32 = 0.95;
const TARGET_OFFSET: f64 = 0.5;
const COLLISION_STEP_SCALE: f32 = 0.9;
const LANDING_DAMPING: f64 = 0.1;
const LANDING_VOLUME: f32 = 2.0;
const JUMP_VOLUME: f32 = 1.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct JumpDimensions {
    width: f32,
    height: f32,
}

impl JumpDimensions {
    pub fn of(dimensions: EntityDimensions) -> Self {
        Self {
            width: dimensions.width,
            height: dimensions.height,
        }
    }

    fn bounds(self, position: Vec3) -> Aabb {
        let half = f64::from(self.width / 2.0);
        Aabb::new(
            position.x - half,
            position.y,
            position.z - half,
            position.x + half,
            position.y + f64::from(self.height),
            position.z + half,
        )
    }
}

fn is_clear_transition(
    world: &impl AiWorld,
    dimensions: JumpDimensions,
    from: Vec3,
    to: Vec3,
) -> bool {
    let delta = to - from;
    let min = f64::from(dimensions.width.min(dimensions.height));
    let steps = (delta.length() / min).ceil() as i32;
    let direction = delta.normalize();
    let mut current = from;
    for index in 0..steps {
        current = if index == steps - 1 {
            to
        } else {
            current + direction.scale(min * f64::from(COLLISION_STEP_SCALE))
        };
        if collides(world, &dimensions.bounds(current)) {
            return false;
        }
    }
    true
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct JumpBody {
    pub position: Vec3,
    pub gravity: f64,
    pub dimensions: JumpDimensions,
}

pub fn calculate_jump_vector_for_angle(
    world: &impl AiWorld,
    body: JumpBody,
    target: Vec3,
    max_jump_velocity: f32,
    angle: i32,
    check_collision: bool,
) -> Option<Vec3> {
    let JumpBody {
        position,
        gravity,
        dimensions,
    } = body;
    let horizontal = Vec3::new(target.x - position.x, 0.0, target.z - position.z)
        .normalize()
        .scale(TARGET_OFFSET);
    let delta = (target - horizontal) - position;
    let angle_rad = angle as f32 * std::f32::consts::PI / 180.0;
    let yaw = delta.z.atan2(delta.x);
    let horizontal_distance_sqr = Vec3::new(delta.x, 0.0, delta.z).length_sqr();
    let horizontal_distance = horizontal_distance_sqr.sqrt();
    let height = delta.y;
    let sin_double = f64::from(2.0 * angle_rad).sin();
    let cos_squared = f64::from(angle_rad).cos().powi(2);
    let sin_angle = f64::from(angle_rad).sin();
    let cos_angle = f64::from(angle_rad).cos();
    let sin_yaw = yaw.sin();
    let cos_yaw = yaw.cos();
    let velocity_squared = horizontal_distance_sqr * gravity
        / (horizontal_distance * sin_double - height * 2.0 * cos_squared);
    if velocity_squared < 0.0 {
        return None;
    }
    let velocity = velocity_squared.sqrt();
    if velocity > f64::from(max_jump_velocity) {
        return None;
    }
    let horizontal_speed = velocity * cos_angle;
    let vertical_speed = velocity * sin_angle;
    if check_collision {
        let steps = (horizontal_distance / horizontal_speed).ceil() as i32 * 2;
        let mut distance = 0.0;
        let mut previous: Option<Vec3> = None;
        for _ in 0..steps - 1 {
            distance += horizontal_distance / f64::from(steps);
            let rise = sin_angle / cos_angle * distance;
            let fall = distance.powi(2) * gravity / (velocity_squared * 2.0 * cos_squared);
            let point = Vec3::new(
                position.x + distance * cos_yaw,
                position.y + (rise - fall),
                position.z + distance * sin_yaw,
            );
            if let Some(previous) = previous
                && !is_clear_transition(world, dimensions, previous, point)
            {
                return None;
            }
            previous = Some(point);
        }
    }
    Some(
        Vec3::new(
            horizontal_speed * cos_yaw,
            vertical_speed,
            horizontal_speed * sin_yaw,
        )
        .scale(f64::from(JUMP_SPEED_SCALE)),
    )
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LongJumpMidJump {
    time_between_long_jumps: UniformInt,
    landing_sound: &'static str,
}

impl LongJumpMidJump {
    const CONDITION: [Condition; 2] = [
        (MemoryModuleType::LookTarget, MemoryStatus::Registered),
        (
            MemoryModuleType::LongJumpMidJump,
            MemoryStatus::ValuePresent,
        ),
    ];

    pub fn new(time_between_long_jumps: UniformInt, landing_sound: &'static str) -> Self {
        Self {
            time_between_long_jumps,
            landing_sound,
        }
    }
}

impl Behavior for LongJumpMidJump {
    fn entry_condition(&self) -> &[Condition] {
        &Self::CONDITION
    }

    fn can_still_use<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        !cx.ai.mob.on_ground
    }

    fn start<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        cx.ai.body.discard_friction = true;
        cx.ai.body.pose = Pose::LongJumping;
    }

    fn stop<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        if cx.ai.mob.on_ground {
            cx.ai.body.velocity =
                cx.ai
                    .body
                    .velocity
                    .multiply(LANDING_DAMPING, 1.0, LANDING_DAMPING);
            cx.ai.act(MobAction::Sound {
                sound: self.landing_sound,
                volume: LANDING_VOLUME,
                pitch: 1.0,
            });
        }
        cx.ai.body.discard_friction = false;
        cx.ai.body.pose = Pose::Standing;
        cx.brain.memories.erase(MemoryModuleType::LongJumpMidJump);
        let cooldown = self.time_between_long_jumps.sample(&mut cx.ai.brain.random);
        cx.brain.memories.set(
            MemoryModuleType::LongJumpCooldownTicks,
            MemoryValue::Int(cooldown),
        );
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PossibleJump {
    target_pos: BlockPos,
    weight: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JumpSound {
    Fixed(&'static str),
    ByVariant {
        regular: &'static str,
        screaming: &'static str,
    },
}

impl JumpSound {
    fn pick(self, screaming: bool) -> &'static str {
        match self {
            Self::Fixed(sound) => sound,
            Self::ByVariant {
                regular,
                screaming: loud,
            } => {
                if screaming {
                    loud
                } else {
                    regular
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LandingSpot {
    Default,
}

impl LandingSpot {
    fn accepts<W: AiWorld>(self, cx: &BehaviorCx<'_, '_, W>, target: BlockPos) -> bool {
        match self {
            Self::Default => default_acceptable_landing_spot(cx, target),
        }
    }
}

pub fn default_acceptable_landing_spot<W: AiWorld>(
    cx: &BehaviorCx<'_, '_, W>,
    target: BlockPos,
) -> bool {
    let solid = cx
        .ai
        .world
        .state(target.offset(0, -1, 0))
        .is_some_and(|state| state.flags().contains(StateFlags::SOLID_RENDER));
    if !solid {
        return false;
    }
    let path_type = crate::walk::mob_path_type(
        cx.ai.world,
        &cx.ai.brain.profile,
        cx.ai.tick_state().state,
        target,
    );
    cx.ai.brain.profile.malus(path_type) == 0.0
}

#[derive(Debug, Clone, PartialEq)]
pub struct LongJumpToRandomPos {
    time_between_long_jumps: UniformInt,
    max_long_jump_height: i32,
    max_long_jump_width: i32,
    max_jump_velocity_multiplier: f32,
    jump_sound: JumpSound,
    acceptable_landing_spot: LandingSpot,
    jump_candidates: Vec<PossibleJump>,
    initial_position: Option<Vec3>,
    chosen_jump: Option<Vec3>,
    find_jump_tries: i32,
    prepare_jump_start: i64,
}

impl LongJumpToRandomPos {
    const CONDITION: [Condition; 3] = [
        (MemoryModuleType::LookTarget, MemoryStatus::Registered),
        (
            MemoryModuleType::LongJumpCooldownTicks,
            MemoryStatus::ValueAbsent,
        ),
        (MemoryModuleType::LongJumpMidJump, MemoryStatus::ValueAbsent),
    ];

    pub fn new(
        time_between_long_jumps: UniformInt,
        max_long_jump_height: i32,
        max_long_jump_width: i32,
        max_jump_velocity_multiplier: f32,
        jump_sound: JumpSound,
    ) -> Self {
        Self {
            time_between_long_jumps,
            max_long_jump_height,
            max_long_jump_width,
            max_jump_velocity_multiplier,
            jump_sound,
            acceptable_landing_spot: LandingSpot::Default,
            jump_candidates: Vec::new(),
            initial_position: None,
            chosen_jump: None,
            find_jump_tries: 0,
            prepare_jump_start: 0,
        }
    }

    fn set_cooldown_to_half<W: AiWorld>(&self, cx: &mut BehaviorCx<'_, '_, W>) {
        let cooldown = self.time_between_long_jumps.sample(&mut cx.ai.brain.random) / 2;
        cx.brain.memories.set(
            MemoryModuleType::LongJumpCooldownTicks,
            MemoryValue::Int(cooldown),
        );
    }

    fn jump_candidate<W: AiWorld>(
        &mut self,
        cx: &mut BehaviorCx<'_, '_, W>,
    ) -> Option<PossibleJump> {
        let total: i32 = self
            .jump_candidates
            .iter()
            .map(|candidate| candidate.weight)
            .sum();
        if total <= 0 {
            return None;
        }
        let mut roll = cx.ai.brain.random.next_int(total);
        let mut picked = None;
        for (index, candidate) in self.jump_candidates.iter().enumerate() {
            roll -= candidate.weight;
            if roll < 0 {
                picked = Some(index);
                break;
            }
        }
        picked.map(|index| self.jump_candidates.remove(index))
    }

    fn is_acceptable_landing_position<W: AiWorld>(
        &self,
        cx: &BehaviorCx<'_, '_, W>,
        target: BlockPos,
    ) -> bool {
        let here = cx.ai.mob.block_pos();
        if here.x == target.x && here.z == target.z {
            return false;
        }
        self.acceptable_landing_spot.accepts(cx, target)
    }

    fn dimensions<W: AiWorld>(cx: &BehaviorCx<'_, '_, W>) -> JumpDimensions {
        JumpDimensions::of(dimensions_in_pose(
            cx.ai.mob.kind,
            cx.ai.mob.baby,
            Pose::LongJumping,
        ))
    }

    fn calculate_optimal_jump_vector<W: AiWorld>(
        &self,
        cx: &mut BehaviorCx<'_, '_, W>,
        target: Vec3,
    ) -> Option<Vec3> {
        let mut angles = ALLOWED_ANGLES;
        for index in (1..angles.len()).rev() {
            let other = cx.ai.brain.random.next_int(index as i32 + 1) as usize;
            angles.swap(index, other);
        }
        let max_velocity =
            (cx.ai.mob.jump_strength * f64::from(self.max_jump_velocity_multiplier)) as f32;
        let body = JumpBody {
            position: cx.ai.mob.position,
            gravity: cx.ai.mob.gravity,
            dimensions: Self::dimensions(cx),
        };
        angles.into_iter().find_map(|angle| {
            calculate_jump_vector_for_angle(cx.ai.world, body, target, max_velocity, angle, true)
        })
    }

    fn pick_candidate<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, now: i64) {
        while !self.jump_candidates.is_empty() {
            let Some(candidate) = self.jump_candidate(cx) else {
                continue;
            };
            let target = candidate.target_pos;
            if !self.is_acceptable_landing_position(cx, target) {
                continue;
            }
            let Some(jump) = self.calculate_optimal_jump_vector(cx, target.center()) else {
                continue;
            };
            cx.brain.memories.set(
                MemoryModuleType::LookTarget,
                MemoryValue::Tracker(PositionTracker::of_block(target)),
            );
            let path: Option<Path> =
                cx.ai
                    .create_path_to_limited(target, 0, MIN_PATHFIND_DISTANCE_TO_VALID_JUMP as f32);
            if path.is_none_or(|path| !path.can_reach()) {
                self.chosen_jump = Some(jump);
                self.prepare_jump_start = now;
                return;
            }
        }
    }
}

impl Behavior for LongJumpToRandomPos {
    fn entry_condition(&self) -> &[Condition] {
        &Self::CONDITION
    }

    fn check_extra_start_conditions<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>) -> bool {
        let mob = cx.ai.mob;
        let on_honey = cx
            .ai
            .world
            .state(mob.block_pos())
            .is_some_and(|state| state.kind() == Some(BlockKind::HoneyBlock));
        let ready = mob.on_ground && !mob.fluid.in_water && !mob.fluid.in_lava() && !on_honey;
        if !ready {
            self.set_cooldown_to_half(cx);
        }
        ready
    }

    fn can_still_use<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        let mob = cx.ai.mob;
        let still = self.initial_position == Some(mob.position)
            && self.find_jump_tries > 0
            && !mob.fluid.in_water
            && (self.chosen_jump.is_some() || !self.jump_candidates.is_empty());
        if !still
            && !cx
                .brain
                .memories
                .has_value(MemoryModuleType::LongJumpMidJump)
        {
            self.set_cooldown_to_half(cx);
            cx.brain.memories.erase(MemoryModuleType::LookTarget);
        }
        still
    }

    fn start<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        self.chosen_jump = None;
        self.find_jump_tries = FIND_JUMP_TRIES;
        self.initial_position = Some(cx.ai.mob.position);
        let here = cx.ai.mob.block_pos();
        let (width, height) = (self.max_long_jump_width, self.max_long_jump_height);
        self.jump_candidates.clear();
        for z in here.z - width..=here.z + width {
            for y in here.y - height..=here.y + height {
                for x in here.x - width..=here.x + width {
                    let pos = BlockPos::new(x, y, z);
                    if pos != here {
                        self.jump_candidates.push(PossibleJump {
                            target_pos: pos,
                            weight: dist_sqr(here, pos).ceil() as i32,
                        });
                    }
                }
            }
        }
    }

    fn tick<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, now: i64) {
        let Some(jump) = self.chosen_jump else {
            self.find_jump_tries -= 1;
            self.pick_candidate(cx, now);
            return;
        };
        if now - self.prepare_jump_start < PREPARE_JUMP_DURATION {
            return;
        }
        cx.ai.body.yaw = cx.ai.body.body_yaw;
        cx.ai.body.discard_friction = true;
        let length = jump.length();
        let power = length + f64::from(cx.ai.mob.jump_boost_power);
        cx.ai.body.velocity = jump.scale(power / length);
        cx.brain
            .memories
            .set(MemoryModuleType::LongJumpMidJump, MemoryValue::Bool(true));
        let sound = self.jump_sound.pick(cx.ai.mob.species.goat.screaming);
        cx.ai.act(MobAction::Sound {
            sound,
            volume: JUMP_VOLUME,
            pitch: 1.0,
        });
    }
}
