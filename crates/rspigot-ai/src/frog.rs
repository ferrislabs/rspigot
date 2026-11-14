use rspigot_blocks::{Face, Support};
use rspigot_domain::entity::Pose;
use rspigot_domain::entity::species::frog;
use rspigot_domain::{BlockPos, EntityId};
use rspigot_entities::EntityKind;

use crate::action::MobAction;
use crate::amphibious::{
    FROG_TRIES_TO_FIND_LAND_NEAR, TryFindLandNearLiquid, TryLaySpawnOnFluidNearLand,
};
use crate::behavior::{
    Activity, ActivityData, AnimalMakeLove, Behavior, BehaviorBrain, BehaviorCx, Condition,
    EntityFilter, FROG_PREFER_JUMP_TO, FollowTemptation, Gate, HurtBySensor, IsInWaterSensor,
    JumpSound, LandingSpot, LongJumpMidJump, LongJumpToRandomPos, Memories, MemoryModuleType,
    MemoryStatus, MemoryValue, MobBehavior, MobControl, MobPredicate, MobTrigger,
    NearestLivingEntitySensor, OrderPolicy, PositionTracker, RandomStroll, RunningPolicy, Sensor,
    SensorKind, SensorType, SetEntityLookTargetSometimes, SetWalkTargetFromLookTarget,
    StartAttacking, StopAttackingIfTargetInvalid, TargetFinder, TemptingSensor, Trigger,
    UniformInt, WalkTarget, is_entity_attackable,
};
use crate::brain::AiContext;
use crate::goals::TemptItems;
use crate::scan::manhattan_ordered;
use crate::world::{AiWorld, Living};

pub const FROG_FOOD: &str = "minecraft:frog_food";
pub const TIME_BETWEEN_LONG_JUMPS: UniformInt =
    UniformInt::of(frog::LONG_JUMP_COOLDOWN.0, frog::LONG_JUMP_COOLDOWN.1);
pub const TONGUE_DURATION: i32 = 100;
pub const CROAK_DURATION: i32 = 100;
pub const CROAK_TICKS: i32 = 60;
pub const UNREACHABLE_TARGETS_LIMIT: usize = 5;
pub const UNREACHABLE_TARGET_TICKS: i64 = 100;
pub const ATTACKABLE_RANGE: f64 = 10.0;
pub const TONGUE_REACH: f32 = 1.75;
pub const TONGUE_WALK_SPEED: f32 = 2.0;
pub const TONGUE_PATH_RECALCULATION: i32 = 10;
pub const CATCH_TICKS: i32 = 5;
pub const EAT_TICKS: i32 = 10;
pub const PULL_SPEED: f64 = 0.75;
pub const FIND_LAND_INTERVAL: i64 = 60;
pub const JUMP_WIDTH: i32 = 4;
pub const JUMP_HEIGHT: i32 = 2;
pub const JUMP_VELOCITY_MULTIPLIER: f32 = 3.571_428_8;
pub const PREFERRED_BLOCK_CHANCE: f32 = 0.5;
pub const TONGUE_SOUND: &str = "minecraft:entity.frog.tongue";
pub const EAT_SOUND: &str = "minecraft:entity.frog.eat";
pub const STEP_SOUND: &str = "minecraft:entity.frog.step";
pub const LONG_JUMP_SOUND: &str = "minecraft:entity.frog.long_jump";
pub const EAT_VOLUME: f32 = 2.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FrogAction {
    SetTongueTarget(EntityId),
    EraseTongueTarget,
    Pull {
        target: EntityId,
        velocity: rspigot_domain::Vec3,
    },
    Eat(EntityId),
    LaySpawn(BlockPos),
}

pub fn init_memories(memories: &mut Memories, long_jump: i32) {
    memories.set(
        MemoryModuleType::LongJumpCooldownTicks,
        MemoryValue::Int(long_jump),
    );
}

