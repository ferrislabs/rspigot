#[path = "blocks_common/mod.rs"]
mod blocks_common;
mod common;
#[path = "common/screen.rs"]
mod screen;

use blocks_common::*;
use common::*;
use rspigot_blocks::BlockKind;
use rspigot_core::events::{ClientEvent, PlayerInput};
use rspigot_domain::{BlockPos, Rotation, Vec3};
use screen::use_block;

const SPOT: BlockPos = BlockPos::new(4, 64, 4);

fn quick_scene(cells: Cells, seed: u64) -> Scene {
    scene_of(cells, seed, 4096)
}

#[test]
fn a_chorus_flower_on_end_stone_grows_a_stem_and_flowers_by_random_ticks() {
    let cells = Cells::default()
        .block(SPOT.offset(0, -1, 0), BlockKind::EndStone)
        .state(SPOT, BlockKind::ChorusFlower, &[("age", "0")]);
    let mut s = quick_scene(cells, 11);
    let mut grew = false;
    for _ in 0..40 {
        s.tick(5);
        if kind_at(&s, SPOT) == Some(BlockKind::ChorusPlant) {
            grew = true;
            break;
        }
    }
    assert!(grew);
    assert!(matches!(
        kind_at(&s, SPOT.offset(0, 1, 0)),
        Some(BlockKind::ChorusFlower | BlockKind::ChorusPlant)
    ));
    assert_eq!(property_at(&s, SPOT, "up").as_deref(), Some("true"));
}

#[test]
fn a_chorus_flower_that_cannot_grow_any_more_dies() {
    let cells = Cells::default()
        .block(SPOT.offset(0, -1, 0), BlockKind::EndStone)
        .block(SPOT.offset(0, 2, 0), BlockKind::Stone)
        .state(SPOT, BlockKind::ChorusFlower, &[("age", "4")]);
    let mut s = quick_scene(cells, 11);
    s.tick(200);
    assert_eq!(property_at(&s, SPOT, "age").as_deref(), Some("5"));
}

#[test]
fn a_chorus_flower_with_something_right_above_it_stays_as_it_is() {
    let cells = Cells::default()
        .block(SPOT.offset(0, -1, 0), BlockKind::EndStone)
        .block(SPOT.offset(0, 1, 0), BlockKind::Stone)
        .state(SPOT, BlockKind::ChorusFlower, &[("age", "2")]);
    let mut s = quick_scene(cells, 11);
    s.tick(200);
    assert_eq!(property_at(&s, SPOT, "age").as_deref(), Some("2"));
}

const ROOF: i32 = 70;

fn drip_cells(base: BlockKind, above: BlockKind) -> Cells {
    Cells::default()
        .block(BlockPos::new(4, ROOF, 4), base)
        .block(BlockPos::new(4, ROOF + 1, 4), above)
        .state(
            BlockPos::new(4, ROOF - 1, 4),
            BlockKind::PointedDripstone,
            &[
                ("thickness", "tip"),
                ("vertical_direction", "down"),
                ("waterlogged", "false"),
            ],
        )
}

fn is_pointed_dripstone(s: &Scene, pos: BlockPos) -> bool {
    kind_at(s, pos) == Some(BlockKind::PointedDripstone)
}

#[test]
fn a_stalactite_under_dripstone_and_water_grows_longer_and_its_old_tip_thickens() {
    let mut s = scene_of(
        drip_cells(BlockKind::DripstoneBlock, BlockKind::Water),
        4,
        4096,
    );
    let tip = BlockPos::new(4, ROOF - 1, 4);
    let mut grown = false;
    for _ in 0..120 {
        s.tick(10);
        if is_pointed_dripstone(&s, tip.offset(0, -1, 0))
            || is_pointed_dripstone(&s, BlockPos::new(4, 64, 4))
        {
            grown = true;
            break;
        }
    }
    assert!(grown);
    if is_pointed_dripstone(&s, tip.offset(0, -1, 0)) {
        assert_eq!(
            property_at(&s, tip, "thickness").as_deref(),
            Some("frustum")
        );
        assert_eq!(
            property_at(&s, tip.offset(0, -1, 0), "vertical_direction").as_deref(),
            Some("down")
        );
    }
}

