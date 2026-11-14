use rspigot_blocks::{BlockKind, StateFlags};
use rspigot_domain::entity::Pose;
use rspigot_domain::entity::raycast::clip_blocks;
use rspigot_domain::{BlockPos, EntityId, Vec3, mth};

use crate::action::MobAction;
use crate::behavior::{
    Activity, ActivityData, Behavior, BehaviorBrain, BehaviorCx, Condition, DoNothing, Gate,
    Memories, MemoryModuleType, MemoryStatus, MemoryValue, MobBehavior, MobControl, MobTrigger,
    MoveToTargetSink, NearestLivingEntitySensor, RandomStroll, Sensor, SensorKind, StartAttacking,
    StopAttackCondition, StopAttackingIfTargetInvalid, TargetErased, TargetFinder, WalkTarget,
    get_entity, is_entity_attackable,
};
use crate::brain::AiContext;
use crate::random::RandomSource;
use crate::random_pos;
use crate::sensing::clip_misses;
use crate::world::{AiWorld, Living};

pub const SHOOT_INITIAL_DELAY_TICKS: i32 = 15;
pub const SHOOT_RECOVER_DELAY_TICKS: i32 = 4;
pub const SHOOT_COOLDOWN_TICKS: i32 = 10;
pub const INHALING_DURATION_TICKS: i32 = 10;
pub const SHOOT_DURATION: i32 = SHOOT_INITIAL_DELAY_TICKS + 1 + SHOOT_RECOVER_DELAY_TICKS;
pub const LONG_JUMP_DURATION: i32 = 200;
pub const ALLOWED_ANGLES: [i32; 5] = [40, 55, 60, 75, 80];
pub const SHOOT_RANGE_SQR: f64 = 256.0;
pub const SLIDE_SPEED: f32 = 0.6;
pub const STUCK_SHOOT_TICKS: i64 = 60;
pub const LANDED_SHOOT_TICKS: i64 = 100;
pub const LANDED_COOLDOWN_TICKS: i64 = 10;
pub const LANDED_COOLDOWN_TICKS_HURT: i64 = 2;
pub const ATTACKABLE_MEMORY_TICKS: i32 = 100;
pub const SHOOT_SOUND_VOLUME: f32 = 1.5;
const JUMP_RANGE_FACTOR: f32 = 0.058_333_334;
const MIN_LINE_OF_SIGHT_RANGE: f64 = 50.0;
const TOO_CLOSE_FOR_JUMP: f32 = 4.0;
const SURFACE_PROBE: f64 = 10.0;
const INHALE_SOUND: &str = "minecraft:entity.breeze.inhale";
const SHOOT_SOUND: &str = "minecraft:entity.breeze.shoot";
const SLIDE_SOUND: &str = "minecraft:entity.breeze.slide";
const CHARGE_SOUND: &str = "minecraft:entity.breeze.charge";
const JUMP_SOUND: &str = "minecraft:entity.breeze.jump";
const LAND_SOUND: &str = "minecraft:entity.breeze.land";
const PASSENGER_TARGET_HEIGHT: f64 = 0.8;
const GROUND_TARGET_HEIGHT: f64 = 0.3;
const INNER_CIRCLE_RADIUS: f64 = 4.0;
const INNER_CIRCLE_HEIGHT: f64 = 10.0;
const FIRING_Y_BODY_OFFSET: f64 = 0.300_000_011_920_928_96;

struct DomainView<'a, V>(&'a V);

impl<V: crate::BlockView> rspigot_domain::entity::BlockView for DomainView<'_, V> {
    fn block(&self, pos: BlockPos) -> Option<rspigot_blocks::StateId> {
        self.0.state(pos)
    }
}

