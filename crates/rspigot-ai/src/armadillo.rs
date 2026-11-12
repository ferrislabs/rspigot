use rspigot_domain::entity::species::armadillo::{self, State};
use rspigot_entities::EntityKind;

use crate::action::{BrainMessage, MobAction};
use crate::behavior::{
    Activity, ActivityData, AdultSensor, AnimalMakeLove, AnimalPanic, BabyFollowAdult, Behavior,
    BehaviorBrain, BehaviorCx, Condition, DoNothing, EntityFilter, FollowTemptation, Gate,
    HurtBySensor, Memories, MemoryModuleType, MemoryStatus, MemoryValue, MobControl, MobTrigger,
    NearestLivingEntitySensor, RandomLookAround, RandomStroll, Sensor, SensorKind, SensorType,
    SetEntityLookTargetSometimes, SetWalkTargetFromLookTarget, TemptDistance, TemptingSensor,
    Trigger, UniformInt,
};
use crate::brain::AiContext;
use crate::goals::TemptItems;
use crate::random::RandomSource;
use crate::world::{AiWorld, Living, MobView, inflate};

pub const ADULT_FOLLOW_RANGE: UniformInt = UniformInt::of(5, 16);
pub const ARMADILLO_FOOD: &str = "minecraft:armadillo_food";
pub const PANIC_ENVIRONMENTAL_CAUSES: &str = "minecraft:panic_environmental_causes";
pub const UNDEAD: &str = "minecraft:undead";
pub const SCARE_SCAN_RATE: i32 = 5;
const SCARE_RANGE_XZ: f64 = 7.0;
const SCARE_RANGE_Y: f64 = 2.0;
const PEEK_MIN_DELAY: i32 = 100;
const PEEK_MAX_DELAY: i32 = 400;
const BALL_UP_STAY_IN_STATE: i32 = armadillo::ROLL_UP_STAY_TICKS;
const ROLL_SOUND: &str = "minecraft:entity.armadillo.roll";
const UNROLL_START_SOUND: &str = "minecraft:entity.armadillo.unroll_start";
const UNROLL_FINISH_SOUND: &str = "minecraft:entity.armadillo.unroll_finish";
const LAND_SOUND: &str = "minecraft:entity.armadillo.land";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ArmadilloFacts {
    pub state: State,
    pub in_state_ticks: i32,
}

