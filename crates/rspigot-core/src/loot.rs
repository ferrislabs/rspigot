use std::num::NonZeroU8;

use rand::Rng;
use rspigot_data::loot::{
    self, BlockPredicate, BonusFormula, Condition, DamageSourcePredicate, EnchantmentPredicate,
    EntityPredicate, EntitySubPredicate, EntityTarget, Entry, EntryKind, FloatProvider, Function,
    FunctionKind, IntProvider, ItemPredicate, Leaf, LootTable, Output, Pool,
};
use rspigot_domain::damage::DamageType;
use rspigot_domain::item::ItemStack;
use rspigot_domain::{BlockStateId, ItemId};
use rspigot_entities::EntityKind;
use rspigot_items::ItemKind;
use rspigot_nbt::{Compound, Tag};

const BLOCK_REGISTRY: &str = "minecraft:block";
const ITEM_REGISTRY: &str = "minecraft:item";
const ENTITY_REGISTRY: &str = "minecraft:entity_type";
const POTION_CONTENTS: &str = "minecraft:potion_contents";
const OMINOUS_BOTTLE_AMPLIFIER: &str = "minecraft:ominous_bottle_amplifier";
const MAX_OMINOUS_AMPLIFIER: i32 = 4;
const MAX_NESTING: usize = 32;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct LootEntity {
    pub kind: Option<EntityKind>,
    pub living: bool,
    pub on_fire: bool,
    pub baby: bool,
    pub mainhand: Option<ItemStack>,
    pub vehicle: Option<Box<LootEntity>>,
    pub components: Vec<(&'static str, String)>,
    pub sheared: Option<bool>,
    pub cube_size: Option<i32>,
    pub raider: Option<RaiderFacts>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RaiderFacts {
    pub has_raid: bool,
    pub captain: bool,
}

impl LootEntity {
    pub fn of(kind: EntityKind) -> Self {
        Self {
            kind: Some(kind),
            living: kind.living(),
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LootDamage {
    pub kind: DamageType,
    pub source: Option<LootEntity>,
    pub direct: Option<LootEntity>,
    pub is_direct: bool,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct LootContext<'a> {
    pub block: Option<BlockStateId>,
    pub tool: Option<&'a ItemStack>,
    pub this: Option<&'a LootEntity>,
    pub damage: Option<&'a LootDamage>,
    pub player: Option<&'a LootEntity>,
    pub luck: f32,
    pub explosion_radius: Option<f32>,
}

impl<'a> LootContext<'a> {
    pub fn block(
        state: BlockStateId,
        tool: Option<&'a ItemStack>,
        breaker: Option<&'a LootEntity>,
    ) -> Self {
        Self {
            block: Some(state),
            tool,
            this: breaker,
            ..Self::default()
        }
    }

    fn target(&self, target: EntityTarget) -> Option<&'a LootEntity> {
        match target {
            EntityTarget::This => self.this,
            EntityTarget::Attacker => self.damage.and_then(|d| d.source.as_ref()),
            EntityTarget::DirectAttacker => self.damage.and_then(|d| d.direct.as_ref()),
            EntityTarget::AttackingPlayer => self.player,
            EntityTarget::TargetEntity | EntityTarget::InteractingEntity => None,
        }
    }

    fn attacker(&self) -> Option<&'a LootEntity> {
        self.target(EntityTarget::Attacker)
    }
}

pub fn tool_is_correct(tool: Option<&ItemStack>, state: BlockStateId) -> bool {
    let Some(kind) = state.kind() else {
        return true;
    };
    !kind.settings().requires_correct_tool
        || tool
            .and_then(|stack| stack.item.kind())
            .and_then(ItemKind::tool)
            .is_some_and(|rules| rules.is_correct_for_drops(kind.id()))
}

pub fn block_drops<R: Rng + ?Sized>(context: &LootContext<'_>, rng: &mut R) -> Vec<ItemStack> {
    context
        .block
        .and_then(BlockStateId::kind)
        .and_then(|kind| loot::block_table(kind.id().0))
        .map_or_else(Vec::new, |table| table_drops(table, context, rng))
}

pub fn table_drops<R: Rng + ?Sized>(
    table: &'static LootTable,
    context: &LootContext<'_>,
    rng: &mut R,
) -> Vec<ItemStack> {
    let mut roller = Roller {
        context,
        rng,
        drops: Vec::new(),
        visiting: Vec::new(),
    };
    roller.table(table, &[]);
    roller.drops
}

const ENCHANTMENTS: &str = "minecraft:enchantments";

fn held_enchantments(stack: Option<&ItemStack>) -> Vec<(String, i32)> {
    let Some(compound) = stack.and_then(|stack| stack.components.get_compound(ENCHANTMENTS)) else {
        return Vec::new();
    };
    let levels = compound.get_compound("levels").unwrap_or(compound);
    levels
        .iter()
        .filter_map(|(name, tag)| Some((name.to_owned(), i32::try_from(tag.as_i64()?).ok()?)))
        .collect()
}

fn enchantment_level(stack: Option<&ItemStack>, enchantment: &str) -> i32 {
    held_enchantments(stack)
        .iter()
        .find(|(name, _)| name == enchantment)
        .map_or(0, |(_, level)| *level)
}

fn entity_enchantment_level(entity: Option<&LootEntity>, enchantment: &str) -> i32 {
    entity
        .filter(|e| e.living)
        .map_or(0, |e| enchantment_level(e.mainhand.as_ref(), enchantment))
}

fn enchantment_matches(predicate: &EnchantmentPredicate, held: &[(String, i32)]) -> bool {
    match (predicate.enchantments, predicate.levels) {
        (Some(names), levels) => names.iter().any(|name| {
            held.iter().any(|(held_name, level)| {
                held_name.as_str() == *name
                    && *level != 0
                    && levels.is_none_or(|range| range.contains(*level))
            })
        }),
        (None, Some(range)) => held.iter().any(|(_, level)| range.contains(*level)),
        (None, None) => !held.is_empty(),
    }
}

fn item_matches(predicate: &ItemPredicate, stack: Option<&ItemStack>) -> bool {
    let item = stack.map_or(ItemId::AIR, |stack| stack.item);
    predicate
        .items
        .is_none_or(|items| items.contains(ITEM_REGISTRY, u32::from(item.0)))
        && predicate
            .enchantments
            .iter()
            .all(|enchantment| enchantment_matches(enchantment, &held_enchantments(stack)))
}

fn block_matches(predicate: &BlockPredicate, state: Option<BlockStateId>) -> bool {
    let Some((state, kind)) = state.and_then(|state| Some((state, state.kind()?))) else {
        return false;
    };
    predicate
        .blocks
        .is_none_or(|blocks| blocks.contains(BLOCK_REGISTRY, u32::from(kind.id().0)))
        && predicate
            .state
            .iter()
            .all(|(name, value)| state.property(name) == Some(*value))
}

fn sub_matches(predicate: &EntitySubPredicate, entity: &LootEntity) -> bool {
    match *predicate {
        EntitySubPredicate::Sheep { sheared } => entity
            .sheared
            .is_some_and(|value| sheared.is_none_or(|wanted| wanted == value)),
        EntitySubPredicate::CubeMob { size } => {
            entity.cube_size.is_some_and(|value| size.contains(value))
        }
        EntitySubPredicate::Raider { has_raid, captain } => entity.raider.is_some_and(|raider| {
            raider.has_raid == has_raid.unwrap_or(false)
                && raider.captain == captain.unwrap_or(false)
        }),
    }
}

fn entity_matches(predicate: &EntityPredicate, entity: Option<&LootEntity>) -> bool {
    let Some(entity) = entity else {
        return false;
    };
    let kind_matches = predicate.kind.is_none_or(|kinds| {
        entity
            .kind
            .is_some_and(|kind| kinds.contains(ENTITY_REGISTRY, u32::from(kind.id().0)))
    });
    let flags = predicate.flags;
    kind_matches
        && flags.on_fire.is_none_or(|wanted| wanted == entity.on_fire)
        && flags
            .baby
            .is_none_or(|wanted| !entity.living || wanted == entity.baby)
        && predicate
            .mainhand
            .is_none_or(|item| entity.living && item_matches(&item, entity.mainhand.as_ref()))
        && predicate.sub.iter().all(|sub| sub_matches(sub, entity))
        && predicate
            .vehicle
            .is_none_or(|vehicle| entity_matches(vehicle, entity.vehicle.as_deref()))
        && predicate.components.iter().all(|(component, value)| {
            entity
                .components
                .iter()
                .any(|(name, held)| name == component && held == value)
        })
}

fn damage_matches(predicate: &DamageSourcePredicate, damage: Option<&LootDamage>) -> bool {
    let Some(damage) = damage else {
        return false;
    };
    predicate
        .tags
        .iter()
        .all(|tag| damage.kind.in_tag(tag.tag) == tag.expected)
        && predicate
            .direct_entity
            .is_none_or(|entity| entity_matches(entity, damage.direct.as_ref()))
        && predicate
            .source_entity
            .is_none_or(|entity| entity_matches(entity, damage.source.as_ref()))
        && predicate
            .is_direct
            .is_none_or(|wanted| wanted == damage.is_direct)
}

fn leaf_weight(leaf: &Leaf, luck: f32) -> i32 {
    (leaf.weight as f32 + leaf.quality as f32 * luck)
        .floor()
        .max(0.0) as i32
}

#[derive(Debug, Clone, PartialEq)]
struct Working {
    item: u16,
    count: i32,
    components: Compound,
}

impl Working {
    fn new(item: u16) -> Self {
        Self {
            item,
            count: 1,
            components: Compound::new(),
        }
    }

    fn is_empty(&self) -> bool {
        self.item == ItemId::AIR.0 || self.count <= 0
    }

    fn count(&self) -> i32 {
        if self.is_empty() { 0 } else { self.count }
    }
}

#[derive(Clone, Copy)]
enum Produce {
    Item(u16),
    Items(&'static [u32]),
    Table(&'static LootTable),
    Nothing,
}

struct Candidate {
    produce: Produce,
    weight: i32,
    modifiers: Vec<&'static [Function]>,
}

struct Roller<'r, 'a, R: ?Sized> {
    context: &'r LootContext<'a>,
    rng: &'r mut R,
    drops: Vec<ItemStack>,
    visiting: Vec<*const LootTable>,
}

impl<R: Rng + ?Sized> Roller<'_, '_, R> {
    fn table(&mut self, table: &'static LootTable, after: &[&'static [Function]]) {
        let marker = std::ptr::from_ref(table);
        if self.visiting.contains(&marker) || self.visiting.len() >= MAX_NESTING {
            tracing::warn!("Detected infinite loop in loot tables");
            return;
        }
        self.visiting.push(marker);
        let mut tail = Vec::with_capacity(after.len() + 2);
        tail.push(&[][..]);
        tail.push(table.modifier);
        tail.extend_from_slice(after);
        for pool in table.pools {
            tail[0] = pool.modifier;
            self.pool(pool, &tail);
        }
        self.visiting.pop();
    }

    fn pool(&mut self, pool: &Pool, tail: &[&'static [Function]]) {
        if !self.passes(pool.condition.as_ref()) {
            return;
        }
        let rolls = self.int(pool.rolls) + (pool.bonus_rolls * self.context.luck).floor() as i32;
        for _ in 0..rolls {
            self.roll(pool, tail);
        }
    }

    fn roll(&mut self, pool: &Pool, tail: &[&'static [Function]]) {
        let mut candidates = Vec::new();
        for entry in pool.entries {
            self.expand(entry, &[], &mut candidates);
        }
        let total: i32 = candidates.iter().map(|candidate| candidate.weight).sum();
        if total <= 0 {
            return;
        }
        let chosen = if candidates.len() == 1 {
            candidates.first()
        } else {
            let mut pick = self.rng.random_range(0..total);
            candidates.iter().find(|candidate| {
                pick -= candidate.weight;
                pick < 0
            })
        };
        let Some(candidate) = chosen else {
            return;
        };
        let mut chain = candidate.modifiers.clone();
        chain.extend_from_slice(tail);
        match candidate.produce {
            Produce::Item(item) => self.emit(Working::new(item), &chain),
            Produce::Items(ids) => {
                for id in ids {
                    if let Ok(item) = u16::try_from(*id) {
                        self.emit(Working::new(item), &chain);
                    }
                }
            }
            Produce::Table(table) => self.table(table, &chain),
            Produce::Nothing => {}
        }
    }

    fn expand(
        &mut self,
        entry: &'static Entry,
        outer: &[&'static [Function]],
        out: &mut Vec<Candidate>,
    ) -> bool {
        if !self.passes(entry.condition.as_ref()) {
            return false;
        }
        let mut chain = Vec::with_capacity(outer.len() + 1);
        chain.push(entry.modifier);
        chain.extend_from_slice(outer);
        match entry.kind {
            EntryKind::Leaf(leaf) => {
                self.leaf(&leaf, chain, out);
                true
            }
            EntryKind::Alternatives(children) => {
                children.iter().any(|child| self.expand(child, &chain, out))
            }
            EntryKind::Sequence(children) => {
                children.iter().all(|child| self.expand(child, &chain, out))
            }
            EntryKind::Group(children) => {
                for child in children {
                    self.expand(child, &chain, out);
                }
                true
            }
        }
    }

    fn leaf(&self, leaf: &Leaf, modifiers: Vec<&'static [Function]>, out: &mut Vec<Candidate>) {
        let weight = leaf_weight(leaf, self.context.luck);
        if weight <= 0 {
            return;
        }
        let produce = match leaf.output {
            Output::Item(item) => Produce::Item(item),
            Output::Items {
                items,
                expand: true,
            } => {
                out.extend(
                    items
                        .ids(ITEM_REGISTRY)
                        .iter()
                        .filter_map(|id| u16::try_from(*id).ok())
                        .map(|item| Candidate {
                            produce: Produce::Item(item),
                            weight,
                            modifiers: modifiers.clone(),
                        }),
                );
                return;
            }
            Output::Items {
                items,
                expand: false,
            } => Produce::Items(items.ids(ITEM_REGISTRY)),
            Output::Table(name) => loot::table(name).map_or(Produce::Nothing, Produce::Table),
            Output::Empty | Output::Dynamic(_) => Produce::Nothing,
        };
        out.push(Candidate {
            produce,
            weight,
            modifiers,
        });
    }

    fn emit(&mut self, stack: Working, chain: &[&'static [Function]]) {
        let mut stack = stack;
        for function in chain.iter().flat_map(|functions| functions.iter()) {
            stack = self.apply(function, stack);
        }
        self.split(stack);
    }

    fn split(&mut self, stack: Working) {
        if stack.is_empty() {
            return;
        }
        let max = i32::from(ItemId(stack.item).max_stack_size()).max(1);
        let mut remaining = stack.count;
        while remaining > 0 {
            let size = remaining.min(max);
            remaining -= size;
            if let Some(size) = u8::try_from(size).ok().and_then(NonZeroU8::new) {
                let mut out = ItemStack::new(ItemId(stack.item), size);
                out.components = stack.components.clone();
                self.drops.push(out);
            }
        }
    }

    fn apply(&mut self, function: &Function, stack: Working) -> Working {
        if !self.passes(function.condition.as_ref()) {
            return stack;
        }
        let mut stack = stack;
        let current = stack.count();
        match function.kind {
            FunctionKind::SetCount { count: amount, add } => {
                let base = if add { current } else { 0 };
                stack.count = base + self.int(amount);
            }
            FunctionKind::ApplyBonus {
                enchantment,
                formula,
            } => {
                let level = enchantment_level(self.context.tool, enchantment);
                stack.count = self.bonus(formula, current, level);
            }
            FunctionKind::LimitCount { min, max } => {
                let low = min.map(|provider| self.int(provider));
                let high = max.map(|provider| self.int(provider));
                let raised = low.map_or(current, |low| current.max(low));
                stack.count = high.map_or(raised, |high| raised.min(high));
            }
            FunctionKind::EnchantedCountIncrease {
                enchantment,
                count,
                limit,
            } => {
                let attacker = self.context.attacker().filter(|e| e.living);
                if attacker.is_some() {
                    let level = entity_enchantment_level(attacker, enchantment);
                    if level != 0 {
                        let grown = level as f32 * self.float(count);
                        stack.count = current + (grown + 0.5).floor() as i32;
                        if limit > 0 && stack.count() > limit {
                            stack.count = limit;
                        }
                    }
                }
            }
            FunctionKind::FurnaceSmelt { use_input_count } => {
                if !stack.is_empty() {
                    match loot::smelted(stack.item) {
                        Some(result) => {
                            let wanted = if use_input_count { current } else { 1 };
                            let max = i32::from(ItemId(result).max_stack_size());
                            stack.item = result;
                            stack.count = wanted.min(max);
                            stack.components = Compound::new();
                        }
                        None => tracing::warn!(
                            "Couldn't smelt item {} because there is no smelting recipe",
                            stack.item
                        ),
                    }
                }
            }
            FunctionKind::SetPotion(potion) => {
                let mut contents = match stack.components.remove(POTION_CONTENTS) {
                    Some(Tag::Compound(contents)) => contents,
                    _ => Compound::new(),
                };
                contents.insert("potion", potion);
                stack.components.insert(POTION_CONTENTS, contents);
            }
            FunctionKind::SetOminousBottleAmplifier(amplifier) => {
                let level = self.int(amplifier).clamp(0, MAX_OMINOUS_AMPLIFIER);
                stack.components.insert(OMINOUS_BOTTLE_AMPLIFIER, level);
            }
            FunctionKind::ExplosionDecay => {
                if let Some(radius) = self.context.explosion_radius {
                    let chance = 1.0 / radius;
                    let mut kept = 0;
                    for _ in 0..stack.count {
                        if self.rng.random::<f32>() <= chance {
                            kept += 1;
                        }
                    }
                    stack.count = kept;
                }
            }
            FunctionKind::Components(_) => {}
        }
        stack
    }

    fn bonus(&mut self, formula: BonusFormula, count: i32, level: i32) -> i32 {
        match formula {
            BonusFormula::OreDrops => {
                if level > 0 {
                    let extra = (self.rng.random_range(0..level + 2) - 1).max(0);
                    count * (extra + 1)
                } else {
                    count
                }
            }
            BonusFormula::UniformBonusCount { bonus_multiplier } => {
                let bound = bonus_multiplier * level;
                if bound > 0 {
                    count + self.rng.random_range(0..=bound)
                } else {
                    count
                }
            }
            BonusFormula::BinomialWithBonusCount { extra, probability } => {
                count + self.successes(level + extra, probability)
            }
        }
    }

    fn successes(&mut self, trials: i32, probability: f32) -> i32 {
        let mut successes = 0;
        for _ in 0..trials {
            if self.rng.random::<f32>() < probability {
                successes += 1;
            }
        }
        successes
    }

    fn int(&mut self, provider: IntProvider) -> i32 {
        match provider {
            IntProvider::Constant(value) => value,
            IntProvider::Uniform { min, max } => {
                if min >= max {
                    min
                } else {
                    self.rng.random_range(min..=max)
                }
            }
            IntProvider::Binomial { n, p } => self.successes(n, p),
        }
    }

    fn float(&mut self, provider: FloatProvider) -> f32 {
        match provider {
            FloatProvider::Constant(value) => value,
            FloatProvider::Uniform { min, max } => {
                if min >= max {
                    min
                } else {
                    self.rng.random::<f32>() * (max - min) + min
                }
            }
        }
    }

    fn passes(&mut self, condition: Option<&Condition>) -> bool {
        condition.is_none_or(|condition| self.test(condition))
    }

    fn test(&mut self, condition: &Condition) -> bool {
        match condition {
            Condition::SurvivesExplosion => match self.context.explosion_radius {
                Some(radius) => self.rng.random::<f32>() <= 1.0 / radius,
                None => true,
            },
            Condition::LocationCheck { .. } => true,
            Condition::MatchBlock(predicate) => block_matches(predicate, self.context.block),
            Condition::MatchTool(predicate) => item_matches(predicate, self.context.tool),
            Condition::EntityProperties { target, predicate } => predicate
                .is_none_or(|predicate| entity_matches(predicate, self.context.target(*target))),
            Condition::KilledByPlayer => self.context.player.is_some(),
            Condition::DamageSourceProperties(predicate) => {
                damage_matches(predicate, self.context.damage)
            }
            Condition::RandomChance(chance) => self.rng.random::<f32>() < *chance,
            Condition::RandomChanceWithEnchantedBonus {
                enchantment,
                unenchanted_chance,
                enchanted_chance,
            } => {
                let level = entity_enchantment_level(self.context.attacker(), enchantment);
                let chance = if level > 0 {
                    enchanted_chance.calculate(level)
                } else {
                    *unenchanted_chance
                };
                self.rng.random::<f32>() < chance
            }
            Condition::TableBonus {
                enchantment,
                chances,
            } => {
                let level = enchantment_level(self.context.tool, enchantment);
                let index = usize::try_from(level)
                    .unwrap_or(0)
                    .min(chances.len().saturating_sub(1));
                chances
                    .get(index)
                    .is_some_and(|chance| self.rng.random::<f32>() < *chance)
            }
            Condition::Inverted(term) => !self.test(term),
            Condition::AnyOf(terms) => terms.iter().any(|term| self.test(term)),
            Condition::AllOf(terms) => terms.iter().all(|term| self.test(term)),
            Condition::Reference(name) => {
                loot::predicate(name).is_some_and(|condition| self.test(condition))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::num::NonZeroU8;

    use rand::SeedableRng;
    use rand::rngs::StdRng;
    use rspigot_blocks::BlockKind;
    use rspigot_data::loot::{
        Condition, Entry, EntryKind, Function, FunctionKind, IntProvider, Leaf, Output, Pool,
    };
    use rspigot_data::tags::{self, HolderSet};
    use rspigot_domain::ItemId;
    use rspigot_domain::damage::DamageType;

    use super::*;

    const RUNS: u64 = 10_000;

    fn item(name: &str) -> u16 {
        ItemKind::from_name(name).unwrap().id().0
    }

    fn stack(name: &str) -> ItemStack {
        ItemStack::new(ItemId(item(name)), NonZeroU8::MIN)
    }

    fn kind(name: &str) -> BlockKind {
        BlockKind::from_name(name).unwrap()
    }

    fn block(name: &str) -> BlockStateId {
        kind(name).default_state()
    }

    fn state(name: &str, properties: &[(&str, &str)]) -> BlockStateId {
        kind(name).state_with(properties).unwrap()
    }

    fn summary(drops: &[ItemStack]) -> Vec<(&'static str, u8)> {
        drops
            .iter()
            .map(|stack| (stack.item.kind().unwrap().name(), stack.count.get()))
            .collect()
    }

    fn breaker() -> LootEntity {
        LootEntity::of(EntityKind::Player)
    }

    fn drops(state: BlockStateId, tool: Option<&ItemStack>, seed: u64) -> Vec<(&'static str, u8)> {
        let player = breaker();
        let context = LootContext::block(state, tool, Some(&player));
        summary(&block_drops(&context, &mut StdRng::seed_from_u64(seed)))
    }

    fn tally(state: BlockStateId, tool: Option<&ItemStack>) -> BTreeMap<(&'static str, u8), u32> {
        let mut rng = StdRng::seed_from_u64(7);
        let player = breaker();
        let context = LootContext::block(state, tool, Some(&player));
        let mut counts = BTreeMap::new();
        for _ in 0..RUNS {
            for drop in summary(&block_drops(&context, &mut rng)) {
                *counts.entry(drop).or_insert(0) += 1;
            }
        }
        counts
    }

    fn occurrences(counts: &BTreeMap<(&'static str, u8), u32>, name: &str) -> u32 {
        counts
            .iter()
            .filter(|((item, _), _)| *item == name)
            .map(|(_, count)| count)
            .sum()
    }

    fn leaf(output: Output, weight: i32) -> Entry {
        Entry {
            condition: None,
            modifier: &[],
            kind: EntryKind::Leaf(Leaf {
                weight,
                quality: 0,
                output,
            }),
        }
    }

    fn table(entries: Vec<Entry>, modifier: Vec<Function>) -> &'static LootTable {
        Box::leak(Box::new(LootTable {
            pools: Vec::leak(vec![Pool {
                rolls: IntProvider::Constant(1),
                bonus_rolls: 0.0,
                condition: None,
                modifier: Vec::leak(modifier),
                entries: Vec::leak(entries),
            }]),
            modifier: &[],
        }))
    }

    fn set_count(count: i32) -> Function {
        Function {
            condition: None,
            kind: FunctionKind::SetCount {
                count: IntProvider::Constant(count),
                add: false,
            },
        }
    }

    fn run(table: &'static LootTable, seed: u64) -> Vec<(&'static str, u8)> {
        let context = LootContext::block(block("minecraft:stone"), None, None);
        summary(&table_drops(
            table,
            &context,
            &mut StdRng::seed_from_u64(seed),
        ))
    }

    #[test]
    fn stone_with_stone_pickaxe_drops_cobblestone() {
        let pickaxe = stack("minecraft:stone_pickaxe");
        let stone = block("minecraft:stone");
        assert!(tool_is_correct(Some(&pickaxe), stone));
        assert_eq!(
            drops(stone, Some(&pickaxe), 1),
            [("minecraft:cobblestone", 1)]
        );
    }

    #[test]
    fn stone_by_hand_is_not_the_correct_tool() {
        assert!(!tool_is_correct(None, block("minecraft:stone")));
        assert!(!tool_is_correct(
            Some(&stack("minecraft:stick")),
            block("minecraft:stone")
        ));
        assert!(!tool_is_correct(
            Some(&stack("minecraft:iron_shovel")),
            block("minecraft:stone")
        ));
    }

    #[test]
    fn dirt_by_hand_drops_dirt() {
        let dirt = block("minecraft:dirt");
        assert!(tool_is_correct(None, dirt));
        assert_eq!(drops(dirt, None, 2), [("minecraft:dirt", 1)]);
    }

    #[test]
    fn diamond_ore_needs_iron_pickaxe() {
        let ore = block("minecraft:diamond_ore");
        let iron = stack("minecraft:iron_pickaxe");
        assert!(tool_is_correct(Some(&iron), ore));
        assert_eq!(drops(ore, Some(&iron), 3), [("minecraft:diamond", 1)]);
        assert!(!tool_is_correct(
            Some(&stack("minecraft:stone_pickaxe")),
            ore
        ));
        assert!(!tool_is_correct(None, ore));
    }

    #[test]
    fn oak_leaves_drop_saplings_sticks_and_apples() {
        let counts = tally(block("minecraft:oak_leaves"), None);
        assert!(counts.keys().all(|(name, count)| match *name {
            "minecraft:oak_sapling" | "minecraft:apple" => *count == 1,
            "minecraft:stick" => (1..=2).contains(count),
            _ => false,
        }));
        let saplings = occurrences(&counts, "minecraft:oak_sapling");
        let sticks = occurrences(&counts, "minecraft:stick");
        let apples = occurrences(&counts, "minecraft:apple");
        assert!((400..=600).contains(&saplings));
        assert!((130..=280).contains(&sticks));
        assert!((20..=90).contains(&apples));
        assert!(counts.contains_key(&("minecraft:stick", 1)));
        assert!(counts.contains_key(&("minecraft:stick", 2)));
    }

    #[test]
    fn oak_leaves_with_shears_drop_leaves() {
        let shears = stack("minecraft:shears");
        assert_eq!(
            drops(block("minecraft:oak_leaves"), Some(&shears), 4),
            [("minecraft:oak_leaves", 1)]
        );
    }

    #[test]
    fn double_oak_slab_drops_two_slabs() {
        let axe = stack("minecraft:wooden_axe");
        let double = state("minecraft:oak_slab", &[("type", "double")]);
        let bottom = state("minecraft:oak_slab", &[("type", "bottom")]);
        assert_eq!(drops(double, Some(&axe), 5), [("minecraft:oak_slab", 2)]);
        assert_eq!(drops(bottom, Some(&axe), 5), [("minecraft:oak_slab", 1)]);
    }

    #[test]
    fn ripe_wheat_drops_wheat_and_one_to_four_seeds() {
        let counts = tally(state("minecraft:wheat", &[("age", "7")]), None);
        assert_eq!(counts.get(&("minecraft:wheat", 1)), Some(&(RUNS as u32)));
        assert_eq!(occurrences(&counts, "minecraft:wheat_seeds"), RUNS as u32);
        for seeds in 1..=4 {
            assert!(counts.contains_key(&("minecraft:wheat_seeds", seeds)));
        }
        assert_eq!(counts.len(), 5);
    }

    #[test]
    fn young_wheat_drops_one_seed() {
        assert_eq!(
            drops(state("minecraft:wheat", &[("age", "0")]), None, 6),
            [("minecraft:wheat_seeds", 1)]
        );
    }

    #[test]
    fn glass_by_hand_drops_nothing() {
        let glass = block("minecraft:glass");
        assert!(tool_is_correct(None, glass));
        assert!(drops(glass, None, 7).is_empty());
    }

    #[test]
    fn grass_block_drops_dirt() {
        assert_eq!(
            drops(block("minecraft:grass_block"), None, 8),
            [("minecraft:dirt", 1)]
        );
    }

    #[test]
    fn tall_grass_lower_half_drops_seeds_one_time_in_eight() {
        let lower = state("minecraft:tall_grass", &[("half", "lower")]);
        let counts = tally(lower, None);
        assert_eq!(counts.len(), 1);
        let seeds = counts[&("minecraft:wheat_seeds", 1)];
        assert!((1100..=1400).contains(&seeds));
        let shears = stack("minecraft:shears");
        assert_eq!(
            drops(lower, Some(&shears), 9),
            [("minecraft:short_grass", 2)]
        );
    }

    #[test]
    fn air_has_no_drops() {
        assert!(drops(BlockStateId::AIR, None, 10).is_empty());
        assert!(drops(block("minecraft:water"), None, 10).is_empty());
        assert!(drops(BlockStateId(u16::MAX), None, 10).is_empty());
    }

    #[test]
    fn dirt_does_not_need_a_tool() {
        assert!(tool_is_correct(None, block("minecraft:dirt")));
    }

    #[test]
    fn cobweb_drops_cobweb_with_shears_and_string_with_a_sword() {
        let cobweb = block("minecraft:cobweb");
        let shears = stack("minecraft:shears");
        let sword = stack("minecraft:iron_sword");
        assert!(tool_is_correct(Some(&shears), cobweb));
        assert!(tool_is_correct(Some(&sword), cobweb));
        assert!(!tool_is_correct(None, cobweb));
        assert_eq!(drops(cobweb, Some(&shears), 11), [("minecraft:cobweb", 1)]);
        assert_eq!(drops(cobweb, Some(&sword), 11), [("minecraft:string", 1)]);
    }

    #[test]
    fn wall_torch_drops_a_torch() {
        assert_eq!(
            drops(block("minecraft:wall_torch"), None, 12),
            [("minecraft:torch", 1)]
        );
    }

    #[test]
    fn mushroom_block_count_is_clamped_at_zero() {
        let counts = tally(block("minecraft:brown_mushroom_block"), None);
        assert!(
            counts
                .keys()
                .all(|(name, count)| *name == "minecraft:brown_mushroom" && *count <= 2)
        );
        let total = occurrences(&counts, "minecraft:brown_mushroom");
        assert!((2000..=2450).contains(&total));
    }

    #[test]
    fn snow_layers_drop_matching_snowballs() {
        assert_eq!(
            drops(state("minecraft:snow", &[("layers", "5")]), None, 13),
            [("minecraft:snowball", 5)]
        );
    }

    #[test]
    fn redstone_ore_drops_four_or_five() {
        let pickaxe = stack("minecraft:iron_pickaxe");
        let counts = tally(block("minecraft:redstone_ore"), Some(&pickaxe));
        assert_eq!(
            counts.keys().copied().collect::<Vec<_>>(),
            [("minecraft:redstone", 4), ("minecraft:redstone", 5)]
        );
    }

    #[test]
    fn stacks_above_max_size_are_split() {
        let pearls = table(
            vec![leaf(Output::Item(item("minecraft:ender_pearl")), 1)],
            vec![set_count(40)],
        );
        assert_eq!(
            run(pearls, 1),
            [
                ("minecraft:ender_pearl", 16),
                ("minecraft:ender_pearl", 16),
                ("minecraft:ender_pearl", 8)
            ]
        );
        let stone = table(
            vec![leaf(Output::Item(item("minecraft:stone")), 1)],
            vec![set_count(100)],
        );
        assert_eq!(
            run(stone, 1),
            [("minecraft:stone", 64), ("minecraft:stone", 36)]
        );
        let none = table(
            vec![leaf(Output::Item(item("minecraft:stone")), 1)],
            vec![set_count(-3)],
        );
        assert!(run(none, 1).is_empty());
    }

    #[test]
    fn weights_pick_entries_proportionally() {
        let weighted = table(
            vec![
                leaf(Output::Item(item("minecraft:stone")), 3),
                leaf(Output::Item(item("minecraft:dirt")), 1),
                leaf(Output::Item(item("minecraft:glass")), 0),
            ],
            vec![],
        );
        let mut stone = 0;
        for seed in 0..4000 {
            match run(weighted, seed).as_slice() {
                [("minecraft:stone", 1)] => stone += 1,
                [("minecraft:dirt", 1)] => {}
                other => panic!("{other:?}"),
            }
        }
        assert!((2800..=3200).contains(&stone));
    }

    #[test]
    fn empty_entries_compete_for_the_roll() {
        let half = table(
            vec![
                leaf(Output::Empty, 1),
                leaf(Output::Item(item("minecraft:stone")), 1),
            ],
            vec![],
        );
        let hits = (0..4000)
            .filter(|seed| !run(half, *seed).is_empty())
            .count();
        assert!((1800..=2200).contains(&hits));
    }

    #[test]
    fn composite_entries_follow_vanilla_expansion() {
        let never = Entry {
            condition: Some(Condition::Inverted(&Condition::SurvivesExplosion)),
            ..leaf(Output::Item(item("minecraft:glass")), 1)
        };
        let children: &'static [Entry] = Vec::leak(vec![
            leaf(Output::Item(item("minecraft:stone")), 1),
            never,
            leaf(Output::Item(item("minecraft:dirt")), 1),
        ]);
        let composite = |kind: EntryKind| Entry {
            condition: None,
            modifier: &[],
            kind,
        };
        let seen = |kind: EntryKind| {
            let table = table(vec![composite(kind)], vec![]);
            let mut names: Vec<&str> = (0..200)
                .flat_map(|seed| run(table, seed))
                .map(|(name, _)| name)
                .collect();
            names.sort_unstable();
            names.dedup();
            names
        };
        assert_eq!(seen(EntryKind::Alternatives(children)), ["minecraft:stone"]);
        assert_eq!(seen(EntryKind::Sequence(children)), ["minecraft:stone"]);
        assert_eq!(
            seen(EntryKind::Group(children)),
            ["minecraft:dirt", "minecraft:stone"]
        );
    }

    #[test]
    fn composite_modifiers_apply_after_child_modifiers() {
        let child = Entry {
            modifier: Vec::leak(vec![set_count(3)]),
            ..leaf(Output::Item(item("minecraft:stone")), 1)
        };
        let add_two = Function {
            condition: None,
            kind: FunctionKind::SetCount {
                count: IntProvider::Constant(2),
                add: true,
            },
        };
        let parent = Entry {
            condition: None,
            modifier: Vec::leak(vec![add_two]),
            kind: EntryKind::Alternatives(Vec::leak(vec![child])),
        };
        let limit = Function {
            condition: None,
            kind: FunctionKind::LimitCount {
                min: None,
                max: Some(IntProvider::Constant(4)),
            },
        };
        assert_eq!(
            run(table(vec![parent], vec![limit]), 1),
            [("minecraft:stone", 4)]
        );
    }

    #[test]
    fn tag_entries_drop_all_or_one_item() {
        let wool = HolderSet::Tag("minecraft:wool");
        let size = tags::tag("minecraft:item", "minecraft:wool").unwrap().len();
        let all = table(
            vec![leaf(
                Output::Items {
                    items: wool,
                    expand: false,
                },
                1,
            )],
            vec![],
        );
        assert_eq!(run(all, 1).len(), size);
        let one = table(
            vec![leaf(
                Output::Items {
                    items: wool,
                    expand: true,
                },
                1,
            )],
            vec![],
        );
        let picked = run(one, 1);
        assert_eq!(picked.len(), 1);
        assert!(picked[0].0.ends_with("_wool"));
    }

    fn entity_table(kind: EntityKind) -> &'static LootTable {
        loot::entity_table(kind.id().0).unwrap()
    }

    fn entity_drops(
        kind: EntityKind,
        this: &LootEntity,
        damage: &LootDamage,
        player: Option<&LootEntity>,
        seed: u64,
    ) -> Vec<(&'static str, u8)> {
        let context = LootContext {
            this: Some(this),
            damage: Some(damage),
            player,
            ..LootContext::default()
        };
        summary(&table_drops(
            entity_table(kind),
            &context,
            &mut StdRng::seed_from_u64(seed),
        ))
    }

    fn generic_kill() -> LootDamage {
        LootDamage {
            kind: DamageType::GenericKill,
            source: None,
            direct: None,
            is_direct: true,
        }
    }

    fn player_attack() -> LootDamage {
        let attacker = LootEntity::of(EntityKind::Player);
        LootDamage {
            kind: DamageType::PlayerAttack,
            source: Some(attacker.clone()),
            direct: Some(attacker),
            is_direct: true,
        }
    }

    fn entity_tally(
        kind: EntityKind,
        this: &LootEntity,
        damage: &LootDamage,
        player: Option<&LootEntity>,
    ) -> BTreeMap<(&'static str, u8), u32> {
        let mut rng = StdRng::seed_from_u64(99);
        let context = LootContext {
            this: Some(this),
            damage: Some(damage),
            player,
            ..LootContext::default()
        };
        let mut counts = BTreeMap::new();
        for _ in 0..RUNS {
            for drop in summary(&table_drops(entity_table(kind), &context, &mut rng)) {
                *counts.entry(drop).or_insert(0) += 1;
            }
        }
        counts
    }

    #[test]
    fn cow_drops_leather_and_beef_with_a_fixed_seed() {
        let cow = LootEntity::of(EntityKind::Cow);
        let first = entity_drops(EntityKind::Cow, &cow, &generic_kill(), None, 1);
        let again = entity_drops(EntityKind::Cow, &cow, &generic_kill(), None, 1);
        assert_eq!(first, again);
        assert!(first.iter().any(|(name, _)| *name == "minecraft:beef"));
        let counts = entity_tally(EntityKind::Cow, &cow, &generic_kill(), None);
        assert!(counts.keys().all(|(name, count)| match *name {
            "minecraft:leather" => (1..=2).contains(count),
            "minecraft:beef" => (1..=3).contains(count),
            _ => false,
        }));
        assert_eq!(occurrences(&counts, "minecraft:beef"), RUNS as u32);
        let leather = occurrences(&counts, "minecraft:leather");
        assert!((6_400..=6_900).contains(&leather), "{leather}");
    }

    #[test]
    fn burning_cow_drops_cooked_beef() {
        let mut cow = LootEntity::of(EntityKind::Cow);
        cow.on_fire = true;
        let counts = entity_tally(EntityKind::Cow, &cow, &generic_kill(), None);
        assert_eq!(occurrences(&counts, "minecraft:beef"), 0);
        assert_eq!(occurrences(&counts, "minecraft:cooked_beef"), RUNS as u32);
    }

    #[test]
    fn zombie_rare_drops_need_a_player_kill() {
        let zombie = LootEntity::of(EntityKind::Zombie);
        let player = LootEntity::of(EntityKind::Player);
        let by_command = entity_tally(EntityKind::Zombie, &zombie, &generic_kill(), None);
        assert!(
            by_command
                .keys()
                .all(|(name, _)| *name == "minecraft:rotten_flesh")
        );
        let by_player = entity_tally(EntityKind::Zombie, &zombie, &player_attack(), Some(&player));
        let rare = [
            "minecraft:iron_ingot",
            "minecraft:carrot",
            "minecraft:potato",
        ]
        .iter()
        .map(|name| occurrences(&by_player, name))
        .sum::<u32>();
        assert!((170..=340).contains(&rare), "{rare}");
        assert_eq!(
            occurrences(&by_player, "minecraft:music_disc_lava_chicken"),
            0
        );
    }

    #[test]
    fn sheep_drop_their_colour_unless_sheared() {
        let mut sheep = LootEntity::of(EntityKind::Sheep);
        sheep.components = vec![("minecraft:sheep/color", "lime".to_owned())];
        sheep.sheared = Some(false);
        let counts = entity_tally(EntityKind::Sheep, &sheep, &generic_kill(), None);
        assert_eq!(occurrences(&counts, "minecraft:lime_wool"), RUNS as u32);
        assert_eq!(occurrences(&counts, "minecraft:white_wool"), 0);
        assert!(occurrences(&counts, "minecraft:mutton") == RUNS as u32);
        sheep.sheared = Some(true);
        let sheared = entity_tally(EntityKind::Sheep, &sheep, &generic_kill(), None);
        assert_eq!(occurrences(&sheared, "minecraft:lime_wool"), 0);
    }

    #[test]
    fn slime_balls_depend_on_size_and_frog_killers() {
        let mut slime = LootEntity::of(EntityKind::Slime);
        slime.cube_size = Some(2);
        assert!(entity_tally(EntityKind::Slime, &slime, &generic_kill(), None).is_empty());
        slime.cube_size = Some(1);
        let frog = LootDamage {
            kind: DamageType::MobAttack,
            source: Some(LootEntity::of(EntityKind::Frog)),
            direct: Some(LootEntity::of(EntityKind::Frog)),
            is_direct: true,
        };
        let eaten = entity_tally(EntityKind::Slime, &slime, &frog, None);
        assert_eq!(
            eaten.get(&("minecraft:slime_ball", 1)),
            Some(&(RUNS as u32))
        );
    }

    #[test]
    fn creepers_killed_by_skeletons_drop_a_disc() {
        let creeper = LootEntity::of(EntityKind::Creeper);
        let skeleton = LootDamage {
            kind: DamageType::MobAttack,
            source: Some(LootEntity::of(EntityKind::Skeleton)),
            direct: Some(LootEntity::of(EntityKind::Skeleton)),
            is_direct: true,
        };
        let counts = entity_tally(EntityKind::Creeper, &creeper, &skeleton, None);
        let discs: u32 = counts
            .iter()
            .filter(|((name, _), _)| name.starts_with("minecraft:music_disc_"))
            .map(|(_, count)| count)
            .sum();
        assert_eq!(discs, RUNS as u32);
        let plain = entity_tally(EntityKind::Creeper, &creeper, &generic_kill(), None);
        assert!(plain.keys().all(|(name, _)| *name == "minecraft:gunpowder"));
    }

    #[test]
    fn potions_and_ominous_bottles_set_components() {
        let potion = Function {
            condition: None,
            kind: FunctionKind::SetPotion("minecraft:poison"),
        };
        let amplifier = Function {
            condition: None,
            kind: FunctionKind::SetOminousBottleAmplifier(IntProvider::Constant(9)),
        };
        let arrows = table(
            vec![leaf(Output::Item(item("minecraft:tipped_arrow")), 1)],
            vec![potion, amplifier],
        );
        let context = LootContext::default();
        let drops = table_drops(arrows, &context, &mut StdRng::seed_from_u64(3));
        let [stack] = drops.as_slice() else {
            panic!("{drops:?}")
        };
        assert_eq!(
            stack
                .components
                .get_compound(POTION_CONTENTS)
                .and_then(|c| c.get_str("potion")),
            Some("minecraft:poison")
        );
        assert_eq!(stack.components.get_i32(OMINOUS_BOTTLE_AMPLIFIER), Some(4));
    }

    #[test]
    fn nested_tables_run_inside_the_parent_context() {
        let guardian = LootEntity::of(EntityKind::Guardian);
        let player = LootEntity::of(EntityKind::Player);
        let counts = entity_tally(
            EntityKind::Guardian,
            &guardian,
            &player_attack(),
            Some(&player),
        );
        let nested: u32 = [
            "minecraft:salmon",
            "minecraft:tropical_fish",
            "minecraft:pufferfish",
        ]
        .iter()
        .map(|name| occurrences(&counts, name))
        .sum();
        assert!((50..=160).contains(&nested), "{nested}");
        let cod = occurrences(&counts, "minecraft:cod");
        assert!((3_800..=4_400).contains(&cod), "{cod}");
        assert!(occurrences(&counts, "minecraft:prismarine_shard") > 0);
    }

    #[test]
    fn killed_by_player_and_enchanted_chance_without_looting() {
        let condition = Condition::AllOf(Vec::leak(vec![
            Condition::KilledByPlayer,
            Condition::RandomChanceWithEnchantedBonus {
                enchantment: "minecraft:looting",
                unenchanted_chance: 1.0,
                enchanted_chance: rspigot_data::loot::LevelBasedValue::Constant(0.0),
            },
        ]));
        let gated = LootTable {
            pools: Vec::leak(vec![Pool {
                rolls: IntProvider::Constant(1),
                bonus_rolls: 0.0,
                condition: Some(condition),
                modifier: &[],
                entries: Vec::leak(vec![leaf(Output::Item(item("minecraft:stone")), 1)]),
            }]),
            modifier: &[],
        };
        let gated: &'static LootTable = Box::leak(Box::new(gated));
        let player = LootEntity::of(EntityKind::Player);
        let with = LootContext {
            player: Some(&player),
            ..LootContext::default()
        };
        assert_eq!(
            summary(&table_drops(gated, &with, &mut StdRng::seed_from_u64(1))),
            [("minecraft:stone", 1)]
        );
        assert!(
            table_drops(
                gated,
                &LootContext::default(),
                &mut StdRng::seed_from_u64(1)
            )
            .is_empty()
        );
    }
}
