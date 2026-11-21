mod atlas;
mod common;

use std::sync::Arc;

use atlas::*;
use common::*;
use rspigot_core::events::{ClickInput, Direction, Hand, PlayerInput};
use rspigot_core::plugins::{BlockOrder, Order};
use rspigot_domain::item::ItemStack;
use rspigot_domain::map_data::MapData;
use rspigot_domain::{BlockPos, Vec3};
use rspigot_items::ItemKind;

const TABLE: BlockPos = BlockPos::new(9, 64, 9);
const MAP_SLOT: i16 = 0;
const MATERIAL_SLOT: i16 = 1;
const RESULT_SLOT: i16 = 2;
const MAIN_FIRST: i16 = 3;
const HOTBAR_FIRST: i16 = 30;
const TAKE_SOUND: &str = "minecraft:ui.cartography_table.take_result";

fn stored(scale: u8, locked: bool, pixel: u8) -> MapData {
    let mut data = MapData::fresh(0.0, 0.0, scale, true, false, OVERWORLD);
    data.set_color(5, 6, pixel);
    data.locked = locked;
    data
}

fn open_table(s: &mut Scene) -> Window {
    stream_chunk_of(s, TABLE);
    s.act(Order::Block {
        pos: TABLE,
        order: BlockOrder::SetWith {
            kind: "minecraft:cartography_table".to_owned(),
            properties: Vec::new(),
        },
    });
    s.players[0].link.take();
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::UseItemOn {
            hand: Hand::Main,
            pos: TABLE,
            face: Direction::Up,
            cursor: Vec3::new(0.5, 1.0, 0.5),
            inside_block: false,
            sequence: 1,
        },
    );
    Window::opened(alice.link.take())
}

fn table_with(map: MapData, material: &str, count: u8) -> (Scene, Arc<Atlas>, Window) {
    let (mut s, atlas) = scene_with_atlas(Meadow::default());
    atlas.put_map(7, &map);
    put_in_slot(&mut s, 9, filled_stack(7));
    console(&mut s, &format!("give Alice minecraft:{material} {count}"));
    s.players[0].link.take();
    let mut window = open_table(&mut s);
    window.click(&mut s, MAIN_FIRST, 0, ClickInput::Pickup);
    window.click(&mut s, MAP_SLOT, 0, ClickInput::Pickup);
    window.click(&mut s, HOTBAR_FIRST, 0, ClickInput::Pickup);
    window.click(&mut s, MATERIAL_SLOT, 0, ClickInput::Pickup);
    (s, atlas, window)
}

fn inventory_maps(window: &Window) -> Vec<ItemStack> {
    (3..39)
        .filter_map(|slot| window.slot(slot))
        .filter(|stack| stack.item.kind() == Some(ItemKind::FilledMap))
        .collect()
}

#[test]
fn paper_offers_a_zoomed_map_and_taking_it_makes_a_blank_map_one_scale_larger() {
    let (mut s, atlas, mut window) = table_with(stored(1, false, 44), "paper", 3);

    let offered = window
        .slot(RESULT_SLOT.try_into().unwrap())
        .expect("a result");
    assert_eq!(offered.item.kind(), Some(ItemKind::FilledMap));
    assert_eq!(offered.count.get(), 1);
    assert_eq!(offered.components.get_str(POST_PROCESSING), Some("scale"));
    assert_eq!(offered.components.get_i32(MAP_ID), Some(7));

    window.click(&mut s, RESULT_SLOT, 0, ClickInput::QuickMove);

    let maps = inventory_maps(&window);
    assert_eq!(maps.len(), 1);
    assert_eq!(maps[0].components.get_str(POST_PROCESSING), None);
    let fresh = maps[0].components.get_i32(MAP_ID).expect("a map id");
    assert_ne!(fresh, 7);
    s.server.save_maps();
    let zoomed = atlas.map(fresh);
    assert_eq!(zoomed.scale, 2);
    assert!(zoomed.colors().iter().all(|c| *c == 0));
    assert!(!zoomed.locked);
    assert_eq!(atlas.map(7).scale, 1);
    assert_eq!(window.slot(MAP_SLOT as usize), None);
    assert_eq!(window.slot(RESULT_SLOT as usize), None);
    assert_eq!(
        window.slot(MATERIAL_SLOT as usize).map(|m| m.count.get()),
        Some(2)
    );
    assert_eq!(window.sounds(TAKE_SOUND), 1);
}