pub fn frog_sensors() -> Vec<SensorKind> {
    vec![
        SensorKind::NearestLivingEntities(NearestLivingEntitySensor),
        SensorKind::HurtBy(HurtBySensor),
        SensorKind::FrogAttackables(FrogAttackablesSensor),
        SensorKind::Tempting(TemptingSensor::of(
            SensorType::FrogTemptations,
            TemptItems::Tag(FROG_FOOD),
        )),
        SensorKind::IsInWater(IsInWaterSensor),
    ]
}

pub fn can_eat(target: &Living) -> bool {
    frog::can_eat(target.kind, i32::from(target.facts.cube_size))
}

fn unreachable_targets(memories: &Memories) -> &[EntityId] {
    memories
        .get(MemoryModuleType::UnreachableTongueTargets)
        .and_then(MemoryValue::as_entities)
        .unwrap_or(&[])
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FrogAttackablesSensor;

impl Sensor for FrogAttackablesSensor {
    fn requires(&self) -> &'static [MemoryModuleType] {
        &[
            MemoryModuleType::NearestAttackable,
            MemoryModuleType::NearestVisibleLivingEntities,
            MemoryModuleType::UnreachableTongueTargets,
        ]
    }

    fn do_tick<W: AiWorld>(&mut self, cx: &mut AiContext<'_, W>, memories: &mut Memories) {
        let nearby: Vec<EntityId> = memories
            .get(MemoryModuleType::NearestVisibleLivingEntities)
            .and_then(MemoryValue::as_visible)
            .map(|visible| visible.nearby_entities().to_vec())
            .unwrap_or_default();
        let unreachable = unreachable_targets(memories).to_vec();
        let mob_position = cx.mob.position;
        let mut found = None;
        for id in nearby {
            let Some(target) = cx.world.living(id) else {
                continue;
            };
            if !is_entity_attackable(cx, memories, &target)
                || !can_eat(&target)
                || unreachable.contains(&id)
                || target.position.distance_squared(mob_position)
                    >= ATTACKABLE_RANGE * ATTACKABLE_RANGE
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TongueState {
    MoveToTarget,
    CatchAnimation,
    EatAnimation,
    Done,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShootTongue {
    state: TongueState,
    eat_animation_timer: i32,
    calculate_path_counter: i32,
    tongue_target: Option<EntityId>,
}

impl Default for ShootTongue {
    fn default() -> Self {
        Self {
            state: TongueState::Done,
            eat_animation_timer: 0,
            calculate_path_counter: 0,
            tongue_target: None,
        }
    }
}

impl ShootTongue {
    const CONDITION: [Condition; 4] = [
        (MemoryModuleType::WalkTarget, MemoryStatus::ValueAbsent),
        (MemoryModuleType::LookTarget, MemoryStatus::Registered),
        (MemoryModuleType::AttackTarget, MemoryStatus::ValuePresent),
        (MemoryModuleType::IsPanicking, MemoryStatus::ValueAbsent),
    ];

    pub fn state(&self) -> TongueState {
        self.state
    }

    fn attack_target<W: AiWorld>(cx: &BehaviorCx<'_, '_, W>) -> Option<Living> {
        cx.brain
            .memories
            .get(MemoryModuleType::AttackTarget)
            .and_then(MemoryValue::as_entity)
            .and_then(|id| cx.ai.world.living(id))
    }

    fn can_pathfind_to_target<W: AiWorld>(cx: &mut BehaviorCx<'_, '_, W>, target: &Living) -> bool {
        cx.ai
            .create_path_to(target.position.block(), 0)
            .is_some_and(|path| path.dist_to_target() < TONGUE_REACH)
    }

    fn add_unreachable_target<W: AiWorld>(cx: &mut BehaviorCx<'_, '_, W>, target: &Living) {
        let mut list = unreachable_targets(&cx.brain.memories).to_vec();
        let contains = list.contains(&target.id);
        if list.len() == UNREACHABLE_TARGETS_LIMIT && !contains {
            list.remove(0);
        }
        if !contains {
            list.push(target.id);
        }
        cx.brain.memories.set_with_expiry(
            MemoryModuleType::UnreachableTongueTargets,
            MemoryValue::Entities(list),
            UNREACHABLE_TARGET_TICKS,
        );
    }

    fn eat_entity<W: AiWorld>(&self, cx: &mut BehaviorCx<'_, '_, W>) {
        cx.ai.act(MobAction::Sound {
            sound: EAT_SOUND,
            volume: EAT_VOLUME,
            pitch: 1.0,
        });
        if let Some(target) = self.tongue_target {
            cx.ai.act(MobAction::Frog(FrogAction::Eat(target)));
        }
    }

    fn set_tongue_target<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, target: EntityId) {
        if self.tongue_target != Some(target) {
            self.tongue_target = Some(target);
            cx.ai
                .act(MobAction::Frog(FrogAction::SetTongueTarget(target)));
        }
    }
}

impl Behavior for ShootTongue {
    fn entry_condition(&self) -> &[Condition] {
        &Self::CONDITION
    }

    fn check_extra_start_conditions<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>) -> bool {
        let Some(target) = Self::attack_target(cx) else {
            return false;
        };
        let reachable = Self::can_pathfind_to_target(cx, &target);
        if !reachable {
            cx.brain.memories.erase(MemoryModuleType::AttackTarget);
            Self::add_unreachable_target(cx, &target);
        }
        reachable && cx.ai.body.pose != Pose::Croaking && can_eat(&target)
    }

    fn can_still_use<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        cx.brain.memories.has_value(MemoryModuleType::AttackTarget)
            && self.state != TongueState::Done
            && !cx.brain.memories.has_value(MemoryModuleType::IsPanicking)
    }

    fn start<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        let Some(target) = Self::attack_target(cx) else {
            return;
        };
        crate::behavior::look_at_entity(cx, target.id);
        self.tongue_target = None;
        self.set_tongue_target(cx, target.id);
        cx.brain.memories.set(
            MemoryModuleType::WalkTarget,
            MemoryValue::WalkTarget(WalkTarget::position(target.position, TONGUE_WALK_SPEED, 0)),
        );
        self.calculate_path_counter = TONGUE_PATH_RECALCULATION;
        self.state = TongueState::MoveToTarget;
    }

    fn stop<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        cx.brain.memories.erase(MemoryModuleType::AttackTarget);
        cx.ai.act(MobAction::Frog(FrogAction::EraseTongueTarget));
        self.tongue_target = None;
        cx.ai.body.pose = Pose::Standing;
    }

    fn tick<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        let Some(target) = Self::attack_target(cx) else {
            return;
        };
        self.set_tongue_target(cx, target.id);
        match self.state {
            TongueState::MoveToTarget => {
                let distance = cx.ai.mob.position.distance_squared(target.position).sqrt() as f32;
                if distance >= TONGUE_REACH {
                    if self.calculate_path_counter < 1 {
                        cx.brain.memories.set(
                            MemoryModuleType::WalkTarget,
                            MemoryValue::WalkTarget(WalkTarget::position(
                                target.position,
                                TONGUE_WALK_SPEED,
                                0,
                            )),
                        );
                        self.calculate_path_counter = TONGUE_PATH_RECALCULATION;
                    } else {
                        self.calculate_path_counter -= 1;
                    }
                } else {
                    cx.ai.act(MobAction::Sound {
                        sound: TONGUE_SOUND,
                        volume: EAT_VOLUME,
                        pitch: 1.0,
                    });
                    cx.ai.body.pose = Pose::UsingTongue;
                    let toward = (cx.ai.mob.position - target.position).normalize();
                    cx.ai.act(MobAction::Frog(FrogAction::Pull {
                        target: target.id,
                        velocity: toward.scale(PULL_SPEED),
                    }));
                    self.eat_animation_timer = 0;
                    self.state = TongueState::CatchAnimation;
                }
            }
            TongueState::CatchAnimation => {
                let elapsed = self.eat_animation_timer;
                self.eat_animation_timer += 1;
                if elapsed > CATCH_TICKS {
                    self.state = TongueState::EatAnimation;
                    self.eat_entity(cx);
                }
            }
            TongueState::EatAnimation => {
                if self.eat_animation_timer < EAT_TICKS {
                    self.eat_animation_timer += 1;
                } else {
                    self.state = TongueState::Done;
                }
            }
            TongueState::Done => {}
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Croak {
    croak_counter: i32,
}

impl Croak {
    const CONDITION: [Condition; 1] = [(MemoryModuleType::WalkTarget, MemoryStatus::ValueAbsent)];
}

impl Behavior for Croak {
    fn entry_condition(&self) -> &[Condition] {
        &Self::CONDITION
    }

    fn check_extra_start_conditions<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>) -> bool {
        cx.ai.body.pose == Pose::Standing
    }

    fn can_still_use<W: AiWorld>(&mut self, _cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        self.croak_counter < CROAK_TICKS
    }

    fn start<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        if cx.ai.mob.fluid.in_liquid() {
            return;
        }
        cx.ai.body.pose = Pose::Croaking;
        self.croak_counter = 0;
    }

    fn stop<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        cx.ai.body.pose = Pose::Standing;
    }

    fn tick<W: AiWorld>(&mut self, _cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        self.croak_counter += 1;
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TryFindLand {
    range: i32,
    speed_modifier: f32,
    next_scan: i64,
}

impl TryFindLand {
    pub fn new(range: i32, speed_modifier: f32) -> Self {
        Self {
            range,
            speed_modifier,
            next_scan: 0,
        }
    }

    pub fn conditions(&self) -> Vec<Condition> {
        vec![
            (MemoryModuleType::AttackTarget, MemoryStatus::ValueAbsent),
            (MemoryModuleType::WalkTarget, MemoryStatus::ValueAbsent),
            (MemoryModuleType::LookTarget, MemoryStatus::Registered),
        ]
    }

    fn can_stand_on<W: AiWorld>(world: &W, pos: BlockPos) -> bool {
        let Some(state) = world.state(pos) else {
            return false;
        };
        state.collision_shape().is_empty()
            && world
                .state(pos.offset(0, -1, 0))
                .is_some_and(|below| below.is_face_sturdy(Face::Up, Support::Full))
    }

    fn find_land<W: AiWorld>(&self, world: &W, origin: BlockPos) -> Option<BlockPos> {
        manhattan_ordered(origin, self.range, self.range, self.range).find(|pos| {
            (pos.x != origin.x || pos.z != origin.z)
                && world.state(*pos).is_some_and(|state| {
                    !state.flags().contains(rspigot_blocks::StateFlags::WATER)
                        && !state.flags().contains(rspigot_blocks::StateFlags::LAVA)
                })
                && Self::can_stand_on(world, *pos)
        })
    }
}

impl Trigger for TryFindLand {
    fn trigger<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, now: i64) -> bool {
        let origin = cx.ai.mob.block_pos();
        let in_water = cx
            .ai
            .world
            .state(origin)
            .is_some_and(|state| state.flags().contains(rspigot_blocks::StateFlags::WATER));
        if !in_water {
            return false;
        }
        if now < self.next_scan {
            self.next_scan = now + FIND_LAND_INTERVAL;
            return true;
        }
        if let Some(land) = self.find_land(cx.ai.world, origin) {
            cx.brain.memories.set(
                MemoryModuleType::LookTarget,
                MemoryValue::Tracker(PositionTracker::of_block(land)),
            );
            cx.brain.memories.set(
                MemoryModuleType::WalkTarget,
                MemoryValue::WalkTarget(WalkTarget::block(land, self.speed_modifier, 1)),
            );
        }
        self.next_scan = now + FIND_LAND_INTERVAL;
        true
    }
}

