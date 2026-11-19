use rspigot_blocks::BlockId;

use crate::{
    AttributeModifier, Consumable, ConsumeAnimation, Craft, EquipmentSlot, Food, Item, ItemId,
    ItemSet, Product, Rarity, Tool, tags,
};

pub struct MushroomStew;

impl Item for MushroomStew {
    const NAME: &'static str = "minecraft:mushroom_stew";
    const ID: ItemId = ItemId(1061);
    const CLASS: &'static str = "Item";
    const MAX_STACK_SIZE: u8 = 1;
    const MAX_DAMAGE: Option<u32> = None;
    const WEAPON_DAMAGE: Option<u32> = None;
    const RARITY: Rarity = Rarity::Common;
    const BLOCK: Option<BlockId> = None;
    const TOOL: Option<Tool> = None;
    const FOOD: Option<Food> = Some(Food {
        nutrition: 6,
        saturation: 7.2000003,
        can_always_eat: false,
    });
    const EQUIPPABLE: Option<EquipmentSlot> = None;
    const ENCHANTABLE: Option<u32> = None;
    const REPAIRABLE: Option<ItemSet> = None;
    const DAMAGE_RESISTANT: Option<&'static str> = None;
    const USE_REMAINDER: Option<ItemId> = Some(ItemId(1006));
    const ATTRIBUTE_MODIFIERS: &'static [AttributeModifier] = &[];
    const CONSUMABLE: Option<Consumable> = Some(Consumable {
        consume_seconds: 1.6,
        animation: ConsumeAnimation::Eat,
        sound: "minecraft:entity.generic.eat",
        has_consume_particles: true,
        on_consume_effects: &[],
    });
    const CRAFTS: &'static [Craft] = &[Craft::Shapeless {
        id: "minecraft:mushroom_stew",
        ingredients: &[
            ItemSet::Tag(&tags::ITEM_MUSHROOMS),
            ItemSet::Tag(&tags::ITEM_MUSHROOMS),
            ItemSet::Direct(&[ItemId(1006)]),
        ],
        product: Product::of(1),
    }];
    type Behavior = crate::family::Plain;
}
