use std::collections::HashMap;

use rspigot_ai::{
    BlockView, MobProfile, MobState, MobTick, Navigation, Path, PathFinder, PathType,
};
use rspigot_blocks::{BlockKind, StateId};
use rspigot_domain::{BlockPos, Vec3};

struct World {
    blocks: HashMap<BlockPos, StateId>,
}

impl BlockView for World {
    fn state(&self, pos: BlockPos) -> Option<StateId> {
        Some(self.blocks.get(&pos).copied().unwrap_or(StateId::AIR))
    }

    fn min_y(&self) -> i32 {
        0
    }

    fn max_y(&self) -> i32 {
        256
    }
}

impl World {
    fn empty() -> Self {
        Self {
            blocks: HashMap::new(),
        }
    }

    fn set(&mut self, pos: BlockPos, kind: BlockKind) {
        self.blocks.insert(pos, kind.default_state());
    }

    fn fill(&mut self, from: (i32, i32, i32), to: (i32, i32, i32), kind: BlockKind) {
        for x in from.0..=to.0 {
            for y in from.1..=to.1 {
                for z in from.2..=to.2 {
                    self.set(BlockPos::new(x, y, z), kind);
                }
            }
        }
    }

    fn pool() -> Self {
        let mut world = Self::empty();
        world.fill((-8, 60, -8), (12, 60, 8), BlockKind::Stone);
        world.fill((-8, 61, -8), (12, 66, 8), BlockKind::Water);
        world
    }
}

fn fish() -> MobProfile {
    MobProfile::new(0.5, 0.3)
        .water_bound(false)
        .with_malus(PathType::Water, 0.0)
}

fn dolphin() -> MobProfile {
    MobProfile::new(0.9, 0.6)
        .water_bound(true)
        .with_malus(PathType::Water, 0.0)
}

fn frog_like() -> MobProfile {
    MobProfile::new(0.6, 0.95).amphibious(false)
}

fn swimming(x: f64, y: f64, z: f64) -> MobState {
    MobState {
        position: Vec3::new(x, y, z),
        on_ground: false,
        in_floatable_fluid: true,
    }
}

fn search(world: &World, profile: MobProfile, from: MobState, target: BlockPos) -> Option<Path> {
    PathFinder::new(profile).find_path(world, &from, &[target], 0)
}

fn search_wide(
    world: &World,
    profile: MobProfile,
    from: MobState,
    target: BlockPos,
) -> Option<Path> {
    let mut finder = PathFinder::new(profile);
    finder.set_max_visited_nodes(4000);
    finder.find_path(world, &from, &[target], 0)
}

fn positions(path: &Path) -> Vec<BlockPos> {
    (0..path.node_count()).map(|i| path.node(i).pos).collect()
}

#[test]
fn a_water_bound_start_node_is_the_floored_minimum_corner_of_the_box() {
    let world = World::pool();
    let path = search(
        &world,
        fish(),
        swimming(0.6, 62.7, 0.4),
        BlockPos::new(5, 62, 0),
    )
    .unwrap();
    assert_eq!(path.node(0).pos, BlockPos::new(0, 63, 0));
}

#[test]
fn a_water_bound_path_stays_in_water_in_three_dimensions() {
    let world = World::pool();
    let path = search(
        &world,
        fish(),
        swimming(0.5, 62.0, 0.5),
        BlockPos::new(6, 65, 3),
    )
    .unwrap();
    assert!(path.can_reach());
    let nodes = positions(&path);
    assert_eq!(nodes.last(), Some(&BlockPos::new(6, 65, 3)));
    assert!(nodes.iter().all(|pos| (61..=66).contains(&pos.y)));
    assert!(nodes.iter().any(|pos| pos.y > 62 && pos.y < 65));
    assert!(
        path.nodes()
            .iter()
            .skip(1)
            .all(|node| node.path_type == PathType::Water)
    );
}

#[test]
fn air_above_the_water_is_blocked_unless_the_mob_may_breach() {
    let mut world = World::pool();
    world.fill((-8, 67, -8), (12, 70, 8), BlockKind::Air);
    let path = search(
        &world,
        fish(),
        swimming(0.5, 65.0, 0.5),
        BlockPos::new(4, 67, 0),
    )
    .unwrap();
    assert!(!path.can_reach());
    assert!(positions(&path).iter().all(|pos| pos.y <= 66));
    let breaching = search_wide(
        &world,
        dolphin(),
        swimming(0.5, 65.0, 0.5),
        BlockPos::new(4, 67, 0),
    )
    .unwrap();
    assert!(breaching.can_reach());
    let last = breaching.end_node().unwrap();
    assert_eq!(last.pos, BlockPos::new(4, 67, 0));
    assert_eq!(last.path_type, PathType::Breach);
    assert_eq!(last.cost_malus, 4.0 + 8.0);
}

