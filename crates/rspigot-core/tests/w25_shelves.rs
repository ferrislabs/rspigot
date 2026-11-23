mod common;
#[path = "common/screen.rs"]
mod screen;
#[path = "common/seeded.rs"]
mod seeded;

use common::*;
use rspigot_blocks::BlockKind;
use rspigot_core::events::{ClientEvent, Direction, Hand, Inbound, PlayerInput};
use rspigot_core::plugins::{Answer, Question};
use rspigot_domain::{BlockPos, Vec3};
use rspigot_nbt::Compound;
use seeded::{Seed, contents, world};

const SHELF: BlockPos = BlockPos::new(3, 64, 3);
const POT: BlockPos = BlockPos::new(5, 64, 3);

fn prop(s: &mut Scene, pos: BlockPos, name: &str) -> String {
    match s.ask(Question::Block(pos)) {
        Answer::Block(state) => state
            .properties
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.clone())
            .unwrap_or_default(),
        other => panic!("expected a block, got {other:?}"),
    }
}

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn survival(s: &mut Scene) -> Player {
    let alice = s.players[0].clone();
    console(s, "gamemode survival Alice");
    alice.link.take();
    alice
}

fn click(s: &mut Scene, player: &Player, pos: BlockPos, face: Direction, cursor: Vec3) {
    send_input(
        &mut s.server,
        player,
        PlayerInput::UseItemOn {
            hand: Hand::Main,
            pos,
            face,
            cursor,
            inside_block: false,
            sequence: 1,
        },
    );
    s.tick(1);
}

fn held(s: &mut Scene, player: &Player, key: &str) -> i32 {
    match s.ask(Question::Inventory(player.id)) {
        Answer::Inventory(inventory) => inventory
            .slots
            .iter()
            .flatten()
            .filter(|item| item.key == key)
            .map(|item| item.count)
            .sum(),
        other => panic!("expected the inventory, got {other:?}"),
    }
}

fn stored_slots(s: &Scene, pos: BlockPos) -> Vec<i64> {
    s.server
        .block_entity_nbt(pos)
        .map(|data| {
            data.get_list("Items")
                .unwrap_or_default()
                .iter()
                .filter_map(|entry| entry.as_compound())
                .filter_map(|entry| entry.get_i64("Slot"))
                .collect()
        })
        .unwrap_or_default()
}

fn rig(seeds: Vec<Seed>) -> Scene {
    let mut s = world(seeds, Vec::new());
    stream_chunk_of(&mut s, SHELF);
    s.tick(2);
    s
}

fn bookshelf() -> Seed {
    Seed::block(SHELF, BlockKind::ChiseledBookshelf).with("facing", "north")
}

fn read_output(s: &Scene, comparator: BlockPos) -> Option<i32> {
    s.server
        .block_entity_nbt(comparator)
        .and_then(|data| data.get_i32("OutputSignal"))
}

#[test]
fn a_book_used_on_the_front_goes_into_the_slot_under_the_cursor() {
    let mut s = rig(vec![bookshelf()]);
    let alice = survival(&mut s);
    console(&mut s, "give Alice minecraft:book 3");
    click(
        &mut s,
        &alice,
        SHELF,
        Direction::North,
        Vec3::new(0.9, 0.75, 0.0),
    );
    assert_eq!(prop(&mut s, SHELF, "slot_0_occupied"), "true");
    assert_eq!(prop(&mut s, SHELF, "slot_1_occupied"), "false");
    assert_eq!(stored_slots(&s, SHELF), vec![0]);
    assert_eq!(held(&mut s, &alice, "minecraft:book"), 2);
}

#[test]
fn the_lower_row_and_the_middle_column_map_to_slot_four() {
    let mut s = rig(vec![bookshelf()]);
    let alice = survival(&mut s);
    console(&mut s, "give Alice minecraft:book 1");
    click(
        &mut s,
        &alice,
        SHELF,
        Direction::North,
        Vec3::new(0.5, 0.25, 0.0),
    );
    assert_eq!(prop(&mut s, SHELF, "slot_4_occupied"), "true");
}

#[test]
fn a_non_book_or_a_side_face_is_not_taken() {
    let mut s = rig(vec![bookshelf()]);
    let alice = survival(&mut s);
    console(&mut s, "give Alice minecraft:stick 1");
    click(
        &mut s,
        &alice,
        SHELF,
        Direction::North,
        Vec3::new(0.9, 0.75, 0.0),
    );
    assert!(stored_slots(&s, SHELF).is_empty());
    console(&mut s, "give Alice minecraft:book 1");
    click(
        &mut s,
        &alice,
        SHELF,
        Direction::East,
        Vec3::new(1.0, 0.75, 0.5),
    );
    assert!(stored_slots(&s, SHELF).is_empty());
}