#[test]
fn a_stalactite_without_water_above_its_dripstone_never_grows() {
    let mut s = scene_of(
        drip_cells(BlockKind::DripstoneBlock, BlockKind::Stone),
        4,
        4096,
    );
    s.tick(300);
    assert!(!is_pointed_dripstone(&s, BlockPos::new(4, ROOF - 2, 4)));
    assert!(!is_pointed_dripstone(&s, BlockPos::new(4, 64, 4)));
}

fn cauldron_scene(source: BlockKind, seed: u64) -> Scene {
    let cells =
        drip_cells(BlockKind::Stone, source).block(BlockPos::new(4, 64, 4), BlockKind::Cauldron);
    scene_of(cells, seed, 4096)
}

#[test]
fn a_stalactite_under_water_slowly_fills_the_cauldron_below_its_tip() {
    let mut s = cauldron_scene(BlockKind::Water, 6);
    let cauldron = BlockPos::new(4, 64, 4);
    let mut level = 0;
    for _ in 0..200 {
        s.tick(10);
        if kind_at(&s, cauldron) == Some(BlockKind::WaterCauldron) {
            level = property_at(&s, cauldron, "level").unwrap().parse().unwrap();
            break;
        }
    }
    assert_eq!(level, 1);
    s.tick(600);
    assert_eq!(property_at(&s, cauldron, "level").as_deref(), Some("3"));
}

#[test]
fn a_stalactite_under_lava_fills_the_cauldron_with_lava_only_once() {
    let mut s = cauldron_scene(BlockKind::Lava, 6);
    s.tick(600);
    assert_eq!(
        kind_at(&s, BlockPos::new(4, 64, 4)),
        Some(BlockKind::LavaCauldron)
    );
}

#[test]
fn a_stalactite_under_stone_without_fluid_leaves_the_cauldron_empty() {
    let mut s = cauldron_scene(BlockKind::Stone, 6);
    s.tick(300);
    assert_eq!(
        kind_at(&s, BlockPos::new(4, 64, 4)),
        Some(BlockKind::Cauldron)
    );
}

#[test]
fn a_cauldron_behind_a_solid_block_receives_nothing() {
    let cells = drip_cells(BlockKind::Stone, BlockKind::Water)
        .block(BlockPos::new(4, 66, 4), BlockKind::Stone)
        .block(BlockPos::new(4, 64, 4), BlockKind::Cauldron);
    let mut s = scene_of(cells, 6, 4096);
    s.tick(300);
    assert_eq!(
        kind_at(&s, BlockPos::new(4, 64, 4)),
        Some(BlockKind::Cauldron)
    );
}

#[test]
fn water_over_mud_dripping_through_a_stalactite_turns_the_mud_to_clay() {
    let cells = Cells::default()
        .block(BlockPos::new(4, ROOF + 1, 4), BlockKind::Mud)
        .block(BlockPos::new(4, ROOF, 4), BlockKind::DripstoneBlock)
        .state(
            BlockPos::new(4, ROOF - 1, 4),
            BlockKind::PointedDripstone,
            &[
                ("thickness", "tip"),
                ("vertical_direction", "down"),
                ("waterlogged", "false"),
            ],
        );
    let mut s = scene_of(cells, 6, 4096);
    s.tick(300);
    assert_eq!(
        kind_at(&s, BlockPos::new(4, ROOF + 1, 4)),
        Some(BlockKind::Clay)
    );
}

const LEAF: BlockPos = BlockPos::new(4, 64, 4);

fn leaf_scene(extra: Option<(BlockPos, BlockKind)>) -> Scene {
    let mut cells = Cells::default().state(
        LEAF,
        BlockKind::BigDripleaf,
        &[
            ("facing", "north"),
            ("tilt", "none"),
            ("waterlogged", "false"),
        ],
    );
    if let Some((pos, kind)) = extra {
        cells = cells.block(pos, kind);
    }
    scene_of(cells, 2, 0)
}

fn step_on(s: &mut Scene, at: Vec3) {
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Move {
            position: Some(at),
            rotation: Some(Rotation::new(0.0, 0.0)),
            on_ground: true,
        },
    );
}

fn sounds(s: &Scene) -> Vec<&'static str> {
    s.players[0]
        .link
        .take()
        .into_iter()
        .filter_map(|event| match event {
            ClientEvent::Sound { sound, .. } => Some(sound),
            _ => None,
        })
        .collect()
}

