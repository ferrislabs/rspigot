mod common;

use common::*;
use rspigot_core::events::{ClientEvent, Inbound};
use rspigot_domain::{BlockPos, EntityId};
use rspigot_entities::EntityKind as Kind;

fn ready() -> Scene {
    let server = configured_level(|_| {}, |level| level.day_time = 18_000).with_rng_seed(3);
    let mut s = scene_on(server, &["Alice"], Vec::new());
    stream_chunk_of(&mut s, BlockPos::new(1, 64, 1));
    s.tick(5);
    s
}

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

fn heard(s: &Scene) -> Vec<String> {
    s.players[0]
        .link
        .take()
        .into_iter()
        .filter_map(|event| match event {
            ClientEvent::NamedSound { sound, .. } => Some(sound),
            ClientEvent::Sound { sound, .. } => Some(sound.to_owned()),
            _ => None,
        })
        .collect()
}

const IRON_SPEAR: &str = "equipment:{mainhand:{id:\"minecraft:iron_spear\",count:1}}";

fn alice(s: &Scene) -> (f64, f64, f64) {
    let at = s.server.player_position(s.players[0].id).unwrap();
    (at.x, at.y, at.z)
}

#[test]
fn a_spear_zombie_charges_a_player_and_lands_a_stab() {
    let mut s = ready();
    let (x, y, z) = alice(&s);
    summon(
        &mut s,
        Kind::Zombie,
        &format!("{} {y} {}", x + 6.0, z),
        &format!("{{Silent:1b,{IRON_SPEAR}}}"),
    );
    let mut sounds = Vec::new();
    for _ in 0..400 {
        s.tick(1);
        sounds.extend(heard(&s));
        if sounds
            .iter()
            .any(|sound| sound == "minecraft:item.spear.hit")
        {
            break;
        }
    }
    assert!(
        sounds
            .iter()
            .any(|sound| sound == "minecraft:item.spear.hit"),
        "{sounds:?}"
    );
}

#[test]
fn a_zombie_with_a_sword_never_makes_a_spear_sound() {
    let mut s = ready();
    let (x, y, z) = alice(&s);
    summon(
        &mut s,
        Kind::Zombie,
        &format!("{} {y} {}", x + 6.0, z),
        "{Silent:1b,equipment:{mainhand:{id:\"minecraft:iron_sword\",count:1}}}",
    );
    let mut sounds = Vec::new();
    for _ in 0..200 {
        s.tick(1);
        sounds.extend(heard(&s));
    }
    assert!(
        !sounds.iter().any(|sound| sound.contains("spear")),
        "{sounds:?}"
    );
}