#[test]
fn an_empty_hand_takes_the_book_back() {
    let mut s = rig(vec![
        bookshelf()
            .with("slot_0_occupied", "true")
            .holding("minecraft:book", 1),
    ]);
    let alice = survival(&mut s);
    assert_eq!(prop(&mut s, SHELF, "slot_0_occupied"), "true");
    click(
        &mut s,
        &alice,
        SHELF,
        Direction::North,
        Vec3::new(0.9, 0.75, 0.0),
    );
    assert_eq!(prop(&mut s, SHELF, "slot_0_occupied"), "false");
    assert!(stored_slots(&s, SHELF).is_empty());
    assert_eq!(held(&mut s, &alice, "minecraft:book"), 1);
}

#[test]
fn a_comparator_reads_the_last_interacted_slot_plus_one() {
    let comparator = SHELF.offset(0, 0, -1);
    let mut s = rig(vec![
        bookshelf(),
        Seed::block(comparator, BlockKind::Comparator).with("facing", "south"),
    ]);
    let alice = survival(&mut s);
    console(&mut s, "give Alice minecraft:book 2");
    click(
        &mut s,
        &alice,
        SHELF,
        Direction::North,
        Vec3::new(0.5, 0.25, 0.0),
    );
    s.tick(4);
    assert_eq!(read_output(&s, comparator), Some(5));
    click(
        &mut s,
        &alice,
        SHELF,
        Direction::North,
        Vec3::new(0.9, 0.75, 0.0),
    );
    s.tick(4);
    assert_eq!(read_output(&s, comparator), Some(1));
}

#[test]
fn a_hopper_pushes_a_book_into_the_first_free_slot_of_the_bookshelf() {
    let hopper = SHELF.offset(0, 1, 0);
    let mut s = rig(vec![
        bookshelf(),
        Seed::block(hopper, BlockKind::Hopper)
            .with("facing", "down")
            .holding("minecraft:book", 1),
    ]);
    s.tick(30);
    assert_eq!(prop(&mut s, SHELF, "slot_0_occupied"), "true");
}

#[test]
fn a_hopper_does_not_push_a_stick_into_the_bookshelf() {
    let hopper = SHELF.offset(0, 1, 0);
    let mut s = rig(vec![
        bookshelf(),
        Seed::block(hopper, BlockKind::Hopper)
            .with("facing", "down")
            .holding("minecraft:stick", 1),
    ]);
    s.tick(30);
    assert!(stored_slots(&s, SHELF).is_empty());
}

fn pot() -> Seed {
    Seed::block(POT, BlockKind::DecoratedPot).with("facing", "north")
}

fn pot_item(s: &Scene) -> Option<(String, i64)> {
    let data = s.server.block_entity_nbt(POT)?;
    let item = data.get_compound("item")?;
    Some((item.get_str("id")?.to_owned(), item.get_i64("count")?))
}

fn wobbles(player: &Player) -> Vec<u8> {
    player
        .link
        .take()
        .iter()
        .filter_map(|event| match event {
            ClientEvent::BlockEvent {
                pos,
                action: 1,
                data,
                ..
            } if *pos == POT => Some(*data),
            _ => None,
        })
        .collect()
}

#[test]
fn a_decorated_pot_takes_one_item_per_use_and_wobbles() {
    let mut s = rig(vec![pot()]);
    let alice = survival(&mut s);
    console(&mut s, "give Alice minecraft:stick 4");
    alice.link.take();
    click(&mut s, &alice, POT, Direction::Up, Vec3::new(0.5, 1.0, 0.5));
    assert_eq!(pot_item(&s), Some(("minecraft:stick".to_owned(), 1)));
    assert_eq!(held(&mut s, &alice, "minecraft:stick"), 3);
    assert_eq!(wobbles(&alice), vec![0]);
    click(&mut s, &alice, POT, Direction::Up, Vec3::new(0.5, 1.0, 0.5));
    assert_eq!(pot_item(&s), Some(("minecraft:stick".to_owned(), 2)));
}

#[test]
fn a_decorated_pot_refuses_a_different_item_with_a_negative_wobble() {
    let mut s = rig(vec![
        pot().carrying(
            "item",
            Compound::new()
                .with("id", "minecraft:stick")
                .with("count", 1),
        ),
    ]);
    let alice = survival(&mut s);
    console(&mut s, "give Alice minecraft:apple 1");
    alice.link.take();
    click(&mut s, &alice, POT, Direction::Up, Vec3::new(0.5, 1.0, 0.5));
    assert_eq!(pot_item(&s), Some(("minecraft:stick".to_owned(), 1)));
    assert_eq!(held(&mut s, &alice, "minecraft:apple"), 1);
    assert_eq!(wobbles(&alice), vec![1]);
}

#[test]
fn a_comparator_reads_the_fill_of_a_decorated_pot() {
    let comparator = POT.offset(0, 0, -1);
    let mut s = rig(vec![
        pot().carrying(
            "item",
            Compound::new()
                .with("id", "minecraft:stick")
                .with("count", 32),
        ),
        Seed::block(comparator, BlockKind::Comparator).with("facing", "south"),
    ]);
    console(&mut s, "setblock 6 64 2 minecraft:stone");
    s.tick(4);
    assert_eq!(read_output(&s, comparator), Some(8));
}

