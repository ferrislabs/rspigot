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
}

fn parrot() -> MobProfile {
    let mut profile = MobProfile::new(0.5, 0.9).flying(true);
    profile.follow_range = 16.0;
    profile
}

fn hovering(x: f64, y: f64, z: f64) -> MobState {
    MobState {
        position: Vec3::new(x, y, z),
        on_ground: false,
        in_floatable_fluid: false,
    }
}

fn search(
    world: &World,
    profile: MobProfile,
    from: MobState,
    target: BlockPos,
    reach: i32,
) -> Path {
    PathFinder::new(profile)
        .find_path(world, &from, &[target], reach)
        .unwrap()
}

fn positions(path: &Path) -> Vec<BlockPos> {
    (0..path.node_count()).map(|i| path.node(i).pos).collect()
}

#[test]
fn a_flying_path_goes_straight_through_open_air_in_three_dimensions() {
    let world = World::empty();
    let path = search(
        &world,
        parrot(),
        hovering(0.5, 80.0, 0.5),
        BlockPos::new(6, 86, 0),
        0,
    );
    assert!(path.can_reach());
    assert_eq!(path.target(), BlockPos::new(6, 86, 0));
    let nodes = positions(&path);
    assert_eq!(nodes.first(), Some(&BlockPos::new(0, 80, 0)));
    assert_eq!(nodes.last(), Some(&BlockPos::new(6, 86, 0)));
    assert!(nodes.iter().any(|pos| pos.y > 80 && pos.y < 86));
    assert!(nodes.len() <= 13);
}

#[test]
fn the_start_node_of_a_flier_is_the_block_at_its_height_rounded_up_from_the_half() {
    let world = World::empty();
    let path = search(
        &world,
        parrot(),
        hovering(0.5, 80.6, 0.5),
        BlockPos::new(3, 81, 0),
        0,
    );
    assert_eq!(path.node(0).pos, BlockPos::new(0, 81, 0));
}

#[test]
fn a_flier_starts_above_the_water_it_swims_in_when_it_can_float() {
    let mut world = World::empty();
    world.fill((0, 78, 0), (0, 80, 0), BlockKind::Water);
    let mut state = hovering(0.5, 78.4, 0.5);
    state.in_floatable_fluid = true;
    let path = search(&world, parrot(), state, BlockPos::new(4, 83, 0), 0);
    assert_eq!(path.node(0).pos, BlockPos::new(0, 81, 0));
    let sinking = MobProfile::new(0.5, 0.9).flying(false);
    let path = search(&world, sinking, state, BlockPos::new(4, 83, 0), 0);
    assert_eq!(path.node(0).pos.y, 78);
}

#[test]
fn a_wall_is_flown_around_or_over() {
    let mut world = World::empty();
    world.fill((3, 78, -2), (3, 82, 2), BlockKind::Stone);
    let mut wide = parrot();
    wide.follow_range = 48.0;
    let path = search(
        &world,
        wide,
        hovering(0.5, 80.0, 0.5),
        BlockPos::new(8, 80, 0),
        0,
    );
    assert!(path.can_reach());
    assert!(
        positions(&path)
            .iter()
            .all(|pos| !(pos.x == 3 && pos.z.abs() <= 2 && (78..=82).contains(&pos.y)))
    );
    assert!(
        positions(&path)
            .iter()
            .any(|pos| pos.z.abs() >= 3 || pos.y > 82 || pos.y < 78)
    );
}

#[test]
fn diagonals_need_both_the_sides_to_be_passable() {
    let mut world = World::empty();
    world.set(BlockPos::new(1, 80, 0), BlockKind::Stone);
    world.set(BlockPos::new(0, 81, 0), BlockKind::Stone);
    let path = search(
        &world,
        parrot(),
        hovering(0.5, 80.0, 0.5),
        BlockPos::new(1, 81, 0),
        0,
    );
    assert!(path.node_count() > 2);
    assert_eq!(
        path.end_node().map(|node| node.pos),
        Some(BlockPos::new(1, 81, 0))
    );
    assert!(
        !positions(&path).windows(2).any(|pair| {
            pair[0] == BlockPos::new(0, 80, 0) && pair[1] == BlockPos::new(1, 81, 0)
        })
    );
}

#[test]
fn nodes_above_the_ground_get_a_walkable_surcharge_and_open_air_none() {
    let mut world = World::empty();
    world.fill((-5, 79, -5), (12, 79, 5), BlockKind::Stone);
    let path = search(
        &world,
        parrot(),
        hovering(0.5, 82.0, 0.5),
        BlockPos::new(6, 82, 0),
        0,
    );
    assert!(path.can_reach());
    assert_eq!(path.node(1).path_type, PathType::Open);
    let low = search(
        &world,
        parrot(),
        hovering(0.5, 80.0, 0.5),
        BlockPos::new(6, 80, 0),
        0,
    );
    assert_eq!(low.node(0).path_type, PathType::Walkable);
    assert!(low.node(0).cost_malus >= 0.0);
}

