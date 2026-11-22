use std::num::NonZeroU8;

use rand::SeedableRng;
use rand::rngs::StdRng;
use rspigot_core::loot::{self, LootContext, LootEntity, LootWorld};
use rspigot_core::pack_data::{PackData, condition};
use rspigot_core::ports::{PackContents, PackFile};
use rspigot_domain::Identifier;
use rspigot_domain::entity::save::item_stack_to_nbt;
use rspigot_domain::item::ItemStack;
use rspigot_entities::EntityKind;
use rspigot_items::ItemKind;
use rspigot_nbt::{Compound, Tag};
use serde_json::{Value, json};

fn stack(kind: ItemKind, count: u8) -> ItemStack {
    ItemStack::new(kind.id(), NonZeroU8::new(count).unwrap())
}

fn drops_of(table: Value, context: &LootContext<'_>) -> Vec<(String, u8)> {
    let pack = PackContents {
        name: "p".to_owned(),
        loot_tables: vec![PackFile {
            id: Identifier::parse("t:slots").unwrap(),
            text: table.to_string(),
        }],
        ..PackContents::default()
    };
    let data = PackData::build(&[&pack]);
    data.activate();
    let table = rspigot_data::loot::table("t:slots").expect("the table parsed");
    loot::table_drops(table, context, &mut StdRng::seed_from_u64(5))
        .iter()
        .map(|drop| {
            (
                drop.item.kind().unwrap().name().to_owned(),
                drop.count.get(),
            )
        })
        .collect()
}

fn slots_table(source: Value) -> Value {
    json!({"pools": [{"rolls": 1, "entries": [{"type": "minecraft:slots", "slot_source": source}]}]})
}

fn player_with_inventory() -> LootEntity {
    let mut me = LootEntity::of(EntityKind::Player);
    me.inventory = vec![
        (0, stack(ItemKind::Apple, 3)),
        (1, stack(ItemKind::Stick, 2)),
        (2, stack(ItemKind::Stone, 9)),
        (103, stack(ItemKind::DiamondHelmet, 1)),
    ];
    me
}

#[test]
fn a_slot_range_entry_drops_copies_of_the_entity_slots() {
    let me = player_with_inventory();
    let context = LootContext {
        this: Some(&me),
        ..LootContext::default()
    };
    let drops = drops_of(
        slots_table(json!({"type": "minecraft:slot_range", "source": "this", "slots": "hotbar.*"})),
        &context,
    );
    assert_eq!(
        drops,
        [
            ("minecraft:apple".to_owned(), 3),
            ("minecraft:stick".to_owned(), 2),
            ("minecraft:stone".to_owned(), 9)
        ]
    );
}

#[test]
fn slot_sources_filter_limit_group_and_stay_empty() {
    let me = player_with_inventory();
    let context = LootContext {
        this: Some(&me),
        ..LootContext::default()
    };
    let hotbar = json!({"type": "minecraft:slot_range", "source": "this", "slots": "hotbar.*"});
    let limited = drops_of(
        slots_table(json!({"type": "minecraft:limit_slots", "slot_source": hotbar, "limit": 2})),
        &context,
    );
    assert_eq!(limited.len(), 2);
    let filtered = drops_of(
        slots_table(json!({"type": "minecraft:filtered", "slot_source": hotbar,
                           "item_filter": {"items": "minecraft:stick"}})),
        &context,
    );
    assert_eq!(filtered, [("minecraft:stick".to_owned(), 2)]);
    let armor = json!({"type": "minecraft:slot_range", "source": "this", "slots": "armor.head"});
    let grouped = drops_of(
        slots_table(json!({"type": "minecraft:group", "terms": [armor, hotbar]})),
        &context,
    );
    assert_eq!(grouped.len(), 4);
    assert!(drops_of(slots_table(json!({"type": "minecraft:empty"})), &context).is_empty());
}

