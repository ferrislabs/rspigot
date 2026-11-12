use crate::behavior::brain::BrainState;
use crate::behavior::memory::{Condition, MemoryModuleType};
use crate::behavior::shuffling::ShufflingList;
use crate::brain::AiContext;
use crate::random::RandomSource;
use crate::world::AiWorld;

pub const DEFAULT_DURATION: i32 = 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Stopped,
    Running,
}

pub struct BehaviorCx<'a, 'b, W> {
    pub ai: &'a mut AiContext<'b, W>,
    pub brain: &'a mut BrainState,
}

pub fn has_required_memories(brain: &BrainState, conditions: &[Condition]) -> bool {
    conditions
        .iter()
        .all(|(ty, status)| brain.memories.check(*ty, *status))
}

fn merged(
    conditions: &[Condition],
    others: impl IntoIterator<Item = MemoryModuleType>,
) -> Vec<MemoryModuleType> {
    let mut memories: Vec<MemoryModuleType> = conditions.iter().map(|(ty, _)| *ty).collect();
    memories.extend(others);
    memories.sort_unstable();
    memories.dedup();
    memories
}

fn random_duration<W: AiWorld>(cx: &mut BehaviorCx<'_, '_, W>, min: i32, max: i32) -> i64 {
    let bound = (max + 1 - min).max(1);
    i64::from(min + cx.ai.brain.random.next_int(bound))
}

pub trait BehaviorControl {
    fn status(&self) -> Status;

    fn required_memories(&self) -> Vec<MemoryModuleType>;

    fn try_start<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, now: i64) -> bool;

    fn tick_or_stop<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, now: i64);

    fn do_stop<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, now: i64);
}

pub trait Behavior {
    fn entry_condition(&self) -> &[Condition];

    fn check_extra_start_conditions<W: AiWorld>(
        &mut self,
        _cx: &mut BehaviorCx<'_, '_, W>,
    ) -> bool {
        true
    }

