#[path = "blocks_common/mod.rs"]
mod blocks_common;
mod common;
#[path = "common/screen.rs"]
mod screen;

use blocks_common::*;
use common::*;
use rspigot_blocks::BlockKind;
use rspigot_core::events::{Direction, Hand, PlayerInput};
use rspigot_core::plugins::{Answer, Question};
use rspigot_domain::{BlockPos, Vec3};
use rspigot_items::ItemKind;

const SHELF: BlockPos = BlockPos::new(3, 64, 3);

fn shelf_scene(powered: &str) -> (Scene, Player) {
    let cells = Cells::default().state(
        SHELF,
        BlockKind::OakShelf,
        &[
            ("facing", "north"),
            ("powered", powered),
            ("side_chain", "unconnected"),
            ("waterlogged", "false"),
        ],
    );
    let mut s = scene_of(cells, 1, 0);
    let alice = s.players[0].clone();
    console(&mut s, "gamemode survival Alice");
    alice.link.take();
    (s, alice)
}

fn click(s: &mut Scene, player: &Player, face: Direction, x: f64) {
    send_input(
        &mut s.server,
        player,
        PlayerInput::UseItemOn {
            hand: Hand::Main,
            pos: SHELF,
            face,
            cursor: Vec3::new(x, 0.5, 0.0),
            inside_block: false,
            sequence: 1,
        },
    );
    s.tick(1);
}

fn held_count(s: &mut Scene, player: &Player, key: &str) -> i32 {
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

fn stored(s: &Scene) -> Vec<(i64, String, i64)> {
    let Some(data) = s.server.block_entity_nbt(SHELF) else {
        return Vec::new();
    };
    data.get_list("Items")
        .unwrap_or_default()
        .iter()
        .filter_map(|entry| entry.as_compound())
        .map(|entry| {
            (
                entry.get_i64("Slot").unwrap(),
                entry.get_str("id").unwrap().to_owned(),
                entry.get_i64("count").unwrap(),
            )
        })
        .collect()
}

#[test]
fn a_shelf_placed_in_the_world_gets_its_block_entity() {
    let (mut s, _) = shelf_scene("false");
    console(&mut s, "setblock 6 64 6 minecraft:oak_shelf");
    assert!(s.server.block_entity_nbt(BlockPos::new(6, 64, 6)).is_some());
}

#[test]
fn a_stack_used_on_the_front_goes_into_the_slot_under_the_cursor() {
    let (mut s, alice) = shelf_scene("false");
    console(&mut s, "give Alice minecraft:stick 5");
    click(&mut s, &alice, Direction::North, 0.9);
    assert_eq!(stored(&s), vec![(0, "minecraft:stick".to_owned(), 5)]);
    assert_eq!(held_count(&mut s, &alice, "minecraft:stick"), 0);
}

#[test]
fn an_empty_hand_takes_the_stack_back() {
    let (mut s, alice) = shelf_scene("false");
    console(&mut s, "give Alice minecraft:stick 5");
    click(&mut s, &alice, Direction::North, 0.1);
    assert_eq!(stored(&s)[0].0, 2);
    click(&mut s, &alice, Direction::North, 0.1);
    assert!(stored(&s).is_empty());
    assert_eq!(held_count(&mut s, &alice, "minecraft:stick"), 5);
}

#[test]
fn a_full_slot_swaps_with_the_held_stack() {
    let (mut s, alice) = shelf_scene("false");
    console(&mut s, "give Alice minecraft:stick 5");
    click(&mut s, &alice, Direction::North, 0.5);
    console(&mut s, "give Alice minecraft:apple 2");
    click(&mut s, &alice, Direction::North, 0.5);
    assert_eq!(stored(&s), vec![(1, "minecraft:apple".to_owned(), 2)]);
    assert_eq!(held_count(&mut s, &alice, "minecraft:stick"), 5);
}

#[test]
fn a_click_on_another_face_does_not_use_the_shelf() {
    let (mut s, alice) = shelf_scene("false");
    console(&mut s, "give Alice minecraft:stick 5");
    click(&mut s, &alice, Direction::East, 0.5);
    assert!(stored(&s).is_empty());
}

#[test]
fn a_powered_shelf_ignores_the_click() {
    let (mut s, alice) = shelf_scene("true");
    console(&mut s, "give Alice minecraft:stick 5");
    click(&mut s, &alice, Direction::North, 0.5);
    assert!(stored(&s).is_empty());
}

#[test]
fn breaking_a_shelf_drops_what_it_holds() {
    let (mut s, alice) = shelf_scene("false");
    console(&mut s, "give Alice minecraft:stick 5");
    click(&mut s, &alice, Direction::North, 0.5);
    console(&mut s, "setblock 3 64 3 air");
    s.tick(1);
    let dropped: u32 = s
        .server
        .ground_items()
        .iter()
        .filter(|(_, stack)| stack.item == ItemKind::Stick.id())
        .map(|(_, stack)| u32::from(stack.count.get()))
        .sum();
    assert_eq!(dropped, 5);
}
