mod common;

use common::*;
use rspigot_blocks::{Block, Stone};
use rspigot_core::events::{ClientEvent, EntityKind};
use rspigot_core::plugins::{ActionError, ActionErrorKind, Answer, Question};
use rspigot_domain::{BlockPos, EntityId, Vec3};

const FALLING_BLOCK: &str = "minecraft:falling_block";
const LAUNCH: Vec3 = Vec3::new(1.0, 0.5, 0.0);
const ORIGIN: Vec3 = Vec3::new(4.5, 66.0, 4.5);

fn spawn_with(s: &mut Scene, kind: &str, block: Option<&str>, velocity: Option<Vec3>) -> Answer {
    s.ask(Question::SpawnWith {
        kind: kind.into(),
        position: ORIGIN,
        block: block.map(str::to_owned),
        velocity,
    })
}

fn stone_cells(s: &mut Scene) -> Vec<BlockPos> {
    (0..80)
        .map(|x| BlockPos::new(x, 64, 4))
        .filter(|&pos| match s.ask(Question::Block(pos)) {
            Answer::Block(state) => state.kind == "minecraft:stone",
            _ => false,
        })
        .collect()
}

fn invalid(answer: Answer) -> bool {
    matches!(
        answer,
        Answer::Failed(ActionError {
            kind: ActionErrorKind::InvalidArgument,
            ..
        })
    )
}

fn falling_data(s: &Scene, id: EntityId) -> Option<i32> {
    s.players[0]
        .link
        .take()
        .iter()
        .find_map(|event| match event {
            ClientEvent::SpawnEntity {
                entity,
                kind: EntityKind::FallingBlock,
                data,
                ..
            } if *entity == id => Some(*data),
            _ => None,
        })
}

#[test]
fn a_thrown_falling_block_flies_then_lands_as_the_requested_block() {
    let mut s = scene(&["Alice"]);
    stream_chunk_of(&mut s, ORIGIN.block());
    let Answer::Spawned(view) =
        spawn_with(&mut s, FALLING_BLOCK, Some("minecraft:stone"), Some(LAUNCH))
    else {
        panic!("expected the spawned falling block");
    };
    assert_eq!(view.kind, FALLING_BLOCK);
    assert_eq!(
        falling_data(&s, view.id),
        Some(i32::from(Stone::DEFAULT_STATE.0))
    );
    let Answer::Entity(details) = s.ask(Question::Entity(view.id)) else {
        panic!("expected the details of the falling block");
    };
    assert_eq!(details.velocity, LAUNCH);
    assert!(stone_cells(&mut s).is_empty());
    s.tick(200);
    assert!(matches!(s.ask(Question::Entity(view.id)), Answer::NotFound));
    let landed = stone_cells(&mut s);
    assert_eq!(landed.len(), 1);
    assert!(landed[0].x > 4);
    assert!(s.warnings().is_empty());
}

#[test]
fn a_block_is_refused_for_any_other_kind_or_unknown_key() {
    let mut s = scene(&["Alice"]);
    stream_chunk_of(&mut s, ORIGIN.block());
    assert!(invalid(spawn_with(
        &mut s,
        "minecraft:cow",
        Some("minecraft:stone"),
        None
    )));
    assert!(invalid(spawn_with(
        &mut s,
        FALLING_BLOCK,
        Some("minecraft:nonsense"),
        None
    )));
    assert!(invalid(spawn_with(
        &mut s,
        FALLING_BLOCK,
        Some("minecraft:air"),
        None
    )));
}

#[test]
fn an_absurd_launch_velocity_is_clamped_per_axis() {
    let mut s = scene(&["Alice"]);
    stream_chunk_of(&mut s, ORIGIN.block());
    let Answer::Spawned(view) = spawn_with(
        &mut s,
        FALLING_BLOCK,
        Some("minecraft:stone"),
        Some(Vec3::new(100.0, -100.0, 0.0)),
    ) else {
        panic!("expected the spawned falling block");
    };
    let Answer::Entity(details) = s.ask(Question::Entity(view.id)) else {
        panic!("expected the details of the falling block");
    };
    assert_eq!(details.velocity, Vec3::new(3.9, -3.9, 0.0));
}
