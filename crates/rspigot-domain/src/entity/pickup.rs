use rspigot_entities::EntityKind;
use rspigot_items::{EquipmentSlot, ItemKind, ModifierOperation};

use super::living::{LivingState, slot_index};
use crate::attribute::{ARMOR, ARMOR_TOUGHNESS, ATTACK_DAMAGE};
use crate::item::ItemStack;
use rspigot_data::tags;

pub const REACH: (f64, f64, f64) = (1.0, 0.0, 1.0);
pub const GUARANTEED_DROP_CHANCE: f32 = 2.0;
pub const DROP_CURRENT_BIAS: f32 = 0.1;

const DAMAGE: &str = "minecraft:damage";
const SKELETON_PREFERRED: &str = "minecraft:skeleton_preferred_weapons";
const DROWNED_PREFERRED: &str = "minecraft:drowned_preferred_weapons";
const PILLAGER_PREFERRED: &str = "minecraft:pillager_preferred_weapons";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Plan {
    pub slot: EquipmentSlot,
    pub taken: u8,
}

pub fn handled_elsewhere(kind: EntityKind) -> bool {
    matches!(
        kind,
        EntityKind::Dolphin
            | EntityKind::Panda
            | EntityKind::Allay
            | EntityKind::Fox
            | EntityKind::Piglin
            | EntityKind::PiglinBrute
            | EntityKind::Villager
            | EntityKind::SulfurCube
    ) || kind.extends("AbstractIllager")
        || kind == EntityKind::Witch
        || kind == EntityKind::Ravager
}

pub fn slot_for(stack: &ItemStack) -> EquipmentSlot {
    stack
        .item
        .kind()
        .and_then(ItemKind::equippable)
        .unwrap_or(EquipmentSlot::Mainhand)
}

const fn is_armor(slot: EquipmentSlot) -> bool {
    matches!(
        slot,
        EquipmentSlot::Feet | EquipmentSlot::Legs | EquipmentSlot::Chest | EquipmentSlot::Head
    )
}

const fn limit(slot: EquipmentSlot) -> Option<u8> {
    match slot {
        EquipmentSlot::Mainhand | EquipmentSlot::Offhand => None,
        _ => Some(1),
    }
}

fn preferred_weapons(kind: EntityKind) -> Option<&'static str> {
    if kind.extends("AbstractSkeleton") {
        Some(SKELETON_PREFERRED)
    } else if kind == EntityKind::Drowned {
        Some(DROWNED_PREFERRED)
    } else if kind == EntityKind::Pillager {
        Some(PILLAGER_PREFERRED)
    } else {
        None
    }
}

fn in_item_tag(stack: &ItemStack, tag: &str) -> bool {
    tags::tag("minecraft:item", tag)
        .is_some_and(|ids| ids.binary_search(&u32::from(stack.item.0)).is_ok())
}

fn base_value(kind: EntityKind, living: &LivingState, name: &str) -> f64 {
    living
        .attributes
        .instances()
        .iter()
        .find(|instance| instance.attribute == name)
        .map(|instance| instance.base)
        .or_else(|| {
            kind.default_attributes()
                .iter()
                .find(|(candidate, _)| *candidate == name)
                .map(|(_, base)| *base)
        })
        .unwrap_or(0.0)
}

fn approximate(
    kind: EntityKind,
    living: &LivingState,
    stack: &ItemStack,
    attribute: &str,
    slot: EquipmentSlot,
) -> f64 {
    let base = base_value(kind, living, attribute);
    let Some(item) = stack.item.kind() else {
        return base;
    };
    let mut value = base;
    let mut multiplied_base = 0.0;
    let mut multiplied_total = Vec::new();
    for modifier in item
        .attribute_modifiers()
        .iter()
        .filter(|m| m.attribute == attribute && m.slot.contains(slot))
    {
        match modifier.operation {
            ModifierOperation::AddValue => value += modifier.amount,
            ModifierOperation::AddMultipliedBase => multiplied_base += modifier.amount,
            ModifierOperation::AddMultipliedTotal => multiplied_total.push(modifier.amount),
        }
    }
    value += base * multiplied_base;
    for amount in multiplied_total {
        value *= 1.0 + amount;
    }
    value
}

