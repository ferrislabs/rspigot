mod common;

use common::*;
use rspigot_core::plugins::{Answer, EventKind, EventPayload, ObjectQuestion, Question};
use rspigot_domain::{EntityId, Vec3};

fn settled() -> Scene {
    let mut s = scene(&["Alice"]);
    s.tick(60);
    s
}

fn spawn(s: &mut Scene, kind: &str, position: Vec3) -> EntityId {
    let Answer::Spawned(view) = s.ask(Question::Spawn {
        kind: kind.into(),
        position,
    }) else {
        panic!("expected the spawned {kind}");
    };
    view.id
}

fn launch(
    s: &mut Scene,
    kind: &str,
    position: Vec3,
    velocity: Vec3,
    shooter: Option<EntityId>,
) -> Answer {
    s.ask(Question::Object(ObjectQuestion::LaunchProjectile {
        kind: kind.to_owned(),
        position,
        velocity,
        shooter,
    }))
}

fn launched(answer: Answer) -> EntityId {
    match answer {
        Answer::Spawned(view) => view.id,
        other => panic!("expected a launched projectile, got {other:?}"),
    }
}

fn velocity(s: &mut Scene, id: EntityId) -> Vec3 {
    match s.ask(Question::Entity(id)) {
        Answer::Entity(details) => details.velocity,
        other => panic!("expected the entity, got {other:?}"),
    }
}

fn health(s: &mut Scene, id: EntityId) -> f32 {
    match s.ask(Question::Entity(id)) {
        Answer::Entity(details) => details.health.unwrap_or(0.0),
        _ => 0.0,
    }
}

#[test]
fn each_family_of_projectile_launches_with_its_velocity() {
    let mut s = settled();
    let at = Vec3::new(8.5, 70.0, 8.5);
    for kind in [
        "minecraft:arrow",
        "minecraft:trident",
        "minecraft:snowball",
        "minecraft:egg",
        "minecraft:splash_potion",
    ] {
        let id = launched(launch(&mut s, kind, at, Vec3::new(0.0, 1.0, 2.0), None));
        assert_eq!(velocity(&mut s, id), Vec3::new(0.0, 1.0, 2.0), "{kind}");
    }
    assert!(s.warnings().is_empty());
}

#[test]
fn a_fireball_is_aimed_along_the_velocity_direction() {
    let mut s = settled();

    let id = launched(launch(
        &mut s,
        "minecraft:fireball",
        Vec3::new(8.5, 70.0, 8.5),
        Vec3::new(0.0, 0.0, 5.0),
        None,
    ));

    let moving = velocity(&mut s, id);
    assert!(moving.z > 0.0 && moving.x == 0.0 && moving.y == 0.0);
}

fn causing_of_the_hit(shooter_of_arrow: bool) -> (Option<EntityId>, EntityId) {
    let mut s = scene_with(
        &["Alice"],
        vec![observe(1, EventKind::EntityDamageByEntity)],
    );
    s.tick(60);
    let victim = spawn(&mut s, "minecraft:cow", Vec3::new(8.5, 64.0, 8.5));
    let shooter = spawn(&mut s, "minecraft:cow", Vec3::new(14.5, 64.0, 2.5));
    s.tick(2);

    launched(launch(
        &mut s,
        "minecraft:arrow",
        Vec3::new(8.5, 64.6, 5.5),
        Vec3::new(0.0, 0.0, 1.5),
        shooter_of_arrow.then_some(shooter),
    ));
    s.tick(6);

    assert!(health(&mut s, victim) < 10.0);
    let hits = settled_for(&s.plugin, EventKind::EntityDamageByEntity);
    assert_eq!(hits.len(), 1);
    let EventPayload::EntityDamageByEntity(hit) = &hits[0].payload else {
        panic!("expected a damage by entity");
    };
    (
        hit.damage_source.causing.as_ref().map(|entity| entity.id),
        shooter,
    )
}

#[test]
fn an_arrow_hurts_a_mob_and_credits_the_shooter_it_was_given() {
    let (causing, shooter) = causing_of_the_hit(true);
    assert_eq!(causing, Some(shooter));
}

