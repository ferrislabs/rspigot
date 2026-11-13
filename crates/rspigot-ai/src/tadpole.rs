use rspigot_entities::EntityKind;

use crate::behavior::{
    Activity, ActivityData, BehaviorBrain, EntityFilter, FollowTemptation, Gate, HurtBySensor,
    MemoryModuleType, MemoryStatus, MobControl, MobPredicate, MobTrigger,
    NearestLivingEntitySensor, OrderPolicy, PlayerSensor, RandomStroll, RunningPolicy, SensorKind,
    SensorType, SetEntityLookTargetSometimes, SetWalkTargetFromLookTarget, TemptingSensor,
    UniformInt,
};
use crate::frog::FROG_FOOD;
use crate::goals::TemptItems;

pub const PANIC_SPEED: f32 = 2.0;
pub const TEMPT_SPEED: f32 = 1.25;
pub const SWIM_SPEED: f32 = 0.5;
pub const LOOK_SPEED: f32 = 0.5;

pub fn tadpole_sensors() -> Vec<SensorKind> {
    vec![
        SensorKind::NearestLivingEntities(NearestLivingEntitySensor),
        SensorKind::NearestPlayers(PlayerSensor),
        SensorKind::HurtBy(HurtBySensor),
        SensorKind::Tempting(TemptingSensor::of(
            SensorType::FrogTemptations,
            TemptItems::Tag(FROG_FOOD),
        )),
    ]
}

fn core_activity() -> ActivityData<MobControl> {
    ActivityData::create(
        Activity::Core,
        0,
        vec![
            MobControl::animal_panic(PANIC_SPEED),
            MobControl::look_at_target_sink(45, 90),
            MobControl::move_to_target_sink(),
            MobControl::count_down_cooldown_ticks(MemoryModuleType::TemptationCooldownTicks),
        ],
    )
}

fn idle_activity() -> ActivityData<MobControl> {
    ActivityData::from_pairs(
        Activity::Idle,
        vec![
            (
                0,
                MobControl::one_shot(MobTrigger::SetEntityLookTargetSometimes(
                    SetEntityLookTargetSometimes::new(
                        EntityFilter::Kind(EntityKind::Player),
                        6.0,
                        UniformInt::of(30, 60),
                    ),
                )),
            ),
            (
                1,
                MobControl::follow_temptation(FollowTemptation::new(TEMPT_SPEED)),
            ),
            (
                2,
                MobControl::Gate(Gate::new(
                    vec![(MemoryModuleType::WalkTarget, MemoryStatus::ValueAbsent)],
                    Vec::new(),
                    OrderPolicy::Ordered,
                    RunningPolicy::TryAll,
                    vec![
                        (
                            MobControl::one_shot(MobTrigger::RandomStroll(RandomStroll::swim(
                                SWIM_SPEED,
                            ))),
                            2,
                        ),
                        (
                            MobControl::one_shot(MobTrigger::SetWalkTargetFromLookTarget(
                                SetWalkTargetFromLookTarget::new(LOOK_SPEED, 3),
                            )),
                            3,
                        ),
                        (
                            MobControl::one_shot(MobTrigger::Condition(MobPredicate::InWater)),
                            5,
                        ),
                    ],
                )),
            ),
        ],
    )
}

pub fn tadpole_activities() -> Vec<ActivityData<MobControl>> {
    vec![core_activity(), idle_activity()]
}

pub fn update_activity(brain: &mut BehaviorBrain<MobControl>) {
    brain
        .state_mut()
        .set_active_activity_to_first_valid(&[Activity::Idle]);
}
