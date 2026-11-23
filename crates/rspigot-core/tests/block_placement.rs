#[path = "blocks_common/mod.rs"]
mod blocks_common;
mod common;

use std::sync::atomic::{AtomicI32, Ordering};

use blocks_common::*;
use common::*;
use rspigot_blocks::BlockKind;
use rspigot_core::events::{DigAction, Direction, Hand, PlayerInput};
use rspigot_domain::{BlockPos, Vec3};

static SEQUENCE: AtomicI32 = AtomicI32::new(1);

fn floor(x: i32, z: i32) -> BlockPos {
    BlockPos::new(x, 63, z)
}

fn above(x: i32, z: i32) -> BlockPos {
    BlockPos::new(x, 64, z)
}

fn start(cells: Cells) -> (Scene, Player) {
    let mut s = scene_of(cells, 7, 0);
    let alice = s.players[0].clone();
    console(&mut s, "gamemode creative Alice");
    alice.link.take();
    (s, alice)
}

fn hold(s: &mut Scene, item: &str) {
    console(s, "clear Alice");
    console(s, &format!("give Alice {item} 1"));
}

fn use_on(
    s: &mut Scene,
    alice: &Player,
    item: &str,
    at: BlockPos,
    face: Direction,
    cursor: (f64, f64, f64),
) {
    hold(s, item);
    send_input(
        &mut s.server,
        alice,
        PlayerInput::UseItemOn {
            hand: Hand::Main,
            pos: at,
            face,
            cursor: Vec3::new(cursor.0, cursor.1, cursor.2),
            inside_block: false,
            sequence: SEQUENCE.fetch_add(1, Ordering::Relaxed),
        },
    );
    s.tick(2);
}

fn on_floor(s: &mut Scene, alice: &Player, item: &str, x: i32, z: i32) {
    use_on(s, alice, item, floor(x, z), Direction::Up, (0.5, 1.0, 0.5));
}

fn dig(s: &mut Scene, alice: &Player, at: BlockPos) {
    send_input(
        &mut s.server,
        alice,
        PlayerInput::Dig {
            action: DigAction::Start,
            pos: at,
            face: Direction::Up,
            sequence: SEQUENCE.fetch_add(1, Ordering::Relaxed),
        },
    );
    s.tick(2);
}

fn prop(s: &Scene, at: BlockPos, name: &str) -> Option<String> {
    property_at(s, at, name)
}

fn items_on_ground(s: &Scene) -> u32 {
    s.server
        .ground_items()
        .iter()
        .map(|(_, stack)| u32::from(stack.count.get()))
        .sum()
}

#[test]
fn w22_1_a_plant_is_only_placed_on_a_block_that_supports_it() {
    let cells = Cells::default().block(floor(6, 6), BlockKind::GrassBlock);
    let (mut s, alice) = start(cells);
    on_floor(&mut s, &alice, "poppy", 5, 6);
    assert_eq!(kind_at(&s, above(5, 6)), Some(BlockKind::Air));
    on_floor(&mut s, &alice, "poppy", 6, 6);
    assert_eq!(kind_at(&s, above(6, 6)), Some(BlockKind::Poppy));
    on_floor(&mut s, &alice, "oak_pressure_plate", 5, 6);
    assert_eq!(kind_at(&s, above(5, 6)), Some(BlockKind::OakPressurePlate));
    use_on(
        &mut s,
        &alice,
        "torch",
        above(5, 6),
        Direction::Up,
        (0.5, 1.0, 0.5),
    );
    assert_ne!(kind_at(&s, above(5, 7)), Some(BlockKind::Torch));
}

