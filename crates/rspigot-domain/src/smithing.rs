use std::num::NonZeroU8;
use std::sync::OnceLock;

use rspigot_items::{Craft, ItemKind, ItemSet};
use rspigot_nbt::Compound;

use crate::item::{ItemStack, Slot};
use crate::plugin_recipes::{self, Body};

pub const TEMPLATE: usize = 0;
pub const BASE: usize = 1;
pub const ADDITION: usize = 2;
pub const RESULT: usize = 3;
pub const INPUTS: usize = 3;

const TRIM: &str = "minecraft:trim";

#[derive(Debug, Clone, Copy)]
enum Recipe {
    Transform { result: ItemKind },
    Trim { pattern: &'static str },
}

#[derive(Debug, Clone, Copy)]
struct Entry {
    id: &'static str,
    recipe: Recipe,
    template: ItemSet,
    base: ItemSet,
    addition: ItemSet,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    NoRecipe,
    Impossible,
    Made(ItemStack),
}

fn entries() -> &'static [Entry] {
    static ENTRIES: OnceLock<Vec<Entry>> = OnceLock::new();
    ENTRIES.get_or_init(|| {
        ItemKind::ALL
            .into_iter()
            .flat_map(|owner| {
                owner.crafts().iter().filter_map(move |craft| match *craft {
                    Craft::SmithingTransform {
                        template,
                        base,
                        addition,
                        ..
                    } => Some(Entry {
                        id: craft.id(),
                        recipe: Recipe::Transform { result: owner },
                        template,
                        base,
                        addition,
                    }),
                    Craft::SmithingTrim {
                        template,
                        base,
                        addition,
                        pattern,
                        ..
                    } => Some(Entry {
                        id: craft.id(),
                        recipe: Recipe::Trim { pattern },
                        template,
                        base,
                        addition,
                    }),
                    _ => None,
                })
            })
            .collect()
    })
}

fn trim_of(material: &str, pattern: &str) -> Compound {
    Compound::new()
        .with("material", material)
        .with("pattern", pattern)
}

fn plugin_made(
    template: &ItemStack,
    base: &ItemStack,
    addition: &ItemStack,
    overriding: bool,
) -> Option<(&'static str, ItemStack)> {
    plugin_recipes::with(|recipes| {
        recipes
            .precedence(overriding)
            .find_map(|recipe| match &recipe.body {
                Body::Smithing {
                    template: wanted_template,
                    base: wanted_base,
                    addition: wanted_addition,
                } if wanted_template.contains(template.item)
                    && wanted_base.contains(base.item)
                    && wanted_addition.contains(addition.item) =>
                {
                    let mut components = base.components.clone();
                    for (name, value) in recipe.result.components.iter() {
                        components.insert(name.to_owned(), value.clone());
                    }
                    Some((
                        recipe.id,
                        ItemStack {
                            item: recipe.result.item,
                            count: recipe.result.count,
                            components,
                        },
                    ))
                }
                _ => None,
            })
    })
}

fn vanilla_entry(
    template: &ItemStack,
    base: &ItemStack,
    addition: &ItemStack,
) -> Option<&'static Entry> {
    entries().iter().find(|entry| {
        entry.template.contains(template.item)
            && entry.base.contains(base.item)
            && entry.addition.contains(addition.item)
    })
}

pub fn recipe_id(template: &Slot, base: &Slot, addition: &Slot) -> Option<&'static str> {
    let (Some(template), Some(base), Some(addition)) = (template, base, addition) else {
        return None;
    };
    plugin_made(template, base, addition, true)
        .map(|(id, _)| id)
        .or_else(|| vanilla_entry(template, base, addition).map(|entry| entry.id))
        .or_else(|| plugin_made(template, base, addition, false).map(|(id, _)| id))
}

pub fn craft(template: &Slot, base: &Slot, addition: &Slot) -> Outcome {
    let (Some(template), Some(base), Some(addition)) = (template, base, addition) else {
        return Outcome::NoRecipe;
    };
    if let Some((_, made)) = plugin_made(template, base, addition, true) {
        return Outcome::Made(made);
    }
    let Some(entry) = vanilla_entry(template, base, addition) else {
        return plugin_made(template, base, addition, false)
            .map_or(Outcome::NoRecipe, |(_, made)| Outcome::Made(made));
    };
    match entry.recipe {
        Recipe::Transform { result } => Outcome::Made(ItemStack {
            item: result.id(),
            count: NonZeroU8::MIN,
            components: base.components.clone(),
        }),
        Recipe::Trim { pattern } => {
            let Some(material) = addition.item.kind().and_then(ItemKind::trim_material) else {
                return Outcome::Impossible;
            };
            let trim = trim_of(material, pattern);
            if base.components.get_compound(TRIM) == Some(&trim) {
                return Outcome::Impossible;
            }
            let mut made = base.clone();
            made.count = NonZeroU8::MIN;
            made.components.insert(TRIM, trim);
            Outcome::Made(made)
        }
    }
}

