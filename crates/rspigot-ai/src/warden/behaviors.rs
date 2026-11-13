use rspigot_domain::entity::Pose;
use rspigot_domain::warden::{ROAR_ANGER_INCREASE, ROAR_DURATION};
use rspigot_domain::{BlockPos, EntityId, Vec3};

use super::state::{
    WardenAction, can_target_id, closer_than_in_plane, entity_angry_at, increase_anger_at,
    increase_anger_default, look_at_roar_target, set_attack_target, set_disturbance_location,
    sound,
};
use crate::action::MobAction;
use crate::behavior::{
    Behavior, BehaviorCx, Condition, MemoryModuleType, MemoryStatus, MemoryValue, PositionTracker,
    Trigger, UniformInt, get_entity,
};
use crate::world::AiWorld;

pub const SONIC_BOOM_TICKS_BEFORE_SOUND: i64 = 34;
pub const SONIC_BOOM_DURATION: i32 = 60;
pub const SONIC_BOOM_COOLDOWN_TICKS: i64 = 40;
pub const SONIC_BOOM_HORIZONTAL_RANGE: f64 = 15.0;
pub const SONIC_BOOM_VERTICAL_RANGE: f64 = 20.0;
pub const SNIFF_RANGE_HORIZONTAL: f64 = 6.0;
pub const SNIFF_RANGE_VERTICAL: f64 = 20.0;
pub const ROAR_SOUND_DELAY_TICKS: i64 = 25;
pub const DIG_COOLDOWN_SETTER_TICKS: i64 = 1200;
pub const CHEST_HEIGHT: f64 = 1.6;
const SNIFF_COOLDOWN: UniformInt = UniformInt::of(100, 200);
const EMERGE_SOUND: &str = "minecraft:entity.warden.emerge";
const DIG_SOUND: &str = "minecraft:entity.warden.dig";
const AGITATED_SOUND: &str = "minecraft:entity.warden.agitated";
const ROAR_SOUND: &str = "minecraft:entity.warden.roar";
const SNIFF_SOUND: &str = "minecraft:entity.warden.sniff";
const SONIC_CHARGE_SOUND: &str = "minecraft:entity.warden.sonic_charge";
const SONIC_BOOM_EVENT: i8 = 62;

pub fn set_sonic_boom_cooldown<W: AiWorld>(cx: &mut BehaviorCx<'_, '_, W>, ticks: i64) {
    cx.brain.memories.set_with_expiry(
        MemoryModuleType::SonicBoomCooldown,
        MemoryValue::Unit,
        ticks,
    );
}

fn stand_if<W: AiWorld>(cx: &mut BehaviorCx<'_, '_, W>, pose: Pose) {
    if cx.ai.body.pose == pose {
        cx.ai.body.pose = Pose::Standing;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Emerging;

impl Emerging {
    const CONDITION: [Condition; 3] = [
        (MemoryModuleType::IsEmerging, MemoryStatus::ValuePresent),
        (MemoryModuleType::WalkTarget, MemoryStatus::ValueAbsent),
        (MemoryModuleType::LookTarget, MemoryStatus::Registered),
    ];
}

impl Behavior for Emerging {
    fn entry_condition(&self) -> &[Condition] {
        &Self::CONDITION
    }

    fn can_still_use<W: AiWorld>(&mut self, _cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        true
    }

    fn start<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        cx.ai.body.pose = Pose::Emerging;
        sound(cx, EMERGE_SOUND, 5.0, 1.0);
    }

    fn stop<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        stand_if(cx, Pose::Emerging);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Digging {
    discarded: bool,
}

impl Digging {
    const CONDITION: [Condition; 2] = [
        (MemoryModuleType::AttackTarget, MemoryStatus::ValueAbsent),
        (MemoryModuleType::WalkTarget, MemoryStatus::ValueAbsent),
    ];

    fn discard<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>) {
        if !self.discarded {
            self.discarded = true;
            cx.ai.act(MobAction::Warden(WardenAction::Discard));
        }
    }
}

impl Behavior for Digging {
    fn entry_condition(&self) -> &[Condition] {
        &Self::CONDITION
    }

    fn check_extra_start_conditions<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>) -> bool {
        cx.ai.mob.on_ground || cx.ai.mob.in_liquid()
    }

    fn can_still_use<W: AiWorld>(&mut self, _cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        !self.discarded
    }

    fn start<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        self.discarded = false;
        if cx.ai.mob.on_ground {
            cx.ai.body.pose = Pose::Digging;
            sound(cx, DIG_SOUND, 5.0, 1.0);
        } else {
            sound(cx, AGITATED_SOUND, 5.0, 1.0);
            self.discard(cx);
        }
    }

    fn stop<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        self.discard(cx);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ForceUnmount;

