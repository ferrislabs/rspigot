use std::collections::HashMap;

use rspigot_blocks::BlockKind;

use super::*;

struct World(HashMap<BlockPos, StateId>);

impl World {
    fn new() -> Self {
        Self(HashMap::new())
    }

    fn set(&mut self, pos: BlockPos, kind: BlockKind) {
        self.0.insert(pos, kind.default_state());
    }

    fn get(&self, pos: BlockPos) -> Option<StateId> {
        Some(
            self.0
                .get(&pos)
                .copied()
                .unwrap_or_else(|| BlockKind::Air.default_state()),
        )
    }

    fn frame(&mut self, origin: BlockPos, axis: Axis, width: i32, height: i32) {
        let along = |i: i32| match axis {
            Axis::X => origin.offset(i, 0, 0),
            Axis::Z => origin.offset(0, 0, i),
        };
        for i in -1..=width {
            self.set(along(i).offset(0, -1, 0), BlockKind::Obsidian);
            self.set(along(i).offset(0, height, 0), BlockKind::Obsidian);
        }
        for y in 0..height {
            self.set(along(-1).offset(0, y, 0), BlockKind::Obsidian);
            self.set(along(width).offset(0, y, 0), BlockKind::Obsidian);
        }
    }
}

const ORIGIN: BlockPos = BlockPos::new(10, 64, 10);

fn find(world: &World, pos: BlockPos, axis: Axis) -> Option<PortalShape> {
    PortalShape::find_empty(&|at| world.get(at), 0, pos, axis)
}

#[test]
fn a_minimal_frame_is_found_from_a_fire_inside_it() {
    let mut world = World::new();
    world.frame(ORIGIN, Axis::X, 2, 3);
    let shape = find(&world, ORIGIN.offset(1, 0, 0), Axis::X).expect("shape");
    assert_eq!((shape.width, shape.height, shape.axis), (2, 3, Axis::X));
    assert_eq!(shape.interior().len(), 6);
    assert!(!shape.is_complete());
}

#[test]
fn the_other_axis_is_tried_when_the_preferred_one_has_no_frame() {
    let mut world = World::new();
    world.frame(ORIGIN, Axis::Z, 3, 4);
    let shape = find(&world, ORIGIN.offset(0, 1, 1), Axis::X).expect("shape");
    assert_eq!((shape.axis, shape.width, shape.height), (Axis::Z, 3, 4));
}

#[test]
fn corners_of_the_frame_are_not_needed() {
    let mut world = World::new();
    world.frame(ORIGIN, Axis::X, 2, 3);
    for corner in [
        ORIGIN.offset(-1, -1, 0),
        ORIGIN.offset(2, -1, 0),
        ORIGIN.offset(-1, 3, 0),
        ORIGIN.offset(2, 3, 0),
    ] {
        world.0.remove(&corner);
    }
    assert!(find(&world, ORIGIN, Axis::X).is_some());
}

#[test]
fn a_missing_side_or_top_block_refuses_the_frame() {
    let mut world = World::new();
    world.frame(ORIGIN, Axis::X, 2, 3);
    world.0.remove(&ORIGIN.offset(2, 1, 0));
    assert!(find(&world, ORIGIN, Axis::X).is_none());
    let mut world = World::new();
    world.frame(ORIGIN, Axis::X, 2, 3);
    world.0.remove(&ORIGIN.offset(0, 3, 0));
    assert!(find(&world, ORIGIN, Axis::X).is_none());
}

#[test]
fn frames_smaller_than_two_by_three_or_larger_than_twenty_one_are_refused() {
    let mut world = World::new();
    world.frame(ORIGIN, Axis::X, 1, 3);
    assert!(find(&world, ORIGIN, Axis::X).is_none());
    let mut world = World::new();
    world.frame(ORIGIN, Axis::X, 2, 2);
    assert!(find(&world, ORIGIN, Axis::X).is_none());
    let mut world = World::new();
    world.frame(ORIGIN, Axis::X, 21, 21);
    let shape = find(&world, ORIGIN, Axis::X).expect("largest frame");
    assert_eq!((shape.width, shape.height), (21, 21));
    let mut world = World::new();
    world.frame(ORIGIN, Axis::X, 22, 5);
    assert!(find(&world, ORIGIN, Axis::X).is_none());
}

#[test]
fn an_inner_block_stops_the_search() {
    let mut world = World::new();
    world.frame(ORIGIN, Axis::X, 3, 4);
    world.set(ORIGIN.offset(1, 2, 0), BlockKind::Stone);
    assert!(find(&world, ORIGIN, Axis::X).is_none());
}

#[test]
fn fire_inside_the_frame_does_not_block_it_and_portal_blocks_make_it_not_empty() {
    let mut world = World::new();
    world.frame(ORIGIN, Axis::X, 2, 3);
    world.set(ORIGIN.offset(1, 1, 0), BlockKind::Fire);
    assert!(find(&world, ORIGIN, Axis::X).is_some());
    for cell in PortalShape::find_any(&|at| world.get(at), 0, ORIGIN, Axis::X).interior() {
        world.set(cell, BlockKind::NetherPortal);
    }
    assert!(find(&world, ORIGIN, Axis::X).is_none());
    let shape = PortalShape::find_any(&|at| world.get(at), 0, ORIGIN, Axis::X);
    assert!(shape.is_complete());
}