#[test]
fn a_player_standing_on_a_big_dripleaf_tips_it_through_its_stages_and_it_springs_back() {
    let mut s = leaf_scene(None);
    s.tick(2);
    s.players[0].link.take();
    step_on(&mut s, Vec3::new(4.5, 64.9375, 4.5));
    s.tick(2);
    assert_eq!(property_at(&s, LEAF, "tilt").as_deref(), Some("unstable"));
    s.tick(10);
    assert_eq!(property_at(&s, LEAF, "tilt").as_deref(), Some("partial"));
    s.tick(10);
    assert_eq!(property_at(&s, LEAF, "tilt").as_deref(), Some("full"));
    assert_eq!(
        sounds(&s)
            .iter()
            .filter(|sound| **sound == "minecraft:block.big_dripleaf.tilt_down")
            .count(),
        2
    );
    step_on(&mut s, Vec3::new(8.5, 64.0, 8.5));
    s.tick(95);
    assert_eq!(property_at(&s, LEAF, "tilt").as_deref(), Some("full"));
    s.tick(10);
    assert_eq!(property_at(&s, LEAF, "tilt").as_deref(), Some("none"));
    assert!(sounds(&s).contains(&"minecraft:block.big_dripleaf.tilt_up"));
}

#[test]
fn a_player_beside_the_leaf_or_in_the_air_does_not_tip_it() {
    let mut s = leaf_scene(None);
    s.tick(2);
    step_on(&mut s, Vec3::new(6.5, 64.9375, 4.5));
    s.tick(30);
    assert_eq!(property_at(&s, LEAF, "tilt").as_deref(), Some("none"));
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Move {
            position: Some(Vec3::new(4.5, 66.0, 4.5)),
            rotation: None,
            on_ground: false,
        },
    );
    s.tick(30);
    assert_eq!(property_at(&s, LEAF, "tilt").as_deref(), Some("none"));
}

#[test]
fn a_redstone_signal_keeps_the_big_dripleaf_from_tipping() {
    let mut s = leaf_scene(Some((BlockPos::new(5, 64, 4), BlockKind::RedstoneBlock)));
    s.tick(2);
    step_on(&mut s, Vec3::new(4.5, 64.9375, 4.5));
    s.tick(30);
    assert_eq!(property_at(&s, LEAF, "tilt").as_deref(), Some("none"));
}

fn bone_meal_scene(cells: Cells, seed: u64) -> (Scene, Player) {
    let mut s = scene_of(cells, seed, 0);
    let alice = s.players[0].clone();
    console(&mut s, "gamemode survival Alice");
    s.tick(1);
    alice.link.take();
    (s, alice)
}

fn count_item(s: &mut Scene, player: &Player, key: &str) -> i32 {
    match s.ask(rspigot_core::plugins::Question::Inventory(player.id)) {
        rspigot_core::plugins::Answer::Inventory(inventory) => inventory
            .slots
            .iter()
            .flatten()
            .filter(|item| item.key == key)
            .map(|item| item.count)
            .sum(),
        other => panic!("expected the inventory, got {other:?}"),
    }
}

#[test]
fn bone_meal_on_a_big_dripleaf_adds_a_stem_and_a_leaf_above() {
    let cells = Cells::default().state(
        LEAF,
        BlockKind::BigDripleaf,
        &[
            ("facing", "east"),
            ("tilt", "none"),
            ("waterlogged", "false"),
        ],
    );
    let (mut s, alice) = bone_meal_scene(cells, 3);
    console(&mut s, "give Alice minecraft:bone_meal 2");
    use_block(&mut s, &alice, LEAF);
    s.tick(1);
    assert_eq!(kind_at(&s, LEAF), Some(BlockKind::BigDripleafStem));
    assert_eq!(
        kind_at(&s, LEAF.offset(0, 1, 0)),
        Some(BlockKind::BigDripleaf)
    );
    assert_eq!(
        property_at(&s, LEAF.offset(0, 1, 0), "facing").as_deref(),
        Some("east")
    );
    assert_eq!(count_item(&mut s, &alice, "minecraft:bone_meal"), 1);
    use_block(&mut s, &alice, LEAF);
    s.tick(1);
    assert_eq!(
        kind_at(&s, LEAF.offset(0, 2, 0)),
        Some(BlockKind::BigDripleaf)
    );
}

