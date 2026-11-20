use rspigot_nbt::{Compound, Tag};
use uuid::Uuid;

use crate::{Attribute, Color, DyeColor, EffectType, EquipmentSlot, ItemStack};

pub(crate) const UNBREAKABLE: &str = "minecraft:unbreakable";
pub(crate) const DAMAGE: &str = "minecraft:damage";
pub(crate) const MAX_DAMAGE: &str = "minecraft:max_damage";
pub(crate) const REPAIR_COST: &str = "minecraft:repair_cost";
pub(crate) const MAX_STACK_SIZE: &str = "minecraft:max_stack_size";
pub(crate) const ITEM_NAME: &str = "minecraft:item_name";
pub(crate) const RARITY: &str = "minecraft:rarity";
pub(crate) const GLINT: &str = "minecraft:enchantment_glint_override";
pub(crate) const ITEM_MODEL: &str = "minecraft:item_model";
pub(crate) const TOOLTIP_DISPLAY: &str = "minecraft:tooltip_display";
pub(crate) const ATTRIBUTE_MODIFIERS: &str = "minecraft:attribute_modifiers";
pub(crate) const DYED_COLOR: &str = "minecraft:dyed_color";
pub(crate) const POTION_CONTENTS: &str = "minecraft:potion_contents";
pub(crate) const BASE_COLOR: &str = "minecraft:base_color";
pub(crate) const BANNER_PATTERNS: &str = "minecraft:banner_patterns";
pub(crate) const PROFILE: &str = "minecraft:profile";
pub(crate) const WRITTEN_BOOK: &str = "minecraft:written_book_content";
pub(crate) const WRITABLE_BOOK: &str = "minecraft:writable_book_content";
pub(crate) const FIREWORK_EXPLOSION: &str = "minecraft:firework_explosion";
pub(crate) const FIREWORKS: &str = "minecraft:fireworks";
pub(crate) const TRIM: &str = "minecraft:trim";
pub(crate) const BUNDLE_CONTENTS: &str = "minecraft:bundle_contents";
pub(crate) const CONTAINER: &str = "minecraft:container";
pub(crate) const CHARGED_PROJECTILES: &str = "minecraft:charged_projectiles";
pub(crate) const LODESTONE_TRACKER: &str = "minecraft:lodestone_tracker";
pub(crate) const INSTRUMENT: &str = "minecraft:instrument";
pub(crate) const JUKEBOX_PLAYABLE: &str = "minecraft:jukebox_playable";
pub(crate) const DAMAGE_RESISTANT: &str = "minecraft:damage_resistant";
pub(crate) const GLIDER: &str = "minecraft:glider";
pub(crate) const EQUIPPABLE: &str = "minecraft:equippable";
pub(crate) const USE_COOLDOWN: &str = "minecraft:use_cooldown";
pub(crate) const TOOL: &str = "minecraft:tool";
pub(crate) const MAP_ID: &str = "minecraft:map_id";
pub(crate) const FIRE_DAMAGE_TAG: &str = "#minecraft:is_fire";

pub(crate) const TYPED_ELSEWHERE: [&str; 4] = [
    "minecraft:custom_name",
    "minecraft:lore",
    "minecraft:enchantments",
    "minecraft:custom_model_data",
];

fn text(compound: &Compound, key: &str) -> Option<String> {
    compound.get_str(key).map(str::to_owned)
}

fn strings(tag: Option<&Tag>) -> Vec<String> {
    match tag {
        Some(Tag::String(single)) => vec![single.clone()],
        Some(Tag::List(items)) => items
            .iter()
            .filter_map(|item| item.as_str().map(str::to_owned))
            .collect(),
        _ => Vec::new(),
    }
}

fn string_list(values: &[String]) -> Tag {
    Tag::List(
        values
            .iter()
            .map(|value| Tag::from(value.as_str()))
            .collect(),
    )
}

fn set_tag(values: &[String]) -> Tag {
    match values {
        [single] if single.starts_with('#') => Tag::from(single.as_str()),
        many => string_list(many),
    }
}

fn filterable(tag: Option<&Tag>) -> Option<String> {
    match tag? {
        Tag::String(value) => Some(value.clone()),
        Tag::Compound(both) => text(both, "raw"),
        _ => None,
    }
}

fn int_list(tag: Option<&Tag>) -> Vec<i32> {
    tag.and_then(Tag::as_list)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_i64().and_then(|value| i32::try_from(value).ok()))
                .collect()
        })
        .unwrap_or_default()
}

fn ints(values: &[i32]) -> Tag {
    Tag::List(values.iter().map(|value| Tag::Int(*value)).collect())
}

