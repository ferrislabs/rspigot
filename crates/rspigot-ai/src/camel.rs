use rspigot_domain::entity::species::camel;
use rspigot_entities::EntityKind;

use crate::action::MobAction;
use crate::behavior::{
    Activity, ActivityData, AdultSensor, AnimalMakeLove, AnimalPanic, BabyFollowAdult, Behavior,
    BehaviorBrain, BehaviorCx, Condition, DoNothing, EntityFilter, FollowTemptation, Gate,
    HurtBySensor, MemoryModuleType, MobControl, MobPredicate, MobTrigger,
    NearestLivingEntitySensor, RandomLookAround, RandomStroll, SensorKind, SensorType,
    SetEntityLookTargetSometimes, SetWalkTargetFromLookTarget, TemptDistance, TemptingSensor,
    UniformInt,
};
use crate::brain::AiContext;
use crate::control::Operation;
use crate::goals::TemptItems;
use crate::world::AiWorld;

pub const ADULT_FOLLOW_RANGE: UniformInt = UniformInt::of(5, 16);
pub const LOOK_AT_PLAYER_DISTANCE: f32 = 6.0;
pub const SET_WALK_CLOSE_ENOUGH: i32 = 3;
pub const LOOK_AROUND_MIN_PITCH: f32 = 0.0;
pub const LOOK_AROUND_MAX_YAW: f32 = 30.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CamelFacts {
    pub sitting: bool,
    pub in_transition: bool,
    pub pose_time: i64,
    pub mob_controlled: bool,
    pub controlled_by_player: bool,
    pub can_change_pose: bool,
}

impl CamelFacts {
    pub fn refuse_to_move(&self) -> bool {
        self.sitting || self.in_transition
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CamelAction {
    SitDown,
    StandUp,
    StandUpInstantly,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CamelPanic {
    inner: AnimalPanic,
}

impl CamelPanic {
    pub fn new(speed_multiplier: f32) -> Self {
        Self {
            inner: AnimalPanic::new(speed_multiplier),
        }
    }
}

impl Behavior for CamelPanic {
    fn entry_condition(&self) -> &[Condition] {
        self.inner.entry_condition()
    }

    fn check_extra_start_conditions<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>) -> bool {
        self.inner.check_extra_start_conditions(cx) && !cx.ai.mob.species.camel.mob_controlled
    }

    fn start<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, now: i64) {
        cx.ai.act(MobAction::Camel(CamelAction::StandUpInstantly));
        self.inner.start(cx, now);
    }

    fn can_still_use<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, now: i64) -> bool {
        self.inner.can_still_use(cx, now)
    }

    fn tick<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, now: i64) {
        self.inner.tick(cx, now);
    }

    fn stop<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, now: i64) {
        self.inner.stop(cx, now);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RandomSitting {
    minimal_pose_ticks: i32,
}

impl RandomSitting {
    pub const fn new(minimal_pose_seconds: i32) -> Self {
        Self {
            minimal_pose_ticks: minimal_pose_seconds * 20,
        }
    }
}

impl Behavior for RandomSitting {
    fn entry_condition(&self) -> &[Condition] {
        &[]
    }

    fn check_extra_start_conditions<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>) -> bool {
        let mob = cx.ai.mob;
        let facts = mob.species.camel;
        !mob.fluid.in_water
            && facts.pose_time >= i64::from(self.minimal_pose_ticks)
            && !mob.leashed
            && mob.on_ground
            && !facts.controlled_by_player
            && facts.can_change_pose
    }

    fn start<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        let panicking = cx.brain.memories.has_value(MemoryModuleType::IsPanicking);
        if cx.ai.mob.species.camel.sitting {
            cx.ai.act(MobAction::Camel(CamelAction::StandUp));
        } else if !panicking {
            cx.ai.act(MobAction::Camel(CamelAction::SitDown));
        }
    }
}

pub fn food_tag(kind: EntityKind) -> &'static str {
    camel::food_tag(kind)
}

pub fn camel_sensors(kind: EntityKind) -> Vec<SensorKind> {
    vec![
        SensorKind::NearestLivingEntities(NearestLivingEntitySensor),
        SensorKind::HurtBy(HurtBySensor),
        SensorKind::Tempting(TemptingSensor::of(
            SensorType::FoodTemptations,
            TemptItems::Tag(food_tag(kind)),
        )),
        SensorKind::NearestAdult(AdultSensor),
    ]
}

