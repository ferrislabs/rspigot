mod common;

use common::*;
use rspigot_core::config::PluginSettings;
use rspigot_core::events::{
    ClientEvent, DigAction, Direction, Hand, Inbound, MetadataValue, PlayerInput,
};
use rspigot_core::plugins::events::ArrowBodyCountChangePatch;
use rspigot_core::plugins::{EventKind, EventPatch, EventPayload, HandlerSpec};
use rspigot_domain::{BlockPos, EntityId, Rotation};
use rspigot_entities::EntityKind as Kind;

const AIM: Rotation = Rotation::new(0.0, 0.0);
const ARROW_COUNT: u8 = 12;

fn waiting_scene(handlers: Vec<HandlerSpec>) -> Scene {
    let mut server = server(PluginSettings {
        decision_ticks: 1_000,
        ..PluginSettings::default()
    });
    let plugin = connect_with(&mut server, 1, "tester", handlers);
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

fn target(s: &mut Scene) -> EntityId {
    console(s, "summon minecraft:zombie 0.5 64 6.5 {NoAI:1b}");
    s.server.entities_of(Kind::Zombie)[0].id
}

fn shoot(s: &mut Scene) {
    let alice = s.players[0].clone();
    console(s, "give Alice minecraft:bow");
    console(s, "give Alice minecraft:arrow 2");
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
    s.tick(15);
}

fn counts_shown(s: &Scene, entity: EntityId) -> Vec<i32> {
    s.players[0]
        .link
        .take()
        .iter()
        .filter_map(|event| match event {
            ClientEvent::EntityMetadata {
                entity: id,
                entries,
            } if *id == entity => Some(entries.clone()),
            _ => None,
        })
        .flatten()
        .filter(|entry| entry.index == ARROW_COUNT)
        .filter_map(|entry| match entry.value {
            MetadataValue::VarInt(count) => Some(count),
            _ => None,
        })
        .collect()
}

#[test]
fn an_arrow_sticking_in_a_mob_tells_the_plugins_the_old_and_new_count() {
    let mut s = waiting_scene(vec![observe(1, EventKind::ArrowBodyCountChange)]);
    let zombie = target(&mut s);
    shoot(&mut s);
    let changes: Vec<_> = fired(&s.plugin)
        .into_iter()
        .filter_map(|event| match event.payload {
            EventPayload::ArrowBodyCountChange(change) => Some(change),
            _ => None,
        })
        .collect();
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].entity.id, zombie);
    assert_eq!(
        (
            changes[0].old_amount,
            changes[0].new_amount,
            changes[0].reset
        ),
        (0, 1, false)
    );
    assert_eq!(counts_shown(&s, zombie), [1]);
}

#[test]
fn a_cancelled_count_change_leaves_the_mob_without_an_arrow() {
    let mut s = waiting_scene(vec![deciding(1, EventKind::ArrowBodyCountChange)]);
    let zombie = target(&mut s);
    shoot(&mut s);
    let asked = last_asked(&s.plugin, EventKind::ArrowBodyCountChange);
    rule_on(&mut s.server, 1, asked.id, true, None);
    s.tick(2);
    assert!(counts_shown(&s, zombie).is_empty());
}

#[test]
fn a_patched_amount_is_the_count_the_mob_carries() {
    let mut s = waiting_scene(vec![deciding(1, EventKind::ArrowBodyCountChange)]);
    let zombie = target(&mut s);
    shoot(&mut s);
    let asked = last_asked(&s.plugin, EventKind::ArrowBodyCountChange);
    rule_on(
        &mut s.server,
        1,
        asked.id,
        false,
        Some(EventPatch::ArrowBodyCountChange(
            ArrowBodyCountChangePatch {
                new_amount: Some(4),
            },
        )),
    );
    s.tick(2);
    assert_eq!(counts_shown(&s, zombie), [4]);
}
