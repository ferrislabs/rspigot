use rspigot_entities::EntityKind;

use crate::action::{BrainMessage, MobAction};
use crate::behavior::{
    Activity, ActivityData, AnimalMakeLove, BabyFollowAdult, BehaviorBrain, BehaviorCx, DoNothing,
    EntityFilter, EraseMemoryIf, Gate, MeleeAttack, MemoryModuleType, MemoryValue, MobControl,
    MobPredicate, MobTrigger, RandomStroll, SensorKind, SetEntityLookTargetSometimes,
    SetWalkTargetAwayFrom, SetWalkTargetFromAttackTargetIfTargetOutOfReach,
    SetWalkTargetFromLookTarget, StartAttacking, StopAttackingIfTargetInvalid, TargetFinder,
    UniformInt, is_other_target_much_further_away_than_current_attack_target, nearest_target,
};
use crate::piglin::behaviors::BecomePassiveIfMemoryPresent;
use crate::piglin::sensors::HoglinSensor;
use crate::piglin::targets::NetherFinder;
use crate::world::{AiWorld, Living};

pub const RETREAT_DURATION: UniformInt = UniformInt::of(100, 400);
pub const ADULT_FOLLOW_RANGE: UniformInt = UniformInt::of(5, 16);
pub const ATTACK_TARGET_MEMORY_TICKS: i64 = 200;
const REPELLENT_PACIFY_TICKS: i32 = 200;
const REPELLENT_DISTANCE: f64 = 8.0;
const LOVE_SPEED: f32 = 0.6;
const LOVE_DISTANCE: i32 = 2;
const MELEE_ADULT_COOLDOWN: i32 = 40;
const MELEE_BABY_COOLDOWN: i32 = 15;
const OUTNUMBER_MARGIN: i32 = 1;
const HOGLIN_STROLL_SPEED: f32 = 0.4;
const RETREAT_SPEED: f32 = 1.3;
const RETREAT_DISTANCE: i32 = 15;
const AWAY_FROM_PIGLIN_DISTANCE: i32 = 8;
const AWAY_FROM_REPELLENT_SPEED: f32 = 1.0;
const MUCH_FURTHER_AWAY: f64 = 4.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HoglinCondition {
    WantsToStopFleeing,
}

impl HoglinCondition {
    pub fn test<W: AiWorld>(self, cx: &BehaviorCx<'_, '_, W>) -> bool {
        match self {
            Self::WantsToStopFleeing => wants_to_stop_fleeing(cx),
        }
    }
}

fn count<W: AiWorld>(cx: &BehaviorCx<'_, '_, W>, memory: MemoryModuleType) -> i32 {
    cx.brain
        .memories
        .get(memory)
        .and_then(MemoryValue::as_int)
        .unwrap_or(0)
}

pub fn piglins_outnumber_hoglins<W: AiWorld>(cx: &BehaviorCx<'_, '_, W>) -> bool {
    if cx.ai.mob.baby {
        return false;
    }
    let piglins = count(cx, MemoryModuleType::VisibleAdultPiglinCount);
    let hoglins = count(cx, MemoryModuleType::VisibleAdultHoglinCount);
    hoglins + OUTNUMBER_MARGIN < piglins
}

pub fn wants_to_stop_fleeing<W: AiWorld>(cx: &BehaviorCx<'_, '_, W>) -> bool {
    !cx.ai.mob.baby && !piglins_outnumber_hoglins(cx)
}

fn is_pacified<W: AiWorld>(cx: &BehaviorCx<'_, '_, W>) -> bool {
    cx.brain.memories.has_value(MemoryModuleType::Pacified)
}

fn visible_adult_hoglins<W: AiWorld>(cx: &BehaviorCx<'_, '_, W>) -> Vec<rspigot_domain::EntityId> {
    cx.brain
        .memories
        .get(MemoryModuleType::NearestVisibleAdultHoglins)
        .and_then(MemoryValue::as_entities)
        .map(<[_]>::to_vec)
        .unwrap_or_default()
}

