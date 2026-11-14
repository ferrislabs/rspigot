mod ai;
mod behaviors;
mod brute;
mod crossbow;
mod facts;
mod hoglin;
mod sensors;
mod targets;
mod zoglin;

use rspigot_domain::ItemId;
use rspigot_entities::EntityKind;

use crate::action::BrainMessage;
use crate::behavior::{
    ActivityData, BehaviorBrain, BehaviorCx, Memories, MemoryModuleType, MobControl,
    PackedMemories, SensorKind,
};
use crate::brain::AiContext;
use crate::random::RandomSource;
use crate::world::{AiWorld, MobView};

pub use ai::{
    ADMIRE_DURATION, ATE_RECENTLY_TICKS, PiglinCondition, TIME_BETWEEN_HUNTS,
    can_replace_current_item, equipment_slot_for, is_barter_currency, is_food, is_loved_item,
};
pub use behaviors::{
    BackUpIfTooClose, BecomePassiveIfMemoryPresent, CopyIf, CopyMemoryWithExpiry,
    RememberIfHoglinWasKilled, StartAdmiringItemIfSeen, StartCelebratingIfTargetDead,
    StartHuntingHoglin, StopAdmiringIfItemTooFarAway, StopAdmiringIfTiredOfTryingToReachItem,
    StopHoldingItemIfNoLongerAdmiring,
};
pub use crossbow::{CROSSBOW_ATTACK_DURATION, CrossbowAttack};
pub use facts::{HoglinFacts, PIGLIN_INVENTORY_SIZE, PiglinFacts};
pub use hoglin::{HoglinCondition, is_pos_near_nearest_repellent};
pub use sensors::{HoglinSensor, PiglinBruteSensor, PiglinSensor, is_player_holding_loved_item};
pub use targets::NetherFinder;

pub fn deliver<W: AiWorld>(cx: &mut BehaviorCx<'_, '_, W>, message: BrainMessage) {
    hoglin::deliver(cx, message);
    zoglin::deliver(cx, message);
    ai::deliver(cx, message);
}

pub fn after_brain_tick<W: AiWorld>(
    brain: &mut BehaviorBrain<MobControl>,
    ai: &mut AiContext<'_, W>,
) {
    match ai.mob.kind {
        EntityKind::Hoglin => hoglin::update_activity(brain, ai),
        EntityKind::Zoglin => zoglin::update_activity(brain, ai),
        EntityKind::Piglin => ai::update_activity(brain, ai),
        EntityKind::PiglinBrute => brute::update_activity(brain, ai),
        _ => {}
    }
}

pub fn wants_to_pickup(mob: &MobView, memories: &Memories, item: ItemId) -> bool {
    match mob.kind {
        EntityKind::Piglin => ai::wants_to_pickup(mob, memories, item),
        EntityKind::PiglinBrute => item == rspigot_items::ItemKind::GoldenAxe.id(),
        _ => false,
    }
}

pub fn can_admire(mob: &MobView, memories: &Memories, held: ItemId) -> bool {
    ai::can_admire(mob, memories, held)
}

pub fn ambient_sound<W: AiWorld>(cx: &BehaviorCx<'_, '_, W>) -> Option<&'static str> {
    let activity = cx.brain.active_non_core_activity();
    match cx.ai.mob.kind {
        EntityKind::Hoglin => activity.map(|activity| hoglin::sound_for_activity(cx, activity)),
        EntityKind::Zoglin => Some(zoglin::ambient_sound(cx)),
        EntityKind::Piglin => activity.map(|activity| ai::sound_for_activity(cx, activity)),
        EntityKind::PiglinBrute => Some("minecraft:entity.piglin_brute.ambient"),
        _ => None,
    }
}

pub fn brain_for(
    kind: EntityKind,
    random: &mut impl RandomSource,
) -> Option<BehaviorBrain<MobControl>> {
    let (memories, sensors, activities): (
        &[MemoryModuleType],
        Vec<SensorKind>,
        Vec<ActivityData<MobControl>>,
    ) = match kind {
        EntityKind::Hoglin => (
            hoglin::hoglin_memories(),
            hoglin::hoglin_sensors(),
            hoglin::hoglin_activities(),
        ),
        EntityKind::Zoglin => (&[], zoglin::zoglin_sensors(), zoglin::zoglin_activities()),
        EntityKind::Piglin => (
            ai::piglin_memories(),
            ai::piglin_sensors(),
            ai::piglin_activities(),
        ),
        EntityKind::PiglinBrute => (
            brute::brute_memories(),
            brute::brute_sensors(),
            brute::brute_activities(),
        ),
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
