mod common;

use common::*;
use rspigot_core::events::{Hand, Inbound, PlayerInput};
use rspigot_core::plugins::EventKind;
use rspigot_domain::{BlockPos, EntityId, Vec3};
use rspigot_entities::EntityKind as Kind;

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn summon(s: &mut Scene, kind: Kind, at: &str, nbt: &str) -> EntityId {
    let known: Vec<EntityId> = s
        .server
        .entities_of(kind)
        .into_iter()
        .map(|entity| entity.id)
        .collect();
    console(s, &format!("summon {} {at} {nbt}", kind.name()));
    s.server
        .entities_of(kind)
        .into_iter()
        .map(|entity| entity.id)
        .find(|id| !known.contains(id))
        .expect("a new entity")
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

fn mushrooms(s: &Scene, key: &str) -> i32 {
    s.server
        .entities_of(Kind::Item)
        .into_iter()
        .filter_map(|item| s.server.saved_entity(item.id))
        .filter_map(|saved| {
            let stack = saved.get_compound("Item")?.clone();
            (stack.get_str("id") == Some(key)).then(|| stack.get_i32("count").unwrap_or(1))
        })
        .sum()
}

fn shearing(handlers: Vec<rspigot_core::plugins::HandlerSpec>, nbt: &str) -> (Scene, EntityId) {
    let plugins = rspigot_core::config::PluginSettings {
        decision_ticks: 1_000,
        ..rspigot_core::config::PluginSettings::default()
    };
    let mut s = scene_on(server(plugins), &["Alice"], handlers);
    stream_chunk_of(&mut s, BlockPos::new(1, 64, 1));
    let cow = summon(&mut s, Kind::Mooshroom, "1.5 64 0.5", nbt);
    console(&mut s, "give Alice minecraft:shears 1");
    (s, cow)
}

#[test]
fn a_red_mooshroom_gives_five_red_mushrooms_from_the_shearing_table() {
    let (mut s, cow) = shearing(Vec::new(), "{NoAI:1b}");
    interact(&mut s, cow);
    s.tick(3);
    assert_eq!(mushrooms(&s, "minecraft:red_mushroom"), 5);
    assert_eq!(mushrooms(&s, "minecraft:brown_mushroom"), 0);
}

#[test]
fn a_brown_mooshroom_gives_five_brown_mushrooms_from_the_shearing_table() {
    let (mut s, cow) = shearing(Vec::new(), "{NoAI:1b,Type:\"brown\"}");
    interact(&mut s, cow);
    s.tick(3);
    assert_eq!(mushrooms(&s, "minecraft:brown_mushroom"), 5);
    assert_eq!(mushrooms(&s, "minecraft:red_mushroom"), 0);
}

#[test]
fn a_plugin_that_cancels_the_conversion_keeps_the_mushrooms_on_the_mooshroom() {
    let (mut s, cow) = shearing(vec![deciding(1, EventKind::EntityTransform)], "{NoAI:1b}");
    interact(&mut s, cow);
    s.tick(2);
    let asked = last_asked(&s.plugin, EventKind::EntityTransform);
    rule_on(&mut s.server, 1, asked.id, true, None);
    s.tick(3);
    assert_eq!(s.server.entities_of(Kind::Mooshroom).len(), 1);
    assert_eq!(mushrooms(&s, "minecraft:red_mushroom"), 0);
}
