use rspigot_blocks::BlockId;

use crate::{
    AttributeModifier, Consumable, ConsumeAnimation, ConsumeEffect, ConsumeMobEffect, Craft,
    EquipmentSlot, Food, Item, ItemId, ItemSet, Product, Rarity, Tool,
};

pub struct GoldenApple;

impl Item for GoldenApple {
    const NAME: &'static str = "minecraft:golden_apple";
    const ID: ItemId = ItemId(1100);
    const CLASS: &'static str = "Item";
    const MAX_STACK_SIZE: u8 = 64;
    const MAX_DAMAGE: Option<u32> = None;
    const WEAPON_DAMAGE: Option<u32> = None;
    const RARITY: Rarity = Rarity::Common;
    const BLOCK: Option<BlockId> = None;
    const TOOL: Option<Tool> = None;
    const FOOD: Option<Food> = Some(Food {
        nutrition: 4,
        saturation: 9.6,
        can_always_eat: true,
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
        on_consume_effects: &[ConsumeEffect::ApplyEffects {
            effects: &[
                ConsumeMobEffect {
                    effect: "minecraft:regeneration",
                    amplifier: 1,
                    duration: 100,
                    show_icon: true,
                },
                ConsumeMobEffect {
                    effect: "minecraft:absorption",
                    amplifier: 0,
                    duration: 2400,
                    show_icon: true,
                },
            ],
            probability: 1.0,
        }],
    });
    const CRAFTS: &'static [Craft] = &[Craft::Shaped {
        id: "minecraft:golden_apple",
        width: 3,
        height: 3,
        symmetrical: true,
        cells: &[
            Some(ItemSet::Direct(&[ItemId(1022)])),
            Some(ItemSet::Direct(&[ItemId(1022)])),
            Some(ItemSet::Direct(&[ItemId(1022)])),
            Some(ItemSet::Direct(&[ItemId(1022)])),
            Some(ItemSet::Direct(&[ItemId(1007)])),
            Some(ItemSet::Direct(&[ItemId(1022)])),
            Some(ItemSet::Direct(&[ItemId(1022)])),
            Some(ItemSet::Direct(&[ItemId(1022)])),
            Some(ItemSet::Direct(&[ItemId(1022)])),
        ],
        product: Product::of(1),
    }];
    type Behavior = crate::family::Plain;
}
