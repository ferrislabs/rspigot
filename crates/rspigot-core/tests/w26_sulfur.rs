#[path = "blocks_common/mod.rs"]
mod blocks_common;
mod common;

use blocks_common::*;
use common::*;
use rspigot_blocks::BlockKind;
use rspigot_domain::{BlockPos, EntityId};
use rspigot_entities::EntityKind as Kind;
use rspigot_nbt::Compound;

const ROOF: i32 = 70;

fn spike_cells(above: BlockKind) -> Cells {
    Cells::default()
        .block(BlockPos::new(4, ROOF, 4), above)
        .state(
            BlockPos::new(4, ROOF - 1, 4),
            BlockKind::SulfurSpike,
            &[
                ("thickness", "tip"),
                ("vertical_direction", "down"),
                ("waterlogged", "false"),
            ],
        )
}

fn is_spike(s: &Scene, pos: BlockPos) -> bool {
    kind_at(s, pos) == Some(BlockKind::SulfurSpike)
}

#[test]
fn a_sulfur_spike_under_a_sulfur_block_grows_by_random_ticks() {
    let mut s = scene_of(spike_cells(BlockKind::Sulfur), 4, 4096);
    let mut grown = false;
    for _ in 0..120 {
        s.tick(10);
        if is_spike(&s, BlockPos::new(4, ROOF - 2, 4)) || is_spike(&s, BlockPos::new(4, 64, 4)) {
            grown = true;
            break;
        }
    }
    assert!(grown);
}

#[test]
fn a_sulfur_spike_under_another_block_never_grows() {
    let mut s = scene_of(spike_cells(BlockKind::Stone), 4, 4096);
    s.tick(400);
    assert!(!is_spike(&s, BlockPos::new(4, ROOF - 2, 4)));
    assert!(!is_spike(&s, BlockPos::new(4, 64, 4)));
}

#[test]
fn a_hanging_sulfur_spike_column_never_grows_beyond_three_blocks() {
    let mut s = scene_of(spike_cells(BlockKind::Sulfur), 9, 4096);
    s.tick(500);
    let hanging = (0..8)
        .filter(|dy| {
            let at = BlockPos::new(4, ROOF - 1 - dy, 4);
            is_spike(&s, at) && property_at(&s, at, "vertical_direction").as_deref() == Some("down")
        })
        .count();
    assert!((1..=3).contains(&hanging), "{hanging}");
}

const SULFUR: BlockPos = BlockPos::new(4, 64, 4);

fn geyser_cells(below: BlockKind, state: &str, water: i32) -> Cells {
    let mut cells = Cells::default()
        .block(SULFUR.offset(0, -1, 0), below)
        .state(
            SULFUR,
            BlockKind::PotentSulfur,
            &[("potent_sulfur_state", state)],
        )
        .entity(SULFUR, "potent_sulfur", Compound::new());
    for dy in 1..=water {
        cells = cells.block(SULFUR.offset(0, dy, 0), BlockKind::Water);
    }
    cells
}

fn sulfur_state(s: &Scene) -> Option<String> {
    property_at(s, SULFUR, "potent_sulfur_state")
}

fn summon(s: &mut Scene, kind: Kind, at: &str, nbt: &str) -> EntityId {
    let known: Vec<EntityId> = s
        .server
        .entities_of(kind)
        .into_iter()
        .map(|entity| entity.id)
        .collect();
    console(s, &format!("summon {} {at} {nbt}", kind.name()));
    s.server
        .entities_of(kind)
        .into_iter()
        .map(|entity| entity.id)
        .find(|id| !known.contains(id))
        .expect("a new entity")
}

fn has_effect(s: &Scene, id: EntityId, name: &str) -> bool {
    s.server
        .saved_entity(id)
        .and_then(|saved| {
            saved.get_list("active_effects").map(|list| {
                list.iter().any(|tag| {
                    tag.as_compound()
                        .and_then(|compound| compound.get_str("id"))
                        == Some(name)
                })
            })
        })
        .unwrap_or(false)
}

#[test]
fn a_continuous_geyser_launches_entities_standing_in_its_water_column() {
    let mut s = scene_of(geyser_cells(BlockKind::Lava, "continuous", 3), 5, 0);
    let pig = summon(&mut s, Kind::Pig, "4.5 65.0 4.5", "{NoAI:1b,Silent:1b}");
    s.tick(3);
    let speed = s
        .server
        .entities_of(Kind::Pig)
        .into_iter()
        .find(|entity| entity.id == pig)
        .map(|entity| entity.velocity.y)
        .unwrap();
    assert!(speed > 0.15, "{speed}");
}