macro_rules! named_enum {
    ($(#[$meta:meta])* $name:ident { $($variant:ident => $key:literal),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum $name {
            $($variant),+
        }

        impl $name {
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];

            pub const fn key(self) -> &'static str {
                match self {
                    $(Self::$variant => $key),+
                }
            }

            pub fn from_key(key: &str) -> Option<Self> {
                let key = key.strip_prefix("minecraft:").unwrap_or(key);
                Self::ALL.iter().copied().find(|known| known.key() == key)
            }
        }
    };
}

named_enum!(Rarity {
    Common => "common",
    Uncommon => "uncommon",
    Rare => "rare",
    Epic => "epic",
});

named_enum!(Operation {
    AddValue => "add_value",
    AddMultipliedBase => "add_multiplied_base",
    AddMultipliedTotal => "add_multiplied_total",
});

named_enum!(SlotGroup {
    Any => "any",
    MainHand => "mainhand",
    OffHand => "offhand",
    Hand => "hand",
    Feet => "feet",
    Legs => "legs",
    Chest => "chest",
    Head => "head",
    Armor => "armor",
    Body => "body",
    Saddle => "saddle",
});

named_enum!(FireworkShape {
    SmallBall => "small_ball",
    LargeBall => "large_ball",
    Star => "star",
    Creeper => "creeper",
    Burst => "burst",
});

impl From<EquipmentSlot> for SlotGroup {
    fn from(slot: EquipmentSlot) -> Self {
        match slot {
            EquipmentSlot::Hand => Self::MainHand,
            EquipmentSlot::OffHand => Self::OffHand,
            EquipmentSlot::Feet => Self::Feet,
            EquipmentSlot::Legs => Self::Legs,
            EquipmentSlot::Chest => Self::Chest,
            EquipmentSlot::Head => Self::Head,
            EquipmentSlot::Body => Self::Body,
            EquipmentSlot::Saddle => Self::Saddle,
        }
    }
}

const EQUIPMENT: [(EquipmentSlot, &str); 8] = [
    (EquipmentSlot::Hand, "mainhand"),
    (EquipmentSlot::Feet, "feet"),
    (EquipmentSlot::Legs, "legs"),
    (EquipmentSlot::Chest, "chest"),
    (EquipmentSlot::Head, "head"),
    (EquipmentSlot::OffHand, "offhand"),
    (EquipmentSlot::Body, "body"),
    (EquipmentSlot::Saddle, "saddle"),
];

const DYES: [(DyeColor, &str); 16] = [
    (DyeColor::White, "white"),
    (DyeColor::Orange, "orange"),
    (DyeColor::Magenta, "magenta"),
    (DyeColor::LightBlue, "light_blue"),
    (DyeColor::Yellow, "yellow"),
    (DyeColor::Lime, "lime"),
    (DyeColor::Pink, "pink"),
    (DyeColor::Gray, "gray"),
    (DyeColor::LightGray, "light_gray"),
    (DyeColor::Cyan, "cyan"),
    (DyeColor::Purple, "purple"),
    (DyeColor::Blue, "blue"),
    (DyeColor::Brown, "brown"),
    (DyeColor::Green, "green"),
    (DyeColor::Red, "red"),
    (DyeColor::Black, "black"),
];

fn dye_key(color: DyeColor) -> &'static str {
    DYES.iter()
        .find(|(known, _)| *known == color)
        .map_or("white", |(_, key)| key)
}

fn dye_of(key: &str) -> Option<DyeColor> {
    let key = key.strip_prefix("minecraft:").unwrap_or(key);
    DYES.iter()
        .find(|(_, known)| *known == key)
        .map(|(color, _)| *color)
}

pub(crate) fn dye_tag(color: DyeColor) -> Tag {
    Tag::from(dye_key(color))
}

pub(crate) fn dye_from(tag: &Tag) -> Option<DyeColor> {
    dye_of(tag.as_str()?)
}

named_enum!(ItemFlag {
    HideEnchantments => "enchantments",
    HideAttributes => "attributes",
    HideUnbreakable => "unbreakable",
    HideCanBreak => "can_break",
    HideCanPlaceOn => "can_place_on",
    HideAdditionalTooltip => "additional_tooltip",
    HideDye => "dye",
    HideArmorTrim => "armor_trim",
    HideCustomData => "custom_data",
    HideMaxStackSize => "max_stack_size",
    HideMaxDamage => "max_damage",
    HideDamage => "damage",
    HideCustomName => "custom_name",
    HideItemName => "item_name",
    HideItemModel => "item_model",
    HideLore => "lore",
    HideRarity => "rarity",
    HideCustomModelData => "custom_model_data",
});