#[test]
fn bone_meal_on_a_big_dripleaf_with_no_room_is_kept() {
    let cells = Cells::default()
        .state(
            LEAF,
            BlockKind::BigDripleaf,
            &[
                ("facing", "east"),
                ("tilt", "none"),
                ("waterlogged", "false"),
            ],
        )
        .block(LEAF.offset(0, 1, 0), BlockKind::Stone);
    let (mut s, alice) = bone_meal_scene(cells, 3);
    console(&mut s, "give Alice minecraft:bone_meal 2");
    use_block(&mut s, &alice, LEAF);
    s.tick(1);
    assert_eq!(kind_at(&s, LEAF), Some(BlockKind::BigDripleaf));
    assert_eq!(count_item(&mut s, &alice, "minecraft:bone_meal"), 2);
}

#[test]
fn bone_meal_on_an_azalea_eventually_grows_an_azalea_tree_on_rooted_dirt() {
    let cells = Cells::default().block(LEAF, BlockKind::Azalea);
    let (mut s, alice) = bone_meal_scene(cells, 8);
    console(&mut s, "give Alice minecraft:bone_meal 64");
    let mut uses = 0;
    while kind_at(&s, LEAF) == Some(BlockKind::Azalea) && uses < 40 {
        use_block(&mut s, &alice, LEAF);
        s.tick(1);
        uses += 1;
    }
    assert!(uses < 40);
    assert_eq!(kind_at(&s, LEAF), Some(BlockKind::OakLog));
    assert_eq!(
        kind_at(&s, LEAF.offset(0, -1, 0)),
        Some(BlockKind::RootedDirt)
    );
    let mut leaves = 0;
    for x in -4..=12 {
        for z in -4..=12 {
            for y in 64..=75 {
                if matches!(
                    kind_at(&s, BlockPos::new(x, y, z)),
                    Some(BlockKind::AzaleaLeaves | BlockKind::FloweringAzaleaLeaves)
                ) {
                    leaves += 1;
                }
            }
        }
    }
    assert!(leaves > 10);
    assert!(count_item(&mut s, &alice, "minecraft:bone_meal") < 64);
}

fn reef(pickle: &str) -> Cells {
    let mut cells = Cells::default();
    for x in 1..=7 {
        for z in 1..=7 {
            cells = cells
                .block(BlockPos::new(x, 63, z), BlockKind::BrainCoralBlock)
                .block(BlockPos::new(x, 64, z), BlockKind::Water)
                .block(BlockPos::new(x, 65, z), BlockKind::Water);
        }
    }
    cells.state(
        LEAF,
        BlockKind::SeaPickle,
        &[("pickles", pickle), ("waterlogged", "true")],
    )
}

fn pickles_around(s: &Scene) -> (usize, usize) {
    let mut count = 0;
    for x in 1..=7 {
        for z in 1..=7 {
            for y in 64..=65 {
                if kind_at(s, BlockPos::new(x, y, z)) == Some(BlockKind::SeaPickle) {
                    count += 1;
                }
            }
        }
    }
    (
        count,
        property_at(s, LEAF, "pickles").unwrap().parse().unwrap(),
    )
}

#[test]
fn bone_meal_on_a_sea_pickle_over_coral_fills_it_and_spreads_new_pickles() {
    let (mut s, alice) = bone_meal_scene(reef("1"), 12);
    console(&mut s, "give Alice minecraft:bone_meal 8");
    use_block(&mut s, &alice, LEAF);
    s.tick(1);
    assert_eq!(pickles_around(&s).1, 4);
    for _ in 0..6 {
        use_block(&mut s, &alice, LEAF);
        s.tick(1);
    }
    assert!(pickles_around(&s).0 > 1);
}

#[test]
fn a_sea_pickle_item_used_on_a_pickle_adds_one_pickle_up_to_four() {
    let (mut s, alice) = bone_meal_scene(reef("2"), 12);
    console(&mut s, "give Alice minecraft:sea_pickle 3");
    use_block(&mut s, &alice, LEAF);
    s.tick(1);
    assert_eq!(property_at(&s, LEAF, "pickles").as_deref(), Some("3"));
    use_block(&mut s, &alice, LEAF);
    s.tick(1);
    assert_eq!(property_at(&s, LEAF, "pickles").as_deref(), Some("4"));
    use_block(&mut s, &alice, LEAF);
    s.tick(1);
    assert_eq!(property_at(&s, LEAF, "pickles").as_deref(), Some("4"));
    assert_eq!(
        kind_at(&s, LEAF.offset(0, 1, 0)),
        Some(BlockKind::SeaPickle)
    );
    assert_eq!(count_item(&mut s, &alice, "minecraft:sea_pickle"), 0);
}
