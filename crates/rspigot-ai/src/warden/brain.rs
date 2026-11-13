use rspigot_domain::warden::{
    DIGGING_DURATION, EMERGE_DURATION, ROAR_DURATION, SNIFFING_DURATION, Standing, Suspect,
};
use rspigot_entities::EntityKind;

use super::behaviors::{
    DigCooldownSetter, Digging, Emerging, ForceUnmount, Roar, SetLookAtAttackTarget, SetRoarTarget,
    SetWardenLookTarget, Sniffing, SonicBoom, TryToSniff,
};
use super::sensor::WardenEntitySensor;
use super::state::{
    ANGER_TICK_INTERVAL, DARKNESS_INTERVAL, WardenAction, active_anger, can_target_entity,
};
use crate::action::MobAction;
use crate::behavior::{
    Activity, ActivityData, BehaviorBrain, BehaviorCx, DoNothing, Gate, GoToTargetLocation,
    MeleeAttack, MemoryModuleType, MemoryStatus, MobBehavior, MobControl, MobTrigger, PlayerSensor,
    RandomStroll, SensorKind, SetWalkTargetFromAttackTargetIfTargetOutOfReach, StopAttackCondition,
    StopAttackingIfTargetInvalid, TargetErased,
};
use crate::brain::AiContext;
use crate::world::AiWorld;

const IDLE_STROLL_SPEED: f32 = 0.5;
const INVESTIGATE_SPEED: f32 = 0.7;
const INVESTIGATE_CLOSE_ENOUGH: i32 = 2;
const FIGHT_SPEED: f32 = 1.2;
const MELEE_COOLDOWN: i32 = 18;
const SWIM_CHANCE: f32 = 0.8;
const FOLLOW_RANGE: &str = "minecraft:follow_range";

pub const WARDEN_MEMORIES: [MemoryModuleType; 4] = [
    MemoryModuleType::NearestVisibleNemesis,
    MemoryModuleType::RecentProjectile,
    MemoryModuleType::TouchCooldown,
    MemoryModuleType::VibrationCooldown,
];

pub fn warden_sensors() -> Vec<SensorKind> {
    vec![
        SensorKind::NearestPlayers(PlayerSensor),
        SensorKind::WardenEntity(WardenEntitySensor),
    ]
}

fn core_activity() -> ActivityData<MobControl> {
    ActivityData::create(
        Activity::Core,
        0,
        vec![
            MobControl::swim(SWIM_CHANCE),
            MobControl::one_shot(MobTrigger::WardenSetLookTarget(SetWardenLookTarget)),
            MobControl::look_at_target_sink(45, 90),
            MobControl::move_to_target_sink(),
        ],
    )
}

fn emerge_activity() -> ActivityData<MobControl> {
    ActivityData::create_erasing(
        Activity::Emerge,
        5,
        vec![MobControl::behavior_for(
            MobBehavior::WardenEmerging(Emerging),
            EMERGE_DURATION,
            EMERGE_DURATION,
        )],
        MemoryModuleType::IsEmerging,
    )
}

fn digging_activity() -> ActivityData<MobControl> {
    ActivityData::with_conditions(
        Activity::Dig,
        vec![
            (
                0,
                MobControl::behavior(MobBehavior::WardenForceUnmount(ForceUnmount)),
            ),
            (
                1,
                MobControl::behavior_for(
                    MobBehavior::WardenDigging(Digging::default()),
                    DIGGING_DURATION,
                    DIGGING_DURATION,
                ),
            ),
        ],
        vec![
            (MemoryModuleType::RoarTarget, MemoryStatus::ValueAbsent),
            (MemoryModuleType::DigCooldown, MemoryStatus::ValueAbsent),
        ],
    )
}

fn idle_activity() -> ActivityData<MobControl> {
    ActivityData::create(
        Activity::Idle,
        10,
        vec![
            MobControl::one_shot(MobTrigger::WardenSetRoarTarget(SetRoarTarget)),
            MobControl::one_shot(MobTrigger::WardenTryToSniff(TryToSniff)),
            MobControl::Gate(Gate::run_one_when(
                vec![(MemoryModuleType::IsSniffing, MemoryStatus::ValueAbsent)],
                vec![
                    (
                        MobControl::one_shot(MobTrigger::RandomStroll(RandomStroll::stroll(
                            IDLE_STROLL_SPEED,
                        ))),
                        2,
                    ),
                    (MobControl::DoNothing(DoNothing::new(30, 60)), 1),
                ],
            )),
        ],
    )
}