#[test]
fn an_arrow_without_a_shooter_credits_nobody() {
    let (causing, shooter) = causing_of_the_hit(false);
    assert_ne!(causing, Some(shooter));
}

#[test]
fn a_shooter_that_is_a_player_is_accepted() {
    let mut s = settled();
    let alice = s.players[0].clone();
    let Answer::Player(state) = s.ask(Question::Player(alice.id)) else {
        panic!("expected the player");
    };

    let answer = launch(
        &mut s,
        "minecraft:snowball",
        Vec3::new(8.5, 70.0, 8.5),
        Vec3::new(1.0, 0.0, 0.0),
        Some(EntityId(state.player.entity_id)),
    );

    assert!(matches!(answer, Answer::Spawned(_)));
}

#[test]
fn a_non_projectile_a_gone_shooter_or_a_bad_velocity_is_refused() {
    let mut s = settled();
    let at = Vec3::new(8.5, 70.0, 8.5);

    assert!(matches!(
        launch(&mut s, "minecraft:cow", at, Vec3::new(0.0, 0.0, 1.0), None),
        Answer::Failed(_)
    ));
    assert!(matches!(
        launch(
            &mut s,
            "minecraft:arrow",
            at,
            Vec3::new(0.0, 0.0, 1.0),
            Some(EntityId(999_999))
        ),
        Answer::Failed(_)
    ));
    assert!(matches!(
        launch(
            &mut s,
            "minecraft:arrow",
            at,
            Vec3::new(f64::NAN, 0.0, 0.0),
            None
        ),
        Answer::Failed(_)
    ));
    assert!(matches!(
        launch(&mut s, "minecraft:nothing", at, Vec3::ZERO, None),
        Answer::Failed(_)
    ));
}

fn player_entity(s: &mut Scene) -> EntityId {
    let alice = s.players[0].clone();
    let Answer::Player(state) = s.ask(Question::Player(alice.id)) else {
        panic!("expected the player");
    };
    EntityId(state.player.entity_id)
}

#[test]
fn a_launched_ender_pearl_teleports_its_player_shooter_where_it_lands() {
    let mut s = settled();
    let shooter = player_entity(&mut s);

    let pearl = launched(launch(
        &mut s,
        "minecraft:ender_pearl",
        Vec3::new(12.5, 70.0, 12.5),
        Vec3::new(0.0, -0.5, 0.0),
        Some(shooter),
    ));
    s.tick(40);

    assert!(matches!(s.ask(Question::Entity(pearl)), Answer::NotFound));
    let alice = s.players[0].clone();
    let Answer::Player(state) = s.ask(Question::Player(alice.id)) else {
        panic!("expected the player");
    };
    assert!((state.player.position.x - 12.5).abs() < 1.0);
    assert!((state.player.position.z - 12.5).abs() < 1.0);
}

#[test]
fn an_ender_pearl_needs_a_player_shooter() {
    let mut s = settled();
    let cow = spawn(&mut s, "minecraft:cow", Vec3::new(3.5, 64.0, 3.5));
    let at = Vec3::new(12.5, 70.0, 12.5);

    assert!(matches!(
        launch(&mut s, "minecraft:ender_pearl", at, Vec3::ZERO, None),
        Answer::Failed(_)
    ));
    assert!(matches!(
        launch(&mut s, "minecraft:ender_pearl", at, Vec3::ZERO, Some(cow)),
        Answer::Failed(_)
    ));
}

#[test]
fn an_eye_of_ender_flies_toward_the_given_direction() {
    let mut s = settled();
    let shooter = player_entity(&mut s);

    let eye = launched(launch(
        &mut s,
        "minecraft:eye_of_ender",
        Vec3::new(8.5, 66.0, 8.5),
        Vec3::new(40.0, 0.0, 0.0),
        Some(shooter),
    ));
    s.tick(10);

    let Answer::Entity(details) = s.ask(Question::Entity(eye)) else {
        panic!("the eye is flying");
    };
    assert!(details.entity.position.x > 10.0);
    assert_eq!(details.entity.kind, "minecraft:eye_of_ender");
}