impl ItemFlag {
    pub const fn components(self) -> &'static [&'static str] {
        match self {
            Self::HideEnchantments => &["minecraft:enchantments", "minecraft:stored_enchantments"],
            Self::HideAttributes => &["minecraft:attribute_modifiers"],
            Self::HideUnbreakable => &["minecraft:unbreakable"],
            Self::HideCanBreak => &["minecraft:can_break"],
            Self::HideCanPlaceOn => &["minecraft:can_place_on"],
            Self::HideAdditionalTooltip => &[
                "minecraft:potion_contents",
                "minecraft:banner_patterns",
                "minecraft:firework_explosion",
                "minecraft:fireworks",
                "minecraft:suspicious_stew_effects",
                "minecraft:map_decorations",
                "minecraft:written_book_content",
                "minecraft:lodestone_tracker",
                "minecraft:jukebox_playable",
                "minecraft:instrument",
                "minecraft:pot_decorations",
                "minecraft:charged_projectiles",
            ],
            Self::HideDye => &["minecraft:dyed_color"],
            Self::HideArmorTrim => &["minecraft:trim"],
            Self::HideCustomData => &["minecraft:custom_data"],
            Self::HideMaxStackSize => &["minecraft:max_stack_size"],
            Self::HideMaxDamage => &["minecraft:max_damage"],
            Self::HideDamage => &["minecraft:damage"],
            Self::HideCustomName => &["minecraft:custom_name"],
            Self::HideItemName => &["minecraft:item_name"],
            Self::HideItemModel => &["minecraft:item_model"],
            Self::HideLore => &["minecraft:lore"],
            Self::HideRarity => &["minecraft:rarity"],
            Self::HideCustomModelData => &["minecraft:custom_model_data"],
        }
    }
}

impl Attribute {
    pub fn key(self) -> String {
        let name = format!("{self:?}");
        let mut key = String::from("minecraft:");
        for (index, letter) in name.chars().enumerate() {
            if letter.is_ascii_uppercase() {
                if index > 0 {
                    key.push('_');
                }
                key.push(letter.to_ascii_lowercase());
            } else {
                key.push(letter);
            }
        }
        key
    }

