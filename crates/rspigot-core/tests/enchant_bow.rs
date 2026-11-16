mod common;

use common::*;
use rspigot_core::config::PluginSettings;
use rspigot_core::events::{DigAction, Direction, Hand, Inbound, PlayerInput};
use rspigot_core::plugins::{Answer, InventoryOrder, ItemSpec, Order, Question};
use rspigot_domain::{BlockPos, EntityId, Rotation};
use rspigot_entities::EntityKind as Kind;

const AIM: Rotation = Rotation::new(0.0, 0.0);

fn bow_scene() -> Scene {
    let mut server = server(PluginSettings::default());
    let plugin = connect_with(&mut server, 1, "tester", Vec::new());
    let alice = join(&mut server, "Alice");
    server.deliver();
    alice.link.take();
    let mut s = Scene {
        server,
        plugin,
        players: vec![alice],
    };
    stream_chunk_of(&mut s, BlockPos::new(1, 64, 1));
    s
}

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn give_bow(s: &mut Scene, enchantments: &[(&str, i32)], arrows: i32) {
    let player = s.players[0].id;
    let mut bow = ItemSpec::of("minecraft:bow", 1);
    bow.enchantments = enchantments
        .iter()
        .map(|(name, level)| ((*name).to_owned(), *level))
        .collect();
    s.act(Order::Inventory {
        player,
        order: InventoryOrder::Set {
            slot: 0,
            item: Some(bow),
        },
    });
    s.act(Order::Inventory {
        player,
        order: InventoryOrder::Set {
            slot: 9,
            item: Some(ItemSpec::of("minecraft:arrow", arrows)),
        },
    });
}

fn shoot(s: &mut Scene) {
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::UseItem {
            hand: Hand::Main,
            sequence: 1,
            rotation: AIM,
        },
    );
    s.tick(25);
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Dig {
            action: DigAction::ReleaseUseItem,
            pos: BlockPos::new(0, 0, 0),
            face: Direction::Down,
            sequence: 0,
        },
    );
}

fn target(s: &mut Scene) -> EntityId {
    console(s, "summon minecraft:husk 0.5 64 6.5 {NoAI:1b}");
    s.server.entities_of(Kind::Husk)[0].id
}

fn health(s: &mut Scene, id: EntityId) -> f32 {
    match s.ask(Question::Entity(id)) {
        Answer::Entity(details) => details.health.unwrap_or(0.0),
        _ => 0.0,
    }
}

fn arrows_left(s: &mut Scene) -> i32 {
    let player = s.players[0].id;
    match s.ask(Question::Inventory(player)) {
        Answer::Inventory(state) => state
            .slots
            .iter()
            .flatten()
            .filter(|stack| stack.key == "minecraft:arrow")
            .map(|stack| stack.count)
            .sum(),
        _ => 0,
    }
}

fn target_health_after(enchantments: &[(&str, i32)]) -> f32 {
    let mut s = bow_scene();
    give_bow(&mut s, enchantments, 3);
    let cow = target(&mut s);
    shoot(&mut s);
    s.tick(15);
    health(&mut s, cow)
}

#[test]
fn a_plain_arrow_hurts_the_target_by_six_to_ten() {
    let damage = 20.0 - target_health_after(&[]);
    assert!((6.0..=10.0).contains(&damage), "{damage}");
}

#[test]
fn power_adds_to_the_arrow_damage() {
    let damage = 20.0 - target_health_after(&[("minecraft:power", 5)]);
    assert!(damage >= 15.0, "{damage}");
}

#[test]
fn infinity_keeps_the_arrow_in_the_quiver() {
    let mut s = bow_scene();
    give_bow(&mut s, &[("minecraft:infinity", 1)], 3);
    shoot(&mut s);
    s.tick(2);
    assert_eq!(arrows_left(&mut s), 3);
    let mut plain = bow_scene();
    give_bow(&mut plain, &[], 3);
    shoot(&mut plain);
    plain.tick(2);
    assert_eq!(arrows_left(&mut plain), 2);
}

#[test]
fn flame_sets_the_target_on_fire() {
    let mut s = bow_scene();
    give_bow(&mut s, &[("minecraft:flame", 1)], 3);
    let cow = target(&mut s);
    shoot(&mut s);
    s.tick(10);
    let burning = match s.ask(Question::Entity(cow)) {
        Answer::Entity(details) => details.fire_ticks,
        _ => 0,
    };
    assert!(burning > 0, "{burning}");
}

fn push_after(enchantments: &[(&str, i32)]) -> f64 {
    let mut s = bow_scene();
    give_bow(&mut s, enchantments, 3);
    let target = target(&mut s);
    shoot(&mut s);
    let mut peak = 0.0_f64;
    for _ in 0..6 {
        s.tick(1);
        if let Answer::Entity(details) = s.ask(Question::Entity(target)) {
            peak = peak.max(details.velocity.z.abs());
        }
    }
    peak
}

#[test]
fn punch_pushes_the_target_along_the_arrow() {
    let plain = push_after(&[]);
    let punched = push_after(&[("minecraft:punch", 2)]);
    assert!(punched > plain + 0.5, "{punched} vs {plain}");
}
