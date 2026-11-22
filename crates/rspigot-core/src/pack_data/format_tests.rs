use rspigot_data::loot::{
    Condition, EntityTarget, EntryKind, FunctionKind, ListMode, NameTarget, Output,
};
use serde_json::{Value, json};

use super::{predicate_file, table};

fn parse_table(value: &Value) -> table::TableOutcome {
    table::parse_named(value, "t:table").unwrap()
}

#[test]
fn a_pool_and_an_entry_read_condition_and_modifier_objects() {
    let outcome = parse_table(&json!({
        "type": "minecraft:block",
        "pools": [{
            "rolls": 1,
            "condition": {"type": "minecraft:survives_explosion"},
            "entries": [{
                "type": "minecraft:item",
                "name": "minecraft:stone",
                "condition": "t:ref",
                "modifier": [
                    {"type": "minecraft:set_count", "count": 2},
                    {"type": "minecraft:explosion_decay"}
                ]
            }]
        }]
    }));
    assert!(outcome.skipped.is_empty(), "{:?}", outcome.skipped);
    let pool = &outcome.table.pools[0];
    assert_eq!(pool.condition, Some(Condition::SurvivesExplosion));
    let entry = &pool.entries[0];
    assert_eq!(entry.condition, Some(Condition::Reference("t:ref")));
    assert_eq!(entry.modifier.len(), 2);
}

#[test]
fn set_name_and_set_lore_keep_their_text_as_snbt() {
    let outcome = parse_table(&json!({
        "pools": [{
            "rolls": 1,
            "entries": [{
                "type": "minecraft:item",
                "name": "minecraft:stone",
                "modifier": [
                    {"type": "minecraft:set_name", "name": {"text": "Hi", "bold": true},
                     "target": "item_name", "entity": "this"},
                    {"type": "minecraft:set_lore", "lore": ["a", {"text": "b"}],
                     "mode": "insert", "offset": 1}
                ]
            }]
        }]
    }));
    let modifier = outcome.table.pools[0].entries[0].modifier;
    assert_eq!(
        modifier[0].kind,
        FunctionKind::SetName {
            name: r#"{"bold":1b,"text":"Hi"}"#,
            target: NameTarget::ItemName,
            entity: Some(EntityTarget::This)
        }
    );
    assert_eq!(
        modifier[1].kind,
        FunctionKind::SetLore {
            lines: &[r#""a""#, r#"{"text":"b"}"#],
            mode: ListMode::Insert { offset: 1 },
            entity: None
        }
    );
}

#[test]
fn a_sequence_function_flattens_and_keeps_its_condition() {
    let outcome = parse_table(&json!({
        "pools": [{
            "rolls": 1,
            "modifier": {
                "type": "minecraft:sequence",
                "condition": {"type": "minecraft:killed_by_player"},
                "functions": [
                    {"type": "minecraft:set_count", "count": 3},
                    {"type": "minecraft:explosion_decay"}
                ]
            },
            "entries": []
        }]
    }));
    let modifier = outcome.table.pools[0].modifier;
    assert_eq!(modifier.len(), 2);
    assert!(
        modifier
            .iter()
            .all(|f| f.condition == Some(Condition::KilledByPlayer))
    );
}

#[test]
fn an_inline_loot_table_entry_becomes_a_named_table() {
    let outcome = parse_table(&json!({
        "pools": [{
            "rolls": 1,
            "entries": [{
                "type": "minecraft:loot_table",
                "value": {"pools": [{"rolls": 1, "entries": [
                    {"type": "minecraft:item", "name": "minecraft:stone"}
                ]}]}
            }]
        }]
    }));
    assert!(outcome.skipped.is_empty(), "{:?}", outcome.skipped);
    let EntryKind::Leaf(leaf) = outcome.table.pools[0].entries[0].kind else {
        panic!()
    };
    let Output::Table(name) = leaf.output else {
        panic!()
    };
    assert_eq!(name, "t:table/inline0");
    assert_eq!(outcome.inline.len(), 1);
    assert_eq!(outcome.inline[0].0, name);
}

#[test]
fn a_26_3_predicate_file_parses_and_value_check_is_gone() {
    let condition = predicate_file(&json!({
        "type": "minecraft:entity_properties",
        "entity": "this",
        "predicate": {
            "minecraft:entity_type": "minecraft:cow",
            "minecraft:flags": {"is_baby": true}
        }
    }))
    .unwrap();
    assert!(matches!(condition, Condition::EntityProperties { .. }));
    assert!(
        predicate_file(&json!({"type": "minecraft:value_check", "value": 1, "range": 1})).is_err()
    );
    assert!(
        predicate_file(
            &json!({"type": "minecraft:float_value_check", "value": 1, "range": {"min": 0}})
        )
        .is_ok()
    );
}