    pub fn from_key(key: &str) -> Option<Self> {
        let key = if key.contains(':') {
            key.to_owned()
        } else {
            format!("minecraft:{key}")
        };
        Self::ALL.iter().copied().find(|known| known.key() == key)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AttributeModifier {
    pub attribute: Attribute,
    pub id: String,
    pub amount: f64,
    pub operation: Operation,
    pub slot: SlotGroup,
}

impl AttributeModifier {
    pub fn new(
        attribute: Attribute,
        id: impl Into<String>,
        amount: f64,
        operation: Operation,
        slot: impl Into<SlotGroup>,
    ) -> Self {
        Self {
            attribute,
            id: id.into(),
            amount,
            operation,
            slot: slot.into(),
        }
    }

    pub(crate) fn to_tag(&self) -> Tag {
        Tag::Compound(
            Compound::new()
                .with("type", self.attribute.key())
                .with("id", self.id.as_str())
                .with("amount", self.amount)
                .with("operation", self.operation.key())
                .with("slot", self.slot.key()),
        )
    }

    pub(crate) fn from_tag(tag: &Tag) -> Option<Self> {
        let entry = tag.as_compound()?;
        Some(Self {
            attribute: Attribute::from_key(entry.get_str("type")?)?,
            id: text(entry, "id")?,
            amount: entry.get_f64("amount")?,
            operation: Operation::from_key(entry.get_str("operation")?)?,
            slot: entry
                .get_str("slot")
                .map_or(Some(SlotGroup::Any), SlotGroup::from_key)?,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PotionEffect {
    pub effect: EffectType,
    pub duration: i32,
    pub amplifier: i32,
    pub ambient: bool,
    pub show_particles: bool,
    pub show_icon: bool,
}

impl PotionEffect {
    pub const fn new(effect: EffectType, duration: i32, amplifier: i32) -> Self {
        Self {
            effect,
            duration,
            amplifier,
            ambient: false,
            show_particles: true,
            show_icon: true,
        }
    }

    fn to_tag(&self) -> Tag {
        Tag::Compound(
            Compound::new()
                .with("id", self.effect.key())
                .with("amplifier", self.amplifier)
                .with("duration", self.duration)
                .with("ambient", self.ambient)
                .with("show_particles", self.show_particles)
                .with("show_icon", self.show_icon),
        )
    }

    fn from_tag(tag: &Tag) -> Option<Self> {
        let effect = tag.as_compound()?;
        let particles = effect.get_bool("show_particles").unwrap_or(true);
        Some(Self {
            effect: EffectType::from_key(effect.get_str("id")?)?,
            duration: effect.get_i32("duration").unwrap_or(600),
            amplifier: effect.get_i32("amplifier").unwrap_or(0),
            ambient: effect.get_bool("ambient").unwrap_or(false),
            show_particles: particles,
            show_icon: effect.get_bool("show_icon").unwrap_or(particles),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct PotionContents {
    pub potion: Option<String>,
    pub custom_color: Option<Color>,
    pub effects: Vec<PotionEffect>,
    pub custom_name: Option<String>,
}

impl PotionContents {
    pub fn of(potion: impl Into<String>) -> Self {
        Self {
            potion: Some(potion.into()),
            ..Self::default()
        }
    }

    pub(crate) fn to_tag(&self) -> Tag {
        let mut out = Compound::new();
        if let Some(potion) = &self.potion {
            out.insert("potion", potion.as_str());
        }
        if let Some(color) = self.custom_color {
            out.insert("custom_color", color.to_rgb() as i32);
        }
        if !self.effects.is_empty() {
            out.insert(
                "custom_effects",
                Tag::List(self.effects.iter().map(PotionEffect::to_tag).collect()),
            );
        }
        if let Some(name) = &self.custom_name {
            out.insert("custom_name", name.as_str());
        }
        Tag::Compound(out)
    }

    pub(crate) fn from_tag(tag: &Tag) -> Option<Self> {
        let contents = match tag {
            Tag::String(potion) => return Some(Self::of(potion.as_str())),
            other => other.as_compound()?,
        };
        Some(Self {
            potion: text(contents, "potion"),
            custom_color: contents.get_i32("custom_color").map(color_of),
            effects: contents
                .get_list("custom_effects")
                .unwrap_or_default()
                .iter()
                .filter_map(PotionEffect::from_tag)
                .collect(),
            custom_name: text(contents, "custom_name"),
        })
    }
}

fn color_of(rgb: i32) -> Color {
    let rgb = rgb as u32;
    Color::rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

pub(crate) fn color_tag(color: Color) -> Tag {
    Tag::Int(color.to_rgb() as i32)
}

pub(crate) fn color_from(tag: &Tag) -> Option<Color> {
    i32::try_from(tag.as_i64()?).ok().map(color_of)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BannerLayer {
    pub pattern: String,
    pub color: DyeColor,
}

impl BannerLayer {
    pub fn new(pattern: impl Into<String>, color: DyeColor) -> Self {
        Self {
            pattern: pattern.into(),
            color,
        }
    }

    pub(crate) fn to_tag(&self) -> Tag {
        Tag::Compound(
            Compound::new()
                .with("pattern", self.pattern.as_str())
                .with("color", dye_key(self.color)),
        )
    }

    pub(crate) fn from_tag(tag: &Tag) -> Option<Self> {
        let layer = tag.as_compound()?;
        Some(Self {
            pattern: text(layer, "pattern")?,
            color: dye_of(layer.get_str("color")?)?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileProperty {
    pub name: String,
    pub value: String,
    pub signature: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Profile {
    pub name: Option<String>,
    pub id: Option<Uuid>,
    pub properties: Vec<ProfileProperty>,
}

impl Profile {
    pub fn named(name: impl Into<String>) -> Self {
        Self {
            name: Some(name.into()),
            ..Self::default()
        }
    }

    pub const fn with_id(id: Uuid) -> Self {
        Self {
            name: None,
            id: Some(id),
            properties: Vec::new(),
        }
    }

    pub(crate) fn to_tag(&self) -> Tag {
        let mut out = Compound::new();
        if let Some(name) = &self.name {
            out.insert("name", name.as_str());
        }
        if let Some(id) = self.id {
            let bits = id.as_u128();
            out.insert(
                "id",
                vec![
                    (bits >> 96) as i32,
                    (bits >> 64) as i32,
                    (bits >> 32) as i32,
                    bits as i32,
                ],
            );
        }
        if !self.properties.is_empty() {
            out.insert(
                "properties",
                Tag::List(
                    self.properties
                        .iter()
                        .map(|property| {
                            let mut entry = Compound::new()
                                .with("name", property.name.as_str())
                                .with("value", property.value.as_str());
                            if let Some(signature) = &property.signature {
                                entry.insert("signature", signature.as_str());
                            }
                            Tag::Compound(entry)
                        })
                        .collect(),
                ),
            );
        }
        Tag::Compound(out)
    }

    pub(crate) fn from_tag(tag: &Tag) -> Option<Self> {
        let profile = match tag {
            Tag::String(name) => return Some(Self::named(name.as_str())),
            other => other.as_compound()?,
        };
        let id = profile.get_int_array("id").and_then(|parts| {
            let [a, b, c, d] = parts[..] else {
                return None;
            };
            Some(Uuid::from_u128(
                (u128::from(a as u32) << 96)
                    | (u128::from(b as u32) << 64)
                    | (u128::from(c as u32) << 32)
                    | u128::from(d as u32),
            ))
        });
        Some(Self {
            name: text(profile, "name"),
            id,
            properties: profile
                .get_list("properties")
                .unwrap_or_default()
                .iter()
                .filter_map(|entry| {
                    let entry = entry.as_compound()?;
                    Some(ProfileProperty {
                        name: text(entry, "name")?,
                        value: text(entry, "value")?,
                        signature: text(entry, "signature"),
                    })
                })
                .collect(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WrittenBook {
    pub title: String,
    pub author: String,
    pub generation: u8,
    pub pages: Vec<String>,
}

impl WrittenBook {
    pub(crate) fn to_tag(&self) -> Tag {
        let mut out = Compound::new()
            .with("title", self.title.as_str())
            .with("author", self.author.as_str());
        if self.generation != 0 {
            out.insert("generation", i32::from(self.generation));
        }
        if !self.pages.is_empty() {
            out.insert("pages", string_list(&self.pages));
        }
        Tag::Compound(out)
    }

    pub(crate) fn from_tag(tag: &Tag) -> Option<Self> {
        let book = tag.as_compound()?;
        Some(Self {
            title: filterable(book.get("title"))?,
            author: text(book, "author")?,
            generation: book
                .get_i32("generation")
                .and_then(|value| u8::try_from(value).ok())
                .unwrap_or(0),
            pages: book
                .get_list("pages")
                .unwrap_or_default()
                .iter()
                .filter_map(|page| filterable(Some(page)))
                .collect(),
        })
    }
}

pub(crate) fn writable_tag(pages: &[String]) -> Tag {
    let mut out = Compound::new();
    if !pages.is_empty() {
        out.insert("pages", string_list(pages));
    }
    Tag::Compound(out)
}

pub(crate) fn writable_from(tag: &Tag) -> Option<Vec<String>> {
    let book = tag.as_compound()?;
    Some(
        book.get_list("pages")
            .unwrap_or_default()
            .iter()
            .filter_map(|page| filterable(Some(page)))
            .collect(),
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FireworkExplosion {
    pub shape: FireworkShape,
    pub colors: Vec<i32>,
    pub fade_colors: Vec<i32>,
    pub trail: bool,
    pub twinkle: bool,
}

impl FireworkExplosion {
    pub const fn new(shape: FireworkShape) -> Self {
        Self {
            shape,
            colors: Vec::new(),
            fade_colors: Vec::new(),
            trail: false,
            twinkle: false,
        }
    }

    pub(crate) fn to_tag(&self) -> Tag {
        let mut out = Compound::new().with("shape", self.shape.key());
        if !self.colors.is_empty() {
            out.insert("colors", ints(&self.colors));
        }
        if !self.fade_colors.is_empty() {
            out.insert("fade_colors", ints(&self.fade_colors));
        }
        if self.trail {
            out.insert("has_trail", true);
        }
        if self.twinkle {
            out.insert("has_twinkle", true);
        }
        Tag::Compound(out)
    }

    pub(crate) fn from_tag(tag: &Tag) -> Option<Self> {
        let explosion = tag.as_compound()?;
        Some(Self {
            shape: FireworkShape::from_key(explosion.get_str("shape")?)?,
            colors: int_list(explosion.get("colors")),
            fade_colors: int_list(explosion.get("fade_colors")),
            trail: explosion.get_bool("has_trail").unwrap_or(false),
            twinkle: explosion.get_bool("has_twinkle").unwrap_or(false),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Fireworks {
    pub flight_duration: u8,
    pub explosions: Vec<FireworkExplosion>,
}

impl Fireworks {
    pub(crate) fn to_tag(&self) -> Tag {
        let mut out = Compound::new();
        if self.flight_duration != 0 {
            out.insert("flight_duration", i32::from(self.flight_duration));
        }
        if !self.explosions.is_empty() {
            out.insert(
                "explosions",
                Tag::List(
                    self.explosions
                        .iter()
                        .map(FireworkExplosion::to_tag)
                        .collect(),
                ),
            );
        }
        Tag::Compound(out)
    }

    pub(crate) fn from_tag(tag: &Tag) -> Option<Self> {
        let rocket = tag.as_compound()?;
        Some(Self {
            flight_duration: rocket
                .get_i32("flight_duration")
                .and_then(|value| u8::try_from(value).ok())
                .unwrap_or(0),
            explosions: rocket
                .get_list("explosions")
                .unwrap_or_default()
                .iter()
                .filter_map(FireworkExplosion::from_tag)
                .collect(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trim {
    pub material: String,
    pub pattern: String,
}

impl Trim {
    pub fn new(material: impl Into<String>, pattern: impl Into<String>) -> Self {
        Self {
            material: material.into(),
            pattern: pattern.into(),
        }
    }

    pub(crate) fn to_tag(&self) -> Tag {
        Tag::Compound(
            Compound::new()
                .with("material", self.material.as_str())
                .with("pattern", self.pattern.as_str()),
        )
    }

    pub(crate) fn from_tag(tag: &Tag) -> Option<Self> {
        let trim = tag.as_compound()?;
        Some(Self {
            material: text(trim, "material")?,
            pattern: text(trim, "pattern")?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LodestoneTarget {
    pub dimension: String,
    pub pos: [i32; 3],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LodestoneTracker {
    pub target: Option<LodestoneTarget>,
    pub tracked: bool,
}

impl LodestoneTracker {
    pub(crate) fn to_tag(&self) -> Tag {
        let mut out = Compound::new();
        if let Some(target) = &self.target {
            out.insert(
                "target",
                Compound::new()
                    .with("dimension", target.dimension.as_str())
                    .with("pos", target.pos.to_vec()),
            );
        }
        if !self.tracked {
            out.insert("tracked", false);
        }
        Tag::Compound(out)
    }

    pub(crate) fn from_tag(tag: &Tag) -> Option<Self> {
        let tracker = tag.as_compound()?;
        let target = tracker.get_compound("target").and_then(|target| {
            let pos: [i32; 3] = target.get_int_array("pos")?.try_into().ok()?;
            Some(LodestoneTarget {
                dimension: text(target, "dimension")?,
                pos,
            })
        });
        Some(Self {
            target,
            tracked: tracker.get_bool("tracked").unwrap_or(true),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Equippable {
    pub slot: EquipmentSlot,
    pub equip_sound: Option<String>,
    pub asset_id: Option<String>,
    pub dispensable: bool,
    pub swappable: bool,
    pub damage_on_hurt: bool,
}

impl Equippable {
    pub const fn new(slot: EquipmentSlot) -> Self {
        Self {
            slot,
            equip_sound: None,
            asset_id: None,
            dispensable: true,
            swappable: true,
            damage_on_hurt: true,
        }
    }

    pub(crate) fn to_tag(&self) -> Tag {
        let slot = EQUIPMENT
            .iter()
            .find(|(known, _)| *known == self.slot)
            .map_or("mainhand", |(_, key)| key);
        let mut out = Compound::new().with("slot", slot);
        if let Some(sound) = &self.equip_sound {
            out.insert("equip_sound", sound.as_str());
        }
        if let Some(asset) = &self.asset_id {
            out.insert("asset_id", asset.as_str());
        }
        if !self.dispensable {
            out.insert("dispensable", false);
        }
        if !self.swappable {
            out.insert("swappable", false);
        }
        if !self.damage_on_hurt {
            out.insert("damage_on_hurt", false);
        }
        Tag::Compound(out)
    }

    pub(crate) fn from_tag(tag: &Tag) -> Option<Self> {
        let gear = tag.as_compound()?;
        let slot = gear.get_str("slot")?;
        Some(Self {
            slot: EQUIPMENT.iter().find(|(_, key)| *key == slot)?.0,
            equip_sound: gear.get_str("equip_sound").map(str::to_owned),
            asset_id: text(gear, "asset_id"),
            dispensable: gear.get_bool("dispensable").unwrap_or(true),
            swappable: gear.get_bool("swappable").unwrap_or(true),
            damage_on_hurt: gear.get_bool("damage_on_hurt").unwrap_or(true),
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct UseCooldown {
    pub seconds: f32,
    pub group: Option<String>,
}

impl UseCooldown {
    pub(crate) fn to_tag(&self) -> Tag {
        let mut out = Compound::new().with("seconds", self.seconds);
        if let Some(group) = &self.group {
            out.insert("cooldown_group", group.as_str());
        }
        Tag::Compound(out)
    }

    pub(crate) fn from_tag(tag: &Tag) -> Option<Self> {
        let cooldown = tag.as_compound()?;
        Some(Self {
            seconds: cooldown.get_f64("seconds")? as f32,
            group: text(cooldown, "cooldown_group"),
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ToolRule {
    pub blocks: Vec<String>,
    pub speed: Option<f32>,
    pub correct_for_drops: Option<bool>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Tool {
    pub rules: Vec<ToolRule>,
    pub default_mining_speed: f32,
    pub damage_per_block: i32,
}

impl Default for Tool {
    fn default() -> Self {
        Self {
            rules: Vec::new(),
            default_mining_speed: 1.0,
            damage_per_block: 1,
        }
    }
}

impl Tool {
    pub(crate) fn to_tag(&self) -> Tag {
        let rules = self
            .rules
            .iter()
            .map(|rule| {
                let mut entry = Compound::new().with("blocks", set_tag(&rule.blocks));
                if let Some(speed) = rule.speed {
                    entry.insert("speed", speed);
                }
                if let Some(correct) = rule.correct_for_drops {
                    entry.insert("correct_for_drops", correct);
                }
                Tag::Compound(entry)
            })
            .collect();
        Tag::Compound(
            Compound::new()
                .with("rules", Tag::List(rules))
                .with("default_mining_speed", self.default_mining_speed)
                .with("damage_per_block", self.damage_per_block),
        )
    }

    pub(crate) fn from_tag(tag: &Tag) -> Option<Self> {
        let tool = tag.as_compound()?;
        Some(Self {
            rules: tool
                .get_list("rules")
                .unwrap_or_default()
                .iter()
                .filter_map(|rule| {
                    let rule = rule.as_compound()?;
                    Some(ToolRule {
                        blocks: strings(rule.get("blocks")),
                        speed: rule.get_f64("speed").map(|speed| speed as f32),
                        correct_for_drops: rule.get_bool("correct_for_drops"),
                    })
                })
                .collect(),
            default_mining_speed: tool.get_f64("default_mining_speed").unwrap_or(1.0) as f32,
            damage_per_block: tool.get_i32("damage_per_block").unwrap_or(1),
        })
    }
}

pub(crate) fn tooltip_display(components: &Compound) -> (bool, Vec<String>) {
    let display = components.get_compound(TOOLTIP_DISPLAY);
    (
        display
            .and_then(|display| display.get_bool("hide_tooltip"))
            .unwrap_or(false),
        display
            .map(|display| strings(display.get("hidden_components")))
            .unwrap_or_default(),
    )
}

pub(crate) fn store_tooltip_display(components: &mut Compound, hide: bool, hidden: &[String]) {
    if !hide && hidden.is_empty() {
        components.remove(TOOLTIP_DISPLAY);
        return;
    }
    let mut display = Compound::new();
    if hide {
        display.insert("hide_tooltip", true);
    }
    if !hidden.is_empty() {
        display.insert("hidden_components", string_list(hidden));
    }
    components.insert(TOOLTIP_DISPLAY, display);
}

pub(crate) fn stack_list(stacks: &[ItemStack]) -> Tag {
    Tag::List(stacks.iter().map(ItemStack::to_nbt).collect())
}

pub(crate) fn stack_list_from(tag: &Tag) -> Vec<ItemStack> {
    tag.as_list()
        .unwrap_or_default()
        .iter()
        .filter_map(ItemStack::from_nbt)
        .collect()
}

pub(crate) fn container_tag(stacks: &[ItemStack]) -> Tag {
    Tag::List(
        stacks
            .iter()
            .enumerate()
            .map(|(slot, stack)| {
                Tag::Compound(
                    Compound::new()
                        .with("slot", slot as i32)
                        .with("item", stack.to_nbt()),
                )
            })
            .collect(),
    )
}

pub(crate) fn container_from(tag: &Tag) -> Vec<ItemStack> {
    tag.as_list()
        .unwrap_or_default()
        .iter()
        .filter_map(|entry| ItemStack::from_nbt(entry.as_compound()?.get("item")?))
        .collect()
}

pub(crate) fn resistant_tag(types: &[String]) -> Tag {
    Tag::Compound(Compound::new().with("types", set_tag(types)))
}

pub(crate) fn resistant_from(tag: &Tag) -> Option<Vec<String>> {
    Some(strings(tag.as_compound()?.get("types")))
}

pub(crate) fn glider_tag() -> Tag {
    Tag::Compound(Compound::new())
}

pub(crate) fn jukebox_tag(song: &str) -> Tag {
    Tag::Compound(Compound::new().with("song", song))
}

pub(crate) fn jukebox_from(tag: &Tag) -> Option<String> {
    match tag {
        Tag::String(song) => Some(song.clone()),
        other => text(other.as_compound()?, "song"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attribute_keys_are_registered_attributes() {
        for attribute in Attribute::ALL {
            let key = attribute.key();
            assert!(
                rspigot_data::registries::static_index("minecraft:attribute", &key).is_some(),
                "{key}"
            );
            assert_eq!(Attribute::from_key(&key), Some(*attribute));
        }
    }

    #[test]
    fn item_flag_components_are_registered_component_types() {
        for flag in ItemFlag::ALL {
            for component in flag.components() {
                assert!(
                    rspigot_data::registries::static_index(
                        "minecraft:data_component_type",
                        component
                    )
                    .is_some(),
                    "{component}"
                );
            }
        }
    }

    #[test]
    fn every_typed_value_round_trips_through_its_tag() {
        let modifier = AttributeModifier::new(
            Attribute::AttackDamage,
            "test:bonus",
            4.5,
            Operation::AddValue,
            EquipmentSlot::Hand,
        );
        assert_eq!(
            AttributeModifier::from_tag(&modifier.to_tag()),
            Some(modifier)
        );
        let potion = PotionContents {
            potion: Some("minecraft:strength".to_owned()),
            custom_color: Some(Color::rgb(1, 2, 3)),
            effects: vec![PotionEffect::new(EffectType::Speed, 200, 1)],
            custom_name: Some("brew".to_owned()),
        };
        assert_eq!(PotionContents::from_tag(&potion.to_tag()), Some(potion));
        let layer = BannerLayer::new("minecraft:stripe_top", DyeColor::Red);
        assert_eq!(BannerLayer::from_tag(&layer.to_tag()), Some(layer));
        let profile = Profile {
            name: Some("Notch".to_owned()),
            id: Some(Uuid::from_u128(0x0123_4567_89ab_cdef_0123_4567_89ab_cdef)),
            properties: vec![ProfileProperty {
                name: "textures".to_owned(),
                value: "v".to_owned(),
                signature: Some("s".to_owned()),
            }],
        };
        assert_eq!(Profile::from_tag(&profile.to_tag()), Some(profile));
        let book = WrittenBook {
            title: "T".to_owned(),
            author: "A".to_owned(),
            generation: 2,
            pages: vec!["one".to_owned(), "two".to_owned()],
        };
        assert_eq!(WrittenBook::from_tag(&book.to_tag()), Some(book));
        let pages = vec!["p".to_owned()];
        assert_eq!(writable_from(&writable_tag(&pages)), Some(pages));
        let explosion = FireworkExplosion {
            shape: FireworkShape::Star,
            colors: vec![0xff0000],
            fade_colors: vec![0x00ff00],
            trail: true,
            twinkle: true,
        };
        assert_eq!(
            FireworkExplosion::from_tag(&explosion.to_tag()),
            Some(explosion.clone())
        );
        let rocket = Fireworks {
            flight_duration: 3,
            explosions: vec![explosion],
        };
        assert_eq!(Fireworks::from_tag(&rocket.to_tag()), Some(rocket));
        let trim = Trim::new("minecraft:quartz", "minecraft:coast");
        assert_eq!(Trim::from_tag(&trim.to_tag()), Some(trim));
        let tracker = LodestoneTracker {
            target: Some(LodestoneTarget {
                dimension: "minecraft:overworld".to_owned(),
                pos: [1, -2, 3],
            }),
            tracked: false,
        };
        assert_eq!(LodestoneTracker::from_tag(&tracker.to_tag()), Some(tracker));
        let gear = Equippable {
            slot: EquipmentSlot::Head,
            equip_sound: Some("minecraft:item.armor.equip_iron".to_owned()),
            asset_id: Some("minecraft:iron".to_owned()),
            dispensable: false,
            swappable: false,
            damage_on_hurt: false,
        };
        assert_eq!(Equippable::from_tag(&gear.to_tag()), Some(gear));
        let cooldown = UseCooldown {
            seconds: 2.5,
            group: Some("test:group".to_owned()),
        };
        assert_eq!(UseCooldown::from_tag(&cooldown.to_tag()), Some(cooldown));
        let tool = Tool {
            rules: vec![ToolRule {
                blocks: vec!["#minecraft:mineable/pickaxe".to_owned()],
                speed: Some(6.0),
                correct_for_drops: Some(true),
            }],
            default_mining_speed: 2.0,
            damage_per_block: 2,
        };
        assert_eq!(Tool::from_tag(&tool.to_tag()), Some(tool));
        let types = vec!["#minecraft:is_fire".to_owned()];
        assert_eq!(resistant_from(&resistant_tag(&types)), Some(types));
        assert_eq!(
            jukebox_from(&jukebox_tag("minecraft:cat")),
            Some("minecraft:cat".to_owned())
        );
    }
}