#[test]
fn the_swim_classification_is_water_breach_or_blocked() {
    let mut world = World::pool();
    world.set(BlockPos::new(2, 64, 2), BlockKind::Stone);
    let mut profile = dolphin();
    profile.can_pass_doors = false;
    let finder = PathFinder::new(profile);
    let path = finder.find_path(
        &world,
        &swimming(0.5, 62.0, 0.5),
        &[BlockPos::new(2, 64, 2)],
        1,
    );
    let path = path.unwrap();
    assert!(path.can_reach());
    let end = path.end_node().unwrap();
    assert_ne!(end.pos, BlockPos::new(2, 64, 2));
}

#[test]
fn a_larger_mob_needs_its_whole_box_in_water() {
    let mut world = World::pool();
    world.fill((-8, 61, -8), (12, 62, 8), BlockKind::Stone);
    world.fill((-8, 63, -8), (12, 66, 8), BlockKind::Water);
    world.fill((3, 63, -8), (3, 63, 8), BlockKind::Stone);
    let thin = search(
        &world,
        fish(),
        swimming(0.5, 64.0, 0.5),
        BlockPos::new(6, 64, 0),
    )
    .unwrap();
    assert!(thin.can_reach());
    let tall = MobProfile::new(0.5, 3.0)
        .water_bound(false)
        .with_malus(PathType::Water, 0.0);
    let path = search(
        &world,
        tall,
        swimming(0.5, 62.5, 0.5),
        BlockPos::new(6, 64, 0),
    );
    assert!(path.is_none_or(|path| path.nodes().iter().all(|node| node.pos.y >= 63)));
}

#[test]
fn nodes_in_water_without_fluid_cost_eight_more() {
    let mut world = World::pool();
    world.fill((-8, 67, -8), (12, 68, 8), BlockKind::Air);
    let path = search_wide(
        &world,
        dolphin(),
        swimming(0.5, 66.0, 0.5),
        BlockPos::new(3, 68, 0),
    )
    .unwrap();
    let breach: Vec<_> = path
        .nodes()
        .iter()
        .filter(|node| node.path_type == PathType::Breach)
        .collect();
    assert!(!breach.is_empty());
    assert!(breach.iter().all(|node| node.cost_malus >= 12.0));
    assert!(
        path.nodes()
            .iter()
            .filter(|node| node.path_type == PathType::Water)
            .all(|node| node.cost_malus == 0.0)
    );
}

#[test]
fn a_water_bound_navigation_cannot_path_out_of_water_unless_breaching() {
    let world = World::pool();
    let dry = MobTick {
        state: MobState {
            position: Vec3::new(0.5, 62.0, 0.5),
            on_ground: false,
            in_floatable_fluid: false,
        },
        in_liquid: false,
        passenger: false,
        speed: 0.2,
        game_time: 0,
    };
    let mut navigation = Navigation::with_follow_range(16.0);
    assert!(Navigation::can_update_path_for(&dolphin(), &dry));
    assert!(!Navigation::can_update_path_for(&fish(), &dry));
    assert!(
        navigation
            .create_path(&world, &fish(), &dry, BlockPos::new(4, 62, 0), 1, 16.0)
            .is_none()
    );
    let mut wet = dry;
    wet.in_liquid = true;
    assert!(Navigation::can_update_path_for(&fish(), &wet));
    assert!(
        navigation
            .create_path(&world, &fish(), &wet, BlockPos::new(4, 62, 0), 1, 16.0)
            .is_some()
    );
    assert!(Navigation::can_update_path_for(&frog_like(), &dry));
}

#[test]
fn a_water_bound_navigation_aims_at_the_node_height_and_waits_half_a_block_vertically() {
    let world = World::pool();
    let profile = fish();
    let mut mob = MobTick {
        state: swimming(0.5, 62.0, 0.5),
        in_liquid: true,
        passenger: false,
        speed: 0.2,
        game_time: 0,
    };
    let mut navigation = Navigation::with_follow_range(16.0);
    let path = navigation.create_path(&world, &profile, &mob, BlockPos::new(1, 62, 0), 0, 16.0);
    assert!(navigation.move_to(path, 1.0, &world, &profile, &mob));
    let target = navigation.tick(&world, &profile, &mob).unwrap();
    let node = navigation.path().unwrap().next_node_pos();
    assert_eq!(node, BlockPos::new(1, 62, 0));
    assert_eq!(target.position.y, f64::from(node.y));
    assert_eq!(profile.max_vertical_distance_to_waypoint(), 0.5);
    let before = navigation.path().unwrap().next_node_index();
    mob.state.position = Vec3::new(1.5, 62.6, 0.5);
    navigation.tick(&world, &profile, &mob);
    assert_eq!(navigation.path().unwrap().next_node_index(), before);
    mob.state.position = Vec3::new(1.5, 62.4, 0.5);
    navigation.tick(&world, &profile, &mob);
    assert!(navigation.path().is_none_or(|path| path.is_done()));
}

