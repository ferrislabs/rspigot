mod common;

use common::*;
use rspigot_core::events::{ClientEvent, Hand, Inbound, PlayerInput};
use rspigot_domain::{BlockPos, EntityId, Rotation, Vec3};
use rspigot_entities::EntityKind as Kind;

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn mounted() -> (Scene, EntityId) {
    let mut s = scene(&["Alice"]);
    stream_chunk_of(&mut s, BlockPos::new(1, 64, 1));
    console(&mut s, "give Alice minecraft:warped_fungus_on_a_stick 1");
    console(
        &mut s,
        "summon minecraft:strider 1.5 64 0.5 {NoAI:1b,equipment:{saddle:{id:\"minecraft:saddle\",count:1}}}",
    );
    s.tick(2);
    let strider = s
        .server
        .entities_of(Kind::Strider)
        .first()
        .map(|entity| entity.id)
        .expect("the strider");
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Interact {
            target: strider,
            hand: Hand::Main,
            sneaking: false,
            location: Vec3::new(0.0, 0.5, 0.0),
        },
    );
    s.tick(2);
    alice.link.take();
    (s, strider)
}

fn drive(s: &mut Scene, to: Vec3) -> Vec<ClientEvent> {
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::MoveVehicle {
            position: to,
            rotation: Rotation::new(90.0, 0.0),
            on_ground: true,
        },
    );
    alice.link.take()
}

fn lift(s: &mut Scene, to: Vec3) {
    drive(s, to);
}

#[test]
fn a_vehicle_held_in_the_air_for_too_long_kicks_its_rider() {
    let (mut s, _) = mounted();
    let alice = s.players[0].clone();
    lift(&mut s, Vec3::new(1.5, 70.0, 0.5));
    lift(&mut s, Vec3::new(1.5, 76.0, 0.5));
    for _ in 0..79 {
        s.tick(1);
    }
    assert_eq!(alice.link.disconnected_with(), None);
    for _ in 0..4 {
        s.tick(1);
    }
    assert_eq!(
        alice.link.disconnected_with(),
        Some(rspigot_domain::text::Component::translatable(
            "multiplayer.disconnect.flying",
            Vec::new()
        ))
    );
}

#[test]
fn a_vehicle_that_lands_again_resets_the_floating_count() {
    let (mut s, _) = mounted();
    let alice = s.players[0].clone();
    lift(&mut s, Vec3::new(1.5, 70.0, 0.5));
    lift(&mut s, Vec3::new(1.5, 76.0, 0.5));
    for _ in 0..60 {
        s.tick(1);
    }
    lift(&mut s, Vec3::new(1.5, 70.0, 0.5));
    lift(&mut s, Vec3::new(1.5, 64.0, 0.5));
    for _ in 0..100 {
        s.tick(1);
    }
    assert_eq!(alice.link.disconnected_with(), None);
}