fn investigate_activity() -> ActivityData<MobControl> {
    ActivityData::create_erasing(
        Activity::Investigate,
        5,
        vec![
            MobControl::one_shot(MobTrigger::WardenSetRoarTarget(SetRoarTarget)),
            MobControl::one_shot(MobTrigger::GoToTargetLocation(GoToTargetLocation::new(
                MemoryModuleType::DisturbanceLocation,
                INVESTIGATE_CLOSE_ENOUGH,
                INVESTIGATE_SPEED,
            ))),
        ],
        MemoryModuleType::DisturbanceLocation,
    )
}

fn sniffing_activity() -> ActivityData<MobControl> {
    ActivityData::create_erasing(
        Activity::Sniff,
        5,
        vec![
            MobControl::one_shot(MobTrigger::WardenSetRoarTarget(SetRoarTarget)),
            MobControl::behavior_for(
                MobBehavior::WardenSniffing(Sniffing),
                SNIFFING_DURATION,
                SNIFFING_DURATION,
            ),
        ],
        MemoryModuleType::IsSniffing,
    )
}

fn roar_activity() -> ActivityData<MobControl> {
    ActivityData::create_erasing(
        Activity::Roar,
        10,
        vec![MobControl::behavior_for(
            MobBehavior::WardenRoar(Roar),
            ROAR_DURATION,
            ROAR_DURATION,
        )],
        MemoryModuleType::RoarTarget,
    )
}

fn fight_activity() -> ActivityData<MobControl> {
    let follow_range = EntityKind::Warden
        .default_attribute(FOLLOW_RANGE)
        .unwrap_or(0.0);
    ActivityData::create_erasing(
        Activity::Fight,
        10,
        vec![
            MobControl::one_shot(MobTrigger::WardenDigCooldownSetter(DigCooldownSetter)),
            MobControl::one_shot(MobTrigger::StopAttackingIfTargetInvalid(
                StopAttackingIfTargetInvalid::new(
                    StopAttackCondition::WardenNotAngry,
                    TargetErased::Warden,
                    false,
                ),
            )),
            MobControl::one_shot(MobTrigger::WardenLookAtAttackTarget(
                SetLookAtAttackTarget::new(follow_range as f32),
            )),
            MobControl::one_shot(MobTrigger::SetWalkTargetFromAttackTargetIfTargetOutOfReach(
                SetWalkTargetFromAttackTargetIfTargetOutOfReach::new(FIGHT_SPEED),
            )),
            MobControl::behavior_for(
                MobBehavior::WardenSonicBoom(SonicBoom),
                super::behaviors::SONIC_BOOM_DURATION,
                super::behaviors::SONIC_BOOM_DURATION,
            ),
            MobControl::one_shot(MobTrigger::MeleeAttack(MeleeAttack::new(MELEE_COOLDOWN))),
        ],
        MemoryModuleType::AttackTarget,
    )
}

pub fn warden_activities() -> Vec<ActivityData<MobControl>> {
    vec![
        core_activity(),
        emerge_activity(),
        digging_activity(),
        idle_activity(),
        roar_activity(),
        fight_activity(),
        investigate_activity(),
        sniffing_activity(),
    ]
}

pub fn update_activity(brain: &mut BehaviorBrain<MobControl>) {
    brain.state_mut().set_active_activity_to_first_valid(&[
        Activity::Emerge,
        Activity::Dig,
        Activity::Roar,
        Activity::Fight,
        Activity::Investigate,
        Activity::Sniff,
        Activity::Idle,
    ]);
}

fn tick_anger<W: AiWorld>(cx: &mut BehaviorCx<'_, '_, W>) {
    let mob = cx.ai.mob;
    let world = cx.ai.world;
    cx.ai.brain.warden.anger.tick(
        |suspect| match world.living(suspect.id) {
            Some(target) if can_target_entity(mob, world, &target) => Standing::Valid,
            Some(_) => Standing::Invalid,
            None => Standing::Removed {
                keeps_anger: world.removal_keeps_anger(suspect.id, suspect.uuid, suspect.player),
            },
        },
        |uuid| {
            world.living_by_uuid(uuid).map(|target| Suspect {
                id: target.id,
                uuid,
                player: target.player,
            })
        },
    );
    let level = active_anger(cx);
    cx.ai
        .act(MobAction::Warden(WardenAction::ClientAngerLevel(level)));
}

pub fn after_brain_tick<W: AiWorld>(
    brain: &mut BehaviorBrain<MobControl>,
    ai: &mut AiContext<'_, W>,
) {
    let tick_count = ai.mob.tick_count;
    let phase = tick_count.wrapping_add(ai.mob.id.0);
    {
        let mut cx = BehaviorCx {
            ai: &mut *ai,
            brain: brain.state_mut(),
        };
        if phase % DARKNESS_INTERVAL == 0 {
            cx.ai.act(MobAction::Warden(WardenAction::Darkness));
        }
        if tick_count % ANGER_TICK_INTERVAL == 0 {
            tick_anger(&mut cx);
        }
    }
    update_activity(brain);
}