fn bottom_center(pos: BlockPos) -> Vec3 {
    Vec3::new(
        f64::from(pos.x) + 0.5,
        f64::from(pos.y),
        f64::from(pos.z) + 0.5,
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BreezeFacts {
    pub attack_target: Option<EntityId>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BreezeAction {
    Shoot { direction: Vec3 },
}

pub fn firing_y_position(mob_y: f64, height: f32) -> f64 {
    mob_y + f64::from(height / 2.0) + FIRING_Y_BODY_OFFSET
}

pub fn within_inner_circle_range(mob_block: BlockPos, target: Vec3) -> bool {
    let center = mob_block.center();
    let (dx, dy, dz) = (
        center.x - target.x,
        center.y - target.y,
        center.z - target.z,
    );
    (dx * dx + dz * dz).sqrt() < INNER_CIRCLE_RADIUS && dy.abs() < INNER_CIRCLE_HEIGHT
}

pub fn look_at_eyes<W: AiWorld>(cx: &mut BehaviorCx<'_, '_, W>, at: Vec3) {
    let eyes = cx.ai.mob.eye_position();
    let (dx, dy, dz) = (at.x - eyes.x, at.y - eyes.y, at.z - eyes.z);
    let horizontal = (dx * dx + dz * dz).sqrt();
    let pitch = mth::wrap_degrees((-(mth::atan2(dy, horizontal) * mth::RAD_TO_DEG)) as f32);
    let yaw = mth::wrap_degrees((mth::atan2(dz, dx) * mth::RAD_TO_DEG) as f32 - 90.0);
    cx.ai.body.pitch = pitch.clamp(-90.0, 90.0);
    cx.ai.body.yaw = yaw;
    cx.ai.body.head_yaw = yaw;
}

pub fn random_point_behind_target(target: &Living, random: &mut impl RandomSource) -> Vec3 {
    let gaussian = random.next_gaussian();
    let yaw = target.facts.head_yaw + 180.0 + (gaussian as f32 * 90.0) / 2.0;
    let blend = random.next_float();
    let distance = 4.0 + blend * (8.0 - 4.0);
    let direction = mth::view_vector(0.0, yaw);
    target.position + direction.scale(f64::from(distance))
}

fn max_line_of_sight_range(follow_range: f64) -> f64 {
    MIN_LINE_OF_SIGHT_RANGE.max(follow_range)
}

pub fn has_line_of_sight<W: AiWorld>(cx: &BehaviorCx<'_, '_, W>, target: Vec3) -> bool {
    let from = cx.ai.mob.position;
    if target.distance_squared(from).sqrt() > max_line_of_sight_range(cx.ai.mob.follow_range) {
        return false;
    }
    clip_misses(cx.ai.world, from, target)
}

fn breeze_target<W: AiWorld>(cx: &BehaviorCx<'_, '_, W>) -> Option<Living> {
    get_entity(cx, MemoryModuleType::AttackTarget).and_then(|id| cx.ai.world.living(id))
}

fn set_unit<W: AiWorld>(cx: &mut BehaviorCx<'_, '_, W>, memory: MemoryModuleType, ticks: i64) {
    cx.brain
        .memories
        .set_with_expiry(memory, MemoryValue::Unit, ticks);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Shoot;

impl Shoot {
    const CONDITION: [Condition; 7] = [
        (MemoryModuleType::AttackTarget, MemoryStatus::ValuePresent),
        (
            MemoryModuleType::BreezeShootCooldown,
            MemoryStatus::ValueAbsent,
        ),
        (
            MemoryModuleType::BreezeShootCharging,
            MemoryStatus::ValueAbsent,
        ),
        (
            MemoryModuleType::BreezeShootRecovering,
            MemoryStatus::ValueAbsent,
        ),
        (MemoryModuleType::BreezeShoot, MemoryStatus::ValuePresent),
        (MemoryModuleType::WalkTarget, MemoryStatus::ValueAbsent),
        (
            MemoryModuleType::BreezeJumpTarget,
            MemoryStatus::ValueAbsent,
        ),
    ];

    fn target_within_range<W: AiWorld>(cx: &BehaviorCx<'_, '_, W>, target: &Living) -> bool {
        cx.ai.mob.position.distance_squared(target.position) < SHOOT_RANGE_SQR
    }
}

impl Behavior for Shoot {
    fn entry_condition(&self) -> &[Condition] {
        &Self::CONDITION
    }

    fn check_extra_start_conditions<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>) -> bool {
        if cx.ai.body.pose != Pose::Standing {
            return false;
        }
        let Some(target) = breeze_target(cx) else {
            return false;
        };
        let within = Self::target_within_range(cx, &target);
        if !within {
            cx.brain.memories.erase(MemoryModuleType::BreezeShoot);
        }
        within
    }

    fn can_still_use<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        cx.brain.memories.has_value(MemoryModuleType::AttackTarget)
            && cx.brain.memories.has_value(MemoryModuleType::BreezeShoot)
    }

    fn start<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        if cx.brain.memories.has_value(MemoryModuleType::AttackTarget) {
            cx.ai.body.pose = Pose::Shooting;
        }
        set_unit(
            cx,
            MemoryModuleType::BreezeShootCharging,
            i64::from(SHOOT_INITIAL_DELAY_TICKS),
        );
        cx.ai.act(MobAction::Sound {
            sound: INHALE_SOUND,
            volume: 1.0,
            pitch: 1.0,
        });
    }

    fn tick<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        let Some(target) = breeze_target(cx) else {
            return;
        };
        look_at_eyes(cx, target.position);
        if cx
            .brain
            .memories
            .has_value(MemoryModuleType::BreezeShootCharging)
            || cx
                .brain
                .memories
                .has_value(MemoryModuleType::BreezeShootRecovering)
        {
            return;
        }
        set_unit(
            cx,
            MemoryModuleType::BreezeShootRecovering,
            i64::from(SHOOT_RECOVER_DELAY_TICKS),
        );
        let mob = cx.ai.mob;
        let height = if target.facts.passenger {
            PASSENGER_TARGET_HEIGHT
        } else {
            GROUND_TARGET_HEIGHT
        };
        let target_height = target.bounds.max_y - target.bounds.min_y;
        let direction = Vec3::new(
            target.position.x - mob.position.x,
            target.position.y + target_height * height
                - firing_y_position(mob.position.y, mob.height),
            target.position.z - mob.position.z,
        );
        cx.ai
            .act(MobAction::Breeze(BreezeAction::Shoot { direction }));
        cx.ai.act(MobAction::Sound {
            sound: SHOOT_SOUND,
            volume: SHOOT_SOUND_VOLUME,
            pitch: 1.0,
        });
    }

    fn stop<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        if cx.ai.body.pose == Pose::Shooting {
            cx.ai.body.pose = Pose::Standing;
        }
        set_unit(
            cx,
            MemoryModuleType::BreezeShootCooldown,
            i64::from(SHOOT_COOLDOWN_TICKS),
        );
        cx.brain.memories.erase(MemoryModuleType::BreezeShoot);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ShootWhenStuck;

impl ShootWhenStuck {
    const CONDITION: [Condition; 5] = [
        (MemoryModuleType::AttackTarget, MemoryStatus::ValuePresent),
        (
            MemoryModuleType::BreezeJumpInhaling,
            MemoryStatus::ValueAbsent,
        ),
        (
            MemoryModuleType::BreezeJumpTarget,
            MemoryStatus::ValueAbsent,
        ),
        (MemoryModuleType::WalkTarget, MemoryStatus::ValueAbsent),
        (MemoryModuleType::BreezeShoot, MemoryStatus::ValueAbsent),
    ];
}

impl Behavior for ShootWhenStuck {
    fn entry_condition(&self) -> &[Condition] {
        &Self::CONDITION
    }

    fn check_extra_start_conditions<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>) -> bool {
        cx.ai.mob.passenger || cx.ai.mob.fluid.in_water || cx.ai.mob.levitating
    }

    fn start<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        set_unit(cx, MemoryModuleType::BreezeShoot, STUCK_SHOOT_TICKS);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Slide;

impl Slide {
    const CONDITION: [Condition; 4] = [
        (MemoryModuleType::AttackTarget, MemoryStatus::ValuePresent),
        (MemoryModuleType::WalkTarget, MemoryStatus::ValueAbsent),
        (
            MemoryModuleType::BreezeJumpCooldown,
            MemoryStatus::ValueAbsent,
        ),
        (MemoryModuleType::BreezeShoot, MemoryStatus::ValueAbsent),
    ];

    fn random_point_in_middle_circle<W: AiWorld>(
        cx: &mut BehaviorCx<'_, '_, W>,
        enemy: &Living,
    ) -> Vec3 {
        let delta = enemy.position - cx.ai.mob.position;
        let length = delta.length();
        let blend = cx.ai.brain.random.next_double();
        let distance = length - (8.0 + blend * (4.0 - 8.0));
        let scaled = delta.normalize().scale(distance);
        cx.ai.mob.position + scaled
    }
}

impl Behavior for Slide {
    fn entry_condition(&self) -> &[Condition] {
        &Self::CONDITION
    }

    fn check_extra_start_conditions<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>) -> bool {
        cx.ai.mob.on_ground && !cx.ai.mob.fluid.in_water && cx.ai.body.pose == Pose::Standing
    }

    fn start<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        let Some(enemy) = breeze_target(cx) else {
            return;
        };
        let mut chosen = None;
        if within_inner_circle_range(cx.ai.mob.block_pos(), enemy.position)
            && let Some(away) = random_pos::pos_away(cx.ai, 5, 5, enemy.position)
            && has_line_of_sight(cx, away)
            && enemy.position.distance_squared(cx.ai.mob.position)
                < enemy.position.distance_squared(away)
        {
            chosen = Some(away);
        }
        let destination = match chosen {
            Some(destination) => destination,
            None => {
                if cx.ai.brain.random.next_boolean() {
                    random_point_behind_target(&enemy, &mut cx.ai.brain.random)
                } else {
                    Self::random_point_in_middle_circle(cx, &enemy)
                }
            }
        };
        cx.brain.memories.set(
            MemoryModuleType::WalkTarget,
            MemoryValue::WalkTarget(WalkTarget::position(destination, SLIDE_SPEED, 1)),
        );
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SlideToTargetSink {
    inner: MoveToTargetSink,
}

impl SlideToTargetSink {
    pub fn new() -> Self {
        Self {
            inner: MoveToTargetSink::new(),
        }
    }
}

impl Default for SlideToTargetSink {
    fn default() -> Self {
        Self::new()
    }
}

impl Behavior for SlideToTargetSink {
    fn entry_condition(&self) -> &[Condition] {
        self.inner.entry_condition()
    }

    fn check_extra_start_conditions<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>) -> bool {
        self.inner.check_extra_start_conditions(cx)
    }

    fn start<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, now: i64) {
        self.inner.start(cx, now);
        cx.ai.act(MobAction::Sound {
            sound: SLIDE_SOUND,
            volume: 1.0,
            pitch: 1.0,
        });
        cx.ai.body.pose = Pose::Sliding;
    }

    fn can_still_use<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, now: i64) -> bool {
        self.inner.can_still_use(cx, now)
    }

    fn tick<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, now: i64) {
        self.inner.tick(cx, now);
    }

    fn stop<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, now: i64) {
        self.inner.stop(cx, now);
        cx.ai.body.pose = Pose::Standing;
        if cx.brain.memories.has_value(MemoryModuleType::AttackTarget) {
            set_unit(cx, MemoryModuleType::BreezeShoot, STUCK_SHOOT_TICKS);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LongJump;

impl LongJump {
    const CONDITION: [Condition; 7] = [
        (MemoryModuleType::AttackTarget, MemoryStatus::ValuePresent),
        (
            MemoryModuleType::BreezeJumpCooldown,
            MemoryStatus::ValueAbsent,
        ),
        (
            MemoryModuleType::BreezeJumpInhaling,
            MemoryStatus::Registered,
        ),
        (MemoryModuleType::BreezeJumpTarget, MemoryStatus::Registered),
        (MemoryModuleType::BreezeShoot, MemoryStatus::ValueAbsent),
        (MemoryModuleType::WalkTarget, MemoryStatus::ValueAbsent),
        (
            MemoryModuleType::BreezeLeavingWater,
            MemoryStatus::Registered,
        ),
    ];

    fn is_air_or_water<W: AiWorld>(cx: &BehaviorCx<'_, '_, W>, pos: BlockPos) -> bool {
        cx.ai
            .world
            .state(pos)
            .is_some_and(|state| state.is_air() || state.flags().contains(StateFlags::WATER))
    }

    fn can_jump_from_current_position<W: AiWorld>(cx: &BehaviorCx<'_, '_, W>) -> bool {
        let here = cx.ai.mob.block_pos();
        if cx
            .ai
            .world
            .state(here)
            .is_some_and(|state| state.kind() == Some(BlockKind::HoneyBlock))
        {
            return false;
        }
        (1..=4).all(|offset| Self::is_air_or_water(cx, here.offset(0, offset, 0)))
    }

    fn snap_to_surface<W: AiWorld>(cx: &BehaviorCx<'_, '_, W>, target: Vec3) -> Option<BlockPos> {
        let down = Vec3::new(target.x, target.y - SURFACE_PROBE, target.z);
        if let Some(hit) = clip_blocks(&DomainView(cx.ai.world), target, down) {
            return Some(hit.location.block().offset(0, 1, 0));
        }
        let up = Vec3::new(target.x, target.y + SURFACE_PROBE, target.z);
        clip_blocks(&DomainView(cx.ai.world), target, up)
            .map(|hit| hit.location.block().offset(0, 1, 0))
    }

    fn can_run<W: AiWorld>(cx: &mut BehaviorCx<'_, '_, W>) -> bool {
        let mob = cx.ai.mob;
        if !mob.on_ground && !mob.fluid.in_water {
            return false;
        }
        if mob.fluid.water_height > f64::from(mob.eye_height) || mob.fluid.in_lava() {
            return false;
        }
        if cx
            .brain
            .memories
            .has_value(MemoryModuleType::BreezeJumpTarget)
        {
            return true;
        }
        let Some(target) = breeze_target(cx) else {
            return false;
        };
        let follow = mob.follow_range;
        if mob.position.distance_squared(target.position) >= follow * follow {
            cx.brain.memories.erase(MemoryModuleType::AttackTarget);
            return false;
        }
        let distance = mob.position.distance_squared(target.position).sqrt() as f32;
        if distance - TOO_CLOSE_FOR_JUMP <= 0.0 {
            return false;
        }
        if !Self::can_jump_from_current_position(cx) {
            return false;
        }
        let point = random_point_behind_target(&target, &mut cx.ai.brain.random);
        let Some(landing) = Self::snap_to_surface(cx, point) else {
            return false;
        };
        let dangerous = cx
            .ai
            .world
            .state(landing.offset(0, -1, 0))
            .is_some_and(|state| {
                rspigot_domain::spawn::placement::is_block_dangerous(cx.ai.mob.kind, state)
            });
        if dangerous {
            return false;
        }
        if !has_line_of_sight(cx, landing.center())
            && !has_line_of_sight(cx, landing.offset(0, 4, 0).center())
        {
            return false;
        }
        cx.brain.memories.set(
            MemoryModuleType::BreezeJumpTarget,
            MemoryValue::Block(landing),
        );
        true
    }

    fn is_finished_inhaling<W: AiWorld>(cx: &BehaviorCx<'_, '_, W>) -> bool {
        !cx.brain
            .memories
            .has_value(MemoryModuleType::BreezeJumpInhaling)
            && cx.ai.body.pose == Pose::Inhaling
    }

    fn is_finished_jumping<W: AiWorld>(cx: &BehaviorCx<'_, '_, W>) -> bool {
        let leaving_water = cx.ai.mob.fluid.in_water
            && cx.brain.memories.check(
                MemoryModuleType::BreezeLeavingWater,
                MemoryStatus::ValueAbsent,
            );
        cx.ai.body.pose == Pose::LongJumping && (cx.ai.mob.on_ground || leaving_water)
    }

    fn calculate_optimal_jump_vector<W: AiWorld>(
        cx: &mut BehaviorCx<'_, '_, W>,
        target: Vec3,
    ) -> Option<Vec3> {
        let mut angles = ALLOWED_ANGLES;
        for index in (1..angles.len()).rev() {
            let other = cx.ai.brain.random.next_int(index as i32 + 1) as usize;
            angles.swap(index, other);
        }
        let mob = cx.ai.mob;
        let max_velocity = (mob.follow_range as f32) * JUMP_RANGE_FACTOR;
        let body = crate::behavior::JumpBody {
            position: mob.position,
            gravity: mob.gravity,
            dimensions: crate::behavior::JumpDimensions::of(rspigot_entities::EntityDimensions {
                width: mob.width,
                height: mob.height,
                eye_height: mob.eye_height,
            }),
        };
        let vector = angles.into_iter().find_map(|angle| {
            crate::behavior::calculate_jump_vector_for_angle(
                cx.ai.world,
                body,
                target,
                max_velocity,
                angle,
                false,
            )
        })?;
        if mob.jump_boost_power > 0.0 {
            let boost = vector.normalize().y * f64::from(mob.jump_boost_power);
            return Some(vector + Vec3::new(0.0, boost, 0.0));
        }
        Some(vector)
    }
}

impl Behavior for LongJump {
    fn entry_condition(&self) -> &[Condition] {
        &Self::CONDITION
    }

    fn check_extra_start_conditions<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>) -> bool {
        Self::can_run(cx)
    }

    fn can_still_use<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        cx.ai.body.pose != Pose::Standing
            && !cx
                .brain
                .memories
                .has_value(MemoryModuleType::BreezeJumpCooldown)
    }

    fn start<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        if cx.brain.memories.check(
            MemoryModuleType::BreezeJumpInhaling,
            MemoryStatus::ValueAbsent,
        ) {
            set_unit(
                cx,
                MemoryModuleType::BreezeJumpInhaling,
                i64::from(INHALING_DURATION_TICKS),
            );
        }
        cx.ai.body.pose = Pose::Inhaling;
        cx.ai.act(MobAction::Sound {
            sound: CHARGE_SOUND,
            volume: 1.0,
            pitch: 1.0,
        });
        if let Some(target) = cx
            .brain
            .memories
            .get(MemoryModuleType::BreezeJumpTarget)
            .and_then(MemoryValue::as_block)
        {
            look_at_eyes(cx, target.center());
        }
    }

    fn tick<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        let in_water = cx.ai.mob.fluid.in_water;
        if !in_water
            && cx.brain.memories.check(
                MemoryModuleType::BreezeLeavingWater,
                MemoryStatus::ValuePresent,
            )
        {
            cx.brain
                .memories
                .erase(MemoryModuleType::BreezeLeavingWater);
        }
        if Self::is_finished_inhaling(cx) {
            let target = cx
                .brain
                .memories
                .get(MemoryModuleType::BreezeJumpTarget)
                .and_then(MemoryValue::as_block);
            let jump =
                target.and_then(|pos| Self::calculate_optimal_jump_vector(cx, bottom_center(pos)));
            let Some(jump) = jump else {
                cx.ai.body.pose = Pose::Standing;
                return;
            };
            if in_water {
                cx.brain
                    .memories
                    .set(MemoryModuleType::BreezeLeavingWater, MemoryValue::Unit);
            }
            cx.ai.act(MobAction::Sound {
                sound: JUMP_SOUND,
                volume: 1.0,
                pitch: 1.0,
            });
            cx.ai.body.pose = Pose::LongJumping;
            cx.ai.body.yaw = cx.ai.body.body_yaw;
            cx.ai.body.discard_friction = true;
            cx.ai.body.velocity = jump;
        } else if Self::is_finished_jumping(cx) {
            cx.ai.act(MobAction::Sound {
                sound: LAND_SOUND,
                volume: 1.0,
                pitch: 1.0,
            });
            cx.ai.body.pose = Pose::Standing;
            cx.ai.body.discard_friction = false;
            let hurt = cx.brain.memories.has_value(MemoryModuleType::HurtBy);
            let cooldown = if hurt {
                LANDED_COOLDOWN_TICKS_HURT
            } else {
                LANDED_COOLDOWN_TICKS
            };
            set_unit(cx, MemoryModuleType::BreezeJumpCooldown, cooldown);
            set_unit(cx, MemoryModuleType::BreezeShoot, LANDED_SHOOT_TICKS);
        }
    }

    fn stop<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        if matches!(cx.ai.body.pose, Pose::LongJumping | Pose::Inhaling) {
            cx.ai.body.pose = Pose::Standing;
        }
        cx.brain.memories.erase(MemoryModuleType::BreezeJumpTarget);
        cx.brain
            .memories
            .erase(MemoryModuleType::BreezeJumpInhaling);
        cx.brain
            .memories
            .erase(MemoryModuleType::BreezeLeavingWater);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BreezeAttackEntitySensor;

