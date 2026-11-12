use rspigot_entities::EntityKind;

use crate::action::BrainMessage;
use crate::armadillo;
use crate::behavior::{
    ActivityData, BehaviorBrain, BehaviorCx, MemoryModuleType, MobControl, PackedMemories,
};
use crate::brain::AiContext;
use crate::breeze;
use crate::creaking;
use crate::goat;
use crate::random::RandomSource;
use crate::world::AiWorld;

pub fn after_brain_tick<W: AiWorld>(
    brain: &mut BehaviorBrain<MobControl>,
    ai: &mut AiContext<'_, W>,
) {
    match ai.mob.kind {
        EntityKind::Goat => goat::update_activity(brain),
        EntityKind::Armadillo => armadillo::update_activity(brain),
        EntityKind::Breeze => breeze::update_activity(brain),
        EntityKind::Creaking => creaking::update_activity(brain, ai.brain.creaking.can_move),
        _ => {}
    }
}

pub fn brain_for(
    kind: EntityKind,
    random: &mut impl RandomSource,
) -> Option<BehaviorBrain<MobControl>> {
    let (memories, sensors, activities): (&[MemoryModuleType], _, Vec<ActivityData<MobControl>>) =
        match kind {
            EntityKind::Goat => (&[], goat::goat_sensors(), goat::goat_activities()),
            EntityKind::Armadillo => (
                &[],
                armadillo::armadillo_sensors(),
                armadillo::armadillo_activities(),
            ),
            EntityKind::Breeze => (&[], breeze::breeze_sensors(), breeze::breeze_activities()),
            EntityKind::Creaking => (
                &[],
                creaking::creaking_sensors(),
                creaking::creaking_activities(),
            ),
            _ => return None,
        };
    let mut brain = BehaviorBrain::new(
        memories,
        sensors,
        activities,
        &PackedMemories::default(),
        random,
    );
    if kind == EntityKind::Breeze {
        breeze::finish_brain(&mut brain);
    }
    Some(brain)
}

pub fn deliver<W: AiWorld>(cx: &mut BehaviorCx<'_, '_, W>, message: BrainMessage) {
    armadillo::deliver(cx, message);
}
