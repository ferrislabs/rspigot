use rspigot_domain::entity::species::allay;
use rspigot_domain::{BlockPos, Vec3};

use crate::action::MobAction;
use crate::behavior::{
    Activity, ActivityData, Behavior, BehaviorBrain, BehaviorCx, Condition, DoNothing,
    EntityFilter, Gate, GlobalPos, GoToWantedItem, HurtBySensor, ItemWish, MemoryModuleType,
    MemoryStatus, MemoryValue, MobBehavior, MobControl, MobTrigger, NearestItemSensor,
    NearestLivingEntitySensor, PlayerSensor, PositionTracker, RandomStroll, SensorKind,
    SetEntityLookTargetSometimes, SetWalkTargetFromLookTarget, Trigger, UniformInt, WalkTarget,
};
use crate::world::AiWorld;

pub const ALLAY_MEMORIES: [MemoryModuleType; 5] = [
    MemoryModuleType::LikedPlayer,
    MemoryModuleType::LikedNoteblockPosition,
    MemoryModuleType::LikedNoteblockCooldownTicks,
    MemoryModuleType::ItemPickupCooldownTicks,
    MemoryModuleType::IsPanicking,
];
const SWIM_CHANCE: f32 = 0.8;
const PANIC_SPEED: f32 = 2.5;
const WANTED_ITEM_SPEED: f32 = 1.75;
const WANTED_ITEM_RANGE: i32 = 32;
const GIVE_SPEED: f32 = 2.25;
const GIVE_TIMEOUT: i32 = 20;
const GIVE_COOLDOWN: i32 = 60;
const GIVE_REACH: f64 = 3.0;
const GIVE_CLOSE_ENOUGH: i32 = 3;
const CLOSE_ENOUGH: i32 = 4;
const LOOK_DISTANCE: f32 = 6.0;
const STROLL_SPEED: f32 = 1.0;
const WALK_FROM_LOOK_DISTANCE: i32 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AllayFacts {
    pub has_item_to_throw: bool,
    pub hand_item: Option<rspigot_domain::ItemId>,
    pub inventory_item: Option<rspigot_domain::ItemId>,
    pub inventory_full: bool,
}