fn look_sometimes() -> MobControl {
    MobControl::one_shot(MobTrigger::SetEntityLookTargetSometimes(
        SetEntityLookTargetSometimes::new(
            EntityFilter::Kind(EntityKind::Player),
            6.0,
            UniformInt::of(30, 60),
        ),
    ))
}

fn start_attacking() -> MobControl {
    MobControl::one_shot(MobTrigger::StartAttacking(StartAttacking::when(
        MobPredicate::HasMemory(MemoryModuleType::BreedTarget).negate(),
        TargetFinder::Memory(MemoryModuleType::NearestAttackable),
    )))
}

fn core_activity() -> ActivityData<MobControl> {
    ActivityData::create(
        Activity::Core,
        0,
        vec![
            MobControl::animal_panic(2.0),
            MobControl::look_at_target_sink(45, 90),
            MobControl::move_to_target_sink(),
            MobControl::count_down_cooldown_ticks(MemoryModuleType::TemptationCooldownTicks),
            MobControl::count_down_cooldown_ticks(MemoryModuleType::LongJumpCooldownTicks),
        ],
    )
}

fn idle_activity() -> ActivityData<MobControl> {
    let fidget = MobControl::Gate(Gate::run_one_when(
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
            (
                MobControl::behavior_for(
                    MobBehavior::FrogCroak(Croak::default()),
                    CROAK_DURATION,
                    CROAK_DURATION,
                ),
                3,
            ),
            (
                MobControl::one_shot(MobTrigger::Condition(MobPredicate::OnGround)),
                2,
            ),
        ],
    ));
    ActivityData::with_conditions(
        Activity::Idle,
        vec![
            (0, look_sometimes()),
            (
                0,
                MobControl::animal_make_love(AnimalMakeLove::new(EntityKind::Frog)),
            ),
            (
                1,
                MobControl::follow_temptation(FollowTemptation::new(1.25)),
            ),
            (2, start_attacking()),
            (
                3,
                MobControl::one_shot(MobTrigger::FrogTryFindLand(TryFindLand::new(6, 1.0))),
            ),
            (4, fidget),
        ],
        vec![
            (MemoryModuleType::LongJumpMidJump, MemoryStatus::ValueAbsent),
            (MemoryModuleType::IsInWater, MemoryStatus::ValueAbsent),
        ],
    )
}