impl Sensor for BreezeAttackEntitySensor {
    fn requires(&self) -> &'static [MemoryModuleType] {
        &[
            MemoryModuleType::NearestLivingEntities,
            MemoryModuleType::NearestVisibleLivingEntities,
            MemoryModuleType::NearestAttackable,
        ]
    }

    fn do_tick<W: AiWorld>(&mut self, cx: &mut AiContext<'_, W>, memories: &mut Memories) {
        NearestLivingEntitySensor.do_tick(cx, memories);
        let nearby: Vec<_> = memories
            .get(MemoryModuleType::NearestLivingEntities)
            .and_then(MemoryValue::as_entities)
            .map(<[_]>::to_vec)
            .unwrap_or_default();
        let mut found = None;
        for id in nearby {
            let Some(entity) = cx.world.living(id) else {
                continue;
            };
            if entity.spectator || (entity.player && entity.creative) {
                continue;
            }
            if is_entity_attackable(cx, memories, &entity) {
                found = Some(entity.id);
                break;
            }
        }
        memories.set_option(
            MemoryModuleType::NearestAttackable,
            found.map(MemoryValue::Entity),
        );
    }
}

pub fn breeze_sensors() -> Vec<SensorKind> {
    vec![
        SensorKind::NearestLivingEntities(NearestLivingEntitySensor),
        SensorKind::HurtBy(crate::behavior::HurtBySensor),
        SensorKind::NearestPlayers(crate::behavior::PlayerSensor),
        SensorKind::BreezeAttackEntity(BreezeAttackEntitySensor),
    ]
}

