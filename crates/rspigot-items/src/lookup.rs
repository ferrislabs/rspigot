use rspigot_blocks::BlockId;

use crate::ItemKind;
use crate::kind::{BLOCKS, NAMES};

const NAMESPACE: &str = "minecraft:";

macro_rules! item_kinds {
    (count: $count:literal, kinds: [$($kind:ident),* $(,)?] $(,)?) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        #[repr(u16)]
        pub enum ItemKind {
            $($kind,)*
        }

        impl ItemKind {
            pub const ALL: [Self; $count] = [$(Self::$kind,)*];

            pub const fn name(self) -> &'static str {
                match self {
                    $(Self::$kind => <$crate::items::$kind as $crate::Item>::NAME,)*
                }
            }

            pub const fn id(self) -> $crate::ItemId {
                match self {
                    $(Self::$kind => <$crate::items::$kind as $crate::Item>::ID,)*
                }
            }

            pub const fn class(self) -> &'static str {
                match self {
                    $(Self::$kind => <$crate::items::$kind as $crate::Item>::CLASS,)*
                }
            }

            pub const fn max_stack_size(self) -> u8 {
                match self {
                    $(Self::$kind => <$crate::items::$kind as $crate::Item>::MAX_STACK_SIZE,)*
                }
            }

            pub const fn max_damage(self) -> Option<u32> {
                match self {
                    $(Self::$kind => <$crate::items::$kind as $crate::Item>::MAX_DAMAGE,)*
                }
            }

            pub const fn weapon_damage(self) -> Option<u32> {
                match self {
                    $(Self::$kind => <$crate::items::$kind as $crate::Item>::WEAPON_DAMAGE,)*
                }
            }

            pub const fn rarity(self) -> $crate::Rarity {
                match self {
                    $(Self::$kind => <$crate::items::$kind as $crate::Item>::RARITY,)*
                }
            }

            pub const fn block(self) -> Option<rspigot_blocks::BlockId> {
                match self {
                    $(Self::$kind => <$crate::items::$kind as $crate::Item>::BLOCK,)*
                }
            }

            pub const fn tool(self) -> Option<$crate::Tool> {
                match self {
                    $(Self::$kind => <$crate::items::$kind as $crate::Item>::TOOL,)*
                }
            }

            pub const fn food(self) -> Option<$crate::Food> {
                match self {
                    $(Self::$kind => <$crate::items::$kind as $crate::Item>::FOOD,)*
                }
            }

            pub const fn equippable(self) -> Option<$crate::EquipmentSlot> {
                match self {
                    $(Self::$kind => <$crate::items::$kind as $crate::Item>::EQUIPPABLE,)*
                }
            }

            pub const fn enchantable(self) -> Option<u32> {
                match self {
                    $(Self::$kind => <$crate::items::$kind as $crate::Item>::ENCHANTABLE,)*
                }
            }

            pub const fn repairable(self) -> Option<$crate::ItemSet> {
                match self {
                    $(Self::$kind => <$crate::items::$kind as $crate::Item>::REPAIRABLE,)*
                }
            }

            pub const fn damage_resistant(self) -> Option<&'static str> {
                match self {
                    $(Self::$kind => <$crate::items::$kind as $crate::Item>::DAMAGE_RESISTANT,)*
                }
            }

            pub const fn use_remainder(self) -> Option<$crate::ItemId> {
                match self {
                    $(Self::$kind => <$crate::items::$kind as $crate::Item>::USE_REMAINDER,)*
                }
            }

            pub const fn crafting_remainder(self) -> Option<$crate::ItemId> {
                match self {
                    $(Self::$kind => <$crate::items::$kind as $crate::Item>::CRAFTING_REMAINDER,)*
                }
            }

            pub const fn consumable(self) -> Option<$crate::Consumable> {
                match self {
                    $(Self::$kind => <$crate::items::$kind as $crate::Item>::CONSUMABLE,)*
                }
            }

            pub const fn potion_contents(self) -> Option<$crate::PotionContentsDefault> {
                match self {
                    $(Self::$kind => <$crate::items::$kind as $crate::Item>::POTION_CONTENTS,)*
                }
            }

            pub const fn suspicious_stew_effects(self) -> Option<&'static [$crate::StewEffect]> {
                match self {
                    $(Self::$kind => <$crate::items::$kind as $crate::Item>::SUSPICIOUS_STEW_EFFECTS,)*
                }
            }

            pub const fn cooking_fuel(self) -> Option<$crate::Fuel> {
                match self {
                    $(Self::$kind => <$crate::items::$kind as $crate::Item>::COOKING_FUEL,)*
                }
            }

            pub const fn trim_material(self) -> Option<&'static str> {
                match self {
                    $(Self::$kind => <$crate::items::$kind as $crate::Item>::TRIM_MATERIAL,)*
                }
            }

            pub const fn crafts(self) -> &'static [$crate::Craft] {
                match self {
                    $(Self::$kind => <$crate::items::$kind as $crate::Item>::CRAFTS,)*
                }
            }

            pub const fn attribute_modifiers(self) -> &'static [$crate::AttributeModifier] {
                match self {
                    $(Self::$kind => <$crate::items::$kind as $crate::Item>::ATTRIBUTE_MODIFIERS,)*
                }
            }

            pub fn placed_block(self) -> Option<rspigot_blocks::BlockKind> {
                match self {
                    $(Self::$kind => <<$crate::items::$kind as $crate::Item>::Behavior as $crate::ItemBehavior>::placed_block::<$crate::items::$kind>(),)*
                }
            }
        }

        const _: () = {
            let mut index = 0;
            while index < ItemKind::ALL.len() {
                let kind = ItemKind::ALL[index];
                assert!(kind.id().0 as usize == index);
                assert!(kind.max_stack_size() >= 1);
                index += 1;
            }
        };
    };
}

pub(crate) use item_kinds;

impl ItemKind {
    pub fn from_id(id: crate::ItemId) -> Option<Self> {
        Self::ALL.get(usize::from(id.0)).copied()
    }

    pub fn from_name(name: &str) -> Option<Self> {
        let path = name.strip_prefix(NAMESPACE).unwrap_or(name);
        NAMES
            .binary_search_by_key(&path, |&(candidate, _)| candidate)
            .ok()
            .map(|index| NAMES[index].1)
    }

    pub fn from_block(block: BlockId) -> Option<Self> {
        BLOCKS.get(usize::from(block.0)).copied().flatten()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_table_is_sorted_and_complete() {
        assert_eq!(NAMES.len(), ItemKind::ALL.len());
        assert!(NAMES.windows(2).all(|pair| pair[0].0 < pair[1].0));
        for (path, kind) in NAMES {
            assert_eq!(kind.name().strip_prefix(NAMESPACE), Some(path));
        }
    }

    #[test]
    fn block_table_points_back_to_its_items() {
        assert_eq!(BLOCKS.len(), rspigot_blocks::BlockKind::ALL.len());
        for (block, kind) in BLOCKS.iter().enumerate() {
            if let Some(kind) = kind {
                assert_eq!(kind.block().map(|id| usize::from(id.0)), Some(block));
            }
        }
        let placing = ItemKind::ALL
            .iter()
            .filter(|kind| kind.block().is_some())
            .count();
        assert_eq!(BLOCKS.iter().flatten().count(), placing);
    }
}