fn swim_activity() -> ActivityData<MobControl> {
    ActivityData::with_conditions(
        Activity::Swim,
        vec![
            (0, look_sometimes()),
            (
                1,
                MobControl::follow_temptation(FollowTemptation::new(1.25)),
            ),
            (2, start_attacking()),
            (
                3,
                MobControl::one_shot(MobTrigger::FrogTryFindLand(TryFindLand::new(8, 1.5))),
            ),
            (
                5,
                MobControl::Gate(Gate::new(
                    vec![(MemoryModuleType::WalkTarget, MemoryStatus::ValueAbsent)],
                    Vec::new(),
                    OrderPolicy::Ordered,
                    RunningPolicy::TryAll,
                    vec![
                        (
                            MobControl::one_shot(MobTrigger::RandomStroll(RandomStroll::swim(
                                0.75,
                            ))),
                            1,
                        ),
                        (
                            MobControl::one_shot(MobTrigger::RandomStroll(
                                RandomStroll::stroll_from_water(1.0, true),
                            )),
                            1,
                        ),
                        (
                            MobControl::one_shot(MobTrigger::SetWalkTargetFromLookTarget(
                                SetWalkTargetFromLookTarget::new(1.0, 3),
                            )),
                            1,
                        ),
                        (
                            MobControl::one_shot(MobTrigger::Condition(MobPredicate::InWater)),
                            5,
                        ),
                    ],
                )),
            ),
        ],
        vec![
            (MemoryModuleType::LongJumpMidJump, MemoryStatus::ValueAbsent),
            (MemoryModuleType::IsInWater, MemoryStatus::ValuePresent),
        ],
    )
}

