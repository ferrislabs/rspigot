mod common;

use common::*;
use rspigot_core::plugins::{
    Answer, EntityOrder, ItemSpec, ObjectOrder, ObjectQuestion, Order, Question,
};
use rspigot_domain::{EntityId, Vec3};

fn settled() -> Scene {
    let mut s = scene(&["Alice"]);
    s.tick(60);
    s
}

fn rocket() -> ItemSpec {
    ItemSpec::of("minecraft:firework_rocket", 1)
}

fn launch(s: &mut Scene, item: ItemSpec, velocity: Option<Vec3>, angled: bool) -> Answer {
    s.ask(Question::Object(ObjectQuestion::LaunchFirework {
        position: Vec3::new(8.5, 65.0, 8.5),
        item,
        shooter: None,
        attached: None,
        angled,
        velocity,
    }))
}

fn rocket_id(answer: Answer) -> EntityId {
    match answer {
        Answer::Spawned(view) => {
            assert_eq!(view.kind, "minecraft:firework_rocket");
            view.id
        }
        other => panic!("expected a rocket, got {other:?}"),
    }
}

fn exists(s: &mut Scene, id: EntityId) -> bool {
    matches!(s.ask(Question::Entity(id)), Answer::Entity(_))
}

#[test]
fn a_rocket_launches_rises_and_bursts_at_the_end_of_its_flight() {
    let mut s = settled();
    let id = rocket_id(launch(&mut s, rocket(), None, false));

    s.tick(5);
    let Answer::Entity(details) = s.ask(Question::Entity(id)) else {
        panic!("the rocket is flying");
    };
    assert!(details.entity.position.y > 65.0);
    assert!(exists(&mut s, id));

    s.tick(40);
    assert!(!exists(&mut s, id));
}

#[test]
fn detonating_a_rocket_bursts_it_on_the_next_tick() {
    let mut s = settled();
    let id = rocket_id(launch(&mut s, rocket(), None, false));
    s.tick(2);
    assert!(exists(&mut s, id));

    s.act(Order::Entity {
        entity: id,
        order: EntityOrder::Object(ObjectOrder::DetonateFirework),
    });
    s.tick(2);

    assert!(s.warnings().is_empty());
    assert!(!exists(&mut s, id));
}

#[test]
fn a_rocket_shot_at_an_angle_flies_along_its_velocity() {
    let mut s = settled();

    let id = rocket_id(launch(
        &mut s,
        rocket(),
        Some(Vec3::new(1.0, 0.0, 0.0)),
        true,
    ));
    s.tick(3);

    let Answer::Entity(details) = s.ask(Question::Entity(id)) else {
        panic!("the rocket is flying");
    };
    assert!(details.entity.position.x > 8.5 + 1.0);
    assert!((details.entity.position.y - 65.0).abs() < 0.5);
}

#[test]
fn a_rocket_needs_a_rocket_item_and_a_velocity_when_angled() {
    let mut s = settled();

    assert!(matches!(
        launch(&mut s, ItemSpec::of("minecraft:stick", 1), None, false),
        Answer::Failed(_)
    ));
    assert!(matches!(
        launch(&mut s, rocket(), None, true),
        Answer::Failed(_)
    ));
    s.act(Order::Entity {
        entity: EntityId(999_999),
        order: EntityOrder::Object(ObjectOrder::DetonateFirework),
    });
    assert_eq!(s.warnings().len(), 1);
}