#[test]
fn w22_1_breaking_the_support_breaks_the_block_and_drops_its_item() {
    let cells = Cells::default()
        .block(above(5, 5), BlockKind::Torch)
        .block(above(7, 5), BlockKind::WhiteCarpet)
        .block(above(5, 7), BlockKind::Rail)
        .block(above(7, 7), BlockKind::OakPressurePlate)
        .state(above(3, 5), BlockKind::OakSign, &[("rotation", "0")])
        .block(above(3, 7), BlockKind::Cactus);
    let (mut s, _alice) = start(cells);
    for (x, z) in [(5, 5), (7, 5), (5, 7), (7, 7), (3, 5)] {
        assert_ne!(kind_at(&s, above(x, z)), Some(BlockKind::Air));
    }
    for (x, z) in [(5, 5), (7, 5), (5, 7), (7, 7), (3, 5), (3, 7)] {
        console(&mut s, &format!("setblock {x} 63 {z} minecraft:air"));
    }
    s.tick(2);
    for (x, z) in [(5, 5), (7, 5), (5, 7), (7, 7), (3, 5), (3, 7)] {
        assert_eq!(
            kind_at(&s, above(x, z)),
            Some(BlockKind::Air),
            "the block at {x},{z} should have broken"
        );
    }
    assert!(items_on_ground(&s) >= 5);
}

#[test]
fn w22_2_fences_walls_and_panes_connect_at_placement_and_on_neighbour_change() {
    let cells = Cells::default()
        .block(above(6, 5), BlockKind::OakFence)
        .block(above(8, 5), BlockKind::Stone)
        .block(above(4, 5), BlockKind::CobblestoneWall)
        .block(above(2, 5), BlockKind::CobblestoneWall);
    let (mut s, alice) = start(cells);
    on_floor(&mut s, &alice, "oak_fence", 7, 5);
    assert_eq!(prop(&s, above(7, 5), "west").as_deref(), Some("true"));
    assert_eq!(prop(&s, above(7, 5), "east").as_deref(), Some("true"));
    assert_eq!(prop(&s, above(6, 5), "east").as_deref(), Some("true"));
    console(&mut s, "setblock 7 64 5 minecraft:air");
    s.tick(2);
    assert_eq!(prop(&s, above(6, 5), "east").as_deref(), Some("false"));
    on_floor(&mut s, &alice, "cobblestone_wall", 3, 5);
    assert_eq!(prop(&s, above(3, 5), "west").as_deref(), Some("low"));
    assert_eq!(prop(&s, above(3, 5), "east").as_deref(), Some("low"));
    assert_eq!(prop(&s, above(3, 5), "up").as_deref(), Some("false"));
    assert_eq!(prop(&s, above(4, 5), "west").as_deref(), Some("low"));
    on_floor(&mut s, &alice, "glass_pane", 4, 3);
    on_floor(&mut s, &alice, "glass_pane", 5, 3);
    assert_eq!(prop(&s, above(4, 3), "east").as_deref(), Some("true"));
    assert_eq!(prop(&s, above(5, 3), "west").as_deref(), Some("true"));
    on_floor(&mut s, &alice, "iron_bars", 6, 3);
    assert_eq!(prop(&s, above(5, 3), "east").as_deref(), Some("true"));
}

#[test]
fn w22_2_stairs_take_a_corner_shape_from_their_neighbours() {
    let cells = Cells::default().state(
        above(6, 5),
        BlockKind::OakStairs,
        &[("facing", "east"), ("half", "bottom")],
    );
    let (mut s, alice) = start(cells);
    on_floor(&mut s, &alice, "oak_stairs", 6, 6);
    assert_eq!(prop(&s, above(6, 6), "facing").as_deref(), Some("south"));
    assert_eq!(
        prop(&s, above(6, 6), "shape").as_deref(),
        Some("inner_left")
    );
}

#[test]
fn w22_2_rails_curve_and_climb_toward_their_neighbours() {
    let cells = Cells::default()
        .state(
            above(7, 6),
            BlockKind::Rail,
            &[("shape", "east_west"), ("waterlogged", "false")],
        )
        .state(
            above(6, 7),
            BlockKind::Rail,
            &[("shape", "north_south"), ("waterlogged", "false")],
        )
        .block(above(5, 3), BlockKind::Stone)
        .state(
            BlockPos::new(5, 65, 3),
            BlockKind::Rail,
            &[("shape", "east_west"), ("waterlogged", "false")],
        );
    let (mut s, alice) = start(cells);
    on_floor(&mut s, &alice, "rail", 6, 6);
    assert_eq!(
        prop(&s, above(6, 6), "shape").as_deref(),
        Some("south_east")
    );
    on_floor(&mut s, &alice, "rail", 4, 3);
    assert_eq!(
        prop(&s, above(4, 3), "shape").as_deref(),
        Some("ascending_east")
    );
}

