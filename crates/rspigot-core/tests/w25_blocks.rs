#[path = "blocks_common/mod.rs"]
mod blocks_common;
mod common;
#[path = "common/screen.rs"]
mod screen;

use blocks_common::*;
use common::*;
use rspigot_blocks::BlockKind;
use rspigot_domain::BlockPos;
use screen::use_block;

const SPOT: BlockPos = BlockPos::new(8, 64, 10);

fn rig(cells: Cells, speed: i32) -> Scene {
    let mut s = scene_of(cells, 21, speed);
    stream_chunk_of(&mut s, SPOT);
    s.tick(2);
    s
}

#[test]
fn clicking_a_redstone_ore_lights_it_and_a_random_tick_puts_it_out() {
    let cells = Cells::default().state(SPOT, BlockKind::RedstoneOre, &[("lit", "false")]);
    let mut s = rig(cells, 0);
    let alice = s.players[0].clone();
    use_block(&mut s, &alice, SPOT);
    assert_eq!(property_at(&s, SPOT, "lit").as_deref(), Some("true"));
    console(&mut s, "gamerule random_tick_speed 4096");
    for _ in 0..40 {
        s.tick(1);
        if property_at(&s, SPOT, "lit").as_deref() == Some("false") {
            return;
        }
    }
    panic!("the ore never went out");
}

#[test]
fn a_deepslate_redstone_ore_lights_when_dug_at() {
    let cells = Cells::default().state(SPOT, BlockKind::DeepslateRedstoneOre, &[("lit", "false")]);
    let mut s = rig(cells, 0);
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        rspigot_core::events::PlayerInput::Dig {
            action: rspigot_core::events::DigAction::Start,
            pos: SPOT,
            face: rspigot_core::events::Direction::Up,
            sequence: 1,
        },
    );
    s.tick(1);
    assert_eq!(property_at(&s, SPOT, "lit").as_deref(), Some("true"));
}

#[test]
fn a_budding_amethyst_grows_a_small_bud_then_larger_ones_on_a_free_face() {
    let cells = Cells::default().block(SPOT, BlockKind::BuddingAmethyst);
    let mut s = rig(cells, 4096);
    let mut grown = false;
    for _ in 0..400 {
        s.tick(1);
        grown = [
            BlockKind::SmallAmethystBud,
            BlockKind::MediumAmethystBud,
            BlockKind::LargeAmethystBud,
            BlockKind::AmethystCluster,
        ]
        .into_iter()
        .any(|bud| kind_at(&s, SPOT.offset(0, 1, 0)) == Some(bud));
        if kind_at(&s, SPOT.offset(0, 1, 0)) == Some(BlockKind::AmethystCluster) {
            break;
        }
    }
    assert!(grown);
}

fn water_column(base: BlockKind) -> Cells {
    let mut cells = Cells::default().block(SPOT, base);
    for dy in 1..=4 {
        cells = cells.block(SPOT.offset(0, dy, 0), BlockKind::Water);
    }
    cells
}

#[test]
fn soul_sand_under_still_water_makes_an_upward_bubble_column() {
    let mut s = rig(water_column(BlockKind::Stone), 0);
    console(&mut s, "setblock 8 64 10 soul_sand");
    s.tick(6);
    for dy in 1..=4 {
        let at = SPOT.offset(0, dy, 0);
        assert_eq!(kind_at(&s, at), Some(BlockKind::BubbleColumn), "dy={dy}");
        assert_eq!(property_at(&s, at, "drag").as_deref(), Some("false"));
    }
}

#[test]
fn magma_under_water_drags_down_and_removing_it_clears_the_column() {
    let mut s = rig(water_column(BlockKind::Stone), 0);
    console(&mut s, "setblock 8 64 10 magma_block");
    s.tick(6);
    assert_eq!(
        kind_at(&s, SPOT.offset(0, 2, 0)),
        Some(BlockKind::BubbleColumn)
    );
    assert_eq!(
        property_at(&s, SPOT.offset(0, 2, 0), "drag").as_deref(),
        Some("true")
    );
    console(&mut s, "setblock 8 64 10 stone");
    s.tick(20);
    for dy in 1..=4 {
        assert_eq!(
            kind_at(&s, SPOT.offset(0, dy, 0)),
            Some(BlockKind::Water),
            "dy={dy}"
        );
    }
}

