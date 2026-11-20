use rspigot_blocks::BlockId;

use crate::{
    AttributeModifier, EquipmentSlot, Food, Item, ItemId, ItemSet, PotionContentsDefault, Rarity,
    Tool,
};

pub struct LingeringPotion;

impl Item for LingeringPotion {
    const NAME: &'static str = "minecraft:lingering_potion";
    const ID: ItemId = ItemId(1445);
    const CLASS: &'static str = "LingeringPotionItem";
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
    const USE_REMAINDER: Option<ItemId> = None;
    const ATTRIBUTE_MODIFIERS: &'static [AttributeModifier] = &[];
    const POTION_CONTENTS: Option<PotionContentsDefault> = Some(PotionContentsDefault {
        potion: None,
        custom_effects: &[],
    });
    type Behavior = crate::family::Plain;
}