pub fn set_avoid_target<W: AiWorld>(cx: &mut BehaviorCx<'_, '_, W>, avoid_target: &Living) {
    cx.brain.memories.erase(MemoryModuleType::AttackTarget);
    cx.brain.memories.erase(MemoryModuleType::WalkTarget);
    let duration = RETREAT_DURATION.sample(&mut cx.ai.brain.random);
    cx.brain.memories.set_with_expiry(
        MemoryModuleType::AvoidTarget,
        MemoryValue::Entity(avoid_target.id),
        i64::from(duration),
    );
}

pub fn retreat_from_nearest_target<W: AiWorld>(
    cx: &mut BehaviorCx<'_, '_, W>,
    new_avoid_target: &Living,
) {
    let avoid = memory_living(cx, MemoryModuleType::AvoidTarget);
    let nearest = nearest_target(cx, avoid, *new_avoid_target);
    let attack = memory_living(cx, MemoryModuleType::AttackTarget);
    let nearest = nearest_target(cx, attack, nearest);
    set_avoid_target(cx, &nearest);
}

fn memory_living<W: AiWorld>(
    cx: &BehaviorCx<'_, '_, W>,
    memory: MemoryModuleType,
) -> Option<Living> {
    cx.brain
        .memories
        .get(memory)
        .and_then(MemoryValue::as_entity)
        .and_then(|id| cx.ai.world.living(id))
}

fn set_attack_target<W: AiWorld>(cx: &mut BehaviorCx<'_, '_, W>, target: &Living) {
    cx.brain
        .memories
        .erase(MemoryModuleType::CantReachWalkTargetSince);
    cx.brain.memories.erase(MemoryModuleType::BreedTarget);
    cx.brain.memories.set_with_expiry(
        MemoryModuleType::AttackTarget,
        MemoryValue::Entity(target.id),
        ATTACK_TARGET_MEMORY_TICKS,
    );
}

pub fn set_attack_target_if_closer_than_current<W: AiWorld>(
    cx: &mut BehaviorCx<'_, '_, W>,
    new_target: &Living,
) {
    if is_pacified(cx) {
        return;
    }
    let current = memory_living(cx, MemoryModuleType::AttackTarget);
    let nearest = nearest_target(cx, current, *new_target);
    set_attack_target(cx, &nearest);
}

fn broadcast_attack_target<W: AiWorld>(cx: &mut BehaviorCx<'_, '_, W>, target: &Living) {
    for hoglin in visible_adult_hoglins(cx) {
        cx.ai.act(MobAction::Message {
            to: hoglin,
            message: BrainMessage::HoglinAttackIfCloser { target: target.id },
        });
    }
}

fn broadcast_retreat<W: AiWorld>(cx: &mut BehaviorCx<'_, '_, W>, target: &Living) {
    for hoglin in visible_adult_hoglins(cx) {
        cx.ai.act(MobAction::Message {
            to: hoglin,
            message: BrainMessage::HoglinRetreat { target: target.id },
        });
    }
}

pub fn on_hit_target<W: AiWorld>(cx: &mut BehaviorCx<'_, '_, W>, target: &Living) {
    if cx.ai.mob.baby {
        return;
    }
    if target.kind == EntityKind::Piglin && piglins_outnumber_hoglins(cx) {
        set_avoid_target(cx, target);
        broadcast_retreat(cx, target);
        return;
    }
    broadcast_attack_target(cx, target);
}

fn maybe_retaliate<W: AiWorld>(cx: &mut BehaviorCx<'_, '_, W>, attacker: &Living) {
    if cx.brain.is_active(Activity::Avoid) && attacker.kind == EntityKind::Piglin {
        return;
    }
    if attacker.kind == EntityKind::Hoglin {
        return;
    }
    if is_other_target_much_further_away_than_current_attack_target(cx, attacker, MUCH_FURTHER_AWAY)
    {
        return;
    }
    if crate::behavior::is_entity_attackable(cx.ai, &cx.brain.memories, attacker) {
        set_attack_target(cx, attacker);
        broadcast_attack_target(cx, attacker);
    }
}

