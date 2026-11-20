use rspigot_blocks::BlockId;

use crate::{
    AttributeModifier, Consumable, ConsumeAnimation, CookingKind, Craft, EquipmentSlot, Food, Item,
    ItemId, ItemSet, Rarity, Tool,
};

pub struct CookedSalmon;

impl Item for CookedSalmon {
    const NAME: &'static str = "minecraft:cooked_salmon";
    const ID: ItemId = ItemId(1195);
    const CLASS: &'static str = "Item";
    const MAX_STACK_SIZE: u8 = 64;
    const MAX_DAMAGE: Option<u32> = None;
    const WEAPON_DAMAGE: Option<u32> = None;
    const RARITY: Rarity = Rarity::Common;
    const BLOCK: Option<BlockId> = None;
    const TOOL: Option<Tool> = None;
    const FOOD: Option<Food> = Some(Food {
        nutrition: 6,
        saturation: 9.6,
        can_always_eat: false,
    });
    const EQUIPPABLE: Option<EquipmentSlot> = None;
    const ENCHANTABLE: Option<u32> = None;
    const REPAIRABLE: Option<ItemSet> = None;
    const DAMAGE_RESISTANT: Option<&'static str> = None;
    const USE_REMAINDER: Option<ItemId> = None;
    const ATTRIBUTE_MODIFIERS: &'static [AttributeModifier] = &[];
    const CONSUMABLE: Option<Consumable> = Some(Consumable {
        consume_seconds: 1.6,
        animation: ConsumeAnimation::Eat,
        sound: "minecraft:entity.generic.eat",
        has_consume_particles: true,
        on_consume_effects: &[],
    });
    const CRAFTS: &'static [Craft] = &[
        Craft::Cooking {
            id: "minecraft:cooked_salmon",
            kind: CookingKind::Smelting,
            ingredient: ItemSet::Direct(&[ItemId(1191)]),
            experience: 0.35,
            cooking_time: 200,
        },
        Craft::Cooking {
            id: "minecraft:cooked_salmon_from_campfire_cooking",
            kind: CookingKind::Campfire,
            ingredient: ItemSet::Direct(&[ItemId(1191)]),
            experience: 0.35,
            cooking_time: 600,
        },
        Craft::Cooking {
            id: "minecraft:cooked_salmon_from_smoking",
            kind: CookingKind::Smoking,
            ingredient: ItemSet::Direct(&[ItemId(1191)]),
            experience: 0.35,
            cooking_time: 200,
        },
    ];
    type Behavior = crate::family::Plain;
}
