use rspigot_domain::EntityId;
use rspigot_domain::entity::species::axolotl as species;
use rspigot_entities::EntityKind;

use crate::action::MobAction;
use crate::amphibious::{AXOLOTL_TRIES_TO_FIND, TryFindLiquid};
use crate::behavior::{
    Activity, ActivityData, AdultSensor, AnimalMakeLove, BabyFollowAdult, Behavior, BehaviorBrain,
    BehaviorCx, Condition, EntityFilter, EraseMemoryIf, FollowTemptation, Gate, HurtBySensor,
    Memories, MemoryModuleType, MemoryStatus, MemoryValue, MobControl, MobPredicate, MobTrigger,
    NearestLivingEntitySensor, OrderPolicy, RandomStroll, RunningPolicy, Sensor, SensorKind,
    SensorType, SetEntityLookTargetSometimes, SetWalkTargetFromAttackTargetIfTargetOutOfReach,
    SetWalkTargetFromLookTarget, StartAttacking, StopAttackCondition, StopAttackingIfTargetInvalid,
    TargetErased, TargetFinder, TemptingSensor, Trigger, UniformInt, is_entity_attackable,
};
use crate::brain::AiContext;
use crate::goals::TemptItems;
use crate::world::{AiWorld, Living};

pub const ADULT_FOLLOW_RANGE: UniformInt = UniformInt::of(5, 16);
pub const PLAY_DEAD_DURATION: i32 = 200;
pub const MELEE_COOLDOWN: i32 = 20;
pub const ATTACKABLE_RANGE_SQR: f64 = 64.0;
pub const WATER_SPEED: f32 = 0.5;
pub const LAND_SPEED: f32 = 0.15;
pub const CHASE_WATER_SPEED: f32 = 0.6;
pub const FOLLOW_ADULT_WATER_SPEED: f32 = 0.6;
pub const MAKE_LOVE_SPEED: f32 = 0.2;
pub const MAKE_LOVE_DISTANCE: i32 = 2;
pub const FIND_LIQUID_RANGE: i32 = 6;
pub const SWIM_STROLL_SPEED: f32 = 0.5;
pub const HUNT_TARGETS: &str = "minecraft:axolotl_hunt_targets";
pub const ALWAYS_HOSTILES: &str = "minecraft:axolotl_always_hostiles";
pub const AXOLOTL_FOOD: &str = "minecraft:axolotl_food";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AxolotlAction {
    PlayDeadStarted,
    TargetErased(EntityId),
}

pub const AXOLOTL_MEMORIES: [MemoryModuleType; 0] = [];

fn in_entity_tag(kind: EntityKind, tag: &str) -> bool {
    rspigot_data::tags::tag("minecraft:entity_type", tag)
        .is_some_and(|ids| ids.binary_search(&u32::from(kind.id().0)).is_ok())
}

pub fn axolotl_sensors() -> Vec<SensorKind> {
    vec![
        SensorKind::NearestLivingEntities(NearestLivingEntitySensor),
        SensorKind::NearestAdult(AdultSensor),
        SensorKind::HurtBy(HurtBySensor),
        SensorKind::AxolotlAttackables(AxolotlAttackablesSensor),
        SensorKind::Tempting(TemptingSensor::of(
            SensorType::FoodTemptations,
            TemptItems::Tag(AXOLOTL_FOOD),
        )),
    ]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AxolotlAttackablesSensor;

impl Sensor for AxolotlAttackablesSensor {
    fn requires(&self) -> &'static [MemoryModuleType] {
        &[
            MemoryModuleType::NearestAttackable,
            MemoryModuleType::NearestVisibleLivingEntities,
            MemoryModuleType::HasHuntingCooldown,
        ]
    }

    fn do_tick<W: AiWorld>(&mut self, cx: &mut AiContext<'_, W>, memories: &mut Memories) {
        let hunting_on_cooldown = memories.has_value(MemoryModuleType::HasHuntingCooldown);
        let nearby: Vec<EntityId> = memories
            .get(MemoryModuleType::NearestVisibleLivingEntities)
            .and_then(MemoryValue::as_visible)
            .map(|visible| visible.nearby_entities().to_vec())
            .unwrap_or_default();
        let origin = cx.mob.position;
        let mut found = None;
        for id in nearby {
            let Some(target) = cx.world.living(id) else {
                continue;
            };
            if !Self::matches(&target, origin, hunting_on_cooldown)
                || !is_entity_attackable(cx, memories, &target)
            {
                continue;
            }
            let visible = memories
                .get(MemoryModuleType::NearestVisibleLivingEntities)
                .and_then(MemoryValue::as_visible)
                .is_some_and(|visible| visible.contains_entity(cx, memories, id));
            if visible {
                found = Some(id);
                break;
            }
        }
        memories.set_option(
            MemoryModuleType::NearestAttackable,
            found.map(MemoryValue::Entity),
        );
    }
}

