use rspigot_blocks::BlockId;

use crate::{
    AttributeModifier, Consumable, ConsumeAnimation, ConsumeEffect, EquipmentSlot, Food, Item,
    ItemId, ItemSet, Rarity, Tool,
};

pub struct OminousBottle;

impl Item for OminousBottle {
    const NAME: &'static str = "minecraft:ominous_bottle";
    const ID: ItemId = ItemId(1657);
    const CLASS: &'static str = "Item";
    const MAX_STACK_SIZE: u8 = 64;
    const MAX_DAMAGE: Option<u32> = None;
    const WEAPON_DAMAGE: Option<u32> = None;
    const RARITY: Rarity = Rarity::Uncommon;
    const BLOCK: Option<BlockId> = None;
    const TOOL: Option<Tool> = None;
    const FOOD: Option<Food> = None;
    const EQUIPPABLE: Option<EquipmentSlot> = None;
    const ENCHANTABLE: Option<u32> = None;
    const REPAIRABLE: Option<ItemSet> = None;
    const DAMAGE_RESISTANT: Option<&'static str> = None;
    const USE_REMAINDER: Option<ItemId> = None;
    const ATTRIBUTE_MODIFIERS: &'static [AttributeModifier] = &[];
    const CONSUMABLE: Option<Consumable> = Some(Consumable {
        consume_seconds: 1.6,
        animation: ConsumeAnimation::Drink,
        sound: "minecraft:entity.generic.drink",
        has_consume_particles: false,
        on_consume_effects: &[ConsumeEffect::PlaySound("minecraft:item.ominous_bottle.dispose")],
    });
    type Behavior = crate::family::Plain;
}
