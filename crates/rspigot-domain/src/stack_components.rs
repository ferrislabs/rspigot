use rspigot_blocks::{BlockId, BlockKind};
use rspigot_items::{
    Consumable, ConsumeAnimation, ConsumeEffect, ConsumeMobEffect, EquipmentSlot, Food, ItemId,
    ItemKind, Tool,
};
use rspigot_nbt::{Compound, Tag};

use crate::BlockStateId;
use crate::item::ItemStack;

pub struct Key {
    pub name: &'static str,
    pub removed: &'static str,
}

macro_rules! key {
    ($name:literal) => {
        Key {
            name: concat!("minecraft:", $name),
            removed: concat!("!minecraft:", $name),
        }
    };
}

pub const FOOD: Key = key!("food");
pub const CONSUMABLE: Key = key!("consumable");
pub const TOOL: Key = key!("tool");
pub const EQUIPPABLE: Key = key!("equippable");
pub const GLIDER: Key = key!("glider");
pub const DEATH_PROTECTION: Key = key!("death_protection");
pub const USE_COOLDOWN: Key = key!("use_cooldown");
pub const MAX_DAMAGE: Key = key!("max_damage");
pub const UNBREAKABLE: Key = key!("unbreakable");
pub const INSTRUMENT: Key = key!("instrument");
pub const BUNDLE_CONTENTS: Key = key!("bundle_contents");
pub const WEAPON: Key = key!("weapon");
pub const BLOCKS_ATTACKS: Key = key!("blocks_attacks");
pub const USE_REMAINDER: Key = key!("use_remainder");
pub const ENCHANTABLE: Key = key!("enchantable");
pub const REPAIRABLE: Key = key!("repairable");
pub const CAN_BREAK: Key = key!("can_break");
pub const CAN_PLACE_ON: Key = key!("can_place_on");

const DEFAULT_CONSUME_SECONDS: f32 = 1.6;
const DEFAULT_EAT_SOUND: &str = "minecraft:entity.generic.eat";
const DEFAULT_MINING_SPEED: f32 = 1.0;
const DEFAULT_DAMAGE_PER_BLOCK: u32 = 1;
const DEFAULT_TELEPORT_DIAMETER: f32 = 16.0;
const TICKS_PER_SECOND: f32 = 20.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Slot<'a> {
    Set(&'a Tag),
    Removed,
    Inherited,
}

pub fn slot<'a>(stack: &'a ItemStack, key: &Key) -> Slot<'a> {
    if let Some(tag) = stack.components.get(key.name) {
        Slot::Set(tag)
    } else if stack.components.contains_key(key.removed) {
        Slot::Removed
    } else {
        Slot::Inherited
    }
}

fn kind_of(stack: &ItemStack) -> Option<ItemKind> {
    stack.item.kind()
}

fn float(compound: &Compound, key: &str, default: f32) -> f32 {
    compound.get_f64(key).map_or(default, |value| value as f32)
}

