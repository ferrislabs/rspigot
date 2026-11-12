use rspigot_entities::EntityKind;

use crate::behavior::{
    BehaviorCx, MemoryModuleType, MemoryValue, is_entity_attackable,
    is_entity_attackable_ignoring_line_of_sight,
};
use crate::world::{AiWorld, Living};

const NEAR_ZOMBIFIED_DISTANCE: f64 = 6.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetherFinder {
    Hoglin,
    Zoglin,
    Piglin,
    PiglinBrute,
}

fn memory_entity<W: AiWorld>(
    cx: &BehaviorCx<'_, '_, W>,
    memory: MemoryModuleType,
) -> Option<Living> {
    cx.brain
        .memories
        .get(memory)
        .and_then(MemoryValue::as_entity)
        .and_then(|id| cx.ai.world.living(id))
}

pub fn is_near_zombified<W: AiWorld>(cx: &BehaviorCx<'_, '_, W>) -> bool {
    memory_entity(cx, MemoryModuleType::NearestVisibleZombified).is_some_and(|zombified| {
        cx.ai.mob.distance_to_sqr(zombified.position)
            < NEAR_ZOMBIFIED_DISTANCE * NEAR_ZOMBIFIED_DISTANCE
    })
}

fn angry_at<W: AiWorld>(cx: &BehaviorCx<'_, '_, W>) -> Option<Living> {
    crate::behavior::living_from_uuid_memory(cx, MemoryModuleType::AngryAt)
}

fn hoglin_target<W: AiWorld>(cx: &BehaviorCx<'_, '_, W>) -> Option<Living> {
    let memories = &cx.brain.memories;
    if memories.has_value(MemoryModuleType::Pacified)
        || memories.has_value(MemoryModuleType::BreedTarget)
    {
        return None;
    }
    memory_entity(cx, MemoryModuleType::NearestVisibleAttackablePlayer)
}

fn zoglin_target<W: AiWorld>(cx: &mut BehaviorCx<'_, '_, W>) -> Option<Living> {
    let nearby = cx
        .brain
        .memories
        .get(MemoryModuleType::NearestVisibleLivingEntities)
        .and_then(MemoryValue::as_visible)
        .map(|visible| visible.nearby_entities().to_vec())
        .unwrap_or_default();
    for id in nearby {
        let Some(living) = cx.ai.world.living(id) else {
            continue;
        };
        if living.kind == EntityKind::Zoglin || living.kind == EntityKind::Creeper {
            continue;
        }
        let visible = cx
            .brain
            .memories
            .get(MemoryModuleType::NearestVisibleLivingEntities)
            .and_then(MemoryValue::as_visible)
            .is_some_and(|visible| visible.contains_entity(cx.ai, &cx.brain.memories, id));
        if visible && is_entity_attackable(cx.ai, &cx.brain.memories, &living) {
            return Some(living);
        }
    }
    None
}

fn piglin_target<W: AiWorld>(cx: &mut BehaviorCx<'_, '_, W>) -> Option<Living> {
    if is_near_zombified(cx) {
        return None;
    }
    if let Some(angry) = angry_at(cx)
        && is_entity_attackable_ignoring_line_of_sight(cx.ai, &cx.brain.memories, &angry)
    {
        return Some(angry);
    }
    if cx
        .brain
        .memories
        .has_value(MemoryModuleType::UniversalAnger)
        && let Some(player) = memory_entity(cx, MemoryModuleType::NearestVisibleAttackablePlayer)
    {
        return Some(player);
    }
    if let Some(nemesis) = memory_entity(cx, MemoryModuleType::NearestVisibleNemesis) {
        return Some(nemesis);
    }
    let player = memory_entity(cx, MemoryModuleType::NearestTargetablePlayerNotWearingGold)?;
    is_entity_attackable(cx.ai, &cx.brain.memories, &player).then_some(player)
}

fn brute_target<W: AiWorld>(cx: &mut BehaviorCx<'_, '_, W>) -> Option<Living> {
    if let Some(angry) = angry_at(cx)
        && is_entity_attackable_ignoring_line_of_sight(cx.ai, &cx.brain.memories, &angry)
    {
        return Some(angry);
    }
    memory_entity(cx, MemoryModuleType::NearestVisibleAttackablePlayer)
        .or_else(|| memory_entity(cx, MemoryModuleType::NearestVisibleNemesis))
}

impl NetherFinder {
    pub fn find<W: AiWorld>(self, cx: &mut BehaviorCx<'_, '_, W>) -> Option<Living> {
        match self {
            Self::Hoglin => hoglin_target(cx),
            Self::Zoglin => zoglin_target(cx),
            Self::Piglin => piglin_target(cx),
            Self::PiglinBrute => brute_target(cx),
        }
    }

    pub fn is_nearest_valid_attack_target<W: AiWorld>(
        self,
        cx: &mut BehaviorCx<'_, '_, W>,
        target: &Living,
    ) -> bool {
        self.find(cx).is_some_and(|found| found.id == target.id)
    }
}