#[test]
fn w22_3_a_tall_plant_places_both_halves_and_breaks_without_a_double_drop() {
    let cells = Cells::default().block(floor(6, 6), BlockKind::GrassBlock);
    let (mut s, alice) = start(cells);
    on_floor(&mut s, &alice, "sunflower", 6, 6);
    assert_eq!(prop(&s, above(6, 6), "half").as_deref(), Some("lower"));
    assert_eq!(
        prop(&s, BlockPos::new(6, 65, 6), "half").as_deref(),
        Some("upper")
    );
    console(&mut s, "gamemode survival Alice");
    dig(&mut s, &alice, above(6, 6));
    assert_eq!(kind_at(&s, above(6, 6)), Some(BlockKind::Air));
    assert_eq!(kind_at(&s, BlockPos::new(6, 65, 6)), Some(BlockKind::Air));
    assert_eq!(items_on_ground(&s), 1);
}

#[test]
fn w22_3_breaking_one_door_half_removes_the_other_with_one_drop() {
    let (mut s, alice) = start(Cells::default());
    on_floor(&mut s, &alice, "oak_door", 6, 6);
    assert_eq!(prop(&s, above(6, 6), "half").as_deref(), Some("lower"));
    assert_eq!(
        prop(&s, BlockPos::new(6, 65, 6), "half").as_deref(),
        Some("upper")
    );
    console(&mut s, "setblock 6 64 6 minecraft:air destroy");
    s.tick(2);
    assert_eq!(kind_at(&s, above(6, 6)), Some(BlockKind::Air));
    assert_eq!(kind_at(&s, BlockPos::new(6, 65, 6)), Some(BlockKind::Air));
    assert_eq!(items_on_ground(&s), 1);
}

#[test]
fn w22_4_slabs_merge_into_a_double_slab() {
    let cells = Cells::default().state(
        above(6, 6),
        BlockKind::StoneSlab,
        &[("type", "bottom"), ("waterlogged", "false")],
    );
    let (mut s, alice) = start(cells);
    use_on(
        &mut s,
        &alice,
        "stone_slab",
        above(6, 6),
        Direction::Up,
        (0.5, 1.0, 0.5),
    );
    assert_eq!(prop(&s, above(6, 6), "type").as_deref(), Some("double"));
    assert_eq!(kind_at(&s, BlockPos::new(6, 65, 6)), Some(BlockKind::Air));
}

#[test]
fn w22_4_candles_snow_eggs_and_petals_stack() {
    let cells = Cells::default()
        .state(above(5, 5), BlockKind::Candle, &[("candles", "1")])
        .state(above(7, 5), BlockKind::Snow, &[("layers", "3")])
        .state(
            above(5, 7),
            BlockKind::TurtleEgg,
            &[("eggs", "1"), ("hatch", "0")],
        )
        .state(
            above(7, 7),
            BlockKind::PinkPetals,
            &[("flower_amount", "1"), ("facing", "north")],
        );
    let (mut s, alice) = start(cells);
    for _ in 0..4 {
        use_on(
            &mut s,
            &alice,
            "candle",
            above(5, 5),
            Direction::Up,
            (0.5, 1.0, 0.5),
        );
    }
    assert_eq!(prop(&s, above(5, 5), "candles").as_deref(), Some("4"));
    use_on(
        &mut s,
        &alice,
        "snow",
        above(7, 5),
        Direction::Up,
        (0.5, 1.0, 0.5),
    );
    assert_eq!(prop(&s, above(7, 5), "layers").as_deref(), Some("4"));
    use_on(
        &mut s,
        &alice,
        "turtle_egg",
        above(5, 7),
        Direction::Up,
        (0.5, 1.0, 0.5),
    );
    assert_eq!(prop(&s, above(5, 7), "eggs").as_deref(), Some("2"));
    use_on(
        &mut s,
        &alice,
        "pink_petals",
        above(7, 7),
        Direction::Up,
        (0.5, 1.0, 0.5),
    );
    assert_eq!(prop(&s, above(7, 7), "flower_amount").as_deref(), Some("2"));
}

