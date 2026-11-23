mod common;

use std::sync::atomic::{AtomicI32, Ordering};

use common::*;
use rspigot_core::events::{ClientEvent, DigAction, Direction, Inbound, PlayerInput};
use rspigot_domain::BlockPos;
use rspigot_entities::EntityKind as Kind;

static SEQUENCE: AtomicI32 = AtomicI32::new(1);
const HEART: BlockPos = BlockPos::new(6, 64, 6);

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

fn awake_heart() -> Scene {
    let mut s = ready();
    console(&mut s, "gamemode creative Alice");
    console(&mut s, "setblock 5 64 6 minecraft:pale_oak_log[axis=x]");
    console(&mut s, "setblock 7 64 6 minecraft:pale_oak_log[axis=x]");
    console(&mut s, "setblock 6 64 6 minecraft:creaking_heart[axis=x]");
    s.tick(120);
    assert_eq!(s.server.entities_of(Kind::Creaking).len(), 1);
    s
}

fn dig_heart(s: &mut Scene) {
    console(s, "tp Alice 6.5 64 9.5");
    console(s, "tp @e[type=creaking] 6.5 64 12.5");
    s.tick(2);
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Dig {
            action: DigAction::Start,
            pos: HEART,
            face: Direction::Up,
            sequence: SEQUENCE.fetch_add(1, Ordering::Relaxed),
        },
    );
    s.tick(1);
}

fn heard(s: &Scene) -> (Vec<String>, Vec<(String, i32)>) {
    let mut sounds = Vec::new();
    let mut particles = Vec::new();
    for event in s.players[0].link.take() {
        match event {
            ClientEvent::NamedSound { sound, .. } => sounds.push(sound),
            ClientEvent::Sound { sound, .. } => sounds.push(sound.to_owned()),
            ClientEvent::Particle(particle) => {
                particles.push((particle.kind.to_owned(), particle.count));
            }
            _ => {}
        }
    }
    (sounds, particles)
}

#[test]
fn breaking_the_heart_as_a_player_makes_the_creaking_twitch_before_it_crumbles() {
    let mut s = awake_heart();
    heard(&s);
    dig_heart(&mut s);
    s.tick(20);
    assert_eq!(s.server.entities_of(Kind::Creaking).len(), 1);
    let (early, _) = heard(&s);
    assert!(
        early
            .iter()
            .any(|sound| sound == "minecraft:entity.creaking.twitch"),
        "{early:?}"
    );
    s.tick(40);
    assert!(s.server.entities_of(Kind::Creaking).is_empty());
    let (late, particles) = heard(&s);
    assert!(
        late.iter()
            .any(|sound| sound == "minecraft:entity.creaking.death"),
        "{late:?}"
    );
    assert!(
        particles.contains(&("minecraft:block_crumble".to_owned(), 100)),
        "{particles:?}"
    );
    assert!(
        particles.contains(&("minecraft:block_crumble".to_owned(), 10)),
        "{particles:?}"
    );
}

#[test]
fn removing_the_heart_without_a_source_crumbles_the_creaking_at_once() {
    let mut s = awake_heart();
    heard(&s);
    console(&mut s, "setblock 6 64 6 minecraft:air");
    s.tick(5);
    assert!(s.server.entities_of(Kind::Creaking).is_empty());
    let (sounds, particles) = heard(&s);
    assert!(
        !sounds
            .iter()
            .any(|sound| sound == "minecraft:entity.creaking.twitch")
    );
    assert!(
        particles.contains(&("minecraft:block_crumble".to_owned(), 100)),
        "{particles:?}"
    );
}

#[test]
fn a_player_standing_inside_the_creaking_makes_its_heart_let_it_go() {
    let mut s = awake_heart();
    let original = s.server.entities_of(Kind::Creaking)[0].uuid;
    let alice = s.server.player_position(s.players[0].id).unwrap();
    let mut gone = false;
    for _ in 0..40 {
        console(
            &mut s,
            &format!("tp @e[type=creaking] {} {} {}", alice.x, alice.y, alice.z),
        );
        s.tick(10);
        if s.server
            .entities_of(Kind::Creaking)
            .iter()
            .all(|creaking| creaking.uuid != original)
        {
            gone = true;
            break;
        }
    }
    assert!(gone);
}

#[test]
fn a_player_next_to_the_creaking_does_not_make_it_vanish() {
    let mut s = awake_heart();
    let original = s.server.entities_of(Kind::Creaking)[0].uuid;
    s.tick(400);
    assert!(
        s.server
            .entities_of(Kind::Creaking)
            .iter()
            .any(|creaking| creaking.uuid == original)
    );
}