#[test]
fn a_flier_avoids_lava_and_fire_below_but_not_open_air() {
    let mut world = World::empty();
    world.fill((2, 79, -6), (2, 79, 6), BlockKind::Lava);
    let profile = parrot();
    let path = search(
        &world,
        profile.clone(),
        hovering(0.5, 80.0, 0.5),
        BlockPos::new(5, 80, 0),
        0,
    );
    assert!(path.can_reach());
    assert!(
        positions(&path)
            .iter()
            .filter(|pos| pos.x == 2)
            .all(|pos| pos.y > 80 || pos.z.abs() > 6)
    );
}

#[test]
fn an_enclosed_flier_cannot_start_and_reports_an_unreachable_path_through_a_wall() {
    let mut world = World::empty();
    world.fill((-1, 79, -1), (1, 81, 1), BlockKind::Stone);
    world.set(BlockPos::new(0, 80, 0), BlockKind::Air);
    let finder = PathFinder::new(parrot());
    let path = finder.find_path(
        &world,
        &hovering(0.5, 80.0, 0.5),
        &[BlockPos::new(8, 80, 0)],
        0,
    );
    assert!(path.is_none_or(|path| !path.can_reach()));
}

fn tick(state: MobState, time: i64) -> MobTick {
    MobTick {
        state,
        in_liquid: false,
        passenger: false,
        speed: 0.4,
        game_time: time,
    }
}

#[test]
fn the_flying_navigation_aims_at_the_node_without_snapping_to_the_ground() {
    let world = World::empty();
    let profile = parrot();
    let state = hovering(0.5, 80.0, 0.5);
    let mob = tick(state, 0);
    let mut navigation = Navigation::with_follow_range(16.0);
    assert!(navigation.move_to_pos(&world, &profile, &mob, Vec3::new(5.5, 84.0, 0.5), 1.0, 16.0));
    let target = navigation.tick(&world, &profile, &mob).unwrap();
    assert_eq!(target.speed_modifier, 1.0);
    let node = navigation.path().unwrap().next_node_pos();
    assert_eq!(target.position.y, f64::from(node.y));
    assert_eq!(target.position.x, f64::from(node.x) + 0.5);
}

#[test]
fn the_flying_navigation_advances_once_the_flier_is_inside_the_waypoint_box() {
    let world = World::empty();
    let profile = parrot();
    let mut mob = tick(hovering(0.5, 80.0, 0.5), 0);
    let mut navigation = Navigation::with_follow_range(16.0);
    assert!(navigation.move_to_pos(&world, &profile, &mob, Vec3::new(5.5, 80.0, 0.5), 1.0, 16.0));
    navigation.tick(&world, &profile, &mob);
    let next = navigation.path().unwrap().next_node_pos();
    mob.state.position = Vec3::new(
        f64::from(next.x) + 0.5,
        f64::from(next.y),
        f64::from(next.z) + 0.5,
    );
    let before = navigation.path().unwrap().next_node_index();
    navigation.tick(&world, &profile, &mob);
    assert!(
        navigation
            .path()
            .is_none_or(|path| path.next_node_index() > before)
    );
}

#[test]
fn a_passenger_flier_does_not_follow_the_path_but_advances_when_it_is_on_the_node() {
    let world = World::empty();
    let profile = parrot();
    let mut mob = tick(hovering(0.5, 80.0, 0.5), 0);
    let mut navigation = Navigation::with_follow_range(16.0);
    assert!(navigation.move_to_pos(&world, &profile, &mob, Vec3::new(5.5, 80.0, 0.5), 1.0, 16.0));
    mob.passenger = true;
    navigation.tick(&world, &profile, &mob);
    assert_eq!(navigation.path().unwrap().next_node_index(), 1);
    mob.state.position = Vec3::new(0.9, 80.9, 0.9);
    navigation.tick(&world, &profile, &mob);
    assert_eq!(navigation.path().unwrap().next_node_index(), 1);
    let next = navigation.path().unwrap().next_entity_pos(profile.width);
    mob.state.position = next;
    navigation.tick(&world, &profile, &mob);
    assert_eq!(navigation.path().unwrap().next_node_index(), 2);
}

#[test]
fn a_flying_navigation_paths_to_targets_below_the_surface_without_snapping_them() {
    let mut world = World::empty();
    world.fill((-8, 60, -8), (8, 60, 8), BlockKind::Stone);
    let profile = parrot();
    let mob = tick(hovering(0.5, 80.0, 0.5), 0);
    let mut navigation = Navigation::with_follow_range(16.0);
    let path = navigation
        .create_path(&world, &profile, &mob, BlockPos::new(2, 75, 2), 0, 16.0)
        .unwrap();
    assert_eq!(path.target(), BlockPos::new(2, 75, 2));
}

#[test]
fn the_flying_path_nodes_may_not_be_a_passenger_navigation_target() {
    let world = World::empty();
    let profile = parrot();
    let mut mob = tick(hovering(0.5, 80.0, 0.5), 0);
    mob.passenger = true;
    let mut navigation = Navigation::with_follow_range(16.0);
    assert!(
        navigation
            .create_path(&world, &profile, &mob, BlockPos::new(4, 80, 0), 0, 16.0)
            .is_none()
    );
}