#[test]
fn a_comparator_reads_a_shelf_as_a_bit_per_occupied_slot() {
    let shelf = SHELF.offset(4, 0, 4);
    let comparator = shelf.offset(0, 0, -1);
    let mut s = rig(vec![
        Seed::block(shelf, BlockKind::OakShelf)
            .with("facing", "north")
            .holding("minecraft:stick", 1)
            .gap()
            .holding("minecraft:stick", 1),
        Seed::block(comparator, BlockKind::Comparator).with("facing", "south"),
    ]);
    console(&mut s, "setblock 6 64 6 minecraft:stone");
    s.tick(4);
    assert_eq!(read_output(&s, comparator), Some(5));
}

#[test]
fn a_comparator_reads_a_copper_golem_statue_pose() {
    let statue = SHELF.offset(0, 0, 6);
    let comparator = statue.offset(0, 0, -1);
    let mut s = rig(vec![
        Seed::block(statue, BlockKind::CopperGolemStatue).with("copper_golem_pose", "running"),
        Seed::block(comparator, BlockKind::Comparator).with("facing", "south"),
    ]);
    console(&mut s, "setblock 4 64 8 minecraft:stone");
    s.tick(4);
    assert_eq!(read_output(&s, comparator), Some(3));
}

#[test]
fn a_comparator_reads_one_from_a_candle_cake() {
    let cake = SHELF.offset(-3, 0, 6);
    let comparator = cake.offset(0, 0, -1);
    let mut s = rig(vec![
        Seed::block(cake, BlockKind::CandleCake),
        Seed::block(comparator, BlockKind::Comparator).with("facing", "south"),
    ]);
    console(&mut s, "setblock 1 64 8 minecraft:stone");
    s.tick(4);
    assert_eq!(read_output(&s, comparator), Some(1));
}

const COMPOSTER: BlockPos = BlockPos::new(1, 64, 2);

#[test]
fn a_hopper_above_a_composter_feeds_it_compostables() {
    let hopper = COMPOSTER.offset(0, 1, 0);
    let mut s = rig(vec![
        Seed::block(COMPOSTER, BlockKind::Composter),
        Seed::block(hopper, BlockKind::Hopper)
            .with("facing", "down")
            .holding("minecraft:wheat_seeds", 64),
    ]);
    s.tick(60);
    let level: i32 = prop(&mut s, COMPOSTER, "level").parse().unwrap();
    assert!(level > 0, "the composter never filled");
    let alice = s.players[0].clone();
    let left = contents(&mut s, &alice, hopper, 5);
    assert!(left[0].is_some_and(|(_, count)| count < 64));
}

#[test]
fn a_hopper_does_not_feed_a_composter_a_stick() {
    let hopper = COMPOSTER.offset(0, 1, 0);
    let mut s = rig(vec![
        Seed::block(COMPOSTER, BlockKind::Composter),
        Seed::block(hopper, BlockKind::Hopper)
            .with("facing", "down")
            .holding("minecraft:stick", 8),
    ]);
    s.tick(60);
    assert_eq!(prop(&mut s, COMPOSTER, "level"), "0");
}

#[test]
fn a_hopper_under_a_ready_composter_takes_the_bone_meal_and_empties_it() {
    let hopper = COMPOSTER.offset(0, -1, 0);
    let mut s = rig(vec![
        Seed::block(COMPOSTER, BlockKind::Composter).with("level", "8"),
        Seed::block(hopper, BlockKind::Hopper).with("facing", "south"),
    ]);
    s.tick(30);
    assert_eq!(prop(&mut s, COMPOSTER, "level"), "0");
    let alice = s.players[0].clone();
    let items = contents(&mut s, &alice, hopper, 5);
    assert_eq!(items[0], Some(("minecraft:bone_meal", 1)));
}

#[test]
fn a_comparator_reads_the_chest_minecart_on_a_powered_detector_rail() {
    let rail = SHELF.offset(-3, 0, -3);
    let comparator = rail.offset(0, 0, -1);
    let mut s = rig(vec![
        Seed::block(rail, BlockKind::DetectorRail).with("shape", "north_south"),
        Seed::block(comparator, BlockKind::Comparator).with("facing", "south"),
    ]);
    console(
        &mut s,
        "summon minecraft:chest_minecart 0.5 64.0 0.5 {Items:[{Slot:0b,id:\"minecraft:cobblestone\",count:64},{Slot:1b,id:\"minecraft:cobblestone\",count:64},{Slot:2b,id:\"minecraft:cobblestone\",count:64}]}",
    );
    s.tick(30);
    assert_eq!(read_output(&s, comparator), Some(2));
}

#[test]
fn a_hopper_above_a_decorated_pot_drops_its_item_into_it() {
    let hopper = POT.offset(0, 1, 0);
    let mut s = rig(vec![
        pot(),
        Seed::block(hopper, BlockKind::Hopper)
            .with("facing", "down")
            .holding("minecraft:stick", 2),
    ]);
    s.tick(40);
    let stored = pot_item(&s).expect("the pot took an item");
    assert_eq!(stored.0, "minecraft:stick");
    assert!(stored.1 >= 1);
}
