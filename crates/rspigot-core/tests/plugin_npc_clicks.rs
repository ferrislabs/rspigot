mod common;

use common::*;
use rspigot_core::events::{ClientEvent, Hand, PlayerInput};
use rspigot_core::plugins::{Answer, EventKind, EventPayload, NpcSpec, NpcView, Question};
use rspigot_domain::Vec3;

fn watched() -> Scene {
    scene_with(
        &["Alice"],
        vec![
            observe(1, EventKind::PlayerInteractAtEntity),
            deciding(2, EventKind::EntityDamageByEntity),
        ],
    )
}

fn put_npc(s: &mut Scene, east: f64) -> (Player, NpcView) {
    let alice = s.players[0].clone();
    let Answer::Player(state) = s.ask(Question::Player(alice.id)) else {
        panic!("expected the state of the player");
    };
    let at = state.player.position;
    let Answer::Npc(npc) = s.ask(Question::CreateNpc(NpcSpec {
        key: "agent-7".to_owned(),
        name: "Scout".to_owned(),
        position: Vec3::new(at.x + east, at.y, at.z),
        rotation: None,
        skin: None,
        skin_layers: None,
    })) else {
        panic!("expected the created npc");
    };
    for _ in 0..400 {
        s.tick(1);
        if alice.link.take().iter().any(
            |event| matches!(event, ClientEvent::SpawnEntity { entity, .. } if *entity == npc.id),
        ) {
            break;
        }
    }
    (alice, npc)
}

fn right_click(s: &mut Scene, alice: &Player, npc: &NpcView) {
    send_input(
        &mut s.server,
        alice,
        PlayerInput::Interact {
            target: npc.id,
            hand: Hand::Main,
            sneaking: false,
            location: Vec3::new(0.0, 0.9, 0.0),
        },
    );
    s.tick(2);
}

#[test]
fn a_right_click_on_an_npc_reaches_the_plugin() {
    let mut s = watched();
    let (alice, npc) = put_npc(&mut s, 2.0);
    right_click(&mut s, &alice, &npc);
    let clicks: Vec<_> = fired(&s.plugin)
        .into_iter()
        .filter_map(|event| match event.payload {
            EventPayload::PlayerInteractAtEntity(click) => Some(click),
            _ => None,
        })
        .collect();
    assert_eq!(clicks.len(), 1, "one click event");
    assert_eq!(clicks[0].right_clicked.id, npc.id);
    assert_eq!(clicks[0].right_clicked.kind, "minecraft:player");
    assert_eq!(clicks[0].player.name, "Alice");
}

#[test]
fn a_right_click_out_of_reach_is_ignored() {
    let mut s = watched();
    let (alice, npc) = put_npc(&mut s, 12.0);
    right_click(&mut s, &alice, &npc);
    assert!(
        fired(&s.plugin)
            .iter()
            .all(|event| !matches!(event.payload, EventPayload::PlayerInteractAtEntity(_)))
    );
}

#[test]
fn a_hit_on_an_npc_fires_a_damage_event_and_hurts_nothing() {
    let mut s = watched();
    let (alice, npc) = put_npc(&mut s, 2.0);
    send_input(&mut s.server, &alice, PlayerInput::Attack(npc.id));
    s.tick(2);
    let hits: Vec<_> = fired(&s.plugin)
        .into_iter()
        .filter_map(|event| match event.payload {
            EventPayload::EntityDamageByEntity(hit) => Some(hit),
            _ => None,
        })
        .collect();
    assert_eq!(hits.len(), 1, "one damage event");
    assert_eq!(hits[0].entity.id, npc.id);
    assert_eq!(
        hits[0].damager.player.as_ref().map(|p| p.name.as_str()),
        Some("Alice")
    );
    assert!(hits[0].damage > 0.0);
    assert!(
        !alice.link.take().iter().any(
            |event| matches!(event, ClientEvent::EntityStatus { entity, .. } if *entity == npc.id)
        ),
        "the npc shows no hurt"
    );
    assert!(matches!(s.ask(Question::Npc(npc.id)), Answer::Npc(_)));
}

#[test]
fn a_hit_out_of_reach_fires_nothing() {
    let mut s = watched();
    let (alice, npc) = put_npc(&mut s, 12.0);
    send_input(&mut s.server, &alice, PlayerInput::Attack(npc.id));
    s.tick(2);
    assert!(
        fired(&s.plugin)
            .iter()
            .all(|event| !matches!(event.payload, EventPayload::EntityDamageByEntity(_)))
    );
}
