mod fake_host;

use fake_host::*;
use rspigot_plugin::{
    Error, ItemStack, Material, NpcPose, NpcSkin, NpcSlot, NpcSpec, Position, Rotation,
};
use rspigot_plugin_protocol::v1::{self, answer, from_plugin, npc_order, question};

const UUID: &str = "00000000-0000-0000-0000-0000000000aa";
const HERE: Position = Position {
    x: 4.5,
    y: 64.0,
    z: -2.5,
};

fn ordered(npc: i32, order: npc_order::Order) -> from_plugin::Body {
    from_plugin::Body::Npc(v1::NpcOrder {
        npc,
        order: Some(order),
    })
}

fn state() -> v1::NpcState {
    v1::NpcState {
        entity_id: 9,
        uuid: UUID.to_owned(),
        key: "agent-7".to_owned(),
        name: "Scout".to_owned(),
        world: "world".to_owned(),
        position: Some(v1::Vec3 {
            x: 4.5,
            y: 64.0,
            z: -2.5,
        }),
        rotation: Some(v1::Rotation {
            yaw: 90.0,
            pitch: 0.0,
        }),
        skin: Some(v1::NpcSkin {
            value: "dGV4".to_owned(),
            signature: "c2ln".to_owned(),
        }),
        skin_layers: 0x7f,
        pose: "crouching".to_owned(),
        gaze_range: 6.0,
        label: Some("Working".to_owned()),
        walking: true,
    }
}

#[tokio::test]
async fn creating_an_npc_sends_its_spec_and_returns_a_handle_that_orders() {
    let mut called = calling(|player| {
        Box::pin(async move {
            let npcs = player.server().world.npcs;
            let spec = NpcSpec::new("agent-7", "Scout", HERE)
                .rotation(Rotation {
                    yaw: 90.0,
                    pitch: 0.0,
                })
                .skin(NpcSkin::new("dGV4", "c2ln"))
                .skin_layers(0x7f);
            let npc = npcs.create(spec).await.unwrap();
            assert_eq!(npc.entity_id, 9);
            assert_eq!(npc.key, "agent-7");
            assert_eq!(npc.name, "Scout");
            assert_eq!(npc.uuid.to_string(), UUID);
            npc.teleport(HERE);
            npc.set_skin(&NpcSkin::new("bmV3", "c2ln"));
            npc.set_skin_layers(0x01);
            npc.set_rotation(Rotation {
                yaw: 10.0,
                pitch: 5.0,
            });
            npc.set_pose(NpcPose::Crouching);
            npc.set_equipment(
                NpcSlot::Head,
                Some(ItemStack::new(Material::DiamondHelmet, 1)),
            );
            npc.set_equipment(NpcSlot::OffHand, None);
            npc.set_label("§aWorking");
            npc.clear_label();
            npc.walk_to(HERE);
            npc.walk_to_at(HERE, 6.0);
            npc.stop_walking();
            npc.hide_from(&player);
            npc.show_to(&player);
            npc.swing_main_hand();
            npc.swing_off_hand();
            npc.look_at(HERE);
            npc.look_at_nearest(6.0);
            npc.stop_looking();
            npc.remove();
        })
    })
    .await;
    let (request, asked) = called.session.asked().await;
    let question::Question::CreateNpc(spec) = asked else {
        panic!("expected the creation of an npc");
    };
    assert_eq!(spec.key, "agent-7");
    assert_eq!(spec.name, "Scout");
    assert_eq!(spec.skin_layers, Some(0x7f));
    assert_eq!(spec.skin.unwrap().value, "dGV4");
    called
        .session
        .say(answer(request, answer::Body::Npc(state())));

    use npc_order::Order;
    let vec3 = || v1::Vec3 {
        x: 4.5,
        y: 64.0,
        z: -2.5,
    };
    let expected = [
        Order::Teleport(vec3()),
        Order::SetSkin(v1::NpcSkin {
            value: "bmV3".to_owned(),
            signature: "c2ln".to_owned(),
        }),
        Order::SetSkinLayers(1),
        Order::SetRotation(v1::Rotation {
            yaw: 10.0,
            pitch: 5.0,
        }),
        Order::SetPose("crouching".to_owned()),
        Order::SetEquipment(v1::NpcEquipment {
            slot: "head".to_owned(),
            item: Some(v1::Item {
                key: "minecraft:diamond_helmet".to_owned(),
                count: 1,
                ..v1::Item::default()
            }),
        }),
        Order::SetEquipment(v1::NpcEquipment {
            slot: "off_hand".to_owned(),
            item: None,
        }),
        Order::SetLabel("§aWorking".to_owned()),
        Order::SetLabel(String::new()),
        Order::WalkTo(v1::NpcWalk {
            target: Some(vec3()),
            speed: 0.0,
        }),
        Order::WalkTo(v1::NpcWalk {
            target: Some(vec3()),
            speed: 6.0,
        }),
        Order::StopWalking(v1::Empty {}),
        Order::HideFrom(STEVE.to_owned()),
        Order::ShowTo(STEVE.to_owned()),
        Order::SwingMainHand(v1::Empty {}),
        Order::SwingOffHand(v1::Empty {}),
        Order::LookAt(vec3()),
        Order::LookAtNearest(v1::NpcGaze { range: 6.0 }),
        Order::LookAtNearest(v1::NpcGaze { range: 0.0 }),
        Order::Remove(v1::Empty {}),
    ];
    for order in expected {
        assert_eq!(called.session.hear().await, ordered(9, order));
    }
    called.finished().await;
}

