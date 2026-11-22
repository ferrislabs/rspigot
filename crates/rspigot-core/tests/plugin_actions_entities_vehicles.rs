mod common;

use common::*;
use rspigot_core::plugins::{Answer, EntityOrder, ObjectOrder, ObjectQuestion, Order, Question};
use rspigot_domain::{EntityId, Vec3};

fn settled() -> Scene {
    let mut s = scene(&["Alice"]);
    s.tick(60);
    s
}

fn spawn_vehicle(s: &mut Scene, kind: &str, velocity: Option<Vec3>) -> Answer {
    s.ask(Question::Object(ObjectQuestion::SpawnVehicle {
        kind: kind.to_owned(),
        position: Vec3::new(8.5, 64.0, 8.5),
        velocity,
    }))
}

fn spawned(answer: Answer) -> (EntityId, String) {
    match answer {
        Answer::Spawned(view) => (view.id, view.kind),
        other => panic!("expected a vehicle, got {other:?}"),
    }
}

fn velocity(s: &mut Scene, id: EntityId) -> Vec3 {
    match s.ask(Question::Entity(id)) {
        Answer::Entity(details) => details.velocity,
        other => panic!("expected the entity, got {other:?}"),
    }
}

fn set_velocity(s: &mut Scene, id: EntityId, velocity: Vec3) {
    s.act(Order::Entity {
        entity: id,
        order: EntityOrder::Object(ObjectOrder::SetVehicleVelocity(velocity)),
    });
}

#[test]
fn boats_and_minecarts_spawn_by_type() {
    let mut s = settled();
    for kind in [
        "minecraft:oak_boat",
        "minecraft:birch_chest_boat",
        "minecraft:bamboo_raft",
        "minecraft:minecart",
        "minecraft:chest_minecart",
        "minecraft:furnace_minecart",
        "minecraft:hopper_minecart",
        "minecraft:tnt_minecart",
    ] {
        let (_, spawned_kind) = spawned(spawn_vehicle(&mut s, kind, None));
        assert_eq!(spawned_kind, kind);
    }
    assert!(s.warnings().is_empty());
}

#[test]
fn a_vehicle_can_spawn_moving_and_be_pushed_later() {
    let mut s = settled();
    let (id, _) = spawned(spawn_vehicle(
        &mut s,
        "minecraft:minecart",
        Some(Vec3::new(0.5, 0.0, 0.0)),
    ));
    assert_eq!(velocity(&mut s, id), Vec3::new(0.5, 0.0, 0.0));

    set_velocity(&mut s, id, Vec3::new(0.0, 0.0, -0.75));

    assert!(s.warnings().is_empty());
    assert_eq!(velocity(&mut s, id), Vec3::new(0.0, 0.0, -0.75));
}

#[test]
fn something_that_is_not_a_vehicle_is_refused_both_ways() {
    let mut s = settled();
    let Answer::Spawned(cow) = s.ask(Question::Spawn {
        kind: "minecraft:cow".into(),
        position: Vec3::new(3.5, 64.0, 3.5),
    }) else {
        panic!("expected a cow");
    };

    assert!(matches!(
        spawn_vehicle(&mut s, "minecraft:cow", None),
        Answer::Failed(_)
    ));
    assert!(matches!(
        spawn_vehicle(
            &mut s,
            "minecraft:minecart",
            Some(Vec3::new(f64::NAN, 0.0, 0.0))
        ),
        Answer::Failed(_)
    ));
    set_velocity(&mut s, cow.id, Vec3::new(1.0, 0.0, 0.0));
    assert_eq!(s.warnings().len(), 1);
}