#[test]
fn a_water_bound_navigation_cuts_the_corner_when_the_line_to_the_next_node_is_clear() {
    let world = World::pool();
    let profile = fish();
    let mob = MobTick {
        state: swimming(0.5, 62.0, 0.5),
        in_liquid: true,
        passenger: false,
        speed: 0.2,
        game_time: 0,
    };
    let mut navigation = Navigation::with_follow_range(16.0);
    assert!(navigation.move_to_pos(&world, &profile, &mob, Vec3::new(4.5, 62.5, 0.5), 1.0, 16.0));
    let first = navigation.path().unwrap().next_node_pos();
    let mut near = mob;
    near.state.position = Vec3::new(
        f64::from(first.x) + 1.4,
        f64::from(first.y) + 0.3,
        f64::from(first.z) + 0.5,
    );
    navigation.tick(&world, &profile, &near);
    assert!(navigation.path().unwrap().next_node_index() >= 1);
}

#[test]
fn an_amphibious_search_walks_on_land_swims_in_water_and_prefers_the_water() {
    let mut world = World::empty();
    world.fill((-8, 62, -8), (12, 62, 8), BlockKind::Stone);
    world.fill((3, 63, -8), (12, 66, 8), BlockKind::Water);
    world.fill((3, 62, -8), (12, 62, 8), BlockKind::Stone);
    let state = MobState {
        position: Vec3::new(0.5, 63.0, 0.5),
        on_ground: true,
        in_floatable_fluid: false,
    };
    let path = search(&world, frog_like(), state, BlockPos::new(8, 64, 0)).unwrap();
    assert!(path.can_reach());
    let kinds: Vec<_> = path.nodes().iter().map(|node| node.path_type).collect();
    assert!(kinds.contains(&PathType::Walkable));
    assert!(
        kinds
            .iter()
            .any(|kind| matches!(kind, PathType::Water | PathType::WaterBorder))
    );
    let costs: Vec<_> = path
        .nodes()
        .iter()
        .filter(|node| node.path_type == PathType::Walkable)
        .map(|node| node.cost_malus)
        .collect();
    assert!(costs.iter().all(|cost| *cost == 6.0));
}

#[test]
fn amphibious_water_touching_a_wall_is_a_water_border_with_a_malus_of_four() {
    let mut world = World::empty();
    world.fill((-4, 60, -4), (8, 60, 4), BlockKind::Stone);
    world.fill((-4, 61, -4), (8, 64, 4), BlockKind::Water);
    let state = swimming(0.5, 62.0, 0.5);
    let path = search(&world, frog_like(), state, BlockPos::new(6, 62, 0)).unwrap();
    assert!(path.can_reach());
    let near_floor = path
        .nodes()
        .iter()
        .find(|node| node.pos.y == 61 && node.path_type == PathType::WaterBorder);
    if let Some(node) = near_floor {
        assert_eq!(node.cost_malus, 4.0);
    }
    assert!(
        path.nodes()
            .iter()
            .filter(|node| node.path_type == PathType::Water)
            .all(|node| node.cost_malus == 0.0)
    );
}

#[test]
fn an_amphibious_swimmer_moves_up_and_down_through_water_columns() {
    let mut world = World::empty();
    world.fill((-4, 60, -4), (4, 60, 4), BlockKind::Stone);
    world.fill((-4, 61, -4), (4, 70, 4), BlockKind::Water);
    let path = search(
        &world,
        frog_like(),
        swimming(0.5, 62.0, 0.5),
        BlockPos::new(0, 68, 0),
    )
    .unwrap();
    assert!(path.can_reach());
    assert!(
        positions(&path)
            .windows(2)
            .all(|pair| pair[0].x == 0 && pair[0].z == 0 && pair[1].y > pair[0].y)
    );
}

#[test]
fn a_shallow_swimmer_pays_one_more_for_water_deeper_than_ten_under_the_sea_level() {
    let mut world = World::empty();
    world.fill((-4, 40, -4), (8, 40, 4), BlockKind::Stone);
    world.fill((-4, 41, -4), (8, 70, 4), BlockKind::Water);
    let mut shallow = MobProfile::new(0.6, 0.95).amphibious(true);
    shallow.sea_level = 63;
    let deep = search(
        &world,
        shallow,
        swimming(0.5, 45.0, 0.5),
        BlockPos::new(5, 45, 0),
    )
    .unwrap();
    assert!(
        deep.nodes()
            .iter()
            .skip(1)
            .filter(|node| node.path_type == PathType::Water)
            .all(|node| node.cost_malus == 1.0)
    );
    let plain = search(
        &world,
        MobProfile::new(0.6, 0.95).amphibious(false),
        swimming(0.5, 45.0, 0.5),
        BlockPos::new(5, 45, 0),
    )
    .unwrap();
    assert!(
        plain
            .nodes()
            .iter()
            .filter(|node| node.path_type == PathType::Water)
            .all(|node| node.cost_malus == 0.0)
    );
}

#[test]
fn an_amphibious_navigation_always_updates_and_a_water_bound_one_ignores_the_ground() {
    let profile = frog_like();
    let mob = MobTick {
        state: MobState {
            position: Vec3::new(0.5, 63.0, 0.5),
            on_ground: false,
            in_floatable_fluid: false,
        },
        in_liquid: false,
        passenger: false,
        speed: 0.2,
        game_time: 0,
    };
    assert!(Navigation::can_update_path_for(&profile, &mob));
    assert_eq!(profile.max_vertical_distance_to_waypoint(), 1.0);
}