impl ArmadilloFacts {
    pub fn switch_to(&mut self, next: State) {
        if self.state != next {
            self.state = next;
            self.in_state_ticks = 0;
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArmadilloAction {
    ResetLove,
}

fn is_scared<W: AiWorld>(cx: &BehaviorCx<'_, '_, W>) -> bool {
    cx.ai.body.armadillo.state.is_threatened()
}

pub fn can_stay_rolled_up(mob: &MobView, memories: &Memories) -> bool {
    !memories.has_value(MemoryModuleType::IsPanicking)
        && !mob.fluid.in_liquid()
        && !mob.leashed
        && !mob.passenger
}

fn can_stay<W: AiWorld>(cx: &BehaviorCx<'_, '_, W>) -> bool {
    can_stay_rolled_up(cx.ai.mob, &cx.brain.memories)
}

pub fn roll_up<W: AiWorld>(cx: &mut BehaviorCx<'_, '_, W>) {
    if is_scared(cx) {
        return;
    }
    cx.ai.stop_navigation();
    cx.ai.brain.movement.set_wait();
    let velocity = cx.ai.body.velocity;
    cx.ai.body.velocity = rspigot_domain::Vec3::new(0.0, velocity.y, 0.0);
    cx.ai.act(MobAction::Armadillo(ArmadilloAction::ResetLove));
    cx.ai.act(MobAction::Voice(ROLL_SOUND));
    cx.ai.body.armadillo.switch_to(State::Rolling);
}

pub fn roll_out<W: AiWorld>(cx: &mut BehaviorCx<'_, '_, W>) {
    if is_scared(cx) {
        cx.ai.act(MobAction::Voice(UNROLL_FINISH_SOUND));
        cx.ai.body.armadillo.switch_to(State::Idle);
    }
}

fn time_until_danger_expires<W: AiWorld>(cx: &BehaviorCx<'_, '_, W>) -> i64 {
    if cx
        .brain
        .memories
        .has_value(MemoryModuleType::DangerDetectedRecently)
    {
        cx.brain
            .memories
            .time_until_expiry(MemoryModuleType::DangerDetectedRecently)
            .unwrap_or(0)
    } else {
        0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ArmadilloBallUp {
    next_peek_timer: i32,
    danger_was_around: bool,
}

impl ArmadilloBallUp {
    fn pick_next_peek_timer<W: AiWorld>(cx: &mut BehaviorCx<'_, '_, W>) -> i32 {
        State::Scared.animation_duration()
            + cx.ai
                .brain
                .random
                .next_int_between(PEEK_MIN_DELAY, PEEK_MAX_DELAY)
    }
}

impl Behavior for ArmadilloBallUp {
    fn entry_condition(&self) -> &[Condition] {
        &[]
    }

    fn check_extra_start_conditions<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>) -> bool {
        cx.ai.mob.on_ground && !cx.ai.mob.fluid.in_liquid()
    }

    fn can_still_use<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        cx.ai.body.armadillo.state.is_threatened()
    }

    fn start<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        roll_up(cx);
    }

    fn stop<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        if !can_stay(cx) {
            roll_out(cx);
        }
    }

    fn tick<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        if self.next_peek_timer > 0 {
            self.next_peek_timer -= 1;
        }
        let facts = cx.ai.body.armadillo;
        if facts.state == State::Rolling
            && facts.in_state_ticks > State::Rolling.animation_duration()
        {
            cx.ai.body.armadillo.switch_to(State::Scared);
            if cx.ai.mob.on_ground {
                cx.ai.act(MobAction::Voice(LAND_SOUND));
            }
            return;
        }
        let ttl = time_until_danger_expires(cx);
        let danger_around = ttl > armadillo::PEEK_DANGER_THRESHOLD;
        if danger_around != self.danger_was_around {
            self.next_peek_timer = Self::pick_next_peek_timer(cx);
        }
        self.danger_was_around = danger_around;
        let unrolling = i64::from(State::Unrolling.animation_duration());
        match facts.state {
            State::Scared => {
                if self.next_peek_timer == 0 && cx.ai.mob.on_ground && danger_around {
                    cx.ai.act(MobAction::EntityEvent(armadillo::PEEK_EVENT));
                    self.next_peek_timer = Self::pick_next_peek_timer(cx);
                }
                if ttl < unrolling {
                    cx.ai.act(MobAction::Voice(UNROLL_START_SOUND));
                    cx.ai.body.armadillo.switch_to(State::Unrolling);
                }
            }
            State::Unrolling if ttl > unrolling => {
                cx.ai.body.armadillo.switch_to(State::Scared);
            }
            _ => {}
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ArmadilloPanic {
    inner: AnimalPanic,
}

impl ArmadilloPanic {
    pub fn new(speed_multiplier: f32) -> Self {
        Self {
            inner: AnimalPanic::with_causes(speed_multiplier, PANIC_ENVIRONMENTAL_CAUSES),
        }
    }
}

impl Behavior for ArmadilloPanic {
    fn entry_condition(&self) -> &[Condition] {
        self.inner.entry_condition()
    }

    fn check_extra_start_conditions<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>) -> bool {
        self.inner.check_extra_start_conditions(cx)
    }

    fn start<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, now: i64) {
        roll_out(cx);
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RollOutWithoutDanger;

impl RollOutWithoutDanger {
    pub fn conditions(&self) -> Vec<Condition> {
        vec![(
            MemoryModuleType::DangerDetectedRecently,
            MemoryStatus::ValueAbsent,
        )]
    }
}

impl Trigger for RollOutWithoutDanger {
    fn trigger<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        if is_scared(cx) {
            roll_out(cx);
            return true;
        }
        false
    }
}

fn is_undead(kind: EntityKind) -> bool {
    rspigot_data::tags::tag("minecraft:entity_type", UNDEAD)
        .is_some_and(|ids| ids.binary_search(&u32::from(kind.id().0)).is_ok())
}

pub fn is_scared_by(mob: &MobView, entity: &Living) -> bool {
    let reach = inflate(&mob.bounds(), SCARE_RANGE_XZ, SCARE_RANGE_Y, SCARE_RANGE_XZ);
    if !reach.intersects(&entity.bounds) {
        return false;
    }
    if is_undead(entity.kind) {
        return true;
    }
    if mob.last_hurt_by_mob == Some(entity.id) {
        return true;
    }
    entity.player && !entity.spectator && (entity.facts.sprinting || entity.facts.passenger)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ArmadilloScareSensor;

impl Sensor for ArmadilloScareSensor {
    fn scan_rate(&self) -> i32 {
        SCARE_SCAN_RATE
    }

    fn requires(&self) -> &'static [MemoryModuleType] {
        &[MemoryModuleType::NearestLivingEntities]
    }

    fn do_tick<W: AiWorld>(&mut self, cx: &mut AiContext<'_, W>, memories: &mut Memories) {
        if !can_stay_rolled_up(cx.mob, memories) {
            memories.erase(MemoryModuleType::DangerDetectedRecently);
            return;
        }
        let nearby = memories
            .get(MemoryModuleType::NearestLivingEntities)
            .and_then(MemoryValue::as_entities)
            .map(<[_]>::to_vec);
        let Some(nearby) = nearby else {
            return;
        };
        let scared = nearby.iter().any(|id| {
            cx.world
                .living(*id)
                .is_some_and(|entity| is_scared_by(cx.mob, &entity))
        });
        if scared {
            memories.set_with_expiry(
                MemoryModuleType::DangerDetectedRecently,
                MemoryValue::Bool(true),
                armadillo::DANGER_TICKS,
            );
        }
    }
}

pub fn armadillo_sensors() -> Vec<SensorKind> {
    vec![
        SensorKind::NearestLivingEntities(NearestLivingEntitySensor),
        SensorKind::HurtBy(HurtBySensor),
        SensorKind::Tempting(TemptingSensor::of(
            SensorType::FoodTemptations,
            TemptItems::Tag(ARMADILLO_FOOD),
        )),
        SensorKind::NearestAdult(AdultSensor),
        SensorKind::ArmadilloScareDetected(ArmadilloScareSensor),
    ]
}

fn core_activity() -> ActivityData<MobControl> {
    ActivityData::create(
        Activity::Core,
        0,
        vec![
            MobControl::swim(0.8),
            MobControl::behavior_for(
                crate::behavior::MobBehavior::ArmadilloPanic(ArmadilloPanic::new(2.0)),
                AnimalPanic::min_duration(),
                AnimalPanic::max_duration(),
            ),
            MobControl::default_look_at_target_sink(),
            MobControl::move_to_target_sink(),
            MobControl::count_down_cooldown_ticks(MemoryModuleType::TemptationCooldownTicks),
            MobControl::count_down_cooldown_ticks(MemoryModuleType::GazeCooldownTicks),
            MobControl::one_shot(MobTrigger::ArmadilloRollOut(RollOutWithoutDanger)),
        ],
    )
}

fn idle_activity() -> ActivityData<MobControl> {
    let follow = MobControl::Gate(Gate::run_one(vec![
        (
            MobControl::follow_temptation(FollowTemptation::with_distance(
                1.25,
                TemptDistance::ByAge {
                    baby: 1.0,
                    adult: 2.0,
                },
                false,
            )),
            1,
        ),
        (
            MobControl::one_shot(MobTrigger::BabyFollowAdult(BabyFollowAdult::new(
                ADULT_FOLLOW_RANGE,
                1.25,
            ))),
            1,
        ),
    ]));
    let wander = MobControl::Gate(Gate::run_one_when(
        vec![(MemoryModuleType::WalkTarget, MemoryStatus::ValueAbsent)],
        vec![
            (
                MobControl::one_shot(MobTrigger::RandomStroll(RandomStroll::stroll(1.0))),
                1,
            ),
            (
                MobControl::one_shot(MobTrigger::SetWalkTargetFromLookTarget(
                    SetWalkTargetFromLookTarget::new(1.0, 3),
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
                    6.0,
                    UniformInt::of(30, 60),
                ),
            )),
            MobControl::animal_make_love(AnimalMakeLove::with_walk(EntityKind::Armadillo, 1.0, 1)),
            follow,
            MobControl::random_look_around(RandomLookAround::new(
                UniformInt::of(150, 250),
                30.0,
                0.0,
                0.0,
            )),
            wander,
        ],
    )
}

fn panic_activity() -> ActivityData<MobControl> {
    ActivityData::with_conditions(
        Activity::Panic,
        vec![(
            0,
            MobControl::behavior_for(
                crate::behavior::MobBehavior::ArmadilloBallUp(ArmadilloBallUp::default()),
                BALL_UP_STAY_IN_STATE,
                BALL_UP_STAY_IN_STATE,
            ),
        )],
        vec![
            (
                MemoryModuleType::DangerDetectedRecently,
                MemoryStatus::ValuePresent,
            ),
            (MemoryModuleType::IsPanicking, MemoryStatus::ValueAbsent),
        ],
    )
}

pub fn armadillo_activities() -> Vec<ActivityData<MobControl>> {
    vec![core_activity(), idle_activity(), panic_activity()]
}

pub fn update_activity(brain: &mut BehaviorBrain<MobControl>) {
    brain
        .state_mut()
        .set_active_activity_to_first_valid(&[Activity::Panic, Activity::Idle]);
}

pub fn deliver<W: AiWorld>(cx: &mut BehaviorCx<'_, '_, W>, message: BrainMessage) {
    if let BrainMessage::ArmadilloHurt {
        by_living,
        environmental,
    } = message
    {
        if by_living {
            cx.brain.memories.set_with_expiry(
                MemoryModuleType::DangerDetectedRecently,
                MemoryValue::Bool(true),
                armadillo::DANGER_TICKS,
            );
            if can_stay(cx) {
                roll_up(cx);
            }
        } else if environmental {
            roll_out(cx);
        }
    }
}