fn damage_of(stack: &ItemStack) -> i64 {
    stack.components.get_i64(DAMAGE).unwrap_or(0)
}

fn has_extra_components(stack: &ItemStack) -> bool {
    stack.components.iter().any(|(key, _)| key != DAMAGE)
}

fn can_replace_equal(new: &ItemStack, current: &ItemStack) -> bool {
    damage_of(new) < damage_of(current)
        || (has_extra_components(new) && !has_extra_components(current))
}

fn can_replace(
    kind: EntityKind,
    living: &LivingState,
    new: &ItemStack,
    current: &ItemStack,
    slot: EquipmentSlot,
) -> bool {
    if is_armor(slot) {
        if crate::enchant::prevents_armor_change(current) {
            return false;
        }
        let defense = |stack| approximate(kind, living, stack, ARMOR, slot);
        let toughness = |stack| approximate(kind, living, stack, ARMOR_TOUGHNESS, slot);
        if defense(new) != defense(current) {
            return defense(new) > defense(current);
        }
        if toughness(new) != toughness(current) {
            return toughness(new) > toughness(current);
        }
        return can_replace_equal(new, current);
    }
    if let Some(tag) = preferred_weapons(kind) {
        let (new_preferred, current_preferred) = (in_item_tag(new, tag), in_item_tag(current, tag));
        if current_preferred && !new_preferred {
            return false;
        }
        if !current_preferred && new_preferred {
            return true;
        }
    }
    let damage = |stack| approximate(kind, living, stack, ATTACK_DAMAGE, slot);
    if damage(new) != damage(current) {
        return damage(new) > damage(current);
    }
    can_replace_equal(new, current)
}

pub fn plan(kind: EntityKind, living: &LivingState, stack: &ItemStack) -> Option<Plan> {
    let mut slot = slot_for(stack);
    if matches!(slot, EquipmentSlot::Body | EquipmentSlot::Saddle) {
        return None;
    }
    let mut replaces = match living.equipment[slot_index(slot)].as_ref() {
        None => true,
        Some(current) => can_replace(kind, living, stack, current, slot),
    };
    if is_armor(slot) && !replaces {
        slot = EquipmentSlot::Mainhand;
        replaces = living.equipment[slot_index(slot)].is_none();
    }
    if !replaces {
        return None;
    }
    let taken = limit(slot).map_or(stack.count.get(), |cap| cap.min(stack.count.get()));
    Some(Plan { slot, taken })
}

#[cfg(test)]
mod tests {
    const ENCHANTMENTS: &str = "minecraft:enchantments";
    const BINDING_CURSE: &str = "minecraft:binding_curse";
    use std::num::NonZeroU8;

    use rspigot_nbt::Compound;
    use uuid::Uuid;

    use super::*;
    use crate::EntityId;
    use crate::entity::EntityBase;

    fn zombie() -> LivingState {
        let mut base = EntityBase::new(EntityId(1), Uuid::from_u128(1), EntityKind::Zombie);
        LivingState::new(&mut base, 0.0)
    }

    fn stack(kind: ItemKind) -> ItemStack {
        ItemStack::new(kind.id(), NonZeroU8::MIN)
    }

    #[test]
    fn an_empty_handed_zombie_wants_a_sword() {
        let living = zombie();
        let plan = plan(EntityKind::Zombie, &living, &stack(ItemKind::IronSword));
        assert_eq!(
            plan,
            Some(Plan {
                slot: EquipmentSlot::Mainhand,
                taken: 1
            })
        );
    }