fn lay_spawn_activity() -> ActivityData<MobControl> {
    ActivityData::with_conditions(
        Activity::LaySpawn,
        vec![
            (0, look_sometimes()),
            (1, start_attacking()),
            (
                2,
                MobControl::one_shot(MobTrigger::TryFindLandNearLiquid(
                    TryFindLandNearLiquid::new(8, 1.0, FROG_TRIES_TO_FIND_LAND_NEAR),
                )),
            ),
            (
                3,
                MobControl::one_shot(MobTrigger::TryLaySpawnOnFluidNearLand(
                    TryLaySpawnOnFluidNearLand,
                )),
            ),
            (
                4,
                MobControl::Gate(Gate::run_one(vec![
                    (
                        MobControl::one_shot(MobTrigger::RandomStroll(RandomStroll::stroll(1.0))),
                        2,
                    ),
                    (
                        MobControl::one_shot(MobTrigger::SetWalkTargetFromLookTarget(
                            SetWalkTargetFromLookTarget::new(1.0, 3),
                        )),
                        1,
                    ),
                    (
                        MobControl::behavior_for(
                            MobBehavior::FrogCroak(Croak::default()),
                            CROAK_DURATION,
                            CROAK_DURATION,
                        ),
                        2,
                    ),
                    (
                        MobControl::one_shot(MobTrigger::Condition(MobPredicate::OnGround)),
                        1,
                    ),
                ])),
            ),
        ],
        vec![
            (MemoryModuleType::LongJumpMidJump, MemoryStatus::ValueAbsent),
            (MemoryModuleType::IsPregnant, MemoryStatus::ValuePresent),
        ],
    )
}