#[test]
fn w22_4_glow_lichen_gains_faces() {
    let cells = Cells::default()
        .block(above(6, 5), BlockKind::Stone)
        .block(above(6, 7), BlockKind::Stone);
    let (mut s, alice) = start(cells);
    use_on(
        &mut s,
        &alice,
        "glow_lichen",
        above(6, 5),
        Direction::South,
        (0.5, 0.5, 1.0),
    );
    assert_eq!(prop(&s, above(6, 6), "north").as_deref(), Some("true"));
    use_on(
        &mut s,
        &alice,
        "glow_lichen",
        above(6, 7),
        Direction::North,
        (0.5, 0.5, 0.0),
    );
    assert_eq!(prop(&s, above(6, 6), "south").as_deref(), Some("true"));
    assert_eq!(prop(&s, above(6, 6), "north").as_deref(), Some("true"));
}

#[test]
fn w22_5_lanterns_hang_and_blocks_take_their_orientation() {
    let cells = Cells::default()
        .block(BlockPos::new(5, 65, 5), BlockKind::Stone)
        .block(above(7, 6), BlockKind::JungleLog);
    let (mut s, alice) = start(cells);
    use_on(
        &mut s,
        &alice,
        "lantern",
        BlockPos::new(5, 65, 5),
        Direction::Down,
        (0.5, 0.0, 0.5),
    );
    assert_eq!(prop(&s, above(5, 5), "hanging").as_deref(), Some("true"));
    on_floor(&mut s, &alice, "lantern", 4, 4);
    assert_eq!(prop(&s, above(4, 4), "hanging").as_deref(), Some("false"));
    on_floor(&mut s, &alice, "crafter", 3, 3);
    assert_eq!(
        prop(&s, above(3, 3), "orientation").as_deref(),
        Some("up_south")
    );
    on_floor(&mut s, &alice, "calibrated_sculk_sensor", 3, 5);
    assert_eq!(prop(&s, above(3, 5), "facing").as_deref(), Some("north"));
    on_floor(&mut s, &alice, "grindstone", 3, 7);
    assert_eq!(prop(&s, above(3, 7), "face").as_deref(), Some("floor"));
    on_floor(&mut s, &alice, "bell", 4, 7);
    assert_eq!(
        prop(&s, above(4, 7), "attachment").as_deref(),
        Some("floor")
    );
    use_on(
        &mut s,
        &alice,
        "cocoa_beans",
        above(7, 6),
        Direction::West,
        (0.0, 0.5, 0.5),
    );
    assert_eq!(prop(&s, above(6, 6), "facing").as_deref(), Some("east"));
}

#[test]
fn w22_6_a_block_placed_in_water_is_waterlogged_and_keeps_flowing_when_broken() {
    let cells = Cells::default().block(above(6, 6), BlockKind::Water);
    let (mut s, alice) = start(cells);
    on_floor(&mut s, &alice, "stone_slab", 6, 6);
    assert_eq!(kind_at(&s, above(6, 6)), Some(BlockKind::StoneSlab));
    assert_eq!(
        prop(&s, above(6, 6), "waterlogged").as_deref(),
        Some("true")
    );
    s.tick(12);
    assert_eq!(kind_at(&s, above(7, 6)), Some(BlockKind::Water));
    assert_eq!(kind_at(&s, above(5, 6)), Some(BlockKind::Water));
    dig(&mut s, &alice, above(6, 6));
    assert_eq!(kind_at(&s, above(6, 6)), Some(BlockKind::Water));
    assert_eq!(prop(&s, above(6, 6), "level").as_deref(), Some("0"));
}

