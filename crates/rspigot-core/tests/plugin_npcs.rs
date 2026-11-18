mod common;

use common::*;
use rspigot_core::events::{ClientEvent, EntityKind, MetadataValue};
use rspigot_core::plugins::{
    ActionErrorKind, Answer, FromPlugin, ItemSpec, NpcOrder, NpcSpec, NpcView, Order, Question,
    ToPlugin,
};
use rspigot_domain::player::PlayerId;
use rspigot_domain::{Rotation, Vec3};

fn position_of(s: &mut Scene, player: &Player) -> Vec3 {
    let Answer::Player(state) = s.ask(Question::Player(player.id)) else {
        panic!("expected the state of the player");
    };
    state.player.position
}

fn spec(key: &str, name: &str, position: Vec3) -> NpcSpec {
    NpcSpec {
        key: key.to_owned(),
        name: name.to_owned(),
        position,
        rotation: None,
        skin: Some(("dGV4dHVyZXM=".to_owned(), "c2ln".to_owned())),
        skin_layers: None,
    }
}

fn create(s: &mut Scene, spec: NpcSpec) -> Answer {
    s.ask(Question::CreateNpc(spec))
}

fn created(s: &mut Scene, spec: NpcSpec) -> NpcView {
    let Answer::Npc(view) = create(s, spec) else {
        panic!("expected the created npc");
    };
    view
}

fn act(s: &mut Scene, npc: &NpcView, order: NpcOrder) {
    s.server
        .handle(rspigot_core::events::Inbound::PluginMessage {
            connection: 1,
            message: FromPlugin::Order(Order::Npc { npc: npc.id, order }),
        });
    s.server.deliver();
}

fn nearby(s: &mut Scene) -> (Player, Vec3) {
    let alice = s.players[0].clone();
    let at = position_of(s, &alice);
    (alice, Vec3::new(at.x + 3.0, at.y, at.z))
}

fn until_seen(s: &mut Scene, viewer: &Player) -> Vec<ClientEvent> {
    let mut seen = Vec::new();
    for _ in 0..400 {
        s.tick(1);
        seen.extend(viewer.link.take());
        if seen
            .iter()
            .any(|event| matches!(event, ClientEvent::SpawnEntity { .. }))
        {
            break;
        }
    }
    seen
}

#[test]
fn a_nearby_viewer_gets_the_npc_unlisted_with_its_skin_layers() {
    let mut s = scene(&["Alice"]);
    let (alice, at) = nearby(&mut s);
    let npc = created(&mut s, spec("agent-7", "Scout", at));
    let events = until_seen(&mut s, &alice);

    let entry = events
        .iter()
        .find_map(|event| match event {
            ClientEvent::PlayerInfo(entries) => {
                entries.iter().find(|e| e.profile.id == PlayerId(npc.uuid))
            }
            _ => None,
        })
        .expect("the tab list entry of the npc");
    assert!(!entry.listed);
    assert_eq!(entry.profile.name, "Scout");
    assert_eq!(entry.profile.properties[0].name, "textures");
    assert_eq!(entry.profile.properties[0].value, "dGV4dHVyZXM=");

    let info_at = events
        .iter()
        .position(|e| matches!(e, ClientEvent::PlayerInfo(_)))
        .unwrap();
    let spawn_at = events
        .iter()
        .position(|e| matches!(e, ClientEvent::SpawnEntity { .. }))
        .unwrap();
    assert!(
        info_at < spawn_at,
        "the profile is announced before the entity"
    );
    assert!(events.iter().any(|event| matches!(
        event,
        ClientEvent::SpawnEntity { entity, uuid, kind: EntityKind::Player, .. }
            if *entity == npc.id && *uuid == npc.uuid
    )));
    let layers = events.iter().find_map(|event| match event {
        ClientEvent::EntityMetadata { entity, entries } if *entity == npc.id => entries
            .iter()
            .find(|entry| entry.index == 16)
            .map(|entry| entry.value.clone()),
        _ => None,
    });
    assert_eq!(layers, Some(MetadataValue::Byte(0x7f)));
}

#[test]
fn creating_an_existing_key_returns_the_same_npc() {
    let mut s = scene(&["Alice"]);
    let (_, at) = nearby(&mut s);
    let first = created(&mut s, spec("agent-7", "Scout", at));
    let second = created(&mut s, spec("agent-7", "Other", at));
    assert_eq!(first.id, second.id);
    assert_eq!(second.name, "Scout");
    let Answer::Npc(found) = s.ask(Question::NpcByKey("agent-7".to_owned())) else {
        panic!("expected the npc by its key");
    };
    assert_eq!(found.id, first.id);
    let Answer::Npcs(all) = s.ask(Question::Npcs) else {
        panic!("expected the list of npcs");
    };
    assert_eq!(all.len(), 1);
}

