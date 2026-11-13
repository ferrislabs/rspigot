use rspigot_domain::entity::species::sniffer::{self, State};
use rspigot_domain::{BlockPos, mth};
use rspigot_entities::EntityKind;

use crate::action::MobAction;
use crate::behavior::{
    Activity, ActivityData, AnimalMakeLove, AnimalPanic, Behavior, BehaviorBrain, BehaviorCx,
    Condition, DoNothing, EntityFilter, FollowTemptation, Gate, GlobalPos, HurtBySensor,
    MAKE_LOVE_DURATION, Memories, MemoryModuleType, MemoryStatus, MemoryValue, MobBehavior,
    MobControl, MobTrigger, NearestLivingEntitySensor, PlayerSensor, RandomStroll, SensorKind,
    SensorType, SetEntityLookTarget, SetWalkTargetFromLookTarget, TemptDistance, TemptingSensor,
    WalkTarget,
};
use crate::goals::TemptItems;
use crate::random_pos::land_pos;
use crate::world::AiWorld;

pub const SNIFFER_FOOD: &str = "minecraft:sniffer_food";
pub const DIGGABLE_BLOCKS: &str = "minecraft:sniffer_diggable_block";
pub const SNIFF_COOLDOWN_TICKS: i64 = 9600;
pub const SEARCH_DURATION: i32 = 600;
pub const SNIFFING_WALK_SPEED: f32 = 1.25;
pub const DIG_POSITION_ATTEMPTS: i32 = 5;
pub const DIG_POSITION_BASE_RANGE: i32 = 10;
pub const DIG_POSITION_RANGE_STEP: i32 = 2;
pub const DIG_POSITION_VERTICAL: i32 = 3;
pub const HAPPY_SOUND: &str = "minecraft:entity.sniffer.happy";
pub const SCENTING_SOUND: &str = "minecraft:entity.sniffer.scenting";
pub const SNIFFING_SOUND: &str = "minecraft:entity.sniffer.sniffing";
pub const DIGGING_STOP_SOUND: &str = "minecraft:entity.sniffer.digging_stop";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SnifferFacts {
    pub state: State,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnifferAction {
    StartDigging,
}

pub const SNIFFER_MEMORIES: [MemoryModuleType; 1] = [MemoryModuleType::SnifferExploredPositions];

pub fn sniffer_sensors() -> Vec<SensorKind> {
    vec![
        SensorKind::NearestLivingEntities(NearestLivingEntitySensor),
        SensorKind::HurtBy(HurtBySensor),
        SensorKind::NearestPlayers(PlayerSensor),
        SensorKind::Tempting(TemptingSensor::of(
            SensorType::FoodTemptations,
            TemptItems::Tag(SNIFFER_FOOD),
        )),
    ]
}

pub fn can_mate_in_states(kind: EntityKind, own: State, other: u8) -> bool {
    kind != EntityKind::Sniffer || (own.can_mate() && State::from_id(i32::from(other)).can_mate())
}

pub fn is_tempted(memories: &Memories) -> bool {
    memories.has_value(MemoryModuleType::TemptingPlayer)
}

pub fn is_panicking(memories: &Memories) -> bool {
    memories.has_value(MemoryModuleType::IsPanicking)
}

pub fn can_sniff<W: AiWorld>(cx: &BehaviorCx<'_, '_, W>) -> bool {
    let mob = cx.ai.mob;
    !is_tempted(&cx.brain.memories)
        && !is_panicking(&cx.brain.memories)
        && !mob.fluid.in_water
        && !mob.in_love
        && mob.on_ground
        && !mob.passenger
        && !mob.leashed
}

pub fn head_block<W: AiWorld>(cx: &BehaviorCx<'_, '_, W>) -> BlockPos {
    let mob = cx.ai.mob;
    let forward = mth::view_vector(mob.pitch, mob.yaw);
    sniffer::head_block(mob.position, forward)
}

pub fn on_pos<W: AiWorld>(cx: &BehaviorCx<'_, '_, W>) -> BlockPos {
    let position = cx.ai.mob.position;
    rspigot_domain::Vec3::new(position.x, position.y - 0.2, position.z).block()
}

pub fn explored_positions(memories: &Memories) -> &[GlobalPos] {
    match memories.get(MemoryModuleType::SnifferExploredPositions) {
        Some(MemoryValue::GlobalPosList(positions)) => positions,
        _ => &[],
    }
}

pub fn store_explored_position<W: AiWorld>(cx: &mut BehaviorCx<'_, '_, W>, pos: BlockPos) {
    let dimension = cx.ai.world.dimension().to_owned();
    let mut positions: Vec<GlobalPos> = explored_positions(&cx.brain.memories)
        .iter()
        .take(sniffer::EXPLORED_LIMIT)
        .cloned()
        .collect();
    positions.insert(0, GlobalPos { dimension, pos });
    cx.brain.memories.set(
        MemoryModuleType::SnifferExploredPositions,
        MemoryValue::GlobalPosList(positions),
    );
}

fn is_explored<W: AiWorld>(cx: &BehaviorCx<'_, '_, W>, pos: BlockPos) -> bool {
    let dimension = cx.ai.world.dimension();
    explored_positions(&cx.brain.memories)
        .iter()
        .any(|explored| explored.pos == pos && explored.dimension == dimension)
}

fn is_diggable<W: AiWorld>(cx: &BehaviorCx<'_, '_, W>, pos: BlockPos) -> bool {
    cx.ai
        .world
        .state(pos)
        .and_then(|state| state.kind())
        .is_some_and(|kind| crate::animal::kind_in_block_tag(kind, DIGGABLE_BLOCKS))
}

pub fn can_dig_at<W: AiWorld>(cx: &mut BehaviorCx<'_, '_, W>, pos: BlockPos) -> bool {
    is_diggable(cx, pos)
        && !is_explored(cx, pos)
        && cx
            .ai
            .create_path_to(pos, 1)
            .is_some_and(|path| path.can_reach())
}

pub fn can_dig<W: AiWorld>(cx: &mut BehaviorCx<'_, '_, W>) -> bool {
    let mob = cx.ai.mob;
    if is_panicking(&cx.brain.memories)
        || is_tempted(&cx.brain.memories)
        || mob.baby
        || mob.fluid.in_water
        || !mob.on_ground
        || mob.passenger
    {
        return false;
    }
    let below = head_block(cx).offset(0, -1, 0);
    can_dig_at(cx, below)
}

pub fn calculate_dig_position<W: AiWorld>(cx: &mut BehaviorCx<'_, '_, W>) -> Option<BlockPos> {
    for attempt in 0..DIG_POSITION_ATTEMPTS {
        let range = DIG_POSITION_BASE_RANGE + DIG_POSITION_RANGE_STEP * attempt;
        let Some(found) = land_pos(cx.ai, range, DIG_POSITION_VERTICAL) else {
            continue;
        };
        let pos = found.block();
        if !cx.ai.world.within_world_border(pos) {
            continue;
        }
        let below = pos.offset(0, -1, 0);
        if can_dig_at(cx, below) {
            return Some(below);
        }
    }
    None
}

pub fn transition_to<W: AiWorld>(cx: &mut BehaviorCx<'_, '_, W>, next: State) {
    match next {
        State::Idling | State::Searching => {}
        State::FeelingHappy => cx.ai.act(MobAction::Sound {
            sound: HAPPY_SOUND,
            volume: 1.0,
            pitch: 1.0,
        }),
        State::Scenting => {
            let pitch = if cx.ai.mob.baby {
                sniffer::BABY_SCENT_PITCH
            } else {
                1.0
            };
            cx.ai.act(MobAction::Sound {
                sound: SCENTING_SOUND,
                volume: 1.0,
                pitch,
            });
        }
        State::Sniffing => cx.ai.act(MobAction::Sound {
            sound: SNIFFING_SOUND,
            volume: 1.0,
            pitch: 1.0,
        }),
        State::Digging => cx.ai.act(MobAction::Sniffer(SnifferAction::StartDigging)),
        State::Rising => cx.ai.act(MobAction::Sound {
            sound: DIGGING_STOP_SOUND,
            volume: 1.0,
            pitch: 1.0,
        }),
    }
    cx.ai.body.sniffer.state = next;
}

pub fn reset_sniffing<W: AiWorld>(cx: &mut BehaviorCx<'_, '_, W>) {
    cx.brain.memories.erase(MemoryModuleType::SnifferDigging);
    cx.brain
        .memories
        .erase(MemoryModuleType::SnifferSniffingTarget);
    transition_to(cx, State::Idling);
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResetsSniffing<B> {
    inner: B,
}

impl<B> ResetsSniffing<B> {
    pub fn new(inner: B) -> Self {
        Self { inner }
    }
}

impl<B: Behavior> Behavior for ResetsSniffing<B> {
    fn entry_condition(&self) -> &[Condition] {
        self.inner.entry_condition()
    }

    fn check_extra_start_conditions<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>) -> bool {
        self.inner.check_extra_start_conditions(cx)
    }

    fn start<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, now: i64) {
        reset_sniffing(cx);
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

    fn stop_with_timeout<W: AiWorld>(
        &mut self,
        cx: &mut BehaviorCx<'_, '_, W>,
        now: i64,
        timed_out: bool,
    ) {
        self.inner.stop_with_timeout(cx, now, timed_out);
    }

    fn timed_out(&self, end_timestamp: i64, now: i64) -> bool {
        self.inner.timed_out(end_timestamp, now)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Searching;

impl Searching {
    const CONDITION: [Condition; 3] = [
        (MemoryModuleType::WalkTarget, MemoryStatus::ValuePresent),
        (MemoryModuleType::IsPanicking, MemoryStatus::ValueAbsent),
        (
            MemoryModuleType::SnifferSniffingTarget,
            MemoryStatus::ValuePresent,
        ),
    ];
}

impl Behavior for Searching {
    fn entry_condition(&self) -> &[Condition] {
        &Self::CONDITION
    }

    fn check_extra_start_conditions<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>) -> bool {
        can_sniff(cx)
    }

    fn can_still_use<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        if !can_sniff(cx) {
            transition_to(cx, State::Idling);
            return false;
        }
        let walking_to = cx
            .brain
            .memories
            .get(MemoryModuleType::WalkTarget)
            .and_then(MemoryValue::as_walk_target)
            .and_then(|target| target.target.current_block_position(cx.ai.world));
        let sniffing = cx
            .brain
            .memories
            .get(MemoryModuleType::SnifferSniffingTarget)
            .and_then(MemoryValue::as_block);
        matches!((walking_to, sniffing), (Some(walk), Some(sniff)) if walk == sniff)
    }

    fn start<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        transition_to(cx, State::Searching);
    }

    fn stop<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        if can_dig(cx) && can_sniff(cx) {
            cx.brain
                .memories
                .set(MemoryModuleType::SnifferDigging, MemoryValue::Bool(true));
        }
        cx.brain.memories.erase(MemoryModuleType::WalkTarget);
        cx.brain
            .memories
            .erase(MemoryModuleType::SnifferSniffingTarget);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Sniffing;

impl Sniffing {
    const CONDITION: [Condition; 3] = [
        (MemoryModuleType::WalkTarget, MemoryStatus::ValueAbsent),
        (
            MemoryModuleType::SnifferSniffingTarget,
            MemoryStatus::ValueAbsent,
        ),
        (MemoryModuleType::SniffCooldown, MemoryStatus::ValueAbsent),
    ];
}

impl Behavior for Sniffing {
    fn entry_condition(&self) -> &[Condition] {
        &Self::CONDITION
    }

    fn check_extra_start_conditions<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>) -> bool {
        !cx.ai.mob.baby && can_sniff(cx)
    }

    fn can_still_use<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        can_sniff(cx)
    }

    fn start<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        transition_to(cx, State::Sniffing);
    }

    fn stop_with_timeout<W: AiWorld>(
        &mut self,
        cx: &mut BehaviorCx<'_, '_, W>,
        _now: i64,
        timed_out: bool,
    ) {
        transition_to(cx, State::Idling);
        if timed_out && let Some(pos) = calculate_dig_position(cx) {
            cx.brain.memories.set(
                MemoryModuleType::SnifferSniffingTarget,
                MemoryValue::Block(pos),
            );
            cx.brain.memories.set(
                MemoryModuleType::WalkTarget,
                MemoryValue::WalkTarget(WalkTarget::block(pos, SNIFFING_WALK_SPEED, 0)),
            );
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Digging;

impl Digging {
    const CONDITION: [Condition; 4] = [
        (MemoryModuleType::IsPanicking, MemoryStatus::ValueAbsent),
        (MemoryModuleType::WalkTarget, MemoryStatus::ValueAbsent),
        (MemoryModuleType::SnifferDigging, MemoryStatus::ValuePresent),
        (MemoryModuleType::SniffCooldown, MemoryStatus::ValueAbsent),
    ];
}

impl Behavior for Digging {
    fn entry_condition(&self) -> &[Condition] {
        &Self::CONDITION
    }

    fn check_extra_start_conditions<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>) -> bool {
        can_sniff(cx)
    }

    fn can_still_use<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        cx.brain
            .memories
            .has_value(MemoryModuleType::SnifferDigging)
            && can_dig(cx)
            && !cx.ai.mob.in_love
    }

    fn start<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        transition_to(cx, State::Digging);
    }

    fn stop_with_timeout<W: AiWorld>(
        &mut self,
        cx: &mut BehaviorCx<'_, '_, W>,
        _now: i64,
        timed_out: bool,
    ) {
        if timed_out {
            cx.brain.memories.set_with_expiry(
                MemoryModuleType::SniffCooldown,
                MemoryValue::Unit,
                SNIFF_COOLDOWN_TICKS,
            );
        } else {
            reset_sniffing(cx);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FinishedDigging;

impl FinishedDigging {
    const CONDITION: [Condition; 4] = [
        (MemoryModuleType::IsPanicking, MemoryStatus::ValueAbsent),
        (MemoryModuleType::WalkTarget, MemoryStatus::ValueAbsent),
        (MemoryModuleType::SnifferDigging, MemoryStatus::ValuePresent),
        (MemoryModuleType::SniffCooldown, MemoryStatus::ValuePresent),
    ];
}

impl Behavior for FinishedDigging {
    fn entry_condition(&self) -> &[Condition] {
        &Self::CONDITION
    }

    fn can_still_use<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        cx.brain
            .memories
            .has_value(MemoryModuleType::SnifferDigging)
    }

    fn start<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        transition_to(cx, State::Rising);
    }

    fn stop_with_timeout<W: AiWorld>(
        &mut self,
        cx: &mut BehaviorCx<'_, '_, W>,
        _now: i64,
        timed_out: bool,
    ) {
        transition_to(cx, State::Idling);
        if timed_out {
            let pos = on_pos(cx);
            store_explored_position(cx, pos);
        }
        cx.brain.memories.erase(MemoryModuleType::SnifferDigging);
        cx.brain
            .memories
            .set(MemoryModuleType::SnifferHappy, MemoryValue::Bool(true));
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Scenting;

impl Scenting {
    const CONDITION: [Condition; 5] = [
        (MemoryModuleType::IsPanicking, MemoryStatus::ValueAbsent),
        (MemoryModuleType::SnifferDigging, MemoryStatus::ValueAbsent),
        (
            MemoryModuleType::SnifferSniffingTarget,
            MemoryStatus::ValueAbsent,
        ),
        (MemoryModuleType::SnifferHappy, MemoryStatus::ValueAbsent),
        (MemoryModuleType::BreedTarget, MemoryStatus::ValueAbsent),
    ];
}

impl Behavior for Scenting {
    fn entry_condition(&self) -> &[Condition] {
        &Self::CONDITION
    }

    fn check_extra_start_conditions<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>) -> bool {
        !is_tempted(&cx.brain.memories)
    }

    fn can_still_use<W: AiWorld>(&mut self, _cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        true
    }

    fn start<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        transition_to(cx, State::Scenting);
    }

    fn stop<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        transition_to(cx, State::Idling);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FeelingHappy;

impl FeelingHappy {
    const CONDITION: [Condition; 1] =
        [(MemoryModuleType::SnifferHappy, MemoryStatus::ValuePresent)];
}

impl Behavior for FeelingHappy {
    fn entry_condition(&self) -> &[Condition] {
        &Self::CONDITION
    }

    fn can_still_use<W: AiWorld>(&mut self, _cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        true
    }

    fn start<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        transition_to(cx, State::FeelingHappy);
    }

    fn stop<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        transition_to(cx, State::Idling);
        cx.brain.memories.erase(MemoryModuleType::SnifferHappy);
    }
}

