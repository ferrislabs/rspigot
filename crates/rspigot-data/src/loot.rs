use std::cell::RefCell;
use std::collections::BTreeMap;
use std::sync::Arc;

use crate::generated::loot::{BLOCK_TABLES, ENTITY_TABLES, PREDICATES, SMELTING, TABLES};
use crate::registries;
use crate::tags::HolderSet;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LootTable {
    pub pools: &'static [Pool],
    pub modifier: &'static [Function],
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pool {
    pub rolls: IntProvider,
    pub bonus_rolls: f32,
    pub condition: Option<Condition>,
    pub modifier: &'static [Function],
    pub entries: &'static [Entry],
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Entry {
    pub condition: Option<Condition>,
    pub modifier: &'static [Function],
    pub kind: EntryKind,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EntryKind {
    Leaf(Leaf),
    Alternatives(&'static [Entry]),
    Sequence(&'static [Entry]),
    Group(&'static [Entry]),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Leaf {
    pub weight: i32,
    pub quality: i32,
    pub output: Output,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Output {
    Item(u16),
    Items { items: HolderSet, expand: bool },
    Empty,
    Dynamic(&'static str),
    Table(&'static str),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Condition {
    SurvivesExplosion,
    MatchBlock(BlockPredicate),
    MatchTool(ItemPredicate),
    LocationCheck {
        offset: [i32; 3],
        block: Option<BlockPredicate>,
        biomes: &'static [&'static str],
    },
    EntityProperties {
        target: EntityTarget,
        predicate: Option<&'static EntityPredicate>,
    },
    KilledByPlayer,
    DamageSourceProperties(&'static DamageSourcePredicate),
    RandomChance(f32),
    RandomChanceByLevel(LevelBasedValue),
    RandomChanceWithEnchantedBonus {
        enchantment: &'static str,
        unenchanted_chance: f32,
        enchanted_chance: LevelBasedValue,
    },
    TableBonus {
        enchantment: &'static str,
        chances: &'static [f32],
    },
    Inverted(&'static Condition),
    AnyOf(&'static [Condition]),
    AllOf(&'static [Condition]),
    Reference(&'static str),
    TimeCheck {
        period: Option<i64>,
        min: Option<f32>,
        max: Option<f32>,
    },
    WeatherCheck {
        raining: Option<bool>,
        thundering: Option<bool>,
    },
    ValueCheck {
        value: NumberProvider,
        min: Option<NumberProvider>,
        max: Option<NumberProvider>,
    },
    EntityScores {
        target: EntityTarget,
        scores: &'static [(&'static str, IntRange)],
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NumberProvider {
    Constant(f32),
    Uniform { min: f32, max: f32 },
    Binomial { n: i32, p: f32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntityTarget {
    This,
    Attacker,
    DirectAttacker,
    AttackingPlayer,
    TargetEntity,
    InteractingEntity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntityFlags {
    pub baby: Option<bool>,
    pub on_fire: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntitySubPredicate {
    Sheep {
        sheared: Option<bool>,
    },
    CubeMob {
        size: IntRange,
    },
    Raider {
        has_raid: Option<bool>,
        captain: Option<bool>,
    },
    FishingHook {
        in_open_water: Option<bool>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntityPredicate {
    pub kind: Option<HolderSet>,
    pub flags: EntityFlags,
    pub mainhand: Option<ItemPredicate>,
    pub vehicle: Option<&'static EntityPredicate>,
    pub components: &'static [(&'static str, &'static str)],
    pub sub: &'static [EntitySubPredicate],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TagPredicate {
    pub tag: &'static str,
    pub expected: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DamageSourcePredicate {
    pub tags: &'static [TagPredicate],
    pub source_entity: Option<&'static EntityPredicate>,
    pub direct_entity: Option<&'static EntityPredicate>,
    pub is_direct: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LevelBasedValue {
    Constant(f32),
    Linear {
        base: f32,
        per_level_above_first: f32,
    },
    Fraction {
        numerator: &'static LevelBasedValue,
        denominator: &'static LevelBasedValue,
    },
}

impl LevelBasedValue {
    pub fn calculate(&self, level: i32) -> f32 {
        match *self {
            Self::Constant(value) => value,
            Self::Linear {
                base,
                per_level_above_first,
            } => base + per_level_above_first * (level - 1) as f32,
            Self::Fraction {
                numerator,
                denominator,
            } => {
                let divisor = denominator.calculate(level);
                if divisor == 0.0 {
                    0.0
                } else {
                    numerator.calculate(level) / divisor
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockPredicate {
    pub blocks: Option<HolderSet>,
    pub state: &'static [(&'static str, &'static str)],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ItemPredicate {
    pub items: Option<HolderSet>,
    pub enchantments: &'static [EnchantmentPredicate],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EnchantmentPredicate {
    pub enchantments: Option<&'static [&'static str]>,
    pub levels: Option<IntRange>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IntRange {
    pub min: Option<i32>,
    pub max: Option<i32>,
}

impl IntRange {
    pub fn contains(&self, value: i32) -> bool {
        self.min.is_none_or(|min| value >= min) && self.max.is_none_or(|max| value <= max)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Function {
    pub condition: Option<Condition>,
    pub kind: FunctionKind,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FunctionKind {
    SetCount {
        count: IntProvider,
        add: bool,
    },
    ApplyBonus {
        enchantment: &'static str,
        formula: BonusFormula,
    },
    LimitCount {
        min: Option<IntProvider>,
        max: Option<IntProvider>,
    },
    ExplosionDecay,
    EnchantedCountIncrease {
        enchantment: &'static str,
        count: FloatProvider,
        limit: i32,
    },
    FurnaceSmelt {
        use_input_count: bool,
    },
    SetPotion(&'static str),
    EnchantRandomly {
        enchantment: &'static str,
        max_level: i32,
    },
    EnchantRandomlyFrom {
        options: &'static str,
        only_compatible: bool,
    },
    EnchantRandomlyAmong {
        options: &'static [&'static str],
        only_compatible: bool,
    },
    SetStewEffect(&'static [(&'static str, IntProvider)]),
    SetInstrument(&'static str),
    SetEnchantments {
        levels: &'static [(&'static str, IntProvider)],
        add: bool,
    },
    ExplorationMap {
        destination: &'static str,
        decoration: &'static str,
        zoom: i32,
        search_radius: i32,
        skip_existing_chunks: bool,
    },
    RequireMapId,
    SetOminousBottleAmplifier(IntProvider),
    SetDamage(FloatProvider),
    EnchantWithLevels {
        options: &'static str,
        levels: IntProvider,
    },
    CopyComponents {
        include: &'static [&'static str],
    },
    CopyState {
        block: &'static str,
        properties: &'static [&'static str],
    },
    SetName {
        name: &'static str,
        target: NameTarget,
    },
    SetLore {
        lines: &'static [&'static str],
        mode: ListMode,
    },
    SetCustomData {
        tag: &'static str,
    },
    CopyCustomData {
        source: CustomDataSource,
        operations: &'static [CopyOperation],
    },
    ToggleTooltips(&'static [(&'static str, bool)]),
    SetComponents(&'static [(&'static str, &'static str)]),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameTarget {
    CustomName,
    ItemName,
}

impl NameTarget {
    pub const fn component(self) -> &'static str {
        match self {
            Self::CustomName => "minecraft:custom_name",
            Self::ItemName => "minecraft:item_name",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListMode {
    ReplaceAll,
    ReplaceSection { offset: i32, size: Option<i32> },
    Insert { offset: i32 },
    Append,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CustomDataSource {
    BlockEntity,
    Entity(EntityTarget),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeStrategy {
    Replace,
    Append,
    Merge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CopyOperation {
    pub source: &'static str,
    pub target: &'static str,
    pub strategy: MergeStrategy,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BonusFormula {
    OreDrops,
    UniformBonusCount { bonus_multiplier: i32 },
    BinomialWithBonusCount { extra: i32, probability: f32 },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FloatProvider {
    Constant(f32),
    Uniform { min: f32, max: f32 },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum IntProvider {
    Constant(i32),
    Uniform { min: i32, max: i32 },
    Binomial { n: i32, p: f32 },
}

pub fn tables() -> &'static [(&'static str, LootTable)] {
    &TABLES
}

#[derive(Debug, Default)]
pub struct LootOverlay {
    tables: BTreeMap<String, &'static LootTable>,
    predicates: BTreeMap<String, &'static Condition>,
}

impl LootOverlay {
    pub fn insert_table(&mut self, name: &str, table: &'static LootTable) {
        self.tables.insert(name.to_owned(), table);
    }

    pub fn insert_predicate(&mut self, name: &str, condition: &'static Condition) {
        self.predicates.insert(name.to_owned(), condition);
    }

    pub fn is_empty(&self) -> bool {
        self.tables.is_empty() && self.predicates.is_empty()
    }

    pub fn table_names(&self) -> impl Iterator<Item = &str> {
        self.tables.keys().map(String::as_str)
    }

    pub fn predicate_names(&self) -> impl Iterator<Item = &str> {
        self.predicates.keys().map(String::as_str)
    }
}

thread_local! {
    static ACTIVE: RefCell<Option<Arc<LootOverlay>>> = const { RefCell::new(None) };
}

pub fn activate(overlay: &Arc<LootOverlay>) {
    ACTIVE.with(|active| {
        let mut active = active.borrow_mut();
        let same = match active.as_ref() {
            Some(current) => Arc::ptr_eq(current, overlay),
            None => overlay.is_empty(),
        };
        if !same {
            *active = (!overlay.is_empty()).then(|| Arc::clone(overlay));
        }
    });
}

fn overlaid<T>(look: impl FnOnce(&LootOverlay) -> Option<T>) -> Option<T> {
    ACTIVE.with(|active| active.borrow().as_ref().and_then(|overlay| look(overlay)))
}

fn overlay_active() -> bool {
    ACTIVE.with(|active| active.borrow().is_some())
}

pub fn table(name: &str) -> Option<&'static LootTable> {
    overlaid(|overlay| overlay.tables.get(name).copied()).or_else(|| static_table(name))
}

pub fn static_table(name: &str) -> Option<&'static LootTable> {
    TABLES
        .binary_search_by(|(candidate, _)| (*candidate).cmp(name))
        .ok()
        .map(|index| &TABLES[index].1)
}

fn overlaid_default(
    registry: &str,
    folder: &str,
    id: u16,
    fallback: Option<&str>,
) -> Option<&'static LootTable> {
    if !overlay_active() {
        return None;
    }
    let name = registries::static_entries(registry)?.get(usize::from(id))?;
    let path = name.strip_prefix("minecraft:")?;
    let own = format!("minecraft:{folder}/{path}");
    overlaid(|overlay| {
        overlay
            .tables
            .get(&own)
            .or_else(|| fallback.and_then(|name| overlay.tables.get(name)))
            .copied()
    })
}

pub fn block_table(block: u16) -> Option<&'static LootTable> {
    let index = BLOCK_TABLES.get(usize::from(block)).copied();
    let fallback = index.and_then(|index| TABLES.get(usize::from(index)).map(|(name, _)| *name));
    overlaid_default("minecraft:block", "blocks", block, fallback).or_else(|| static_default(index))
}

pub fn entity_table(entity_type: u16) -> Option<&'static LootTable> {
    let index = ENTITY_TABLES.get(usize::from(entity_type)).copied();
    let fallback = index.and_then(|index| TABLES.get(usize::from(index)).map(|(name, _)| *name));
    overlaid_default("minecraft:entity_type", "entities", entity_type, fallback)
        .or_else(|| static_default(index))
}

fn static_default(index: Option<u16>) -> Option<&'static LootTable> {
    TABLES.get(usize::from(index?)).map(|(_, table)| table)
}

pub fn smelted(item: u16) -> Option<u16> {
    SMELTING
        .binary_search_by_key(&item, |(input, _)| *input)
        .ok()
        .map(|index| SMELTING[index].1)
}

pub fn predicates() -> &'static [(&'static str, Condition)] {
    &PREDICATES
}

pub fn predicate(name: &str) -> Option<&'static Condition> {
    overlaid(|overlay| overlay.predicates.get(name).copied()).or_else(|| static_predicate(name))
}

pub fn static_predicate(name: &str) -> Option<&'static Condition> {
    PREDICATES
        .binary_search_by(|(candidate, _)| (*candidate).cmp(name))
        .ok()
        .map(|index| &PREDICATES[index].1)
}

#[cfg(test)]
mod tests {
    use std::ptr;

    use super::*;
    use crate::registries;
    use crate::tags::HolderSet;

    const BLOCK_TABLE_FILES: usize = 1201;
    const ENTITY_TABLE_FILES: usize = 109;
    const REFERENCED_TABLES: usize = 1;
    const GAMEPLAY_TABLES: usize = 22;
    const CHEST_TABLE_FILES: usize = 58;
    const SPAWNER_TABLES: usize = 5;
    const FISHING_TABLES: usize = 3;
    const BRUSH_TABLES: usize = 1;
    const ARCHAEOLOGY_TABLES: usize = 6;
    const SNOW_GOLEM_SHEARING_TABLES: usize = 1;
    const SHEEP_COLOUR_TABLES: usize = 16;
    const CHARGED_CREEPER_TABLES: usize = 6;
    const TILL_TABLES: usize = 1;
    const HARVEST_TABLES: usize = 3;
    const ENTITY_TYPES_WITH_TABLE: usize = 93;
    const WALL_VARIANTS_WITHOUT_OWN_TABLE: usize = 63;

    fn block_id(name: &str) -> u16 {
        u16::try_from(registries::static_index("minecraft:block", name).unwrap()).unwrap()
    }

    fn item_id(name: &str) -> u16 {
        u16::try_from(registries::static_index("minecraft:item", name).unwrap()).unwrap()
    }

    fn visit_condition<'a>(condition: &'a Condition, out: &mut Vec<&'a Condition>) {
        out.push(condition);
        match condition {
            Condition::Inverted(term) => visit_condition(term, out),
            Condition::AnyOf(terms) | Condition::AllOf(terms) => {
                terms.iter().for_each(|term| visit_condition(term, out));
            }
            _ => {}
        }
    }

    fn visit_functions<'a>(functions: &'a [Function], out: &mut Vec<&'a Condition>) {
        for function in functions {
            if let Some(condition) = &function.condition {
                visit_condition(condition, out);
            }
        }
    }

    fn visit_entry<'a>(entry: &'a Entry, out: &mut Vec<&'a Condition>) {
        if let Some(condition) = &entry.condition {
            visit_condition(condition, out);
        }
        visit_functions(entry.modifier, out);
        match &entry.kind {
            EntryKind::Leaf(_) => {}
            EntryKind::Alternatives(children)
            | EntryKind::Sequence(children)
            | EntryKind::Group(children) => {
                children.iter().for_each(|child| visit_entry(child, out));
            }
        }
    }

    fn all_conditions() -> Vec<&'static Condition> {
        let mut out = Vec::new();
        for (_, table) in tables() {
            visit_functions(table.modifier, &mut out);
            for pool in table.pools {
                if let Some(condition) = &pool.condition {
                    visit_condition(condition, &mut out);
                }
                visit_functions(pool.modifier, &mut out);
                pool.entries
                    .iter()
                    .for_each(|entry| visit_entry(entry, &mut out));
            }
        }
        for (_, condition) in predicates() {
            visit_condition(condition, &mut out);
        }
        out
    }

    fn entity_id(name: &str) -> u16 {
        u16::try_from(registries::static_index("minecraft:entity_type", name).unwrap()).unwrap()
    }

    #[test]
    fn tables_are_sorted_and_addressable() {
        assert_eq!(
            tables().len(),
            BLOCK_TABLE_FILES
                + ENTITY_TABLE_FILES
                + REFERENCED_TABLES
                + CHEST_TABLE_FILES
                + SPAWNER_TABLES
                + GAMEPLAY_TABLES
                + FISHING_TABLES
                + BRUSH_TABLES
                + ARCHAEOLOGY_TABLES
                + SHEEP_COLOUR_TABLES
                + SNOW_GOLEM_SHEARING_TABLES
                + CHARGED_CREEPER_TABLES
                + TILL_TABLES
                + HARVEST_TABLES
        );
        assert!(tables().windows(2).all(|pair| pair[0].0 < pair[1].0));
        let count = |prefix: &str| {
            tables()
                .iter()
                .filter(|(name, _)| name.starts_with(prefix))
                .count()
        };
        assert_eq!(count("minecraft:blocks/"), BLOCK_TABLE_FILES);
        assert_eq!(count("minecraft:entities/"), ENTITY_TABLE_FILES);
        assert!(table("minecraft:gameplay/fishing/fish").is_some());
        assert!(table("minecraft:gameplay/chicken_lay").is_some());
        assert!(table("minecraft:gameplay/piglin_bartering").is_some());
        assert!(table("minecraft:gameplay/hero_of_the_village/armorer_gift").is_some());
        assert!(table("minecraft:gameplay/hero_of_the_village/baby_gift").is_some());
        assert!(table("minecraft:gameplay/armadillo_shed").is_some());
        assert!(table("minecraft:gameplay/sniffer_digging").is_some());
        assert!(table("minecraft:brush/armadillo").is_some());
        assert!(table("minecraft:shearing/sheep").is_some());
        assert!(table("minecraft:shearing/snow_golem").is_some());
        assert_eq!(count("minecraft:shearing/sheep/"), SHEEP_COLOUR_TABLES);
        assert_eq!(count("minecraft:charged_creeper/"), CHARGED_CREEPER_TABLES);
        assert_eq!(count("minecraft:till/"), TILL_TABLES);
        assert!(table("minecraft:harvest/beehive").is_some());
        assert_eq!(count("minecraft:spawners/"), SPAWNER_TABLES);
        for (name, entry) in tables() {
            assert!(ptr::eq(table(name).unwrap(), entry));
        }
        assert!(table("minecraft:blocks/nope").is_none());
    }

    #[test]
    fn entity_types_resolve_their_default_table() {
        let types = registries::static_entries("minecraft:entity_type").unwrap();
        let mut resolved = 0;
        for (index, name) in types.iter().enumerate() {
            let id = u16::try_from(index).unwrap();
            let path = name.strip_prefix("minecraft:").unwrap();
            let own = table(&format!("minecraft:entities/{path}"));
            assert_eq!(entity_table(id).map(ptr::from_ref), own.map(ptr::from_ref));
            resolved += usize::from(own.is_some());
        }
        assert_eq!(resolved, ENTITY_TYPES_WITH_TABLE);
        assert!(entity_table(entity_id("minecraft:item")).is_none());
        assert!(entity_table(u16::MAX).is_none());
    }

    #[test]
    fn sheep_nests_one_table_per_colour() {
        let sheep = entity_table(entity_id("minecraft:sheep")).unwrap();
        let nested: Vec<&str> = sheep
            .pools
            .iter()
            .flat_map(|pool| pool.entries.iter())
            .flat_map(|entry| match entry.kind {
                EntryKind::Alternatives(children) => children.to_vec(),
                _ => vec![*entry],
            })
            .filter_map(|entry| match entry.kind {
                EntryKind::Leaf(Leaf {
                    output: Output::Table(name),
                    ..
                }) => Some(name),
                _ => None,
            })
            .collect();
        assert_eq!(nested.len(), 16);
        for name in nested {
            assert!(name.starts_with("minecraft:entities/sheep/"));
            assert!(table(name).is_some());
        }
    }

    #[test]
    fn cow_table_shape() {
        let cow = entity_table(entity_id("minecraft:cow")).unwrap();
        let [leather, beef] = cow.pools else { panic!() };
        let [entry] = leather.entries else { panic!() };
        assert_eq!(
            entry.kind,
            EntryKind::Leaf(Leaf {
                weight: 1,
                quality: 0,
                output: Output::Item(item_id("minecraft:leather")),
            })
        );
        let [set_count, looting] = entry.modifier else {
            panic!()
        };
        assert_eq!(
            set_count.kind,
            FunctionKind::SetCount {
                count: IntProvider::Uniform { min: 0, max: 2 },
                add: false
            }
        );
        assert_eq!(
            looting.kind,
            FunctionKind::EnchantedCountIncrease {
                enchantment: "minecraft:looting",
                count: FloatProvider::Uniform { min: 0.0, max: 1.0 },
                limit: 0
            }
        );
        let [entry] = beef.entries else { panic!() };
        let [_, smelt, _] = entry.modifier else {
            panic!()
        };
        assert_eq!(
            smelt.kind,
            FunctionKind::FurnaceSmelt {
                use_input_count: true
            }
        );
        let Some(Condition::AnyOf([on_fire, _])) = smelt.condition else {
            panic!()
        };
        let Condition::EntityProperties {
            target: EntityTarget::This,
            predicate: Some(predicate),
        } = on_fire
        else {
            panic!()
        };
        assert_eq!(predicate.flags.on_fire, Some(true));
    }

    #[test]
    fn smelting_maps_raw_food_to_cooked() {
        assert_eq!(
            smelted(item_id("minecraft:beef")),
            Some(item_id("minecraft:cooked_beef"))
        );
        assert_eq!(
            smelted(item_id("minecraft:cod")),
            Some(item_id("minecraft:cooked_cod"))
        );
        assert_eq!(
            smelted(item_id("minecraft:oak_log")),
            Some(item_id("minecraft:charcoal"))
        );
        assert_eq!(smelted(item_id("minecraft:leather")), None);
    }

    #[test]
    fn every_block_with_a_table_resolves() {
        let blocks = registries::static_entries("minecraft:block").unwrap();
        let mut resolved = 0;
        for (index, name) in blocks.iter().enumerate() {
            let id = u16::try_from(index).unwrap();
            let path = name.strip_prefix("minecraft:").unwrap();
            if let Some(own) = table(&format!("minecraft:blocks/{path}")) {
                assert!(ptr::eq(block_table(id).unwrap(), own));
            }
            if block_table(id).is_some() {
                resolved += 1;
            }
        }
        assert_eq!(
            resolved,
            BLOCK_TABLE_FILES + WALL_VARIANTS_WITHOUT_OWN_TABLE
        );
        let floor = |wall: &str, floor: &str| {
            assert!(ptr::eq(
                block_table(block_id(wall)).unwrap(),
                table(floor).unwrap()
            ));
        };
        floor("minecraft:wall_torch", "minecraft:blocks/torch");
        floor("minecraft:oak_wall_sign", "minecraft:blocks/oak_sign");
        floor("minecraft:red_wall_banner", "minecraft:blocks/red_banner");
        floor(
            "minecraft:tube_coral_wall_fan",
            "minecraft:blocks/tube_coral_fan",
        );
        for name in ["minecraft:air", "minecraft:water", "minecraft:bedrock"] {
            assert!(block_table(block_id(name)).is_none());
        }
        assert!(block_table(u16::MAX).is_none());
    }

    #[test]
    fn every_named_predicate_reference_resolves() {
        assert_eq!(predicates().len(), 3);
        assert!(predicates().windows(2).all(|pair| pair[0].0 < pair[1].0));
        let mut references = Vec::new();
        for condition in all_conditions() {
            if let Condition::Reference(name) = condition {
                assert!(predicate(name).is_some());
                references.push(*name);
            }
        }
        references.sort_unstable();
        references.dedup();
        assert_eq!(
            references,
            ["minecraft:tool/can_shear", "minecraft:tool/can_silk_touch"]
        );
        assert!(predicate("minecraft:block/fast_cooking").is_some());
        assert!(predicate("minecraft:nope").is_none());
    }

    #[test]
    fn stone_table_shape() {
        let stone = block_table(block_id("minecraft:stone")).unwrap();
        assert!(stone.modifier.is_empty());
        let [pool] = stone.pools else { panic!() };
        assert_eq!(pool.rolls, IntProvider::Constant(1));
        assert!(pool.condition.is_none());
        let [entry] = pool.entries else { panic!() };
        let EntryKind::Alternatives([silk, plain]) = entry.kind else {
            panic!()
        };
        assert_eq!(
            silk.condition,
            Some(Condition::Reference("minecraft:tool/can_silk_touch"))
        );
        assert_eq!(
            silk.kind,
            EntryKind::Leaf(Leaf {
                weight: 1,
                quality: 0,
                output: Output::Item(item_id("minecraft:stone")),
            })
        );
        assert_eq!(plain.condition, Some(Condition::SurvivesExplosion));
        assert_eq!(
            plain.kind,
            EntryKind::Leaf(Leaf {
                weight: 1,
                quality: 0,
                output: Output::Item(item_id("minecraft:cobblestone")),
            })
        );
    }

    #[test]
    fn named_predicates_shape() {
        assert_eq!(
            predicate("minecraft:tool/can_shear"),
            Some(&Condition::MatchTool(ItemPredicate {
                items: Some(HolderSet::Direct(Vec::leak(vec![u32::from(item_id(
                    "minecraft:shears"
                ))]))),
                enchantments: &[],
            }))
        );
        assert_eq!(
            predicate("minecraft:tool/can_silk_touch"),
            Some(&Condition::MatchTool(ItemPredicate {
                items: None,
                enchantments: &[EnchantmentPredicate {
                    enchantments: Some(&["minecraft:silk_touch"]),
                    levels: Some(IntRange {
                        min: Some(1),
                        max: None
                    }),
                }],
            }))
        );
    }

    #[test]
    fn slab_and_crop_functions() {
        let slab = block_table(block_id("minecraft:oak_slab")).unwrap();
        let [pool] = slab.pools else { panic!() };
        let [entry] = pool.entries else { panic!() };
        let [set_count, decay] = entry.modifier else {
            panic!()
        };
        assert_eq!(
            set_count.kind,
            FunctionKind::SetCount {
                count: IntProvider::Constant(2),
                add: false
            }
        );
        assert_eq!(
            set_count.condition,
            Some(Condition::MatchBlock(BlockPredicate {
                blocks: Some(HolderSet::Direct(Vec::leak(vec![u32::from(block_id(
                    "minecraft:oak_slab"
                ))]))),
                state: &[("type", "double")],
            }))
        );
        assert_eq!(decay.kind, FunctionKind::ExplosionDecay);
        let wheat = block_table(block_id("minecraft:wheat")).unwrap();
        assert_eq!(wheat.modifier.len(), 1);
        let seeds = &wheat.pools[1].entries[0];
        assert_eq!(
            seeds.modifier[0].kind,
            FunctionKind::ApplyBonus {
                enchantment: "minecraft:fortune",
                formula: BonusFormula::BinomialWithBonusCount {
                    extra: 3,
                    probability: 0.5714286
                },
            }
        );
    }

    fn marker(item: u16) -> &'static LootTable {
        let entry: &'static [Entry] = Vec::leak(vec![Entry {
            condition: None,
            modifier: &[],
            kind: EntryKind::Leaf(Leaf {
                weight: 1,
                quality: 0,
                output: Output::Item(item),
            }),
        }]);
        let pools: &'static [Pool] = Vec::leak(vec![Pool {
            rolls: IntProvider::Constant(1),
            bonus_rolls: 0.0,
            condition: None,
            modifier: &[],
            entries: entry,
        }]);
        Box::leak(Box::new(LootTable {
            pools,
            modifier: &[],
        }))
    }

    #[test]
    fn an_active_overlay_replaces_tables_and_predicates_by_name() {
        let stone = block_id("minecraft:stone");
        let static_stone = block_table(stone).unwrap();
        let diamond = marker(item_id("minecraft:diamond"));
        let torch = marker(item_id("minecraft:stick"));
        let flag: &'static Condition = Box::leak(Box::new(Condition::KilledByPlayer));
        let mut overlay = LootOverlay::default();
        overlay.insert_table("minecraft:blocks/stone", diamond);
        overlay.insert_table("minecraft:blocks/torch", torch);
        overlay.insert_table("t:new", diamond);
        overlay.insert_predicate("t:p", flag);
        overlay.insert_predicate("minecraft:tool/can_shear", flag);
        let overlay = Arc::new(overlay);
        activate(&overlay);
        assert!(ptr::eq(block_table(stone).unwrap(), diamond));
        assert!(ptr::eq(table("t:new").unwrap(), diamond));
        assert!(ptr::eq(
            block_table(block_id("minecraft:wall_torch")).unwrap(),
            torch
        ));
        assert!(ptr::eq(predicate_of("t:p"), flag));
        assert!(ptr::eq(predicate_of("minecraft:tool/can_shear"), flag));
        assert!(ptr::eq(
            table("minecraft:blocks/dirt").unwrap(),
            block_table(block_id("minecraft:dirt")).unwrap()
        ));
        activate(&Arc::new(LootOverlay::default()));
        assert!(ptr::eq(block_table(stone).unwrap(), static_stone));
        assert!(table("t:new").is_none());
        assert!(predicate("t:p").is_none());
    }

    fn predicate_of(name: &str) -> &'static Condition {
        predicate(name).unwrap()
    }
}
