mod common;

use common::*;
use rspigot_core::config::PluginSettings;
use rspigot_core::events::Inbound;
use rspigot_core::plugins::events::{BlockIgniteCause, ExplosionPrimePatch};
use rspigot_core::plugins::{Answer, EventKind, EventPatch, EventPayload, HandlerSpec, Question};
use rspigot_domain::{BlockPos, EntityId};
use rspigot_entities::EntityKind as Kind;

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

fn creeper(s: &mut Scene) -> EntityId {
    let at = BlockPos::new(8, 64, 8);
    stream_chunk_of(s, at);
    console(
        s,
        "summon minecraft:creeper 8.5 64 8.5 {NoAI:1b,ignited:1b,Fuse:2s}",
    );
    let found = s.server.entities_of(Kind::Creeper);
    assert_eq!(found.len(), 1);
    found[0].id
}

fn block(s: &mut Scene, pos: BlockPos) -> String {
    match s.ask(Question::Block(pos)) {
        Answer::Block(state) => state.kind,
        _ => "minecraft:air".to_owned(),
    }
}

fn blow_with_fire(s: &mut Scene) {
    creeper(s);
    s.tick(5);
    let asked = last_asked(&s.plugin, EventKind::ExplosionPrime);
    rule_on(
        &mut s.server,
        1,
        asked.id,
        false,
        Some(EventPatch::ExplosionPrime(ExplosionPrimePatch {
            fire: Some(true),
            radius: Some(12.0),
        })),
    );
}

fn fires(s: &mut Scene) -> usize {
    let mut fires = 0;
    for x in -6..=22 {
        for z in -6..=22 {
            for y in 59..=64 {
                if block(s, BlockPos::new(x, y, z)) == "minecraft:fire" {
                    fires += 1;
                }
            }
        }
    }
    fires
}

#[test]
fn an_explosion_with_fire_asks_block_ignite_with_the_explosion_cause() {
    let mut s = waiting_scene(vec![
        deciding(1, EventKind::ExplosionPrime),
        observe(2, EventKind::BlockIgnite),
    ]);
    blow_with_fire(&mut s);
    let ignites = settled_for(&s.plugin, EventKind::BlockIgnite);
    assert!(!ignites.is_empty());
    for ignite in &ignites {
        let EventPayload::BlockIgnite(event) = &ignite.payload else {
            panic!("expected a BlockIgnite");
        };
        assert_eq!(event.cause, BlockIgniteCause::Explosion);
        assert!(event.igniting_entity.is_some());
    }
    assert_eq!(fires(&mut s), ignites.len());
}

#[test]
fn a_cancelled_explosion_ignition_keeps_the_air() {
    let mut s = waiting_scene(vec![
        deciding(1, EventKind::ExplosionPrime),
        deciding(2, EventKind::BlockIgnite),
    ]);
    blow_with_fire(&mut s);
    let asked = asked_for(&s.plugin, EventKind::BlockIgnite);
    assert!(!asked.is_empty());
    for ignite in &asked {
        judge(&mut s.server, 1, ignite.id, 2, true, None);
    }
    assert_eq!(fires(&mut s), 0);
}
