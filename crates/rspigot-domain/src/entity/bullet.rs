use rspigot_blocks::Face;
use rspigot_entities::EntityKind;
use uuid::Uuid;

use super::aabb::Axis;
use super::base::EntityBase;
use super::physics::BlockView;
use crate::random::{LegacyRandom, RandomSource};
use crate::{BlockPos, Vec3};

pub const GRAVITY: f64 = 0.04;
pub const DAMAGE: f32 = 4.0;
pub const LEVITATION_TICKS: u32 = 200;
pub const ROTATION_SPEED: f32 = 0.5;
pub const CLOSE_DISTANCE: f64 = 2.0;
pub const TARGET_SPEED: f64 = 0.15;
pub const TARGET_GROWTH: f64 = 1.025;
pub const STEERING: f64 = 0.2;
pub const STEP_TICKS: i32 = 10;
pub const STEP_RANDOM: i32 = 5;
pub const REROLLS: i32 = 5;
pub const HIT_PARTICLES: i32 = 2;
pub const HURT_PARTICLES: i32 = 15;
pub const PARTICLE_SPREAD: f64 = 0.2;
pub const KEYS: &[&str] = &["Steps", "TXD", "TYD", "TZD", "Dir", "Target"];

#[derive(Debug, Clone, PartialEq)]
pub struct BulletState {
    pub owner: Option<Uuid>,
    pub left_owner: bool,
    pub left_owner_checked: bool,
    pub has_been_shot: bool,
    pub target: Option<Uuid>,
    pub move_direction: Option<Face>,
    pub flight_steps: i32,
    pub target_delta: Vec3,
    pub random: LegacyRandom,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Quarry {
    pub position: Vec3,
    pub height: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pursuit {
    Lost,
    Falling,
    Homing,
}

pub fn is_shulker_bullet(kind: EntityKind) -> bool {
    kind == EntityKind::ShulkerBullet
}

pub fn axis_of(face: Face) -> Axis {
    match face {
        Face::Down | Face::Up => Axis::Y,
        Face::North | Face::South => Axis::Z,
        Face::West | Face::East => Axis::X,
    }
}

pub fn legacy_id(face: Face) -> i8 {
    face as i8
}

pub fn from_legacy_id(id: i8) -> Face {
    Face::ALL[(i32::from(id) % 6).unsigned_abs() as usize]
}

pub fn random_face(random: &mut impl RandomSource) -> Face {
    Face::ALL[random.next_int(6) as usize]
}

pub fn step(pos: BlockPos, face: Face) -> BlockPos {
    let (dx, dy, dz) = face.offset();
    pos.offset(dx, dy, dz)
}

fn center_distance_sqr(pos: BlockPos, point: Vec3) -> f64 {
    let dx = point.x - (f64::from(pos.x) + 0.5);
    let dy = point.y - (f64::from(pos.y) + 0.5);
    let dz = point.z - (f64::from(pos.z) + 0.5);
    dx * dx + dy * dy + dz * dz
}

impl BulletState {
    pub fn new(base: &mut EntityBase, seed: i64) -> Self {
        base.no_physics = true;
        Self {
            owner: None,
            left_owner: false,
            left_owner_checked: false,
            has_been_shot: false,
            target: None,
            move_direction: None,
            flight_steps: 0,
            target_delta: Vec3::ZERO,
            random: LegacyRandom::new(seed),
        }
    }
}

fn is_empty(view: &impl BlockView, pos: BlockPos) -> bool {
    view.block_or_air(pos).is_air()
}

fn candidate_faces(
    view: &impl BlockView,
    here: BlockPos,
    goal: BlockPos,
    avoid: Option<Axis>,
) -> Vec<Face> {
    let mut faces = Vec::new();
    if avoid != Some(Axis::X) {
        if here.x < goal.x && is_empty(view, step(here, Face::East)) {
            faces.push(Face::East);
        } else if goal.x < here.x && is_empty(view, step(here, Face::West)) {
            faces.push(Face::West);
        }
    }
    if avoid != Some(Axis::Y) {
        if here.y < goal.y && is_empty(view, step(here, Face::Up)) {
            faces.push(Face::Up);
        } else if goal.y < here.y && is_empty(view, step(here, Face::Down)) {
            faces.push(Face::Down);
        }
    }
    if avoid != Some(Axis::Z) {
        if here.z < goal.z && is_empty(view, step(here, Face::South)) {
            faces.push(Face::South);
        } else if goal.z < here.z && is_empty(view, step(here, Face::North)) {
            faces.push(Face::North);
        }
    }
    faces
}

pub fn select_next_move_direction(
    base: &mut EntityBase,
    state: &mut BulletState,
    avoid: Option<Axis>,
    quarry: Option<Quarry>,
    view: &impl BlockView,
) {
    let (goal, height_offset) = match quarry {
        None => (base.position.block().offset(0, -1, 0), 0.5),
        Some(quarry) => {
            let offset = f64::from(quarry.height) * 0.5;
            (
                Vec3::new(
                    quarry.position.x,
                    quarry.position.y + offset,
                    quarry.position.z,
                )
                .block(),
                offset,
            )
        }
    };
    let mut aim = Vec3::new(
        f64::from(goal.x) + 0.5,
        f64::from(goal.y) + height_offset,
        f64::from(goal.z) + 0.5,
    );
    let mut direction = None;
    if center_distance_sqr(goal, base.position) >= CLOSE_DISTANCE * CLOSE_DISTANCE {
        let here = base.position.block();
        let faces = candidate_faces(view, here, goal, avoid);
        let mut chosen = random_face(&mut state.random);
        if faces.is_empty() {
            let mut rerolls = REROLLS;
            while !is_empty(view, step(here, chosen)) && rerolls > 0 {
                chosen = random_face(&mut state.random);
                rerolls -= 1;
            }
        } else {
            chosen = faces[state.random.next_int(faces.len() as i32) as usize];
        }
        let (dx, dy, dz) = chosen.offset();
        aim = Vec3::new(
            base.position.x + f64::from(dx),
            base.position.y + f64::from(dy),
            base.position.z + f64::from(dz),
        );
        direction = Some(chosen);
    }
    state.move_direction = direction;
    let delta = aim - base.position;
    let length = delta.length();
    state.target_delta = if length == 0.0 {
        Vec3::ZERO
    } else {
        Vec3::new(
            delta.x / length * TARGET_SPEED,
            delta.y / length * TARGET_SPEED,
            delta.z / length * TARGET_SPEED,
        )
    };
    base.needs_sync = true;
    state.flight_steps = state.random.next_int(STEP_RANDOM) * STEP_TICKS + STEP_TICKS;
}

pub fn launch(
    base: &mut EntityBase,
    state: &mut BulletState,
    target: Quarry,
    avoid: Axis,
    view: &impl BlockView,
) {
    state.move_direction = Some(Face::Up);
    select_next_move_direction(base, state, Some(avoid), Some(target), view);
}

pub fn retarget(
    base: &mut EntityBase,
    state: &mut BulletState,
    quarry: Option<Quarry>,
    view: &impl BlockView,
) {
    state.move_direction = Some(Face::Up);
    select_next_move_direction(base, state, Some(Axis::X), quarry, view);
}

pub fn accelerate(base: &mut EntityBase, state: &mut BulletState, pursuit: Pursuit) {
    match pursuit {
        Pursuit::Homing => {
            let grow = |value: f64| (value * TARGET_GROWTH).clamp(-1.0, 1.0);
            state.target_delta = Vec3::new(
                grow(state.target_delta.x),
                grow(state.target_delta.y),
                grow(state.target_delta.z),
            );
            let movement = base.velocity;
            base.velocity = movement
                + Vec3::new(
                    (state.target_delta.x - movement.x) * STEERING,
                    (state.target_delta.y - movement.y) * STEERING,
                    (state.target_delta.z - movement.z) * STEERING,
                );
        }
        Pursuit::Lost | Pursuit::Falling => {
            if !base.is_no_gravity() {
                base.velocity = base.velocity + Vec3::new(0.0, -GRAVITY, 0.0);
            }
        }
    }
}

pub fn steer(
    base: &mut EntityBase,
    state: &mut BulletState,
    quarry: Quarry,
    view: &impl BlockView,
    can_stand_on: impl Fn(BlockPos) -> bool,
) {
    if state.flight_steps > 0 {
        state.flight_steps -= 1;
        if state.flight_steps == 0 {
            let axis = state.move_direction.map(axis_of);
            select_next_move_direction(base, state, axis, Some(quarry), view);
        }
    }
    let Some(direction) = state.move_direction else {
        return;
    };
    let here = base.position.block();
    let axis = axis_of(direction);
    if can_stand_on(step(here, direction)) {
        select_next_move_direction(base, state, Some(axis), Some(quarry), view);
        return;
    }
    let goal = quarry.position.block();
    let aligned = match axis {
        Axis::X => here.x == goal.x,
        Axis::Y => here.y == goal.y,
        Axis::Z => here.z == goal.z,
    };
    if aligned {
        select_next_move_direction(base, state, Some(axis), Some(quarry), view);
    }
}

#[cfg(test)]
mod tests {
    use rspigot_blocks::BlockKind;

    use super::*;
    use crate::{BlockStateId, EntityId};

    struct Open;

    impl BlockView for Open {
        fn block(&self, _pos: BlockPos) -> Option<BlockStateId> {
            Some(BlockStateId::AIR)
        }
    }

    struct Walled(BlockPos);

    impl BlockView for Walled {
        fn block(&self, pos: BlockPos) -> Option<BlockStateId> {
            Some(if pos == self.0 {
                BlockKind::Stone.default_state()
            } else {
                BlockStateId::AIR
            })
        }
    }

    fn bullet_at(position: Vec3) -> (EntityBase, BulletState) {
        let mut base = EntityBase::new(EntityId(1), Uuid::from_u128(1), EntityKind::ShulkerBullet);
        base.position = position;
        let state = BulletState::new(&mut base, 7);
        (base, state)
    }

    fn quarry_at(x: f64, y: f64, z: f64) -> Quarry {
        Quarry {
            position: Vec3::new(x, y, z),
            height: 1.8,
        }
    }

    #[test]
    fn legacy_direction_ids_follow_the_three_dimensional_data_order() {
        let order = [
            Face::Down,
            Face::Up,
            Face::North,
            Face::South,
            Face::West,
            Face::East,
        ];
        for (id, face) in order.into_iter().enumerate() {
            assert_eq!(legacy_id(face), id as i8);
            assert_eq!(from_legacy_id(id as i8), face);
        }
        assert_eq!(from_legacy_id(7), Face::Up);
        assert_eq!(from_legacy_id(-2), Face::North);
    }

    #[test]
    fn a_launch_starts_up_and_picks_a_step_count_in_tens_from_ten_to_fifty() {
        let (mut base, mut state) = bullet_at(Vec3::new(0.5, 64.5, 0.5));
        launch(
            &mut base,
            &mut state,
            quarry_at(20.5, 64.0, 0.5),
            Axis::Y,
            &Open,
        );
        assert!(base.needs_sync);
        assert!(matches!(state.flight_steps, 10 | 20 | 30 | 40 | 50));
        assert_eq!(state.move_direction, Some(Face::East));
    }

    #[test]
    fn a_far_target_is_approached_along_an_axis_that_closes_the_distance_and_is_not_avoided() {
        for seed in 0..40 {
            let (mut base, mut state) = bullet_at(Vec3::new(0.5, 64.5, 0.5));
            state.random = LegacyRandom::new(seed);
            let quarry = quarry_at(10.5, 70.0, -9.5);
            select_next_move_direction(&mut base, &mut state, Some(Axis::Y), Some(quarry), &Open);
            let direction = state.move_direction.unwrap();
            assert!(
                matches!(direction, Face::East | Face::North),
                "seed {seed} {direction:?}"
            );
            let length = state.target_delta.length();
            assert!((length - TARGET_SPEED).abs() < 1.0e-12, "{length}");
            let (dx, dy, dz) = direction.offset();
            let expected = Vec3::new(f64::from(dx), f64::from(dy), f64::from(dz)).normalize();
            assert!((state.target_delta.x - expected.x * TARGET_SPEED).abs() < 1.0e-12);
            assert!((state.target_delta.z - expected.z * TARGET_SPEED).abs() < 1.0e-12);
        }
    }

    #[test]
    fn the_candidate_draws_consume_one_face_roll_then_one_pick() {
        let (mut base, mut state) = bullet_at(Vec3::new(0.5, 64.5, 0.5));
        let mut mirror = state.random.clone();
        let quarry = quarry_at(10.5, 70.0, -9.5);
        select_next_move_direction(&mut base, &mut state, None, Some(quarry), &Open);
        let _ = mirror.next_int(6);
        let pick = mirror.next_int(3);
        let faces = [Face::East, Face::Up, Face::North];
        assert_eq!(state.move_direction, Some(faces[pick as usize]));
        let steps = mirror.next_int(5) * 10 + 10;
        assert_eq!(state.flight_steps, steps);
    }

    #[test]
    fn a_blocked_candidate_is_skipped_and_with_none_left_random_faces_are_rerolled_up_to_five_times()
     {
        let (mut base, mut state) = bullet_at(Vec3::new(0.5, 64.5, 0.5));
        let blocked = Walled(BlockPos::new(1, 64, 0));
        let quarry = quarry_at(10.5, 64.0, 0.5);
        let mut mirror = state.random.clone();
        select_next_move_direction(&mut base, &mut state, None, Some(quarry), &blocked);
        let mut face = random_face(&mut mirror);
        let mut rerolls = 5;
        while step(BlockPos::new(0, 64, 0), face) == BlockPos::new(1, 64, 0) && rerolls > 0 {
            face = random_face(&mut mirror);
            rerolls -= 1;
        }
        assert_eq!(state.move_direction, Some(face));
    }

    #[test]
    fn a_target_within_two_blocks_of_the_bullet_is_homed_on_directly_without_a_direction() {
        let (mut base, mut state) = bullet_at(Vec3::new(0.5, 64.5, 0.5));
        let quarry = quarry_at(1.5, 64.0, 0.5);
        select_next_move_direction(&mut base, &mut state, None, Some(quarry), &Open);
        assert_eq!(state.move_direction, None);
        let aim_y = 64.0 + f64::from(1.8f32) * 0.5;
        let expected = (Vec3::new(1.5, aim_y, 0.5) - Vec3::new(0.5, 64.5, 0.5))
            .normalize()
            .scale(TARGET_SPEED);
        assert!((state.target_delta.x - expected.x).abs() < 1.0e-12);
        assert!((state.target_delta.y - expected.y).abs() < 1.0e-12);
        assert_eq!(state.target_delta.z, 0.0);
    }

    #[test]
    fn without_a_target_the_bullet_aims_at_the_block_below_itself() {
        let (mut base, mut state) = bullet_at(Vec3::new(0.5, 64.5, 0.5));
        select_next_move_direction(&mut base, &mut state, None, None, &Open);
        assert_eq!(state.move_direction, None);
        assert!(state.target_delta.y < 0.0);
        assert_eq!(state.target_delta.x, 0.0);
    }

    #[test]
    fn a_homing_bullet_grows_its_target_delta_then_steers_a_fifth_of_the_way() {
        let (mut base, mut state) = bullet_at(Vec3::new(0.0, 64.0, 0.0));
        state.target_delta = Vec3::new(0.15, 0.0, -0.9);
        base.velocity = Vec3::new(0.05, 0.1, 0.0);
        accelerate(&mut base, &mut state, Pursuit::Homing);
        assert_eq!(state.target_delta.x, 0.15 * 1.025);
        assert_eq!(state.target_delta.z, -0.9 * 1.025);
        assert_eq!(base.velocity.x, 0.05 + (0.15 * 1.025 - 0.05) * 0.2);
        assert_eq!(base.velocity.y, 0.1 + (0.0 - 0.1) * 0.2);
        assert_eq!(base.velocity.z, (-0.9 * 1.025) * 0.2);
        state.target_delta = Vec3::new(0.99, 0.0, 0.0);
        accelerate(&mut base, &mut state, Pursuit::Homing);
        assert_eq!(state.target_delta.x, 1.0);
    }

    #[test]
    fn a_bullet_without_a_live_target_falls_at_four_hundredths_per_tick_with_no_drag() {
        for pursuit in [Pursuit::Lost, Pursuit::Falling] {
            let (mut base, mut state) = bullet_at(Vec3::ZERO);
            base.velocity = Vec3::new(0.3, 0.0, 0.0);
            let mut expected = 0.0;
            for tick in 0..20 {
                accelerate(&mut base, &mut state, pursuit);
                expected -= 0.04;
                assert_eq!(base.velocity.y, expected, "tick {tick}");
                assert_eq!(base.velocity.x, 0.3);
            }
        }
    }

    #[test]
    fn the_flight_timer_reselects_a_direction_on_the_tick_it_runs_out_avoiding_the_current_axis() {
        let (mut base, mut state) = bullet_at(Vec3::new(0.5, 64.5, 0.5));
        state.move_direction = Some(Face::East);
        state.flight_steps = 2;
        let quarry = quarry_at(8.5, 90.0, -20.5);
        steer(&mut base, &mut state, quarry, &Open, |_| false);
        assert_eq!(state.flight_steps, 1);
        assert_eq!(state.move_direction, Some(Face::East));
        steer(&mut base, &mut state, quarry, &Open, |_| false);
        assert!(matches!(state.move_direction, Some(Face::Up | Face::North)));
        assert!(state.flight_steps >= 10);
    }

    #[test]
    fn a_solid_block_ahead_or_alignment_with_the_target_on_the_axis_picks_a_new_direction() {
        let quarry = quarry_at(8.5, 90.0, -20.5);
        let (mut base, mut state) = bullet_at(Vec3::new(0.5, 64.5, 0.5));
        state.move_direction = Some(Face::East);
        steer(&mut base, &mut state, quarry, &Open, |pos| {
            pos == BlockPos::new(1, 64, 0)
        });
        assert!(matches!(state.move_direction, Some(Face::Up | Face::North)));
        let (mut base, mut state) = bullet_at(Vec3::new(0.5, 64.5, 0.5));
        state.move_direction = Some(Face::East);
        steer(&mut base, &mut state, quarry, &Open, |_| false);
        assert_eq!(state.flight_steps, 0);
        assert_eq!(state.move_direction, Some(Face::East));
        let aligned = quarry_at(0.5, 90.0, -20.5);
        steer(&mut base, &mut state, aligned, &Open, |_| false);
        assert!(state.flight_steps >= 10);
        assert!(matches!(state.move_direction, Some(Face::Up | Face::North)));
    }

    #[test]
    fn the_flight_state_and_the_owner_round_trip_through_the_save_format() {
        use crate::entity::save::{load_entity, save_entity};
        use rspigot_nbt::Compound;
        let target = Uuid::from_u128(0x1234_5678_9abc_def0_1122_3344_5566_7788);
        let owner = Uuid::from_u128(0x0fed_cba9_8765_4321_8877_6655_4433_2211);
        let mut compound = Compound::new().with("id", "minecraft:shulker_bullet");
        compound.insert("Steps", 17);
        compound.insert("TXD", 0.25f64);
        compound.insert("TYD", -0.5f64);
        compound.insert("TZD", 1.0f64);
        compound.insert("Dir", 4i8);
        compound.insert("Target", crate::entity::save::uuid_tag(target));
        compound.insert("Owner", crate::entity::save::uuid_tag(owner));
        compound.insert("LeftOwner", true);
        let entity = load_entity(&compound, EntityId(1), Uuid::from_u128(1)).unwrap();
        let crate::entity::EntityState::Bullet(state) = &entity.state else {
            panic!("a bullet");
        };
        assert_eq!(state.flight_steps, 17);
        assert_eq!(state.target_delta, Vec3::new(0.25, -0.5, 1.0));
        assert_eq!(state.move_direction, Some(Face::West));
        assert_eq!(state.target, Some(target));
        assert_eq!(state.owner, Some(owner));
        assert!(state.left_owner);
        assert!(entity.base.no_physics);
        let saved = save_entity(&entity, None).unwrap();
        assert_eq!(saved.get_i64("Dir"), Some(4));
        assert_eq!(saved.get_i32("Steps"), Some(17));
        let again = load_entity(&saved, EntityId(1), Uuid::from_u128(1)).unwrap();
        assert_eq!(save_entity(&again, None), Some(saved));
        let bare = load_entity(
            &Compound::new().with("id", "minecraft:shulker_bullet"),
            EntityId(1),
            Uuid::from_u128(1),
        )
        .unwrap();
        let crate::entity::EntityState::Bullet(state) = &bare.state else {
            panic!("a bullet");
        };
        assert_eq!(state.move_direction, None);
        assert_eq!(state.target, None);
        assert_eq!(state.flight_steps, 0);
        assert!(save_entity(&bare, None).unwrap().get("Dir").is_none());
    }
}