fn core_activity() -> ActivityData<MobControl> {
    ActivityData::create(
        Activity::Core,
        0,
        vec![
            MobControl::swim(0.8),
            MobControl::look_at_target_sink(45, 90),
        ],
    )
}

fn idle_activity() -> ActivityData<MobControl> {
    let behaviors = vec![
        MobControl::one_shot(MobTrigger::StartAttacking(StartAttacking::new(
            TargetFinder::Memory(MemoryModuleType::NearestAttackable),
        ))),
        MobControl::one_shot(MobTrigger::StartAttacking(StartAttacking::new(
            TargetFinder::HurtBySource,
        ))),
        MobControl::behavior_for(
            MobBehavior::BreezeSlideToTarget(SlideToTargetSink::new()),
            20,
            40,
        ),
        MobControl::Gate(Gate::run_one(vec![
            (MobControl::DoNothing(DoNothing::new(20, 100)), 1),
            (
                MobControl::one_shot(MobTrigger::RandomStroll(RandomStroll::stroll(0.6))),
                2,
            ),
        ])),
    ];
    ActivityData::create(Activity::Idle, 0, behaviors)
}

fn fight_activity() -> ActivityData<MobControl> {
    let stop = StopAttackingIfTargetInvalid::new(
        StopAttackCondition::not_attackable_recently(ATTACKABLE_MEMORY_TICKS),
        TargetErased::Nothing,
        true,
    );
    let behaviors = vec![
        MobControl::one_shot(MobTrigger::StopAttackingIfTargetInvalid(stop)),
        MobControl::behavior_for(
            MobBehavior::BreezeShoot(Shoot),
            SHOOT_DURATION,
            SHOOT_DURATION,
        ),
        MobControl::behavior_for(
            MobBehavior::BreezeLongJump(LongJump),
            LONG_JUMP_DURATION,
            LONG_JUMP_DURATION,
        ),
        MobControl::behavior(MobBehavior::BreezeShootWhenStuck(ShootWhenStuck)),
        MobControl::behavior(MobBehavior::BreezeSlide(Slide)),
    ];
    ActivityData::create_when(
        Activity::Fight,
        0,
        behaviors,
        vec![
            (MemoryModuleType::AttackTarget, MemoryStatus::ValuePresent),
            (MemoryModuleType::WalkTarget, MemoryStatus::ValueAbsent),
        ],
    )
}

pub fn breeze_activities() -> Vec<ActivityData<MobControl>> {
    vec![core_activity(), idle_activity(), fight_activity()]
}

pub fn finish_brain(brain: &mut BehaviorBrain<MobControl>) {
    brain.state_mut().set_default_activity(Activity::Fight);
    brain.state_mut().use_default_activity();
}

pub fn update_activity(brain: &mut BehaviorBrain<MobControl>) {
    brain
        .state_mut()
        .set_active_activity_to_first_valid(&[Activity::Fight, Activity::Idle]);
}

pub fn ambient_sound(on_ground: bool, has_target: bool) -> Option<&'static str> {
    if has_target && on_ground {
        return None;
    }
    Some(if on_ground {
        "minecraft:entity.breeze.idle_ground"
    } else {
        "minecraft:entity.breeze.idle_air"
    })
}
