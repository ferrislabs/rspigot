use rspigot_entities::EntityKind;
use rspigot_items::ItemKind;

use crate::action::{MobAction, PiglinAction};
use crate::behavior::{
    BehaviorCx, Condition, MemoryModuleType, MemoryStatus, MemoryValue, MobPredicate, Trigger,
    UniformInt, get_entity, look_at_entity,
};
use crate::goals::TemptItems;
use crate::piglin::ai::{
    TIME_BETWEEN_HUNTS, broadcast_anger_target, dont_kill_any_more_hoglins_for_a_while,
    set_anger_target,
};
use crate::random::{MobRandom, RandomSource};
use crate::world::AiWorld;
use rspigot_domain::mth;

const PIGLIN_LOVED: TemptItems = TemptItems::Tag("minecraft:piglin_loved");
const DANCE_CHANCE: f32 = 0.1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BecomePassiveIfMemoryPresent {
    pacifying: MemoryModuleType,
    duration: i32,
}

impl BecomePassiveIfMemoryPresent {
    pub fn new(pacifying: MemoryModuleType, duration: i32) -> Self {
        Self {
            pacifying,
            duration,
        }
    }

    pub fn conditions(&self) -> Vec<Condition> {
        vec![
            (MemoryModuleType::AttackTarget, MemoryStatus::Registered),
            (MemoryModuleType::Pacified, MemoryStatus::ValueAbsent),
            (self.pacifying, MemoryStatus::ValuePresent),
        ]
    }
}

impl Trigger for BecomePassiveIfMemoryPresent {
    fn trigger<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        cx.brain.memories.set_with_expiry(
            MemoryModuleType::Pacified,
            MemoryValue::Bool(true),
            i64::from(self.duration),
        );
        cx.brain.memories.erase(MemoryModuleType::AttackTarget);
        true
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BackUpIfTooClose {
    too_close_distance: i32,
    strafe_speed: f32,
}

impl BackUpIfTooClose {
    pub fn new(too_close_distance: i32, strafe_speed: f32) -> Self {
        Self {
            too_close_distance,
            strafe_speed,
        }
    }

