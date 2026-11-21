use super::grid::{
    BLOCKED, Face, Grid, Mansion, ORIGIN, ROOM_1X2, ROOM_ID, ROOM_KIND, STAIRS, is_house,
};
use super::placer::mansion_pieces;
use crate::legacy::JavaRandom;
use crate::overworld::features::structure::Rotation;
use crate::structures::Library;
use crate::structures::shaped::tpl::Tpl;

fn mansion(seed: i64) -> Mansion {
    Mansion::generate(&mut JavaRandom::new(seed))
}

fn count(grid: &Grid, value: i32) -> usize {
    (0..grid.width)
        .flat_map(|x| (0..grid.height).map(move |y| (x, y)))
        .filter(|(x, y)| grid.get(*x, *y) == value)
        .count()
}

#[test]
fn the_ground_floor_always_keeps_its_entrance_and_corridor_frame() {
    for seed in 0..40 {
        let m = mansion(seed);
        assert_eq!(m.base.get(7, 4), 3, "{seed}");
        assert_eq!(m.base.get(8, 5), 3, "{seed}");
        assert_eq!(m.base.get(6, 4), 2, "{seed}");
        assert_eq!(m.base.get(8, 2), 1, "{seed}");
        assert_eq!(m.base.get(8, 7), 1, "{seed}");
        assert!(count(&m.base, 1) > 6, "{seed}");
        assert!(count(&m.base, 2) > 6, "{seed}");
        assert_eq!(m.rooms[0].get(8, 4), BLOCKED);
        assert_eq!(m.rooms[1].get(8, 5), BLOCKED);
    }
}

#[test]
fn every_room_cell_of_the_ground_floor_belongs_to_a_room() {
    for seed in 0..40 {
        let m = mansion(seed);
        for y in 0..11 {
            for x in 0..11 {
                if m.base.get(x, y) == 2 {
                    let value = m.rooms[0].get(x, y);
                    assert!(value & ROOM_ID >= 10, "{seed} {x},{y}");
                    assert_ne!(value & ROOM_KIND, 0, "{seed} {x},{y}");
                }
            }
        }
    }
}

#[test]
fn a_room_has_exactly_one_origin_cell() {
    for seed in 0..40 {
        let m = mansion(seed);
        let mut origins = std::collections::BTreeMap::new();
        for y in 0..11 {
            for x in 0..11 {
                let value = m.rooms[0].get(x, y);
                if value & ROOM_ID >= 10 && value & ROOM_ID < 0x8000 {
                    let entry = origins.entry(value & ROOM_ID).or_insert(0);
                    if value & ORIGIN == ORIGIN {
                        *entry += 1;
                    }
                }
            }
        }
        assert!(origins.values().all(|n| *n == 1), "{seed} {origins:?}");
    }
}

#[test]
fn the_top_floor_is_a_subset_of_the_ground_floor() {
    for seed in 0..60 {
        let m = mansion(seed);
        for y in 0..11 {
            for x in 0..11 {
                if is_house(&m.third, x, y) {
                    assert!(is_house(&m.base, x, y), "{seed} {x},{y}");
                }
            }
        }
    }
}

#[test]
fn a_one_by_two_room_with_stairs_leads_to_the_third_floor() {
    let mut with_third = 0;
    for seed in 0..60 {
        let m = mansion(seed);
        let top_rooms = (0..11)
            .flat_map(|x| (0..11).map(move |y| (x, y)))
            .filter(|(x, y)| m.third.get(*x, *y) == 3)
            .count();
        if top_rooms > 0 {
            with_third += 1;
            let stairs = (0..11)
                .flat_map(|x| (0..11).map(move |y| (x, y)))
                .any(|(x, y)| {
                    let value = m.rooms[1].get(x, y);
                    value & ROOM_KIND == ROOM_1X2 && value & STAIRS == STAIRS
                });
            assert!(stairs, "{seed}");
        }
    }
    assert!(with_third > 0);
}

#[test]
fn the_pair_direction_finds_the_other_half_of_a_one_by_two_room() {
    let m = mansion(3);
    for y in 0..11 {
        for x in 0..11 {
            let value = m.rooms[0].get(x, y);
            if value & ROOM_KIND == ROOM_1X2 && value & ORIGIN == ORIGIN {
                let face = m
                    .pair_direction(0, x, y, value & ROOM_ID)
                    .expect("a partner");
                let (dx, dz) = face.step();
                assert_eq!(m.rooms[0].get(x + dx, y + dz) & ROOM_ID, value & ROOM_ID);
                assert!(matches!(
                    face,
                    Face::North | Face::East | Face::South | Face::West
                ));
            }
        }
    }
}

#[test]
fn every_piece_of_every_rotation_names_a_template_that_exists() {
    let library = Library::embedded();
    for seed in 0..25 {
        for rotation in Rotation::ALL {
            let specs = mansion_pieces((100, 70, -40), rotation, &mut JavaRandom::new(seed));
            assert!(specs.len() > 60, "{seed} {}", specs.len());
            for spec in specs {
                assert!(
                    Tpl::new(
                        library,
                        spec.name,
                        spec.pos,
                        spec.rotation,
                        spec.mirror,
                        (0, 0)
                    )
                    .is_some(),
                    "{}",
                    spec.name
                );
            }
        }
    }
}

#[test]
fn the_same_seed_gives_the_same_mansion() {
    let first = mansion_pieces((0, 70, 0), Rotation::None, &mut JavaRandom::new(77));
    let second = mansion_pieces((0, 70, 0), Rotation::None, &mut JavaRandom::new(77));
    let lines = |specs: &[super::placer::Spec]| -> Vec<String> {
        specs
            .iter()
            .map(|spec| {
                format!(
                    "{} {:?} {:?} {:?}",
                    spec.name, spec.pos, spec.rotation, spec.mirror
                )
            })
            .collect()
    };
    assert_eq!(lines(&first), lines(&second));
}

#[test]
fn a_mansion_spans_three_floors_with_a_roof() {
    let specs = mansion_pieces((0, 70, 0), Rotation::None, &mut JavaRandom::new(5));
    let names: std::collections::BTreeSet<&str> = specs.iter().map(|spec| spec.name).collect();
    for needed in [
        "minecraft:woodland_mansion/entrance",
        "minecraft:woodland_mansion/wall_flat",
        "minecraft:woodland_mansion/wall_window",
        "minecraft:woodland_mansion/roof",
        "minecraft:woodland_mansion/corridor_floor",
    ] {
        assert!(names.contains(needed), "{needed}");
    }
    let tops = specs.iter().map(|spec| spec.pos.1).max().unwrap();
    assert!(tops >= 70 + 27, "{tops}");
}
