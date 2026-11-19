use rspigot_blocks::BlockId;

use crate::{
    AttributeModifier, Consumable, ConsumeAnimation, ConsumeEffect, Craft, EquipmentSlot, Food,
    Item, ItemId, ItemSet, Product, Rarity, Tool,
};

pub struct HoneyBottle;

impl Item for HoneyBottle {
    const NAME: &'static str = "minecraft:honey_bottle";
    const ID: ItemId = ItemId(1533);
    const CLASS: &'static str = "Item";
    const MAX_STACK_SIZE: u8 = 16;
    const MAX_DAMAGE: Option<u32> = None;
    const WEAPON_DAMAGE: Option<u32> = None;
    const RARITY: Rarity = Rarity::Common;
    const BLOCK: Option<BlockId> = None;
    const TOOL: Option<Tool> = None;
    const FOOD: Option<Food> = Some(Food {
        nutrition: 6,
        saturation: 1.2,
        can_always_eat: true,
    });
    const EQUIPPABLE: Option<EquipmentSlot> = None;
    const ENCHANTABLE: Option<u32> = None;
    const REPAIRABLE: Option<ItemSet> = None;
    const DAMAGE_RESISTANT: Option<&'static str> = None;
    const USE_REMAINDER: Option<ItemId> = Some(ItemId(1270));
    const ATTRIBUTE_MODIFIERS: &'static [AttributeModifier] = &[];
    const CRAFTING_REMAINDER: Option<ItemId> = Some(ItemId(1270));
    const CONSUMABLE: Option<Consumable> = Some(Consumable {
        consume_seconds: 2.0,
        animation: ConsumeAnimation::Drink,
        sound: "minecraft:item.honey_bottle.drink",
        has_consume_particles: false,
        on_consume_effects: &[ConsumeEffect::RemoveEffects(&["minecraft:poison"])],
    });
    const CRAFTS: &'static [Craft] = &[Craft::Shapeless {
        id: "minecraft:honey_bottle",
        ingredients: &[
            ItemSet::Direct(&[ItemId(832)]),
            ItemSet::Direct(&[ItemId(1270)]),
            ItemSet::Direct(&[ItemId(1270)]),
            ItemSet::Direct(&[ItemId(1270)]),
            ItemSet::Direct(&[ItemId(1270)]),
        ],
        product: Product::of(4),
    }];
    type Behavior = crate::family::Plain;
}