    #[test]
    fn armor_goes_to_its_slot_one_piece_at_a_time() {
        let living = zombie();
        let mut boots = stack(ItemKind::IronBoots);
        boots.count = NonZeroU8::new(3).unwrap();
        let plan = plan(EntityKind::Zombie, &living, &boots).expect("plan");
        assert_eq!(plan.slot, EquipmentSlot::Feet);
        assert_eq!(plan.taken, 1);
    }

    #[test]
    fn a_better_helmet_replaces_a_worse_one_and_not_the_other_way() {
        let mut living = zombie();
        living.equipment[slot_index(EquipmentSlot::Head)] = Some(stack(ItemKind::LeatherHelmet));
        let better = plan(EntityKind::Zombie, &living, &stack(ItemKind::IronHelmet));
        assert_eq!(better.map(|p| p.slot), Some(EquipmentSlot::Head));
        living.equipment[slot_index(EquipmentSlot::Head)] = Some(stack(ItemKind::DiamondHelmet));
        let worse = plan(EntityKind::Zombie, &living, &stack(ItemKind::LeatherHelmet));
        assert_eq!(worse.map(|p| p.slot), Some(EquipmentSlot::Mainhand));
    }

    #[test]
    fn a_rejected_helmet_is_held_in_an_empty_hand() {
        let mut living = zombie();
        living.equipment[slot_index(EquipmentSlot::Head)] = Some(stack(ItemKind::DiamondHelmet));
        let held = plan(EntityKind::Zombie, &living, &stack(ItemKind::LeatherHelmet));
        assert_eq!(held.map(|p| p.slot), Some(EquipmentSlot::Mainhand));
        living.equipment[slot_index(EquipmentSlot::Mainhand)] = Some(stack(ItemKind::Stick));
        assert_eq!(
            plan(EntityKind::Zombie, &living, &stack(ItemKind::LeatherHelmet)),
            None
        );
    }

    #[test]
    fn a_stronger_weapon_replaces_a_weaker_one() {
        let mut living = zombie();
        living.equipment[slot_index(EquipmentSlot::Mainhand)] = Some(stack(ItemKind::WoodenSword));
        assert!(plan(EntityKind::Zombie, &living, &stack(ItemKind::DiamondSword)).is_some());
        living.equipment[slot_index(EquipmentSlot::Mainhand)] = Some(stack(ItemKind::DiamondSword));
        assert!(plan(EntityKind::Zombie, &living, &stack(ItemKind::WoodenSword)).is_none());
    }

    #[test]
    fn a_skeleton_keeps_its_bow_over_a_sword() {
        let mut base = EntityBase::new(EntityId(2), Uuid::from_u128(2), EntityKind::Skeleton);
        let mut living = LivingState::new(&mut base, 0.0);
        living.equipment[slot_index(EquipmentSlot::Mainhand)] = Some(stack(ItemKind::Bow));
        assert!(
            plan(
                EntityKind::Skeleton,
                &living,
                &stack(ItemKind::DiamondSword)
            )
            .is_none()
        );
    }

    #[test]
    fn a_binding_curse_keeps_the_worn_piece() {
        let mut living = zombie();
        let mut cursed = stack(ItemKind::LeatherHelmet);
        cursed
            .components
            .insert(ENCHANTMENTS, Compound::new().with(BINDING_CURSE, 1));
        living.equipment[slot_index(EquipmentSlot::Head)] = Some(cursed);
        living.equipment[slot_index(EquipmentSlot::Mainhand)] = Some(stack(ItemKind::Stick));
        assert!(plan(EntityKind::Zombie, &living, &stack(ItemKind::DiamondHelmet)).is_none());
    }

    #[test]
    fn horse_armor_slots_are_not_used_by_ordinary_mobs() {
        let living = zombie();
        assert!(plan(EntityKind::Zombie, &living, &stack(ItemKind::Saddle)).is_none());
    }
}