impl Behavior for ForceUnmount {
    fn entry_condition(&self) -> &[Condition] {
        &[]
    }

    fn check_extra_start_conditions<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>) -> bool {
        cx.ai.mob.passenger
    }

    fn start<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        cx.ai.act(MobAction::Warden(WardenAction::Unmount));
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Roar;

impl Roar {
    const CONDITION: [Condition; 4] = [
        (MemoryModuleType::RoarTarget, MemoryStatus::ValuePresent),
        (MemoryModuleType::AttackTarget, MemoryStatus::ValueAbsent),
        (
            MemoryModuleType::RoarSoundCooldown,
            MemoryStatus::Registered,
        ),
        (MemoryModuleType::RoarSoundDelay, MemoryStatus::Registered),
    ];
}

impl Behavior for Roar {
    fn entry_condition(&self) -> &[Condition] {
        &Self::CONDITION
    }

    fn can_still_use<W: AiWorld>(&mut self, _cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        true
    }

    fn start<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        cx.brain.memories.set_with_expiry(
            MemoryModuleType::RoarSoundDelay,
            MemoryValue::Unit,
            ROAR_SOUND_DELAY_TICKS,
        );
        cx.brain.memories.erase(MemoryModuleType::WalkTarget);
        let target = get_entity(cx, MemoryModuleType::RoarTarget);
        if let Some(target) = target {
            look_at_roar_target(cx, target);
        }
        cx.ai.body.pose = Pose::Roaring;
        if let Some(target) = target {
            increase_anger_at(cx, target, ROAR_ANGER_INCREASE, false);
        }
    }

    fn tick<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        if cx
            .brain
            .memories
            .has_value(MemoryModuleType::RoarSoundDelay)
            || cx
                .brain
                .memories
                .has_value(MemoryModuleType::RoarSoundCooldown)
        {
            return;
        }
        cx.brain.memories.set_with_expiry(
            MemoryModuleType::RoarSoundCooldown,
            MemoryValue::Unit,
            i64::from(ROAR_DURATION) - ROAR_SOUND_DELAY_TICKS,
        );
        sound(cx, ROAR_SOUND, 3.0, 1.0);
    }

    fn stop<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        stand_if(cx, Pose::Roaring);
        if let Some(target) = get_entity(cx, MemoryModuleType::RoarTarget) {
            set_attack_target(cx, target);
        }
        cx.brain.memories.erase(MemoryModuleType::RoarTarget);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Sniffing;

impl Sniffing {
    const CONDITION: [Condition; 7] = [
        (MemoryModuleType::IsSniffing, MemoryStatus::ValuePresent),
        (MemoryModuleType::AttackTarget, MemoryStatus::ValueAbsent),
        (MemoryModuleType::WalkTarget, MemoryStatus::ValueAbsent),
        (MemoryModuleType::LookTarget, MemoryStatus::Registered),
        (
            MemoryModuleType::NearestAttackable,
            MemoryStatus::Registered,
        ),
        (
            MemoryModuleType::DisturbanceLocation,
            MemoryStatus::Registered,
        ),
        (MemoryModuleType::SniffCooldown, MemoryStatus::Registered),
    ];
}

impl Behavior for Sniffing {
    fn entry_condition(&self) -> &[Condition] {
        &Self::CONDITION
    }

    fn can_still_use<W: AiWorld>(&mut self, _cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        true
    }

