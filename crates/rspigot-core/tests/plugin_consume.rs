mod common;

use common::*;
use rspigot_core::config::PluginSettings;
use rspigot_core::events::{Hand, Inbound, PlayerInput};
use rspigot_core::plugins::events::{EquipmentSlot, PlayerItemConsumePatch};
use rspigot_core::plugins::{
    Answer, EventKind, EventPatch, EventPayload, HandlerSpec, ItemSpec, Question,
};
use rspigot_domain::Rotation;

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

fn eat(s: &mut Scene, item: &str, count: u32) {
    console(s, &format!("give Alice {item} {count}"));
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::UseItem {
            hand: Hand::Main,
            sequence: 1,
            rotation: Rotation::new(0.0, 0.0),
        },
    );
    s.tick(40);
}

fn held(s: &mut Scene, item: &str) -> i32 {
    let Answer::Inventory(state) = s.ask(Question::Inventory(s.players[0].id)) else {
        panic!("expected Alice's inventory");
    };
    state
        .slots
        .iter()
        .flatten()
        .filter(|stack| stack.key == item)
        .map(|stack| stack.count)
        .sum()
}

#[test]
fn eating_a_golden_apple_tells_the_plugins_the_hand_and_the_item_and_spends_it() {
    let mut s = waiting_scene(vec![observe(1, EventKind::PlayerItemConsume)]);
    eat(&mut s, "minecraft:golden_apple", 2);
    let consumed: Vec<_> = fired(&s.plugin)
        .into_iter()
        .filter_map(|event| match event.payload {
            EventPayload::PlayerItemConsume(consume) => Some(consume),
            _ => None,
        })
        .collect();
    assert_eq!(consumed.len(), 1);
    assert_eq!(consumed[0].hand, EquipmentSlot::Hand);
    assert_eq!(consumed[0].item.key, "minecraft:golden_apple");
    assert_eq!(consumed[0].item.count, 2);
    assert_eq!(held(&mut s, "minecraft:golden_apple"), 1);
}

#[test]
fn a_cancelled_consumption_keeps_the_item() {
    let mut s = waiting_scene(vec![deciding(1, EventKind::PlayerItemConsume)]);
    eat(&mut s, "minecraft:golden_apple", 2);
    let asked = last_asked(&s.plugin, EventKind::PlayerItemConsume);
    rule_on(&mut s.server, 1, asked.id, true, None);
    s.tick(2);
    assert_eq!(held(&mut s, "minecraft:golden_apple"), 2);
}

#[test]
fn an_allowed_consumption_eats_the_item() {
    let mut s = waiting_scene(vec![deciding(1, EventKind::PlayerItemConsume)]);
    eat(&mut s, "minecraft:golden_apple", 2);
    let asked = last_asked(&s.plugin, EventKind::PlayerItemConsume);
    rule_on(&mut s.server, 1, asked.id, false, None);
    s.tick(2);
    assert_eq!(held(&mut s, "minecraft:golden_apple"), 1);
}

#[test]
fn a_replacement_item_is_the_one_eaten_and_what_is_left_of_it_takes_the_hand() {
    let mut s = waiting_scene(vec![deciding(1, EventKind::PlayerItemConsume)]);
    eat(&mut s, "minecraft:golden_apple", 3);
    let asked = last_asked(&s.plugin, EventKind::PlayerItemConsume);
    rule_on(
        &mut s.server,
        1,
        asked.id,
        false,
        Some(EventPatch::PlayerItemConsume(PlayerItemConsumePatch {
            item: Some(ItemSpec::of("minecraft:enchanted_golden_apple", 2)),
        })),
    );
    s.tick(2);
    assert_eq!(held(&mut s, "minecraft:golden_apple"), 0);
    assert_eq!(held(&mut s, "minecraft:enchanted_golden_apple"), 1);
}