    pub fn conditions(&self) -> Vec<Condition> {
        vec![
            (MemoryModuleType::WalkTarget, MemoryStatus::ValueAbsent),
            (MemoryModuleType::LookTarget, MemoryStatus::Registered),
            (MemoryModuleType::AttackTarget, MemoryStatus::ValuePresent),
            (
                MemoryModuleType::NearestVisibleLivingEntities,
                MemoryStatus::ValuePresent,
            ),
        ]
    }
}

impl Trigger for BackUpIfTooClose {
    fn trigger<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        let Some(target) =
            get_entity(cx, MemoryModuleType::AttackTarget).and_then(|id| cx.ai.world.living(id))
        else {
            return false;
        };
        let distance = f64::from(self.too_close_distance);
        if cx.ai.mob.distance_to_sqr(target.position) >= distance * distance {
            return false;
        }
        let visible = cx
            .brain
            .memories
            .get(MemoryModuleType::NearestVisibleLivingEntities)
            .and_then(MemoryValue::as_visible)
            .is_some_and(|visible| visible.contains_entity(cx.ai, &cx.brain.memories, target.id));
        if !visible {
            return false;
        }
        look_at_entity(cx, target.id);
        cx.ai.brain.movement.strafe(-self.strafe_speed, 0.0);
        let mob = cx.ai.mob;
        cx.ai.body.yaw = mth::rotate_if_necessary(mob.yaw, mob.head_yaw, 0.0);
        true
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CopyIf {
    IsBaby,
    NearZombified,
    BabyEvery(UniformInt),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CopyMemoryWithExpiry {
    copy_if: CopyIf,
    source: MemoryModuleType,
    target: MemoryModuleType,
    duration: UniformInt,
    ticks_until_next_start: i32,
}

impl CopyMemoryWithExpiry {
    pub fn new(
        copy_if: CopyIf,
        source: MemoryModuleType,
        target: MemoryModuleType,
        duration: UniformInt,
    ) -> Self {
        Self {
            copy_if,
            source,
            target,
            duration,
            ticks_until_next_start: 0,
        }
    }

    fn tick_down_and_check<W: AiWorld>(
        &mut self,
        cx: &mut BehaviorCx<'_, '_, W>,
        interval: UniformInt,
    ) -> bool {
        if self.ticks_until_next_start == 0 {
            self.ticks_until_next_start = interval.sample(&mut cx.ai.brain.random) - 1;
            return false;
        }
        self.ticks_until_next_start -= 1;
        self.ticks_until_next_start == 0
    }

    pub fn conditions(&self) -> Vec<Condition> {
        vec![
            (self.source, MemoryStatus::ValuePresent),
            (self.target, MemoryStatus::ValueAbsent),
        ]
    }
}

impl Trigger for CopyMemoryWithExpiry {
    fn trigger<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        let copy = match self.copy_if {
            CopyIf::IsBaby => MobPredicate::IsBaby.test(cx),
            CopyIf::NearZombified => crate::piglin::targets::is_near_zombified(cx),
            CopyIf::BabyEvery(interval) => {
                MobPredicate::IsBaby.test(cx) && self.tick_down_and_check(cx, interval)
            }
        };
        if !copy {
            return false;
        }
        let Some(value) = cx.brain.memories.get(self.source).cloned() else {
            return false;
        };
        let ticks = self.duration.sample(&mut cx.ai.brain.random);
        cx.brain
            .memories
            .set_with_expiry(self.target, value, i64::from(ticks));
        true
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StartCelebratingIfTargetDead {
    celebrate_duration: i32,
}

impl StartCelebratingIfTargetDead {
    pub fn new(celebrate_duration: i32) -> Self {
        Self { celebrate_duration }
    }

    pub fn conditions(&self) -> Vec<Condition> {
        vec![
            (MemoryModuleType::AttackTarget, MemoryStatus::ValuePresent),
            (MemoryModuleType::AngryAt, MemoryStatus::Registered),
            (
                MemoryModuleType::CelebrateLocation,
                MemoryStatus::ValueAbsent,
            ),
            (MemoryModuleType::Dancing, MemoryStatus::Registered),
        ]
    }
}

impl Trigger for StartCelebratingIfTargetDead {
    fn trigger<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        let Some(target) =
            get_entity(cx, MemoryModuleType::AttackTarget).and_then(|id| cx.ai.world.living(id))
        else {
            return false;
        };
        if target.alive {
            return false;
        }
        let ticks = i64::from(self.celebrate_duration);
        if target.kind == EntityKind::Hoglin
            && MobRandom::new(cx.ai.world.game_time()).next_float() < DANCE_CHANCE
        {
            cx.brain.memories.set_with_expiry(
                MemoryModuleType::Dancing,
                MemoryValue::Bool(true),
                ticks,
            );
        }
        cx.brain.memories.set_with_expiry(
            MemoryModuleType::CelebrateLocation,
            MemoryValue::Block(target.position.block()),
            ticks,
        );
        if target.player && !cx.ai.world.forgive_dead_players() {
            return true;
        }
        cx.brain.memories.erase(MemoryModuleType::AttackTarget);
        cx.brain.memories.erase(MemoryModuleType::AngryAt);
        true
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StartHuntingHoglin;

impl StartHuntingHoglin {
    pub fn conditions(&self) -> Vec<Condition> {
        vec![
            (
                MemoryModuleType::NearestVisibleHuntableHoglin,
                MemoryStatus::ValuePresent,
            ),
            (MemoryModuleType::AngryAt, MemoryStatus::ValueAbsent),
            (MemoryModuleType::HuntedRecently, MemoryStatus::ValueAbsent),
            (
                MemoryModuleType::NearestVisibleAdultPiglins,
                MemoryStatus::Registered,
            ),
        ]
    }
}

impl Trigger for StartHuntingHoglin {
    fn trigger<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        if cx.ai.mob.baby {
            return false;
        }
        let others: Vec<rspigot_domain::EntityId> = cx
            .brain
            .memories
            .get(MemoryModuleType::NearestVisibleAdultPiglins)
            .and_then(MemoryValue::as_entities)
            .map(<[_]>::to_vec)
            .unwrap_or_default();
        let hunted_recently = others.iter().any(|id| {
            cx.ai
                .world
                .living(*id)
                .is_some_and(|other| other.facts.hunted_recently)
        });
        if hunted_recently {
            return false;
        }
        let Some(hoglin) = get_entity(cx, MemoryModuleType::NearestVisibleHuntableHoglin)
            .and_then(|id| cx.ai.world.living(id))
        else {
            return false;
        };
        set_anger_target(cx, &hoglin);
        dont_kill_any_more_hoglins_for_a_while(cx);
        broadcast_anger_target(cx, &hoglin);
        for other in others {
            cx.ai.act(MobAction::Message {
                to: other,
                message: crate::action::BrainMessage::PiglinDontHunt,
            });
        }
        true
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RememberIfHoglinWasKilled;

impl RememberIfHoglinWasKilled {
    pub fn conditions(&self) -> Vec<Condition> {
        vec![
            (MemoryModuleType::AttackTarget, MemoryStatus::ValuePresent),
            (MemoryModuleType::HuntedRecently, MemoryStatus::Registered),
        ]
    }
}

impl Trigger for RememberIfHoglinWasKilled {
    fn trigger<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        let Some(target) =
            get_entity(cx, MemoryModuleType::AttackTarget).and_then(|id| cx.ai.world.living(id))
        else {
            return false;
        };
        if target.kind == EntityKind::Hoglin && !target.alive {
            let ticks = TIME_BETWEEN_HUNTS.sample(&mut cx.ai.brain.random);
            cx.brain.memories.set_with_expiry(
                MemoryModuleType::HuntedRecently,
                MemoryValue::Bool(true),
                i64::from(ticks),
            );
            return true;
        }
        false
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StartAdmiringItemIfSeen {
    admire_duration: i32,
}

impl StartAdmiringItemIfSeen {
    pub fn new(admire_duration: i32) -> Self {
        Self { admire_duration }
    }

    pub fn conditions(&self) -> Vec<Condition> {
        vec![
            (
                MemoryModuleType::NearestVisibleWantedItem,
                MemoryStatus::ValuePresent,
            ),
            (MemoryModuleType::AdmiringItem, MemoryStatus::ValueAbsent),
            (
                MemoryModuleType::AdmiringDisabled,
                MemoryStatus::ValueAbsent,
            ),
            (
                MemoryModuleType::DisableWalkToAdmireItem,
                MemoryStatus::ValueAbsent,
            ),
        ]
    }
}

impl Trigger for StartAdmiringItemIfSeen {
    fn trigger<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        let Some(item) = get_entity(cx, MemoryModuleType::NearestVisibleWantedItem)
            .and_then(|id| cx.ai.world.dropped_item(id))
        else {
            return false;
        };
        if PIGLIN_LOVED.matches(Some(item.item)) {
            cx.brain.memories.set_with_expiry(
                MemoryModuleType::AdmiringItem,
                MemoryValue::Bool(true),
                i64::from(self.admire_duration),
            );
            return true;
        }
        false
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StopAdmiringIfItemTooFarAway {
    max_distance_to_item: i32,
}

impl StopAdmiringIfItemTooFarAway {
    pub fn new(max_distance_to_item: i32) -> Self {
        Self {
            max_distance_to_item,
        }
    }

    pub fn conditions(&self) -> Vec<Condition> {
        vec![
            (MemoryModuleType::AdmiringItem, MemoryStatus::ValuePresent),
            (
                MemoryModuleType::NearestVisibleWantedItem,
                MemoryStatus::Registered,
            ),
        ]
    }
}

impl Trigger for StopAdmiringIfItemTooFarAway {
    fn trigger<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        if cx.ai.mob.off_hand.is_some() {
            return false;
        }
        let item = get_entity(cx, MemoryModuleType::NearestVisibleWantedItem)
            .and_then(|id| cx.ai.world.dropped_item(id));
        let distance = f64::from(self.max_distance_to_item);
        if item.is_some_and(|item| cx.ai.mob.distance_to_sqr(item.position) < distance * distance) {
            return false;
        }
        cx.brain.memories.erase(MemoryModuleType::AdmiringItem);
        true
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StopAdmiringIfTiredOfTryingToReachItem {
    max_time_to_reach_item: i32,
    disable_time: i32,
}

impl StopAdmiringIfTiredOfTryingToReachItem {
    pub fn new(max_time_to_reach_item: i32, disable_time: i32) -> Self {
        Self {
            max_time_to_reach_item,
            disable_time,
        }
    }

    pub fn conditions(&self) -> Vec<Condition> {
        vec![
            (MemoryModuleType::AdmiringItem, MemoryStatus::ValuePresent),
            (
                MemoryModuleType::NearestVisibleWantedItem,
                MemoryStatus::ValuePresent,
            ),
            (
                MemoryModuleType::TimeTryingToReachAdmireItem,
                MemoryStatus::Registered,
            ),
            (
                MemoryModuleType::DisableWalkToAdmireItem,
                MemoryStatus::Registered,
            ),
        ]
    }
}

impl Trigger for StopAdmiringIfTiredOfTryingToReachItem {
    fn trigger<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        if cx.ai.mob.off_hand.is_some() {
            return false;
        }
        let time = cx
            .brain
            .memories
            .get(MemoryModuleType::TimeTryingToReachAdmireItem)
            .and_then(MemoryValue::as_int);
        match time {
            None => cx.brain.memories.set(
                MemoryModuleType::TimeTryingToReachAdmireItem,
                MemoryValue::Int(0),
            ),
            Some(time) if self.max_time_to_reach_item < time => {
                cx.brain.memories.erase(MemoryModuleType::AdmiringItem);
                cx.brain
                    .memories
                    .erase(MemoryModuleType::TimeTryingToReachAdmireItem);
                cx.brain.memories.set_with_expiry(
                    MemoryModuleType::DisableWalkToAdmireItem,
                    MemoryValue::Bool(true),
                    i64::from(self.disable_time),
                );
            }
            Some(time) => cx.brain.memories.set(
                MemoryModuleType::TimeTryingToReachAdmireItem,
                MemoryValue::Int(time + 1),
            ),
        }
        true
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StopHoldingItemIfNoLongerAdmiring;

impl StopHoldingItemIfNoLongerAdmiring {
    pub fn conditions(&self) -> Vec<Condition> {
        vec![(MemoryModuleType::AdmiringItem, MemoryStatus::ValueAbsent)]
    }
}

impl Trigger for StopHoldingItemIfNoLongerAdmiring {
    fn trigger<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        if cx
            .ai
            .mob
            .off_hand
            .is_some_and(|item| item != ItemKind::Shield.id())
        {
            cx.ai
                .act(MobAction::Piglin(PiglinAction::StopHoldingOffHand {
                    bartering_enabled: true,
                }));
            return true;
        }
        false
    }
}