#[test]
fn a_bad_or_taken_name_is_refused() {
    let mut s = scene(&["Alice"]);
    let (_, at) = nearby(&mut s);
    for name in ["", "way_too_long_for_a_name", "has space"] {
        let Answer::Failed(error) = create(&mut s, spec("k", name, at)) else {
            panic!("{name:?} must be refused");
        };
        assert_eq!(error.kind, ActionErrorKind::InvalidArgument);
    }
    created(&mut s, spec("one", "Scout", at));
    for name in ["scout", "Alice"] {
        let Answer::Failed(error) = create(&mut s, spec("two", name, at)) else {
            panic!("{name} is already used");
        };
        assert_eq!(error.kind, ActionErrorKind::InvalidArgument);
    }
}

#[test]
fn a_plugin_holds_at_most_256_npcs() {
    let mut s = scene(&["Alice"]);
    let (_, at) = nearby(&mut s);
    for index in 0..256 {
        created(&mut s, spec(&format!("k{index}"), &format!("N{index}"), at));
    }
    let Answer::Failed(error) = create(&mut s, spec("k256", "N256", at)) else {
        panic!("the 257th npc must be refused");
    };
    assert_eq!(error.kind, ActionErrorKind::LimitReached);
}

#[test]
fn an_npc_is_not_a_player() {
    let mut s = scene(&["Alice"]);
    let (_, at) = nearby(&mut s);
    created(&mut s, spec("agent-7", "Scout", at));
    s.tick(3);
    let Answer::Players(players) = s.ask(Question::Players) else {
        panic!("expected the players");
    };
    assert_eq!(players.len(), 1);
    let Answer::Server(state) = s.ask(Question::Server) else {
        panic!("expected the server state");
    };
    assert_eq!(state.online, 1);
}

#[test]
fn a_teleport_moves_it_for_viewers_and_a_far_one_despawns_it() {
    let mut s = scene(&["Alice"]);
    let (alice, at) = nearby(&mut s);
    let npc = created(&mut s, spec("agent-7", "Scout", at));
    until_seen(&mut s, &alice);

    let near = Vec3::new(at.x, at.y, at.z + 2.0);
    act(&mut s, &npc, NpcOrder::Teleport(near));
    assert!(alice.link.take().iter().any(|event| matches!(
        event,
        ClientEvent::EntityMoved { entity, to, .. } if *entity == npc.id && *to == near
    )));

    let far = Vec3::new(at.x + 5000.0, at.y, at.z);
    act(&mut s, &npc, NpcOrder::Teleport(far));
    s.tick(2);
    let events = alice.link.take();
    assert!(events.contains(&ClientEvent::RemoveEntities(vec![npc.id])));
    assert!(events.contains(&ClientEvent::PlayerInfoRemove(vec![PlayerId(npc.uuid)])));
}

#[test]
fn removing_it_despawns_it_and_forgets_its_key() {
    let mut s = scene(&["Alice"]);
    let (alice, at) = nearby(&mut s);
    let npc = created(&mut s, spec("agent-7", "Scout", at));
    until_seen(&mut s, &alice);

    act(&mut s, &npc, NpcOrder::Remove);
    let events = alice.link.take();
    assert!(events.contains(&ClientEvent::RemoveEntities(vec![npc.id])));
    assert!(events.contains(&ClientEvent::PlayerInfoRemove(vec![PlayerId(npc.uuid)])));
    assert!(matches!(
        s.ask(Question::NpcByKey("agent-7".to_owned())),
        Answer::NotFound
    ));
    act(&mut s, &npc, NpcOrder::SwingMainHand);
    assert!(s.warnings().is_empty());
}

#[test]
fn pose_equipment_swing_and_skin_reach_the_viewer() {
    let mut s = scene(&["Alice"]);
    let (alice, at) = nearby(&mut s);
    let npc = created(&mut s, spec("agent-7", "Scout", at));
    until_seen(&mut s, &alice);

    act(&mut s, &npc, NpcOrder::SetPose("crouching".to_owned()));
    act(&mut s, &npc, NpcOrder::SwingMainHand);
    act(
        &mut s,
        &npc,
        NpcOrder::SetEquipment {
            slot: "head".to_owned(),
            item: Some(ItemSpec::of("minecraft:diamond_helmet", 1)),
        },
    );
    let events = alice.link.take();
    assert!(events.iter().any(
        |event| matches!(event, ClientEvent::EntityMetadata { entity, .. } if *entity == npc.id)
    ));
    assert!(events.iter().any(
        |event| matches!(event, ClientEvent::EntityAnimation { entity, .. } if *entity == npc.id)
    ));
    assert!(events.iter().any(|event| matches!(event, ClientEvent::EntityEquipment { entity, slots } if *entity == npc.id && slots.len() == 1)));

    act(
        &mut s,
        &npc,
        NpcOrder::SetSkin {
            value: "bmV3".to_owned(),
            signature: "c2ln".to_owned(),
        },
    );
    let events = alice.link.take();
    assert!(events.iter().any(|event| matches!(event, ClientEvent::PlayerInfo(entries) if entries[0].profile.properties[0].value == "bmV3")));
}

