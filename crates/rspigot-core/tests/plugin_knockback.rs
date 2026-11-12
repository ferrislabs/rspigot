mod common;

use common::*;
use rspigot_core::config::PluginSettings;
use rspigot_core::events::{Inbound, MovementAction, PlayerInput};
use rspigot_core::plugins::events::{EntityKnockbackCause, EntityKnockbackPatch};
use rspigot_core::plugins::{
    Answer, EntityOrder, EntityView, EventKind, EventPatch, EventPayload, HandlerSpec, Order,
    Question, WorldOrder,
};
use rspigot_domain::{EntityId, Vec3};

fn waiting_scene(handlers: Vec<HandlerSpec>) -> Scene {
    let mut server = server(PluginSettings {
        decision_ticks: 1_000,
        ..PluginSettings::default()
    });
    let plugin = connect_with(&mut server, 1, "tester", handlers);
    let alice = join(&mut server, "Alice");
    server.deliver();
    alice.link.take();
    Scene {
        server,
        plugin,
        players: vec![alice],
    }
}

fn spawn(s: &mut Scene, kind: &str, position: Vec3) -> EntityView {
    let Answer::Spawned(view) = s.ask(Question::Spawn {
        kind: kind.into(),
        position,
    }) else {
        panic!("expected the spawned {kind}");
    };
    view
}

fn velocity(s: &mut Scene, id: EntityId) -> Vec3 {
    match s.ask(Question::Entity(id)) {
        Answer::Entity(details) => details.velocity,
        _ => panic!("expected the entity {id:?}"),
    }
}

type Knock = (EventKind, EntityKnockbackCause, f64, Option<EntityId>);

fn knocks(s: &Scene) -> Vec<Knock> {
    fired(&s.plugin)
        .into_iter()
        .filter_map(|event| match event.payload {
            EventPayload::EntityKnockback(knock) => {
                Some((EventKind::EntityKnockback, knock.cause, knock.force, None))
            }
            EventPayload::EntityKnockbackByEntity(knock) => Some((
                EventKind::EntityKnockbackByEntity,
                knock.cause,
                knock.force,
                Some(knock.source_entity.id),
            )),
            _ => None,
        })
        .collect()
}

fn alice_entity(s: &mut Scene) -> EntityId {
    let Answer::Player(state) = s.ask(Question::Player(s.players[0].id)) else {
        panic!("expected Alice");
    };
    EntityId(state.player.entity_id)
}

#[test]
fn a_sprinting_hit_knocks_back_twice_by_the_attacker() {
    let mut s = scene_with(&["Alice"], vec![observe(1, EventKind::EntityKnockback)]);
    let alice = s.players[0].clone();
    let cow = spawn(&mut s, "minecraft:cow", Vec3::new(1.5, 64.0, 0.5));
    s.tick(20);
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Movement(MovementAction::StartSprinting),
    );
    send_input(&mut s.server, &alice, PlayerInput::Attack(cow.id));
    let attacker = alice_entity(&mut s);
    let hits = knocks(&s);
    assert_eq!(hits.len(), 2, "{hits:?}");
    assert_eq!(
        hits[0],
        (
            EventKind::EntityKnockbackByEntity,
            EntityKnockbackCause::EntityAttack,
            0.400_000_005_960_464_5,
            Some(attacker)
        )
    );
    assert_eq!(
        hits[1],
        (
            EventKind::EntityKnockbackByEntity,
            EntityKnockbackCause::EntityAttack,
            0.5,
            Some(attacker)
        )
    );
}

