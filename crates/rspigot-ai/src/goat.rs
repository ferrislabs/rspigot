use rspigot_domain::{EntityId, Vec3};
use rspigot_entities::EntityKind;

use crate::behavior::{
    Activity, ActivityData, AnimalMakeLove, BabyFollowAdult, BehaviorBrain, DoNothing,
    EntityFilter, FollowTemptation, Gate, ItemWish, JumpSound, LongJumpMidJump,
    LongJumpToRandomPos, MemoryModuleType, MemoryStatus, MemoryValue, MobBehavior, MobControl,
    MobTrigger, NearestItemSensor, NearestLivingEntitySensor, PlayerSensor,
    PrepareRamNearestTarget, RamTarget, RamTimings, RandomStroll, SensorKind,
    SetEntityLookTargetSometimes, SetWalkTargetFromLookTarget, TemptingSensor, UniformInt,
};
use crate::behavior::{
    AdultSensor, HurtBySensor, LONG_JUMP_TIME_OUT, MID_JUMP_TIME_OUT, PREPARE_RAM_TIME_OUT,
    RAM_TIME_OUT,
};
use crate::goals::TemptItems;

pub const ADULT_FOLLOW_RANGE: UniformInt = UniformInt::of(5, 16);
pub const TIME_BETWEEN_LONG_JUMPS: UniformInt = UniformInt::of(
    rspigot_domain::entity::species::goat::LONG_JUMP_COOLDOWN.0,
    rspigot_domain::entity::species::goat::LONG_JUMP_COOLDOWN.1,
);
pub const TIME_BETWEEN_RAMS: UniformInt = UniformInt::of(
    rspigot_domain::entity::species::goat::RAM_COOLDOWN.0,
    rspigot_domain::entity::species::goat::RAM_COOLDOWN.1,
);
pub const TIME_BETWEEN_RAMS_SCREAMER: UniformInt = UniformInt::of(100, 300);
pub const GOAT_FOOD: &str = "minecraft:goat_food";
const MAX_LONG_JUMP_HEIGHT: i32 = 5;
const MAX_LONG_JUMP_WIDTH: i32 = 5;
const MAX_JUMP_VELOCITY_MULTIPLIER: f32 = 3.571_428_8;
const RAM_SPEED: f32 = 3.0;
const MIN_RAM_DISTANCE: i32 = 4;
const MAX_RAM_DISTANCE: i32 = 7;
const PREPARE_RAM_WALK_SPEED: f32 = 1.25;
const RAM_PREPARE_TIME: i32 = 20;
const STEP_SOUND: &str = "minecraft:entity.goat.step";
const LONG_JUMP_SOUND: JumpSound = JumpSound::ByVariant {
    regular: "minecraft:entity.goat.long_jump",
    screaming: "minecraft:entity.goat.screaming.long_jump",
};

