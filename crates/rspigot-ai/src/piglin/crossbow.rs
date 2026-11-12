use rspigot_items::ItemKind;

use crate::action::MobAction;
use crate::behavior::{
    Behavior, BehaviorCx, Condition, MemoryModuleType, MemoryStatus, MemoryValue, get_entity,
    look_at_entity,
};
use crate::combat::weapon_hand;
use crate::crossbow::{CrossbowAction, DEFAULT_CHARGE_TICKS};
use crate::random::RandomSource;
use crate::world::{AiWorld, Living};

pub const CROSSBOW_ATTACK_DURATION: i32 = 1200;
const ATTACK_DELAY_BASE: i32 = 20;
const ATTACK_DELAY_SPREAD: i32 = 20;
const RANGED_POWER: f32 = 1.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Uncharged,
    Charging,
    Charged,
    ReadyToAttack,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CrossbowAttack {
    state: State,
    attack_delay: i32,
}

impl Default for CrossbowAttack {
    fn default() -> Self {
        Self::new()
    }
}

impl CrossbowAttack {
    const CONDITION: [Condition; 2] = [
        (MemoryModuleType::LookTarget, MemoryStatus::Registered),
        (MemoryModuleType::AttackTarget, MemoryStatus::ValuePresent),
    ];

    pub fn new() -> Self {
        Self {
            state: State::Uncharged,
            attack_delay: 0,
        }
    }

    fn attack_target<W: AiWorld>(cx: &BehaviorCx<'_, '_, W>) -> Option<Living> {
        get_entity(cx, MemoryModuleType::AttackTarget).and_then(|id| cx.ai.world.living(id))
    }

    fn can_see<W: AiWorld>(cx: &mut BehaviorCx<'_, '_, W>, target: &Living) -> bool {
        cx.brain
            .memories
            .get(MemoryModuleType::NearestVisibleLivingEntities)
            .and_then(MemoryValue::as_visible)
            .is_some_and(|visible| visible.contains_entity(cx.ai, &cx.brain.memories, target.id))
    }

    fn crossbow_attack<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, target: &Living) {
        match self.state {
            State::Uncharged => {
                if cx.ai.body.using_item.is_none() {
                    cx.ai.body.using_item = Some(weapon_hand(cx.ai.mob, ItemKind::Crossbow));
                }
                self.state = State::Charging;
                cx.ai
                    .act(MobAction::Crossbow(CrossbowAction::SetCharging(true)));
            }
            State::Charging => {
                let using = cx.ai.body.using_item;
                if using.is_none() {
                    self.state = State::Uncharged;
                }
                let (ticks, duration) = match using {
                    Some(_) => (
                        cx.ai.mob.ticks_using_item,
                        cx.ai.mob.species.crossbow.charge_ticks,
                    ),
                    None => (0, DEFAULT_CHARGE_TICKS),
                };
                if duration <= ticks
                    && let Some(hand) = using
                {
                    cx.ai.body.using_item = None;
                    cx.ai
                        .act(MobAction::Crossbow(CrossbowAction::Release { hand, ticks }));
                    self.state = State::Charged;
                    self.attack_delay =
                        ATTACK_DELAY_BASE + cx.ai.brain.random.next_int(ATTACK_DELAY_SPREAD);
                    cx.ai
                        .act(MobAction::Crossbow(CrossbowAction::SetCharging(false)));
                }
            }
            State::Charged => {
                self.attack_delay -= 1;
                if self.attack_delay == 0 {
                    self.state = State::ReadyToAttack;
                }
            }
            State::ReadyToAttack => {
                cx.ai.act(MobAction::RangedAttack {
                    target: target.id,
                    power: RANGED_POWER,
                });
                self.state = State::Uncharged;
            }
        }
    }
}

impl Behavior for CrossbowAttack {
    fn entry_condition(&self) -> &[Condition] {
        &Self::CONDITION
    }

    fn check_extra_start_conditions<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>) -> bool {
        let Some(target) = Self::attack_target(cx) else {
            return false;
        };
        cx.ai.mob.is_holding(ItemKind::Crossbow.id())
            && Self::can_see(cx, &target)
            && crate::behavior::is_within_attack_range(cx, &target, 0)
    }

    fn can_still_use<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        cx.brain.memories.has_value(MemoryModuleType::AttackTarget)
            && self.check_extra_start_conditions(cx)
    }

    fn tick<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        let Some(target) = Self::attack_target(cx) else {
            return;
        };
        look_at_entity(cx, target.id);
        self.crossbow_attack(cx, &target);
    }

    fn stop<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        if cx.ai.body.using_item.is_some() {
            cx.ai.body.using_item = None;
        }
        if cx.ai.mob.is_holding(ItemKind::Crossbow.id()) {
            cx.ai
                .act(MobAction::Crossbow(CrossbowAction::SetCharging(false)));
            cx.ai.act(MobAction::Crossbow(CrossbowAction::ClearCharged));
        }
    }
}