#[test]
fn the_touched_blocks_list_the_frame_once_then_the_interior_in_vanilla_order() {
    let mut world = World::new();
    world.frame(ORIGIN, Axis::X, 2, 3);
    let shape = find(&world, ORIGIN, Axis::X).expect("shape");
    let touched = shape.blocks_touched();
    let mut unique = touched.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), touched.len());
    let interior = shape.interior();
    assert_eq!(&touched[touched.len() - interior.len()..], &interior[..]);
    assert_eq!(touched.len(), shape.frame().len() + 6);
    assert_eq!(interior[0], BlockPos::new(10, 64, 10));
    assert_eq!(interior[1], BlockPos::new(11, 64, 10));
}

#[test]
fn the_largest_rectangle_covers_the_whole_portal_interior() {
    let mut world = World::new();
    world.frame(ORIGIN, Axis::X, 3, 4);
    for y in 0..4 {
        for x in 0..3 {
            world.set(ORIGIN.offset(x, y, 0), BlockKind::NetherPortal);
        }
    }
    let rectangle = largest_rectangle_around(ORIGIN.offset(1, 2, 0), Axis::X, 21, 21, |at| {
        world
            .get(at)
            .is_some_and(|state| state.kind() == Some(BlockKind::NetherPortal))
    });
    assert_eq!(rectangle.min_corner, ORIGIN);
    assert_eq!((rectangle.axis1_size, rectangle.axis2_size), (3, 4));
}

fn parse(text: &str) -> Vec<(i32, i32)> {
    text.split_whitespace()
        .map(|pair| {
            let (x, z) = pair.split_once(',').expect("pair");
            (x.parse().expect("x"), z.parse().expect("z"))
        })
        .collect()
}

#[test]
fn the_spiral_visits_the_square_in_the_vanilla_order() {
    let small = parse(
        "0,0 1,0 1,1 0,1 -1,1 -1,0 -1,-1 0,-1 1,-1 2,-1 2,0 2,1 2,2 1,2 0,2 -1,2 -2,2 -2,1 -2,0 -2,-1 -2,-2 -1,-2 0,-2 1,-2 2,-2",
    );
    assert_eq!(spiral(BlockPos::new(0, 0, 0), 2), small);
    let wide = spiral(BlockPos::new(0, 0, 0), 16);
    assert_eq!(wide.len(), 1089);
    let head = parse(
        "0,0 1,0 1,1 0,1 -1,1 -1,0 -1,-1 0,-1 1,-1 2,-1 2,0 2,1 2,2 1,2 0,2 -1,2 -2,2 -2,1 -2,0 -2,-1 -2,-2 -1,-2 0,-2 1,-2 2,-2 3,-2 3,-1 3,0 3,1 3,2 3,3 2,3 1,3 0,3 -1,3 -2,3 -3,3 -3,2 -3,1 -3,0",
    );
    assert_eq!(&wide[..head.len()], &head[..]);
    assert_eq!(&wide[wide.len() - 3..], &[(14, -16), (15, -16), (16, -16)]);
}

fn flat(
    world: &World,
    origin: BlockPos,
    axis: Axis,
    floor: i32,
    top: i32,
) -> Option<CreatedPortal> {
    let solid = |state: StateId| !state.is_air();
    let replaceable = |state: StateId| state.is_air();
    let block = |at: BlockPos| {
        if at.y <= floor {
            Some(BlockKind::Netherrack.default_state())
        } else {
            world.get(at)
        }
    };
    let height = |_: i32, _: i32| floor + 1;
    let inside = |_: i32, _: i32| true;
    let clamp = |at: BlockPos| at;
    let terrain = Terrain {
        block: &block,
        replaceable: &replaceable,
        solid: &solid,
        motion_blocking_height: &height,
        within_border: &inside,
        clamp_to_border: &clamp,
        min_y: 0,
        top_y: top,
    };
    terrain.create_portal(origin, axis, 16)
}

#[test]
fn a_portal_is_built_on_the_closest_floor_with_a_four_by_five_frame() {
    let world = World::new();
    let created = flat(&world, BlockPos::new(0, 64, 0), Axis::X, 60, 127).expect("portal");
    let obsidian = created
        .changes
        .iter()
        .filter(|(_, state)| state.kind() == Some(BlockKind::Obsidian))
        .count();
    let portal: Vec<_> = created
        .changes
        .iter()
        .filter(|(_, state)| state.kind() == Some(BlockKind::NetherPortal))
        .collect();
    assert_eq!(obsidian, 14);
    assert_eq!(portal.len(), 6);
    assert_eq!(portal[0].1.property("axis"), Some("x"));
    assert_eq!(created.exit.y, 61);
}

#[test]
fn a_platform_is_made_when_the_floor_cannot_host_the_frame() {
    let mut world = World::new();
    for x in -20..=20 {
        for z in -20..=20 {
            for y in 61..=100 {
                world.set(BlockPos::new(x, y, z), BlockKind::Netherrack);
            }
        }
    }
    let created = flat(&world, BlockPos::new(0, 64, 0), Axis::X, 60, 127).expect("portal");
    let air = created
        .changes
        .iter()
        .filter(|(_, state)| state.is_air())
        .count();
    assert!(air > 0);
    assert_eq!(created.exit.y, 70);
}

#[test]
fn no_portal_is_built_without_vertical_room() {
    let mut world = World::new();
    for x in -20..=20 {
        for z in -20..=20 {
            for y in 61..=70 {
                world.set(BlockPos::new(x, y, z), BlockKind::Netherrack);
            }
        }
    }
    assert!(flat(&world, BlockPos::new(0, 64, 0), Axis::X, 60, 70).is_none());
}
