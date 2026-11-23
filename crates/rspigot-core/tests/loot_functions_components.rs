use rand::SeedableRng;
use rand::rngs::StdRng;
use rspigot_blocks::BlockKind;
use rspigot_core::loot::{self, LootContext};
use rspigot_data::loot::{
    CopyOperation, CustomDataSource, Function, FunctionKind, ListMode, MergeStrategy, NameTarget,
};
use rspigot_domain::cartography::PENDING_EXPLORATION;
use rspigot_domain::item::ItemStack;
use rspigot_items::ItemKind;
use rspigot_nbt::{Compound, Tag, parse_tag};

fn stick() -> ItemStack {
    ItemStack::new(ItemKind::Stick.id(), std::num::NonZeroU8::MIN)
}

fn run(functions: &[Function], stack: &ItemStack, context: &LootContext<'_>) -> ItemStack {
    loot::modified_stack(functions, stack, context, &mut StdRng::seed_from_u64(1))
        .expect("a surviving stack")
}

fn plain(kind: FunctionKind) -> Function {
    Function {
        condition: None,
        kind,
    }
}

fn tag(snbt: &str) -> Tag {
    parse_tag(snbt).expect("valid snbt")
}

#[test]
fn copy_state_stores_the_listed_properties_of_the_block() {
    let hive = BlockKind::Beehive
        .state_with(&[("honey_level", "3")])
        .unwrap();
    let context = LootContext::block(hive, None, None);
    let out = run(
        &[plain(FunctionKind::CopyState {
            block: "minecraft:beehive",
            properties: &["honey_level"],
        })],
        &stick(),
        &context,
    );
    let held = out
        .components
        .get_compound("minecraft:block_state")
        .unwrap();
    assert_eq!(held.get_str("honey_level"), Some("3"));
}

#[test]
fn copy_state_ignores_a_block_of_another_kind() {
    let context = LootContext::block(BlockKind::Stone.default_state(), None, None);
    let out = run(
        &[plain(FunctionKind::CopyState {
            block: "minecraft:beehive",
            properties: &["honey_level"],
        })],
        &stick(),
        &context,
    );
    assert!(!out.components.contains_key("minecraft:block_state"));
}

#[test]
fn set_name_writes_the_chosen_name_component() {
    let context = LootContext::default();
    let item_name = run(
        &[plain(FunctionKind::SetName {
            name: "{\"translate\":\"filled_map.test\"}",
            target: NameTarget::ItemName,
            entity: None,
        })],
        &stick(),
        &context,
    );
    assert_eq!(
        item_name.components.get("minecraft:item_name"),
        Some(&tag("{\"translate\":\"filled_map.test\"}"))
    );
    let custom = run(
        &[plain(FunctionKind::SetName {
            name: "\"Excalibur\"",
            target: NameTarget::CustomName,
            entity: None,
        })],
        &stick(),
        &context,
    );
    assert_eq!(
        custom.components.get_str("minecraft:custom_name"),
        Some("Excalibur")
    );
}

