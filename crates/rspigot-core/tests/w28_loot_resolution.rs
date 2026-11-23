use std::num::NonZeroU8;

use rand::SeedableRng;
use rand::rngs::StdRng;
use rspigot_core::loot::{self, LootContext, LootEntity};
use rspigot_core::pack_data::parse_functions;
use rspigot_domain::item::ItemStack;
use rspigot_entities::EntityKind;
use rspigot_items::ItemKind;
use rspigot_nbt::Tag;
use serde_json::json;

fn stick() -> ItemStack {
    ItemStack::new(ItemKind::Stick.id(), NonZeroU8::MIN)
}

fn rolled(functions: serde_json::Value, this: Option<&LootEntity>) -> ItemStack {
    let functions = parse_functions(&functions).unwrap();
    let context = LootContext {
        this,
        ..LootContext::default()
    };
    loot::modified_stack(functions, &stick(), &context, &mut StdRng::seed_from_u64(1))
        .expect("a stack")
}

fn alice() -> LootEntity {
    let mut entity = LootEntity::of(EntityKind::Player);
    entity.name = Some("Alice".to_owned());
    entity.scores = vec![("n".to_owned(), 7)];
    entity
}

const SCORE: &str = r#"{"score":{"name":"@s","objective":"n"}}"#;

#[test]
fn set_name_with_an_entity_resolves_a_score_and_a_selector() {
    let me = alice();
    let stack = rolled(
        json!({"type": "minecraft:set_name", "entity": "this",
               "name": {"score": {"name": "@s", "objective": "n"}}}),
        Some(&me),
    );
    assert_eq!(
        stack.components.get("minecraft:custom_name"),
        Some(&Tag::String("7".to_owned()))
    );
    let stack = rolled(
        json!({"type": "minecraft:set_name", "entity": "this",
               "name": {"selector": "@s"}}),
        Some(&me),
    );
    assert_eq!(
        stack.components.get("minecraft:custom_name"),
        Some(&Tag::String("Alice".to_owned()))
    );
}

#[test]
fn set_lore_with_an_entity_resolves_every_line() {
    let me = alice();
    let stack = rolled(
        json!({"type": "minecraft:set_lore", "entity": "this",
               "lore": [{"score": {"name": "@s", "objective": "n"}}, "plain"]}),
        Some(&me),
    );
    let lore = stack.components.get_list("minecraft:lore").unwrap();
    assert_eq!(lore[0], Tag::String("7".to_owned()));
    assert_eq!(lore[1], Tag::String("plain".to_owned()));
}

#[test]
fn without_an_entity_the_text_is_left_alone() {
    let me = alice();
    let stack = rolled(
        json!({"type": "minecraft:set_name", "name": {"score": {"name": "@s", "objective": "n"}}}),
        Some(&me),
    );
    assert!(
        stack
            .components
            .get("minecraft:custom_name")
            .is_some_and(|tag| tag != &Tag::String("7".to_owned())),
        "{SCORE}"
    );
}

#[test]
fn an_entity_target_that_is_absent_from_the_context_leaves_the_text_alone() {
    let stack = rolled(
        json!({"type": "minecraft:set_name", "entity": "attacker",
               "name": {"selector": "@s"}}),
        Some(&alice()),
    );
    assert!(
        stack
            .components
            .get("minecraft:custom_name")
            .is_some_and(|tag| tag != &Tag::String("Alice".to_owned()))
    );
}

#[test]
fn set_item_swaps_the_item_and_keeps_the_rest() {
    let stack = rolled(
        json!({"type": "minecraft:set_item", "item": "minecraft:diamond"}),
        None,
    );
    assert_eq!(stack.item, ItemKind::Diamond.id());
    assert_eq!(stack.count.get(), 1);
}

#[test]
fn set_loot_table_writes_the_container_loot_component() {
    let stack = rolled(
        json!({"type": "minecraft:set_loot_table", "loot_table_id": "minecraft:chests/igloo_chest", "seed": 42}),
        None,
    );
    let loot = stack
        .components
        .get_compound("minecraft:container_loot")
        .expect("a container loot component");
    assert_eq!(
        loot.get_str("loot_table"),
        Some("minecraft:chests/igloo_chest")
    );
    assert_eq!(loot.get_i64("seed"), Some(42));
}
