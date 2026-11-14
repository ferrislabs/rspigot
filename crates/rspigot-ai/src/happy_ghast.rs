use rspigot_domain::entity::species::happy_ghast;

use crate::behavior::{
    Activity, ActivityData, AdultSensorAnyType, BabyFollowAdult, BehaviorBrain, FollowTemptation,
    Gate, HurtBySensor, MemoryModuleType, MemoryStatus, MobControl, MobTrigger,
    NearestLivingEntitySensor, PlayerSensor, RandomStroll, SensorKind, SensorType,
    SetWalkTargetFromLookTarget, TemptDistance, TemptingSensor, UniformInt,
};
use crate::brain::AiContext;
use crate::ghast::{GhastGoal, RandomFloatAroundGoal};
use crate::goal::{Flag, Flags, Goal};
use crate::goals::{FloatGoal, MobGoal, TemptGoal, TemptItems};
use crate::profile::MobProfile;
use crate::registry::Goals;
use crate::world::AiWorld;

pub const ADULT_FOLLOW_RANGE: UniformInt = UniformInt::of(3, 16);
pub const SWIM_CHANCE: f32 = 0.8;
pub const PANIC_SPEED: f32 = 2.0;
pub const PANIC_FLY_HEIGHT: i32 = 0;
pub const TEMPT_SPEED: f32 = 1.25;
pub const TEMPT_CLOSE_ENOUGH: f64 = 3.0;
pub const FOLLOW_SPEED: f32 = 1.1;
pub const STROLL_SPEED: f32 = 1.0;
pub const WALK_TARGET_SPEED: f32 = 1.0;
pub const WALK_TARGET_CLOSE_ENOUGH: i32 = 3;
pub const BABY_FLY_MAX_TURN: i32 = happy_ghast::BABY_FLY_MAX_TURN;
pub const BABY_REQUIRED_PATH_LENGTH: f32 = happy_ghast::BABY_REQUIRED_PATH_LENGTH;
pub const FLOAT_GOAL_PRIORITY: i32 = 3;
pub const TEMPT_GOAL_PRIORITY: i32 = 4;
pub const FLOAT_AROUND_GOAL_PRIORITY: i32 = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct HappyGhastFacts {
    pub wearing_harness: bool,
    pub vehicle: bool,
    pub on_still_timeout: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct HappyGhastBrain {
    pub baby: bool,
    pub vehicle: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct HappyGhastFloatGoal;

impl<W: AiWorld> Goal<AiContext<'_, W>> for HappyGhastFloatGoal {
    fn flags(&self) -> Flags {
        Flags::of(Flag::Jump)
    }

    fn can_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        !context.mob.species.happy_ghast.on_still_timeout && FloatGoal.can_use(context)
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, context: &mut AiContext<'_, W>) {
        FloatGoal.tick(context);
    }
}

pub fn adult_goals(add: &mut Goals<'_>) {
    add.goal(
        FLOAT_GOAL_PRIORITY,
        MobGoal::Ghast(GhastGoal::HappyFloat(HappyGhastFloatGoal)),
    );
    add.goal(
        TEMPT_GOAL_PRIORITY,
        MobGoal::Tempt(
            TemptGoal::new(happy_ghast::TEMPT_SPEED, TemptItems::HappyGhast)
                .with_stop_distance(happy_ghast::TEMPT_STOP_DISTANCE),
        ),
    );
    add.goal(
        FLOAT_AROUND_GOAL_PRIORITY,
        MobGoal::Ghast(GhastGoal::RandomFloatAround(
            RandomFloatAroundGoal::with_distance(happy_ghast::FLOAT_AROUND_DISTANCE),
        )),
    );
}

pub fn profile(baby: bool, width: f32, height: f32) -> MobProfile {
    let mut profile = MobProfile::new(width, height);
    profile.set_malus(crate::path_type::PathType::FireInNeighbor, 16.0);
    profile.set_malus(crate::path_type::PathType::Fire, -1.0);
    if baby {
        profile = profile.flying(true).direct_flight();
    }
    profile
}

