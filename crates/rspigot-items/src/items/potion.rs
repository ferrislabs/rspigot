use rspigot_blocks::BlockId;

use crate::{
    AttributeModifier, Consumable, ConsumeAnimation, EquipmentSlot, Food, Item, ItemId, ItemSet,
    PotionContentsDefault, Rarity, Tool,
};

pub struct Potion;

impl Item for Potion {
    const NAME: &'static str = "minecraft:potion";
    const ID: ItemId = ItemId(1271);
    const CLASS: &'static str = "PotionItem";
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
    const USE_REMAINDER: Option<ItemId> = Some(ItemId(1270));
    const ATTRIBUTE_MODIFIERS: &'static [AttributeModifier] = &[];
    const CONSUMABLE: Option<Consumable> = Some(Consumable {
        consume_seconds: 1.6,
        animation: ConsumeAnimation::Drink,
        sound: "minecraft:entity.generic.drink",
        has_consume_particles: false,
        on_consume_effects: &[],
    });
    const POTION_CONTENTS: Option<PotionContentsDefault> = Some(PotionContentsDefault {
        potion: None,
        custom_effects: &[],
    });
    type Behavior = crate::family::Plain;
}
