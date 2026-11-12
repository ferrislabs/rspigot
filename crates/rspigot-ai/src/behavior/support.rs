use rspigot_domain::{BlockPos, EntityId};
use rspigot_entities::EntityKind;

use crate::behavior::control::BehaviorCx;
use crate::behavior::memory::{MemoryModuleType, MemoryValue, WalkTarget};
use crate::behavior::tracker::PositionTracker;
use crate::piglin::{HoglinCondition, PiglinCondition};
use crate::random::RandomSource;
use crate::world::{AiWorld, Living};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UniformInt {
    pub min_inclusive: i32,
    pub max_inclusive: i32,
}

impl UniformInt {
    pub const fn of(min_inclusive: i32, max_inclusive: i32) -> Self {
        Self {
            min_inclusive,
            max_inclusive,
        }
    }

    pub fn sample(&self, random: &mut impl RandomSource) -> i32 {
        random.next_int_between(self.min_inclusive, self.max_inclusive)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntityFilter {
    Any,
    Kind(EntityKind),
    SeesNoPlayerHoldingLovedItem,
}

impl EntityFilter {
    pub fn matches(self, entity: &Living) -> bool {
        match self {
            Self::Any => true,
            Self::Kind(kind) => entity.kind == kind,
            Self::SeesNoPlayerHoldingLovedItem => !entity.facts.sees_player_holding_loved_item,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum MobPredicate {
    Always,
    IsAdult,
    IsBaby,
    HasMemory(MemoryModuleType),
    Not(Box<MobPredicate>),
    Hoglin(HoglinCondition),
    Piglin(PiglinCondition),
    CreakingActive,
    CreakingCanMove,
}

impl MobPredicate {
    pub fn negate(self) -> Self {
        Self::Not(Box::new(self))
    }

    pub fn test<W: AiWorld>(&self, cx: &BehaviorCx<'_, '_, W>) -> bool {
        match self {
            Self::Always => true,
            Self::IsAdult => !cx.ai.mob.baby,
            Self::IsBaby => cx.ai.mob.baby,
            Self::HasMemory(memory) => cx.brain.memories.has_value(*memory),
            Self::Not(inner) => !inner.test(cx),
            Self::Hoglin(condition) => condition.test(cx),
            Self::Piglin(condition) => condition.test(cx),
            Self::CreakingActive => cx.ai.brain.creaking.active,
            Self::CreakingCanMove => cx.ai.brain.creaking.can_move,
        }
    }
}

pub fn manhattan_distance(a: BlockPos, b: BlockPos) -> i32 {
    (a.x - b.x).abs() + (a.y - b.y).abs() + (a.z - b.z).abs()
}

pub fn dist_sqr(a: BlockPos, b: BlockPos) -> f64 {
    let (dx, dy, dz) = (
        f64::from(a.x - b.x),
        f64::from(a.y - b.y),
        f64::from(a.z - b.z),
    );
    dx * dx + dy * dy + dz * dz
}

pub fn look_at_entity<W: AiWorld>(cx: &mut BehaviorCx<'_, '_, W>, target: EntityId) {
    cx.brain.memories.set(
        MemoryModuleType::LookTarget,
        MemoryValue::Tracker(PositionTracker::of_entity(target, true)),
    );
}

pub fn set_walk_target<W: AiWorld>(cx: &mut BehaviorCx<'_, '_, W>, target: WalkTarget) {
    cx.brain.memories.set(
        MemoryModuleType::WalkTarget,
        MemoryValue::WalkTarget(target),
    );
}

pub fn get_walk_target<W: AiWorld>(cx: &BehaviorCx<'_, '_, W>) -> Option<WalkTarget> {
    cx.brain
        .memories
        .get(MemoryModuleType::WalkTarget)
        .and_then(MemoryValue::as_walk_target)
        .copied()
}

pub fn get_entity<W: AiWorld>(
    cx: &BehaviorCx<'_, '_, W>,
    ty: MemoryModuleType,
) -> Option<EntityId> {
    cx.brain.memories.get(ty).and_then(MemoryValue::as_entity)
}

pub fn is_within_melee_range<W: AiWorld>(cx: &BehaviorCx<'_, '_, W>, target: &Living) -> bool {
    crate::combat::within_melee_range(cx.ai.mob, target)
}

pub fn is_within_attack_range<W: AiWorld>(
    cx: &BehaviorCx<'_, '_, W>,
    target: &Living,
    projectile_attack_range_margin: i32,
) -> bool {
    let mob = cx.ai.mob;
    if let Some(range) = mob.main_hand.and_then(|item| mob.projectile_range(item)) {
        let reach = f64::from(range - projectile_attack_range_margin);
        return mob.distance_to_sqr(target.position) < reach * reach;
    }
    is_within_melee_range(cx, target)
}

pub fn is_holding_usable_non_melee_weapon<W: AiWorld>(cx: &BehaviorCx<'_, '_, W>) -> bool {
    let mob = cx.ai.mob;
    [mob.main_hand, mob.off_hand]
        .into_iter()
        .flatten()
        .any(|item| mob.can_use_non_melee_weapon(item))
}

pub fn nearest_target<W: AiWorld>(
    cx: &BehaviorCx<'_, '_, W>,
    current: Option<Living>,
    candidate: Living,
) -> Living {
    match current {
        Some(current) => {
            let mob = cx.ai.mob;
            if mob.distance_to_sqr(candidate.position) <= mob.distance_to_sqr(current.position) {
                candidate
            } else {
                current
            }
        }
        None => candidate,
    }
}

pub fn is_other_target_much_further_away_than_current_attack_target<W: AiWorld>(
    cx: &BehaviorCx<'_, '_, W>,
    other: &Living,
    how_much_further_away: f64,
) -> bool {
    let Some(current) =
        get_entity(cx, MemoryModuleType::AttackTarget).and_then(|id| cx.ai.world.living(id))
    else {
        return false;
    };
    let mob = cx.ai.mob;
    mob.distance_to_sqr(current.position) + how_much_further_away * how_much_further_away
        < mob.distance_to_sqr(other.position)
}

pub fn living_from_uuid_memory<W: AiWorld>(
    cx: &BehaviorCx<'_, '_, W>,
    memory: MemoryModuleType,
) -> Option<Living> {
    let uuid = cx
        .brain
        .memories
        .get(memory)
        .and_then(MemoryValue::as_uuid)?;
    cx.ai.world.living_by_uuid(uuid)
}