pub fn happy_ghast_sensors() -> Vec<SensorKind> {
    vec![
        SensorKind::NearestLivingEntities(NearestLivingEntitySensor),
        SensorKind::HurtBy(HurtBySensor),
        SensorKind::Tempting(TemptingSensor::of(
            SensorType::FoodTemptations,
            TemptItems::Tag(happy_ghast::FOOD_TAG),
        )),
        SensorKind::NearestAdultAnyType(AdultSensorAnyType),
        SensorKind::NearestPlayers(PlayerSensor),
    ]
}

fn core_activity() -> ActivityData<MobControl> {
    ActivityData::create(
        Activity::Core,
        0,
        vec![
            MobControl::swim(SWIM_CHANCE),
            MobControl::animal_panic_flying(PANIC_SPEED, PANIC_FLY_HEIGHT),
            MobControl::default_look_at_target_sink(),
            MobControl::move_to_target_sink(),
            MobControl::count_down_cooldown_ticks(MemoryModuleType::TemptationCooldownTicks),
        ],
    )
}

fn idle_activity() -> ActivityData<MobControl> {
    let wander = MobControl::Gate(Gate::run_one(vec![
        (
            MobControl::one_shot(MobTrigger::RandomStroll(RandomStroll::fly(STROLL_SPEED))),
            1,
        ),
        (
            MobControl::one_shot(MobTrigger::SetWalkTargetFromLookTarget(
                SetWalkTargetFromLookTarget::new(WALK_TARGET_SPEED, WALK_TARGET_CLOSE_ENOUGH),
            )),
            1,
        ),
    ]));
    ActivityData::with_conditions(
        Activity::Idle,
        vec![
            (
                1,
                MobControl::follow_temptation(FollowTemptation::with_distance(
                    TEMPT_SPEED,
                    TemptDistance::Constant(TEMPT_CLOSE_ENOUGH),
                    true,
                )),
            ),
            (
                2,
                MobControl::one_shot(MobTrigger::BabyFollowAdult(BabyFollowAdult::of(
                    ADULT_FOLLOW_RANGE,
                    FOLLOW_SPEED,
                    MemoryModuleType::NearestVisiblePlayer,
                    true,
                ))),
            ),
            (
                3,
                MobControl::one_shot(MobTrigger::BabyFollowAdult(BabyFollowAdult::of(
                    ADULT_FOLLOW_RANGE,
                    FOLLOW_SPEED,
                    MemoryModuleType::NearestVisibleAdult,
                    true,
                ))),
            ),
            (4, wander),
        ],
        Vec::new(),
    )
}

fn panic_activity() -> ActivityData<MobControl> {
    ActivityData::with_conditions(
        Activity::Panic,
        Vec::new(),
        vec![(MemoryModuleType::IsPanicking, MemoryStatus::ValuePresent)],
    )
}

pub fn happy_ghast_activities() -> Vec<ActivityData<MobControl>> {
    vec![core_activity(), idle_activity(), panic_activity()]
}

pub fn update_activity(brain: &mut BehaviorBrain<MobControl>) {
    brain
        .state_mut()
        .set_active_activity_to_first_valid(&[Activity::Panic, Activity::Idle]);
}

pub fn is_baby(
    base: &rspigot_domain::entity::EntityBase,
    living: &rspigot_domain::entity::living::LivingState,
) -> bool {
    living
        .mob
        .as_ref()
        .and_then(|mob| mob.ageable.as_ref())
        .is_some_and(|age| rspigot_domain::entity::ageable::is_baby(base, age))
}

pub fn is_on_still_timeout(
    base: &rspigot_domain::entity::EntityBase,
    living: &rspigot_domain::entity::living::LivingState,
) -> bool {
    match living.mob.as_ref().map(|mob| mob.species) {
        Some(rspigot_domain::entity::species::Species::HappyGhast(state)) => {
            happy_ghast::is_on_still_timeout(base, &state)
        }
        _ => false,
    }
}
