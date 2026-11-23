use std::num::NonZeroU8;

use rand::SeedableRng;
use rand::rngs::StdRng;
use rspigot_core::loot::{self, LootContext, LootEntity};
use rspigot_core::pack_data::parse_functions;
use rspigot_domain::item::ItemStack;
use rspigot_entities::EntityKind;
use rspigot_items::ItemKind;
use rspigot_nbt::{Compound, Tag};
use serde_json::{Value, json};

fn of(kind: ItemKind) -> ItemStack {
    ItemStack::new(kind.id(), NonZeroU8::MIN)
}

fn run_on(functions: Value, stack: &ItemStack, context: &LootContext<'_>) -> Option<ItemStack> {
    let functions = parse_functions(&functions).unwrap();
    loot::modified_stack(functions, stack, context, &mut StdRng::seed_from_u64(3))
}

fn run(functions: Value, kind: ItemKind) -> ItemStack {
    run_on(functions, &of(kind), &LootContext::default()).expect("a stack")
}

fn component<'a>(stack: &'a ItemStack, name: &str) -> &'a Tag {
    stack
        .components
        .get(name)
        .unwrap_or_else(|| panic!("no {name} in {:?}", stack.components))
}

fn compound<'a>(stack: &'a ItemStack, name: &str) -> &'a Compound {
    match component(stack, name) {
        Tag::Compound(found) => found,
        other => panic!("{name} is {other:?}"),
    }
}

fn texts(list: &[Tag]) -> Vec<&str> {
    list.iter().filter_map(Tag::as_str).collect()
}

#[test]
fn fill_player_head_copies_the_profile_of_a_player_target() {
    let mut me = LootEntity::of(EntityKind::Player);
    me.name = Some("Alice".to_owned());
    let context = LootContext {
        this: Some(&me),
        ..LootContext::default()
    };
    let stack = run_on(
        json!({"function": "minecraft:fill_player_head", "entity": "this"}),
        &of(ItemKind::PlayerHead),
        &context,
    )
    .unwrap();
    assert_eq!(
        compound(&stack, "minecraft:profile").get_str("name"),
        Some("Alice")
    );
    let other = run_on(
        json!({"function": "minecraft:fill_player_head", "entity": "this"}),
        &of(ItemKind::Stick),
        &context,
    )
    .unwrap();
    assert!(!other.components.contains_key("minecraft:profile"));
}

#[test]
fn set_book_cover_edits_the_written_book_content() {
    let stack = run(
        json!({"function": "minecraft:set_book_cover", "title": "T", "author": "A",
               "generation": 2}),
        ItemKind::WrittenBook,
    );
    let book = compound(&stack, "minecraft:written_book_content");
    assert_eq!(book.get_str("title"), Some("T"));
    assert_eq!(book.get_str("author"), Some("A"));
    assert_eq!(book.get_i32("generation"), Some(2));
}

#[test]
fn page_functions_edit_the_book_pages_with_list_operations() {
    let writable = run(
        json!([
            {"function": "minecraft:set_writable_book_pages", "pages": ["a", "b"]},
            {"function": "minecraft:set_writable_book_pages", "pages": ["c"], "mode": "append"}
        ]),
        ItemKind::WritableBook,
    );
    let pages = compound(&writable, "minecraft:writable_book_content")
        .get_list("pages")
        .unwrap();
    assert_eq!(texts(pages), ["a", "b", "c"]);
    let written = run(
        json!({"function": "minecraft:set_written_book_pages", "pages": ["x"]}),
        ItemKind::WrittenBook,
    );
    let pages = compound(&written, "minecraft:written_book_content")
        .get_list("pages")
        .unwrap();
    assert_eq!(pages.len(), 1);
}

#[test]
fn set_custom_model_data_applies_each_list_operation() {
    let stack = run(
        json!({"function": "minecraft:set_custom_model_data",
               "floats": {"values": [1.5, 2.5]},
               "flags": {"values": [true]},
               "strings": {"values": ["s"]},
               "colors": {"values": [255]}}),
        ItemKind::Stick,
    );
    let data = compound(&stack, "minecraft:custom_model_data");
    assert_eq!(
        data.get_list("floats").unwrap(),
        [Tag::Float(1.5), Tag::Float(2.5)]
    );
    assert_eq!(data.get_list("flags").unwrap(), [Tag::Byte(1)]);
    assert_eq!(texts(data.get_list("strings").unwrap()), ["s"]);
    assert_eq!(data.get_list("colors").unwrap(), [Tag::Int(255)]);
}