    fn start<W: AiWorld>(&mut self, _cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {}

    fn can_still_use<W: AiWorld>(&mut self, _cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        false
    }

    fn tick<W: AiWorld>(&mut self, _cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {}

    fn stop<W: AiWorld>(&mut self, _cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {}

    fn timed_out(&self, end_timestamp: i64, now: i64) -> bool {
        end_timestamp < now
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Timed<B> {
    status: Status,
    end_timestamp: i64,
    min_duration: i32,
    max_duration: i32,
    inner: B,
}

impl<B> Timed<B> {
    pub fn new(inner: B) -> Self {
        Self::with_duration(inner, DEFAULT_DURATION, DEFAULT_DURATION)
    }

    pub fn with_timeout(inner: B, time_out_duration: i32) -> Self {
        Self::with_duration(inner, time_out_duration, time_out_duration)
    }

    pub fn with_duration(inner: B, min_duration: i32, max_duration: i32) -> Self {
        Self {
            status: Status::Stopped,
            end_timestamp: 0,
            min_duration,
            max_duration,
            inner,
        }
    }

    pub fn inner(&self) -> &B {
        &self.inner
    }

    pub fn inner_mut(&mut self) -> &mut B {
        &mut self.inner
    }

    pub fn end_timestamp(&self) -> i64 {
        self.end_timestamp
    }
}

impl<B: Behavior> BehaviorControl for Timed<B> {
    fn status(&self) -> Status {
        self.status
    }

    fn required_memories(&self) -> Vec<MemoryModuleType> {
        merged(self.inner.entry_condition(), [])
    }

    fn try_start<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, now: i64) -> bool {
        if !has_required_memories(cx.brain, self.inner.entry_condition())
            || !self.inner.check_extra_start_conditions(cx)
        {
            return false;
        }
        self.status = Status::Running;
        self.end_timestamp = now + random_duration(cx, self.min_duration, self.max_duration);
        self.inner.start(cx, now);
        true
    }

    fn tick_or_stop<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, now: i64) {
        if !self.inner.timed_out(self.end_timestamp, now) && self.inner.can_still_use(cx, now) {
            self.inner.tick(cx, now);
        } else {
            self.do_stop(cx, now);
        }
    }

    fn do_stop<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, now: i64) {
        self.status = Status::Stopped;
        self.inner.stop(cx, now);
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DoNothing {
    status: Status,
    end_timestamp: i64,
    min_duration: i32,
    max_duration: i32,
}

impl DoNothing {
    pub fn new(min_duration: i32, max_duration: i32) -> Self {
        Self {
            status: Status::Stopped,
            end_timestamp: 0,
            min_duration,
            max_duration,
        }
    }
}

impl BehaviorControl for DoNothing {
    fn status(&self) -> Status {
        self.status
    }

    fn required_memories(&self) -> Vec<MemoryModuleType> {
        Vec::new()
    }

    fn try_start<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, now: i64) -> bool {
        self.status = Status::Running;
        self.end_timestamp = now + random_duration(cx, self.min_duration, self.max_duration);
        true
    }

    fn tick_or_stop<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, now: i64) {
        if self.end_timestamp < now {
            self.do_stop(cx, now);
        }
    }

    fn do_stop<W: AiWorld>(&mut self, _cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        self.status = Status::Stopped;
    }
}

pub trait Trigger {
    fn trigger<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, now: i64) -> bool;

    fn required_memories(&self) -> Vec<MemoryModuleType> {
        Vec::new()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct OneShot<T> {
    status: Status,
    conditions: Vec<Condition>,
    trigger: T,
}

impl<T: Trigger> OneShot<T> {
    pub fn new(conditions: Vec<Condition>, trigger: T) -> Self {
        Self {
            status: Status::Stopped,
            conditions,
            trigger,
        }
    }

    pub fn trigger<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, now: i64) -> bool {
        has_required_memories(cx.brain, &self.conditions) && self.trigger.trigger(cx, now)
    }

    pub fn sequence<F: Trigger>(first: F, second: Self) -> OneShot<Sequence<F, Self>> {
        OneShot::new(Vec::new(), Sequence { first, second })
    }
}

impl<T: Trigger> Trigger for OneShot<T> {
    fn trigger<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, now: i64) -> bool {
        OneShot::trigger(self, cx, now)
    }

    fn required_memories(&self) -> Vec<MemoryModuleType> {
        merged(&self.conditions, self.trigger.required_memories())
    }
}

impl<T: Trigger> BehaviorControl for OneShot<T> {
    fn status(&self) -> Status {
        self.status
    }

    fn required_memories(&self) -> Vec<MemoryModuleType> {
        Trigger::required_memories(self)
    }

    fn try_start<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, now: i64) -> bool {
        if OneShot::trigger(self, cx, now) {
            self.status = Status::Running;
            return true;
        }
        false
    }

