use rspigot_blocks::BlockId;

use crate::{
    AttributeModifier, Consumable, ConsumeAnimation, ConsumeEffect, EquipmentSlot, Food, Item,
    ItemId, ItemSet, Rarity, Tool,
};

pub struct MilkBucket;

impl Item for MilkBucket {
    const NAME: &'static str = "minecraft:milk_bucket";
    const ID: ItemId = ItemId(1134);
    const CLASS: &'static str = "Item";
    const MAX_STACK_SIZE: u8 = 1;
    const MAX_DAMAGE: Option<u32> = None;
    const WEAPON_DAMAGE: Option<u32> = None;
    const RARITY: Rarity = Rarity::Common;
    const BLOCK: Option<BlockId> = None;
    const TOOL: Option<Tool> = None;
    const FOOD: Option<Food> = None;
    const EQUIPPABLE: Option<EquipmentSlot> = None;
    const ENCHANTABLE: Option<u32> = None;
    const REPAIRABLE: Option<ItemSet> = None;
    const DAMAGE_RESISTANT: Option<&'static str> = None;
    const USE_REMAINDER: Option<ItemId> = Some(ItemId(1128));
    const ATTRIBUTE_MODIFIERS: &'static [AttributeModifier] = &[];
    const CRAFTING_REMAINDER: Option<ItemId> = Some(ItemId(1128));
    const CONSUMABLE: Option<Consumable> = Some(Consumable {
        consume_seconds: 1.6,
        animation: ConsumeAnimation::Drink,
        sound: "minecraft:entity.generic.drink",
        has_consume_particles: false,
        on_consume_effects: &[ConsumeEffect::ClearAllEffects],
    });
    type Behavior = crate::family::Plain;
}