pub fn food(stack: &ItemStack) -> Option<Food> {
    match slot(stack, &FOOD) {
        Slot::Set(tag) => {
            let compound = tag.as_compound()?;
            Some(Food {
                nutrition: u32::try_from(compound.get_i32("nutrition")?).ok()?,
                saturation: float(compound, "saturation", 0.0),
                can_always_eat: compound.get_bool("can_always_eat").unwrap_or(false),
            })
        }
        Slot::Removed => None,
        Slot::Inherited => kind_of(stack).and_then(ItemKind::food),
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct StackMobEffect {
    pub effect: String,
    pub amplifier: u8,
    pub duration: i32,
    pub show_icon: bool,
}

impl From<&ConsumeMobEffect> for StackMobEffect {
    fn from(spec: &ConsumeMobEffect) -> Self {
        Self {
            effect: spec.effect.to_owned(),
            amplifier: spec.amplifier,
            duration: spec.duration,
            show_icon: spec.show_icon,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum StackConsumeEffect {
    ApplyEffects {
        effects: Vec<StackMobEffect>,
        probability: f32,
    },
    RemoveEffects(Vec<String>),
    ClearAllEffects,
    TeleportRandomly {
        diameter: f32,
    },
    PlaySound(String),
}

impl From<&ConsumeEffect> for StackConsumeEffect {
    fn from(effect: &ConsumeEffect) -> Self {
        match effect {
            ConsumeEffect::ApplyEffects {
                effects,
                probability,
            } => Self::ApplyEffects {
                effects: effects.iter().map(StackMobEffect::from).collect(),
                probability: *probability,
            },
            ConsumeEffect::RemoveEffects(names) => {
                Self::RemoveEffects(names.iter().map(|name| (*name).to_owned()).collect())
            }
            ConsumeEffect::ClearAllEffects => Self::ClearAllEffects,
            ConsumeEffect::TeleportRandomly { diameter } => Self::TeleportRandomly {
                diameter: *diameter,
            },
            ConsumeEffect::PlaySound(sound) => Self::PlaySound((*sound).to_owned()),
        }
    }
}

fn parse_mob_effect(tag: &Tag) -> Option<StackMobEffect> {
    let compound = tag.as_compound()?;
    let particles = compound.get_bool("show_particles").unwrap_or(true);
    Some(StackMobEffect {
        effect: compound.get_str("id")?.to_owned(),
        amplifier: u8::try_from(compound.get_i32("amplifier").unwrap_or(0)).ok()?,
        duration: compound.get_i32("duration").unwrap_or(600),
        show_icon: compound.get_bool("show_icon").unwrap_or(particles),
    })
}

fn names_of(tag: &Tag) -> Vec<String> {
    match tag {
        Tag::String(name) => vec![name.clone()],
        Tag::List(entries) => entries
            .iter()
            .filter_map(|entry| entry.as_str().map(str::to_owned))
            .collect(),
        _ => Vec::new(),
    }
}

fn parse_consume_effect(tag: &Tag) -> Option<StackConsumeEffect> {
    let compound = tag.as_compound()?;
    let name = compound.get_str("type")?;
    let name = name.strip_prefix("minecraft:").unwrap_or(name);
    Some(match name {
        "apply_effects" => StackConsumeEffect::ApplyEffects {
            effects: compound
                .get_list("effects")?
                .iter()
                .filter_map(parse_mob_effect)
                .collect(),
            probability: float(compound, "probability", 1.0),
        },
        "remove_effects" => StackConsumeEffect::RemoveEffects(names_of(compound.get("effects")?)),
        "clear_all_effects" => StackConsumeEffect::ClearAllEffects,
        "teleport_randomly" => StackConsumeEffect::TeleportRandomly {
            diameter: float(compound, "diameter", DEFAULT_TELEPORT_DIAMETER),
        },
        "play_sound" => StackConsumeEffect::PlaySound(compound.get_str("sound")?.to_owned()),
        _ => return None,
    })
}

fn parse_consume_effects(compound: &Compound, key: &str) -> Vec<StackConsumeEffect> {
    compound
        .get_list(key)
        .map(|list| list.iter().filter_map(parse_consume_effect).collect())
        .unwrap_or_default()
}

#[derive(Debug, Clone, PartialEq)]
pub struct StackConsumable {
    pub consume_seconds: f32,
    pub animation: ConsumeAnimation,
    pub sound: String,
    pub has_consume_particles: bool,
    pub on_consume_effects: Vec<StackConsumeEffect>,
}

impl StackConsumable {
    pub fn consume_ticks(&self) -> u64 {
        (self.consume_seconds * TICKS_PER_SECOND) as u64
    }
}

impl From<&Consumable> for StackConsumable {
    fn from(consumable: &Consumable) -> Self {
        Self {
            consume_seconds: consumable.consume_seconds,
            animation: consumable.animation,
            sound: consumable.sound.to_owned(),
            has_consume_particles: consumable.has_consume_particles,
            on_consume_effects: consumable
                .on_consume_effects
                .iter()
                .map(StackConsumeEffect::from)
                .collect(),
        }
    }
}

pub fn consumable(stack: &ItemStack) -> Option<StackConsumable> {
    match slot(stack, &CONSUMABLE) {
        Slot::Set(tag) => {
            let compound = tag.as_compound()?;
            Some(StackConsumable {
                consume_seconds: float(compound, "consume_seconds", DEFAULT_CONSUME_SECONDS),
                animation: match compound.get_str("animation") {
                    Some("drink" | "minecraft:drink") => ConsumeAnimation::Drink,
                    _ => ConsumeAnimation::Eat,
                },
                sound: compound
                    .get_str("sound")
                    .unwrap_or(DEFAULT_EAT_SOUND)
                    .to_owned(),
                has_consume_particles: compound.get_bool("has_consume_particles").unwrap_or(true),
                on_consume_effects: parse_consume_effects(compound, "on_consume_effects"),
            })
        }
        Slot::Removed => None,
        Slot::Inherited => kind_of(stack)
            .and_then(ItemKind::consumable)
            .map(|consumable| StackConsumable::from(&consumable)),
    }
}

#[derive(Debug, Clone, PartialEq)]
enum StackBlocks {
    Tag(&'static [u32]),
    Direct(Vec<BlockId>),
}

impl StackBlocks {
    fn parse(tag: &Tag) -> Self {
        if let Tag::String(name) = tag
            && let Some(tag_name) = name.strip_prefix('#')
        {
            let qualified = if tag_name.contains(':') {
                tag_name.to_owned()
            } else {
                format!("minecraft:{tag_name}")
            };
            return Self::Tag(
                rspigot_data::tags::tag("minecraft:block", &qualified).unwrap_or(&[]),
            );
        }
        Self::Direct(
            names_of(tag)
                .iter()
                .filter_map(|name| BlockKind::from_name(name))
                .map(BlockKind::id)
                .collect(),
        )
    }

    fn contains(&self, block: BlockId) -> bool {
        match self {
            Self::Tag(ids) => ids.binary_search(&u32::from(block.0)).is_ok(),
            Self::Direct(ids) => ids.contains(&block),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct StackToolRule {
    blocks: StackBlocks,
    speed: Option<f32>,
    correct_for_drops: Option<bool>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StackTool {
    rules: Vec<StackToolRule>,
    default_mining_speed: f32,
    damage_per_block: u32,
    can_destroy_blocks_in_creative: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum EffectiveTool {
    Item(Tool),
    Stack(StackTool),
}

impl EffectiveTool {
    pub fn mining_speed(&self, block: BlockId) -> f32 {
        match self {
            Self::Item(tool) => tool.mining_speed(block),
            Self::Stack(tool) => tool
                .rules
                .iter()
                .find_map(|rule| rule.speed.filter(|_| rule.blocks.contains(block)))
                .unwrap_or(tool.default_mining_speed),
        }
    }

    pub fn is_correct_for_drops(&self, block: BlockId) -> bool {
        match self {
            Self::Item(tool) => tool.is_correct_for_drops(block),
            Self::Stack(tool) => tool
                .rules
                .iter()
                .find_map(|rule| {
                    rule.correct_for_drops
                        .filter(|_| rule.blocks.contains(block))
                })
                .unwrap_or(false),
        }
    }

    pub fn damage_per_block(&self) -> u32 {
        match self {
            Self::Item(tool) => tool.damage_per_block,
            Self::Stack(tool) => tool.damage_per_block,
        }
    }

    pub fn can_destroy_blocks_in_creative(&self) -> bool {
        match self {
            Self::Item(tool) => tool.can_destroy_blocks_in_creative,
            Self::Stack(tool) => tool.can_destroy_blocks_in_creative,
        }
    }
}

fn parse_tool(compound: &Compound) -> StackTool {
    let rules = compound
        .get_list("rules")
        .unwrap_or_default()
        .iter()
        .filter_map(Tag::as_compound)
        .filter_map(|rule| {
            Some(StackToolRule {
                blocks: StackBlocks::parse(rule.get("blocks")?),
                speed: rule.get_f64("speed").map(|speed| speed as f32),
                correct_for_drops: rule.get_bool("correct_for_drops"),
            })
        })
        .collect();
    StackTool {
        rules,
        default_mining_speed: float(compound, "default_mining_speed", DEFAULT_MINING_SPEED),
        damage_per_block: compound
            .get_i32("damage_per_block")
            .and_then(|amount| u32::try_from(amount).ok())
            .unwrap_or(DEFAULT_DAMAGE_PER_BLOCK),
        can_destroy_blocks_in_creative: compound
            .get_bool("can_destroy_blocks_in_creative")
            .unwrap_or(true),
    }
}

pub fn tool(stack: &ItemStack) -> Option<EffectiveTool> {
    match slot(stack, &TOOL) {
        Slot::Set(tag) => Some(EffectiveTool::Stack(parse_tool(tag.as_compound()?))),
        Slot::Removed => None,
        Slot::Inherited => kind_of(stack)
            .and_then(ItemKind::tool)
            .map(EffectiveTool::Item),
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Equippable<'a> {
    pub slot: EquipmentSlot,
    pub swappable: bool,
    pub equip_sound: &'a str,
    pub equip_on_interact: bool,
}

fn slot_named(name: &str) -> Option<EquipmentSlot> {
    let name = name.strip_prefix("minecraft:").unwrap_or(name);
    [
        EquipmentSlot::Mainhand,
        EquipmentSlot::Offhand,
        EquipmentSlot::Feet,
        EquipmentSlot::Legs,
        EquipmentSlot::Chest,
        EquipmentSlot::Head,
        EquipmentSlot::Body,
        EquipmentSlot::Saddle,
    ]
    .into_iter()
    .find(|slot| slot.name() == name)
}

pub fn equippable(stack: &ItemStack) -> Option<Equippable<'_>> {
    match slot(stack, &EQUIPPABLE) {
        Slot::Set(tag) => {
            let compound = tag.as_compound()?;
            Some(Equippable {
                slot: slot_named(compound.get_str("slot")?)?,
                swappable: compound.get_bool("swappable").unwrap_or(true),
                equip_sound: compound
                    .get_str("equip_sound")
                    .unwrap_or("minecraft:item.armor.equip_generic"),
                equip_on_interact: compound.get_bool("equip_on_interact").unwrap_or(false),
            })
        }
        Slot::Removed => None,
        Slot::Inherited => {
            let kind = kind_of(stack)?;
            Some(Equippable {
                slot: kind.equippable()?,
                swappable: kind.equippable_swappable(),
                equip_sound: kind.equip_sound(),
                equip_on_interact: false,
            })
        }
    }
}

pub fn is_glider(stack: &ItemStack) -> bool {
    match slot(stack, &GLIDER) {
        Slot::Set(_) => true,
        Slot::Removed => false,
        Slot::Inherited => kind_of(stack).is_some_and(ItemKind::is_glider),
    }
}

pub fn death_effects(stack: &ItemStack) -> Option<Vec<StackConsumeEffect>> {
    match slot(stack, &DEATH_PROTECTION) {
        Slot::Set(tag) => Some(parse_consume_effects(tag.as_compound()?, "death_effects")),
        Slot::Removed => None,
        Slot::Inherited => kind_of(stack)
            .and_then(ItemKind::death_effects)
            .map(|effects| effects.iter().map(StackConsumeEffect::from).collect()),
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UseCooldown<'a> {
    pub seconds: f32,
    pub group: &'a str,
}

impl UseCooldown<'_> {
    pub fn ticks(&self) -> i32 {
        (self.seconds * TICKS_PER_SECOND) as i32
    }
}

pub fn use_cooldown(stack: &ItemStack) -> Option<UseCooldown<'_>> {
    match slot(stack, &USE_COOLDOWN) {
        Slot::Set(tag) => {
            let compound = tag.as_compound()?;
            let seconds = float(compound, "seconds", 0.0);
            (seconds > 0.0).then(|| UseCooldown {
                seconds,
                group: compound
                    .get_str("cooldown_group")
                    .or_else(|| kind_of(stack).map(ItemKind::name))
                    .unwrap_or(""),
            })
        }
        Slot::Removed => None,
        Slot::Inherited => {
            let kind = kind_of(stack)?;
            Some(UseCooldown {
                seconds: kind.use_cooldown_seconds()?,
                group: kind.name(),
            })
        }
    }
}

pub fn cooldown_group(stack: &ItemStack) -> Option<&str> {
    use_cooldown(stack)
        .map(|cooldown| cooldown.group)
        .or_else(|| kind_of(stack).map(ItemKind::name))
}

fn property_matches(wanted: &Tag, actual: &str) -> bool {
    match wanted {
        Tag::String(text) => text == actual,
        Tag::Compound(range) => {
            let above = |bound: Option<&str>| {
                bound.is_none_or(|bound| compare(actual, bound) != std::cmp::Ordering::Less)
            };
            let below = |bound: Option<&str>| {
                bound.is_none_or(|bound| compare(actual, bound) != std::cmp::Ordering::Greater)
            };
            above(range.get_str("min")) && below(range.get_str("max"))
        }
        other => other
            .as_i64()
            .is_some_and(|flag| (flag != 0).to_string() == actual),
    }
}

fn compare(left: &str, right: &str) -> std::cmp::Ordering {
    match (left.parse::<i64>(), right.parse::<i64>()) {
        (Ok(left), Ok(right)) => left.cmp(&right),
        _ => left.cmp(right),
    }
}

fn block_predicate_matches(entry: &Compound, state: BlockStateId) -> bool {
    let Some(kind) = state.kind() else {
        return false;
    };
    if entry.contains_key("nbt") {
        return false;
    }
    let block_ok = entry
        .get("blocks")
        .is_none_or(|blocks| StackBlocks::parse(blocks).contains(kind.id()));
    let state_ok = entry.get_compound("state").is_none_or(|wanted| {
        wanted.iter().all(|(name, value)| {
            state
                .property(name)
                .is_some_and(|actual| property_matches(value, actual))
        })
    });
    block_ok && state_ok
}

pub fn adventure_allows(stack: &ItemStack, key: &Key, state: BlockStateId) -> bool {
    match slot(stack, key) {
        Slot::Set(Tag::List(entries)) => entries
            .iter()
            .filter_map(Tag::as_compound)
            .any(|entry| block_predicate_matches(entry, state)),
        Slot::Set(Tag::Compound(entry)) => block_predicate_matches(entry, state),
        _ => false,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Instrument {
    pub sound: String,
    pub use_seconds: f32,
    pub range: f32,
}

const GOAT_HORN: &str = "minecraft:goat_horn";
const DEFAULT_HORN: &str = "minecraft:ponder_goat_horn";
const HORN_SECONDS: f32 = 7.0;
const HORN_RANGE: f32 = 256.0;
const GOAT_HORNS: [(&str, &str); 8] = [
    (
        "minecraft:ponder_goat_horn",
        "minecraft:item.goat_horn.sound.0",
    ),
    (
        "minecraft:sing_goat_horn",
        "minecraft:item.goat_horn.sound.1",
    ),
    (
        "minecraft:seek_goat_horn",
        "minecraft:item.goat_horn.sound.2",
    ),
    (
        "minecraft:feel_goat_horn",
        "minecraft:item.goat_horn.sound.3",
    ),
    (
        "minecraft:admire_goat_horn",
        "minecraft:item.goat_horn.sound.4",
    ),
    (
        "minecraft:call_goat_horn",
        "minecraft:item.goat_horn.sound.5",
    ),
    (
        "minecraft:yearn_goat_horn",
        "minecraft:item.goat_horn.sound.6",
    ),
    (
        "minecraft:dream_goat_horn",
        "minecraft:item.goat_horn.sound.7",
    ),
];

fn horn(id: &str) -> Option<Instrument> {
    GOAT_HORNS
        .iter()
        .find(|(name, _)| *name == id)
        .map(|(_, sound)| Instrument {
            sound: (*sound).to_owned(),
            use_seconds: HORN_SECONDS,
            range: HORN_RANGE,
        })
}

pub fn instrument(stack: &ItemStack) -> Option<Instrument> {
    match slot(stack, &INSTRUMENT) {
        Slot::Set(Tag::String(id)) => horn(id),
        Slot::Set(Tag::Compound(inline)) => Some(Instrument {
            sound: inline.get_str("sound_event")?.to_owned(),
            use_seconds: float(inline, "use_duration", HORN_SECONDS),
            range: float(inline, "range", HORN_RANGE),
        }),
        Slot::Set(_) | Slot::Removed => None,
        Slot::Inherited => (kind_of(stack)?.name() == GOAT_HORN)
            .then(|| horn(DEFAULT_HORN))
            .flatten(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Weapon {
    pub item_damage_per_attack: u32,
    pub disable_blocking_seconds: f32,
}

const AXE_DISABLE_SECONDS: f32 = 5.0;

pub fn weapon(stack: &ItemStack) -> Option<Weapon> {
    match slot(stack, &WEAPON) {
        Slot::Set(tag) => {
            let compound = tag.as_compound()?;
            Some(Weapon {
                item_damage_per_attack: compound
                    .get_i32("item_damage_per_attack")
                    .and_then(|amount| u32::try_from(amount).ok())
                    .unwrap_or(1),
                disable_blocking_seconds: float(compound, "disable_blocking_for_seconds", 0.0),
            })
        }
        Slot::Removed => None,
        Slot::Inherited => {
            let kind = kind_of(stack)?;
            Some(Weapon {
                item_damage_per_attack: kind.weapon_damage()?,
                disable_blocking_seconds: if kind.name().ends_with("_axe") {
                    AXE_DISABLE_SECONDS
                } else {
                    0.0
                },
            })
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct BlocksAttacks {
    pub block_delay_ticks: u64,
    pub block_sound: Option<String>,
    pub disabled_sound: Option<String>,
    pub damage_threshold: f32,
    pub damage_base: f32,
    pub damage_factor: f32,
    pub bypassed_by: Option<String>,
}

const SHIELD_BLOCK_SOUND: &str = "minecraft:item.shield.block";
const SHIELD_BREAK_SOUND: &str = "minecraft:item.shield.break";
const SHIELD_BYPASSED_BY: &str = "minecraft:bypasses_shield";

pub fn blocks_attacks(stack: &ItemStack) -> Option<BlocksAttacks> {
    match slot(stack, &BLOCKS_ATTACKS) {
        Slot::Set(tag) => {
            let compound = tag.as_compound()?;
            let damage = compound.get_compound("item_damage");
            let read = |key: &str, default: f32| damage.map_or(default, |d| float(d, key, default));
            Some(BlocksAttacks {
                block_delay_ticks: (float(compound, "block_delay_seconds", 0.0) * TICKS_PER_SECOND)
                    as u64,
                block_sound: compound.get_str("block_sound").map(str::to_owned),
                disabled_sound: compound.get_str("disabled_sound").map(str::to_owned),
                damage_threshold: read("threshold", 1.0),
                damage_base: read("base", 0.0),
                damage_factor: read("factor", 1.0),
                bypassed_by: compound
                    .get_str("bypassed_by")
                    .map(|name| name.trim_start_matches('#').to_owned()),
            })
        }
        Slot::Removed => None,
        Slot::Inherited => (kind_of(stack) == Some(ItemKind::Shield)).then(|| BlocksAttacks {
            block_delay_ticks: 5,
            block_sound: Some(SHIELD_BLOCK_SOUND.to_owned()),
            disabled_sound: Some(SHIELD_BREAK_SOUND.to_owned()),
            damage_threshold: 3.0,
            damage_base: 1.0,
            damage_factor: 1.0,
            bypassed_by: Some(SHIELD_BYPASSED_BY.to_owned()),
        }),
    }
}

pub fn use_remainder(stack: &ItemStack) -> Option<ItemId> {
    match slot(stack, &USE_REMAINDER) {
        Slot::Set(tag) => {
            let name = match tag {
                Tag::String(name) => name.as_str(),
                other => other.as_compound()?.get_str("id")?,
            };
            ItemKind::from_name(name).map(ItemKind::id)
        }
        Slot::Removed => None,
        Slot::Inherited => kind_of(stack).and_then(ItemKind::use_remainder),
    }
}

pub fn enchantable(stack: &ItemStack) -> Option<u32> {
    match slot(stack, &ENCHANTABLE) {
        Slot::Set(tag) => u32::try_from(tag.as_compound()?.get_i32("value")?).ok(),
        Slot::Removed => None,
        Slot::Inherited => kind_of(stack).and_then(ItemKind::enchantable),
    }
}

pub fn repairs_with(stack: &ItemStack, material: ItemId) -> bool {
    match slot(stack, &REPAIRABLE) {
        Slot::Set(tag) => tag
            .as_compound()
            .and_then(|compound| compound.get("items"))
            .is_some_and(|items| item_set_contains(items, material)),
        Slot::Removed => false,
        Slot::Inherited => kind_of(stack)
            .and_then(ItemKind::repairable)
            .is_some_and(|set| set.contains(material)),
    }
}

fn item_set_contains(set: &Tag, item: ItemId) -> bool {
    if let Tag::String(name) = set
        && let Some(tag_name) = name.strip_prefix('#')
    {
        let qualified = if tag_name.contains(':') {
            tag_name.to_owned()
        } else {
            format!("minecraft:{tag_name}")
        };
        return rspigot_data::tags::tag("minecraft:item", &qualified)
            .is_some_and(|ids| ids.binary_search(&u32::from(item.0)).is_ok());
    }
    names_of(set)
        .iter()
        .any(|name| ItemKind::from_name(name).is_some_and(|kind| kind.id() == item))
}

pub fn durability(stack: &ItemStack) -> Option<i32> {
    durability_of(stack.item, &stack.components)
}

pub fn durability_of(item: ItemId, components: &Compound) -> Option<i32> {
    if let Some(tag) = components.get(MAX_DAMAGE.name) {
        return tag.as_i64().and_then(|max| i32::try_from(max).ok());
    }
    if components.contains_key(MAX_DAMAGE.removed) {
        return None;
    }
    item.kind()
        .and_then(ItemKind::max_damage)
        .and_then(|max| i32::try_from(max).ok())
}

pub fn max_damage(stack: &ItemStack) -> Option<i32> {
    if matches!(slot(stack, &UNBREAKABLE), Slot::Set(_)) {
        return None;
    }
    durability(stack)
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU8;

    use super::*;

    fn stack(kind: ItemKind) -> ItemStack {
        ItemStack::new(kind.id(), NonZeroU8::MIN)
    }

    #[test]
    fn a_stack_without_overrides_inherits_the_item_defaults() {
        let apple = stack(ItemKind::Apple);
        assert_eq!(food(&apple), ItemKind::Apple.food());
        assert!(consumable(&apple).is_some());
        assert!(is_glider(&stack(ItemKind::Elytra)));
        assert_eq!(
            max_damage(&stack(ItemKind::DiamondPickaxe)),
            ItemKind::DiamondPickaxe.max_damage().map(|max| max as i32)
        );
    }

    #[test]
    fn a_food_component_on_the_stack_wins_over_the_item() {
        let mut stick = stack(ItemKind::Stick);
        assert!(food(&stick).is_none());
        stick.components.insert(
            FOOD.name,
            Compound::new()
                .with("nutrition", 6)
                .with("saturation", 2.0f32)
                .with("can_always_eat", true),
        );
        let eaten = food(&stick).unwrap();
        assert_eq!(eaten.nutrition, 6);
        assert!(eaten.can_always_eat);
    }

    #[test]
    fn a_removed_component_hides_the_item_default() {
        let mut apple = stack(ItemKind::Apple);
        apple.components.insert(FOOD.removed, Compound::new());
        apple.components.insert(CONSUMABLE.removed, Compound::new());
        assert!(food(&apple).is_none());
        assert!(consumable(&apple).is_none());
        let mut elytra = stack(ItemKind::Elytra);
        elytra.components.insert(GLIDER.removed, Compound::new());
        assert!(!is_glider(&elytra));
        let mut totem = stack(ItemKind::TotemOfUndying);
        assert!(death_effects(&totem).is_some());
        totem
            .components
            .insert(DEATH_PROTECTION.removed, Compound::new());
        assert!(death_effects(&totem).is_none());
    }

    #[test]
    fn a_glider_or_death_protection_component_makes_any_item_one() {
        let mut stick = stack(ItemKind::Stick);
        assert!(!is_glider(&stick));
        stick.components.insert(GLIDER.name, Compound::new());
        assert!(is_glider(&stick));
        stick.components.insert(
            DEATH_PROTECTION.name,
            Compound::new().with(
                "death_effects",
                Tag::List(vec![Tag::Compound(
                    Compound::new().with("type", "minecraft:clear_all_effects"),
                )]),
            ),
        );
        assert_eq!(
            death_effects(&stick),
            Some(vec![StackConsumeEffect::ClearAllEffects])
        );
    }

    #[test]
    fn the_consumable_component_sets_time_and_effects() {
        let mut stick = stack(ItemKind::Stick);
        stick.components.insert(
            CONSUMABLE.name,
            Compound::new()
                .with("consume_seconds", 0.5f32)
                .with("animation", "drink"),
        );
        let found = consumable(&stick).unwrap();
        assert_eq!(found.consume_ticks(), 10);
        assert_eq!(found.animation, ConsumeAnimation::Drink);
    }

    #[test]
    fn the_tool_component_replaces_the_item_tool() {
        let mut stick = stack(ItemKind::Stick);
        assert!(tool(&stick).is_none());
        let rule = Compound::new()
            .with("blocks", "#minecraft:mineable/pickaxe")
            .with("speed", 9.0f32)
            .with("correct_for_drops", true);
        stick.components.insert(
            TOOL.name,
            Compound::new()
                .with("rules", Tag::List(vec![Tag::Compound(rule)]))
                .with("damage_per_block", 2),
        );
        let found = tool(&stick).unwrap();
        let stone = BlockKind::from_name("stone").unwrap().id();
        let dirt = BlockKind::from_name("dirt").unwrap().id();
        assert_eq!(found.mining_speed(stone), 9.0);
        assert_eq!(found.mining_speed(dirt), 1.0);
        assert!(found.is_correct_for_drops(stone));
        assert_eq!(found.damage_per_block(), 2);
        let mut pickaxe = stack(ItemKind::DiamondPickaxe);
        pickaxe.components.insert(TOOL.removed, Compound::new());
        assert!(tool(&pickaxe).is_none());
    }

    #[test]
    fn equippable_swappable_and_slot_come_from_the_stack_first() {
        let helmet = stack(ItemKind::DiamondHelmet);
        let found = equippable(&helmet).unwrap();
        assert_eq!(found.slot, EquipmentSlot::Head);
        assert!(found.swappable);
        assert!(
            !equippable(&stack(ItemKind::CarvedPumpkin))
                .unwrap()
                .swappable
        );
        let mut stick = stack(ItemKind::Stick);
        assert!(equippable(&stick).is_none());
        stick.components.insert(
            EQUIPPABLE.name,
            Compound::new()
                .with("slot", "chest")
                .with("swappable", false),
        );
        let found = equippable(&stick).unwrap();
        assert_eq!(found.slot, EquipmentSlot::Chest);
        assert!(!found.swappable);
    }

    #[test]
    fn use_cooldown_reads_seconds_and_group_with_the_item_name_as_fallback() {
        let pearl = stack(ItemKind::EnderPearl);
        let default = use_cooldown(&pearl).unwrap();
        assert_eq!(default.ticks(), 20);
        assert_eq!(default.group, ItemKind::EnderPearl.name());
        let mut stick = stack(ItemKind::Stick);
        assert!(use_cooldown(&stick).is_none());
        stick.components.insert(
            USE_COOLDOWN.name,
            Compound::new()
                .with("seconds", 2.5f32)
                .with("cooldown_group", "minecraft:custom"),
        );
        let found = use_cooldown(&stick).unwrap();
        assert_eq!(found.ticks(), 50);
        assert_eq!(cooldown_group(&stick), Some("minecraft:custom"));
        let mut pearl = pearl;
        pearl
            .components
            .insert(USE_COOLDOWN.removed, Compound::new());
        assert!(use_cooldown(&pearl).is_none());
    }

    #[test]
    fn adventure_state_predicates_match_block_properties() {
        let slab = BlockKind::from_name("oak_slab").unwrap();
        let top = slab.state_with(&[("type", "top")]).unwrap();
        let bottom = slab.state_with(&[("type", "bottom")]).unwrap();
        let mut stick = stack(ItemKind::Stick);
        stick.components.insert(
            CAN_BREAK.name,
            Tag::List(vec![Tag::Compound(
                Compound::new()
                    .with("blocks", "minecraft:oak_slab")
                    .with("state", Compound::new().with("type", "top")),
            )]),
        );
        assert!(adventure_allows(&stick, &CAN_BREAK, top));
        assert!(!adventure_allows(&stick, &CAN_BREAK, bottom));
        let door = BlockKind::from_name("oak_stairs").unwrap().default_state();
        assert!(!adventure_allows(&stick, &CAN_BREAK, door));
    }

    #[test]
    fn adventure_state_predicates_accept_ranges_and_reject_unknown_properties() {
        let wheat = BlockKind::from_name("wheat").unwrap();
        let young = wheat.state_with(&[("age", "2")]).unwrap();
        let ripe = wheat.state_with(&[("age", "7")]).unwrap();
        let mut stick = stack(ItemKind::Stick);
        let range = Compound::new().with("min", "5").with("max", "7");
        stick.components.insert(
            CAN_BREAK.name,
            Compound::new().with("state", Compound::new().with("age", range)),
        );
        assert!(adventure_allows(&stick, &CAN_BREAK, ripe));
        assert!(!adventure_allows(&stick, &CAN_BREAK, young));
        stick.components.insert(
            CAN_BREAK.name,
            Compound::new().with("state", Compound::new().with("nonsense", "1")),
        );
        assert!(!adventure_allows(&stick, &CAN_BREAK, ripe));
    }

    #[test]
    fn adventure_predicates_match_listed_blocks_and_tags() {
        let stone = BlockKind::from_name("stone").unwrap().default_state();
        let dirt = BlockKind::from_name("dirt").unwrap().default_state();
        let mut stick = stack(ItemKind::Stick);
        assert!(!adventure_allows(&stick, &CAN_BREAK, stone));
        stick.components.insert(
            CAN_BREAK.name,
            Tag::List(vec![Tag::Compound(
                Compound::new().with("blocks", "minecraft:stone"),
            )]),
        );
        assert!(adventure_allows(&stick, &CAN_BREAK, stone));
        assert!(!adventure_allows(&stick, &CAN_BREAK, dirt));
        assert!(!adventure_allows(&stick, &CAN_PLACE_ON, stone));
        stick.components.insert(
            CAN_PLACE_ON.name,
            Compound::new().with("blocks", "#minecraft:mineable/pickaxe"),
        );
        assert!(adventure_allows(&stick, &CAN_PLACE_ON, stone));
        assert!(!adventure_allows(&stick, &CAN_PLACE_ON, dirt));
        stick.components.insert(
            CAN_BREAK.name,
            Tag::List(vec![Tag::Compound(Compound::new())]),
        );
        assert!(adventure_allows(&stick, &CAN_BREAK, dirt));
    }

    #[test]
    fn a_goat_horn_plays_its_instrument_sound_for_seven_seconds() {
        let mut horn = stack(ItemKind::GoatHorn);
        let default = instrument(&horn).unwrap();
        assert_eq!(default.sound, "minecraft:item.goat_horn.sound.0");
        assert_eq!(default.use_seconds, 7.0);
        assert_eq!(default.range, 256.0);
        horn.components
            .insert(INSTRUMENT.name, "minecraft:dream_goat_horn");
        assert_eq!(
            instrument(&horn).unwrap().sound,
            "minecraft:item.goat_horn.sound.7"
        );
        assert!(instrument(&stack(ItemKind::Stick)).is_none());
    }

    #[test]
    fn use_remainder_enchantable_and_repairable_follow_the_stack() {
        let mut stick = stack(ItemKind::Stick);
        assert!(use_remainder(&stick).is_none());
        stick.components.insert(
            USE_REMAINDER.name,
            Compound::new().with("id", "minecraft:bowl"),
        );
        assert_eq!(use_remainder(&stick), Some(ItemKind::Bowl.id()));
        assert_eq!(enchantable(&stick), None);
        stick
            .components
            .insert(ENCHANTABLE.name, Compound::new().with("value", 15));
        assert_eq!(enchantable(&stick), Some(15));
        assert!(!repairs_with(&stick, ItemKind::Diamond.id()));
        stick.components.insert(
            REPAIRABLE.name,
            Compound::new().with("items", "minecraft:diamond"),
        );
        assert!(repairs_with(&stick, ItemKind::Diamond.id()));
        assert!(!repairs_with(&stick, ItemKind::Emerald.id()));
        let mut stew = stack(ItemKind::MushroomStew);
        assert_eq!(use_remainder(&stew), ItemKind::MushroomStew.use_remainder());
        stew.components
            .insert(USE_REMAINDER.removed, Compound::new());
        assert!(use_remainder(&stew).is_none());
    }

    #[test]
    fn durability_of_reads_a_loose_component_map() {
        let mut components = Compound::new();
        assert_eq!(durability_of(ItemKind::Stick.id(), &components), None);
        components.insert(MAX_DAMAGE.name, 12);
        assert_eq!(durability_of(ItemKind::Stick.id(), &components), Some(12));
        let mut removed = Compound::new();
        removed.insert(MAX_DAMAGE.removed, Compound::new());
        assert_eq!(durability_of(ItemKind::DiamondSword.id(), &removed), None);
    }

    #[test]
    fn weapon_and_blocks_attacks_follow_the_stack() {
        let axe = stack(ItemKind::IronAxe);
        let found = weapon(&axe).unwrap();
        assert_eq!(found.item_damage_per_attack, 2);
        assert_eq!(found.disable_blocking_seconds, 5.0);
        assert_eq!(weapon(&stack(ItemKind::Stick)), None);
        let shield = blocks_attacks(&stack(ItemKind::Shield)).unwrap();
        assert_eq!(shield.block_delay_ticks, 5);
        assert_eq!(shield.damage_threshold, 3.0);
        assert!(blocks_attacks(&stack(ItemKind::Stick)).is_none());
    }

    #[test]
    fn max_damage_honours_the_component_and_unbreakable() {
        let mut stick = stack(ItemKind::Stick);
        assert_eq!(max_damage(&stick), None);
        stick.components.insert(MAX_DAMAGE.name, 40);
        assert_eq!(max_damage(&stick), Some(40));
        stick.components.insert(UNBREAKABLE.name, Compound::new());
        assert_eq!(max_damage(&stick), None);
    }
}
