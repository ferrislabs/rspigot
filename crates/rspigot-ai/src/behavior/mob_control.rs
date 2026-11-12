use crate::behavior::control::{
    Behavior, BehaviorControl, BehaviorCx, DoNothing, Gate, Status, Timed,
};
use crate::behavior::memory::{Condition, MemoryModuleType, MemoryStatus, MemoryValue};
use crate::world::AiWorld;

#[derive(Debug, Clone, PartialEq)]
pub struct CountDownCooldownTicks {
    cooldown_ticks: MemoryModuleType,
    condition: [Condition; 1],
}

impl CountDownCooldownTicks {
    pub fn new(cooldown_ticks: MemoryModuleType) -> Self {
        Self {
            cooldown_ticks,
            condition: [(cooldown_ticks, MemoryStatus::ValuePresent)],
        }
    }

    fn cooldown<W: AiWorld>(&self, cx: &BehaviorCx<'_, '_, W>) -> Option<i32> {
        cx.brain
            .memories
            .get(self.cooldown_ticks)
            .and_then(MemoryValue::as_int)
    }
}

impl Behavior for CountDownCooldownTicks {
    fn entry_condition(&self) -> &[Condition] {
        &self.condition
    }

    fn timed_out(&self, _end_timestamp: i64, _now: i64) -> bool {
        false
    }

    fn can_still_use<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        self.cooldown(cx).is_some_and(|ticks| 0 < ticks)
    }

    fn tick<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        if let Some(ticks) = self.cooldown(cx) {
            cx.brain
                .memories
                .set(self.cooldown_ticks, MemoryValue::Int(ticks - 1));
        }
    }

    fn stop<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        cx.brain.memories.erase(self.cooldown_ticks);
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum MobBehavior {
    CountDownCooldownTicks(CountDownCooldownTicks),
}

impl Behavior for MobBehavior {
    fn entry_condition(&self) -> &[Condition] {
        match self {
            Self::CountDownCooldownTicks(behavior) => behavior.entry_condition(),
        }
    }

    fn check_extra_start_conditions<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>) -> bool {
        match self {
            Self::CountDownCooldownTicks(behavior) => behavior.check_extra_start_conditions(cx),
        }
    }

    fn start<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, now: i64) {
        match self {
            Self::CountDownCooldownTicks(behavior) => behavior.start(cx, now),
        }
    }

    fn can_still_use<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, now: i64) -> bool {
        match self {
            Self::CountDownCooldownTicks(behavior) => behavior.can_still_use(cx, now),
        }
    }

    fn tick<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, now: i64) {
        match self {
            Self::CountDownCooldownTicks(behavior) => behavior.tick(cx, now),
        }
    }

    fn stop<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, now: i64) {
        match self {
            Self::CountDownCooldownTicks(behavior) => behavior.stop(cx, now),
        }
    }

    fn timed_out(&self, end_timestamp: i64, now: i64) -> bool {
        match self {
            Self::CountDownCooldownTicks(behavior) => behavior.timed_out(end_timestamp, now),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum MobControl {
    DoNothing(DoNothing),
    Gate(Gate<MobControl>),
    Timed(Timed<MobBehavior>),
}

impl BehaviorControl for MobControl {
    fn status(&self) -> Status {
        match self {
            Self::DoNothing(control) => control.status(),
            Self::Gate(control) => control.status(),
            Self::Timed(control) => control.status(),
        }
    }

    fn required_memories(&self) -> Vec<MemoryModuleType> {
        match self {
            Self::DoNothing(control) => control.required_memories(),
            Self::Gate(control) => control.required_memories(),
            Self::Timed(control) => control.required_memories(),
        }
    }

    fn try_start<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, now: i64) -> bool {
        match self {
            Self::DoNothing(control) => control.try_start(cx, now),
            Self::Gate(control) => control.try_start(cx, now),
            Self::Timed(control) => control.try_start(cx, now),
        }
    }

    fn tick_or_stop<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, now: i64) {
        match self {
            Self::DoNothing(control) => control.tick_or_stop(cx, now),
            Self::Gate(control) => control.tick_or_stop(cx, now),
            Self::Timed(control) => control.tick_or_stop(cx, now),
        }
    }

    fn do_stop<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, now: i64) {
        match self {
            Self::DoNothing(control) => control.do_stop(cx, now),
            Self::Gate(control) => control.do_stop(cx, now),
            Self::Timed(control) => control.do_stop(cx, now),
        }
    }
}
