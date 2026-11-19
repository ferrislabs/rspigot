use rspigot_blocks::BlockId;

use crate::{
    AttributeModifier, Craft, EquipmentSlot, Food, Item, ItemId, ItemSet, PotionContentsDefault,
    Rarity, Tool,
};

pub struct TippedArrow;

impl Item for TippedArrow {
    const NAME: &'static str = "minecraft:tipped_arrow";
    const ID: ItemId = ItemId(1444);
    const CLASS: &'static str = "TippedArrowItem";
    const MAX_STACK_SIZE: u8 = 64;
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
    const CRAFTS: &'static [Craft] = &[Craft::Imbue {
        id: "minecraft:tipped_arrow",
        source: ItemSet::Direct(&[ItemId(1445)]),
        material: ItemSet::Direct(&[ItemId(1009)]),
        count: 8,
    }];
    type Behavior = crate::family::Plain;
}
