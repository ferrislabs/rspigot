use std::collections::{BTreeMap, BTreeSet};

use rspigot_nbt::Compound;

use crate::behavior::activity::{Activity, Schedule};
use crate::behavior::control::{BehaviorControl, BehaviorCx, Status};
use crate::behavior::memory::{Condition, Memories, MemoryModuleType, MemoryStatus};
use crate::behavior::nbt::PackedMemories;
use crate::behavior::sensor::{Sensor, SensorKind, SensorSlot};
use crate::brain::AiContext;
use crate::random::RandomSource;
use crate::world::AiWorld;

pub const SCHEDULE_UPDATE_INTERVAL: i64 = 20;
const NEVER_UPDATED: i64 = -9999;

#[derive(Debug, Clone, PartialEq)]
pub struct BrainState {
    pub memories: Memories,
    activity_requirements: BTreeMap<Activity, Vec<Condition>>,
    activity_memories_to_erase_when_stopped: BTreeMap<Activity, Vec<MemoryModuleType>>,
    core_activities: BTreeSet<Activity>,
    active_activities: BTreeSet<Activity>,
    default_activity: Activity,
    schedule: Option<Schedule>,
    last_schedule_update: i64,
}

impl Default for BrainState {
    fn default() -> Self {
        let mut state = Self {
            memories: Memories::default(),
            activity_requirements: BTreeMap::new(),
            activity_memories_to_erase_when_stopped: BTreeMap::new(),
            core_activities: BTreeSet::new(),
            active_activities: BTreeSet::new(),
            default_activity: Activity::Idle,
            schedule: None,
            last_schedule_update: NEVER_UPDATED,
        };
        state.set_core_activities([Activity::Core]);
        state.use_default_activity();
        state
    }
}

impl BrainState {
    pub fn schedule(&self) -> Option<Schedule> {
        self.schedule
    }

    pub fn set_schedule(&mut self, schedule: Option<Schedule>) {
        self.schedule = schedule;
    }

    pub fn set_core_activities(&mut self, activities: impl IntoIterator<Item = Activity>) {
        self.core_activities = activities.into_iter().collect();
    }

    pub fn set_default_activity(&mut self, activity: Activity) {
        self.default_activity = activity;
    }

    pub fn active_activities(&self) -> &BTreeSet<Activity> {
        &self.active_activities
    }

    pub fn is_active(&self, activity: Activity) -> bool {
        self.active_activities.contains(&activity)
    }

    pub fn active_non_core_activity(&self) -> Option<Activity> {
        self.active_activities
            .iter()
            .copied()
            .find(|activity| !self.core_activities.contains(activity))
    }

    pub fn use_default_activity(&mut self) {
        self.set_active_activity(self.default_activity);
    }

    pub fn set_active_activity_if_possible(&mut self, activity: Activity) {
        if self.activity_requirements_are_met(activity) {
            self.set_active_activity(activity);
        } else {
            self.use_default_activity();
        }
    }

    pub fn set_active_activity_to_first_valid(&mut self, activities: &[Activity]) {
        if let Some(activity) = activities
            .iter()
            .copied()
            .find(|activity| self.activity_requirements_are_met(*activity))
        {
            self.set_active_activity(activity);
        }
    }

    pub fn update_activity_from_schedule(&mut self, clock_ticks: i64, game_time: i64) {
        if game_time - self.last_schedule_update <= SCHEDULE_UPDATE_INTERVAL {
            return;
        }
        self.last_schedule_update = game_time;
        let activity = self
            .schedule
            .map_or(Activity::Idle, |schedule| schedule.activity_at(clock_ticks));
        if !self.is_active(activity) {
            self.set_active_activity_if_possible(activity);
        }
    }

    fn set_active_activity(&mut self, activity: Activity) {
        if self.is_active(activity) {
            return;
        }
        self.erase_memories_for_other_activities_than(activity);
        self.active_activities.clear();
        self.active_activities
            .extend(self.core_activities.iter().copied());
        self.active_activities.insert(activity);
    }

    fn erase_memories_for_other_activities_than(&mut self, activity: Activity) {
        for active in &self.active_activities {
            if *active == activity {
                continue;
            }
            if let Some(memories) = self.activity_memories_to_erase_when_stopped.get(active) {
                for memory in memories {
                    self.memories.erase(*memory);
                }
            }
        }
    }