#[tokio::test]
async fn an_npc_is_found_by_key_listed_and_read_back() {
    let mut called = calling(|player| {
        Box::pin(async move {
            let npcs = player.server().world.npcs;
            let found = npcs.get("agent-7").await.unwrap().unwrap();
            assert_eq!(found.entity_id, 9);
            assert!(npcs.get("nobody").await.unwrap().is_none());
            let listed = npcs.get_npcs().await.unwrap();
            assert_eq!(listed, std::slice::from_ref(&found));
            let read = found.get_state().await.unwrap();
            assert_eq!(read.pose, NpcPose::Crouching);
            assert_eq!(read.skin_layers, 0x7f);
            assert_eq!(read.gaze_range, 6.0);
            assert_eq!(read.skin.unwrap().value, "dGV4");
            assert_eq!(read.world, "world");
            assert!(read.walking);
            assert_eq!(read.label.as_deref(), Some("Working"));
            assert!(matches!(found.get_state().await, Err(Error::NotFound)));
        })
    })
    .await;
    let (request, asked) = called.session.asked().await;
    assert_eq!(asked, question::Question::NpcByKey("agent-7".to_owned()));
    called
        .session
        .say(answer(request, answer::Body::Npc(state())));
    let (request, _) = called.session.asked().await;
    called
        .session
        .say(answer(request, answer::Body::NotFound(v1::NotFound {})));
    let (request, asked) = called.session.asked().await;
    assert_eq!(asked, question::Question::Npcs(v1::Empty {}));
    called.session.say(answer(
        request,
        answer::Body::Npcs(v1::NpcList {
            npcs: vec![state()],
        }),
    ));
    let (request, asked) = called.session.asked().await;
    assert_eq!(asked, question::Question::Npc(9));
    called
        .session
        .say(answer(request, answer::Body::Npc(state())));
    let (request, _) = called.session.asked().await;
    called
        .session
        .say(answer(request, answer::Body::NotFound(v1::NotFound {})));
    called.finished().await;
}

#[tokio::test]
async fn a_refused_creation_surfaces_as_an_action_error() {
    let mut called = calling(|player| {
        Box::pin(async move {
            let made = player
                .server()
                .world
                .npcs
                .create(NpcSpec::new("k", "bad name", HERE))
                .await;
            assert!(matches!(made, Err(Error::Action(_))));
        })
    })
    .await;
    let (request, _) = called.session.asked().await;
    called.session.say(answer(
        request,
        answer::Body::Failed(v1::ActionError {
            kind: v1::ActionErrorKind::InvalidArgument as i32,
            reason: "an npc name holds letters".to_owned(),
        }),
    ));
    called.finished().await;
}

#[tokio::test]
async fn an_npc_in_sight_is_a_target_npc() {
    use rspigot_plugin::Target;

    let mut called = calling(|player| {
        Box::pin(async move {
            let Some(Target::Npc(npc)) = player.target_entity(3.0).await.unwrap() else {
                panic!("expected an npc in sight");
            };
            assert_eq!(npc.entity_id, 9);
            assert_eq!(npc.key, "agent-7");
            npc.set_label("Hello");
        })
    })
    .await;
    let (request, asked) = called.session.asked().await;
    assert_eq!(
        asked,
        question::Question::TargetEntity(v1::TargetQuery {
            player: STEVE.to_owned(),
            max_distance: 3.0,
        })
    );
    called.session.say(answer(
        request,
        answer::Body::Target(v1::Target {
            hit: Some(v1::target::Hit::Npc(state())),
        }),
    ));
    assert_eq!(
        called.session.hear().await,
        ordered(9, npc_order::Order::SetLabel("Hello".to_owned()))
    );
    called.finished().await;
}
