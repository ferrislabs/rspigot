use crate::generated::enchantments::ENCHANTMENTS;
use crate::registries;
use crate::tags;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cost {
    pub base: i32,
    pub per_level_above_first: i32,
}

impl Cost {
    pub fn at(self, level: i32) -> i32 {
        self.base + self.per_level_above_first * (level - 1)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EnchantmentDef {
    pub name: &'static str,
    pub max_level: u8,
    pub min_cost: Cost,
    pub max_cost: Cost,
    pub anvil_cost: u8,
    pub weight: u8,
    pub exclusive_set: Option<&'static str>,
    pub supported_items: &'static str,
    pub primary_items: Option<&'static str>,
}

const REGISTRY: &str = "minecraft:enchantment";
const ITEM_REGISTRY: &str = "minecraft:item";
const CURSE_TAG: &str = "minecraft:curse";

pub fn all() -> &'static [EnchantmentDef] {
    &ENCHANTMENTS
}

pub fn id(name: &str) -> Option<u32> {
    registries::synced_index(REGISTRY, name)
}

pub fn by_name(name: &str) -> Option<&'static EnchantmentDef> {
    ENCHANTMENTS.get(id(name)? as usize)
}

impl EnchantmentDef {
    pub fn id(&self) -> u32 {
        id(self.name).unwrap_or(u32::MAX)
    }

    pub fn is_curse(&self) -> bool {
        tags::tag(REGISTRY, CURSE_TAG).is_some_and(|ids| ids.contains(&self.id()))
    }

    pub fn supports(&self, item: u16) -> bool {
        tags::tag(ITEM_REGISTRY, self.supported_items.trim_start_matches('#'))
            .is_some_and(|ids| ids.binary_search(&u32::from(item)).is_ok())
    }

    pub fn compatible_with(&self, other: &EnchantmentDef) -> bool {
        if self.name == other.name {
            return false;
        }
        let excludes = |from: &EnchantmentDef, to: &EnchantmentDef| {
            from.exclusive_set
                .and_then(|set| tags::tag(REGISTRY, set.trim_start_matches('#')))
                .is_some_and(|ids| ids.contains(&to.id()))
        };
        !excludes(self, other) && !excludes(other, self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(name: &str) -> u16 {
        registries::static_index("minecraft:item", name).unwrap() as u16
    }

    #[test]
    fn every_synced_enchantment_has_a_definition_in_registry_order() {
        assert_eq!(all().len(), 43);
        for (index, def) in all().iter().enumerate() {
            assert_eq!(id(def.name), Some(index as u32));
        }
    }

    #[test]
    fn sharpness_matches_the_datapack() {
        let sharpness = by_name("minecraft:sharpness").unwrap();
        assert_eq!(sharpness.max_level, 5);
        assert_eq!(sharpness.anvil_cost, 1);
        assert_eq!(sharpness.weight, 10);
        assert_eq!(sharpness.min_cost.at(3), 23);
        assert_eq!(sharpness.max_cost.at(1), 21);
        assert!(sharpness.supports(item("minecraft:diamond_sword")));
        assert!(!sharpness.supports(item("minecraft:diamond_pickaxe")));
    }

    #[test]
    fn exclusive_sets_and_curses() {
        let sharpness = by_name("minecraft:sharpness").unwrap();
        let smite = by_name("minecraft:smite").unwrap();
        let unbreaking = by_name("minecraft:unbreaking").unwrap();
        assert!(!sharpness.compatible_with(smite));
        assert!(sharpness.compatible_with(unbreaking));
        assert!(!sharpness.compatible_with(sharpness));
        assert!(by_name("minecraft:vanishing_curse").unwrap().is_curse());
        assert!(!sharpness.is_curse());
    }

    #[test]
    fn mending_is_a_book_and_tool_enchantment() {
        let mending = by_name("minecraft:mending").unwrap();
        assert_eq!(mending.anvil_cost, 4);
        assert_eq!(mending.max_level, 1);
        assert!(mending.supports(item("minecraft:diamond_pickaxe")));
    }
}