#[test]
fn a_glass_pane_locks_a_copy_that_keeps_the_pixels_under_a_new_id() {
    let (mut s, atlas, mut window) = table_with(stored(2, false, 44), "glass_pane", 1);

    let offered = window.slot(RESULT_SLOT as usize).expect("a result");
    assert_eq!(offered.components.get_str(POST_PROCESSING), Some("lock"));

    window.click(&mut s, RESULT_SLOT, 0, ClickInput::Pickup);

    let carried = window.carried().expect("the locked map is carried");
    assert_eq!(carried.components.get_str(POST_PROCESSING), None);
    let fresh = carried.components.get_i32(MAP_ID).expect("a map id");
    assert_ne!(fresh, 7);
    s.server.save_maps();
    let locked = atlas.map(fresh);
    assert!(locked.locked);
    assert_eq!(locked.scale, 2);
    assert_eq!(locked.colors()[5 + 6 * 128], 44);
    assert!(!atlas.map(7).locked);
    assert_eq!(window.sounds(TAKE_SOUND), 1);
}

#[test]
fn an_empty_map_clones_the_same_map_twice_even_when_it_is_locked() {
    let (mut s, atlas, mut window) = table_with(stored(4, true, 44), "map", 2);

    let offered = window.slot(RESULT_SLOT as usize).expect("a result");
    assert_eq!(offered.count.get(), 2);
    assert_eq!(offered.components.get_str(POST_PROCESSING), None);

    window.click(&mut s, RESULT_SLOT, 0, ClickInput::QuickMove);

    let maps = inventory_maps(&window);
    assert_eq!(maps.len(), 1);
    assert_eq!(maps[0].count.get(), 2);
    assert_eq!(maps[0].components.get_i32(MAP_ID), Some(7));
    assert_eq!(
        window.slot(MATERIAL_SLOT as usize).map(|m| m.count.get()),
        Some(1)
    );
    assert!(atlas.file("maps/last_id").is_none());
}

#[test]
fn the_vanilla_limits_leave_the_result_slot_empty() {
    for (map, material) in [
        (stored(4, false, 1), "paper"),
        (stored(1, true, 1), "paper"),
        (stored(1, true, 1), "glass_pane"),
    ] {
        let (_s, _atlas, window) = table_with(map, material, 1);
        assert_eq!(window.slot(RESULT_SLOT as usize), None, "{material}");
    }
}

#[test]
fn changing_the_material_changes_the_offer() {
    let (mut s, _atlas, mut window) = table_with(stored(1, false, 44), "paper", 1);
    assert!(window.slot(RESULT_SLOT as usize).is_some());
    window.click(&mut s, MATERIAL_SLOT, 0, ClickInput::Pickup);
    assert_eq!(window.slot(RESULT_SLOT as usize), None);
}

const BENCH: BlockPos = BlockPos::new(9, 64, 11);
const BENCH_MAIN_FIRST: i16 = 10;
const BENCH_HOTBAR_FIRST: i16 = 37;

fn open_bench(s: &mut Scene) -> Window {
    stream_chunk_of(s, BENCH);
    s.act(Order::Block {
        pos: BENCH,
        order: BlockOrder::SetWith {
            kind: "minecraft:crafting_table".to_owned(),
            properties: Vec::new(),
        },
    });
    s.players[0].link.take();
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::UseItemOn {
            hand: Hand::Main,
            pos: BENCH,
            face: Direction::Up,
            cursor: Vec3::new(0.5, 1.0, 0.5),
            inside_block: false,
            sequence: 1,
        },
    );
    Window::opened(alice.link.take())
}

