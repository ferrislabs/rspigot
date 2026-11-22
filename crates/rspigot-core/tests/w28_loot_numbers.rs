use std::num::NonZeroU8;

use rand::SeedableRng;
use rand::rngs::StdRng;
use rspigot_core::loot::{self, LootContext, LootEntity, LootWorld};
use rspigot_core::pack_data::{condition, parse_functions};
use rspigot_domain::item::ItemStack;
use rspigot_entities::EntityKind;
use rspigot_items::ItemKind;
use rspigot_nbt::Compound;
use serde_json::{Value, json};

fn stick() -> ItemStack {
    ItemStack::new(ItemKind::Stick.id(), NonZeroU8::MIN)
}

const SHIFT: i32 = 20;

fn count_with(provider: Value, context: &LootContext<'_>) -> i32 {
    let shifted = json!({"type": "minecraft:add", "inputs": [provider, SHIFT]});
    let functions =
        parse_functions(&json!({"function": "minecraft:set_count", "count": shifted})).unwrap();
    let stack = loot::modified_stack(functions, &stick(), context, &mut StdRng::seed_from_u64(7));
    stack.map_or(0, |stack| i32::from(stack.count.get())) - SHIFT
}

fn count(provider: Value) -> i32 {
    count_with(provider, &LootContext::default())
}

fn float_is(provider: Value, expected: f64, context: &LootContext<'_>) -> bool {
    let check = condition(&json!({
        "condition": "minecraft:float_value_check",
        "value": provider,
        "range": {"min": expected - 0.001, "max": expected + 0.001}
    }))
    .unwrap();
    loot::condition_holds(&check, context, &mut StdRng::seed_from_u64(7))
}

fn k(value: i32) -> Value {
    json!({"type": "minecraft:constant", "value": value})
}

fn kf(value: f64) -> Value {
    json!({"type": "minecraft:constant", "value": value})
}

#[test]
fn aggregate_providers_fold_their_inputs() {
    let inputs = json!([k(2), k(3), k(7)]);
    for (kind, expected) in [("add", 12), ("mul", 42), ("avg", 4), ("min", 2), ("max", 7)] {
        let provider = json!({"type": format!("minecraft:{kind}"), "inputs": inputs});
        assert_eq!(count(provider), expected, "{kind}");
    }
}

#[test]
fn binary_providers_use_java_integer_semantics() {
    let binary = |kind: &str, left: i32, right: i32| {
        count(json!({"type": format!("minecraft:{kind}"), "left": k(left), "right": k(right)}))
    };
    assert_eq!(binary("sub", 9, 4), 5);
    assert_eq!(binary("div", 7, 2), 3);
    assert_eq!(binary("div", -7, 2), -3);
    assert_eq!(binary("floor_div", -7, 2), -4);
    assert_eq!(binary("mod", 7, 3), 1);
    assert_eq!(binary("mod", -7, 3), -1);
    assert_eq!(binary("floor_mod", -7, 3), 2);
}

#[test]
fn unary_and_power_providers_evaluate() {
    assert_eq!(count(json!({"type": "minecraft:abs", "input": k(-5)})), 5);
    assert_eq!(
        count(json!({"type": "minecraft:negate", "input": k(5)})),
        -5
    );
    assert_eq!(
        count(json!({"type": "minecraft:pow", "base": k(2), "exponent": k(5)})),
        32
    );
    assert_eq!(
        count(json!({"type": "minecraft:from_float", "input": kf(2.5)})),
        3
    );
}

#[test]
fn uniform_and_binomial_providers_take_nested_providers() {
    let uniform = json!({"type": "minecraft:uniform", "min": k(4), "max": {
        "type": "minecraft:add", "inputs": [k(2), k(2)]}});
    assert_eq!(count(uniform), 4);
    let binomial = json!({"type": "minecraft:binomial", "n": {
        "type": "minecraft:add", "inputs": [k(2), k(3)]}, "p": kf(1.0)});
    assert_eq!(count(binomial), 5);
}

