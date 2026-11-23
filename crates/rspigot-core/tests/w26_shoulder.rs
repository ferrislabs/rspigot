mod common;

use common::*;
use rspigot_core::events::{ClientEvent, Inbound, PlayerInput};
use rspigot_domain::{BlockPos, EntityId, Rotation, Vec3};
use rspigot_entities::EntityKind as Kind;

fn ready() -> Scene {
    let server = configured_level(|_| {}, |level| level.day_time = 18_000).with_rng_seed(11);
    let mut s = scene_on(server, &["Alice"], Vec::new());
    stream_chunk_of(&mut s, BlockPos::new(1, 64, 1));
    s.tick(5);
    s
}

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn stand(s: &mut Scene) {
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Move {
            position: Some(Vec3::new(0.5, 64.0, 0.5)),
            rotation: Some(Rotation::new(0.0, 0.0)),
            on_ground: true,
        },
    );
    s.tick(1);
}

fn owned_parrot(s: &mut Scene, silent: bool) -> EntityId {
    stand(s);
    let uuid = s.players[0].id.0.as_u128();
    let parts = [
        (uuid >> 96) as u32 as i32,
        (uuid >> 64) as u32 as i32,
        (uuid >> 32) as u32 as i32,
        uuid as u32 as i32,
    ];
    let known: Vec<EntityId> = s
        .server
        .entities_of(Kind::Parrot)
        .into_iter()
        .map(|entity| entity.id)
        .collect();
    let silence = if silent { "Silent:1b," } else { "" };
    console(
        s,
        &format!(
            "summon minecraft:parrot 0.5 64 0.5 {{NoAI:1b,{silence}Tame:1b,Owner:[I;{},{},{},{}],Variant:2}}",
            parts[0], parts[1], parts[2], parts[3]
        ),
    );
    s.server
        .entities_of(Kind::Parrot)
        .into_iter()
        .map(|entity| entity.id)
        .find(|id| !known.contains(id))
        .expect("a parrot")
}

fn sounds(s: &Scene) -> Vec<String> {
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

fn on_shoulder(s: &Scene) -> (bool, bool) {
    s.server.player_shoulder_entities(s.players[0].id)
}

#[test]
fn a_spectator_does_not_carry_a_parrot() {
    let mut s = ready();
    owned_parrot(&mut s, true);
    s.tick(130);
    assert_eq!(on_shoulder(&s), (true, false));
    console(&mut s, "gamemode spectator Alice");
    s.tick(3);
    assert_eq!(on_shoulder(&s), (false, false));
    assert_eq!(s.server.entities_of(Kind::Parrot).len(), 1);
}

#[test]
fn a_shoulder_parrot_makes_ambient_or_imitation_sounds_now_and_then() {
    let mut s = ready();
    owned_parrot(&mut s, false);
    s.tick(130);
    assert_eq!(on_shoulder(&s), (true, false));
    sounds(&s);
    let mut heard = Vec::new();
    for _ in 0..40 {
        s.tick(50);
        heard.extend(sounds(&s));
    }
    assert!(
        heard.iter().any(|sound| sound.contains("parrot")),
        "{heard:?}"
    );
}

#[test]
fn a_silent_shoulder_parrot_never_makes_a_sound() {
    let mut s = ready();
    owned_parrot(&mut s, true);
    s.tick(130);
    sounds(&s);
    let mut heard = Vec::new();
    for _ in 0..20 {
        s.tick(50);
        heard.extend(sounds(&s));
    }
    assert!(
        !heard.iter().any(|sound| sound.contains("parrot")),
        "{heard:?}"
    );
}