#[test]
fn w22_7_a_torch_may_be_placed_at_the_feet_of_a_player_but_a_stone_may_not() {
    let (mut s, alice) = start(Cells::default());
    let feet = above(8, 8);
    on_floor(&mut s, &alice, "stone", 8, 8);
    assert_eq!(kind_at(&s, feet), Some(BlockKind::Air));
    on_floor(&mut s, &alice, "torch", 8, 8);
    assert_eq!(kind_at(&s, feet), Some(BlockKind::Torch));
    on_floor(&mut s, &alice, "oak_pressure_plate", 9, 8);
}

#[test]
fn w22_7_a_mob_an_armor_stand_and_a_boat_block_building() {
    let (mut s, alice) = start(Cells::default());
    console(&mut s, "summon minecraft:armor_stand 5.5 64 5.5");
    console(&mut s, "summon minecraft:zombie 3.5 64 3.5 {NoAI:1b}");
    console(&mut s, "summon minecraft:oak_boat 3.5 64 5.5");
    s.tick(2);
    on_floor(&mut s, &alice, "stone", 5, 5);
    assert_eq!(kind_at(&s, above(5, 5)), Some(BlockKind::Air));
    on_floor(&mut s, &alice, "stone", 3, 3);
    assert_eq!(kind_at(&s, above(3, 3)), Some(BlockKind::Air));
    on_floor(&mut s, &alice, "stone", 3, 5);
    assert_eq!(kind_at(&s, above(3, 5)), Some(BlockKind::Air));
    on_floor(&mut s, &alice, "torch", 5, 5);
    assert_eq!(kind_at(&s, above(5, 5)), Some(BlockKind::Torch));
    on_floor(&mut s, &alice, "stone", 7, 7);
    assert_eq!(kind_at(&s, above(7, 7)), Some(BlockKind::Stone));
}

#[test]
fn w22_8_farmland_turns_to_dirt_under_a_solid_block() {
    let cells = Cells::default().state(floor(6, 6), BlockKind::Farmland, &[("moisture", "7")]);
    let (mut s, alice) = start(cells);
    on_floor(&mut s, &alice, "stone", 6, 6);
    assert_eq!(kind_at(&s, floor(6, 6)), Some(BlockKind::Dirt));
}

#[test]
fn w22_8_a_falling_player_tramples_farmland() {
    let cells = Cells::default().state(floor(6, 6), BlockKind::Farmland, &[("moisture", "7")]);
    let (mut s, alice) = start(cells);
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Move {
            position: Some(Vec3::new(6.5, 70.0, 6.5)),
            rotation: None,
            on_ground: false,
        },
    );
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Move {
            position: Some(Vec3::new(6.5, 64.0, 6.5)),
            rotation: None,
            on_ground: true,
        },
    );
    s.tick(1);
    assert_eq!(kind_at(&s, floor(6, 6)), Some(BlockKind::Dirt));
}

#[test]
fn w22_6_a_water_bucket_waterlogs_a_waterloggable_block_and_the_bucket_takes_it_back() {
    let cells = Cells::default().state(
        floor(8, 8),
        BlockKind::StoneSlab,
        &[("type", "bottom"), ("waterlogged", "false")],
    );
    let (mut s, alice) = start(cells);
    console(&mut s, "gamemode survival Alice");
    hold(&mut s, "water_bucket");
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::UseItem {
            hand: Hand::Main,
            sequence: SEQUENCE.fetch_add(1, Ordering::Relaxed),
            rotation: rspigot_domain::Rotation::new(0.0, 90.0),
        },
    );
    s.tick(2);
    assert_eq!(kind_at(&s, floor(8, 8)), Some(BlockKind::StoneSlab));
    assert_eq!(
        prop(&s, floor(8, 8), "waterlogged").as_deref(),
        Some("true")
    );
    hold(&mut s, "bucket");
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::UseItem {
            hand: Hand::Main,
            sequence: SEQUENCE.fetch_add(1, Ordering::Relaxed),
            rotation: rspigot_domain::Rotation::new(0.0, 90.0),
        },
    );
    s.tick(2);
    assert_eq!(
        prop(&s, floor(8, 8), "waterlogged").as_deref(),
        Some("false")
    );
}