fn core_activity() -> ActivityData<MobControl> {
    ActivityData::create(
        Activity::Core,
        0,
        vec![
            MobControl::swim(0.8),
            MobControl::behavior_for(
                MobBehavior::SnifferPanic(ResetsSniffing::new(AnimalPanic::new(2.0))),
                AnimalPanic::min_duration(),
                AnimalPanic::max_duration(),
            ),
            MobControl::move_to_target_sink_for(500, 700),
            MobControl::count_down_cooldown_ticks(MemoryModuleType::TemptationCooldownTicks),
        ],
    )
}

fn idle_activity() -> ActivityData<MobControl> {
    let fidget = MobControl::Gate(Gate::run_one(vec![
        (
            MobControl::one_shot(MobTrigger::SetWalkTargetFromLookTarget(
                SetWalkTargetFromLookTarget::new(1.0, 3),
            )),
            2,
        ),
        (
            MobControl::behavior_for(MobBehavior::SnifferScenting(Scenting), 40, 80),
            1,
        ),
        (
            MobControl::behavior_for(MobBehavior::SnifferSniffing(Sniffing), 40, 80),
            1,
        ),
        (
            MobControl::one_shot(MobTrigger::SetEntityLookTarget(SetEntityLookTarget::new(
                EntityFilter::Kind(EntityKind::Player),
                6.0,
            ))),
            1,
        ),
        (
            MobControl::one_shot(MobTrigger::RandomStroll(RandomStroll::stroll(1.0))),
            1,
        ),
        (MobControl::DoNothing(DoNothing::new(5, 20)), 2),
    ]));
    ActivityData::with_conditions(
        Activity::Idle,
        vec![
            (
                0,
                MobControl::behavior_for(
                    MobBehavior::SnifferMakeLove(ResetsSniffing::new(AnimalMakeLove::new(
                        EntityKind::Sniffer,
                    ))),
                    MAKE_LOVE_DURATION,
                    MAKE_LOVE_DURATION,
                ),
            ),
            (
                1,
                MobControl::behavior(MobBehavior::SnifferFollowTemptation(ResetsSniffing::new(
                    FollowTemptation::with_distance(
                        1.25,
                        TemptDistance::ByAge {
                            baby: 2.5,
                            adult: 3.5,
                        },
                        false,
                    ),
                ))),
            ),
            (2, MobControl::look_at_target_sink(45, 90)),
            (
                3,
                MobControl::behavior_for(MobBehavior::SnifferFeelingHappy(FeelingHappy), 40, 100),
            ),
            (4, fidget),
        ],
        vec![(MemoryModuleType::SnifferDigging, MemoryStatus::ValueAbsent)],
    )
}