impl AxolotlAttackablesSensor {
    fn matches(target: &Living, origin: rspigot_domain::Vec3, hunting_on_cooldown: bool) -> bool {
        let close = target.position.distance_squared(origin) <= ATTACKABLE_RANGE_SQR;
        let hostile = in_entity_tag(target.kind, ALWAYS_HOSTILES);
        let hunt = !hunting_on_cooldown && in_entity_tag(target.kind, HUNT_TARGETS);
        close && target.in_water && (hostile || hunt)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PlayDead;

impl PlayDead {
    const CONDITION: [Condition; 2] = [
        (MemoryModuleType::PlayDeadTicks, MemoryStatus::ValuePresent),
        (MemoryModuleType::HurtByEntity, MemoryStatus::ValuePresent),
    ];
}

impl Behavior for PlayDead {
    fn entry_condition(&self) -> &[Condition] {
        &Self::CONDITION
    }

    fn check_extra_start_conditions<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>) -> bool {
        cx.ai.mob.fluid.in_water
    }

    fn can_still_use<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        cx.ai.mob.fluid.in_water && cx.brain.memories.has_value(MemoryModuleType::PlayDeadTicks)
    }

    fn start<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        cx.brain.memories.erase(MemoryModuleType::WalkTarget);
        cx.brain.memories.erase(MemoryModuleType::LookTarget);
        cx.ai
            .act(MobAction::Axolotl(AxolotlAction::PlayDeadStarted));
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ValidatePlayDead;

impl ValidatePlayDead {
    pub fn conditions(&self) -> Vec<Condition> {
        vec![
            (MemoryModuleType::PlayDeadTicks, MemoryStatus::ValuePresent),
            (MemoryModuleType::HurtByEntity, MemoryStatus::Registered),
        ]
    }
}

impl Trigger for ValidatePlayDead {
    fn trigger<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        let ticks = cx
            .brain
            .memories
            .get(MemoryModuleType::PlayDeadTicks)
            .and_then(MemoryValue::as_int)
            .unwrap_or(0);
        if ticks < 1 {
            cx.brain.memories.erase(MemoryModuleType::PlayDeadTicks);
            cx.brain.memories.erase(MemoryModuleType::HurtByEntity);
            cx.brain.use_default_activity();
        } else {
            cx.brain
                .memories
                .set(MemoryModuleType::PlayDeadTicks, MemoryValue::Int(ticks - 1));
        }
        true
    }
}

fn is_breeding() -> MobPredicate {
    MobPredicate::HasMemory(MemoryModuleType::BreedTarget)
}

fn core_activity() -> ActivityData<MobControl> {
    ActivityData::create(
        Activity::Core,
        0,
        vec![
            MobControl::look_at_target_sink(45, 90),
            MobControl::move_to_target_sink(),
            MobControl::one_shot(MobTrigger::AxolotlValidatePlayDead(ValidatePlayDead)),
            MobControl::count_down_cooldown_ticks(MemoryModuleType::TemptationCooldownTicks),
        ],
    )
}

