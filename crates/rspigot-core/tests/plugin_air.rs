mod common;

use common::*;
use rspigot_core::config::PluginSettings;
use rspigot_core::events::Inbound;
use rspigot_core::plugins::events::EntityAirChangePatch;
use rspigot_core::plugins::{EventKind, EventPatch, EventPayload, HandlerSpec};
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
    let mut s = Scene {
        server,
        plugin,
        players: vec![alice],
    };
    stream_chunk_of(&mut s, BlockPos::new(1, 64, 1));
    s
}

fn stranded_cod(s: &mut Scene) -> EntityId {
    s.server.handle(Inbound::Console(
        "summon minecraft:cod 2.5 64 2.5 {NoAI:1b}".to_owned(),
    ));
    s.server.deliver();
    s.server.entities_of(Kind::Cod)[0].id
}

fn air_of(s: &Scene, id: EntityId) -> i64 {
    s.server
        .saved_entity(id)
        .and_then(|nbt| nbt.get_i64("Air"))
        .expect("an air supply")
}

#[test]
fn a_fish_out_of_water_tells_the_plugins_each_breath_it_loses() {
    let mut s = waiting_scene(vec![observe(1, EventKind::EntityAirChange)]);
    let cod = stranded_cod(&mut s);
    s.tick(3);
    let amounts: Vec<i32> = fired(&s.plugin)
        .into_iter()
        .filter_map(|event| match event.payload {
            EventPayload::EntityAirChange(change) if change.entity.id == cod => Some(change.amount),
            _ => None,
        })
        .collect();
    assert!(amounts.len() >= 2, "{amounts:?}");
    assert_eq!(amounts[0] - amounts[1], 1);
    assert!(air_of(&s, cod) < 300);
}

#[test]
fn a_cancelled_air_change_keeps_the_air_where_it_was() {
    let mut s = waiting_scene(vec![deciding(1, EventKind::EntityAirChange)]);
    let cod = stranded_cod(&mut s);
    s.tick(3);
    let before = air_of(&s, cod);
    let asked = last_asked(&s.plugin, EventKind::EntityAirChange);
    rule_on(&mut s.server, 1, asked.id, true, None);
    assert_eq!(air_of(&s, cod), before);
}

#[test]
fn a_patched_amount_is_the_air_the_entity_ends_with() {
    let mut s = waiting_scene(vec![deciding(1, EventKind::EntityAirChange)]);
    let cod = stranded_cod(&mut s);
    s.tick(3);
    let asked = last_asked(&s.plugin, EventKind::EntityAirChange);
    rule_on(
        &mut s.server,
        1,
        asked.id,
        false,
        Some(EventPatch::EntityAirChange(EntityAirChangePatch {
            amount: Some(123),
        })),
    );
    assert_eq!(air_of(&s, cod), 123);
}
