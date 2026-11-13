use rspigot_blocks::BlockKind;
use rspigot_entities::EntityKind;
use rustc_hash::FxHashMap;
use uuid::Uuid;

use super::living::{self, LivingState};
use super::physics::{BlockView, MoveTraits, move_entity};
use super::{EntityBase, item};
use crate::item::ItemStack;
use crate::{BlockPos, BlockStateId, EntityId, ItemId, Vec3};

#[derive(Default)]
struct World {
    blocks: FxHashMap<BlockPos, BlockStateId>,
}

impl World {
    fn floor(kind: BlockKind) -> Self {
        let mut world = Self::default();
        for x in -4..8 {
            for z in -4..8 {
                world
                    .blocks
                    .insert(BlockPos::new(x, 63, z), kind.default_state());
            }
        }
        world
    }

    fn set(&mut self, x: i32, y: i32, z: i32, state: BlockStateId) {
        self.blocks.insert(BlockPos::new(x, y, z), state);
    }
}

impl BlockView for World {
    fn block(&self, pos: BlockPos) -> Option<BlockStateId> {
        Some(self.blocks.get(&pos).copied().unwrap_or(BlockStateId::AIR))
    }
}

fn cow_at(position: Vec3) -> (EntityBase, LivingState) {
    let mut base = EntityBase::new(EntityId(1), Uuid::from_u128(1), EntityKind::Cow);
    base.position = position;
    let living = LivingState::new(&mut base, 0.0);
    (base, living)
}

fn walker_traits(living: &mut LivingState, base: &EntityBase) -> MoveTraits {
    living.move_traits(base)
}

#[test]
fn summoned_cow_falls_two_blocks_and_settles_on_the_floor() {
    let world = World::floor(BlockKind::Stone);
    let (mut base, mut living) = cow_at(Vec3::new(0.5, 66.0, 0.5));
    let mut heights = Vec::new();
    for _ in 0..12 {
        living::ai_step(&mut base, &mut living, &world);
        heights.push(base.position.y);
    }
    assert_eq!(heights[0], 66.0);
    let drag = f64::from(0.98f32);
    assert_eq!(heights[1], 66.0 - 0.08 * drag);
    assert_eq!(heights[2], heights[1] - (0.08 * drag + 0.08) * drag);
    let landed = heights.iter().position(|y| *y == 64.0);
    assert_eq!(landed, Some(7));
    assert!(base.on_ground);
    assert_eq!(base.velocity, Vec3::new(0.0, -0.08 * drag, 0.0));
    assert_eq!(base.fall_distance, 0.0);
}

#[test]
fn walking_into_a_bottom_slab_steps_up_half_a_block() {
    let mut world = World::floor(BlockKind::Stone);
    world.set(1, 64, 0, BlockKind::StoneSlab.default_state());
    let (mut base, mut living) = cow_at(Vec3::new(0.5, 64.0, 0.5));
    base.on_ground = true;
    let traits = walker_traits(&mut living, &base);
    move_entity(&mut base, &world, Vec3::new(0.3, -0.0784, 0.0), &traits);
    assert!((base.position.y - 64.5).abs() < 1e-9);
    assert!((base.position.x - 0.8).abs() < 1e-9);
    assert!(!base.horizontal_collision);
}

#[test]
fn stairs_are_climbed_one_half_step_at_a_time() {
    let mut world = World::floor(BlockKind::Stone);
    let stairs = BlockKind::OakStairs
        .state_with(&[("facing", "east"), ("half", "bottom")])
        .unwrap_or_default();
    world.set(1, 64, 0, stairs);
    let (mut base, mut living) = cow_at(Vec3::new(0.5, 64.0, 0.5));
    base.on_ground = true;
    let traits = walker_traits(&mut living, &base);
    move_entity(&mut base, &world, Vec3::new(0.2, -0.0784, 0.0), &traits);
    assert!((base.position.y - 64.5).abs() < 1e-9);
    move_entity(&mut base, &world, Vec3::new(0.6, -0.0784, 0.0), &traits);
    assert!((base.position.y - 65.0).abs() < 1e-9);
}

#[test]
fn a_full_block_wall_stops_the_entity_flush_against_its_face() {
    let mut world = World::floor(BlockKind::Stone);
    world.set(2, 64, 0, BlockKind::Stone.default_state());
    world.set(2, 65, 0, BlockKind::Stone.default_state());
    let (mut base, mut living) = cow_at(Vec3::new(0.5, 64.0, 0.5));
    base.on_ground = true;
    base.velocity = Vec3::new(2.0, 0.0, 0.0);
    let traits = walker_traits(&mut living, &base);
    move_entity(&mut base, &world, Vec3::new(2.0, 0.0, 0.0), &traits);
    let half = f64::from(0.9f32 / 2.0);
    assert!((base.position.x + half - 2.0).abs() < 1e-9);
    assert!(base.horizontal_collision);
    assert_eq!(base.velocity.x, 0.0);
    assert_eq!(base.position.y, 64.0);
}