pub fn was_hurt_by<W: AiWorld>(cx: &mut BehaviorCx<'_, '_, W>, attacker: &Living) {
    cx.brain.memories.erase(MemoryModuleType::Pacified);
    cx.brain.memories.erase(MemoryModuleType::BreedTarget);
    if cx.ai.mob.baby {
        retreat_from_nearest_target(cx, attacker);
        return;
    }
    maybe_retaliate(cx, attacker);
}

pub fn is_pos_near_nearest_repellent<W: AiWorld>(
    memories: &crate::behavior::Memories,
    pos: rspigot_domain::BlockPos,
) -> bool {
    memories
        .get(MemoryModuleType::NearestRepellent)
        .and_then(MemoryValue::as_block)
        .is_some_and(|repellent| {
            let dx = f64::from(repellent.x - pos.x);
            let dy = f64::from(repellent.y - pos.y);
            let dz = f64::from(repellent.z - pos.z);
            dx * dx + dy * dy + dz * dz < REPELLENT_DISTANCE * REPELLENT_DISTANCE
        })
}

pub fn sound_for_activity<W: AiWorld>(
    cx: &BehaviorCx<'_, '_, W>,
    activity: Activity,
) -> &'static str {
    if activity == Activity::Avoid || cx.ai.mob.species.hoglin.converting {
        return "minecraft:entity.hoglin.retreat";
    }
    if activity == Activity::Fight {
        return "minecraft:entity.hoglin.angry";
    }
    if cx
        .brain
        .memories
        .has_value(MemoryModuleType::NearestRepellent)
    {
        return "minecraft:entity.hoglin.retreat";
    }
    "minecraft:entity.hoglin.ambient"
}

pub fn update_activity<W: AiWorld>(
    brain: &mut BehaviorBrain<MobControl>,
    ai: &mut crate::brain::AiContext<'_, W>,
) {
    let before = brain.state().active_non_core_activity();
    brain.state_mut().set_active_activity_to_first_valid(&[
        Activity::Fight,
        Activity::Avoid,
        Activity::Idle,
    ]);
    let after = brain.state().active_non_core_activity();
    if before != after
        && let Some(activity) = after
    {
        let cx = BehaviorCx {
            ai: &mut *ai,
            brain: brain.state_mut(),
        };
        let sound = sound_for_activity(&cx, activity);
        cx.ai.act(MobAction::Voice(sound));
    }
    ai.body.aggressive = brain
        .state()
        .memories
        .has_value(MemoryModuleType::AttackTarget);
}

fn idle_movement() -> Gate<MobControl> {
    Gate::run_one(vec![
        (
            MobControl::one_shot(MobTrigger::RandomStroll(RandomStroll::stroll(
                HOGLIN_STROLL_SPEED,
            ))),
            2,
        ),
        (
            MobControl::one_shot(MobTrigger::SetWalkTargetFromLookTarget(
                SetWalkTargetFromLookTarget::new(HOGLIN_STROLL_SPEED, 3),
            )),
            2,
        ),
        (MobControl::DoNothing(DoNothing::new(30, 60)), 1),
    ])
}

fn look_sometimes() -> MobControl {
    MobControl::one_shot(MobTrigger::SetEntityLookTargetSometimes(
        SetEntityLookTargetSometimes::new(EntityFilter::Any, 8.0, UniformInt::of(30, 60)),
    ))
}

fn become_passive() -> MobControl {
    MobControl::one_shot(MobTrigger::BecomePassiveIfMemoryPresent(
        BecomePassiveIfMemoryPresent::new(
            MemoryModuleType::NearestRepellent,
            REPELLENT_PACIFY_TICKS,
        ),
    ))
}

fn love() -> MobControl {
    MobControl::animal_make_love(AnimalMakeLove::with_walk(
        EntityKind::Hoglin,
        LOVE_SPEED,
        LOVE_DISTANCE,
    ))
}