fn tongue_activity() -> ActivityData<MobControl> {
    ActivityData::create_erasing(
        Activity::Tongue,
        0,
        vec![
            MobControl::one_shot(MobTrigger::StopAttackingIfTargetInvalid(
                StopAttackingIfTargetInvalid::default(),
            )),
            MobControl::behavior_for(
                MobBehavior::FrogShootTongue(ShootTongue::default()),
                TONGUE_DURATION,
                TONGUE_DURATION,
            ),
        ],
        MemoryModuleType::AttackTarget,
    )
}

fn jump_activity() -> ActivityData<MobControl> {
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
                    crate::behavior::MID_JUMP_TIME_OUT,
                    crate::behavior::MID_JUMP_TIME_OUT,
                ),
            ),
            (
                1,
                MobControl::behavior_for(
                    MobBehavior::LongJumpToRandomPos(
                        LongJumpToRandomPos::new(
                            TIME_BETWEEN_LONG_JUMPS,
                            JUMP_HEIGHT,
                            JUMP_WIDTH,
                            JUMP_VELOCITY_MULTIPLIER,
                            JumpSound::Fixed(LONG_JUMP_SOUND),
                        )
                        .preferring_blocks(
                            FROG_PREFER_JUMP_TO,
                            PREFERRED_BLOCK_CHANCE,
                            LandingSpot::Frog,
                        ),
                    ),
                    crate::behavior::LONG_JUMP_TIME_OUT,
                    crate::behavior::LONG_JUMP_TIME_OUT,
                ),
            ),
        ],
        vec![
            (MemoryModuleType::TemptingPlayer, MemoryStatus::ValueAbsent),
            (MemoryModuleType::BreedTarget, MemoryStatus::ValueAbsent),
            (
                MemoryModuleType::LongJumpCooldownTicks,
                MemoryStatus::ValueAbsent,
            ),
            (MemoryModuleType::IsInWater, MemoryStatus::ValueAbsent),
        ],
    )
}

pub fn frog_activities() -> Vec<ActivityData<MobControl>> {
    vec![
        core_activity(),
        idle_activity(),
        swim_activity(),
        lay_spawn_activity(),
        tongue_activity(),
        jump_activity(),
    ]
}

pub fn update_activity(brain: &mut BehaviorBrain<MobControl>) {
    brain.state_mut().set_active_activity_to_first_valid(&[
        Activity::Tongue,
        Activity::LaySpawn,
        Activity::LongJump,
        Activity::Swim,
        Activity::Idle,
    ]);
}
