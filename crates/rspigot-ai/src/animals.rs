use rspigot_entities::EntityKind;

use crate::behavior::{ActivityData, BehaviorBrain, MemoryModuleType, MobControl, PackedMemories};
use crate::brain::AiContext;
use crate::goat;
use crate::random::RandomSource;
use crate::world::AiWorld;

pub fn after_brain_tick<W: AiWorld>(
    brain: &mut BehaviorBrain<MobControl>,
    ai: &mut AiContext<'_, W>,
) {
    if ai.mob.kind == EntityKind::Goat {
        goat::update_activity(brain);
    }
}

pub fn brain_for(
    kind: EntityKind,
    random: &mut impl RandomSource,
) -> Option<BehaviorBrain<MobControl>> {
    let (memories, sensors, activities): (&[MemoryModuleType], _, Vec<ActivityData<MobControl>>) =
        match kind {
            EntityKind::Goat => (&[], goat::goat_sensors(), goat::goat_activities()),
            _ => return None,
        };
    Some(BehaviorBrain::new(
        memories,
        sensors,
        activities,
        &PackedMemories::default(),
        random,
    ))
}