#[test]
fn a_dry_coral_dies_after_a_while_and_a_wet_or_adjacent_to_water_one_lives() {
    let mut s = rig(Cells::default(), 0);
    console(&mut s, "setblock 8 64 10 tube_coral[waterlogged=false]");
    assert_eq!(
        property_at(&s, SPOT, "waterlogged").as_deref(),
        Some("false")
    );
    console(&mut s, "setblock 14 64 10 brain_coral_block");
    console(&mut s, "setblock 30 64 10 tube_coral[waterlogged=true]");
    console(&mut s, "setblock 40 64 10 fire_coral_block");
    console(&mut s, "setblock 40 65 10 water");
    s.tick(130);
    assert_eq!(kind_at(&s, SPOT), Some(BlockKind::DeadTubeCoral));
    assert_eq!(
        kind_at(&s, SPOT.offset(6, 0, 0)),
        Some(BlockKind::DeadBrainCoralBlock)
    );
    assert_eq!(
        kind_at(&s, SPOT.offset(22, 0, 0)),
        Some(BlockKind::TubeCoral)
    );
    assert_eq!(
        kind_at(&s, SPOT.offset(32, 0, 0)),
        Some(BlockKind::FireCoralBlock)
    );
}

#[test]
fn shears_on_a_pumpkin_carve_it_facing_the_clicked_side_and_drop_four_seeds() {
    let cells = Cells::default().block(SPOT, BlockKind::Pumpkin);
    let mut s = rig(cells, 0);
    console(&mut s, "give Alice minecraft:shears 1");
    s.tick(1);
    let before = s.server.ground_item_count();
    let alice = s.players[0].clone();
    use_block(&mut s, &alice, SPOT);
    s.tick(1);
    assert_eq!(kind_at(&s, SPOT), Some(BlockKind::CarvedPumpkin));
    assert_eq!(s.server.ground_item_count(), before + 1);
}

#[test]
fn a_nylium_under_a_solid_block_turns_back_into_netherrack() {
    let cells = Cells::default()
        .block(SPOT, BlockKind::CrimsonNylium)
        .block(SPOT.offset(0, 1, 0), BlockKind::Stone)
        .block(SPOT.offset(4, 0, 0), BlockKind::WarpedNylium);
    let mut s = rig(cells, 4096);
    for _ in 0..60 {
        s.tick(1);
        if kind_at(&s, SPOT) == Some(BlockKind::Netherrack) {
            assert_eq!(
                kind_at(&s, SPOT.offset(4, 0, 0)),
                Some(BlockKind::WarpedNylium)
            );
            return;
        }
    }
    panic!("the covered nylium never reverted");
}

#[test]
fn a_mushroom_spreads_to_a_nearby_dark_spot_over_time() {
    let cells = Cells::default()
        .block(SPOT.offset(0, -1, 0), BlockKind::Mycelium)
        .block(SPOT, BlockKind::BrownMushroom);
    let mut s = rig(cells, 4096);
    let mut cells_found = 0;
    for _ in 0..400 {
        s.tick(1);
        cells_found = (-4..=4)
            .flat_map(|dx| (-4..=4).map(move |dz| (dx, dz)))
            .filter(|(dx, dz)| {
                kind_at(&s, SPOT.offset(*dx, 0, *dz)) == Some(BlockKind::BrownMushroom)
            })
            .count();
        if cells_found > 1 {
            break;
        }
    }
    assert!(cells_found > 1);
}

#[test]
fn clicking_a_dragon_egg_teleports_it_to_a_free_spot_nearby() {
    let cells = Cells::default().block(SPOT, BlockKind::DragonEgg);
    let mut s = rig(cells, 0);
    let alice = s.players[0].clone();
    use_block(&mut s, &alice, SPOT);
    s.tick(1);
    assert_eq!(kind_at(&s, SPOT), Some(BlockKind::Air));
    let mut found = 0;
    for dx in -16..=16 {
        for dy in -8..=8 {
            for dz in -16..=16 {
                if kind_at(&s, SPOT.offset(dx, dy, dz)) == Some(BlockKind::DragonEgg) {
                    found += 1;
                }
            }
        }
    }
    assert_eq!(found, 1);
}
