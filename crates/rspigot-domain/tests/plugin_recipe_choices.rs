use std::num::NonZeroU8;
use std::sync::Arc;

use rspigot_domain::crafting;
use rspigot_domain::furnace;
use rspigot_domain::item::{ItemStack, Slot};
use rspigot_domain::plugin_recipes::{
    self, Body, Category, PluginRecipe, PluginRecipes, intern_id, intern_items,
};
use rspigot_domain::smithing::{self, Outcome};
use rspigot_items::{CookingKind, ItemKind};

fn stack(kind: ItemKind) -> ItemStack {
    ItemStack::new(kind.id(), NonZeroU8::MIN)
}

fn named(kind: ItemKind, name: &str) -> ItemStack {
    let mut item = stack(kind);
    item.components.insert("minecraft:custom_name", name);
    item
}

fn install(id: &str, body: Body, exact: Vec<Option<ItemStack>>, result: ItemStack) {
    let (id, display_id) = intern_id(id);
    let mut set = PluginRecipes::default();
    set.add(PluginRecipe {
        id,
        display_id,
        owner: "test".to_owned(),
        body,
        result,
        group: String::new(),
        category: Category::Misc,
        override_vanilla: false,
        auto_discover: false,
        exact,
    });
    plugin_recipes::activate(&Arc::new(set));
}

fn grid(cells: &[(usize, ItemStack)]) -> Vec<Slot> {
    let mut slots = vec![None; 9];
    for (at, item) in cells {
        slots[*at] = Some(item.clone());
    }
    slots
}

fn one(kind: ItemKind) -> rspigot_items::ItemSet {
    intern_items(&[kind.id()])
}

#[test]
fn an_exact_shapeless_ingredient_takes_only_the_item_with_those_components() {
    install(
        "t:exact_shapeless",
        Body::Shapeless {
            ingredients: vec![one(ItemKind::Stick), one(ItemKind::Diamond)],
        },
        vec![Some(named(ItemKind::Stick, "Magic")), None],
        stack(ItemKind::Emerald),
    );
    let diamond = stack(ItemKind::Diamond);

    let plain = grid(&[(0, stack(ItemKind::Stick)), (1, diamond.clone())]);
    assert_eq!(crafting::result(3, 3, &plain), None);
    let other = grid(&[(0, named(ItemKind::Stick, "Other")), (1, diamond.clone())]);
    assert_eq!(crafting::result(3, 3, &other), None);
    let magic = grid(&[(0, named(ItemKind::Stick, "Magic")), (1, diamond)]);
    assert_eq!(
        crafting::result(3, 3, &magic).map(|made| made.item),
        Some(ItemKind::Emerald.id())
    );
}

#[test]
fn an_exact_shaped_cell_is_checked_at_its_own_position() {
    install(
        "t:exact_shaped",
        Body::Shaped {
            width: 2,
            height: 1,
            mirror: false,
            cells: vec![Some(one(ItemKind::Stick)), Some(one(ItemKind::Diamond))],
        },
        vec![None, Some(named(ItemKind::Diamond, "Cut"))],
        stack(ItemKind::Emerald),
    );
    let wrong = grid(&[(0, stack(ItemKind::Stick)), (1, stack(ItemKind::Diamond))]);
    assert_eq!(crafting::result(3, 3, &wrong), None);
    let right = grid(&[
        (0, stack(ItemKind::Stick)),
        (1, named(ItemKind::Diamond, "Cut")),
    ]);
    assert!(crafting::result(3, 3, &right).is_some());
}

#[test]
fn a_transmute_recipe_turns_the_input_into_the_result_keeping_its_components() {
    install(
        "t:transmute",
        Body::Transmute {
            input: one(ItemKind::DiamondSword),
            material: one(ItemKind::Emerald),
        },
        Vec::new(),
        stack(ItemKind::NetheriteSword),
    );
    let mut sword = stack(ItemKind::DiamondSword);
    sword.components.insert("minecraft:damage", 12);
    for cells in [
        grid(&[(0, sword.clone()), (1, stack(ItemKind::Emerald))]),
        grid(&[(4, stack(ItemKind::Emerald)), (5, sword.clone())]),
    ] {
        let made = crafting::result(3, 3, &cells).expect("a transmuted sword");
        assert_eq!(made.item, ItemKind::NetheriteSword.id());
        assert_eq!(made.components.get_i32("minecraft:damage"), Some(12));
    }
    let lone = grid(&[(0, sword)]);
    assert_eq!(crafting::result(3, 3, &lone), None);
}

#[test]
fn a_plugin_smithing_trim_applies_the_pattern_and_the_material_of_the_addition() {
    install(
        "t:trim",
        Body::SmithingTrim {
            template: one(ItemKind::NetheriteUpgradeSmithingTemplate),
            base: one(ItemKind::DiamondChestplate),
            addition: one(ItemKind::Emerald),
            pattern: "minecraft:coast".to_owned(),
        },
        Vec::new(),
        stack(ItemKind::DiamondChestplate),
    );
    let template = Some(stack(ItemKind::NetheriteUpgradeSmithingTemplate));
    let base = Some(stack(ItemKind::DiamondChestplate));
    let addition = Some(stack(ItemKind::Emerald));

    let Outcome::Made(made) = smithing::craft(&template, &base, &addition) else {
        panic!("a trimmed chestplate");
    };
    assert_eq!(made.item, ItemKind::DiamondChestplate.id());
    let trim = made
        .components
        .get_compound("minecraft:trim")
        .expect("a trim");
    assert_eq!(trim.get_str("pattern"), Some("minecraft:coast"));
    assert_eq!(trim.get_str("material"), Some("minecraft:emerald"));
    assert_eq!(
        smithing::recipe_id(&template, &base, &addition),
        Some("t:trim")
    );
}

#[test]
fn an_exact_smithing_base_and_an_exact_cooking_input_need_their_components() {
    install(
        "t:exact_smith",
        Body::Smithing {
            template: one(ItemKind::NetheriteUpgradeSmithingTemplate),
            base: one(ItemKind::Stick),
            addition: one(ItemKind::NetheriteIngot),
        },
        vec![None, Some(named(ItemKind::Stick, "Rod")), None],
        stack(ItemKind::Trident),
    );
    let template = Some(stack(ItemKind::NetheriteUpgradeSmithingTemplate));
    let ingot = Some(stack(ItemKind::NetheriteIngot));
    assert_eq!(
        smithing::craft(&template, &Some(stack(ItemKind::Stick)), &ingot),
        Outcome::NoRecipe
    );
    assert!(matches!(
        smithing::craft(&template, &Some(named(ItemKind::Stick, "Rod")), &ingot),
        Outcome::Made(_)
    ));

    install(
        "t:exact_cook",
        Body::Cooking {
            kind: CookingKind::Smelting,
            ingredient: one(ItemKind::Stick),
            experience: 1.0,
            time: 40,
        },
        vec![Some(named(ItemKind::Stick, "Rod"))],
        stack(ItemKind::Diamond),
    );
    assert!(furnace::recipe_for(CookingKind::Smelting, &stack(ItemKind::Stick)).is_none());
    assert!(furnace::recipe_for(CookingKind::Smelting, &named(ItemKind::Stick, "Rod")).is_some());
}