pub const RAM_TIMINGS: RamTimings = RamTimings {
    regular: TIME_BETWEEN_RAMS,
    screaming: TIME_BETWEEN_RAMS_SCREAMER,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct GoatFacts {
    pub screaming: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GoatAction {
    Ram {
        target: EntityId,
        speed: f32,
        force: f64,
        direction: Vec3,
    },
    DropHorn,
}

pub fn init_memories(memories: &mut crate::behavior::Memories, long_jump: i32, ram: i32) {
    memories.set(
        MemoryModuleType::LongJumpCooldownTicks,
        MemoryValue::Int(long_jump),
    );
    memories.set(MemoryModuleType::RamCooldownTicks, MemoryValue::Int(ram));
}

pub fn goat_sensors() -> Vec<SensorKind> {
    vec![
        SensorKind::NearestLivingEntities(NearestLivingEntitySensor),
        SensorKind::NearestPlayers(PlayerSensor),
        SensorKind::NearestItems(NearestItemSensor::new(ItemWish::Never)),
        SensorKind::NearestAdult(AdultSensor),
        SensorKind::HurtBy(HurtBySensor),
        SensorKind::Tempting(TemptingSensor::new(TemptItems::Tag(GOAT_FOOD))),
    ]
}

fn core_activity() -> ActivityData<MobControl> {
    ActivityData::create(
        Activity::Core,
        0,
        vec![
            MobControl::swim(0.8),
            MobControl::animal_panic(2.0),
            MobControl::default_look_at_target_sink(),
            MobControl::move_to_target_sink(),
            MobControl::count_down_cooldown_ticks(MemoryModuleType::TemptationCooldownTicks),
            MobControl::count_down_cooldown_ticks(MemoryModuleType::LongJumpCooldownTicks),
            MobControl::count_down_cooldown_ticks(MemoryModuleType::RamCooldownTicks),
        ],
    )
}

fn idle_activity() -> ActivityData<MobControl> {
    let behaviors = vec![
        MobControl::one_shot(MobTrigger::SetEntityLookTargetSometimes(
            SetEntityLookTargetSometimes::new(
                EntityFilter::Kind(EntityKind::Player),
                6.0,
                UniformInt::of(30, 60),
            ),
        )),
        MobControl::animal_make_love(AnimalMakeLove::new(EntityKind::Goat)),
        MobControl::follow_temptation(FollowTemptation::new(1.25)),
        MobControl::one_shot(MobTrigger::BabyFollowAdult(BabyFollowAdult::new(
            ADULT_FOLLOW_RANGE,
            1.25,
        ))),
        MobControl::Gate(Gate::run_one(vec![
            (
                MobControl::one_shot(MobTrigger::RandomStroll(RandomStroll::stroll(1.0))),
                2,
            ),
            (
                MobControl::one_shot(MobTrigger::SetWalkTargetFromLookTarget(
                    SetWalkTargetFromLookTarget::new(1.0, 3),
                )),
                2,
            ),
            (MobControl::DoNothing(DoNothing::new(30, 60)), 1),
        ])),
    ];
    let pairs = [0, 0, 1, 2, 3].into_iter().zip(behaviors).collect();
    ActivityData::with_conditions(
        Activity::Idle,
        pairs,
        vec![
            (
                MemoryModuleType::RamCooldownTicks,
                MemoryStatus::ValuePresent,
            ),
            (
                MemoryModuleType::LongJumpCooldownTicks,
                MemoryStatus::ValuePresent,
            ),
        ],
    )
}

fn long_jump_activity() -> ActivityData<MobControl> {
    ActivityData::with_conditions(
        Activity::LongJump,
        vec![
            (
                0,
                MobControl::behavior_for(
                    MobBehavior::LongJumpMidJump(LongJumpMidJump::new(
                        TIME_BETWEEN_LONG_JUMPS,
                        STEP_SOUND,
                    )),
                    MID_JUMP_TIME_OUT,
                    MID_JUMP_TIME_OUT,
                ),
            ),
            (
                1,
                MobControl::behavior_for(
                    MobBehavior::LongJumpToRandomPos(LongJumpToRandomPos::new(
                        TIME_BETWEEN_LONG_JUMPS,
                        MAX_LONG_JUMP_HEIGHT,
                        MAX_LONG_JUMP_WIDTH,
                        MAX_JUMP_VELOCITY_MULTIPLIER,
                        LONG_JUMP_SOUND,
                    )),
                    LONG_JUMP_TIME_OUT,
                    LONG_JUMP_TIME_OUT,
                ),
            ),
        ],
        vec![
            (MemoryModuleType::TemptingPlayer, MemoryStatus::ValueAbsent),
            (MemoryModuleType::BreedTarget, MemoryStatus::ValueAbsent),
            (MemoryModuleType::WalkTarget, MemoryStatus::ValueAbsent),
            (
                MemoryModuleType::LongJumpCooldownTicks,
                MemoryStatus::ValueAbsent,
            ),
        ],
    )
}

fn ram_activity() -> ActivityData<MobControl> {
    ActivityData::with_conditions(
        Activity::Ram,
        vec![
            (
                0,
                MobControl::behavior_for(
                    MobBehavior::RamTarget(RamTarget::new(RAM_TIMINGS, RAM_SPEED)),
                    RAM_TIME_OUT,
                    RAM_TIME_OUT,
                ),
            ),
            (
                1,
                MobControl::behavior_for(
                    MobBehavior::PrepareRamNearestTarget(PrepareRamNearestTarget::new(
                        RAM_TIMINGS,
                        MIN_RAM_DISTANCE,
                        MAX_RAM_DISTANCE,
                        PREPARE_RAM_WALK_SPEED,
                        RAM_PREPARE_TIME,
                    )),
                    PREPARE_RAM_TIME_OUT,
                    PREPARE_RAM_TIME_OUT,
                ),
            ),
        ],
        vec![
            (MemoryModuleType::TemptingPlayer, MemoryStatus::ValueAbsent),
            (MemoryModuleType::BreedTarget, MemoryStatus::ValueAbsent),
            (
                MemoryModuleType::RamCooldownTicks,
                MemoryStatus::ValueAbsent,
            ),
        ],
    )
}

pub fn goat_activities() -> Vec<ActivityData<MobControl>> {
    vec![
        core_activity(),
        idle_activity(),
        long_jump_activity(),
        ram_activity(),
    ]
}

pub fn update_activity(brain: &mut BehaviorBrain<MobControl>) {
    brain.state_mut().set_active_activity_to_first_valid(&[
        Activity::Ram,
        Activity::LongJump,
        Activity::Idle,
    ]);
}