fn idle_activity() -> ActivityData<MobControl> {
    let follow = MobControl::Gate(Gate::run_one(vec![
        (
            MobControl::follow_temptation(FollowTemptation::new(LAND_SPEED).in_water(WATER_SPEED)),
            1,
        ),
        (
            MobControl::one_shot(MobTrigger::BabyFollowAdult(
                BabyFollowAdult::new(ADULT_FOLLOW_RANGE, LAND_SPEED)
                    .in_water(FOLLOW_ADULT_WATER_SPEED),
            )),
            1,
        ),
    ]));
    let wander = MobControl::Gate(Gate::new(
        vec![(MemoryModuleType::WalkTarget, MemoryStatus::ValueAbsent)],
        Vec::new(),
        OrderPolicy::Ordered,
        RunningPolicy::TryAll,
        vec![
            (
                MobControl::one_shot(MobTrigger::RandomStroll(RandomStroll::swim(
                    SWIM_STROLL_SPEED,
                ))),
                2,
            ),
            (
                MobControl::one_shot(MobTrigger::RandomStroll(RandomStroll::stroll_from_water(
                    LAND_SPEED, false,
                ))),
                2,
            ),
            (
                MobControl::one_shot(MobTrigger::SetWalkTargetFromLookTarget(
                    SetWalkTargetFromLookTarget::new(LAND_SPEED, 3)
                        .in_water(WATER_SPEED)
                        .only_when_water_matches(),
                )),
                3,
            ),
            (
                MobControl::one_shot(MobTrigger::Condition(MobPredicate::InWater)),
                5,
            ),
            (
                MobControl::one_shot(MobTrigger::Condition(MobPredicate::OnGround)),
                5,
            ),
        ],
    ));
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
                MobControl::animal_make_love(AnimalMakeLove::with_walk(
                    EntityKind::Axolotl,
                    MAKE_LOVE_SPEED,
                    MAKE_LOVE_DISTANCE,
                )),
            ),
            (2, follow),
            (
                3,
                MobControl::one_shot(MobTrigger::StartAttacking(StartAttacking::when(
                    is_breeding().negate(),
                    TargetFinder::Memory(MemoryModuleType::NearestAttackable),
                ))),
            ),
            (
                3,
                MobControl::one_shot(MobTrigger::TryFindLiquid(TryFindLiquid::new(
                    FIND_LIQUID_RANGE,
                    LAND_SPEED,
                    AXOLOTL_TRIES_TO_FIND,
                ))),
            ),
            (4, wander),
        ],
    )
}

fn fight_activity() -> ActivityData<MobControl> {
    ActivityData::create_erasing(
        Activity::Fight,
        0,
        vec![
            MobControl::one_shot(MobTrigger::StopAttackingIfTargetInvalid(
                StopAttackingIfTargetInvalid::new(
                    StopAttackCondition::Never,
                    TargetErased::Axolotl,
                    true,
                ),
            )),
            MobControl::one_shot(MobTrigger::SetWalkTargetFromAttackTargetIfTargetOutOfReach(
                SetWalkTargetFromAttackTargetIfTargetOutOfReach::new(LAND_SPEED)
                    .in_water(CHASE_WATER_SPEED),
            )),
            MobControl::one_shot(MobTrigger::MeleeAttack(crate::behavior::MeleeAttack::new(
                MELEE_COOLDOWN,
            ))),
            MobControl::one_shot(MobTrigger::EraseMemoryIf(EraseMemoryIf::new(
                is_breeding(),
                MemoryModuleType::AttackTarget,
            ))),
        ],
        MemoryModuleType::AttackTarget,
    )
}

fn play_dead_activity() -> ActivityData<MobControl> {
    let mut data = ActivityData::from_pairs(
        Activity::PlayDead,
        vec![
            (
                0,
                MobControl::behavior_for(
                    crate::behavior::MobBehavior::AxolotlPlayDead(PlayDead),
                    PLAY_DEAD_DURATION,
                    PLAY_DEAD_DURATION,
                ),
            ),
            (
                1,
                MobControl::one_shot(MobTrigger::EraseMemoryIf(EraseMemoryIf::new(
                    is_breeding(),
                    MemoryModuleType::PlayDeadTicks,
                ))),
            ),
        ],
    );
    data.conditions = vec![(MemoryModuleType::PlayDeadTicks, MemoryStatus::ValuePresent)];
    data.memories_to_erase_when_stopped = vec![MemoryModuleType::PlayDeadTicks];
    data
}

pub fn axolotl_activities() -> Vec<ActivityData<MobControl>> {
    vec![
        core_activity(),
        idle_activity(),
        fight_activity(),
        play_dead_activity(),
    ]
}

pub fn update_activity(brain: &mut BehaviorBrain<MobControl>) {
    let previous = brain.state().active_non_core_activity();
    if previous == Some(Activity::PlayDead) {
        return;
    }
    brain.state_mut().set_active_activity_to_first_valid(&[
        Activity::PlayDead,
        Activity::Fight,
        Activity::Idle,
    ]);
    if previous == Some(Activity::Fight)
        && brain.state().active_non_core_activity() != Some(Activity::Fight)
    {
        brain.memories_mut().set_with_expiry(
            MemoryModuleType::HasHuntingCooldown,
            MemoryValue::Bool(true),
            species::HUNTING_COOLDOWN,
        );
    }
}
