mod container;
mod transport;

use rspigot_domain::BlockPos;
use rspigot_domain::entity::species::copper_golem::GolemState;
use rspigot_domain::item::Slot;
use rspigot_entities::EntityKind;

use crate::behavior::{
    Activity, ActivityData, BehaviorBrain, EntityFilter, Gate, HurtBySensor, InteractWithDoor,
    MemoryModuleType, MemoryStatus, MobBehavior, MobControl, MobTrigger, NearestLivingEntitySensor,
    RandomStroll, SensorKind, SetEntityLookTargetSometimes, UniformInt,
};
use crate::random::RandomSource;

pub use container::{
    ChestSide, add_items, chest_side, connected_pos, is_blocked_by_block, is_cat_sitting_on,
    is_chest_blocked_at, pick_up,
};
pub use transport::{
    COOLDOWN_TICKS, INTERACTION_TICKS, InteractionState, TargetVerdict,
    TransportItemsBetweenContainers, TransportState, TransportTarget,
};

pub const ITEM_GET: &str = "minecraft:entity.copper_golem.no_item_get";
pub const ITEM_NO_GET: &str = "minecraft:entity.copper_golem.no_item_no_get";
pub const ITEM_DROP: &str = "minecraft:entity.copper_golem.item_drop";
pub const ITEM_NO_DROP: &str = "minecraft:entity.copper_golem.item_no_drop";
pub const PANIC_SPEED: f32 = 1.5;
pub const LOOK_AT_PLAYER_DISTANCE: f32 = 6.0;

#[derive(Debug, Clone, PartialEq)]
pub enum CopperGolemAction {
    OpenContainer(BlockPos),
    CloseContainer(BlockPos),
    ClearOpenedChest,
    SetState(GolemState),
    SetContainer {
        pos: BlockPos,
        slots: Vec<(usize, Slot)>,
    },
    SetMainHand {
        stack: Slot,
        guaranteed: bool,
    },
    TargetBlock(BlockPos),
}

pub const COPPER_GOLEM_MEMORIES: [MemoryModuleType; 3] = [
    MemoryModuleType::TransportItemsCooldownTicks,
    MemoryModuleType::VisitedBlockPositions,
    MemoryModuleType::UnreachableTransportBlockPositions,
];

pub fn copper_golem_sensors() -> Vec<SensorKind> {
    vec![
        SensorKind::NearestLivingEntities(NearestLivingEntitySensor),
        SensorKind::HurtBy(HurtBySensor),
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
            MobControl::one_shot(MobTrigger::InteractWithDoor(InteractWithDoor::default())),
            MobControl::count_down_cooldown_ticks(MemoryModuleType::GazeCooldownTicks),
            MobControl::count_down_cooldown_ticks(MemoryModuleType::TransportItemsCooldownTicks),
        ],
    )
}

fn idle_activity() -> ActivityData<MobControl> {
    ActivityData::from_pairs(
        Activity::Idle,
        vec![
            (
                0,
                MobControl::behavior(MobBehavior::TransportItems(
                    TransportItemsBetweenContainers::new(),
                )),
            ),
            (
                1,
                MobControl::one_shot(MobTrigger::SetEntityLookTargetSometimes(
                    SetEntityLookTargetSometimes::new(
                        EntityFilter::Kind(EntityKind::Player),
                        LOOK_AT_PLAYER_DISTANCE,
                        UniformInt::of(40, 80),
                    ),
                )),
            ),
            (
                2,
                MobControl::Gate(Gate::run_one_when(
                    vec![
                        (MemoryModuleType::WalkTarget, MemoryStatus::ValueAbsent),
                        (
                            MemoryModuleType::TransportItemsCooldownTicks,
                            MemoryStatus::ValuePresent,
                        ),
                    ],
                    vec![
                        (
                            MobControl::one_shot(MobTrigger::RandomStroll(
                                RandomStroll::stroll_within(1.0, 2, 2),
                            )),
                            1,
                        ),
                        (
                            MobControl::DoNothing(crate::behavior::DoNothing::new(30, 60)),
                            1,
                        ),
                    ],
                )),
            ),
        ],
    )
}

pub fn copper_golem_activities() -> Vec<ActivityData<MobControl>> {
    vec![core_activity(), idle_activity()]
}

pub fn update_activity(brain: &mut BehaviorBrain<MobControl>) {
    brain
        .state_mut()
        .set_active_activity_to_first_valid(&[Activity::Idle]);
}

pub fn initial_cooldown(random: &mut impl RandomSource) -> i32 {
    rspigot_domain::entity::species::copper_golem::initial_transport_cooldown(random)
}