#[test]
fn conditional_and_dispatcher_providers_pick_by_condition() {
    let weather =
        |raining: bool| json!({"condition": "minecraft:weather_check", "raining": raining});
    let clear = LootContext {
        weather: Some((false, false)),
        ..LootContext::default()
    };
    let conditional = json!({"type": "minecraft:conditional", "condition": weather(true),
        "on_true": k(9), "on_false": k(1)});
    assert_eq!(count_with(conditional, &clear), 1);
    let dispatcher = json!({"type": "minecraft:number_dispatcher",
        "cases": [{"condition": weather(true), "value": k(5)},
                  {"condition": weather(false), "value": k(6)}],
        "default": k(7)});
    assert_eq!(count_with(dispatcher.clone(), &clear), 6);
    assert_eq!(count_with(dispatcher, &LootContext::default()), 7);
}

#[test]
fn weighted_lists_draw_from_their_distribution() {
    let provider = json!({"type": "minecraft:weighted_list", "distribution": [
        {"data": k(3), "weight": 0}, {"data": k(8), "weight": 5}]});
    assert_eq!(count(provider), 8);
}

#[test]
fn score_providers_read_a_holder_or_fall_back() {
    let mut me = LootEntity::of(EntityKind::Player);
    me.scores = vec![("kills".to_owned(), 11)];
    let world = LootWorld {
        scores: vec![("Fixed".to_owned(), "kills".to_owned(), 4)],
        ..LootWorld::default()
    };
    let context = LootContext {
        this: Some(&me),
        world: Some(&world),
        ..LootContext::default()
    };
    let context_target = json!({"type": "minecraft:score",
        "target": {"type": "minecraft:context", "target": "this"},
        "score": "kills", "fallback": k(1)});
    assert_eq!(count_with(context_target, &context), 11);
    let fixed = json!({"type": "minecraft:score",
        "target": {"type": "minecraft:fixed", "name": "Fixed"},
        "score": "kills", "fallback": k(1)});
    assert_eq!(count_with(fixed, &context), 4);
    let missing = json!({"type": "minecraft:score",
        "target": {"type": "minecraft:fixed", "name": "Nobody"},
        "score": "kills", "fallback": k(2)});
    assert_eq!(count_with(missing, &context), 2);
}

#[test]
fn storage_providers_read_a_number_at_a_path_or_fall_back() {
    let data = Compound::new().with("a", Compound::new().with("n", 6));
    let world = LootWorld {
        storages: vec![("test:s", &data)],
        ..LootWorld::default()
    };
    let context = LootContext {
        world: Some(&world),
        ..LootContext::default()
    };
    let at = |path: &str| {
        json!({"type": "minecraft:storage", "storage": "test:s", "path": path,
               "fallback": k(3)})
    };
    assert_eq!(count_with(at("a.n"), &context), 6);
    assert_eq!(count_with(at("a.missing"), &context), 3);
}

#[test]
fn float_providers_follow_java_float_semantics() {
    let context = LootContext::default();
    let binary = |kind: &str, left: f64, right: f64| json!({"type": format!("minecraft:{kind}"), "left": kf(left), "right": kf(right)});
    assert!(float_is(binary("div", 7.0, 2.0), 3.5, &context));
    assert!(float_is(binary("mod", 7.5, 2.0), 1.5, &context));
    assert!(float_is(binary("sub", 7.5, 2.0), 5.5, &context));
    assert!(float_is(
        json!({"type": "minecraft:pow", "base": kf(2.0), "exponent": kf(0.5)}),
        std::f64::consts::SQRT_2,
        &context
    ));
    for (kind, input, expected) in [
        ("ceil", 1.2, 2.0),
        ("floor", -1.2, -2.0),
        ("round", 2.5, 3.0),
        ("truncate", -1.9, -1.0),
        ("sqrt", 9.0, 3.0),
        ("abs", -4.0, 4.0),
        ("negate", 4.0, -4.0),
        ("sin", 0.0, 0.0),
        ("cos", 0.0, 1.0),
    ] {
        let provider = json!({"type": format!("minecraft:{kind}"), "input": kf(input)});
        assert!(float_is(provider, expected, &context), "{kind}");
    }
    let inputs = json!([kf(3.0), kf(4.0)]);
    for (kind, expected) in [
        ("add", 7.0),
        ("mul", 12.0),
        ("avg", 3.5),
        ("min", 3.0),
        ("max", 4.0),
        ("length", 5.0),
    ] {
        let provider = json!({"type": format!("minecraft:{kind}"), "inputs": inputs});
        assert!(float_is(provider, expected, &context), "{kind}");
    }
}

