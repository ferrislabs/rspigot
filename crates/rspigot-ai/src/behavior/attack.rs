use crate::action::{MobAction, TargetReason};
use crate::behavior::control::{BehaviorCx, Trigger};
use crate::behavior::memory::{Condition, MemoryModuleType, MemoryStatus, MemoryValue, WalkTarget};
use crate::behavior::sensor::is_entity_attackable;
use crate::behavior::support::{
    MobPredicate, get_entity, is_holding_usable_non_melee_weapon, is_within_attack_range,
    is_within_melee_range, look_at_entity, set_walk_target,
};
use crate::behavior::tracker::PositionTracker;
use crate::piglin::NetherFinder;
use crate::targeting;
use crate::world::{AiWorld, Hand, Living};

const TIRED_OF_TRYING_TO_REACH_TICKS: i64 = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetFinder {
    Memory(MemoryModuleType),
    HurtBySource,
    Nether(NetherFinder),
}

impl TargetFinder {
    fn find<W: AiWorld>(self, cx: &mut BehaviorCx<'_, '_, W>) -> Option<Living> {
        match self {
            Self::Memory(memory) => get_entity(cx, memory).and_then(|id| cx.ai.world.living(id)),
            Self::HurtBySource => cx
                .brain
                .memories
                .get(MemoryModuleType::HurtBy)
                .and_then(MemoryValue::as_hurt_by)
                .and_then(|damage| damage.entity)
                .and_then(|id| cx.ai.world.living(id)),
            Self::Nether(finder) => finder.find(cx),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct StartAttacking {
    can_attack: MobPredicate,
    finder: TargetFinder,
}

impl StartAttacking {
    pub fn new(finder: TargetFinder) -> Self {
        Self::when(MobPredicate::Always, finder)
    }

    pub fn when(can_attack: MobPredicate, finder: TargetFinder) -> Self {
        Self { can_attack, finder }
    }

    pub fn conditions(&self) -> Vec<Condition> {
        vec![
            (MemoryModuleType::AttackTarget, MemoryStatus::ValueAbsent),
            (
                MemoryModuleType::CantReachWalkTargetSince,
                MemoryStatus::Registered,
            ),
        ]
    }
}

impl Trigger for StartAttacking {
    fn trigger<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        if !self.can_attack.test(cx) {
            return false;
        }
        let Some(target) = self.finder.find(cx) else {
            return false;
        };
        if !targeting::mob_can_attack(cx.ai.mob, cx.ai.world, &target) {
            return false;
        }
        let reason = if target.player {
            TargetReason::ClosestPlayer
        } else {
            TargetReason::ClosestEntity
        };
        let previous = get_entity(cx, MemoryModuleType::AttackTarget);
        if cx
            .ai
            .request_brain_target(previous, Some(target.id), reason)
        {
            cx.brain.memories.set(
                MemoryModuleType::AttackTarget,
                MemoryValue::Entity(target.id),
            );
            cx.brain
                .memories
                .erase(MemoryModuleType::CantReachWalkTargetSince);
        }
        true
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StopAttackCondition {
    #[default]
    Never,
    NotNearestValid(NetherFinder),
    NotInNearestAttackablePlayers,
    NotAttackableRecently {
        invocations: i32,
        positives_left: i32,
    },
}

impl StopAttackCondition {
    pub fn not_attackable_recently(invocations: i32) -> Self {
        Self::NotAttackableRecently {
            invocations,
            positives_left: 0,
        }
    }

    fn test<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, target: &Living) -> bool {
        match self {
            Self::Never => false,
            Self::NotNearestValid(finder) => !finder.is_nearest_valid_attack_target(cx, target),
            Self::NotInNearestAttackablePlayers => {
                !(target.player
                    && cx
                        .brain
                        .memories
                        .get(MemoryModuleType::NearestVisibleAttackablePlayers)
                        .and_then(MemoryValue::as_entities)
                        .is_some_and(|players| players.contains(&target.id)))
            }
            Self::NotAttackableRecently {
                invocations,
                positives_left,
            } => {
                if is_entity_attackable(cx.ai, &cx.brain.memories, target) {
                    *positives_left = *invocations;
                    return false;
                }
                *positives_left -= 1;
                *positives_left < 0
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TargetErased {
    #[default]
    Nothing,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StopAttackingIfTargetInvalid {
    stop_attacking_when: StopAttackCondition,
    on_target_erased: TargetErased,
    can_grow_tired_of_trying_to_reach_target: bool,
}

impl Default for StopAttackingIfTargetInvalid {
    fn default() -> Self {
        Self::new(StopAttackCondition::Never, TargetErased::Nothing, true)
    }
}

impl StopAttackingIfTargetInvalid {
    pub fn new(
        stop_attacking_when: StopAttackCondition,
        on_target_erased: TargetErased,
        can_grow_tired_of_trying_to_reach_target: bool,
    ) -> Self {
        Self {
            stop_attacking_when,
            on_target_erased,
            can_grow_tired_of_trying_to_reach_target,
        }
    }

    pub fn conditions(&self) -> Vec<Condition> {
        vec![
            (MemoryModuleType::AttackTarget, MemoryStatus::ValuePresent),
            (
                MemoryModuleType::CantReachWalkTargetSince,
                MemoryStatus::Registered,
            ),
        ]
    }

    fn is_tired_of_trying_to_reach_target<W: AiWorld>(cx: &BehaviorCx<'_, '_, W>) -> bool {
        cx.brain
            .memories
            .get(MemoryModuleType::CantReachWalkTargetSince)
            .and_then(MemoryValue::as_long)
            .is_some_and(|since| cx.ai.world.game_time() - since > TIRED_OF_TRYING_TO_REACH_TICKS)
    }

    fn still_valid<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, target: &Living) -> bool {
        targeting::mob_can_attack(cx.ai.mob, cx.ai.world, target)
            && (!self.can_grow_tired_of_trying_to_reach_target
                || !Self::is_tired_of_trying_to_reach_target(cx))
            && target.alive
            && !self.stop_attacking_when.test(cx, target)
    }
}

impl Trigger for StopAttackingIfTargetInvalid {
    fn trigger<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        let target =
            get_entity(cx, MemoryModuleType::AttackTarget).and_then(|id| cx.ai.world.living(id));
        if target.is_some_and(|target| self.still_valid(cx, &target)) {
            return true;
        }
        let previous = get_entity(cx, MemoryModuleType::AttackTarget);
        if cx
            .ai
            .request_brain_target(previous, None, TargetReason::Unknown)
        {
            match self.on_target_erased {
                TargetErased::Nothing => {}
            }
            cx.brain.memories.erase(MemoryModuleType::AttackTarget);
        }
        true
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MeleeAttack {
    can_attack: MobPredicate,
    cooldown_between_attacks: i32,
}

impl MeleeAttack {
    pub fn new(cooldown_between_attacks: i32) -> Self {
        Self::when(MobPredicate::Always, cooldown_between_attacks)
    }

    pub fn when(can_attack: MobPredicate, cooldown_between_attacks: i32) -> Self {
        Self {
            can_attack,
            cooldown_between_attacks,
        }
    }

    pub fn conditions(&self) -> Vec<Condition> {
        vec![
            (MemoryModuleType::LookTarget, MemoryStatus::Registered),
            (MemoryModuleType::AttackTarget, MemoryStatus::ValuePresent),
            (
                MemoryModuleType::AttackCoolingDown,
                MemoryStatus::ValueAbsent,
            ),
            (
                MemoryModuleType::NearestVisibleLivingEntities,
                MemoryStatus::ValuePresent,
            ),
        ]
    }
}

impl Trigger for MeleeAttack {
    fn trigger<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        let Some(target) =
            get_entity(cx, MemoryModuleType::AttackTarget).and_then(|id| cx.ai.world.living(id))
        else {
            return false;
        };
        if !self.can_attack.test(cx)
            || is_holding_usable_non_melee_weapon(cx)
            || !is_within_melee_range(cx, &target)
        {
            return false;
        }
        let visible = cx
            .brain
            .memories
            .get(MemoryModuleType::NearestVisibleLivingEntities)
            .and_then(MemoryValue::as_visible)
            .is_some_and(|visible| visible.contains_entity(cx.ai, &cx.brain.memories, target.id));
        if !visible {
            return false;
        }
        look_at_entity(cx, target.id);
        cx.ai.act(MobAction::Swing(Hand::Main));
        cx.ai.act(MobAction::Attack(target.id));
        cx.brain.memories.set_with_expiry(
            MemoryModuleType::AttackCoolingDown,
            MemoryValue::Bool(true),
            i64::from(self.cooldown_between_attacks),
        );
        true
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SetWalkTargetFromAttackTargetIfTargetOutOfReach {
    speed_modifier: f32,
}

impl SetWalkTargetFromAttackTargetIfTargetOutOfReach {
    pub fn new(speed_modifier: f32) -> Self {
        Self { speed_modifier }
    }

    pub fn conditions(&self) -> Vec<Condition> {
        vec![
            (MemoryModuleType::WalkTarget, MemoryStatus::Registered),
            (MemoryModuleType::LookTarget, MemoryStatus::Registered),
            (MemoryModuleType::AttackTarget, MemoryStatus::ValuePresent),
            (
                MemoryModuleType::NearestVisibleLivingEntities,
                MemoryStatus::Registered,
            ),
        ]
    }
}

impl Trigger for SetWalkTargetFromAttackTargetIfTargetOutOfReach {
    fn trigger<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        let Some(target) =
            get_entity(cx, MemoryModuleType::AttackTarget).and_then(|id| cx.ai.world.living(id))
        else {
            return false;
        };
        let visible = cx
            .brain
            .memories
            .get(MemoryModuleType::NearestVisibleLivingEntities)
            .and_then(MemoryValue::as_visible)
            .is_some_and(|visible| visible.contains_entity(cx.ai, &cx.brain.memories, target.id));
        if visible && is_within_attack_range(cx, &target, 1) {
            cx.brain.memories.erase(MemoryModuleType::WalkTarget);
            return true;
        }
        look_at_entity(cx, target.id);
        set_walk_target(
            cx,
            WalkTarget {
                target: PositionTracker::of_entity(target.id, false),
                speed_modifier: self.speed_modifier,
                close_enough_dist: 0,
            },
        );
        true
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct EraseMemoryIf {
    predicate: MobPredicate,
    memory: MemoryModuleType,
}

impl EraseMemoryIf {
    pub fn new(predicate: MobPredicate, memory: MemoryModuleType) -> Self {
        Self { predicate, memory }
    }

    pub fn conditions(&self) -> Vec<Condition> {
        vec![(self.memory, MemoryStatus::ValuePresent)]
    }
}

impl Trigger for EraseMemoryIf {
    fn trigger<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        if self.predicate.test(cx) {
            cx.brain.memories.erase(self.memory);
            return true;
        }
        false
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StopBeingAngryIfTargetDead;

impl StopBeingAngryIfTargetDead {
    pub fn conditions(&self) -> Vec<Condition> {
        vec![(MemoryModuleType::AngryAt, MemoryStatus::ValuePresent)]
    }
}

impl Trigger for StopBeingAngryIfTargetDead {
    fn trigger<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        let Some(uuid) = cx
            .brain
            .memories
            .get(MemoryModuleType::AngryAt)
            .and_then(MemoryValue::as_uuid)
        else {
            return true;
        };
        let forgives = cx
            .ai
            .world
            .living_by_uuid(uuid)
            .filter(|target| !target.alive)
            .is_some_and(|target| !target.player || cx.ai.world.forgive_dead_players());
        if forgives {
            cx.brain.memories.erase(MemoryModuleType::AngryAt);
        }
        true
    }
}