fn sniff_activity() -> ActivityData<MobControl> {
    ActivityData::with_conditions(
        Activity::Sniff,
        vec![(
            0,
            MobControl::behavior_for(
                MobBehavior::SnifferSearching(Searching),
                SEARCH_DURATION,
                SEARCH_DURATION,
            ),
        )],
        vec![
            (MemoryModuleType::IsPanicking, MemoryStatus::ValueAbsent),
            (
                MemoryModuleType::SnifferSniffingTarget,
                MemoryStatus::ValuePresent,
            ),
            (MemoryModuleType::WalkTarget, MemoryStatus::ValuePresent),
        ],
    )
}

fn dig_activity() -> ActivityData<MobControl> {
    ActivityData::with_conditions(
        Activity::Dig,
        vec![
            (
                0,
                MobControl::behavior_for(MobBehavior::SnifferDigging(Digging), 160, 180),
            ),
            (
                0,
                MobControl::behavior_for(
                    MobBehavior::SnifferFinishedDigging(FinishedDigging),
                    40,
                    40,
                ),
            ),
        ],
        vec![
            (MemoryModuleType::IsPanicking, MemoryStatus::ValueAbsent),
            (MemoryModuleType::WalkTarget, MemoryStatus::ValueAbsent),
            (MemoryModuleType::SnifferDigging, MemoryStatus::ValuePresent),
        ],
    )
}

pub fn sniffer_activities() -> Vec<ActivityData<MobControl>> {
    vec![
        core_activity(),
        idle_activity(),
        sniff_activity(),
        dig_activity(),
    ]
}

pub fn update_activity(brain: &mut BehaviorBrain<MobControl>) {
    brain.state_mut().set_active_activity_to_first_valid(&[
        Activity::Dig,
        Activity::Sniff,
        Activity::Idle,
    ]);
}