#[test]
fn a_sweep_knocks_every_swept_target_twice() {
    let mut s = scene_on(
        server(PluginSettings::default()).with_rng_seed(0),
        &["Alice"],
        vec![observe(1, EventKind::EntityKnockback)],
    );
    let alice = s.players[0].clone();
    s.server.handle(Inbound::Console(
        "give Alice minecraft:diamond_sword".to_owned(),
    ));
    let first = spawn(&mut s, "minecraft:cow", Vec3::new(1.5, 64.0, 0.5));
    let second = spawn(&mut s, "minecraft:cow", Vec3::new(1.5, 64.0, 1.5));
    s.act(Order::Entity {
        entity: first.id,
        order: EntityOrder::SetVelocity(Vec3::ZERO),
    });
    s.tick(30);
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Move {
            position: Some(Vec3::new(0.5, 64.0, 0.5)),
            rotation: None,
            on_ground: true,
        },
    );
    send_input(&mut s.server, &alice, PlayerInput::Attack(first.id));
    let causes: Vec<EntityKnockbackCause> = knocks(&s).into_iter().map(|knock| knock.1).collect();
    assert!(
        causes.contains(&EntityKnockbackCause::SweepAttack),
        "{causes:?}"
    );
    let swept: Vec<Knock> = fired(&s.plugin)
        .into_iter()
        .filter_map(|event| match event.payload {
            EventPayload::EntityKnockbackByEntity(knock) if knock.entity.id == second.id => Some((
                EventKind::EntityKnockbackByEntity,
                knock.cause,
                knock.force,
                Some(knock.source_entity.id),
            )),
            _ => None,
        })
        .collect();
    assert_eq!(swept.len(), 2, "{swept:?}");
    assert_eq!(swept[0].1, EntityKnockbackCause::EntityAttack);
    assert_eq!(swept[1].1, EntityKnockbackCause::SweepAttack);
}

#[test]
fn a_patched_knockback_sets_the_velocity_and_a_cancelled_one_keeps_it() {
    let mut s = waiting_scene(vec![deciding(1, EventKind::EntityKnockback)]);
    let alice = s.players[0].clone();
    let cow = spawn(&mut s, "minecraft:cow", Vec3::new(1.5, 64.0, 0.5));
    s.tick(1);
    send_input(&mut s.server, &alice, PlayerInput::Attack(cow.id));
    let asked = last_asked(&s.plugin, EventKind::EntityKnockbackByEntity);
    let EventPayload::EntityKnockbackByEntity(knock) = &asked.payload else {
        panic!("a knockback by entity");
    };
    assert_eq!(knock.cause, EntityKnockbackCause::EntityAttack);
    assert_eq!(knock.entity.id, cow.id);
    assert_eq!(knock.source_entity.id, alice_entity(&mut s));
    let before = velocity(&mut s, cow.id);
    assert_ne!(knock.final_knockback, before);
    let wanted = Vec3::new(0.0, 0.7, 0.0);
    rule_on(
        &mut s.server,
        1,
        asked.id,
        false,
        Some(EventPatch::EntityKnockback(EntityKnockbackPatch {
            final_knockback: Some(wanted),
        })),
    );
    assert_eq!(velocity(&mut s, cow.id), wanted);
    s.tick(20);
    let resting = velocity(&mut s, cow.id);
    send_input(&mut s.server, &alice, PlayerInput::Attack(cow.id));
    let asked = last_asked(&s.plugin, EventKind::EntityKnockbackByEntity);
    rule_on(&mut s.server, 1, asked.id, true, None);
    assert_eq!(velocity(&mut s, cow.id), resting);
}

#[test]
fn a_cancelled_explosion_push_leaves_the_velocity() {
    let mut s = waiting_scene(vec![deciding(1, EventKind::EntityKnockback)]);
    let cow = spawn(&mut s, "minecraft:cow", Vec3::new(3.5, 64.0, 0.5));
    s.tick(1);
    let before = velocity(&mut s, cow.id);
    s.act(Order::World(WorldOrder::Explode {
        position: Vec3::new(1.5, 64.0, 0.5),
        power: 2.0,
        breaks_blocks: false,
    }));
    let asked: Vec<Fired> = asked_for(&s.plugin, EventKind::EntityKnockback)
        .into_iter()
        .filter(|event| {
            matches!(&event.payload, EventPayload::EntityKnockback(knock)
                if knock.cause == EntityKnockbackCause::Explosion && knock.entity.id == cow.id)
        })
        .collect();
    assert_eq!(asked.len(), 1);
    for event in asked_for(&s.plugin, EventKind::EntityKnockback) {
        rule_on(&mut s.server, 1, event.id, true, None);
    }
    assert_eq!(velocity(&mut s, cow.id), before);
}
