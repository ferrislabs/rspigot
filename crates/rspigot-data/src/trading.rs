use crate::generated::trading::{ENCHANTABILITY, ENCHANTMENTS, TRADE_SETS, TRADES};
use crate::registries;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TradeCost {
    pub item: &'static str,
    pub count: i32,
    pub potion: Option<&'static str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Require {
    Enchantments,
    StoredEnchantments,
    DyedColor,
    MapId,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Modifier {
    EnchantRandomly {
        options: &'static str,
        only_compatible: bool,
        additional_cost: bool,
    },
    EnchantWithLevels {
        options: &'static str,
        min: i32,
        max: i32,
        additional_cost: bool,
    },
    SetStewEffects(&'static [(&'static str, i32)]),
    SetRandomDyes {
        base: i32,
        trials: i32,
        chance: f32,
    },
    ExplorationMap {
        decoration: &'static str,
        destination: &'static str,
        search_radius: i32,
    },
    SetRandomPotion,
    SetPotion(&'static str),
    Require(Require),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Trade {
    pub name: &'static str,
    pub wants: TradeCost,
    pub additional_wants: Option<TradeCost>,
    pub gives: &'static str,
    pub gives_count: i32,
    pub max_uses: i32,
    pub xp: i32,
    pub reputation_discount: f32,
    pub variants: &'static [&'static str],
    pub modifiers: &'static [Modifier],
    pub double_price: Option<&'static str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TradeSet {
    pub name: &'static str,
    pub trades: &'static [u16],
    pub amount: i32,
    pub allow_duplicates: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EnchantmentDef {
    pub weight: i32,
    pub max_level: i32,
    pub min_base: i32,
    pub min_step: i32,
    pub max_base: i32,
    pub max_step: i32,
    pub supported: &'static str,
    pub primary: Option<&'static str>,
    pub exclusive: Option<&'static str>,
}

impl EnchantmentDef {
    pub fn min_cost(&self, level: i32) -> i32 {
        self.min_base + self.min_step * (level - 1)
    }

    pub fn max_cost(&self, level: i32) -> i32 {
        self.max_base + self.max_step * (level - 1)
    }
}

pub fn trade_set(name: &str) -> Option<&'static TradeSet> {
    TRADE_SETS
        .binary_search_by(|candidate| candidate.name.cmp(name))
        .ok()
        .map(|index| &TRADE_SETS[index])
}

pub fn trade(index: u16) -> Option<&'static Trade> {
    TRADES.get(usize::from(index))
}

pub fn trade_count() -> usize {
    TRADES.len()
}

pub fn enchantment(index: usize) -> Option<&'static EnchantmentDef> {
    ENCHANTMENTS.get(index)
}

pub fn enchantment_name(index: usize) -> Option<&'static str> {
    registries::synced_entries("minecraft:enchantment")?
        .get(index)
        .copied()
}

pub fn enchantment_count() -> usize {
    ENCHANTMENTS.len()
}

pub fn enchantability(item: u16) -> Option<u8> {
    ENCHANTABILITY
        .binary_search_by_key(&item, |(candidate, _)| *candidate)
        .ok()
        .map(|index| ENCHANTABILITY[index].1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named(name: &str) -> &'static Trade {
        (0..trade_count() as u16)
            .filter_map(trade)
            .find(|candidate| candidate.name == name)
            .unwrap()
    }

    #[test]
    fn every_profession_level_has_a_trade_set_and_the_trader_has_three() {
        for profession in [
            "armorer",
            "butcher",
            "cartographer",
            "cleric",
            "farmer",
            "fisherman",
            "fletcher",
            "leatherworker",
            "librarian",
            "mason",
            "shepherd",
            "toolsmith",
            "weaponsmith",
        ] {
            for level in 1..=5 {
                let set = trade_set(&format!("minecraft:{profession}/level_{level}")).unwrap();
                let expected = if (profession, level) == ("librarian", 5) {
                    3
                } else {
                    2
                };
                assert_eq!(set.amount, expected, "{profession} {level}");
                assert!(!set.allow_duplicates);
                assert!(!set.trades.is_empty());
            }
        }
        assert_eq!(
            trade_set("minecraft:wandering_trader/common")
                .unwrap()
                .amount,
            5
        );
        assert_eq!(
            trade_set("minecraft:wandering_trader/uncommon")
                .unwrap()
                .amount,
            2
        );
        assert_eq!(
            trade_set("minecraft:wandering_trader/buying")
                .unwrap()
                .amount,
            2
        );
        assert!(trade_set("minecraft:nitwit/level_1").is_none());
    }

    #[test]
    fn a_librarian_book_trade_costs_the_additional_component_only() {
        let book = named("minecraft:librarian/1/emerald_and_book_enchanted_book");
        assert_eq!(book.wants.item, "minecraft:emerald");
        assert_eq!(book.wants.count, 0);
        assert_eq!(book.additional_wants.unwrap().item, "minecraft:book");
        assert_eq!(book.gives, "minecraft:enchanted_book");
        assert_eq!(book.max_uses, 12);
        assert_eq!(book.double_price, Some("minecraft:double_trade_price"));
        assert_eq!(book.modifiers.len(), 2);
        assert_eq!(
            book.modifiers[1],
            Modifier::Require(Require::StoredEnchantments)
        );
    }

    #[test]
    fn defaults_follow_the_codec() {
        let dye = named("minecraft:wandering_trader/emerald_green_dye");
        assert_eq!(dye.wants.count, 1);
        assert_eq!(dye.gives_count, 3);
        assert_eq!(dye.xp, 1);
        let water = named("minecraft:wandering_trader/water_bottle_emerald");
        assert_eq!(water.wants.potion, Some("minecraft:water"));
        assert_eq!(water.max_uses, 2);
    }

    #[test]
    fn enchantments_carry_their_vanilla_costs_in_registry_order() {
        let sharpness = registries::synced_index("minecraft:enchantment", "minecraft:sharpness")
            .unwrap() as usize;
        let definition = enchantment(sharpness).unwrap();
        assert_eq!(enchantment_name(sharpness), Some("minecraft:sharpness"));
        assert_eq!(definition.weight, 10);
        assert_eq!(definition.max_level, 5);
        assert_eq!(definition.min_cost(1), 1);
        assert_eq!(definition.min_cost(5), 45);
        assert_eq!(definition.max_cost(5), 65);
        assert_eq!(definition.exclusive, Some("minecraft:exclusive_set/damage"));
        assert_eq!(enchantment_count(), 43);
    }

    #[test]
    fn items_report_their_enchantability() {
        let boots = registries::static_index("minecraft:item", "minecraft:iron_boots").unwrap();
        assert_eq!(enchantability(boots as u16), Some(9));
        let stone = registries::static_index("minecraft:item", "minecraft:stone").unwrap();
        assert_eq!(enchantability(stone as u16), None);
    }
}
