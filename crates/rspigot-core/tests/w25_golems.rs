mod common;
#[path = "common/screen.rs"]
mod screen;
#[path = "common/seeded.rs"]
mod seeded;

use common::*;
use rspigot_blocks::BlockKind;
use rspigot_core::events::Inbound;
use rspigot_core::plugins::{Answer, BlockOrder, Order, Question};
use rspigot_domain::BlockPos;
use rspigot_entities::EntityKind as Kind;
use seeded::{Seed, contents, world};

const COLUMN: BlockPos = BlockPos::new(4, 63, 4);

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn kind_at(s: &mut Scene, pos: BlockPos) -> String {
    match s.ask(Question::Block(pos)) {
        Answer::Block(state) => state.kind,
        other => panic!("expected a block, got {other:?}"),
    }
}

fn pile(blocks: &[(BlockPos, BlockKind)]) -> Scene {
    world(
        blocks
            .iter()
            .map(|(pos, kind)| Seed::block(*pos, *kind))
            .collect(),
        Vec::new(),
    )
}

fn snow_column() -> Vec<(BlockPos, BlockKind)> {
    vec![
        (COLUMN.offset(0, 1, 0), BlockKind::SnowBlock),
        (COLUMN.offset(0, 2, 0), BlockKind::SnowBlock),
    ]
}

#[test]
fn a_carved_pumpkin_on_two_snow_blocks_builds_a_snow_golem() {
    let mut s = pile(&snow_column());
    console(&mut s, "setblock 4 66 4 carved_pumpkin");
    s.tick(2);
    let golems = s.server.entities_of(Kind::SnowGolem);
    assert_eq!(golems.len(), 1);
    assert_eq!(golems[0].position.x, 4.5);
    assert_eq!(golems[0].position.z, 4.5);
    assert!(golems[0].position.y >= 64.0 && golems[0].position.y < 64.2);
    for y in 65..=66 {
        assert_eq!(
            kind_at(&mut s, BlockPos::new(4, y, 4)),
            "minecraft:air",
            "y={y}"
        );
    }
}

#[test]
fn a_jack_o_lantern_builds_a_snow_golem_too() {
    let mut s = pile(&snow_column());
    console(&mut s, "setblock 4 66 4 jack_o_lantern");
    s.tick(2);
    assert_eq!(s.server.entities_of(Kind::SnowGolem).len(), 1);
}

#[test]
fn a_pumpkin_on_one_snow_block_builds_nothing() {
    let mut s = pile(&[(COLUMN.offset(0, 1, 0), BlockKind::SnowBlock)]);
    console(&mut s, "setblock 4 65 4 carved_pumpkin");
    s.tick(2);
    assert!(s.server.entities_of(Kind::SnowGolem).is_empty());
    assert_eq!(
        kind_at(&mut s, BlockPos::new(4, 65, 4)),
        "minecraft:carved_pumpkin"
    );
}

fn iron_tee(along_x: bool) -> Vec<(BlockPos, BlockKind)> {
    let mut blocks = vec![(COLUMN.offset(0, 1, 0), BlockKind::IronBlock)];
    for offset in -1..=1 {
        let (dx, dz) = if along_x { (offset, 0) } else { (0, offset) };
        blocks.push((COLUMN.offset(dx, 2, dz), BlockKind::IronBlock));
    }
    blocks
}

#[test]
fn a_carved_pumpkin_on_a_t_of_iron_builds_a_player_created_iron_golem() {
    let mut s = pile(&iron_tee(true));
    console(&mut s, "setblock 4 66 4 carved_pumpkin");
    s.tick(2);
    let golems = s.server.entities_of(Kind::IronGolem);
    assert_eq!(golems.len(), 1);
    let saved = s.server.saved_entity(golems[0].id).unwrap();
    assert_eq!(saved.get_bool("PlayerCreated"), Some(true));
    assert_eq!(golems[0].position.x, 4.5);
    assert!(golems[0].position.y >= 64.0 && golems[0].position.y < 64.2);
    for (pos, _) in iron_tee(true) {
        assert_eq!(kind_at(&mut s, pos), "minecraft:air");
    }
    assert_eq!(kind_at(&mut s, BlockPos::new(4, 66, 4)), "minecraft:air");
}

#[test]
fn the_iron_t_works_along_z_as_well() {
    let mut s = pile(&iron_tee(false));
    console(&mut s, "setblock 4 66 4 carved_pumpkin");
    s.tick(2);
    assert_eq!(s.server.entities_of(Kind::IronGolem).len(), 1);
}

#[test]
fn an_iron_t_with_a_block_under_the_arms_builds_nothing() {
    let mut blocks = iron_tee(true);
    blocks.push((COLUMN.offset(1, 1, 0), BlockKind::Stone));
    let mut s = pile(&blocks);
    console(&mut s, "setblock 4 66 4 carved_pumpkin");
    s.tick(2);
    assert!(s.server.entities_of(Kind::IronGolem).is_empty());
}

fn dispenser_rig(extra: &[(BlockPos, BlockKind)]) -> Scene {
    let mut seeds = vec![
        Seed::block(BlockPos::new(3, 64, 7), BlockKind::Dispenser)
            .with("facing", "east")
            .holding("minecraft:carved_pumpkin", 2),
    ];
    for (pos, kind) in extra {
        seeds.push(Seed::block(*pos, *kind));
    }
    world(seeds, Vec::new())
}

fn power(s: &mut Scene) {
    s.act(Order::Block {
        pos: BlockPos::new(3, 64, 6),
        order: BlockOrder::Set("minecraft:redstone_block".to_owned()),
    });
    s.tick(6);
}

#[test]
fn a_dispenser_places_the_pumpkin_that_completes_a_snow_golem() {
    let mut s = dispenser_rig(&[
        (BlockPos::new(4, 63, 7), BlockKind::SnowBlock),
        (BlockPos::new(4, 62, 7), BlockKind::SnowBlock),
    ]);
    power(&mut s);
    assert_eq!(s.server.entities_of(Kind::SnowGolem).len(), 1);
    let alice = s.players[0].clone();
    assert_eq!(
        contents(&mut s, &alice, BlockPos::new(3, 64, 7), 1),
        [Some(("minecraft:carved_pumpkin", 1))]
    );
}

#[test]
fn a_dispenser_places_a_lone_pumpkin_as_an_item_drop_when_no_golem_would_form() {
    let mut s = dispenser_rig(&[]);
    power(&mut s);
    assert!(s.server.entities_of(Kind::SnowGolem).is_empty());
    assert_ne!(
        kind_at(&mut s, BlockPos::new(4, 64, 7)),
        "minecraft:carved_pumpkin"
    );
}
