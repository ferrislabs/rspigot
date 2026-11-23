mod defaults;
pub mod family;
#[rustfmt::skip]
mod items;
#[rustfmt::skip]
mod kind;
mod lookup;
#[rustfmt::skip]
mod tags;

pub use family::ItemBehavior;
pub use items::*;
pub use kind::{DEFAULT_MAX_STACK_SIZE, ItemKind};
use rspigot_blocks::BlockId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ItemId(pub u16);

impl ItemId {
    pub const AIR: Self = Self(0);

    pub fn kind(self) -> Option<ItemKind> {
        ItemKind::from_id(self)
    }

    pub fn max_stack_size(self) -> u8 {
        self.kind()
            .map_or(DEFAULT_MAX_STACK_SIZE, ItemKind::max_stack_size)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Rarity {
    Common,
    Uncommon,
    Rare,
    Epic,
}

impl Rarity {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Common => "common",
            Self::Uncommon => "uncommon",
            Self::Rare => "rare",
            Self::Epic => "epic",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum EquipmentSlot {
    Mainhand,
    Offhand,
    Feet,
    Legs,
    Chest,
    Head,
    Body,
    Saddle,
}

impl EquipmentSlot {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Mainhand => "mainhand",
            Self::Offhand => "offhand",
            Self::Feet => "feet",
            Self::Legs => "legs",
            Self::Chest => "chest",
            Self::Head => "head",
            Self::Body => "body",
            Self::Saddle => "saddle",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ModifierOperation {
    AddValue,
    AddMultipliedBase,
    AddMultipliedTotal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SlotGroup {
    Any,
    Mainhand,
    Offhand,
    Hand,
    Feet,
    Legs,
    Chest,
    Head,
    Armor,
    Body,
    Saddle,
}

impl SlotGroup {
    pub const fn contains(self, slot: EquipmentSlot) -> bool {
        match self {
            Self::Any => true,
            Self::Mainhand => matches!(slot, EquipmentSlot::Mainhand),
            Self::Offhand => matches!(slot, EquipmentSlot::Offhand),
            Self::Hand => matches!(slot, EquipmentSlot::Mainhand | EquipmentSlot::Offhand),
            Self::Feet => matches!(slot, EquipmentSlot::Feet),
            Self::Legs => matches!(slot, EquipmentSlot::Legs),
            Self::Chest => matches!(slot, EquipmentSlot::Chest),
            Self::Head => matches!(slot, EquipmentSlot::Head),
            Self::Armor => matches!(
                slot,
                EquipmentSlot::Feet
                    | EquipmentSlot::Legs
                    | EquipmentSlot::Chest
                    | EquipmentSlot::Head
                    | EquipmentSlot::Body
            ),
            Self::Body => matches!(slot, EquipmentSlot::Body),
            Self::Saddle => matches!(slot, EquipmentSlot::Saddle),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AttributeModifier {
    pub attribute: &'static str,
    pub id: &'static str,
    pub amount: f64,
    pub operation: ModifierOperation,
    pub slot: SlotGroup,
}

#[derive(Debug, PartialEq, Eq)]
pub struct BlockTag {
    pub name: &'static str,
    pub blocks: &'static [BlockId],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockSet {
    Tag(&'static BlockTag),
    Direct(&'static [BlockId]),
}

impl BlockSet {
    pub const fn blocks(self) -> &'static [BlockId] {
        match self {
            Self::Tag(tag) => tag.blocks,
            Self::Direct(blocks) => blocks,
        }
    }

    pub const fn tag(self) -> Option<&'static str> {
        match self {
            Self::Tag(tag) => Some(tag.name),
            Self::Direct(_) => None,
        }
    }

    pub fn contains(self, block: BlockId) -> bool {
        self.blocks().binary_search(&block).is_ok()
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct ItemTag {
    pub name: &'static str,
    pub items: &'static [ItemId],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemSet {
    Tag(&'static ItemTag),
    Direct(&'static [ItemId]),
}

impl ItemSet {
    pub const fn items(self) -> &'static [ItemId] {
        match self {
            Self::Tag(tag) => tag.items,
            Self::Direct(items) => items,
        }
    }

    pub const fn tag(self) -> Option<&'static str> {
        match self {
            Self::Tag(tag) => Some(tag.name),
            Self::Direct(_) => None,
        }
    }

    pub fn contains(self, item: ItemId) -> bool {
        self.items().binary_search(&item).is_ok()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StewEffect {
    pub effect: &'static str,
    pub duration: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProductComponent {
    SuspiciousStewEffects(&'static [StewEffect]),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Product {
    pub count: u8,
    pub components: &'static [ProductComponent],
}

impl Product {
    pub const fn of(count: u8) -> Self {
        Self {
            count,
            components: &[],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FireworkShapes {
    pub burst: ItemSet,
    pub creeper: ItemSet,
    pub large_ball: ItemSet,
    pub star: ItemSet,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fuel {
    pub burn_time: u16,
    pub fast_burn_time: u16,
}

impl Fuel {
    pub const fn burn_time(self, fast: bool) -> u16 {
        if fast {
            self.fast_burn_time
        } else {
            self.burn_time
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CookingKind {
    Smelting,
    Blasting,
    Smoking,
    Campfire,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Craft {
    Shaped {
        id: &'static str,
        width: u8,
        height: u8,
        symmetrical: bool,
        cells: &'static [Option<ItemSet>],
        product: Product,
    },
    Shapeless {
        id: &'static str,
        ingredients: &'static [ItemSet],
        product: Product,
    },
    Transmute {
        id: &'static str,
        input: ItemSet,
        material: ItemSet,
        min_material: u8,
        max_material: u8,
        add_material_count: bool,
        count: u8,
    },
    Dye {
        id: &'static str,
        dye: ItemSet,
    },
    Imbue {
        id: &'static str,
        source: ItemSet,
        material: ItemSet,
        count: u8,
    },
    DecoratedPot {
        id: &'static str,
        back: ItemSet,
        front: ItemSet,
        left: ItemSet,
        right: ItemSet,
    },
    BannerDuplicate {
        id: &'static str,
    },
    RepairItem {
        id: &'static str,
    },
    ShieldDecoration {
        id: &'static str,
        banner: ItemSet,
    },
    MapExtending {
        id: &'static str,
        map: ItemSet,
        material: ItemSet,
    },
    FireworkStarFade {
        id: &'static str,
        dye: ItemSet,
    },
    FireworkStar {
        id: &'static str,
        dye: ItemSet,
        fuel: ItemSet,
        trail: ItemSet,
        twinkle: ItemSet,
        shapes: FireworkShapes,
    },
    FireworkRocket {
        id: &'static str,
        shell: ItemSet,
        star: ItemSet,
        fuel: ItemSet,
        count: u8,
    },
    BookCloning {
        id: &'static str,
        source: ItemSet,
        material: ItemSet,
    },
    Cooking {
        id: &'static str,
        kind: CookingKind,
        ingredient: ItemSet,
        experience: f32,
        cooking_time: u16,
    },
    Stonecutting {
        id: &'static str,
        ingredient: ItemSet,
        count: u8,
    },
    SmithingTransform {
        id: &'static str,
        template: ItemSet,
        base: ItemSet,
        addition: ItemSet,
    },
    SmithingTrim {
        id: &'static str,
        template: ItemSet,
        base: ItemSet,
        addition: ItemSet,
        pattern: &'static str,
    },
}

impl Craft {
    pub const fn id(&self) -> &'static str {
        match self {
            Self::Shaped { id, .. }
            | Self::Shapeless { id, .. }
            | Self::Transmute { id, .. }
            | Self::Dye { id, .. }
            | Self::Imbue { id, .. }
            | Self::DecoratedPot { id, .. }
            | Self::BannerDuplicate { id }
            | Self::RepairItem { id }
            | Self::ShieldDecoration { id, .. }
            | Self::MapExtending { id, .. }
            | Self::FireworkStarFade { id, .. }
            | Self::FireworkStar { id, .. }
            | Self::FireworkRocket { id, .. }
            | Self::BookCloning { id, .. }
            | Self::Cooking { id, .. }
            | Self::Stonecutting { id, .. }
            | Self::SmithingTransform { id, .. }
            | Self::SmithingTrim { id, .. } => id,
        }
    }

    pub const fn uses_crafting_grid(&self) -> bool {
        !matches!(
            self,
            Self::Cooking { .. }
                | Self::Stonecutting { .. }
                | Self::SmithingTransform { .. }
                | Self::SmithingTrim { .. }
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ToolRule {
    pub blocks: BlockSet,
    pub speed: Option<f32>,
    pub correct_for_drops: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tool {
    pub rules: &'static [ToolRule],
    pub default_mining_speed: f32,
    pub damage_per_block: u32,
    pub can_destroy_blocks_in_creative: bool,
}

impl Tool {
    pub fn mining_speed(&self, block: BlockId) -> f32 {
        self.rules
            .iter()
            .find_map(|rule| rule.speed.filter(|_| rule.blocks.contains(block)))
            .unwrap_or(self.default_mining_speed)
    }

    pub fn is_correct_for_drops(&self, block: BlockId) -> bool {
        self.rules
            .iter()
            .find_map(|rule| {
                rule.correct_for_drops
                    .filter(|_| rule.blocks.contains(block))
            })
            .unwrap_or(false)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Food {
    pub nutrition: u32,
    pub saturation: f32,
    pub can_always_eat: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConsumeAnimation {
    Eat,
    Drink,
}

impl ConsumeAnimation {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Eat => "eat",
            Self::Drink => "drink",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConsumeMobEffect {
    pub effect: &'static str,
    pub amplifier: u8,
    pub duration: i32,
    pub show_icon: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ConsumeEffect {
    ApplyEffects {
        effects: &'static [ConsumeMobEffect],
        probability: f32,
    },
    RemoveEffects(&'static [&'static str]),
    ClearAllEffects,
    TeleportRandomly {
        diameter: f32,
    },
    PlaySound(&'static str),
}

pub const TICKS_PER_SECOND: f32 = 20.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Consumable {
    pub consume_seconds: f32,
    pub animation: ConsumeAnimation,
    pub sound: &'static str,
    pub has_consume_particles: bool,
    pub on_consume_effects: &'static [ConsumeEffect],
}

impl Consumable {
    pub const fn consume_ticks(&self) -> u64 {
        (self.consume_seconds * TICKS_PER_SECOND) as u64
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PotionContentsDefault {
    pub potion: Option<&'static str>,
    pub custom_effects: &'static [ConsumeMobEffect],
}

pub trait Item: 'static {
    const NAME: &'static str;
    const ID: ItemId;
    const CLASS: &'static str;
    const MAX_STACK_SIZE: u8;
    const MAX_DAMAGE: Option<u32>;
    const WEAPON_DAMAGE: Option<u32>;
    const RARITY: Rarity;
    const BLOCK: Option<BlockId>;
    const TOOL: Option<Tool>;
    const FOOD: Option<Food>;
    const EQUIPPABLE: Option<EquipmentSlot>;
    const ENCHANTABLE: Option<u32>;
    const REPAIRABLE: Option<ItemSet>;
    const DAMAGE_RESISTANT: Option<&'static str>;
    const USE_REMAINDER: Option<ItemId>;
    const ATTRIBUTE_MODIFIERS: &'static [AttributeModifier];
    const CONSUMABLE: Option<Consumable> = None;
    const POTION_CONTENTS: Option<PotionContentsDefault> = None;
    const SUSPICIOUS_STEW_EFFECTS: Option<&'static [StewEffect]> = None;
    const CRAFTING_REMAINDER: Option<ItemId> = None;
    const COOKING_FUEL: Option<Fuel> = None;
    const TRIM_MATERIAL: Option<&'static str> = None;
    const CRAFTS: &'static [Craft] = &[];
    type Behavior: ItemBehavior;
}

#[cfg(test)]
mod tests {
    use super::*;

    static TAG: BlockTag = BlockTag {
        name: "minecraft:test",
        blocks: &[BlockId(2), BlockId(5)],
    };

    static RULES: [ToolRule; 2] = [
        ToolRule {
            blocks: BlockSet::Tag(&TAG),
            speed: None,
            correct_for_drops: Some(false),
        },
        ToolRule {
            blocks: BlockSet::Direct(&[BlockId(1), BlockId(2), BlockId(3)]),
            speed: Some(4.0),
            correct_for_drops: Some(true),
        },
    ];

    const TOOL: Tool = Tool {
        rules: &RULES,
        default_mining_speed: 1.0,
        damage_per_block: 1,
        can_destroy_blocks_in_creative: true,
    };

    #[test]
    fn first_matching_rule_with_a_value_wins() {
        assert!(!TOOL.is_correct_for_drops(BlockId(2)));
        assert!(TOOL.is_correct_for_drops(BlockId(3)));
        assert!(!TOOL.is_correct_for_drops(BlockId(5)));
        assert!(!TOOL.is_correct_for_drops(BlockId(9)));
        assert_eq!(TOOL.mining_speed(BlockId(2)), 4.0);
        assert_eq!(TOOL.mining_speed(BlockId(5)), 1.0);
        assert_eq!(TOOL.mining_speed(BlockId(9)), 1.0);
    }

    #[test]
    fn sets_expose_their_members() {
        let tag = BlockSet::Tag(&TAG);
        assert_eq!(tag.tag(), Some("minecraft:test"));
        assert_eq!(tag.blocks(), &[BlockId(2), BlockId(5)]);
        assert!(tag.contains(BlockId(5)));
        assert!(!tag.contains(BlockId(3)));
        let direct = ItemSet::Direct(&[ItemId(4)]);
        assert_eq!(direct.tag(), None);
        assert!(direct.contains(ItemId(4)));
        assert!(!direct.contains(ItemId::AIR));
    }

    #[test]
    fn consumable_components_come_from_the_reports() {
        let kelp = ItemKind::DriedKelp.consumable().unwrap();
        assert_eq!(kelp.consume_seconds, 0.8);
        assert_eq!(kelp.consume_ticks(), 16);
        assert_eq!(kelp.animation, ConsumeAnimation::Eat);
        assert_eq!(kelp.sound, "minecraft:entity.generic.eat");
        assert!(kelp.has_consume_particles);
        let honey = ItemKind::HoneyBottle.consumable().unwrap();
        assert_eq!(honey.consume_ticks(), 40);
        assert_eq!(honey.animation, ConsumeAnimation::Drink);
        assert_eq!(honey.sound, "minecraft:item.honey_bottle.drink");
        assert!(!honey.has_consume_particles);
        assert_eq!(
            honey.on_consume_effects,
            &[ConsumeEffect::RemoveEffects(&["minecraft:poison"])]
        );
        let potion = ItemKind::Potion.consumable().unwrap();
        assert_eq!(potion.consume_ticks(), 32);
        assert_eq!(potion.animation, ConsumeAnimation::Drink);
        assert_eq!(potion.sound, "minecraft:entity.generic.drink");
        assert!(ItemKind::SplashPotion.consumable().is_none());
        assert!(ItemKind::Stone.consumable().is_none());
    }

    #[test]
    fn consume_effects_are_read_whole() {
        let golden = ItemKind::EnchantedGoldenApple.consumable().unwrap();
        let [
            ConsumeEffect::ApplyEffects {
                effects,
                probability,
            },
        ] = golden.on_consume_effects
        else {
            panic!("expected one apply_effects");
        };
        assert_eq!(*probability, 1.0);
        assert_eq!(effects.len(), 4);
        assert_eq!(
            effects[0],
            ConsumeMobEffect {
                effect: "minecraft:regeneration",
                amplifier: 1,
                duration: 400,
                show_icon: true,
            }
        );
        assert_eq!(
            ItemKind::ChorusFruit
                .consumable()
                .unwrap()
                .on_consume_effects,
            &[ConsumeEffect::TeleportRandomly { diameter: 16.0 }]
        );
        assert_eq!(
            ItemKind::MilkBucket
                .consumable()
                .unwrap()
                .on_consume_effects,
            &[ConsumeEffect::ClearAllEffects]
        );
    }

    #[test]
    fn potions_and_stews_expose_their_default_component() {
        assert_eq!(
            ItemKind::Potion.potion_contents(),
            Some(PotionContentsDefault {
                potion: None,
                custom_effects: &[],
            })
        );
        assert!(ItemKind::SplashPotion.potion_contents().is_some());
        assert!(ItemKind::Apple.potion_contents().is_none());
        assert_eq!(
            ItemKind::SuspiciousStew.suspicious_stew_effects(),
            Some(&[][..])
        );
        assert!(ItemKind::MushroomStew.suspicious_stew_effects().is_none());
    }
}
