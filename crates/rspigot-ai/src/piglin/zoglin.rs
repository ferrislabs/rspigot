use crate::action::{BrainMessage, MobAction};
use crate::behavior::{
    Activity, ActivityData, BehaviorBrain, BehaviorCx, DoNothing, EntityFilter, Gate, MeleeAttack,
    MemoryModuleType, MemoryValue, MobControl, MobPredicate, MobTrigger, RandomStroll, SensorKind,
    SetEntityLookTargetSometimes, SetWalkTargetFromAttackTargetIfTargetOutOfReach,
    SetWalkTargetFromLookTarget, StartAttacking, StopAttackingIfTargetInvalid, TargetFinder,
    UniformInt, is_other_target_much_further_away_than_current_attack_target,
};
use crate::piglin::targets::NetherFinder;
use crate::world::AiWorld;

const ATTACK_TARGET_MEMORY_TICKS: i64 = 200;
const MUCH_FURTHER_AWAY: f64 = 4.0;
const STROLL_SPEED: f32 = 0.4;
const MELEE_ADULT_COOLDOWN: i32 = 40;
const MELEE_BABY_COOLDOWN: i32 = 15;

pub fn zoglin_activities() -> Vec<ActivityData<MobControl>> {
    let core = ActivityData::create(
        Activity::Core,
        0,
        vec![
            MobControl::default_look_at_target_sink(),
            MobControl::move_to_target_sink(),
        ],
    );
    let idle = ActivityData::create(
        Activity::Idle,
        10,
        vec![
            MobControl::one_shot(MobTrigger::StartAttacking(StartAttacking::new(
                TargetFinder::Nether(NetherFinder::Zoglin),
            ))),
            MobControl::one_shot(MobTrigger::SetEntityLookTargetSometimes(
                SetEntityLookTargetSometimes::new(EntityFilter::Any, 8.0, UniformInt::of(30, 60)),
            )),
            MobControl::Gate(Gate::run_one(vec![
                (
                    MobControl::one_shot(MobTrigger::RandomStroll(RandomStroll::stroll(
                        STROLL_SPEED,
                    ))),
                    2,
                ),
                (
                    MobControl::one_shot(MobTrigger::SetWalkTargetFromLookTarget(
                        SetWalkTargetFromLookTarget::new(STROLL_SPEED, 3),
                    )),
                    2,
                ),
                (MobControl::DoNothing(DoNothing::new(30, 60)), 1),
            ])),
        ],
    );
    let fight = ActivityData::create_erasing(
        Activity::Fight,
        10,
        vec![
            MobControl::one_shot(MobTrigger::SetWalkTargetFromAttackTargetIfTargetOutOfReach(
                SetWalkTargetFromAttackTargetIfTargetOutOfReach::new(1.0),
            )),
            MobControl::trigger_if(
                MobPredicate::IsAdult,
                MobControl::declared(MobTrigger::MeleeAttack(MeleeAttack::new(
                    MELEE_ADULT_COOLDOWN,
                ))),
            ),
            MobControl::trigger_if(
                MobPredicate::IsBaby,
                MobControl::declared(MobTrigger::MeleeAttack(MeleeAttack::new(
                    MELEE_BABY_COOLDOWN,
                ))),
            ),
            MobControl::one_shot(MobTrigger::StopAttackingIfTargetInvalid(
                StopAttackingIfTargetInvalid::default(),
            )),
        ],
        MemoryModuleType::AttackTarget,
    );
    vec![core, idle, fight]
}

pub fn zoglin_sensors() -> Vec<SensorKind> {
    use crate::behavior::{NearestLivingEntitySensor, PlayerSensor};
    vec![
        SensorKind::NearestLivingEntities(NearestLivingEntitySensor),
        SensorKind::NearestPlayers(PlayerSensor),
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
    if after == Some(Activity::Fight) && before != Some(Activity::Fight) {
        ai.act(MobAction::Voice("minecraft:entity.zoglin.angry"));
    }
    ai.body.aggressive = brain
        .state()
        .memories
        .has_value(MemoryModuleType::AttackTarget);
}

pub fn ambient_sound<W: AiWorld>(cx: &BehaviorCx<'_, '_, W>) -> &'static str {
    if cx.brain.memories.has_value(MemoryModuleType::AttackTarget) {
        "minecraft:entity.zoglin.angry"
    } else {
        "minecraft:entity.zoglin.ambient"
    }
}

pub fn deliver<W: AiWorld>(cx: &mut BehaviorCx<'_, '_, W>, message: BrainMessage) {
    let BrainMessage::ZoglinHurtBy { attacker } = message else {
        return;
    };
    let Some(attacker) = cx.ai.world.living(attacker) else {
        return;
    };
    if !crate::targeting::mob_can_attack(cx.ai.mob, cx.ai.world, &attacker)
        || is_other_target_much_further_away_than_current_attack_target(
            cx,
            &attacker,
            MUCH_FURTHER_AWAY,
        )
    {
        return;
    }
    cx.brain
        .memories
        .erase(MemoryModuleType::CantReachWalkTargetSince);
    cx.brain.memories.set_with_expiry(
        MemoryModuleType::AttackTarget,
        MemoryValue::Entity(attacker.id),
        ATTACK_TARGET_MEMORY_TICKS,
    );
}
