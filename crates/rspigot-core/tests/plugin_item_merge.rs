mod common;

use common::*;
use rspigot_core::config::PluginSettings;
use rspigot_core::events::Inbound;
use rspigot_core::plugins::HandlerSpec;
use rspigot_core::plugins::{EventKind, EventPayload};

fn waiting_scene(handlers: Vec<HandlerSpec>) -> Scene {
    let mut server = server(PluginSettings {
        decision_ticks: 1_000,
        ..PluginSettings::default()
    });
    let plugin = connect_with(&mut server, 1, "tester", handlers);
    let alice = join(&mut server, "Alice");
    server.deliver();
    alice.link.take();
    Scene {
        server,
        plugin,
        players: vec![alice],
    }
}

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn drop_stack(s: &mut Scene, item: &str, count: u8) {
    console(
        s,
        &format!(
            "summon item 8.5 64 8.5 {{Item:{{id:\"minecraft:{item}\",count:{count}}},Motion:[0.0,0.0,0.0]}}"
        ),
    );
}

fn stacks(s: &Scene) -> Vec<(String, u8)> {
    let mut found: Vec<(String, u8)> = s
        .server
        .ground_items()
        .into_iter()
        .map(|(_, stack)| {
            (
                stack
                    .item
                    .kind()
                    .map_or_else(String::new, |kind| kind.name().to_owned()),
                stack.count.get(),
            )
        })
        .collect();
    found.sort();
    found
}

#[test]
fn two_stacks_of_the_same_item_merge_into_one_and_fire_the_event() {
    let mut s = scene_with(&["Alice"], vec![observe(1, EventKind::ItemMerge)]);
    drop_stack(&mut s, "cobblestone", 3);
    drop_stack(&mut s, "cobblestone", 4);
    s.tick(45);
    assert_eq!(stacks(&s), [("minecraft:cobblestone".to_owned(), 7)]);
    let events = settled_for(&s.plugin, EventKind::ItemMerge);
    assert_eq!(events.len(), 1);
    let EventPayload::ItemMerge(event) = &events[0].payload else {
        panic!("expected a merge event");
    };
    assert_eq!(event.entity_type, "minecraft:item");
    assert_ne!(event.entity.id, event.target.id);
}

#[test]
fn a_cancelled_merge_keeps_both_stacks() {
    let mut s = waiting_scene(vec![deciding(1, EventKind::ItemMerge)]);
    drop_stack(&mut s, "cobblestone", 3);
    drop_stack(&mut s, "cobblestone", 4);
    s.tick(45);
    for asked in asked_for(&s.plugin, EventKind::ItemMerge) {
        rule_on(&mut s.server, 1, asked.id, true, None);
    }
    assert_eq!(stacks(&s).len(), 2);
}

#[test]
fn an_allowed_merge_joins_the_stacks() {
    let mut s = waiting_scene(vec![deciding(1, EventKind::ItemMerge)]);
    drop_stack(&mut s, "cobblestone", 3);
    drop_stack(&mut s, "cobblestone", 4);
    s.tick(45);
    for asked in asked_for(&s.plugin, EventKind::ItemMerge) {
        rule_on(&mut s.server, 1, asked.id, false, None);
    }
    assert_eq!(stacks(&s), [("minecraft:cobblestone".to_owned(), 7)]);
}

#[test]
fn other_items_and_full_stacks_do_not_merge() {
    let mut s = scene_with(&["Alice"], vec![observe(1, EventKind::ItemMerge)]);
    drop_stack(&mut s, "cobblestone", 3);
    drop_stack(&mut s, "dirt", 4);
    drop_stack(&mut s, "stone", 40);
    drop_stack(&mut s, "stone", 40);
    s.tick(45);
    assert_eq!(stacks(&s).len(), 4);
    assert!(settled_for(&s.plugin, EventKind::ItemMerge).is_empty());
}

fn merged_stacks_at_a_gap(radius: f64) -> usize {
    let mut s = scene_on(
        configured(|config| config.item_merge_radius = radius),
        &["Alice"],
        Vec::new(),
    );
    console(
        &mut s,
        "summon item 8.5 64 8.5 {Item:{id:\"minecraft:cobblestone\",count:3},Motion:[0.0,0.0,0.0]}",
    );
    console(
        &mut s,
        "summon item 9.9 64 8.5 {Item:{id:\"minecraft:cobblestone\",count:4},Motion:[0.0,0.0,0.0]}",
    );
    s.tick(45);
    stacks(&s).len()
}

#[test]
fn the_merge_radius_setting_decides_whether_far_stacks_join() {
    assert_eq!(merged_stacks_at_a_gap(0.5), 2);
    assert_eq!(merged_stacks_at_a_gap(2.0), 1);
}
