mod common;

use common::*;
use rspigot_core::config::PluginSettings;
use rspigot_core::events::{Hand, Inbound, PlayerInput};
use rspigot_core::plugins::events::{EntityRegainHealthPatch, EntityRegainHealthRegainReason};
use rspigot_core::plugins::{Answer, EventKind, EventPatch, EventPayload, Question};
use rspigot_domain::{BlockPos, EntityId, Vec3};
use rspigot_entities::EntityKind as Kind;

fn waiting_scene() -> Scene {
    let mut server = server(PluginSettings {
        decision_ticks: 1_000,
        ..PluginSettings::default()
    });
    let plugin = connect_with(
        &mut server,
        1,
        "tester",
        vec![deciding(1, EventKind::EntityRegainHealth)],
    );
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

fn golem(s: &mut Scene, nbt: &str) -> EntityId {
    stream_chunk_of(s, BlockPos::new(1, 64, 1));
    console(s, &format!("summon minecraft:iron_golem 1.5 64 1.5 {nbt}"));
    s.server.entities_of(Kind::IronGolem)[0].id
}

fn interact(s: &mut Scene, target: EntityId) {
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Interact {
            target,
            hand: Hand::Main,
            sneaking: false,
            location: Vec3::new(0.0, 0.5, 0.0),
        },
    );
}

fn health(s: &Scene, id: EntityId) -> f32 {
    s.server
        .entities_of(Kind::IronGolem)
        .into_iter()
        .find(|entity| entity.id == id)
        .and_then(|entity| entity.health)
        .expect("a living entity")
}

fn ingots(s: &mut Scene) -> i32 {
    let Answer::Inventory(state) = s.ask(Question::Inventory(s.players[0].id)) else {
        panic!("expected Alice's inventory");
    };
    state
        .slots
        .iter()
        .flatten()
        .filter(|item| item.key == "minecraft:iron_ingot")
        .map(|item| item.count)
        .sum()
}

#[test]
fn the_repair_asks_a_custom_regain_and_spends_the_ingot_only_when_it_applies() {
    let mut s = waiting_scene();
    console(&mut s, "give Alice minecraft:iron_ingot 2");
    let id = golem(&mut s, "{NoAI:1b,Health:40f}");
    interact(&mut s, id);
    let asked = last_asked(&s.plugin, EventKind::EntityRegainHealth);
    let EventPayload::EntityRegainHealth(regain) = &asked.payload else {
        panic!("a regain");
    };
    assert_eq!(regain.entity.id, id);
    assert_eq!(regain.regain_reason, EntityRegainHealthRegainReason::Custom);
    assert_eq!(regain.amount, 25.0);
    assert_eq!(health(&s, id), 40.0);
    assert_eq!(ingots(&mut s), 2);
    rule_on(&mut s.server, 1, asked.id, true, None);
    assert_eq!(health(&s, id), 40.0);
    assert_eq!(ingots(&mut s), 2);
    interact(&mut s, id);
    let asked = last_asked(&s.plugin, EventKind::EntityRegainHealth);
    rule_on(
        &mut s.server,
        1,
        asked.id,
        false,
        Some(EventPatch::EntityRegainHealth(EntityRegainHealthPatch {
            amount: Some(10.0),
        })),
    );
    assert_eq!(health(&s, id), 50.0);
    assert_eq!(ingots(&mut s), 1);
}

#[test]
fn a_golem_at_full_health_still_asks_the_regain_but_keeps_the_ingot() {
    let mut s = waiting_scene();
    console(&mut s, "give Alice minecraft:iron_ingot 1");
    let id = golem(&mut s, "{NoAI:1b}");
    interact(&mut s, id);
    let asked = last_asked(&s.plugin, EventKind::EntityRegainHealth);
    rule_on(&mut s.server, 1, asked.id, false, None);
    assert_eq!(health(&s, id), 100.0);
    assert_eq!(ingots(&mut s), 1);
}
