use rspigot_blocks::BlockId;

use crate::{
    AttributeModifier, Consumable, ConsumeAnimation, Craft, EquipmentSlot, Food, Item, ItemId,
    ItemSet, Product, ProductComponent, Rarity, StewEffect, Tool, tags,
};

pub struct SuspiciousStew;

impl Item for SuspiciousStew {
    const NAME: &'static str = "minecraft:suspicious_stew";
    const ID: ItemId = ItemId(1492);
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
        can_always_eat: true,
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
    const SUSPICIOUS_STEW_EFFECTS: Option<&'static [StewEffect]> = Some(&[]);
    const CRAFTS: &'static [Craft] = &[
        Craft::Shapeless {
            id: "minecraft:suspicious_stew_from_allium",
            ingredients: &[
                ItemSet::Direct(&[ItemId(1006)]),
                ItemSet::Tag(&tags::ITEM_MUSHROOMS),
                ItemSet::Tag(&tags::ITEM_MUSHROOMS),
                ItemSet::Direct(&[ItemId(304)]),
            ],
            product: Product {
                count: 1,
                components: &[ProductComponent::SuspiciousStewEffects(&[StewEffect {
                    effect: "minecraft:fire_resistance",
                    duration: 60,
                }])],
            },
        },
        Craft::Shapeless {
            id: "minecraft:suspicious_stew_from_azure_bluet",
            ingredients: &[
                ItemSet::Direct(&[ItemId(1006)]),
                ItemSet::Tag(&tags::ITEM_MUSHROOMS),
                ItemSet::Tag(&tags::ITEM_MUSHROOMS),
                ItemSet::Direct(&[ItemId(305)]),
            ],
            product: Product {
                count: 1,
                components: &[ProductComponent::SuspiciousStewEffects(&[StewEffect {
                    effect: "minecraft:blindness",
                    duration: 220,
                }])],
            },
        },
        Craft::Shapeless {
            id: "minecraft:suspicious_stew_from_blue_orchid",
            ingredients: &[
                ItemSet::Direct(&[ItemId(1006)]),
                ItemSet::Tag(&tags::ITEM_MUSHROOMS),
                ItemSet::Tag(&tags::ITEM_MUSHROOMS),
                ItemSet::Direct(&[ItemId(303)]),
            ],
            product: Product {
                count: 1,
                components: &[ProductComponent::SuspiciousStewEffects(&[StewEffect {
                    effect: "minecraft:saturation",
                    duration: 7,
                }])],
            },
        },
        Craft::Shapeless {
            id: "minecraft:suspicious_stew_from_closed_eyeblossom",
            ingredients: &[
                ItemSet::Direct(&[ItemId(1006)]),
                ItemSet::Tag(&tags::ITEM_MUSHROOMS),
                ItemSet::Tag(&tags::ITEM_MUSHROOMS),
                ItemSet::Direct(&[ItemId(301)]),
            ],
            product: Product {
                count: 1,
                components: &[ProductComponent::SuspiciousStewEffects(&[StewEffect {
                    effect: "minecraft:nausea",
                    duration: 140,
                }])],
            },
        },
        Craft::Shapeless {
            id: "minecraft:suspicious_stew_from_cornflower",
            ingredients: &[
                ItemSet::Direct(&[ItemId(1006)]),
                ItemSet::Tag(&tags::ITEM_MUSHROOMS),
                ItemSet::Tag(&tags::ITEM_MUSHROOMS),
                ItemSet::Direct(&[ItemId(311)]),
            ],
            product: Product {
                count: 1,
                components: &[ProductComponent::SuspiciousStewEffects(&[StewEffect {
                    effect: "minecraft:jump_boost",
                    duration: 100,
                }])],
            },
        },
        Craft::Shapeless {
            id: "minecraft:suspicious_stew_from_dandelion",
            ingredients: &[
                ItemSet::Direct(&[ItemId(1006)]),
                ItemSet::Tag(&tags::ITEM_MUSHROOMS),
                ItemSet::Tag(&tags::ITEM_MUSHROOMS),
                ItemSet::Direct(&[ItemId(298)]),
            ],
            product: Product {
                count: 1,
                components: &[ProductComponent::SuspiciousStewEffects(&[StewEffect {
                    effect: "minecraft:saturation",
                    duration: 7,
                }])],
            },
        },
        Craft::Shapeless {
            id: "minecraft:suspicious_stew_from_golden_dandelion",
            ingredients: &[
                ItemSet::Direct(&[ItemId(1006)]),
                ItemSet::Tag(&tags::ITEM_MUSHROOMS),
                ItemSet::Tag(&tags::ITEM_MUSHROOMS),
                ItemSet::Direct(&[ItemId(299)]),
            ],
            product: Product {
                count: 1,
                components: &[ProductComponent::SuspiciousStewEffects(&[StewEffect {
                    effect: "minecraft:saturation",
                    duration: 7,
                }])],
            },
        },
        Craft::Shapeless {
            id: "minecraft:suspicious_stew_from_lily_of_the_valley",
            ingredients: &[
                ItemSet::Direct(&[ItemId(1006)]),
                ItemSet::Tag(&tags::ITEM_MUSHROOMS),
                ItemSet::Tag(&tags::ITEM_MUSHROOMS),
                ItemSet::Direct(&[ItemId(312)]),
            ],
            product: Product {
                count: 1,
                components: &[ProductComponent::SuspiciousStewEffects(&[StewEffect {
                    effect: "minecraft:poison",
                    duration: 220,
                }])],
            },
        },
        Craft::Shapeless {
            id: "minecraft:suspicious_stew_from_open_eyeblossom",
            ingredients: &[
                ItemSet::Direct(&[ItemId(1006)]),
                ItemSet::Tag(&tags::ITEM_MUSHROOMS),
                ItemSet::Tag(&tags::ITEM_MUSHROOMS),
                ItemSet::Direct(&[ItemId(300)]),
            ],
            product: Product {
                count: 1,
                components: &[ProductComponent::SuspiciousStewEffects(&[StewEffect {
                    effect: "minecraft:blindness",
                    duration: 220,
                }])],
            },
        },
        Craft::Shapeless {
            id: "minecraft:suspicious_stew_from_orange_tulip",
            ingredients: &[
                ItemSet::Direct(&[ItemId(1006)]),
                ItemSet::Tag(&tags::ITEM_MUSHROOMS),
                ItemSet::Tag(&tags::ITEM_MUSHROOMS),
                ItemSet::Direct(&[ItemId(307)]),
            ],
            product: Product {
                count: 1,
                components: &[ProductComponent::SuspiciousStewEffects(&[StewEffect {
                    effect: "minecraft:weakness",
                    duration: 140,
                }])],
            },
        },
        Craft::Shapeless {
            id: "minecraft:suspicious_stew_from_oxeye_daisy",
            ingredients: &[
                ItemSet::Direct(&[ItemId(1006)]),
                ItemSet::Tag(&tags::ITEM_MUSHROOMS),
                ItemSet::Tag(&tags::ITEM_MUSHROOMS),
                ItemSet::Direct(&[ItemId(310)]),
            ],
            product: Product {
                count: 1,
                components: &[ProductComponent::SuspiciousStewEffects(&[StewEffect {
                    effect: "minecraft:regeneration",
                    duration: 140,
                }])],
            },
        },
        Craft::Shapeless {
            id: "minecraft:suspicious_stew_from_pink_tulip",
            ingredients: &[
                ItemSet::Direct(&[ItemId(1006)]),
                ItemSet::Tag(&tags::ITEM_MUSHROOMS),
                ItemSet::Tag(&tags::ITEM_MUSHROOMS),
                ItemSet::Direct(&[ItemId(309)]),
            ],
            product: Product {
                count: 1,
                components: &[ProductComponent::SuspiciousStewEffects(&[StewEffect {
                    effect: "minecraft:weakness",
                    duration: 140,
                }])],
            },
        },
        Craft::Shapeless {
            id: "minecraft:suspicious_stew_from_poppy",
            ingredients: &[
                ItemSet::Direct(&[ItemId(1006)]),
                ItemSet::Tag(&tags::ITEM_MUSHROOMS),
                ItemSet::Tag(&tags::ITEM_MUSHROOMS),
                ItemSet::Direct(&[ItemId(302)]),
            ],
            product: Product {
                count: 1,
                components: &[ProductComponent::SuspiciousStewEffects(&[StewEffect {
                    effect: "minecraft:night_vision",
                    duration: 100,
                }])],
            },
        },
        Craft::Shapeless {
            id: "minecraft:suspicious_stew_from_red_tulip",
            ingredients: &[
                ItemSet::Direct(&[ItemId(1006)]),
                ItemSet::Tag(&tags::ITEM_MUSHROOMS),
                ItemSet::Tag(&tags::ITEM_MUSHROOMS),
                ItemSet::Direct(&[ItemId(306)]),
            ],
            product: Product {
                count: 1,
                components: &[ProductComponent::SuspiciousStewEffects(&[StewEffect {
                    effect: "minecraft:weakness",
                    duration: 140,
                }])],
            },
        },
        Craft::Shapeless {
            id: "minecraft:suspicious_stew_from_torchflower",
            ingredients: &[
                ItemSet::Direct(&[ItemId(1006)]),
                ItemSet::Tag(&tags::ITEM_MUSHROOMS),
                ItemSet::Tag(&tags::ITEM_MUSHROOMS),
                ItemSet::Direct(&[ItemId(314)]),
            ],
            product: Product {
                count: 1,
                components: &[ProductComponent::SuspiciousStewEffects(&[StewEffect {
                    effect: "minecraft:night_vision",
                    duration: 100,
                }])],
            },
        },
        Craft::Shapeless {
            id: "minecraft:suspicious_stew_from_white_tulip",
            ingredients: &[
                ItemSet::Direct(&[ItemId(1006)]),
                ItemSet::Tag(&tags::ITEM_MUSHROOMS),
                ItemSet::Tag(&tags::ITEM_MUSHROOMS),
                ItemSet::Direct(&[ItemId(308)]),
            ],
            product: Product {
                count: 1,
                components: &[ProductComponent::SuspiciousStewEffects(&[StewEffect {
                    effect: "minecraft:weakness",
                    duration: 140,
                }])],
            },
        },
        Craft::Shapeless {
            id: "minecraft:suspicious_stew_from_wither_rose",
            ingredients: &[
                ItemSet::Direct(&[ItemId(1006)]),
                ItemSet::Tag(&tags::ITEM_MUSHROOMS),
                ItemSet::Tag(&tags::ITEM_MUSHROOMS),
                ItemSet::Direct(&[ItemId(313)]),
            ],
            product: Product {
                count: 1,
                components: &[ProductComponent::SuspiciousStewEffects(&[StewEffect {
                    effect: "minecraft:wither",
                    duration: 140,
                }])],
            },
        },
    ];
    type Behavior = crate::family::Plain;
}
