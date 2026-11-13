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
        EntityKind::Sniffer => crate::sniffer::update_activity(brain),
        EntityKind::Breeze => breeze::update_activity(brain),
        EntityKind::Creaking => creaking::update_activity(brain, ai.brain.creaking.can_move),
        EntityKind::Warden => crate::warden::after_brain_tick(brain, ai),
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
            EntityKind::Sniffer => (
                &crate::sniffer::SNIFFER_MEMORIES,
                crate::sniffer::sniffer_sensors(),
                crate::sniffer::sniffer_activities(),
            ),
            EntityKind::Breeze => (&[], breeze::breeze_sensors(), breeze::breeze_activities()),
            EntityKind::Creaking => (
                &[],
                creaking::creaking_sensors(),
                creaking::creaking_activities(),
            ),
            EntityKind::CopperGolem => (
                &crate::copper_golem::COPPER_GOLEM_MEMORIES,
                crate::copper_golem::copper_golem_sensors(),
                crate::copper_golem::copper_golem_activities(),
            ),
            EntityKind::Warden => (
                &crate::warden::WARDEN_MEMORIES,
                crate::warden::warden_sensors(),
                crate::warden::warden_activities(),
            ),
            EntityKind::Villager => {
                return Some(crate::villager::villager_brain(
                    rspigot_domain::entity::species::villager::Profession::None,
                    false,
                    &PackedMemories::default(),
                    random,
                ));
            }
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
