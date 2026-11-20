use rspigot_blocks::BlockId;

use crate::{
    AttributeModifier, Consumable, ConsumeAnimation, ConsumeEffect, ConsumeMobEffect, EquipmentSlot,
    Food, Item, ItemId, ItemSet, Rarity, Tool,
};

pub struct EnchantedGoldenApple;

impl Item for EnchantedGoldenApple {
    const NAME: &'static str = "minecraft:enchanted_golden_apple";
    const ID: ItemId = ItemId(1101);
    const CLASS: &'static str = "Item";
    const MAX_STACK_SIZE: u8 = 64;
    const MAX_DAMAGE: Option<u32> = None;
    const WEAPON_DAMAGE: Option<u32> = None;
    const RARITY: Rarity = Rarity::Rare;
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
                    duration: 400,
                    show_icon: true,
                },
                ConsumeMobEffect {
                    effect: "minecraft:resistance",
                    amplifier: 0,
                    duration: 6000,
                    show_icon: true,
                },
                ConsumeMobEffect {
                    effect: "minecraft:fire_resistance",
                    amplifier: 0,
                    duration: 6000,
                    show_icon: true,
                },
                ConsumeMobEffect {
                    effect: "minecraft:absorption",
                    amplifier: 3,
                    duration: 2400,
                    show_icon: true,
                },
            ],
            probability: 1.0,
        }],
    });
    type Behavior = crate::family::Plain;
}