fn bench_with(map: MapData, material: &str, count: u8) -> (Scene, Arc<Atlas>, Window) {
    let (mut s, atlas) = scene_with_atlas(Meadow::default());
    atlas.put_map(7, &map);
    put_in_slot(&mut s, 9, filled_stack(7));
    console(&mut s, &format!("give Alice minecraft:{material} {count}"));
    s.players[0].link.take();
    let mut window = open_bench(&mut s);
    window.click(&mut s, BENCH_HOTBAR_FIRST, 0, ClickInput::Pickup);
    for cell in [1, 2, 3, 4, 6, 7, 8, 9] {
        if cell <= i16::from(count) + i16::from(material == "paper") * 8 {
            window.click(&mut s, cell, 1, ClickInput::Pickup);
        }
    }
    (s, atlas, window)
}

#[test]
fn a_paper_ring_around_a_map_crafts_a_zoomed_map_under_a_new_id() {
    let (mut s, atlas, mut window) = bench_with(stored(1, false, 44), "paper", 8);
    window.click(&mut s, BENCH_MAIN_FIRST, 0, ClickInput::Pickup);
    window.click(&mut s, 5, 0, ClickInput::Pickup);

    let offered = window.slot(0).expect("a result");
    assert_eq!(offered.components.get_str(POST_PROCESSING), Some("scale"));

    window.click(&mut s, 0, 0, ClickInput::QuickMove);

    let maps: Vec<ItemStack> = (10..46)
        .filter_map(|slot| window.slot(slot))
        .filter(|stack| stack.item.kind() == Some(ItemKind::FilledMap))
        .collect();
    assert_eq!(maps.len(), 1);
    let fresh = maps[0].components.get_i32(MAP_ID).expect("a map id");
    assert_ne!(fresh, 7);
    assert_eq!(maps[0].components.get_str(POST_PROCESSING), None);
    s.server.save_maps();
    assert_eq!(atlas.map(fresh).scale, 2);
    assert!(atlas.map(fresh).colors().iter().all(|c| *c == 0));
    assert_eq!(atlas.map(7).scale, 1);
    for cell in 1..=9 {
        assert_eq!(window.slot(cell), None, "cell {cell}");
    }
}

#[test]
fn a_paper_ring_around_a_maximum_scale_or_locked_map_crafts_nothing() {
    for map in [stored(4, false, 1), stored(1, true, 1)] {
        let (mut s, _atlas, mut window) = bench_with(map, "paper", 8);
        window.click(&mut s, BENCH_MAIN_FIRST, 0, ClickInput::Pickup);
        window.click(&mut s, 5, 0, ClickInput::Pickup);
        assert_eq!(window.slot(0), None);
    }
}

#[test]
fn empty_maps_around_a_map_craft_copies_of_the_same_map() {
    let (mut s, atlas, mut window) = bench_with(stored(1, false, 44), "map", 2);
    window.click(&mut s, BENCH_MAIN_FIRST, 0, ClickInput::Pickup);
    window.click(&mut s, 5, 0, ClickInput::Pickup);

    let offered = window.slot(0).expect("a result");
    assert_eq!(offered.count.get(), 3);
    assert_eq!(offered.components.get_i32(MAP_ID), Some(7));
    assert_eq!(offered.components.get_str(POST_PROCESSING), None);

    window.click(&mut s, 0, 0, ClickInput::QuickMove);
    let copies: u32 = (10..46)
        .filter_map(|slot| window.slot(slot))
        .filter(|stack| stack.components.get_i32(MAP_ID) == Some(7))
        .map(|stack| u32::from(stack.count.get()))
        .sum();
    assert_eq!(copies, 3);
    assert!(atlas.file("maps/last_id").is_none());
}
