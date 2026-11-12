use rspigot_domain::{BlockPos, EntityId};
use rspigot_entities::EntityKind;

use crate::behavior::control::BehaviorCx;
use crate::behavior::memory::{MemoryModuleType, MemoryValue, WalkTarget};
use crate::behavior::tracker::PositionTracker;
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
}

impl EntityFilter {
    pub fn matches(self, entity: &Living) -> bool {
        match self {
            Self::Any => true,
            Self::Kind(kind) => entity.kind == kind,
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

pub fn is_within_attack_range<W: AiWorld>(cx: &BehaviorCx<'_, '_, W>, target: &Living) -> bool {
    is_within_melee_range(cx, target)
}