#[test]
fn division_by_zero_in_floats_is_replaced_by_zero() {
    let provider = json!({"type": "minecraft:div", "left": kf(1.0), "right": kf(0.0)});
    assert!(float_is(provider, 0.0, &LootContext::default()));
}

#[test]
fn float_providers_convert_and_look_up_levels() {
    let context = LootContext {
        enchantment_level: 3,
        ..LootContext::default()
    };
    let level = json!({"type": "minecraft:enchantment_level",
        "amount": {"type": "minecraft:linear", "base": 1.0, "per_level_above_first": 0.5}});
    assert!(float_is(level, 2.0, &context));
    assert!(float_is(
        json!({"type": "minecraft:from_int", "input": k(4)}),
        4.0,
        &context
    ));
}

#[test]
fn environment_attribute_providers_read_the_clock_driven_value() {
    let world = LootWorld {
        clocks: vec![("minecraft:overworld", 6000)],
        ..LootWorld::default()
    };
    let context = LootContext {
        world: Some(&world),
        ..LootContext::default()
    };
    let provider = json!({"type": "minecraft:environment_attribute",
        "attribute": "minecraft:gameplay/sky_light_level"});
    assert!(float_is(provider.clone(), 15.0, &context));
    let night = LootWorld {
        clocks: vec![("minecraft:overworld", 18_000)],
        ..LootWorld::default()
    };
    let context = LootContext {
        world: Some(&night),
        ..LootContext::default()
    };
    assert!(float_is(provider, 4.0, &context));
}

#[test]
fn registry_entries_are_resolved_by_reference_and_cycles_fall_back_to_zero() {
    use rspigot_core::pack_data::{PackData, Registries};
    use rspigot_core::ports::{PackExtras, PackFile, PackRegistryFile};
    let entry = |registry: &str, id: &str, text: &str| PackRegistryFile {
        registry: registry.to_owned(),
        file: PackFile {
            id: rspigot_domain::Identifier::parse(id).unwrap(),
            text: text.to_owned(),
        },
    };
    let extras = PackExtras {
        name: "p".to_owned(),
        registries: vec![
            entry(
                "context_int_provider",
                "t:five",
                r#"{"type":"minecraft:constant","value":5}"#,
            ),
            entry("context_int_provider", "t:loop", r#""t:loop""#),
            entry("context_float_provider", "t:half", "0.5"),
        ],
        ..PackExtras::default()
    };
    PackData::build(&[])
        .with_registries(Registries::build(&[&extras]))
        .activate();
    assert_eq!(count(json!("t:five")), 5);
    let looping = parse_functions(
        &json!({"function": "minecraft:set_count", "count": "t:loop", "add": true}),
    )
    .unwrap();
    let kept = loot::modified_stack(
        looping,
        &stick(),
        &LootContext::default(),
        &mut StdRng::seed_from_u64(1),
    );
    assert_eq!(kept.map(|stack| stack.count.get()), Some(1));
    assert!(float_is(json!("t:half"), 0.5, &LootContext::default()));
}