#[test]
fn a_contents_source_expands_the_container_component_of_each_stack() {
    let mut shulker = stack(ItemKind::ShulkerBox, 1);
    let inner = item_stack_to_nbt(&stack(ItemKind::Diamond, 4));
    shulker.components.insert(
        "minecraft:container",
        Tag::List(vec![Tag::Compound(
            Compound::new().with("slot", 0).with("item", inner),
        )]),
    );
    let mut me = LootEntity::of(EntityKind::Player);
    me.inventory = vec![(0, shulker)];
    let context = LootContext {
        this: Some(&me),
        ..LootContext::default()
    };
    let drops = drops_of(
        slots_table(
            json!({"type": "minecraft:contents", "component": "minecraft:container",
            "slot_source": {"type": "minecraft:slot_range", "source": "this", "slots": "hotbar.0"}}),
        ),
        &context,
    );
    assert_eq!(drops, [("minecraft:diamond".to_owned(), 4)]);
}

#[test]
fn a_block_entity_slot_range_reads_the_container_of_the_context() {
    let held = [
        (0, stack(ItemKind::GoldIngot, 1)),
        (5, stack(ItemKind::IronIngot, 2)),
    ];
    let context = LootContext {
        container: &held,
        ..LootContext::default()
    };
    let drops = drops_of(
        slots_table(
            json!({"type": "minecraft:slot_range", "source": "block_entity", "slots": "container.*"}),
        ),
        &context,
    );
    assert_eq!(drops.len(), 2);
}

#[test]
fn time_checks_read_the_clock_they_name() {
    let world = LootWorld {
        clocks: vec![("minecraft:overworld", 100), ("minecraft:the_end", 5000)],
        ..LootWorld::default()
    };
    let context = LootContext {
        world: Some(&world),
        day_time: Some(100),
        ..LootContext::default()
    };
    let check = |clock: &str| {
        condition(&json!({"condition": "minecraft:time_check", "clock": clock,
                          "value": {"min": 4000, "max": 6000}}))
        .unwrap()
    };
    let holds =
        |clock: &str| loot::condition_holds(&check(clock), &context, &mut StdRng::seed_from_u64(1));
    assert!(holds("minecraft:the_end"));
    assert!(!holds("minecraft:overworld"));
    let wrapped = condition(&json!({"condition": "minecraft:time_check",
        "clock": "minecraft:the_end", "period": 1000, "value": {"min": 0, "max": 10}}))
    .unwrap();
    assert!(loot::condition_holds(
        &wrapped,
        &context,
        &mut StdRng::seed_from_u64(1)
    ));
}

#[test]
fn environment_attribute_checks_follow_the_clock_and_the_dimension() {
    let check = |attribute: &str, value: Value| {
        condition(
            &json!({"condition": "minecraft:environment_attribute_check",
                          "attribute": attribute, "value": value}),
        )
        .unwrap()
    };
    let at = |ticks: i64, dimension: &'static str, check: &rspigot_data::loot::Condition| {
        let world = LootWorld {
            clocks: vec![("minecraft:overworld", ticks), ("minecraft:the_end", 0)],
            dimension,
            ..LootWorld::default()
        };
        let context = LootContext {
            world: Some(&world),
            ..LootContext::default()
        };
        loot::condition_holds(check, &context, &mut StdRng::seed_from_u64(1))
    };
    let burns = check("minecraft:gameplay/monsters_burn", json!(true));
    assert!(at(6000, "minecraft:overworld", &burns));
    assert!(!at(18_000, "minecraft:overworld", &burns));
    let nether_light = check("minecraft:gameplay/sky_light_level", json!(4.0));
    assert!(at(6000, "minecraft:the_nether", &nether_light));
    assert!(!at(6000, "minecraft:overworld", &nether_light));
    let fast_lava = check("minecraft:gameplay/fast_lava", json!(true));
    assert!(at(0, "minecraft:the_nether", &fast_lava));
    assert!(!at(0, "minecraft:overworld", &fast_lava));
}

#[test]
fn enchantment_active_checks_compare_the_context_flag() {
    let check = condition(&json!({"condition": "minecraft:enchantment_active_check",
                                  "active": true}))
    .unwrap();
    let holds = |active: Option<bool>| {
        let context = LootContext {
            enchantment_active: active,
            ..LootContext::default()
        };
        loot::condition_holds(&check, &context, &mut StdRng::seed_from_u64(1))
    };
    assert!(holds(Some(true)));
    assert!(!holds(Some(false)));
    assert!(!holds(None));
}
