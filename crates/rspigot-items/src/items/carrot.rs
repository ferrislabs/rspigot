use rspigot_blocks::BlockId;

use crate::{
    AttributeModifier, Consumable, ConsumeAnimation, EquipmentSlot, Food, Item, ItemId, ItemSet,
    Rarity, Tool,
};

pub struct Carrot;

impl Item for Carrot {
    const NAME: &'static str = "minecraft:carrot";
    const ID: ItemId = ItemId(1378);
    const CLASS: &'static str = "BlockItem";
    const MAX_STACK_SIZE: u8 = 64;
    const MAX_DAMAGE: Option<u32> = None;
    const WEAPON_DAMAGE: Option<u32> = None;
    const RARITY: Rarity = Rarity::Common;
    const BLOCK: Option<BlockId> = Some(BlockId(493));
    const TOOL: Option<Tool> = None;
    const FOOD: Option<Food> = Some(Food {
        nutrition: 3,
        saturation: 3.6000001,
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
    type Behavior = crate::family::BlockItem;
}
