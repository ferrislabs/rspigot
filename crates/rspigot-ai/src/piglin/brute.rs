use rspigot_entities::EntityKind;

use crate::action::MobAction;
use crate::behavior::{
    Activity, ActivityData, BehaviorBrain, DoNothing, EntityFilter, Gate, InteractWith,
    InteractWithDoor, MeleeAttack, MemoryModuleType, MobControl, MobTrigger, RandomStroll,
    SensorKind, SetEntityLookTarget, SetLookAndInteract,
    SetWalkTargetFromAttackTargetIfTargetOutOfReach, StartAttacking, StopAttackCondition,
    StopAttackingIfTargetInvalid, StopBeingAngryIfTargetDead, StrollAroundPoi, StrollToPoi,
    TargetErased, TargetFinder,
};
use crate::piglin::sensors::PiglinBruteSensor;
use crate::piglin::targets::NetherFinder;
use crate::random::RandomSource;
use crate::world::AiWorld;

const ACTIVITY_SOUND_CHANCE: f32 = 0.0125;

fn idle_look_behaviors() -> Gate<MobControl> {
    let look = |filter| {
        MobControl::one_shot(MobTrigger::SetEntityLookTarget(SetEntityLookTarget::new(
            filter, 8.0,
        )))
    };
    Gate::run_one(vec![
        (look(EntityFilter::Kind(EntityKind::Player)), 1),
        (look(EntityFilter::Kind(EntityKind::Piglin)), 1),
        (look(EntityFilter::Kind(EntityKind::PiglinBrute)), 1),
        (look(EntityFilter::Any), 1),
        (MobControl::DoNothing(DoNothing::new(30, 60)), 1),
    ])
}

fn idle_movement_behaviors() -> Gate<MobControl> {
    let interact = |kind| {
        MobControl::one_shot(MobTrigger::InteractWith(InteractWith::new(
            kind,
            8,
            MemoryModuleType::InteractionTarget,
            0.6,
            2,
        )))
    };
    Gate::run_one(vec![
        (
            MobControl::one_shot(MobTrigger::RandomStroll(RandomStroll::stroll(0.6))),
            2,
        ),
        (interact(EntityKind::Piglin), 2),
        (interact(EntityKind::PiglinBrute), 2),
        (
            MobControl::one_shot(MobTrigger::StrollToPoi(StrollToPoi::new(
                MemoryModuleType::Home,
                0.6,
                2,
                100,
            ))),
            2,
        ),
        (
            MobControl::one_shot(MobTrigger::StrollAroundPoi(StrollAroundPoi::new(
                MemoryModuleType::Home,
                0.6,
                5,
            ))),
            2,
        ),
        (MobControl::DoNothing(DoNothing::new(30, 60)), 1),
    ])
}

pub fn brute_activities() -> Vec<ActivityData<MobControl>> {
    let core = ActivityData::create(
        Activity::Core,
        0,
        vec![
            MobControl::default_look_at_target_sink(),
            MobControl::move_to_target_sink(),
            MobControl::one_shot(MobTrigger::InteractWithDoor(InteractWithDoor::default())),
            MobControl::one_shot(MobTrigger::StopBeingAngryIfTargetDead(
                StopBeingAngryIfTargetDead,
            )),
        ],
    );
    let idle = ActivityData::create(
        Activity::Idle,
        10,
        vec![
            MobControl::one_shot(MobTrigger::StartAttacking(StartAttacking::new(
                TargetFinder::Nether(NetherFinder::PiglinBrute),
            ))),
            MobControl::Gate(idle_look_behaviors()),
            MobControl::Gate(idle_movement_behaviors()),
            MobControl::one_shot(MobTrigger::SetLookAndInteract(SetLookAndInteract::new(
                EntityKind::Player,
                4,
            ))),
        ],
    );
    let fight = ActivityData::create_erasing(
        Activity::Fight,
        10,
        vec![
            MobControl::one_shot(MobTrigger::StopAttackingIfTargetInvalid(
                StopAttackingIfTargetInvalid::new(
                    StopAttackCondition::NotNearestValid(NetherFinder::PiglinBrute),
                    TargetErased::Nothing,
                    true,
                ),
            )),
            MobControl::one_shot(MobTrigger::SetWalkTargetFromAttackTargetIfTargetOutOfReach(
                SetWalkTargetFromAttackTargetIfTargetOutOfReach::new(1.0),
            )),
            MobControl::one_shot(MobTrigger::MeleeAttack(MeleeAttack::new(20))),
        ],
        MemoryModuleType::AttackTarget,
    );
    vec![core, idle, fight]
}

pub fn brute_sensors() -> Vec<SensorKind> {
    use crate::behavior::{
        HurtBySensor, ItemWish, NearestItemSensor, NearestLivingEntitySensor, PlayerSensor,
    };
    vec![
        SensorKind::NearestLivingEntities(NearestLivingEntitySensor),
        SensorKind::NearestPlayers(PlayerSensor),
        SensorKind::NearestItems(NearestItemSensor::new(ItemWish::PiglinBrute)),
        SensorKind::HurtBy(HurtBySensor),
        SensorKind::PiglinBruteSpecific(PiglinBruteSensor),
    ]
}

pub fn brute_memories() -> &'static [MemoryModuleType] {
    &[
        MemoryModuleType::DoorsToClose,
        MemoryModuleType::NearestVisibleAdultPiglins,
    ]
}

pub fn update_activity<W: AiWorld>(
    brain: &mut BehaviorBrain<MobControl>,
    ai: &mut crate::brain::AiContext<'_, W>,
) {
    let before = brain.state().active_non_core_activity();
    brain
        .state_mut()
        .set_active_activity_to_first_valid(&[Activity::Fight, Activity::Idle]);
    let after = brain.state().active_non_core_activity();
    if before != after && after == Some(Activity::Fight) {
        ai.act(MobAction::Voice("minecraft:entity.piglin_brute.angry"));
    }
    ai.body.aggressive = brain
        .state()
        .memories
        .has_value(MemoryModuleType::AttackTarget);
    if ai.brain.random.next_float() < ACTIVITY_SOUND_CHANCE
        && brain.state().active_non_core_activity() == Some(Activity::Fight)
    {
        ai.act(MobAction::Voice("minecraft:entity.piglin_brute.angry"));
    }
}