impl AllayFacts {
    pub fn wants(&self, item: rspigot_domain::ItemId, griefing: bool) -> bool {
        let room = match self.inventory_item {
            None => true,
            Some(held) => held == item && !self.inventory_full,
        };
        self.hand_item == Some(item) && griefing && room
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AllayAction {
    ThrowItem { target: Vec3 },
}

pub fn allay_sensors() -> Vec<SensorKind> {
    vec![
        SensorKind::NearestLivingEntities(NearestLivingEntitySensor),
        SensorKind::NearestPlayers(PlayerSensor),
        SensorKind::HurtBy(HurtBySensor),
        SensorKind::NearestItems(NearestItemSensor::new(ItemWish::Allay)),
    ]
}

fn core_activity() -> ActivityData<MobControl> {
    ActivityData::create(
        Activity::Core,
        0,
        vec![
            MobControl::swim(SWIM_CHANCE),
            MobControl::animal_panic(PANIC_SPEED),
            MobControl::default_look_at_target_sink(),
            MobControl::move_to_target_sink(),
            MobControl::count_down_cooldown_ticks(MemoryModuleType::LikedNoteblockCooldownTicks),
            MobControl::count_down_cooldown_ticks(MemoryModuleType::ItemPickupCooldownTicks),
        ],
    )
}

fn idle_activity() -> ActivityData<MobControl> {
    ActivityData::with_conditions(
        Activity::Idle,
        vec![
            (
                0,
                MobControl::one_shot(MobTrigger::GoToWantedItem(GoToWantedItem::new(
                    WANTED_ITEM_SPEED,
                    true,
                    WANTED_ITEM_RANGE,
                ))),
            ),
            (
                1,
                MobControl::behavior_for(
                    MobBehavior::AllayGiveItems(GoAndGiveItemsToTarget::new()),
                    GIVE_TIMEOUT,
                    GIVE_TIMEOUT,
                ),
            ),
            (
                2,
                MobControl::one_shot(MobTrigger::StayCloseToTarget(StayCloseToTarget::new())),
            ),
            (
                3,
                MobControl::one_shot(MobTrigger::SetEntityLookTargetSometimes(
                    SetEntityLookTargetSometimes::new(
                        EntityFilter::Any,
                        LOOK_DISTANCE,
                        UniformInt::of(30, 60),
                    ),
                )),
            ),
            (
                4,
                MobControl::Gate(Gate::run_one(vec![
                    (
                        MobControl::one_shot(MobTrigger::RandomStroll(RandomStroll::fly(
                            STROLL_SPEED,
                        ))),
                        2,
                    ),
                    (
                        MobControl::one_shot(MobTrigger::SetWalkTargetFromLookTarget(
                            SetWalkTargetFromLookTarget::new(STROLL_SPEED, WALK_FROM_LOOK_DISTANCE),
                        )),
                        2,
                    ),
                    (MobControl::DoNothing(DoNothing::new(30, 60)), 1),
                ])),
            ),
        ],
        Vec::new(),
    )
}

pub fn allay_activities() -> Vec<ActivityData<MobControl>> {
    vec![core_activity(), idle_activity()]
}

pub fn update_activity(brain: &mut BehaviorBrain<MobControl>) {
    brain
        .state_mut()
        .set_active_activity_to_first_valid(&[Activity::Idle]);
}

fn global_pos(world: &impl AiWorld, pos: BlockPos) -> GlobalPos {
    GlobalPos {
        dimension: world.dimension().to_owned(),
        pos,
    }
}

pub fn hear_noteblock<W: AiWorld>(cx: &mut BehaviorCx<'_, '_, W>, pos: BlockPos) {
    let heard = global_pos(cx.ai.world, pos);
    let memories = &mut cx.brain.memories;
    let remembered = memories
        .get(MemoryModuleType::LikedNoteblockPosition)
        .and_then(MemoryValue::as_global_pos)
        .cloned();
    let refresh = match remembered {
        None => {
            memories.set(
                MemoryModuleType::LikedNoteblockPosition,
                MemoryValue::GlobalPos(heard),
            );
            true
        }
        Some(current) => current == heard,
    };
    if refresh {
        memories.set(
            MemoryModuleType::LikedNoteblockCooldownTicks,
            MemoryValue::Int(allay::LIKED_NOTEBLOCK_COOLDOWN),
        );
    }
}

fn should_deposit_at_liked_noteblock<W: AiWorld>(
    cx: &BehaviorCx<'_, '_, W>,
    noteblock: &GlobalPos,
) -> bool {
    let near = noteblock.dimension == cx.ai.world.dimension() && {
        let from = cx.ai.mob.block_pos();
        let (dx, dy, dz) = (
            i64::from(noteblock.pos.x - from.x),
            i64::from(noteblock.pos.y - from.y),
            i64::from(noteblock.pos.z - from.z),
        );
        dx * dx + dy * dy + dz * dz
            <= i64::from(allay::NOTEBLOCK_REACH) * i64::from(allay::NOTEBLOCK_REACH)
    };
    near && cx
        .ai
        .world
        .state(noteblock.pos)
        .is_some_and(|state| state.kind() == Some(rspigot_blocks::BlockKind::NoteBlock))
        && cx
            .brain
            .memories
            .has_value(MemoryModuleType::LikedNoteblockCooldownTicks)
}

pub fn liked_player<W: AiWorld>(cx: &BehaviorCx<'_, '_, W>) -> Option<crate::world::Living> {
    let uuid = cx
        .brain
        .memories
        .get(MemoryModuleType::LikedPlayer)
        .and_then(MemoryValue::as_uuid)?;
    let player = cx.ai.world.living_by_uuid(uuid)?;
    let playing = player.player && !player.spectator;
    (playing
        && cx.ai.mob.distance_to_sqr(player.position)
            < allay::LIKED_PLAYER_RANGE * allay::LIKED_PLAYER_RANGE)
        .then_some(player)
}

pub fn item_deposit_position<W: AiWorld>(
    cx: &mut BehaviorCx<'_, '_, W>,
) -> Option<PositionTracker> {
    let noteblock = cx
        .brain
        .memories
        .get(MemoryModuleType::LikedNoteblockPosition)
        .and_then(MemoryValue::as_global_pos)
        .cloned();
    if let Some(noteblock) = noteblock {
        if should_deposit_at_liked_noteblock(cx, &noteblock) {
            return Some(PositionTracker::of_block(noteblock.pos.offset(0, 1, 0)));
        }
        cx.brain
            .memories
            .erase(MemoryModuleType::LikedNoteblockPosition);
    }
    liked_player(cx).map(|player| PositionTracker::of_entity(player.id, true))
}

fn has_wanted_item<W: AiWorld>(cx: &BehaviorCx<'_, '_, W>) -> bool {
    cx.brain
        .memories
        .has_value(MemoryModuleType::NearestVisibleWantedItem)
}

fn set_walk_and_look<W: AiWorld>(
    cx: &mut BehaviorCx<'_, '_, W>,
    target: PositionTracker,
    speed: f32,
    close_enough: i32,
) {
    cx.brain
        .memories
        .set(MemoryModuleType::LookTarget, MemoryValue::Tracker(target));
    cx.brain.memories.set(
        MemoryModuleType::WalkTarget,
        MemoryValue::WalkTarget(WalkTarget {
            target,
            speed_modifier: speed,
            close_enough_dist: close_enough,
        }),
    );
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StayCloseToTarget;

impl StayCloseToTarget {
    pub fn new() -> Self {
        Self
    }

    pub fn conditions(&self) -> Vec<Condition> {
        vec![
            (MemoryModuleType::LookTarget, MemoryStatus::Registered),
            (MemoryModuleType::WalkTarget, MemoryStatus::Registered),
        ]
    }
}

impl Trigger for StayCloseToTarget {
    fn trigger<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        let Some(target) = item_deposit_position(cx) else {
            return false;
        };
        if has_wanted_item(cx) {
            return false;
        }
        let Some(at) = target.current_position(cx.ai.world) else {
            return false;
        };
        let reach = f64::from(CLOSE_ENOUGH);
        if cx.ai.mob.position.distance_squared(at) < reach * reach {
            return false;
        }
        cx.brain
            .memories
            .set(MemoryModuleType::LookTarget, MemoryValue::Tracker(target));
        cx.brain.memories.set(
            MemoryModuleType::WalkTarget,
            MemoryValue::WalkTarget(WalkTarget {
                target,
                speed_modifier: GIVE_SPEED,
                close_enough_dist: CLOSE_ENOUGH,
            }),
        );
        true
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct GoAndGiveItemsToTarget {
    conditions: [Condition; 3],
}

impl GoAndGiveItemsToTarget {
    pub fn new() -> Self {
        Self {
            conditions: [
                (MemoryModuleType::LookTarget, MemoryStatus::Registered),
                (MemoryModuleType::WalkTarget, MemoryStatus::Registered),
                (
                    MemoryModuleType::ItemPickupCooldownTicks,
                    MemoryStatus::Registered,
                ),
            ],
        }
    }

    fn can_throw<W: AiWorld>(&self, cx: &mut BehaviorCx<'_, '_, W>) -> bool {
        cx.ai.mob.species.allay.has_item_to_throw && item_deposit_position(cx).is_some()
    }
}

impl Default for GoAndGiveItemsToTarget {
    fn default() -> Self {
        Self::new()
    }
}

impl Behavior for GoAndGiveItemsToTarget {
    fn entry_condition(&self) -> &[Condition] {
        &self.conditions
    }

    fn check_extra_start_conditions<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>) -> bool {
        self.can_throw(cx)
    }

    fn can_still_use<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        self.can_throw(cx)
    }

    fn start<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        if let Some(target) = item_deposit_position(cx) {
            set_walk_and_look(cx, target, GIVE_SPEED, GIVE_CLOSE_ENOUGH);
        }
    }

    fn tick<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        let Some(target) = item_deposit_position(cx) else {
            return;
        };
        let Some(at) = target.current_position(cx.ai.world) else {
            return;
        };
        if at.distance_squared(cx.ai.mob.eye_position()).sqrt() < GIVE_REACH {
            cx.ai.act(MobAction::Air(crate::AirAction::Allay(
                AllayAction::ThrowItem { target: at },
            )));
            cx.brain.memories.set(
                MemoryModuleType::ItemPickupCooldownTicks,
                MemoryValue::Int(GIVE_COOLDOWN),
            );
        }
    }
}

pub fn deliver<W: AiWorld>(cx: &mut BehaviorCx<'_, '_, W>, message: crate::BrainMessage) {
    if let crate::BrainMessage::AllayHeardNoteblock { pos } = message {
        hear_noteblock(cx, pos);
    }
}