#[test]
fn diagonal_movement_slides_along_the_wall_of_a_corner() {
    let mut world = World::floor(BlockKind::Stone);
    for z in -2..4 {
        world.set(2, 64, z, BlockKind::Stone.default_state());
        world.set(2, 65, z, BlockKind::Stone.default_state());
    }
    let (mut base, mut living) = cow_at(Vec3::new(0.5, 64.0, 0.5));
    base.on_ground = true;
    let traits = walker_traits(&mut living, &base);
    move_entity(&mut base, &world, Vec3::new(1.5, 0.0, 0.4), &traits);
    let half = f64::from(0.9f32 / 2.0);
    assert!((base.position.x + half - 2.0).abs() < 1e-9);
    assert!((base.position.z - 0.9).abs() < 1e-9);
    assert!(base.horizontal_collision);
}

#[test]
fn ice_keeps_horizontal_speed_longer_than_stone() {
    let slide = |kind: BlockKind| {
        let world = World::floor(kind);
        let (mut base, mut living) = cow_at(Vec3::new(0.5, 64.0, 0.5));
        base.on_ground = true;
        base.velocity = Vec3::new(0.5, 0.0, 0.0);
        living::ai_step(&mut base, &mut living, &world);
        base.velocity.x
    };
    let stone = slide(BlockKind::Stone);
    let ice = slide(BlockKind::Ice);
    assert!((stone - 0.5 * f64::from(0.6f32 * 0.91f32)).abs() < 1e-9);
    assert!((ice - 0.5 * f64::from(0.98f32 * 0.91f32)).abs() < 1e-9);
}

#[test]
fn cobweb_slows_the_next_move_and_resets_the_fall() {
    let mut world = World::floor(BlockKind::Stone);
    world.set(0, 65, 0, BlockKind::Cobweb.default_state());
    let (mut base, mut living) = cow_at(Vec3::new(0.5, 65.5, 0.5));
    base.fall_distance = 3.0;
    base.velocity = Vec3::new(0.0, -0.5, 0.0);
    living::ai_step(&mut base, &mut living, &world);
    assert_eq!(base.fall_distance, 0.0);
    assert_eq!(
        base.stuck_speed_multiplier,
        Vec3::new(0.25, f64::from(0.05f32), 0.25)
    );
    let before = base.position.y;
    base.velocity = Vec3::new(0.0, -0.4, 0.0);
    living::ai_step(&mut base, &mut living, &world);
    assert!((before - base.position.y - 0.4 * f64::from(0.05f32)).abs() < 1e-9);
}

#[test]
fn a_wither_ignores_cobwebs_and_keeps_its_fall_distance() {
    let mut world = World::floor(BlockKind::Stone);
    world.set(0, 65, 0, BlockKind::Cobweb.default_state());
    let (mut base, mut living) = cow_at(Vec3::new(0.5, 65.5, 0.5));
    base.kind = EntityKind::Wither;
    base.fall_distance = 3.0;
    base.velocity = Vec3::new(0.0, -0.5, 0.0);
    living::ai_step(&mut base, &mut living, &world);
    assert_eq!(base.stuck_speed_multiplier, Vec3::ZERO);
    assert!(base.fall_distance >= 3.0);
}

#[test]
fn items_bounce_and_rest_on_the_ground() {
    let world = World::floor(BlockKind::Stone);
    let mut base = EntityBase::new(EntityId(3), Uuid::from_u128(3), EntityKind::Item);
    base.position = Vec3::new(0.5, 65.0, 0.5);
    let mut state = item::ItemState::new(
        &mut base,
        ItemStack::new(ItemId(1), std::num::NonZeroU8::MIN),
        10,
    );
    let mut next = || 0.5f32;
    for _ in 0..40 {
        base.tick_count += 1;
        item::tick_item(&mut base, &mut state, &world, &mut next);
    }
    assert_eq!(base.position.y, 64.0);
    assert!(base.on_ground);
    assert_eq!(state.pickup_delay, -30);
}

#[test]
fn spider_pushing_into_a_wall_climbs_it() {
    let mut world = World::floor(BlockKind::Stone);
    for y in 64..68 {
        world.set(2, y, 0, BlockKind::Stone.default_state());
    }
    let mut base = EntityBase::new(EntityId(1), Uuid::from_u128(1), EntityKind::Spider);
    base.position = Vec3::new(1.2, 64.0, 0.5);
    base.on_ground = true;
    let mut living = LivingState::new(&mut base, 0.0);
    base.set_yaw(-90.0);
    let mut highest = base.position.y;
    for _ in 0..30 {
        living.speed = 0.3;
        living.zza = 0.3;
        living::ai_step(&mut base, &mut living, &world);
        let climbing = base.horizontal_collision;
        living::set_climbing(&mut base, climbing);
        highest = highest.max(base.position.y);
    }
    assert!(highest > 66.0, "spider stayed at {highest}");
    let (mut cow, mut cow_state) = cow_at(Vec3::new(1.2, 64.0, 0.5));
    cow.on_ground = true;
    cow.set_yaw(-90.0);
    for _ in 0..30 {
        cow_state.speed = 0.3;
        cow_state.zza = 0.3;
        living::ai_step(&mut cow, &mut cow_state, &world);
    }
    assert!(cow.position.y < 64.7, "cow climbed to {}", cow.position.y);
}