    fn start<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        sound(cx, SNIFF_SOUND, 5.0, 1.0);
    }

    fn stop<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        stand_if(cx, Pose::Sniffing);
        cx.brain.memories.erase(MemoryModuleType::IsSniffing);
        let Some(found) = get_entity(cx, MemoryModuleType::NearestAttackable) else {
            return;
        };
        let Some(entity) = cx.ai.world.living(found) else {
            return;
        };
        if !can_target_id(cx, found) {
            return;
        }
        if closer_than_in_plane(
            cx.ai.mob.position,
            entity.position,
            SNIFF_RANGE_HORIZONTAL,
            SNIFF_RANGE_VERTICAL,
        ) {
            increase_anger_default(cx, found);
        }
        if !cx
            .brain
            .memories
            .has_value(MemoryModuleType::DisturbanceLocation)
        {
            set_disturbance_location(cx, entity.position.block());
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SonicBoom;

impl SonicBoom {
    const CONDITION: [Condition; 4] = [
        (MemoryModuleType::AttackTarget, MemoryStatus::ValuePresent),
        (
            MemoryModuleType::SonicBoomCooldown,
            MemoryStatus::ValueAbsent,
        ),
        (
            MemoryModuleType::SonicBoomSoundCooldown,
            MemoryStatus::Registered,
        ),
        (
            MemoryModuleType::SonicBoomSoundDelay,
            MemoryStatus::Registered,
        ),
    ];

    fn in_range<W: AiWorld>(cx: &BehaviorCx<'_, '_, W>, target: EntityId) -> bool {
        cx.ai.world.living(target).is_some_and(|living| {
            closer_than_in_plane(
                cx.ai.mob.position,
                living.position,
                SONIC_BOOM_HORIZONTAL_RANGE,
                SONIC_BOOM_VERTICAL_RANGE,
            )
        })
    }

    fn boom<W: AiWorld>(cx: &mut BehaviorCx<'_, '_, W>, target: EntityId) {
        let Some(living) = cx.ai.world.living(target) else {
            return;
        };
        let origin = cx.ai.mob.position + Vec3::new(0.0, CHEST_HEIGHT, 0.0);
        let eyes = Vec3::new(living.position.x, living.eye_y(), living.position.z);
        let delta = eyes - origin;
        let length = delta.length();
        cx.ai.act(MobAction::Warden(WardenAction::SonicBoom {
            target,
            origin,
            direction: delta.normalize(),
            length,
        }));
    }
}

impl Behavior for SonicBoom {
    fn entry_condition(&self) -> &[Condition] {
        &Self::CONDITION
    }

    fn check_extra_start_conditions<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>) -> bool {
        get_entity(cx, MemoryModuleType::AttackTarget)
            .is_some_and(|target| Self::in_range(cx, target))
    }

    fn can_still_use<W: AiWorld>(&mut self, _cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        true
    }

    fn start<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        cx.brain.memories.set_with_expiry(
            MemoryModuleType::AttackCoolingDown,
            MemoryValue::Bool(true),
            i64::from(SONIC_BOOM_DURATION),
        );
        cx.brain.memories.set_with_expiry(
            MemoryModuleType::SonicBoomSoundDelay,
            MemoryValue::Unit,
            SONIC_BOOM_TICKS_BEFORE_SOUND,
        );
        cx.ai.act(MobAction::EntityEvent(SONIC_BOOM_EVENT));
        sound(cx, SONIC_CHARGE_SOUND, 3.0, 1.0);
    }

    fn tick<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        let target = get_entity(cx, MemoryModuleType::AttackTarget);
        if let Some(living) = target.and_then(|id| cx.ai.world.living(id)) {
            cx.ai.brain.look.look_at(living.position);
        }
        if cx
            .brain
            .memories
            .has_value(MemoryModuleType::SonicBoomSoundDelay)
            || cx
                .brain
                .memories
                .has_value(MemoryModuleType::SonicBoomSoundCooldown)
        {
            return;
        }
        cx.brain.memories.set_with_expiry(
            MemoryModuleType::SonicBoomSoundCooldown,
            MemoryValue::Unit,
            i64::from(SONIC_BOOM_DURATION) - SONIC_BOOM_TICKS_BEFORE_SOUND,
        );
        if let Some(target) = target
            && can_target_id(cx, target)
            && Self::in_range(cx, target)
        {
            Self::boom(cx, target);
        }
    }

    fn stop<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        set_sonic_boom_cooldown(cx, SONIC_BOOM_COOLDOWN_TICKS);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SetRoarTarget;

impl SetRoarTarget {
    pub fn conditions(&self) -> Vec<Condition> {
        vec![
            (MemoryModuleType::RoarTarget, MemoryStatus::ValueAbsent),
            (MemoryModuleType::AttackTarget, MemoryStatus::ValueAbsent),
            (
                MemoryModuleType::CantReachWalkTargetSince,
                MemoryStatus::Registered,
            ),
        ]
    }
}

