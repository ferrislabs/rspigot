use std::num::NonZeroU8;

use rspigot_data::registries;
use rspigot_data::tags;
use rspigot_data::trading::{
    EnchantmentDef, Modifier, Require, Trade, TradeCost as TradeCostDef, TradeSet,
};
use rspigot_effects::EffectKind;
use rspigot_items::ItemKind;
use rspigot_nbt::{Compound, Tag};

use crate::dye::DYE_COLORS;
use crate::entity::trading::{ItemCost, MerchantOffer};
use crate::item::ItemStack;
use crate::random::RandomSource;

const ENCHANTMENT_REGISTRY: &str = "minecraft:enchantment";
const ITEM_REGISTRY: &str = "minecraft:item";
const POTION_REGISTRY: &str = "minecraft:potion";
const ENCHANTMENTS: &str = "minecraft:enchantments";
const STORED_ENCHANTMENTS: &str = "minecraft:stored_enchantments";
const ADDITIONAL_TRADE_COST: &str = "minecraft:additional_trade_cost";
const DYED_COLOR: &str = "minecraft:dyed_color";
const POTION_CONTENTS: &str = "minecraft:potion_contents";
const SUSPICIOUS_STEW_EFFECTS: &str = "minecraft:suspicious_stew_effects";
const MAP_ID: &str = "minecraft:map_id";
const TRADEABLE_POTIONS: &str = "minecraft:tradeable";
const RANDOM_ENCHANT_ROLL: i32 = 50;
const ENCHANTABILITY_SPREAD_DIVISOR: i32 = 4;
const ENCHANT_COST_JITTER: f32 = 0.15;
const TICKS_PER_SECOND: i32 = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TradeEnv<'a> {
    pub variant: Option<&'a str>,
}

fn item_kind(name: &str) -> Option<ItemKind> {
    ItemKind::from_name(name)
}

fn in_tag(registry: &str, tag: &str, id: u32) -> bool {
    tags::tag(registry, tag).is_some_and(|ids| ids.binary_search(&id).is_ok())
}

fn single(item: ItemKind) -> ItemStack {
    ItemStack::new(item.id(), NonZeroU8::MIN)
}

fn is_enchantable_book(stack: &ItemStack) -> bool {
    stack.item == ItemKind::Book.id()
}

fn enchantment_key(stack: &ItemStack) -> &'static str {
    if stack.item == ItemKind::EnchantedBook.id() {
        STORED_ENCHANTMENTS
    } else {
        ENCHANTMENTS
    }
}

fn levels_of(stack: &ItemStack, key: &str) -> Compound {
    match stack.components.get(key) {
        Some(Tag::Compound(levels)) => levels
            .get_compound("levels")
            .cloned()
            .unwrap_or_else(|| levels.clone()),
        _ => Compound::new(),
    }
}

pub fn enchant(stack: &mut ItemStack, name: &str, level: i32) {
    let key = enchantment_key(stack);
    let mut levels = levels_of(stack, key);
    let upgraded = levels.get_i32(name).map_or(level, |held| held.max(level));
    levels.insert(name, upgraded);
    stack.components.insert(key, levels);
}

fn count_enchantments(stack: &ItemStack, key: &str) -> usize {
    levels_of(stack, key).len()
}

fn definition(index: usize) -> Option<&'static EnchantmentDef> {
    rspigot_data::trading::enchantment(index)
}

fn supports(definition: &EnchantmentDef, stack: &ItemStack) -> bool {
    in_tag(ITEM_REGISTRY, definition.supported, u32::from(stack.item.0))
}

fn is_primary(definition: &EnchantmentDef, stack: &ItemStack) -> bool {
    supports(definition, stack)
        && definition
            .primary
            .is_none_or(|tag| in_tag(ITEM_REGISTRY, tag, u32::from(stack.item.0)))
}

fn are_compatible(first: usize, second: usize) -> bool {
    if first == second {
        return false;
    }
    let excludes = |owner: usize, other: usize| {
        definition(owner)
            .and_then(|definition| definition.exclusive)
            .is_some_and(|tag| in_tag(ENCHANTMENT_REGISTRY, tag, other as u32))
    };
    !excludes(first, second) && !excludes(second, first)
}