#[test]
fn an_invalid_pose_or_slot_warns_and_changes_nothing() {
    let mut s = scene(&["Alice"]);
    let (_, at) = nearby(&mut s);
    let npc = created(&mut s, spec("agent-7", "Scout", at));
    act(&mut s, &npc, NpcOrder::SetPose("flying".to_owned()));
    act(
        &mut s,
        &npc,
        NpcOrder::SetEquipment {
            slot: "tail".to_owned(),
            item: None,
        },
    );
    assert_eq!(s.warnings().len(), 2);
}

#[test]
fn look_at_turns_it_toward_a_point() {
    let mut s = scene(&["Alice"]);
    let (alice, at) = nearby(&mut s);
    let npc = created(&mut s, spec("agent-7", "Scout", at));
    until_seen(&mut s, &alice);

    act(
        &mut s,
        &npc,
        NpcOrder::LookAt(Vec3::new(at.x + 10.0, at.y + 1.62, at.z)),
    );
    let turned = alice.link.take().into_iter().find_map(|event| match event {
        ClientEvent::EntityMoved {
            entity,
            rotation: Some(rotation),
            ..
        } if entity == npc.id => Some(rotation),
        _ => None,
    });
    let rotation = turned.expect("the npc turns");
    assert!(
        (rotation.yaw + 90.0).abs() < 0.01,
        "east is yaw -90, got {}",
        rotation.yaw
    );
    assert!(rotation.pitch.abs() < 0.01);
}

#[test]
fn the_gaze_follows_the_nearest_player_in_range_and_ignores_the_rest() {
    let mut s = scene(&["Alice"]);
    let (alice, at) = nearby(&mut s);
    let npc = created(&mut s, spec("agent-7", "Scout", at));
    until_seen(&mut s, &alice);

    act(&mut s, &npc, NpcOrder::LookAtNearest(10.0));
    s.tick(1);
    let turned: Vec<Rotation> = alice
        .link
        .take()
        .into_iter()
        .filter_map(|event| match event {
            ClientEvent::EntityMoved {
                entity,
                rotation: Some(rotation),
                ..
            } if entity == npc.id => Some(rotation),
            _ => None,
        })
        .collect();
    assert!(!turned.is_empty(), "the npc faces Alice");
    assert!(
        (turned[0].yaw - 90.0).abs() < 5.0,
        "Alice is west of the npc, got {}",
        turned[0].yaw
    );

    act(&mut s, &npc, NpcOrder::LookAtNearest(0.0));
    alice.link.take();
    s.tick(3);
    assert!(!alice.link.take().iter().any(
        |event| matches!(event, ClientEvent::EntityMoved { entity, .. } if *entity == npc.id)
    ));
}

#[test]
fn another_plugin_cannot_order_it() {
    let mut s = scene(&["Alice"]);
    let (_, at) = nearby(&mut s);
    let npc = created(&mut s, spec("agent-7", "Scout", at));
    let intruder = connect(&mut s.server, 2, "intruder");
    order(
        &mut s.server,
        2,
        FromPlugin::Order(Order::Npc {
            npc: npc.id,
            order: NpcOrder::Remove,
        }),
    );
    assert!(heard(&intruder).iter().any(|message| matches!(message, ToPlugin::Warning(text) if text.contains("belongs to the plugin tester"))));
    assert!(matches!(s.ask(Question::Npc(npc.id)), Answer::Npc(_)));
}

fn label_spawn(events: &[ClientEvent], npc: &NpcView) -> Option<(i32, Vec3)> {
    events.iter().find_map(|event| match event {
        ClientEvent::SpawnEntity {
            entity,
            kind: EntityKind::Other(_),
            position,
            ..
        } if *entity != npc.id => Some((entity.0, *position)),
        _ => None,
    })
}