pub fn result(template: &Slot, base: &Slot, addition: &Slot) -> Slot {
    match craft(template, base, addition) {
        Outcome::Made(stack) => Some(stack),
        Outcome::NoRecipe | Outcome::Impossible => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stack(kind: ItemKind) -> Slot {
        Some(ItemStack::new(kind.id(), NonZeroU8::MIN))
    }

    fn with(kind: ItemKind, key: &str, value: impl Into<rspigot_nbt::Tag>) -> Slot {
        let mut stack = ItemStack::new(kind.id(), NonZeroU8::MIN);
        stack.components.insert(key, value);
        Some(stack)
    }

    #[test]
    fn netherite_upgrades_a_diamond_tool_and_keeps_what_it_carried() {
        let mut pickaxe = ItemStack::new(ItemKind::DiamondPickaxe.id(), NonZeroU8::MIN);
        pickaxe.components.insert("minecraft:damage", 40);
        pickaxe.components.insert("minecraft:custom_name", "mine");
        let outcome = craft(
            &stack(ItemKind::NetheriteUpgradeSmithingTemplate),
            &Some(pickaxe),
            &stack(ItemKind::NetheriteIngot),
        );
        let Outcome::Made(upgraded) = outcome else {
            panic!("{outcome:?}");
        };
        assert_eq!(upgraded.item, ItemKind::NetheritePickaxe.id());
        assert_eq!(upgraded.count.get(), 1);
        assert_eq!(upgraded.components.get_i32("minecraft:damage"), Some(40));
        assert_eq!(
            upgraded.components.get_str("minecraft:custom_name"),
            Some("mine")
        );
    }

    #[test]
    fn every_diamond_item_has_its_netherite_twin() {
        for (base, made) in [
            (ItemKind::DiamondSword, ItemKind::NetheriteSword),
            (ItemKind::DiamondHelmet, ItemKind::NetheriteHelmet),
            (ItemKind::DiamondChestplate, ItemKind::NetheriteChestplate),
            (ItemKind::DiamondLeggings, ItemKind::NetheriteLeggings),
            (ItemKind::DiamondBoots, ItemKind::NetheriteBoots),
            (ItemKind::DiamondHoe, ItemKind::NetheriteHoe),
            (ItemKind::DiamondShovel, ItemKind::NetheriteShovel),
            (ItemKind::DiamondAxe, ItemKind::NetheriteAxe),
            (ItemKind::DiamondHorseArmor, ItemKind::NetheriteHorseArmor),
        ] {
            let outcome = result(
                &stack(ItemKind::NetheriteUpgradeSmithingTemplate),
                &stack(base),
                &stack(ItemKind::NetheriteIngot),
            );
            assert_eq!(outcome.map(|made| made.item), Some(made.id()), "{base:?}");
        }
    }

    #[test]
    fn a_trim_puts_the_pattern_and_the_material_on_the_armor() {
        let outcome = craft(
            &stack(ItemKind::BoltArmorTrimSmithingTemplate),
            &with(ItemKind::IronHelmet, "minecraft:damage", 3),
            &stack(ItemKind::CopperIngot),
        );
        let Outcome::Made(trimmed) = outcome else {
            panic!("{outcome:?}");
        };
        assert_eq!(trimmed.item, ItemKind::IronHelmet.id());
        let trim = trimmed.components.get_compound("minecraft:trim").unwrap();
        assert_eq!(trim.get_str("material"), Some("minecraft:copper"));
        assert_eq!(trim.get_str("pattern"), Some("minecraft:bolt"));
        assert_eq!(trimmed.components.get_i32("minecraft:damage"), Some(3));
    }

    #[test]
    fn the_same_trim_twice_is_a_recipe_with_nothing_to_make() {
        let trimmed = with(
            ItemKind::IronHelmet,
            "minecraft:trim",
            trim_of("minecraft:copper", "minecraft:bolt"),
        );
        let same = craft(
            &stack(ItemKind::BoltArmorTrimSmithingTemplate),
            &trimmed,
            &stack(ItemKind::CopperIngot),
        );
        assert_eq!(same, Outcome::Impossible);
        let other_material = craft(
            &stack(ItemKind::BoltArmorTrimSmithingTemplate),
            &trimmed,
            &stack(ItemKind::GoldIngot),
        );
        assert!(matches!(other_material, Outcome::Made(_)));
        let other_pattern = craft(
            &stack(ItemKind::CoastArmorTrimSmithingTemplate),
            &trimmed,
            &stack(ItemKind::CopperIngot),
        );
        assert!(matches!(other_pattern, Outcome::Made(_)));
    }

    #[test]
    fn a_trim_overwrites_the_old_one_and_never_touches_the_stack_count() {
        let mut helmets = ItemStack::new(ItemKind::IronHelmet.id(), NonZeroU8::new(1).unwrap());
        helmets.components.insert(
            "minecraft:trim",
            trim_of("minecraft:gold", "minecraft:wild"),
        );
        let outcome = result(
            &stack(ItemKind::RibArmorTrimSmithingTemplate),
            &Some(helmets),
            &stack(ItemKind::Emerald),
        )
        .unwrap();
        let trim = outcome.components.get_compound("minecraft:trim").unwrap();
        assert_eq!(trim.get_str("material"), Some("minecraft:emerald"));
        assert_eq!(trim.get_str("pattern"), Some("minecraft:rib"));
    }

    #[test]
    fn a_trim_makes_one_piece_even_from_a_stack() {
        let stack_of_two = Some(ItemStack::new(
            ItemKind::IronHelmet.id(),
            NonZeroU8::new(2).unwrap(),
        ));
        let made = result(
            &stack(ItemKind::BoltArmorTrimSmithingTemplate),
            &stack_of_two,
            &stack(ItemKind::CopperIngot),
        )
        .unwrap();
        assert_eq!(made.count.get(), 1);
    }

    #[test]
    fn every_trim_template_and_material_pair_works_on_armor() {
        let templates: Vec<_> = ItemKind::ALL
            .into_iter()
            .filter(|kind| kind.name().ends_with("_armor_trim_smithing_template"))
            .collect();
        assert_eq!(templates.len(), 18);
        let materials: Vec<_> = ItemKind::ALL
            .into_iter()
            .filter(|kind| kind.trim_material().is_some())
            .collect();
        assert_eq!(materials.len(), 11);
        for template in &templates {
            for material in &materials {
                let made = result(
                    &stack(*template),
                    &stack(ItemKind::DiamondChestplate),
                    &stack(*material),
                );
                assert!(made.is_some(), "{template:?} {material:?}");
            }
        }
    }

    #[test]
    fn nothing_is_made_unless_all_three_inputs_fit_one_recipe() {
        let template = stack(ItemKind::NetheriteUpgradeSmithingTemplate);
        let tool = stack(ItemKind::DiamondPickaxe);
        let ingot = stack(ItemKind::NetheriteIngot);
        assert_eq!(craft(&None, &tool, &ingot), Outcome::NoRecipe);
        assert_eq!(craft(&template, &None, &ingot), Outcome::NoRecipe);
        assert_eq!(craft(&template, &tool, &None), Outcome::NoRecipe);
        assert_eq!(
            craft(&template, &stack(ItemKind::IronPickaxe), &ingot),
            Outcome::NoRecipe
        );
        assert_eq!(
            craft(&template, &tool, &stack(ItemKind::Diamond)),
            Outcome::NoRecipe
        );
        assert_eq!(
            craft(
                &stack(ItemKind::BoltArmorTrimSmithingTemplate),
                &tool,
                &ingot
            ),
            Outcome::NoRecipe
        );
        assert_eq!(
            craft(&stack(ItemKind::Stick), &tool, &ingot),
            Outcome::NoRecipe
        );
    }

    fn plugin_upgrade(overriding: bool) {
        let (id, display_id) = plugin_recipes::intern_id("t:upgrade");
        let mut set = plugin_recipes::PluginRecipes::default();
        let mut result = ItemStack::new(ItemKind::Trident.id(), NonZeroU8::MIN);
        result.components.insert("minecraft:custom_name", "forged");
        set.add(plugin_recipes::PluginRecipe {
            id,
            display_id,
            owner: "test".to_owned(),
            body: Body::Smithing {
                template: plugin_recipes::intern_items(&[
                    ItemKind::NetheriteUpgradeSmithingTemplate.id(),
                ]),
                base: plugin_recipes::intern_items(&[
                    ItemKind::DiamondSword.id(),
                    ItemKind::Stick.id(),
                ]),
                addition: plugin_recipes::intern_items(&[ItemKind::NetheriteIngot.id()]),
            },
            result,
            group: String::new(),
            category: plugin_recipes::Category::Misc,
            override_vanilla: overriding,
            auto_discover: false,
        });
        plugin_recipes::activate(&std::sync::Arc::new(set));
    }

    #[test]
    fn a_plugin_smithing_recipe_keeps_the_base_components_and_loses_to_vanilla_unless_it_overrides()
    {
        plugin_upgrade(false);
        let template = stack(ItemKind::NetheriteUpgradeSmithingTemplate);
        let ingot = stack(ItemKind::NetheriteIngot);
        let mut stick = ItemStack::new(ItemKind::Stick.id(), NonZeroU8::MIN);
        stick.components.insert("minecraft:damage", 4);
        let made = result(&template, &Some(stick), &ingot).unwrap();
        assert_eq!(made.item, ItemKind::Trident.id());
        assert_eq!(made.components.get_i32("minecraft:damage"), Some(4));
        assert_eq!(
            made.components.get_str("minecraft:custom_name"),
            Some("forged")
        );
        assert_eq!(
            recipe_id(&template, &stack(ItemKind::Stick), &ingot),
            Some("t:upgrade")
        );
        let vanilla = result(&template, &stack(ItemKind::DiamondSword), &ingot).unwrap();
        assert_eq!(vanilla.item, ItemKind::NetheriteSword.id());
        plugin_upgrade(true);
        let overridden = result(&template, &stack(ItemKind::DiamondSword), &ingot).unwrap();
        assert_eq!(overridden.item, ItemKind::Trident.id());
        plugin_recipes::activate(&std::sync::Arc::default());
        assert_eq!(result(&template, &stack(ItemKind::Stick), &ingot), None);
    }
}
