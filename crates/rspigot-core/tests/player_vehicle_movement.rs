mod common;

use common::*;
use rspigot_core::events::{ClientEvent, Hand, Inbound, PlayerInput};
use rspigot_core::plugins::{BlockOrder, Order};
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

fn position(s: &Scene, strider: EntityId) -> Vec3 {
    s.server
        .entities_of(Kind::Strider)
        .into_iter()
        .find(|entity| entity.id == strider)
        .expect("the strider")
        .position
}

fn corrections(events: &[ClientEvent]) -> Vec<Vec3> {
    events
        .iter()
        .filter_map(|event| match event {
            ClientEvent::MoveVehicle { position, .. } => Some(*position),
            _ => None,
        })
        .collect()
}

#[test]
fn w23_1_a_sane_vehicle_step_is_applied_without_a_correction() {
    let (mut s, strider) = mounted();
    let events = drive(&mut s, Vec3::new(2.5, 64.0, 0.5));
    assert!(corrections(&events).is_empty());
    assert!((position(&s, strider).x - 2.5).abs() < 1e-6);
}

#[test]
fn w23_1_a_huge_vehicle_step_is_answered_with_a_move_vehicle_correction() {
    let (mut s, strider) = mounted();
    let before = position(&s, strider);
    let events = drive(&mut s, Vec3::new(60.5, 64.0, 0.5));
    assert_eq!(corrections(&events), vec![before]);
    assert_eq!(position(&s, strider), before);
}

#[test]
fn w23_1_a_vehicle_driven_through_a_wall_is_answered_with_a_correction() {
    let (mut s, strider) = mounted();
    for y in 64..=66 {
        s.act(Order::Block {
            pos: BlockPos::new(3, y, 0),
            order: BlockOrder::Set("minecraft:stone".to_owned()),
        });
    }
    let before = position(&s, strider);
    let events = drive(&mut s, Vec3::new(5.5, 64.0, 0.5));
    assert_eq!(corrections(&events), vec![before]);
    assert_eq!(position(&s, strider), before);
}

#[test]
fn w23_1_a_non_finite_vehicle_position_kicks_the_rider() {
    let (mut s, _) = mounted();
    let alice = s.players[0].clone();
    drive(&mut s, Vec3::new(f64::NAN, 64.0, 0.5));
    assert_eq!(
        alice.link.disconnected_with(),
        Some(rspigot_domain::text::Component::translatable(
            "multiplayer.disconnect.invalid_vehicle_movement",
            Vec::new()
        ))
    );
}