fn option_ids(tag: &str) -> Vec<usize> {
    tags::tag(ENCHANTMENT_REGISTRY, tag)
        .map(|ids| ids.iter().map(|id| *id as usize).collect())
        .unwrap_or_default()
}

fn random_between(random: &mut impl RandomSource, min: i32, max: i32) -> i32 {
    if min >= max {
        min
    } else {
        random.next_int(max - min + 1) + min
    }
}

fn weighted_index(random: &mut impl RandomSource, weights: &[i32]) -> Option<usize> {
    let total: i32 = weights.iter().sum();
    if total <= 0 {
        return None;
    }
    let mut roll = random.next_int(total);
    for (index, weight) in weights.iter().enumerate() {
        roll -= weight;
        if roll < 0 {
            return Some(index);
        }
    }
    None
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Pick {
    enchantment: usize,
    level: i32,
}

fn available_picks(value: i32, stack: &ItemStack, options: &[usize]) -> Vec<Pick> {
    let book = is_enchantable_book(stack);
    let mut picks = Vec::new();
    for &enchantment in options {
        let Some(definition) = definition(enchantment) else {
            continue;
        };
        if !(is_primary(definition, stack) || book) {
            continue;
        }
        let mut level = definition.max_level;
        while level >= 1 {
            if definition.min_cost(level) <= value && value <= definition.max_cost(level) {
                picks.push(Pick { enchantment, level });
                break;
            }
            level -= 1;
        }
    }
    picks
}

fn enchantability(stack: &ItemStack) -> Option<i32> {
    rspigot_data::trading::enchantability(stack.item.0).map(i32::from)
}

fn pick_weighted(random: &mut impl RandomSource, picks: &[Pick]) -> Option<Pick> {
    let weights: Vec<i32> = picks
        .iter()
        .map(|pick| definition(pick.enchantment).map_or(0, |definition| definition.weight))
        .collect();
    weighted_index(random, &weights).map(|index| picks[index])
}

fn select_enchantments(
    random: &mut impl RandomSource,
    stack: &ItemStack,
    cost: i32,
    options: &[usize],
) -> Vec<Pick> {
    let mut chosen = Vec::new();
    let Some(value) = enchantability(stack) else {
        return chosen;
    };
    let spread = value / ENCHANTABILITY_SPREAD_DIVISOR + 1;
    let first = random.next_int(spread);
    let second = random.next_int(spread);
    let mut cost = cost + first + 1 + second;
    let jitter = (random.next_float() + random.next_float()) - 1.0;
    let rounded = (cost as f32 + cost as f32 * jitter * ENCHANT_COST_JITTER + 0.5).floor();
    cost = (rounded as i32).max(1);
    let mut available = available_picks(cost, stack, options);
    if available.is_empty() {
        return chosen;
    }
    if let Some(pick) = pick_weighted(random, &available) {
        chosen.push(pick);
    }
    while random.next_int(RANDOM_ENCHANT_ROLL) <= cost {
        if let Some(last) = chosen.last().copied() {
            available.retain(|candidate| are_compatible(last.enchantment, candidate.enchantment));
        }
        if available.is_empty() {
            return chosen;
        }
        if let Some(pick) = pick_weighted(random, &available) {
            chosen.push(pick);
        }
        cost /= 2;
    }
    chosen
}

fn enchant_item(
    random: &mut impl RandomSource,
    stack: ItemStack,
    cost: i32,
    options: &[usize],
) -> ItemStack {
    let picks = select_enchantments(random, &stack, cost, options);
    let mut stack = if is_enchantable_book(&stack) {
        single(ItemKind::EnchantedBook)
    } else {
        stack
    };
    for pick in picks {
        if let Some(name) = rspigot_data::trading::enchantment_name(pick.enchantment) {
            enchant(&mut stack, name, pick.level);
        }
    }
    stack
}

fn enchant_randomly(
    random: &mut impl RandomSource,
    stack: ItemStack,
    options: &str,
    only_compatible: bool,
    additional_cost: bool,
) -> ItemStack {
    let check = !is_enchantable_book(&stack) && only_compatible;
    let candidates: Vec<usize> = option_ids(options)
        .into_iter()
        .filter(|&enchantment| {
            !check || definition(enchantment).is_some_and(|definition| supports(definition, &stack))
        })
        .collect();
    if candidates.is_empty() {
        return stack;
    }
    let chosen = candidates[random.next_int(candidates.len() as i32) as usize];
    let Some(definition) = definition(chosen) else {
        return stack;
    };
    let Some(name) = rspigot_data::trading::enchantment_name(chosen) else {
        return stack;
    };
    let level = random_between(random, 1, definition.max_level);
    let mut stack = if is_enchantable_book(&stack) {
        single(ItemKind::EnchantedBook)
    } else {
        stack
    };
    enchant(&mut stack, name, level);
    if additional_cost {
        let cost = random.next_int(level * 10 + 5) + 2 + level * 3;
        stack.components.insert(ADDITIONAL_TRADE_COST, cost);
    }
    stack
}

fn binomial(random: &mut impl RandomSource, trials: i32, chance: f32) -> i32 {
    (0..trials).filter(|_| random.next_float() < chance).count() as i32
}

pub fn apply_dyes(current: Option<i32>, dyes: &[usize]) -> i32 {
    let mut sums = [0i32; 3];
    let mut brightest = 0i32;
    let mut count = 0i32;
    let colors = current
        .into_iter()
        .chain(dyes.iter().map(|&dye| DYE_COLORS[dye].texture));
    for color in colors {
        let channels = [(color >> 16) & 0xFF, (color >> 8) & 0xFF, color & 0xFF];
        brightest += channels.into_iter().max().unwrap_or(0);
        for (sum, channel) in sums.iter_mut().zip(channels) {
            *sum += channel;
        }
        count += 1;
    }
    let average = sums.map(|sum| sum / count);
    let average_max = brightest as f32 / count as f32;
    let max_average = average.into_iter().max().unwrap_or(0) as f32;
    let [red, green, blue] =
        average.map(|channel| (channel as f32 * average_max / max_average).round() as i32);
    ((red & 0xFF) << 16) | ((green & 0xFF) << 8) | (blue & 0xFF)
}

fn set_random_dyes(
    random: &mut impl RandomSource,
    mut stack: ItemStack,
    base: i32,
    trials: i32,
    chance: f32,
) -> ItemStack {
    let number = base + binomial(random, trials, chance);
    if number < 1 {
        return stack;
    }
    let dyes: Vec<usize> = (0..number)
        .map(|_| random.next_int(DYE_COLORS.len() as i32) as usize)
        .collect();
    let current = stack.components.get_i32(DYED_COLOR);
    stack.count = NonZeroU8::MIN;
    stack
        .components
        .insert(DYED_COLOR, apply_dyes(current, &dyes));
    stack
}

fn set_stew_effects(
    random: &mut impl RandomSource,
    mut stack: ItemStack,
    effects: &[(&str, i32)],
) -> ItemStack {
    if stack.item != ItemKind::SuspiciousStew.id() || effects.is_empty() {
        return stack;
    }
    let (name, seconds) = effects[random.next_int(effects.len() as i32) as usize];
    let instant = EffectKind::from_name(name).is_some_and(EffectKind::instantaneous);
    let duration = if instant {
        seconds
    } else {
        seconds * TICKS_PER_SECOND
    };
    let entry = Compound::new().with("id", name).with("duration", duration);
    stack.components.insert(
        SUSPICIOUS_STEW_EFFECTS,
        Tag::List(vec![Tag::Compound(entry)]),
    );
    stack
}

fn with_potion(mut stack: ItemStack, potion: &str) -> ItemStack {
    let mut contents = match stack.components.remove(POTION_CONTENTS) {
        Some(Tag::Compound(contents)) => contents,
        _ => Compound::new(),
    };
    contents.insert("potion", potion);
    stack.components.insert(POTION_CONTENTS, contents);
    stack
}

fn set_random_potion(random: &mut impl RandomSource, stack: ItemStack) -> ItemStack {
    let Some(ids) = tags::tag(POTION_REGISTRY, TRADEABLE_POTIONS) else {
        return stack;
    };
    if ids.is_empty() {
        return stack;
    }
    let id = ids[random.next_int(ids.len() as i32) as usize];
    match registries::static_entries(POTION_REGISTRY).and_then(|names| names.get(id as usize)) {
        Some(name) => with_potion(stack, name),
        None => stack,
    }
}

fn passes(stack: &ItemStack, require: Require) -> bool {
    match require {
        Require::Enchantments => count_enchantments(stack, ENCHANTMENTS) > 0,
        Require::StoredEnchantments => count_enchantments(stack, STORED_ENCHANTMENTS) > 0,
        Require::DyedColor => stack.components.contains_key(DYED_COLOR),
        Require::MapId => stack.components.contains_key(MAP_ID),
    }
}

fn apply_modifier(
    random: &mut impl RandomSource,
    stack: Option<ItemStack>,
    modifier: &Modifier,
) -> Option<ItemStack> {
    let stack = stack?;
    Some(match *modifier {
        Modifier::EnchantRandomly {
            options,
            only_compatible,
            additional_cost,
        } => enchant_randomly(random, stack, options, only_compatible, additional_cost),
        Modifier::EnchantWithLevels {
            options,
            min,
            max,
            additional_cost,
        } => {
            let levels = random_between(random, min, max);
            let enchanted = enchant_item(random, stack, levels, &option_ids(options));
            let mut enchanted = enchanted;
            if additional_cost && levels > 0 {
                enchanted.components.insert(ADDITIONAL_TRADE_COST, levels);
            }
            enchanted
        }
        Modifier::SetStewEffects(effects) => set_stew_effects(random, stack, effects),
        Modifier::SetRandomDyes {
            base,
            trials,
            chance,
        } => set_random_dyes(random, stack, base, trials, chance),
        Modifier::ExplorationMap { .. } => stack,
        Modifier::SetRandomPotion => set_random_potion(random, stack),
        Modifier::SetPotion(potion) => with_potion(stack, potion),
        Modifier::Require(require) => {
            if passes(&stack, require) {
                stack
            } else {
                return None;
            }
        }
    })
}

fn cost_of(definition: &TradeCostDef, additional: i32) -> Option<ItemCost> {
    let item = item_kind(definition.item)?;
    let max = i32::from(item.id().max_stack_size());
    let count = (definition.count + additional).clamp(0, max);
    let mut cost = ItemCost::new(item.id(), count);
    if let Some(potion) = definition.potion {
        cost.components
            .insert(POTION_CONTENTS, Compound::new().with("potion", potion));
    }
    Some(cost)
}

fn doubles_price(trade: &Trade, stack: &ItemStack) -> bool {
    let Some(tag) = trade.double_price else {
        return false;
    };
    levels_of(stack, STORED_ENCHANTMENTS)
        .iter()
        .any(|(name, _)| {
            registries::synced_index(ENCHANTMENT_REGISTRY, name)
                .is_some_and(|index| in_tag(ENCHANTMENT_REGISTRY, tag, index))
        })
}

pub fn build_offer(
    trade: &Trade,
    env: &TradeEnv<'_>,
    random: &mut impl RandomSource,
) -> Option<MerchantOffer> {
    if !trade.variants.is_empty() {
        let variant = env.variant?;
        if !trade.variants.contains(&variant) {
            return None;
        }
    }
    let gives = item_kind(trade.gives)?;
    let count = u8::try_from(trade.gives_count)
        .ok()
        .and_then(NonZeroU8::new)?;
    let mut stack = Some(ItemStack::new(gives.id(), count));
    for modifier in trade.modifiers {
        stack = apply_modifier(random, stack, modifier);
    }
    let mut result = stack?;
    let mut additional = 0;
    if let Some(Tag::Int(extra)) = result.components.remove(ADDITIONAL_TRADE_COST) {
        additional += extra;
    }
    if doubles_price(trade, &result) {
        additional *= 2;
    }
    let cost_a = cost_of(&trade.wants, additional)?;
    if cost_a.count < 1 {
        return None;
    }
    let cost_b = match &trade.additional_wants {
        Some(definition) => {
            let cost = cost_of(definition, 0)?;
            if cost.count < 1 {
                return None;
            }
            Some(cost)
        }
        None => None,
    };
    Some(MerchantOffer::new(
        cost_a,
        cost_b,
        result,
        trade.max_uses.max(1),
        trade.xp.max(0),
        trade.reputation_discount.max(0.0),
    ))
}

pub fn offers_from_set(
    set: &TradeSet,
    env: &TradeEnv<'_>,
    random: &mut impl RandomSource,
) -> Vec<MerchantOffer> {
    let mut pool: Vec<u16> = set.trades.to_vec();
    let mut offers = Vec::new();
    let mut accepted = 0;
    while accepted < set.amount && !pool.is_empty() {
        let index = random.next_int(pool.len() as i32) as usize;
        let trade = rspigot_data::trading::trade(pool[index]);
        let offer = trade.and_then(|trade| build_offer(trade, env, random));
        if set.allow_duplicates {
            match offer {
                Some(offer) => {
                    offers.push(offer);
                    accepted += 1;
                }
                None => {
                    pool.remove(index);
                }
            }
        } else {
            pool.remove(index);
            if let Some(offer) = offer {
                offers.push(offer);
                accepted += 1;
            }
        }
    }
    offers
}

pub fn offers_for(
    set_name: &str,
    env: &TradeEnv<'_>,
    random: &mut impl RandomSource,
) -> Vec<MerchantOffer> {
    rspigot_data::trading::trade_set(set_name)
        .map(|set| offers_from_set(set, env, random))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::random::LegacyRandom;

    fn trade_named(name: &str) -> &'static Trade {
        (0..rspigot_data::trading::trade_count() as u16)
            .filter_map(rspigot_data::trading::trade)
            .find(|candidate| candidate.name == name)
            .unwrap()
    }

    fn env() -> TradeEnv<'static> {
        TradeEnv {
            variant: Some("minecraft:plains"),
        }
    }

    #[test]
    fn a_plain_trade_builds_its_offer_with_the_datapack_numbers() {
        let trade = trade_named("minecraft:farmer/1/wheat_emerald");
        let mut random = LegacyRandom::new(1);
        let offer = build_offer(trade, &env(), &mut random).unwrap();
        assert_eq!(offer.cost_a.item, ItemKind::Wheat.id());
        assert_eq!(offer.cost_a.count, 20);
        assert_eq!(offer.result.item, ItemKind::Emerald.id());
        assert_eq!(offer.max_uses, 16);
        assert_eq!(offer.xp, 2);
        assert!((offer.price_multiplier - 0.05).abs() < 1e-6);
        assert_eq!(offer.uses, 0);
        assert!(offer.reward_exp);
    }

    #[test]
    fn a_trade_with_a_variant_predicate_is_dropped_for_other_villager_types() {
        let trade = trade_named("minecraft:cartographer/2/emerald_and_compass_explorer_jungle_map");
        let mut random = LegacyRandom::new(1);
        assert!(build_offer(trade, &env(), &mut random).is_none());
    }

    #[test]
    fn exploration_maps_are_discarded_without_a_map_id() {
        let trade = trade_named("minecraft:cartographer/2/emerald_and_compass_explorer_jungle_map");
        let jungle = TradeEnv {
            variant: Some("minecraft:jungle"),
        };
        let mut random = LegacyRandom::new(1);
        assert!(build_offer(trade, &jungle, &mut random).is_none());
    }

    #[test]
    fn a_librarian_book_prices_the_enchantment_into_the_emerald_cost() {
        let trade = trade_named("minecraft:librarian/1/emerald_and_book_enchanted_book");
        let mut seen_double = false;
        for seed in 0..300 {
            let mut random = LegacyRandom::new(seed);
            let offer = build_offer(trade, &env(), &mut random).unwrap();
            assert_eq!(offer.result.item, ItemKind::EnchantedBook.id());
            assert!(!offer.result.components.contains_key(ADDITIONAL_TRADE_COST));
            let levels = levels_of(&offer.result, STORED_ENCHANTMENTS);
            assert_eq!(levels.len(), 1);
            let (name, level) = levels.iter().next().unwrap();
            let level = level.as_i64().unwrap() as i32;
            let index = registries::synced_index(ENCHANTMENT_REGISTRY, name).unwrap();
            let definition = definition(index as usize).unwrap();
            assert!((1..=definition.max_level).contains(&level));
            let doubled = in_tag(ENCHANTMENT_REGISTRY, "minecraft:double_trade_price", index);
            let base_low = 2 + level * 3;
            let base_high = base_low + level * 10 + 4;
            let (low, high) = if doubled {
                (base_low * 2, base_high * 2)
            } else {
                (base_low, base_high)
            };
            seen_double |= doubled;
            assert!(
                offer.cost_a.count >= low.min(64) && offer.cost_a.count <= high.min(64),
                "{name} {level} {} {low} {high}",
                offer.cost_a.count
            );
            assert_eq!(offer.cost_b.as_ref().unwrap().item, ItemKind::Book.id());
        }
        assert!(seen_double);
    }

    #[test]
    fn an_enchanted_iron_pickaxe_carries_enchantments_priced_by_the_levels_spent() {
        let trade = trade_named("minecraft:toolsmith/3/emerald_enchanted_iron_pickaxe");
        let mut enchanted = 0;
        for seed in 0..60 {
            let mut random = LegacyRandom::new(seed);
            if let Some(offer) = build_offer(trade, &env(), &mut random) {
                assert_eq!(offer.result.item, ItemKind::IronPickaxe.id());
                assert!(!offer.result.enchantments().is_empty());
                assert!(offer.cost_a.count >= trade.wants.count + 5);
                enchanted += 1;
            }
        }
        assert!(enchanted > 50);
    }

    #[test]
    fn leather_dyes_blend_one_to_three_random_dyes() {
        let trade = trade_named("minecraft:leatherworker/1/emerald_dyed_leather_chestplate");
        let mut colors = std::collections::BTreeSet::new();
        for seed in 0..40 {
            let mut random = LegacyRandom::new(seed);
            let offer = build_offer(trade, &env(), &mut random).unwrap();
            colors.insert(offer.result.components.get_i32(DYED_COLOR).unwrap());
        }
        assert!(colors.len() > 10);
        assert_eq!(apply_dyes(None, &[14]), 0xB02E26);
    }

    #[test]
    fn the_suspicious_stew_gets_one_effect_in_ticks() {
        let trade = trade_named("minecraft:farmer/4/emerald_suspicious_stew");
        let mut random = LegacyRandom::new(5);
        let offer = build_offer(trade, &env(), &mut random).unwrap();
        let list = offer
            .result
            .components
            .get_list(SUSPICIOUS_STEW_EFFECTS)
            .unwrap();
        assert_eq!(list.len(), 1);
        let entry = list[0].as_compound().unwrap();
        let duration = entry.get_i32("duration").unwrap();
        let name = entry.get_str("id").unwrap();
        let seconds = [
            ("minecraft:night_vision", 5),
            ("minecraft:jump_boost", 8),
            ("minecraft:weakness", 7),
            ("minecraft:blindness", 6),
            ("minecraft:poison", 14),
            ("minecraft:saturation", 7),
        ]
        .into_iter()
        .find(|(candidate, _)| *candidate == name)
        .unwrap()
        .1;
        let instant = EffectKind::from_name(name).unwrap().instantaneous();
        assert_eq!(duration, if instant { seconds } else { seconds * 20 });
    }

    #[test]
    fn the_water_bottle_trade_wants_a_potion_with_its_contents() {
        let trade = trade_named("minecraft:wandering_trader/water_bottle_emerald");
        let mut random = LegacyRandom::new(5);
        let offer = build_offer(trade, &env(), &mut random).unwrap();
        assert_eq!(offer.cost_a.item, ItemKind::Potion.id());
        assert_eq!(
            offer
                .cost_a
                .components
                .get_compound(POTION_CONTENTS)
                .and_then(|contents| contents.get_str("potion")),
            Some("minecraft:water")
        );
    }

    #[test]
    fn the_long_invisibility_potion_and_tipped_arrows_set_their_potion() {
        let mut random = LegacyRandom::new(2);
        let invisibility = build_offer(
            trade_named("minecraft:wandering_trader/emerald_long_invisibility_potion"),
            &env(),
            &mut random,
        )
        .unwrap();
        assert_eq!(
            invisibility
                .result
                .components
                .get_compound(POTION_CONTENTS)
                .and_then(|contents| contents.get_str("potion")),
            Some("minecraft:long_invisibility")
        );
        let arrow = build_offer(
            trade_named("minecraft:fletcher/5/arrow_and_emerald_tipped_arrow"),
            &env(),
            &mut random,
        )
        .unwrap();
        assert!(arrow.result.components.contains_key(POTION_CONTENTS));
    }

    #[test]
    fn a_set_without_duplicates_gives_distinct_offers_up_to_its_amount() {
        let set = rspigot_data::trading::trade_set("minecraft:farmer/level_1").unwrap();
        for seed in 0..30 {
            let mut random = LegacyRandom::new(seed);
            let offers = offers_from_set(set, &env(), &mut random);
            assert_eq!(offers.len(), 2);
            assert_ne!(
                (offers[0].cost_a.item, offers[0].result.item),
                (offers[1].cost_a.item, offers[1].result.item)
            );
        }
    }

    #[test]
    fn the_wandering_trader_sets_have_their_own_amounts() {
        let mut random = LegacyRandom::new(8);
        let common = offers_for("minecraft:wandering_trader/common", &env(), &mut random);
        assert_eq!(common.len(), 5);
        let buying = offers_for("minecraft:wandering_trader/buying", &env(), &mut random);
        assert_eq!(buying.len(), 2);
        assert!(offers_for("minecraft:nitwit/level_1", &env(), &mut random).is_empty());
    }

    #[test]
    fn enchanting_with_levels_follows_the_enchanting_table_selection() {
        let mut stack = single(ItemKind::DiamondSword);
        let options = option_ids("minecraft:on_traded_equipment");
        let mut random = LegacyRandom::new(11);
        let mut got_multiple = false;
        for _ in 0..200 {
            let enchanted = enchant_item(&mut random, stack.clone(), 30, &options);
            let names = enchanted.enchantments();
            assert!(!names.is_empty());
            got_multiple |= names.len() > 1;
            for (name, level) in &names {
                assert!(*level >= 1, "{name}");
            }
        }
        assert!(got_multiple);
        stack = single(ItemKind::Stone);
        assert!(
            enchant_item(&mut random, stack, 30, &options)
                .components
                .is_empty()
        );
    }

    #[test]
    fn exclusive_enchantments_never_come_together() {
        let options = option_ids("minecraft:on_traded_equipment");
        let sharpness =
            registries::synced_index(ENCHANTMENT_REGISTRY, "minecraft:sharpness").unwrap() as usize;
        let smite =
            registries::synced_index(ENCHANTMENT_REGISTRY, "minecraft:smite").unwrap() as usize;
        assert!(!are_compatible(sharpness, smite));
        assert!(!are_compatible(sharpness, sharpness));
        let mut random = LegacyRandom::new(3);
        for _ in 0..300 {
            let sword = enchant_item(&mut random, single(ItemKind::DiamondSword), 40, &options);
            let names: Vec<String> = sword
                .enchantments()
                .into_iter()
                .map(|(name, _)| name)
                .collect();
            let damage = [
                "minecraft:sharpness",
                "minecraft:smite",
                "minecraft:bane_of_arthropods",
            ]
            .iter()
            .filter(|name| names.iter().any(|held| held == **name))
            .count();
            assert!(damage <= 1, "{names:?}");
        }
    }
}
