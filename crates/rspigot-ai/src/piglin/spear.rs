use rspigot_domain::Vec3;

use crate::behavior::{
    Behavior, BehaviorCx, Condition, MemoryModuleType, MemoryStatus, MemoryValue, get_entity,
    look_at_entity,
};
use crate::random_pos::land_pos_away_between;
use crate::spear_use::{PASSENGER_EXTRA_DISTANCE, is_spear_item, kinetic_use_duration};
use crate::world::{AiWorld, Hand, Living};

pub const SPEAR_RETREAT_DURATION: i32 = 100;
pub const MIN_REPOSITION_DISTANCE: i32 = 6;
pub const MAX_REPOSITION_DISTANCE: i32 = 7;
pub const MIN_COOLDOWN_DISTANCE: i32 = 9;
pub const MAX_COOLDOWN_DISTANCE: i32 = 11;
pub const MAX_FLEEING_TIME: i32 = 100;
const AWAY_VERTICAL: i32 = 7;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum SpearStatus {
    Approach = 0,
    Charging = 1,
    Retreat = 2,
}

impl SpearStatus {
    fn of(value: i32) -> Option<Self> {
        match value {
            0 => Some(Self::Approach),
            1 => Some(Self::Charging),
            2 => Some(Self::Retreat),
            _ => None,
        }
    }

    fn memory(self) -> MemoryValue {
        MemoryValue::Int(self as i32)
    }
}

fn status<W: AiWorld>(cx: &BehaviorCx<'_, '_, W>) -> SpearStatus {
    cx.brain
        .memories
        .get(MemoryModuleType::SpearStatus)
        .and_then(MemoryValue::as_int)
        .and_then(SpearStatus::of)
        .unwrap_or(SpearStatus::Approach)
}

fn int<W: AiWorld>(cx: &BehaviorCx<'_, '_, W>, memory: MemoryModuleType, default: i32) -> i32 {
    cx.brain
        .memories
        .get(memory)
        .and_then(MemoryValue::as_int)
        .unwrap_or(default)
}

fn target<W: AiWorld>(cx: &BehaviorCx<'_, '_, W>) -> Option<Living> {
    get_entity(cx, MemoryModuleType::AttackTarget).and_then(|id| cx.ai.world.living(id))
}

fn able_to_attack<W: AiWorld>(cx: &BehaviorCx<'_, '_, W>) -> bool {
    target(cx).is_some() && cx.ai.mob.main_hand.is_some_and(is_spear_item)
}

fn is_using_item<W: AiWorld>(cx: &BehaviorCx<'_, '_, W>) -> bool {
    cx.ai.body.using_item.is_some()
}

