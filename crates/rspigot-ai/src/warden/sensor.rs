use rspigot_domain::EntityId;

use super::state::can_target_entity;
use crate::behavior::{Memories, MemoryModuleType, MemoryValue, NearestLivingEntitySensor, Sensor};
use crate::brain::AiContext;
use crate::world::{AiWorld, Living};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WardenEntitySensor;

fn closest<W: AiWorld>(
    cx: &AiContext<'_, W>,
    nearby: &[EntityId],
    wanted: impl Fn(&Living) -> bool,
) -> Option<EntityId> {
    nearby.iter().copied().find(|id| {
        cx.world
            .living(*id)
            .is_some_and(|entity| can_target_entity(cx.mob, cx.world, &entity) && wanted(&entity))
    })
}

impl Sensor for WardenEntitySensor {
    fn requires(&self) -> &'static [MemoryModuleType] {
        &[
            MemoryModuleType::NearestLivingEntities,
            MemoryModuleType::NearestVisibleLivingEntities,
            MemoryModuleType::NearestAttackable,
        ]
    }

    fn do_tick<W: AiWorld>(&mut self, cx: &mut AiContext<'_, W>, memories: &mut Memories) {
        NearestLivingEntitySensor.do_tick(cx, memories);
        let nearby: Vec<EntityId> = memories
            .get(MemoryModuleType::NearestLivingEntities)
            .and_then(MemoryValue::as_entities)
            .map(<[_]>::to_vec)
            .unwrap_or_default();
        let found = closest(cx, &nearby, |entity| entity.player)
            .or_else(|| closest(cx, &nearby, |entity| !entity.player));
        memories.set_option(
            MemoryModuleType::NearestAttackable,
            found.map(MemoryValue::Entity),
        );
    }
}