#[test]
fn a_dry_potent_sulfur_block_launches_nobody() {
    let mut s = scene_of(geyser_cells(BlockKind::Lava, "dry", 3), 5, 0);
    let pig = summon(&mut s, Kind::Pig, "4.5 65.0 4.5", "{NoAI:1b,Silent:1b}");
    s.tick(3);
    let speed = s
        .server
        .entities_of(Kind::Pig)
        .into_iter()
        .find(|entity| entity.id == pig)
        .map(|entity| entity.velocity.y)
        .unwrap();
    assert!(speed <= 0.0, "{speed}");
}

#[test]
fn a_wet_geyser_gives_nausea_to_an_animal_at_the_surface() {
    let mut s = scene_of(geyser_cells(BlockKind::Stone, "wet", 3), 5, 0);
    let pig = summon(
        &mut s,
        Kind::Pig,
        "4.5 67.2 4.5",
        "{NoAI:1b,NoGravity:1b,Silent:1b}",
    );
    s.tick(25);
    assert!(has_effect(&s, pig, "minecraft:nausea"));
}

#[test]
fn a_dry_block_never_gives_nausea() {
    let mut s = scene_of(geyser_cells(BlockKind::Stone, "dry", 3), 5, 0);
    let pig = summon(
        &mut s,
        Kind::Pig,
        "4.5 67.2 4.5",
        "{NoAI:1b,NoGravity:1b,Silent:1b}",
    );
    s.tick(25);
    assert!(!has_effect(&s, pig, "minecraft:nausea"));
}

fn timed_cells(countdown: i32) -> Cells {
    let mut data = Compound::new();
    data.insert("countdown", countdown);
    let mut cells = Cells::default()
        .block(SULFUR.offset(0, -1, 0), BlockKind::MagmaBlock)
        .state(
            SULFUR,
            BlockKind::PotentSulfur,
            &[("potent_sulfur_state", "dormant")],
        )
        .entity(SULFUR, "potent_sulfur", data);
    for dy in 1..=3 {
        cells = cells.block(SULFUR.offset(0, dy, 0), BlockKind::Water);
    }
    cells
}

#[test]
fn a_dormant_geyser_erupts_when_its_countdown_runs_out_then_goes_back_to_sleep() {
    let mut s = scene_of(timed_cells(1), 5, 0);
    assert_eq!(sulfur_state(&s).as_deref(), Some("dormant"));
    s.tick(25);
    assert_eq!(sulfur_state(&s).as_deref(), Some("erupting"));
    let mut slept = false;
    for _ in 0..12 {
        s.tick(20);
        if sulfur_state(&s).as_deref() == Some("dormant") {
            slept = true;
            break;
        }
    }
    assert!(slept);
}

#[test]
fn a_dormant_geyser_with_a_long_countdown_keeps_sleeping() {
    let mut s = scene_of(timed_cells(500), 5, 0);
    s.tick(100);
    assert_eq!(sulfur_state(&s).as_deref(), Some("dormant"));
}

#[test]
fn placing_potent_sulfur_under_water_on_lava_makes_it_erupt_continuously() {
    let cells = Cells::default()
        .block(SULFUR.offset(0, -1, 0), BlockKind::Lava)
        .block(SULFUR.offset(0, 1, 0), BlockKind::Water)
        .block(SULFUR.offset(0, 2, 0), BlockKind::Water);
    let mut s = scene_of(cells, 5, 0);
    console(&mut s, "setblock 4 64 4 minecraft:potent_sulfur");
    s.tick(2);
    assert_eq!(sulfur_state(&s).as_deref(), Some("continuous"));
}

#[test]
fn placing_potent_sulfur_without_water_above_leaves_it_dry() {
    let cells = Cells::default().block(SULFUR.offset(0, -1, 0), BlockKind::Lava);
    let mut s = scene_of(cells, 5, 0);
    console(&mut s, "setblock 4 64 4 minecraft:potent_sulfur");
    s.tick(2);
    assert_eq!(sulfur_state(&s).as_deref(), Some("dry"));
}

#[test]
fn removing_the_water_above_a_wet_block_dries_it() {
    let mut s = scene_of(geyser_cells(BlockKind::Stone, "wet", 2), 5, 0);
    console(&mut s, "setblock 4 65 4 minecraft:air");
    s.tick(2);
    assert_eq!(sulfur_state(&s).as_deref(), Some("dry"));
}