impl Trigger for SetRoarTarget {
    fn trigger<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        let Some(target) = entity_angry_at(cx).filter(|id| can_target_id(cx, *id)) else {
            return false;
        };
        cx.brain
            .memories
            .set(MemoryModuleType::RoarTarget, MemoryValue::Entity(target));
        cx.brain
            .memories
            .erase(MemoryModuleType::CantReachWalkTargetSince);
        true
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TryToSniff;

impl TryToSniff {
    pub fn conditions(&self) -> Vec<Condition> {
        vec![
            (MemoryModuleType::IsSniffing, MemoryStatus::Registered),
            (MemoryModuleType::WalkTarget, MemoryStatus::Registered),
            (MemoryModuleType::SniffCooldown, MemoryStatus::ValueAbsent),
            (
                MemoryModuleType::NearestAttackable,
                MemoryStatus::ValuePresent,
            ),
            (
                MemoryModuleType::DisturbanceLocation,
                MemoryStatus::ValueAbsent,
            ),
        ]
    }
}

impl Trigger for TryToSniff {
    fn trigger<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        cx.brain
            .memories
            .set(MemoryModuleType::IsSniffing, MemoryValue::Unit);
        let cooldown = SNIFF_COOLDOWN.sample(&mut cx.ai.brain.random);
        cx.brain.memories.set_with_expiry(
            MemoryModuleType::SniffCooldown,
            MemoryValue::Unit,
            i64::from(cooldown),
        );
        cx.brain.memories.erase(MemoryModuleType::WalkTarget);
        cx.ai.body.pose = Pose::Sniffing;
        true
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SetWardenLookTarget;

impl SetWardenLookTarget {
    pub fn conditions(&self) -> Vec<Condition> {
        vec![
            (MemoryModuleType::LookTarget, MemoryStatus::Registered),
            (
                MemoryModuleType::DisturbanceLocation,
                MemoryStatus::Registered,
            ),
            (MemoryModuleType::RoarTarget, MemoryStatus::Registered),
            (MemoryModuleType::AttackTarget, MemoryStatus::ValueAbsent),
        ]
    }
}

impl Trigger for SetWardenLookTarget {
    fn trigger<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        let roar: Option<BlockPos> = get_entity(cx, MemoryModuleType::RoarTarget)
            .and_then(|id| cx.ai.world.living(id))
            .map(|living| living.position.block());
        let disturbance = cx
            .brain
            .memories
            .get(MemoryModuleType::DisturbanceLocation)
            .and_then(MemoryValue::as_block);
        let Some(pos) = roar.or(disturbance) else {
            return false;
        };
        cx.brain.memories.set(
            MemoryModuleType::LookTarget,
            MemoryValue::Tracker(PositionTracker::of_block(pos)),
        );
        true
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DigCooldownSetter;

impl DigCooldownSetter {
    pub fn conditions(&self) -> Vec<Condition> {
        vec![(MemoryModuleType::DigCooldown, MemoryStatus::Registered)]
    }
}

impl Trigger for DigCooldownSetter {
    fn trigger<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        if cx.brain.memories.has_value(MemoryModuleType::DigCooldown) {
            cx.brain.memories.set_with_expiry(
                MemoryModuleType::DigCooldown,
                MemoryValue::Unit,
                DIG_COOLDOWN_SETTER_TICKS,
            );
        }
        true
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SetLookAtAttackTarget {
    max_dist_sqr: f64,
}

impl SetLookAtAttackTarget {
    pub fn new(max_dist: f32) -> Self {
        Self {
            max_dist_sqr: f64::from(max_dist * max_dist),
        }
    }

    pub fn conditions(&self) -> Vec<Condition> {
        vec![
            (MemoryModuleType::LookTarget, MemoryStatus::ValueAbsent),
            (
                MemoryModuleType::NearestVisibleLivingEntities,
                MemoryStatus::ValuePresent,
            ),
        ]
    }
}

impl Trigger for SetLookAtAttackTarget {
    fn trigger<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        let target = get_entity(cx, MemoryModuleType::AttackTarget);
        let mob = cx.ai.mob;
        let max = self.max_dist_sqr;
        let found = cx
            .brain
            .memories
            .get(MemoryModuleType::NearestVisibleLivingEntities)
            .and_then(MemoryValue::as_visible)
            .and_then(|visible| {
                visible.find_closest(cx.ai, &cx.brain.memories, |entity| {
                    Some(entity.id) == target && mob.distance_to_sqr(entity.position) <= max
                })
            });
        let Some(found) = found else {
            return false;
        };
        crate::behavior::look_at_entity(cx, found.id);
        true
    }
}