fn passenger_extra<W: AiWorld>(cx: &BehaviorCx<'_, '_, W>) -> i32 {
    if cx.ai.mob.passenger {
        PASSENGER_EXTRA_DISTANCE
    } else {
        0
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpearApproach {
    speed_modifier_when_repositioning: f64,
    approach_distance_sq: f32,
}

impl SpearApproach {
    const CONDITION: [Condition; 1] = [(MemoryModuleType::SpearStatus, MemoryStatus::ValueAbsent)];

    pub fn new(speed_modifier_when_repositioning: f64, approach_distance: f32) -> Self {
        Self {
            speed_modifier_when_repositioning,
            approach_distance_sq: approach_distance * approach_distance,
        }
    }

    fn far_enough<W: AiWorld>(&self, cx: &BehaviorCx<'_, '_, W>) -> bool {
        target(cx).is_some_and(|target| {
            cx.ai.mob.distance_to_sqr(target.position) > f64::from(self.approach_distance_sq)
        })
    }
}

impl Behavior for SpearApproach {
    fn entry_condition(&self) -> &[Condition] {
        &Self::CONDITION
    }

    fn check_extra_start_conditions<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>) -> bool {
        able_to_attack(cx) && !is_using_item(cx)
    }

    fn start<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        cx.ai.body.aggressive = true;
        cx.brain.memories.set(
            MemoryModuleType::SpearStatus,
            SpearStatus::Approach.memory(),
        );
    }

    fn can_still_use<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        able_to_attack(cx) && self.far_enough(cx)
    }

    fn tick<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        let Some(target) = target(cx) else {
            return;
        };
        look_at_entity(cx, target.id);
        cx.ai
            .move_to_entity(&target, self.speed_modifier_when_repositioning);
    }

    fn stop<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        cx.ai.stop_navigation();
        cx.brain.memories.set(
            MemoryModuleType::SpearStatus,
            SpearStatus::Charging.memory(),
        );
    }

    fn timed_out(&self, _end_timestamp: i64, _now: i64) -> bool {
        false
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpearAttack {
    speed_modifier_when_charging: f64,
    speed_modifier_when_repositioning: f64,
    target_in_range_radius_sq: f32,
}

impl SpearAttack {
    const CONDITION: [Condition; 1] = [(MemoryModuleType::SpearStatus, MemoryStatus::ValuePresent)];

    pub fn new(
        speed_modifier_when_charging: f64,
        speed_modifier_when_repositioning: f64,
        target_in_range_radius: f32,
    ) -> Self {
        Self {
            speed_modifier_when_charging,
            speed_modifier_when_repositioning,
            target_in_range_radius_sq: target_in_range_radius * target_in_range_radius,
        }
    }
}

impl Behavior for SpearAttack {
    fn entry_condition(&self) -> &[Condition] {
        &Self::CONDITION
    }

    fn check_extra_start_conditions<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>) -> bool {
        status(cx) == SpearStatus::Charging && able_to_attack(cx) && !is_using_item(cx)
    }

    fn start<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        cx.ai.body.aggressive = true;
        let duration = cx.ai.mob.main_hand.map_or(0, kinetic_use_duration);
        cx.brain.memories.set(
            MemoryModuleType::SpearEngageTime,
            MemoryValue::Int(duration),
        );
        cx.brain
            .memories
            .erase(MemoryModuleType::SpearChargePosition);
        cx.ai.body.using_item = Some(Hand::Main);
    }

    fn can_still_use<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        int(cx, MemoryModuleType::SpearEngageTime, 0) > 0 && able_to_attack(cx)
    }

    fn tick<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        let Some(target) = target(cx) else {
            return;
        };
        let distance_sqr = cx.ai.mob.distance_to_sqr(target.position);
        let extra = passenger_extra(cx);
        look_at_entity(cx, target.id);
        let engage = int(cx, MemoryModuleType::SpearEngageTime, 0) - 1;
        cx.brain
            .memories
            .set(MemoryModuleType::SpearEngageTime, MemoryValue::Int(engage));
        let charge_position = cx
            .brain
            .memories
            .get(MemoryModuleType::SpearChargePosition)
            .and_then(MemoryValue::as_position);
        if let Some(position) = charge_position {
            cx.ai
                .move_to_pos(position, self.speed_modifier_when_repositioning);
            if cx.ai.brain.navigation.is_done() {
                cx.brain
                    .memories
                    .erase(MemoryModuleType::SpearChargePosition);
            }
        } else {
            cx.ai
                .move_to_entity(&target, self.speed_modifier_when_charging);
            if distance_sqr < f64::from(self.target_in_range_radius_sq)
                || cx.ai.brain.navigation.is_done()
            {
                let distance = distance_sqr.sqrt();
                let min = f64::from(MIN_REPOSITION_DISTANCE + extra) - distance;
                let max = f64::from(MAX_REPOSITION_DISTANCE + extra) - distance;
                let away: Option<Vec3> =
                    land_pos_away_between(cx.ai, min, max, AWAY_VERTICAL, target.position);
                cx.brain.memories.set_option(
                    MemoryModuleType::SpearChargePosition,
                    away.map(MemoryValue::Position),
                );
            }
        }
    }

    fn stop<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        cx.ai.stop_navigation();
        cx.ai.body.using_item = None;
        cx.brain
            .memories
            .erase(MemoryModuleType::SpearChargePosition);
        cx.brain.memories.erase(MemoryModuleType::SpearEngageTime);
        cx.brain
            .memories
            .set(MemoryModuleType::SpearStatus, SpearStatus::Retreat.memory());
    }

    fn timed_out(&self, _end_timestamp: i64, _now: i64) -> bool {
        false
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpearRetreat {
    speed_modifier_when_repositioning: f64,
}

impl SpearRetreat {
    const CONDITION: [Condition; 1] = [(MemoryModuleType::SpearStatus, MemoryStatus::ValuePresent)];

    pub fn new(speed_modifier_when_repositioning: f64) -> Self {
        Self {
            speed_modifier_when_repositioning,
        }
    }
}

impl Behavior for SpearRetreat {
    fn entry_condition(&self) -> &[Condition] {
        &Self::CONDITION
    }

    fn check_extra_start_conditions<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>) -> bool {
        if !able_to_attack(cx) || is_using_item(cx) {
            return false;
        }
        if status(cx) != SpearStatus::Retreat {
            return false;
        }
        let Some(target) = target(cx) else {
            return false;
        };
        let distance = cx.ai.mob.distance_to_sqr(target.position).sqrt();
        let extra = passenger_extra(cx);
        let min = (f64::from(MIN_COOLDOWN_DISTANCE + extra) - distance).max(0.0);
        let max = (f64::from(MAX_COOLDOWN_DISTANCE + extra) - distance).max(1.0);
        let Some(away) = land_pos_away_between(cx.ai, min, max, AWAY_VERTICAL, target.position)
        else {
            return false;
        };
        cx.brain.memories.set(
            MemoryModuleType::SpearFleeingPosition,
            MemoryValue::Position(away),
        );
        true
    }

    fn start<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        cx.ai.body.aggressive = true;
        cx.brain
            .memories
            .set(MemoryModuleType::SpearFleeingTime, MemoryValue::Int(0));
    }

    fn can_still_use<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        int(cx, MemoryModuleType::SpearFleeingTime, MAX_FLEEING_TIME) < MAX_FLEEING_TIME
            && cx
                .brain
                .memories
                .has_value(MemoryModuleType::SpearFleeingPosition)
            && !cx.ai.brain.navigation.is_done()
            && able_to_attack(cx)
    }

    fn tick<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        let Some(target) = target(cx) else {
            return;
        };
        look_at_entity(cx, target.id);
        let time = int(cx, MemoryModuleType::SpearFleeingTime, 0) + 1;
        cx.brain
            .memories
            .set(MemoryModuleType::SpearFleeingTime, MemoryValue::Int(time));
        let away = cx
            .brain
            .memories
            .get(MemoryModuleType::SpearFleeingPosition)
            .and_then(MemoryValue::as_position);
        if let Some(away) = away {
            cx.ai
                .move_to_pos(away, self.speed_modifier_when_repositioning);
        }
    }

    fn stop<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        cx.ai.stop_navigation();
        cx.ai.body.aggressive = false;
        cx.ai.body.using_item = None;
        cx.brain.memories.erase(MemoryModuleType::SpearFleeingTime);
        cx.brain
            .memories
            .erase(MemoryModuleType::SpearFleeingPosition);
        cx.brain.memories.erase(MemoryModuleType::SpearStatus);
    }
}