    fn tick_or_stop<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, now: i64) {
        self.do_stop(cx, now);
    }

    fn do_stop<W: AiWorld>(&mut self, _cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        self.status = Status::Stopped;
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Sequence<A, B> {
    first: A,
    second: B,
}

impl<A, B> Sequence<A, B> {
    pub fn new(first: A, second: B) -> Self {
        Self { first, second }
    }
}

impl<A: Trigger, B: Trigger> Trigger for Sequence<A, B> {
    fn trigger<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, now: i64) -> bool {
        self.first.trigger(cx, now) && self.second.trigger(cx, now)
    }

    fn required_memories(&self) -> Vec<MemoryModuleType> {
        self.second.required_memories()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderPolicy {
    Ordered,
    Shuffled,
}

impl OrderPolicy {
    fn apply<T>(self, list: &mut ShufflingList<T>, random: &mut impl RandomSource) {
        match self {
            Self::Ordered => {}
            Self::Shuffled => list.shuffle(random),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunningPolicy {
    RunOne,
    TryAll,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TriggerGate<T> {
    order: OrderPolicy,
    running: RunningPolicy,
    triggers: ShufflingList<T>,
}

impl<T: Trigger> TriggerGate<T> {
    pub fn new(weighted: Vec<(T, i32)>, order: OrderPolicy, running: RunningPolicy) -> Self {
        Self {
            order,
            running,
            triggers: weighted.into_iter().collect(),
        }
    }

    pub fn one_shot(
        weighted: Vec<(T, i32)>,
        order: OrderPolicy,
        running: RunningPolicy,
    ) -> OneShot<Self> {
        OneShot::new(Vec::new(), Self::new(weighted, order, running))
    }

    pub fn one_shuffled(weighted: Vec<(T, i32)>) -> OneShot<Self> {
        Self::one_shot(weighted, OrderPolicy::Shuffled, RunningPolicy::RunOne)
    }
}

impl<T: Trigger> Trigger for TriggerGate<T> {
    fn trigger<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, now: i64) -> bool {
        self.order
            .apply(&mut self.triggers, &mut cx.ai.brain.random);
        for index in 0..self.triggers.len() {
            let Some(trigger) = self.triggers.get_mut(index) else {
                break;
            };
            if trigger.trigger(cx, now) && self.running == RunningPolicy::RunOne {
                return true;
            }
        }
        true
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Gate<C> {
    status: Status,
    entry_condition: Vec<Condition>,
    exit_erased_memories: Vec<MemoryModuleType>,
    order: OrderPolicy,
    running: RunningPolicy,
    behaviors: ShufflingList<C>,
}

pub type RunOne<C> = Gate<C>;

impl<C: BehaviorControl> Gate<C> {
    pub fn new(
        entry_condition: Vec<Condition>,
        exit_erased_memories: Vec<MemoryModuleType>,
        order: OrderPolicy,
        running: RunningPolicy,
        behaviors: Vec<(C, i32)>,
    ) -> Self {
        Self {
            status: Status::Stopped,
            entry_condition,
            exit_erased_memories,
            order,
            running,
            behaviors: behaviors.into_iter().collect(),
        }
    }

    pub fn run_one(weighted_behaviors: Vec<(C, i32)>) -> Self {
        Self::run_one_when(Vec::new(), weighted_behaviors)
    }

    pub fn run_one_when(
        entry_condition: Vec<Condition>,
        weighted_behaviors: Vec<(C, i32)>,
    ) -> Self {
        Self::new(
            entry_condition,
            Vec::new(),
            OrderPolicy::Shuffled,
            RunningPolicy::RunOne,
            weighted_behaviors,
        )
    }

    pub fn behaviors(&self) -> &ShufflingList<C> {
        &self.behaviors
    }

    fn for_each_running<W: AiWorld>(
        &mut self,
        cx: &mut BehaviorCx<'_, '_, W>,
        mut action: impl FnMut(&mut C, &mut BehaviorCx<'_, '_, W>),
    ) {
        for index in 0..self.behaviors.len() {
            if let Some(behavior) = self.behaviors.get_mut(index)
                && behavior.status() == Status::Running
            {
                action(behavior, cx);
            }
        }
    }
}

impl<C: BehaviorControl> BehaviorControl for Gate<C> {
    fn status(&self) -> Status {
        self.status
    }

    fn required_memories(&self) -> Vec<MemoryModuleType> {
        merged(
            &self.entry_condition,
            self.behaviors
                .iter()
                .flat_map(BehaviorControl::required_memories),
        )
    }

    fn try_start<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, now: i64) -> bool {
        if !has_required_memories(cx.brain, &self.entry_condition) {
            return false;
        }
        self.status = Status::Running;
        self.order
            .apply(&mut self.behaviors, &mut cx.ai.brain.random);
        for index in 0..self.behaviors.len() {
            let Some(behavior) = self.behaviors.get_mut(index) else {
                break;
            };
            if behavior.status() != Status::Stopped {
                continue;
            }
            let started = behavior.try_start(cx, now);
            if started && self.running == RunningPolicy::RunOne {
                break;
            }
        }
        true
    }

    fn tick_or_stop<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, now: i64) {
        self.for_each_running(cx, |behavior, cx| behavior.tick_or_stop(cx, now));
        if self
            .behaviors
            .iter()
            .all(|behavior| behavior.status() != Status::Running)
        {
            self.do_stop(cx, now);
        }
    }

    fn do_stop<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, now: i64) {
        self.status = Status::Stopped;
        self.for_each_running(cx, |behavior, cx| behavior.do_stop(cx, now));
        for memory in &self.exit_erased_memories {
            cx.brain.memories.erase(*memory);
        }
    }
}
