mod common;

use common::*;
use rspigot_core::events::{Direction, Hand, MovementAction, PlayerInput};
use rspigot_core::plugins::events::Pose;
use rspigot_core::plugins::{
    Answer, BlockOrder, EventKind, EventPayload, InventoryOrder, ItemSpec, Order, Question,
};
use rspigot_domain::{BlockPos, EntityId, Rotation, Vec3};

const FLOOR: BlockPos = BlockPos::new(2, 63, 0);
const FOOT: BlockPos = BlockPos::new(2, 64, 0);
const HEAD: BlockPos = BlockPos::new(2, 64, 1);

fn poses(s: &Scene) -> Vec<(EntityId, Pose)> {
    settled_for(&s.plugin, EventKind::EntityPoseChange)
        .into_iter()
        .filter_map(|event| match event.payload {
            EventPayload::EntityPoseChange(change) => Some((change.entity.id, change.pose)),
            _ => None,
        })
        .collect()
}

fn night_scene() -> Scene {
    let server = configured_level(
        |config| config.rules.respawn_radius = 0,
        |level| level.day_time = 18_000,
    );
    let mut s = scene_on(
        server,
        &["Alice"],
        vec![observe(1, EventKind::EntityPoseChange)],
    );
    stream_chunk_of(&mut s, FLOOR);
    for (pos, part) in [(FOOT, "foot"), (HEAD, "head")] {
        s.act(Order::Block {
            pos,
            order: BlockOrder::SetWith {
                kind: "minecraft:red_bed".to_owned(),
                properties: vec![
                    ("part".to_owned(), part.to_owned()),
                    ("facing".to_owned(), "south".to_owned()),
                ],
            },
        });
    }
    s.players[0].link.take();
    s
}

#[test]
fn a_player_going_to_bed_announces_the_sleeping_pose_then_standing_on_leaving() {
    let mut s = night_scene();
    let alice = s.players[0].clone();
    s.tick(1);
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::UseItemOn {
            hand: Hand::Main,
            pos: FOOT,
            face: Direction::Up,
            cursor: Vec3::new(0.5, 0.5, 0.5),
            inside_block: false,
            sequence: 1,
        },
    );
    s.tick(2);
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Movement(MovementAction::LeaveBed),
    );
    s.tick(2);
    let seen: Vec<Pose> = poses(&s).into_iter().map(|(_, pose)| pose).collect();
    assert_eq!(seen, [Pose::Sleeping, Pose::Standing]);
}

#[test]
fn a_gliding_player_announces_the_fall_flying_pose() {
    let mut s = scene_with(&["Alice"], vec![observe(1, EventKind::EntityPoseChange)]);
    let alice = s.players[0].clone();
    s.act(Order::Inventory {
        player: alice.id,
        order: InventoryOrder::Set {
            slot: 38,
            item: Some(ItemSpec::of("minecraft:elytra", 1)),
        },
    });
    stream_chunk_of(&mut s, BlockPos::new(1, 64, 1));
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Move {
            position: Some(Vec3::new(0.5, 70.0, 0.5)),
            rotation: Some(Rotation::new(0.0, 0.0)),
            on_ground: false,
        },
    );
    s.tick(1);
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Movement(MovementAction::StartFallFlying),
    );
    s.tick(2);
    let alice = alice_entity(&mut s);
    assert!(poses(&s).contains(&(alice, Pose::FallFlying)));
}

fn alice_entity(s: &mut Scene) -> EntityId {
    let alice = s.players[0].id;
    match s.ask(Question::Player(alice)) {
        Answer::Player(details) => EntityId(details.player.entity_id),
        other => panic!("expected a player, got {other:?}"),
    }
}