    fn activity_requirements_are_met(&self, activity: Activity) -> bool {
        self.activity_requirements
            .get(&activity)
            .is_some_and(|conditions| {
                conditions
                    .iter()
                    .all(|(ty, status)| self.memories.check(*ty, *status))
            })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ActivityData<C> {
    pub activity: Activity,
    pub behaviors: Vec<(i32, C)>,
    pub conditions: Vec<Condition>,
    pub memories_to_erase_when_stopped: Vec<MemoryModuleType>,
}

impl<C> ActivityData<C> {
    pub fn create(activity: Activity, first_priority: i32, behaviors: Vec<C>) -> Self {
        Self::with_conditions(
            activity,
            Self::priority_pairs(first_priority, behaviors),
            Vec::new(),
        )
    }

    pub fn create_erasing(
        activity: Activity,
        first_priority: i32,
        behaviors: Vec<C>,
        memory_that_must_have_value_and_will_be_erased_after: MemoryModuleType,
    ) -> Self {
        Self {
            activity,
            behaviors: Self::priority_pairs(first_priority, behaviors),
            conditions: vec![(
                memory_that_must_have_value_and_will_be_erased_after,
                MemoryStatus::ValuePresent,
            )],
            memories_to_erase_when_stopped: vec![
                memory_that_must_have_value_and_will_be_erased_after,
            ],
        }
    }

    pub fn create_when(
        activity: Activity,
        first_priority: i32,
        behaviors: Vec<C>,
        conditions: Vec<Condition>,
    ) -> Self {
        Self::with_conditions(
            activity,
            Self::priority_pairs(first_priority, behaviors),
            conditions,
        )
    }

    pub fn from_pairs_erasing(
        activity: Activity,
        behaviors: Vec<(i32, C)>,
        memory_that_must_have_value_and_will_be_erased_after: MemoryModuleType,
    ) -> Self {
        Self {
            activity,
            behaviors,
            conditions: vec![(
                memory_that_must_have_value_and_will_be_erased_after,
                MemoryStatus::ValuePresent,
            )],
            memories_to_erase_when_stopped: vec![
                memory_that_must_have_value_and_will_be_erased_after,
            ],
        }
    }

    pub fn from_pairs(activity: Activity, behaviors: Vec<(i32, C)>) -> Self {
        Self::with_conditions(activity, behaviors, Vec::new())
    }

    pub fn with_conditions(
        activity: Activity,
        behaviors: Vec<(i32, C)>,
        conditions: Vec<Condition>,
    ) -> Self {
        Self {
            activity,
            behaviors,
            conditions,
            memories_to_erase_when_stopped: Vec::new(),
        }
    }

    pub fn priority_pairs(first_priority: i32, behaviors: Vec<C>) -> Vec<(i32, C)> {
        behaviors
            .into_iter()
            .enumerate()
            .map(|(index, behavior)| (first_priority + index as i32, behavior))
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct BehaviorBrain<C> {
    state: BrainState,
    sensors: Vec<SensorSlot>,
    behaviors: BTreeMap<i32, Vec<(Activity, Vec<C>)>>,
}

impl<C: BehaviorControl> BehaviorBrain<C> {
    pub fn new(
        memory_types: &[MemoryModuleType],
        sensors: Vec<SensorKind>,
        activities: Vec<ActivityData<C>>,
        packed: &PackedMemories,
        random: &mut impl RandomSource,
    ) -> Self {
        let mut brain = Self {
            state: BrainState::default(),
            sensors: Vec::new(),
            behaviors: BTreeMap::new(),
        };
        for ty in memory_types {
            brain.state.memories.register(*ty);
        }
        for sensor in sensors {
            for ty in sensor.requires() {
                brain.state.memories.register(*ty);
            }
            brain.sensors.push(SensorSlot::new(sensor, random));
        }
        for data in activities {
            brain.add_activity(data);
        }
        brain.state.memories.load(packed);
        brain.state.set_core_activities([Activity::Core]);
        brain.state.use_default_activity();
        brain
    }

    pub fn state(&self) -> &BrainState {
        &self.state
    }

    pub fn state_mut(&mut self) -> &mut BrainState {
        &mut self.state
    }

    pub fn memories(&self) -> &Memories {
        &self.state.memories
    }

    pub fn memories_mut(&mut self) -> &mut Memories {
        &mut self.state.memories
    }

    pub fn sensors(&self) -> &[SensorSlot] {
        &self.sensors
    }

    pub fn add_activity(&mut self, data: ActivityData<C>) {
        let ActivityData {
            activity,
            behaviors,
            conditions,
            memories_to_erase_when_stopped,
        } = data;
        self.state
            .activity_requirements
            .insert(activity, conditions);
        if !memories_to_erase_when_stopped.is_empty() {
            self.state
                .activity_memories_to_erase_when_stopped
                .insert(activity, memories_to_erase_when_stopped);
        }
        for (priority, behavior) in behaviors {
            for ty in behavior.required_memories() {
                self.state.memories.register(ty);
            }
            let by_activity = self.behaviors.entry(priority).or_default();
            match by_activity.iter_mut().find(|(known, _)| *known == activity) {
                Some((_, list)) => list.push(behavior),
                None => by_activity.push((activity, vec![behavior])),
            }
        }
    }

    pub fn activity_behaviors(&self, activity: Activity) -> Vec<(i32, &C)> {
        self.behaviors
            .iter()
            .flat_map(|(priority, by_activity)| {
                by_activity
                    .iter()
                    .filter(move |(candidate, _)| *candidate == activity)
                    .flat_map(move |(_, list)| list.iter().map(move |c| (*priority, c)))
            })
            .collect()
    }

    pub fn remove_all_behaviors(&mut self) {
        self.behaviors.clear();
    }

    pub fn is_brain_dead(&self) -> bool {
        self.state.memories.is_empty() && self.sensors.is_empty() && self.behaviors.is_empty()
    }

    pub fn running_count(&self) -> usize {
        self.behaviors
            .values()
            .flatten()
            .flat_map(|(_, list)| list)
            .filter(|behavior| behavior.status() == Status::Running)
            .count()
    }

    pub fn pack(&self) -> PackedMemories {
        self.state.memories.pack()
    }

    pub fn save(&self) -> Compound {
        self.pack().to_nbt()
    }

    pub fn load(&mut self, nbt: &Compound) {
        self.state.memories.load(&PackedMemories::from_nbt(nbt));
    }

    pub fn update_activity_from_schedule<W: AiWorld>(&mut self, ai: &AiContext<'_, W>) {
        self.state
            .update_activity_from_schedule(ai.world.clock_time(), ai.world.game_time());
    }

    pub fn tick<W: AiWorld>(&mut self, ai: &mut AiContext<'_, W>) {
        let now = ai.world.game_time();
        self.state.memories.forget_outdated();
        for slot in &mut self.sensors {
            slot.tick(ai, &mut self.state.memories);
        }
        self.start_each_non_running(ai, now);
        self.tick_each_running(ai, now);
    }

    pub fn stop_all<W: AiWorld>(&mut self, ai: &mut AiContext<'_, W>) {
        let now = ai.world.game_time();
        for (priority, entry, index) in self.running_behaviors() {
            if let Some(behavior) = behavior_mut(&mut self.behaviors, priority, entry, index) {
                let mut cx = BehaviorCx {
                    ai: &mut *ai,
                    brain: &mut self.state,
                };
                behavior.do_stop(&mut cx, now);
            }
        }
    }

    fn start_each_non_running<W: AiWorld>(&mut self, ai: &mut AiContext<'_, W>, now: i64) {
        for by_activity in self.behaviors.values_mut() {
            for (activity, list) in by_activity.iter_mut() {
                if !self.state.is_active(*activity) {
                    continue;
                }
                for behavior in list.iter_mut() {
                    if behavior.status() == Status::Stopped {
                        let mut cx = BehaviorCx {
                            ai: &mut *ai,
                            brain: &mut self.state,
                        };
                        behavior.try_start(&mut cx, now);
                    }
                }
            }
        }
    }

    fn tick_each_running<W: AiWorld>(&mut self, ai: &mut AiContext<'_, W>, now: i64) {
        for (priority, entry, index) in self.running_behaviors() {
            if let Some(behavior) = behavior_mut(&mut self.behaviors, priority, entry, index) {
                let mut cx = BehaviorCx {
                    ai: &mut *ai,
                    brain: &mut self.state,
                };
                behavior.tick_or_stop(&mut cx, now);
            }
        }
    }

    fn running_behaviors(&self) -> Vec<(i32, usize, usize)> {
        let mut running = Vec::new();
        for (priority, by_activity) in &self.behaviors {
            for (entry, (_, list)) in by_activity.iter().enumerate() {
                for (index, behavior) in list.iter().enumerate() {
                    if behavior.status() == Status::Running {
                        running.push((*priority, entry, index));
                    }
                }
            }
        }
        running
    }
}

fn behavior_mut<C>(
    behaviors: &mut BTreeMap<i32, Vec<(Activity, Vec<C>)>>,
    priority: i32,
    entry: usize,
    index: usize,
) -> Option<&mut C> {
    behaviors
        .get_mut(&priority)?
        .get_mut(entry)?
        .1
        .get_mut(index)
}