fn core_activity() -> ActivityData<MobControl> {
    ActivityData::create(
        Activity::Core,
        0,
        vec![
            MobControl::swim(camel::SWIM_CHANCE),
            MobControl::behavior_for(
                crate::behavior::MobBehavior::CamelPanic(CamelPanic::new(camel::PANIC_SPEED)),
                AnimalPanic::min_duration(),
                AnimalPanic::max_duration(),
            ),
            MobControl::default_look_at_target_sink(),
            MobControl::move_to_target_sink(),
            MobControl::count_down_cooldown_ticks(MemoryModuleType::TemptationCooldownTicks),
            MobControl::count_down_cooldown_ticks(MemoryModuleType::GazeCooldownTicks),
        ],
    )
}

fn unless_refusing(trigger: MobTrigger) -> MobControl {
    MobControl::trigger_if(
        MobPredicate::CamelRefusesToMove.negate(),
        MobControl::declared(trigger),
    )
}

fn idle_activity() -> ActivityData<MobControl> {
    let follow = MobControl::Gate(Gate::run_one(vec![
        (
            MobControl::follow_temptation(FollowTemptation::with_distance(
                camel::BABY_FOLLOW_SPEED,
                TemptDistance::ByAge {
                    baby: camel::BABY_TEMPT_DISTANCE,
                    adult: camel::ADULT_TEMPT_DISTANCE,
                },
                false,
            )),
            1,
        ),
        (
            unless_refusing(MobTrigger::BabyFollowAdult(BabyFollowAdult::new(
                ADULT_FOLLOW_RANGE,
                camel::BABY_FOLLOW_SPEED,
            ))),
            1,
        ),
    ]));
    let wander = MobControl::Gate(Gate::run_one_when(
        vec![(
            MemoryModuleType::WalkTarget,
            crate::behavior::MemoryStatus::ValueAbsent,
        )],
        vec![
            (
                unless_refusing(MobTrigger::RandomStroll(RandomStroll::stroll(
                    camel::WALK_SPEED as f32,
                ))),
                1,
            ),
            (
                unless_refusing(MobTrigger::SetWalkTargetFromLookTarget(
                    SetWalkTargetFromLookTarget::new(
                        camel::WALK_SPEED as f32,
                        SET_WALK_CLOSE_ENOUGH,
                    ),
                )),
                1,
            ),
            (
                MobControl::behavior(crate::behavior::MobBehavior::CamelRandomSitting(
                    RandomSitting::new(camel::RANDOM_SITTING_SECONDS),
                )),
                1,
            ),
            (MobControl::DoNothing(DoNothing::new(30, 60)), 1),
        ],
    ));
    ActivityData::create(
        Activity::Idle,
        0,
        vec![
            MobControl::one_shot(MobTrigger::SetEntityLookTargetSometimes(
                SetEntityLookTargetSometimes::new(
                    EntityFilter::Kind(EntityKind::Player),
                    LOOK_AT_PLAYER_DISTANCE,
                    UniformInt::of(30, 60),
                ),
            )),
            MobControl::animal_make_love(AnimalMakeLove::new(EntityKind::Camel)),
            follow,
            MobControl::random_look_around(RandomLookAround::new(
                UniformInt::of(150, 250),
                LOOK_AROUND_MAX_YAW,
                LOOK_AROUND_MIN_PITCH,
                0.0,
            )),
            wander,
        ],
    )
}

pub fn camel_activities() -> Vec<ActivityData<MobControl>> {
    vec![core_activity(), idle_activity()]
}

pub fn update_activity(brain: &mut BehaviorBrain<MobControl>) {
    brain
        .state_mut()
        .set_active_activity_to_first_valid(&[Activity::Idle]);
}

pub fn after_brain_tick<W: AiWorld>(
    brain: &mut BehaviorBrain<MobControl>,
    ai: &mut AiContext<'_, W>,
) {
    update_activity(brain);
    let facts = ai.mob.species.camel;
    if ai.brain.movement.operation() == Operation::MoveTo
        && !ai.mob.leashed
        && facts.sitting
        && !facts.in_transition
        && facts.can_change_pose
    {
        ai.act(MobAction::Camel(CamelAction::StandUp));
    }
}

pub fn can_mate(kind: EntityKind, partner: EntityKind) -> bool {
    camel::can_mate(kind, partner)
}
