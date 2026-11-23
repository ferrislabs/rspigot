use rspigot_domain::attribute::{self, enchantment_modifiers};
use rspigot_domain::effect::AttributeState;
use rspigot_domain::entity::LivingState;
use rspigot_items::EquipmentSlot;

const SLOTS: [EquipmentSlot; 7] = [
    EquipmentSlot::Mainhand,
    EquipmentSlot::Offhand,
    EquipmentSlot::Feet,
    EquipmentSlot::Legs,
    EquipmentSlot::Chest,
    EquipmentSlot::Head,
    EquipmentSlot::Body,
];

pub(crate) fn with_worn_enchantments(base: f64, name: &'static str, living: &LivingState) -> f64 {
    let modifiers = SLOTS
        .into_iter()
        .filter_map(|slot| living.equipment(slot).map(|stack| (slot, stack)))
        .flat_map(|(slot, stack)| enchantment_modifiers(slot, stack))
        .filter(|(attribute, _)| *attribute == name)
        .map(|(_, modifier)| modifier)
        .collect();
    attribute::value(&AttributeState {
        attribute: name,
        base,
        modifiers,
    })
}