fn label_text(events: &[ClientEvent], label: i32) -> Option<String> {
    events.iter().find_map(|event| match event {
        ClientEvent::EntityMetadata { entity, entries } if entity.0 == label => entries
            .iter()
            .find(|entry| entry.index == 23)
            .and_then(|entry| match &entry.value {
                MetadataValue::Component(text) => Some(text.to_legacy()),
                _ => None,
            }),
        _ => None,
    })
}

#[test]
fn a_label_is_spawned_with_the_npc_above_its_head() {
    let mut s = scene(&["Alice"]);
    let (alice, at) = nearby(&mut s);
    let npc = created(&mut s, spec("agent-7", "Scout", at));
    act(&mut s, &npc, NpcOrder::SetLabel("§aWorking".to_owned()));
    let events = until_seen(&mut s, &alice);
    let (label, position) = label_spawn(&events, &npc).expect("the label is spawned");
    assert!((position.y - (at.y + 2.35)).abs() < 1.0e-9);
    assert_eq!(label_text(&events, label).as_deref(), Some("§aWorking"));
    assert!(events.iter().any(|event| matches!(
        event,
        ClientEvent::EntityMetadata { entity, entries }
            if entity.0 == label
                && entries.iter().any(|e| e.index == 15 && e.value == MetadataValue::Byte(3))
    )));
}

#[test]
fn a_label_follows_the_npc_changes_and_clears() {
    let mut s = scene(&["Alice"]);
    let (alice, at) = nearby(&mut s);
    let npc = created(&mut s, spec("agent-7", "Scout", at));
    until_seen(&mut s, &alice);

    act(&mut s, &npc, NpcOrder::SetLabel("Step 1".to_owned()));
    let events = alice.link.take();
    let (label, _) = label_spawn(&events, &npc).expect("a label set late is spawned");
    assert_eq!(label_text(&events, label).as_deref(), Some("Step 1"));

    act(&mut s, &npc, NpcOrder::SetLabel("Step 2".to_owned()));
    assert_eq!(
        label_text(&alice.link.take(), label).as_deref(),
        Some("Step 2")
    );

    let near = Vec3::new(at.x, at.y, at.z + 2.0);
    act(&mut s, &npc, NpcOrder::Teleport(near));
    assert!(alice.link.take().iter().any(|event| matches!(
        event,
        ClientEvent::EntityMoved { entity, to, .. }
            if entity.0 == label && (to.y - (near.y + 2.35)).abs() < 1.0e-9 && to.z == near.z
    )));

    act(&mut s, &npc, NpcOrder::SetLabel(String::new()));
    assert!(alice
        .link
        .take()
        .iter()
        .any(|event| matches!(event, ClientEvent::RemoveEntities(ids) if ids.iter().any(|id| id.0 == label))));
}

#[test]
fn removing_an_npc_removes_its_label_too() {
    let mut s = scene(&["Alice"]);
    let (alice, at) = nearby(&mut s);
    let npc = created(&mut s, spec("agent-7", "Scout", at));
    act(&mut s, &npc, NpcOrder::SetLabel("Idle".to_owned()));
    let events = until_seen(&mut s, &alice);
    let (label, _) = label_spawn(&events, &npc).expect("the label is spawned");
    act(&mut s, &npc, NpcOrder::Remove);
    assert!(alice.link.take().iter().any(|event| matches!(
        event,
        ClientEvent::RemoveEntities(ids) if ids.len() == 2 && ids.iter().any(|id| id.0 == label)
    )));
}

#[test]
fn a_hidden_viewer_loses_the_npc_while_others_keep_it() {
    let mut s = scene(&["Alice", "Bob"]);
    let (alice, at) = nearby(&mut s);
    let bob = s.players[1].clone();
    let npc = created(&mut s, spec("agent-7", "Scout", at));
    for _ in 0..400 {
        s.tick(1);
        let seen = |viewer: &Player| {
            viewer
                .link
                .take()
                .iter()
                .any(|event| matches!(event, ClientEvent::SpawnEntity { entity, .. } if *entity == npc.id))
        };
        if seen(&alice) & seen(&bob) {
            break;
        }
    }

    act(&mut s, &npc, NpcOrder::HideFrom(alice.id));
    assert!(
        alice
            .link
            .take()
            .contains(&ClientEvent::RemoveEntities(vec![npc.id]))
    );
    assert!(bob.link.take().is_empty());
    s.tick(5);
    assert!(!alice.link.take().iter().any(
        |event| matches!(event, ClientEvent::SpawnEntity { entity, .. } if *entity == npc.id)
    ));

    act(&mut s, &npc, NpcOrder::ShowTo(alice.id));
    assert!(alice.link.take().iter().any(
        |event| matches!(event, ClientEvent::SpawnEntity { entity, .. } if *entity == npc.id)
    ));
}
