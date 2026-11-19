use rspigot_blocks::BlockId;

use crate::{
    AttributeModifier, Consumable, ConsumeAnimation, Craft, EquipmentSlot, Food, Item, ItemId,
    ItemSet, Product, Rarity, Tool,
};

pub struct Bread;

impl Item for Bread {
    const NAME: &'static str = "minecraft:bread";
    const ID: ItemId = ItemId(1067);
    const CLASS: &'static str = "Item";
    const MAX_STACK_SIZE: u8 = 64;
    const MAX_DAMAGE: Option<u32> = None;
    const WEAPON_DAMAGE: Option<u32> = None;
    const RARITY: Rarity = Rarity::Common;
    const BLOCK: Option<BlockId> = None;
    const TOOL: Option<Tool> = None;
    const FOOD: Option<Food> = Some(Food {
        nutrition: 5,
        saturation: 6.0,
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
    const CRAFTS: &'static [Craft] = &[Craft::Shaped {
        id: "minecraft:bread",
        width: 3,
        height: 1,
        symmetrical: true,
        cells: &[
            Some(ItemSet::Direct(&[ItemId(1066)])),
            Some(ItemSet::Direct(&[ItemId(1066)])),
            Some(ItemSet::Direct(&[ItemId(1066)])),
        ],
        product: Product::of(1),
    }];
    type Behavior = crate::family::Plain;
}