#[test]
fn firework_functions_write_explosions_and_flight() {
    let stack = run(
        json!({"function": "minecraft:set_fireworks", "flight_duration": 2,
               "explosions": {"values": [{"shape": "star", "colors": [16711680],
                                          "has_trail": true}]}}),
        ItemKind::FireworkRocket,
    );
    let fireworks = compound(&stack, "minecraft:fireworks");
    assert_eq!(fireworks.get_i32("flight_duration"), Some(2));
    let explosions = fireworks.get_list("explosions").unwrap();
    assert_eq!(explosions.len(), 1);
    let Tag::Compound(first) = &explosions[0] else {
        panic!("not a compound")
    };
    assert_eq!(first.get_str("shape"), Some("star"));
    assert_eq!(first.get_bool("has_trail"), Some(true));
    let star = run(
        json!({"function": "minecraft:set_firework_explosion", "shape": "creeper",
               "fade_colors": [255], "twinkle": true}),
        ItemKind::FireworkStar,
    );
    let explosion = compound(&star, "minecraft:firework_explosion");
    assert_eq!(explosion.get_str("shape"), Some("creeper"));
    assert_eq!(explosion.get_int_array("fade_colors"), Some(&[255][..]));
    assert_eq!(explosion.get_bool("has_twinkle"), Some(true));
}

#[test]
fn set_banner_pattern_replaces_or_appends_layers() {
    let layer = |pattern: &str, color: &str| json!({"pattern": pattern, "color": color});
    let first = run(
        json!({"function": "minecraft:set_banner_pattern", "append": false,
               "patterns": [layer("minecraft:stripe_top", "red")]}),
        ItemKind::WhiteBanner,
    );
    let second = run_on(
        json!({"function": "minecraft:set_banner_pattern", "append": true,
               "patterns": [layer("minecraft:border", "blue")]}),
        &first,
        &LootContext::default(),
    )
    .unwrap();
    let Tag::List(layers) = component(&second, "minecraft:banner_patterns") else {
        panic!("not a list")
    };
    assert_eq!(layers.len(), 2);
    let Tag::Compound(last) = &layers[1] else {
        panic!("not a compound")
    };
    assert_eq!(last.get_str("color"), Some("blue"));
    assert_eq!(last.get_str("pattern"), Some("minecraft:border"));
}

#[test]
fn set_attributes_writes_modifiers_and_replaces_by_default() {
    let modifier = json!({"attribute": "minecraft:attack_damage", "id": "t:bonus",
        "operation": "add_value", "amount": {"type": "minecraft:uniform", "min": 2.0, "max": 2.0},
        "slot": "mainhand"});
    let stack = run(
        json!({"function": "minecraft:set_attributes", "modifiers": [modifier]}),
        ItemKind::DiamondSword,
    );
    let Tag::List(entries) = component(&stack, "minecraft:attribute_modifiers") else {
        panic!("not a list")
    };
    let Tag::Compound(entry) = &entries[0] else {
        panic!("not a compound")
    };
    assert_eq!(entry.get_str("type"), Some("minecraft:attack_damage"));
    assert_eq!(entry.get_str("id"), Some("t:bonus"));
    assert_eq!(entry.get_f64("amount"), Some(2.0));
    assert_eq!(entry.get_str("operation"), Some("add_value"));
    assert_eq!(entry.get_str("slot"), Some("mainhand"));
    let kept = run_on(
        json!({"function": "minecraft:set_attributes", "replace": false, "modifiers": [modifier]}),
        &stack,
        &LootContext::default(),
    )
    .unwrap();
    let Tag::List(entries) = component(&kept, "minecraft:attribute_modifiers") else {
        panic!("not a list")
    };
    assert_eq!(entries.len(), 2);
}

#[test]
fn set_contents_fills_and_modify_contents_edits_a_container() {
    let filled = run(
        json!({"function": "minecraft:set_contents", "component": "minecraft:container",
               "entries": [{"type": "minecraft:item", "name": "minecraft:apple"},
                           {"type": "minecraft:item", "name": "minecraft:stick"}]}),
        ItemKind::ShulkerBox,
    );
    let Tag::List(slots) = component(&filled, "minecraft:container") else {
        panic!("not a list")
    };
    assert_eq!(slots.len(), 2);
    let Tag::Compound(second) = &slots[1] else {
        panic!("not a compound")
    };
    assert_eq!(second.get_i32("slot"), Some(1));
    assert_eq!(
        second.get_compound("item").unwrap().get_str("id"),
        Some("minecraft:stick")
    );
    let doubled = run_on(
        json!({"function": "minecraft:modify_contents", "component": "minecraft:container",
               "modifier": {"function": "minecraft:set_count", "count": 5}}),
        &filled,
        &LootContext::default(),
    )
    .unwrap();
    let Tag::List(slots) = component(&doubled, "minecraft:container") else {
        panic!("not a list")
    };
    let Tag::Compound(first) = &slots[0] else {
        panic!("not a compound")
    };
    assert_eq!(
        first.get_compound("item").unwrap().get_i32("count"),
        Some(5)
    );
    let bundle = run(
        json!({"function": "minecraft:set_contents", "component": "minecraft:bundle_contents",
               "entries": [{"type": "minecraft:item", "name": "minecraft:apple"}]}),
        ItemKind::Bundle,
    );
    let Tag::List(items) = component(&bundle, "minecraft:bundle_contents") else {
        panic!("not a list")
    };
    assert_eq!(items.len(), 1);
}