pub fn hoglin_activities() -> Vec<ActivityData<MobControl>> {
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
            become_passive(),
            love(),
            MobControl::one_shot(MobTrigger::SetWalkTargetAwayFrom(
                SetWalkTargetAwayFrom::pos(
                    MemoryModuleType::NearestRepellent,
                    AWAY_FROM_REPELLENT_SPEED,
                    AWAY_FROM_PIGLIN_DISTANCE,
                    true,
                ),
            )),
            MobControl::one_shot(MobTrigger::StartAttacking(StartAttacking::new(
                TargetFinder::Nether(NetherFinder::Hoglin),
            ))),
            MobControl::trigger_if(
                MobPredicate::IsAdult,
                MobControl::declared(MobTrigger::SetWalkTargetAwayFrom(
                    SetWalkTargetAwayFrom::entity(
                        MemoryModuleType::NearestVisibleAdultPiglin,
                        HOGLIN_STROLL_SPEED,
                        AWAY_FROM_PIGLIN_DISTANCE,
                        false,
                    ),
                )),
            ),
            look_sometimes(),
            MobControl::one_shot(MobTrigger::BabyFollowAdult(BabyFollowAdult::new(
                ADULT_FOLLOW_RANGE,
                LOVE_SPEED,
            ))),
            MobControl::Gate(idle_movement()),
        ],
    );
    let fight = ActivityData::create_erasing(
        Activity::Fight,
        10,
        vec![
            become_passive(),
            love(),
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
            MobControl::one_shot(MobTrigger::EraseMemoryIf(EraseMemoryIf::new(
                MobPredicate::HasMemory(MemoryModuleType::BreedTarget),
                MemoryModuleType::AttackTarget,
            ))),
        ],
        MemoryModuleType::AttackTarget,
    );
    let retreat = ActivityData::create_erasing(
        Activity::Avoid,
        10,
        vec![
            MobControl::one_shot(MobTrigger::SetWalkTargetAwayFrom(
                SetWalkTargetAwayFrom::entity(
                    MemoryModuleType::AvoidTarget,
                    RETREAT_SPEED,
                    RETREAT_DISTANCE,
                    false,
                ),
            )),
            MobControl::Gate(idle_movement()),
            look_sometimes(),
            MobControl::one_shot(MobTrigger::EraseMemoryIf(EraseMemoryIf::new(
                MobPredicate::Hoglin(HoglinCondition::WantsToStopFleeing),
                MemoryModuleType::AvoidTarget,
            ))),
        ],
        MemoryModuleType::AvoidTarget,
    );
    vec![core, idle, fight, retreat]
}

pub fn hoglin_sensors() -> Vec<SensorKind> {
    use crate::behavior::{AdultSensor, NearestLivingEntitySensor, PlayerSensor};
    vec![
        SensorKind::NearestLivingEntities(NearestLivingEntitySensor),
        SensorKind::NearestPlayers(PlayerSensor),
        SensorKind::NearestAdult(AdultSensor),
        SensorKind::HoglinSpecific(HoglinSensor),
    ]
}

pub fn hoglin_memories() -> &'static [MemoryModuleType] {
    &[]
}

pub fn deliver<W: AiWorld>(cx: &mut BehaviorCx<'_, '_, W>, message: BrainMessage) {
    match message {
        BrainMessage::HoglinAttackIfCloser { target } => {
            if let Some(target) = cx.ai.world.living(target) {
                set_attack_target_if_closer_than_current(cx, &target);
            }
        }
        BrainMessage::HoglinRetreat { target } => {
            if let Some(target) = cx.ai.world.living(target) {
                retreat_from_nearest_target(cx, &target);
            }
        }
        BrainMessage::HoglinHurtBy { attacker } => {
            if let Some(attacker) = cx.ai.world.living(attacker) {
                was_hurt_by(cx, &attacker);
            }
        }
        BrainMessage::HoglinHitTarget { target } => {
            if let Some(target) = cx.ai.world.living(target) {
                on_hit_target(cx, &target);
            }
        }
        _ => {}
    }
}