fn lore_after(mode: ListMode, start: &[&str]) -> Vec<String> {
    let mut seed = stick();
    seed.components.insert(
        "minecraft:lore",
        Tag::List(
            start
                .iter()
                .map(|line| Tag::String((*line).to_owned()))
                .collect(),
        ),
    );
    let out = run(
        &[plain(FunctionKind::SetLore {
            lines: &["\"x\"", "\"y\""],
            mode,
            entity: None,
        })],
        &seed,
        &LootContext::default(),
    );
    out.components
        .get_list("minecraft:lore")
        .unwrap()
        .iter()
        .map(|line| line.as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn set_lore_follows_the_list_operation() {
    assert_eq!(
        lore_after(ListMode::ReplaceAll, &["a", "b", "c"]),
        ["x", "y"]
    );
    assert_eq!(
        lore_after(ListMode::Append, &["a", "b"]),
        ["a", "b", "x", "y"]
    );
    assert_eq!(
        lore_after(ListMode::Insert { offset: 1 }, &["a", "b"]),
        ["a", "x", "y", "b"]
    );
    assert_eq!(
        lore_after(
            ListMode::ReplaceSection {
                offset: 1,
                size: Some(1)
            },
            &["a", "b", "c"]
        ),
        ["a", "x", "y", "c"]
    );
    assert_eq!(
        lore_after(
            ListMode::ReplaceSection {
                offset: 0,
                size: None
            },
            &["a", "b", "c"]
        ),
        ["x", "y", "c"]
    );
}

#[test]
fn set_custom_data_merges_into_the_existing_data() {
    let mut seed = stick();
    seed.components
        .insert("minecraft:custom_data", tag("{keep:1,nested:{a:1,b:2}}"));
    let out = run(
        &[plain(FunctionKind::SetCustomData {
            tag: "{added:\"yes\",nested:{b:9}}",
        })],
        &seed,
        &LootContext::default(),
    );
    assert_eq!(
        out.components.get("minecraft:custom_data"),
        Some(&tag("{keep:1,nested:{a:1,b:9},added:\"yes\"}"))
    );
}

#[test]
fn copy_custom_data_reads_the_block_entity() {
    let entity = Compound::new()
        .with("Lock", "key")
        .with("bees", Tag::List(vec![tag("{n:1}"), tag("{n:2}")]))
        .with("inner", tag("{a:1}"));
    let context = LootContext::block(BlockKind::Beehive.default_state(), None, None)
        .with_block_entity(Some(&entity));
    let out = run(
        &[plain(FunctionKind::CopyCustomData {
            source: CustomDataSource::BlockEntity,
            operations: &[
                CopyOperation {
                    source: "Lock",
                    target: "lock",
                    strategy: MergeStrategy::Replace,
                },
                CopyOperation {
                    source: "bees[0]",
                    target: "seen",
                    strategy: MergeStrategy::Append,
                },
                CopyOperation {
                    source: "inner",
                    target: "merged",
                    strategy: MergeStrategy::Merge,
                },
            ],
        })],
        &stick(),
        &context,
    );
    assert_eq!(
        out.components.get("minecraft:custom_data"),
        Some(&tag("{lock:\"key\",seen:[{n:1}],merged:{a:1}}"))
    );
}

#[test]
fn copy_custom_data_without_a_source_changes_nothing() {
    let out = run(
        &[plain(FunctionKind::CopyCustomData {
            source: CustomDataSource::BlockEntity,
            operations: &[CopyOperation {
                source: "Lock",
                target: "lock",
                strategy: MergeStrategy::Replace,
            }],
        })],
        &stick(),
        &LootContext::default(),
    );
    assert!(!out.components.contains_key("minecraft:custom_data"));
}

#[test]
fn toggle_tooltips_hides_and_shows_components() {
    let mut seed = stick();
    seed.components.insert(
        "minecraft:tooltip_display",
        tag("{hidden_components:[\"minecraft:enchantments\",\"minecraft:trim\"]}"),
    );
    let out = run(
        &[plain(FunctionKind::ToggleTooltips(&[
            ("minecraft:attribute_modifiers", false),
            ("minecraft:trim", true),
        ]))],
        &seed,
        &LootContext::default(),
    );
    let display = out
        .components
        .get_compound("minecraft:tooltip_display")
        .unwrap();
    let hidden: Vec<&str> = display
        .get_list("hidden_components")
        .unwrap()
        .iter()
        .map(|name| name.as_str().unwrap())
        .collect();
    assert_eq!(
        hidden,
        ["minecraft:enchantments", "minecraft:attribute_modifiers"]
    );
}

#[test]
fn set_components_writes_each_component_value() {
    let out = run(
        &[plain(FunctionKind::SetComponents(&[
            (
                "minecraft:trim",
                "{material:\"minecraft:copper\",pattern:\"minecraft:bolt\"}",
            ),
            ("minecraft:damage", "5"),
        ]))],
        &stick(),
        &LootContext::default(),
    );
    assert_eq!(
        out.components.get("minecraft:trim"),
        Some(&tag(
            "{material:\"minecraft:copper\",pattern:\"minecraft:bolt\"}"
        ))
    );
    assert_eq!(out.components.get("minecraft:damage"), Some(&Tag::Int(5)));
}

#[test]
fn a_silk_touched_beehive_keeps_its_honey_level() {
    let hive = BlockKind::Beehive
        .state_with(&[("honey_level", "5")])
        .unwrap();
    let mut pickaxe = ItemStack::new(ItemKind::DiamondPickaxe.id(), std::num::NonZeroU8::MIN);
    pickaxe.set_enchantments(&[("minecraft:silk_touch".to_owned(), 1)]);
    let drops = loot::block_drops(
        &LootContext::block(hive, Some(&pickaxe), None),
        &mut StdRng::seed_from_u64(3),
    );
    assert_eq!(drops.len(), 1);
    let held = drops[0]
        .components
        .get_compound("minecraft:block_state")
        .unwrap();
    assert_eq!(held.get_str("honey_level"), Some("5"));
}

#[test]
fn a_copper_golem_statue_keeps_its_pose() {
    let pose = BlockKind::CopperGolemStatue
        .state_with(&[("copper_golem_pose", "sitting")])
        .unwrap();
    let drops = loot::block_drops(
        &LootContext::block(pose, None, None),
        &mut StdRng::seed_from_u64(3),
    );
    assert_eq!(drops.len(), 1);
    let held = drops[0]
        .components
        .get_compound("minecraft:block_state")
        .unwrap();
    assert_eq!(held.get_str("copper_golem_pose"), Some("sitting"));
}

#[test]
fn the_abandoned_camp_maps_carry_their_item_name() {
    let table = rspigot_data::loot::table("minecraft:chests/abandoned_camp_common_chest").unwrap();
    let context = LootContext {
        exploration_maps: true,
        ..LootContext::default()
    };
    let mut named = 0;
    for seed in 0..400 {
        for drop in loot::table_drops(table, &context, &mut StdRng::seed_from_u64(seed)) {
            if drop.components.contains_key(PENDING_EXPLORATION) {
                assert!(
                    drop.components.contains_key("minecraft:item_name"),
                    "seed {seed}"
                );
                named += 1;
            }
        }
    }
    assert!(named > 0);
}