#[test]
fn set_random_dyes_colours_a_dyeable_item_only() {
    let dyed = run(
        json!({"function": "minecraft:set_random_dyes", "number_of_dyes": 2}),
        ItemKind::LeatherChestplate,
    );
    assert!(matches!(
        component(&dyed, "minecraft:dyed_color"),
        Tag::Int(_)
    ));
    let plain = run(
        json!({"function": "minecraft:set_random_dyes", "number_of_dyes": 2}),
        ItemKind::Stick,
    );
    assert!(!plain.components.contains_key("minecraft:dyed_color"));
}

#[test]
fn set_random_potion_picks_from_the_options() {
    let stack = run(
        json!({"function": "minecraft:set_random_potion", "options": ["minecraft:swiftness"]}),
        ItemKind::Potion,
    );
    assert_eq!(
        compound(&stack, "minecraft:potion_contents").get_str("potion"),
        Some("minecraft:swiftness")
    );
}

#[test]
fn discard_and_filtered_remove_or_branch_stacks() {
    let discarded = run_on(
        json!({"function": "minecraft:discard"}),
        &of(ItemKind::Stick),
        &LootContext::default(),
    );
    assert!(discarded.is_none());
    let filtered = json!({"function": "minecraft:filtered",
        "item_filter": {"items": "minecraft:stick"},
        "on_pass": {"function": "minecraft:set_count", "count": 4},
        "on_fail": {"type": "minecraft:discard"}});
    let passed = run_on(
        filtered.clone(),
        &of(ItemKind::Stick),
        &LootContext::default(),
    );
    assert_eq!(passed.map(|stack| stack.count.get()), Some(4));
    let failed = run_on(filtered, &of(ItemKind::Apple), &LootContext::default());
    assert!(failed.is_none());
}

#[test]
fn copy_custom_data_reads_a_command_storage_source() {
    let data = Compound::new().with("a", Compound::new().with("b", 9));
    let world = loot::LootWorld {
        storages: vec![("t:store", &data)],
        ..loot::LootWorld::default()
    };
    let context = LootContext {
        world: Some(&world),
        ..LootContext::default()
    };
    let stack = run_on(
        json!({"function": "minecraft:copy_custom_data",
               "source": {"type": "minecraft:storage", "source": "t:store"},
               "ops": [{"source": "a.b", "target": "copied", "op": "replace"}]}),
        &of(ItemKind::Stick),
        &context,
    )
    .unwrap();
    assert_eq!(
        compound(&stack, "minecraft:custom_data").get_i32("copied"),
        Some(9)
    );
}

#[test]
fn copy_name_and_copy_components_accept_entity_sources_and_excludes() {
    let mut me = LootEntity::of(EntityKind::Player);
    me.name = Some("Alice".to_owned());
    me.components = vec![("minecraft:custom_name", "\"Bob\"".to_owned())];
    let block = Compound::new()
        .with("CustomName", "\"Chest\"")
        .with("lock", "key");
    let context = LootContext {
        this: Some(&me),
        block_entity: Some(&block),
        ..LootContext::default()
    };
    let named = run_on(
        json!({"function": "minecraft:copy_name", "source": "this"}),
        &of(ItemKind::Stick),
        &context,
    )
    .unwrap();
    assert_eq!(
        component(&named, "minecraft:custom_name"),
        &Tag::String("Alice".to_owned())
    );
    let from_entity = run_on(
        json!({"function": "minecraft:copy_components", "source": "this",
               "include": ["minecraft:custom_name"]}),
        &of(ItemKind::Stick),
        &context,
    )
    .unwrap();
    assert!(from_entity.components.contains_key("minecraft:custom_name"));
    let excluded = run_on(
        json!({"function": "minecraft:copy_components", "source": "block_entity",
               "exclude": ["minecraft:lock"]}),
        &of(ItemKind::Stick),
        &context,
    )
    .unwrap();
    assert!(excluded.components.contains_key("minecraft:custom_name"));
    assert!(!excluded.components.contains_key("minecraft:lock"));
}

#[test]
fn enchant_randomly_without_options_draws_from_every_enchantment() {
    let stack = run(
        json!({"function": "minecraft:enchant_randomly"}),
        ItemKind::DiamondSword,
    );
    let Tag::Compound(enchantments) = component(&stack, "minecraft:enchantments") else {
        panic!("not a compound")
    };
    assert!(!enchantments.is_empty());
}
