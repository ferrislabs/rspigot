use std::borrow::Cow;

use rspigot_blocks::{BlockKind, Face};
use rspigot_nbt::Compound;

use crate::effect::{AttributeModifier, AttributeOperation};
use crate::entity::Aabb;
use crate::entity::base::EntityBase;
use crate::entity::data::SyncedValue;
use crate::entity::living::{ARMOR, LivingState};
use crate::entity::physics::{self, BlockView};
use crate::entity::spawn::{SpawnEnvironment, mob_finalize};
use crate::mth;
use crate::random::RandomSource;
use crate::spawn::SpawnGroupData;
use crate::{BlockPos, Vec3};

pub const DATA_ATTACH_FACE: u8 = 16;
pub const DATA_PEEK: u8 = 17;
pub const DATA_COLOR: u8 = 18;
pub const NO_COLOR: i8 = 16;
pub const KEYS: &[&str] = &["AttachFace", "Peek", "Color"];
pub const COVERED_ARMOR_ID: &str = "minecraft:covered";
pub const COVERED_ARMOR: f64 = 20.0;
pub const PEEK_STEP: f32 = 0.05;
pub const RAW_PEEK_SCALE: f32 = 0.01;
pub const ATTACK_PEEK: i32 = 100;
pub const IDLE_PEEK: i32 = 30;
pub const SCALE: f32 = 1.0;
pub const TELEPORT_ATTEMPTS: i32 = 5;
pub const TELEPORT_RANGE: i32 = 8;
pub const TELEPORT_TOLERANCE: f64 = 1.0e-6;
pub const MAX_HEAD_ROTATION: f32 = 180.0;
pub const SHOOT_RANGE_SQR: f64 = 400.0;
pub const SHOOT_COOLDOWN: i32 = 20;
pub const SHOOT_COOLDOWN_RANDOM: i32 = 10;
pub const PEEK_TIME_RANDOM: i32 = 3;
pub const PEEK_TIME_UNIT: i32 = 20;
pub const PEEK_ODDS: i32 = 40;
pub const BREEDING_RADIUS: f64 = 8.0;
pub const BREEDING_DIVISOR: f32 = 5.0;
pub const TELEPORT_HEALTH_RATIO: f32 = 0.5;
pub const TELEPORT_ODDS: i32 = 4;
pub const TELEPORT_LEVEL_EVENT: i32 = 0x7e0;
pub const TELEPORT_BLOCKED_TAG: &str = "minecraft:shulker_does_not_teleport_to";
pub const SHOOT_SOUND_VOLUME: f32 = 2.0;
pub const SHOOT_SOUND_PITCH_SPREAD: f32 = 0.2;
const FACE_EPSILON: f64 = 1.0e-7;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ShulkerState {
    pub peek: f32,
    pub peek_old: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeekChange {
    Opened,
    Closed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hop {
    pub target: BlockPos,
    pub face: Face,
}

pub fn define(base: &mut EntityBase) -> ShulkerState {
    base.data
        .define(DATA_ATTACH_FACE, SyncedValue::Direction(Face::Down));
    base.data.define(DATA_PEEK, SyncedValue::Byte(0));
    base.data.define(DATA_COLOR, SyncedValue::Byte(NO_COLOR));
    let state = ShulkerState::default();
    refresh_shape(base, &state);
    state
}

pub fn attach_face(base: &EntityBase) -> Face {
    base.data.direction(DATA_ATTACH_FACE)
}

pub fn set_attach_face(base: &mut EntityBase, state: &ShulkerState, face: Face) {
    base.data
        .set(DATA_ATTACH_FACE, SyncedValue::Direction(face));
    refresh_shape(base, state);
}

pub fn raw_peek(base: &EntityBase) -> i32 {
    i32::from(base.data.byte(DATA_PEEK))
}

pub fn is_closed(base: &EntityBase) -> bool {
    raw_peek(base) == 0
}

pub fn set_raw_peek(base: &mut EntityBase, living: &mut LivingState, amount: i32) -> PeekChange {
    living.attributes.remove_modifier(ARMOR, COVERED_ARMOR_ID);
    let change = if amount == 0 {
        living.attributes.add_modifier(
            ARMOR,
            AttributeModifier {
                id: Cow::Borrowed(COVERED_ARMOR_ID),
                amount: COVERED_ARMOR,
                operation: AttributeOperation::AddValue,
            },
        );
        PeekChange::Closed
    } else {
        PeekChange::Opened
    };
    base.data.set(DATA_PEEK, SyncedValue::Byte(amount as i8));
    change
}

pub fn color(base: &EntityBase) -> Option<u8> {
    let raw = base.data.byte(DATA_COLOR);
    (0..NO_COLOR).contains(&raw).then_some(raw as u8)
}

pub fn set_color(base: &mut EntityBase, color: Option<u8>) {
    let raw = color.map_or(NO_COLOR, |color| color as i8);
    base.data.set(DATA_COLOR, SyncedValue::Byte(raw));
}

pub fn physical_peek(amount: f32) -> f32 {
    0.5 - mth::sin(f64::from((amount + 0.5) * std::f32::consts::PI)) * 0.5
}

fn contract(area: Aabb, delta: Vec3) -> Aabb {
    let mut out = area;
    if delta.x < 0.0 {
        out.min_x -= delta.x;
    } else if delta.x > 0.0 {
        out.max_x -= delta.x;
    }
    if delta.y < 0.0 {
        out.min_y -= delta.y;
    } else if delta.y > 0.0 {
        out.max_y -= delta.y;
    }
    if delta.z < 0.0 {
        out.min_z -= delta.z;
    } else if delta.z > 0.0 {
        out.max_z -= delta.z;
    }
    out
}

pub fn progress_delta_aabb(size: f32, direction: Face, from: f32, to: f32, position: Vec3) -> Aabb {
    let half = f64::from(size * 0.5);
    let base = Aabb::new(-half, 0.0, -half, half, f64::from(size), half);
    let far = f64::from(from.max(to));
    let near = f64::from(from.min(to));
    let size = f64::from(size);
    let (dx, dy, dz) = direction.offset();
    let (dx, dy, dz) = (f64::from(dx), f64::from(dy), f64::from(dz));
    let grown = base.expand_towards(Vec3::new(dx * far * size, dy * far * size, dz * far * size));
    let shrunk = contract(
        grown,
        Vec3::new(
            -dx * (near + 1.0) * size,
            -dy * (near + 1.0) * size,
            -dz * (near + 1.0) * size,
        ),
    );
    shrunk.move_by(position)
}

pub fn progress_aabb(size: f32, direction: Face, to: f32, position: Vec3) -> Aabb {
    progress_delta_aabb(size, direction, -1.0, to, position)
}

pub fn dimensions_for(
    kind: rspigot_entities::EntityKind,
    face: Face,
    peek: f32,
) -> rspigot_entities::EntityDimensions {
    let base = kind.dimensions();
    if face == Face::Down && peek > 0.0 {
        let scale = peek + 1.0;
        rspigot_entities::EntityDimensions {
            width: base.width,
            height: base.height * scale,
            eye_height: base.eye_height * scale,
        }
    } else {
        base
    }
}

pub fn refresh_shape(base: &mut EntityBase, state: &ShulkerState) {
    let face = attach_face(base);
    base.dimensions = dimensions_for(base.kind, face, state.peek);
    base.custom_box = Some(progress_aabb(
        SCALE,
        face.opposite(),
        physical_peek(state.peek),
        Vec3::ZERO,
    ));
}

pub fn update_peek(base: &EntityBase, state: &mut ShulkerState) -> bool {
    state.peek_old = state.peek;
    let target = raw_peek(base) as f32 * RAW_PEEK_SCALE;
    if state.peek == target {
        return false;
    }
    state.peek = if state.peek <= target {
        mth::clamp(state.peek + PEEK_STEP, 0.0, target)
    } else {
        mth::clamp(state.peek - PEEK_STEP, target, 1.0)
    };
    true
}

pub fn peek_sweep(base: &EntityBase, state: &ShulkerState) -> Aabb {
    let lid = attach_face(base).opposite();
    progress_delta_aabb(
        SCALE,
        lid,
        physical_peek(state.peek_old),
        physical_peek(state.peek),
        base.position,
    )
}

pub fn peek_push(base: &EntityBase, state: &ShulkerState) -> (Face, f32) {
    let lid = attach_face(base).opposite();
    let delta = physical_peek(state.peek) - physical_peek(state.peek_old);
    (lid, delta * SCALE)
}

pub fn snapped(position: Vec3) -> Vec3 {
    Vec3::new(
        position.x.floor() + 0.5,
        (position.y + 0.5).floor(),
        position.z.floor() + 0.5,
    )
}

pub fn set_position(base: &mut EntityBase, position: Vec3) {
    let before = base.position.block();
    base.position = snapped(position);
    if base.tick_count != 0 && base.position.block() != before {
        base.data.set(DATA_PEEK, SyncedValue::Byte(0));
        base.needs_sync = true;
    }
}

fn covers_unit_square(boxes: &[(f64, f64, f64, f64)]) -> bool {
    let mut us = vec![0.0, 1.0];
    let mut vs = vec![0.0, 1.0];
    for (u0, u1, v0, v1) in boxes {
        us.extend([*u0, *u1]);
        vs.extend([*v0, *v1]);
    }
    us.retain(|u| (0.0..=1.0).contains(u));
    vs.retain(|v| (0.0..=1.0).contains(v));
    us.sort_by(f64::total_cmp);
    vs.sort_by(f64::total_cmp);
    us.dedup();
    vs.dedup();
    us.windows(2).all(|u| {
        vs.windows(2).all(|v| {
            let (cu, cv) = ((u[0] + u[1]) / 2.0, (v[0] + v[1]) / 2.0);
            u[1] - u[0] < FACE_EPSILON
                || v[1] - v[0] < FACE_EPSILON
                || boxes
                    .iter()
                    .any(|(u0, u1, v0, v1)| *u0 <= cu && cu <= *u1 && *v0 <= cv && cv <= *v1)
        })
    })
}

pub fn is_face_full(shapes: &[rspigot_blocks::Aabb], face: Face) -> bool {
    let touching: Vec<(f64, f64, f64, f64)> = shapes
        .iter()
        .filter_map(|shape| match face {
            Face::Up if shape.max_y > 1.0 - FACE_EPSILON => {
                Some((shape.min_x, shape.max_x, shape.min_z, shape.max_z))
            }
            Face::Down if shape.min_y < FACE_EPSILON => {
                Some((shape.min_x, shape.max_x, shape.min_z, shape.max_z))
            }
            Face::South if shape.max_z > 1.0 - FACE_EPSILON => {
                Some((shape.min_x, shape.max_x, shape.min_y, shape.max_y))
            }
            Face::North if shape.min_z < FACE_EPSILON => {
                Some((shape.min_x, shape.max_x, shape.min_y, shape.max_y))
            }
            Face::East if shape.max_x > 1.0 - FACE_EPSILON => {
                Some((shape.min_z, shape.max_z, shape.min_y, shape.max_y))
            }
            Face::West if shape.min_x < FACE_EPSILON => {
                Some((shape.min_z, shape.max_z, shape.min_y, shape.max_y))
            }
            _ => None,
        })
        .collect();
    !touching.is_empty() && covers_unit_square(&touching)
}

pub fn can_stand_on_face(view: &impl BlockView, pos: BlockPos, face: Face) -> bool {
    view.block(pos)
        .is_some_and(|state| is_face_full(state.collision_shape(), face))
}

fn position_blocked(view: &impl BlockView, own: BlockPos, pos: BlockPos) -> bool {
    let state = view.block_or_air(pos);
    if state.is_air() {
        return false;
    }
    !(state.kind() == Some(BlockKind::MovingPiston) && pos == own)
}

pub fn can_stay_at(
    view: &impl BlockView,
    own: BlockPos,
    pos: BlockPos,
    face: Face,
    entity_free: impl Fn(&Aabb) -> bool,
) -> bool {
    if position_blocked(view, own, pos) {
        return false;
    }
    let (dx, dy, dz) = face.offset();
    if !can_stand_on_face(view, pos.offset(dx, dy, dz), face.opposite()) {
        return false;
    }
    let bottom = Vec3::new(
        f64::from(pos.x) + 0.5,
        f64::from(pos.y),
        f64::from(pos.z) + 0.5,
    );
    let area = progress_aabb(SCALE, face.opposite(), 1.0, bottom).deflate(TELEPORT_TOLERANCE);
    physics::block_colliders(view, &area).is_empty() && entity_free(&area)
}

pub fn find_attachable_surface(
    view: &impl BlockView,
    own: BlockPos,
    pos: BlockPos,
    entity_free: &impl Fn(&Aabb) -> bool,
) -> Option<Face> {
    Face::ALL
        .into_iter()
        .find(|face| can_stay_at(view, own, pos, *face, entity_free))
}

pub fn find_teleport(
    view: &impl BlockView,
    random: &mut impl RandomSource,
    origin: BlockPos,
    min_y: i32,
    within_border: impl Fn(BlockPos) -> bool,
    entity_free: impl Fn(&Aabb) -> bool,
) -> Option<Hop> {
    for _ in 0..TELEPORT_ATTEMPTS {
        let dx = random.next_int_between(-TELEPORT_RANGE, TELEPORT_RANGE);
        let dy = random.next_int_between(-TELEPORT_RANGE, TELEPORT_RANGE);
        let dz = random.next_int_between(-TELEPORT_RANGE, TELEPORT_RANGE);
        let target = origin.offset(dx, dy, dz);
        if target.y <= min_y || !view.block_or_air(target).is_air() || !within_border(target) {
            continue;
        }
        let area = Aabb::block(target).deflate(TELEPORT_TOLERANCE);
        if !physics::block_colliders(view, &area).is_empty() || !entity_free(&area) {
            continue;
        }
        let Some(face) = find_attachable_surface(view, origin, target, &entity_free) else {
            continue;
        };
        let (fx, fy, fz) = face.offset();
        let support = view.block_or_air(target.offset(fx, fy, fz));
        if physics::has_tag(support, TELEPORT_BLOCKED_TAG) {
            continue;
        }
        return Some(Hop { target, face });
    }
    None
}

pub fn should_teleport_when_hurt(health: f32, max_health: f32, roll: i32) -> bool {
    health < max_health * TELEPORT_HEALTH_RATIO && roll == 0
}

pub fn breeding_chance(nearby: usize) -> f32 {
    (nearby as f32 - 1.0) / BREEDING_DIVISOR
}

pub fn read(nbt: &Compound, base: &mut EntityBase, state: &mut ShulkerState) {
    let face = nbt.get_i64("AttachFace").map_or(Face::Down, |id| {
        crate::entity::bullet::from_legacy_id(id as i8)
    });
    set_attach_face(base, state, face);
    base.data.set(
        DATA_PEEK,
        SyncedValue::Byte(nbt.get_i64("Peek").map_or(0, |peek| peek as i8)),
    );
    base.data.set(
        DATA_COLOR,
        SyncedValue::Byte(nbt.get_i64("Color").map_or(NO_COLOR, |color| color as i8)),
    );
}

pub fn write(base: &EntityBase, _state: &ShulkerState, out: &mut Compound) {
    out.insert(
        "AttachFace",
        crate::entity::bullet::legacy_id(attach_face(base)),
    );
    out.insert("Peek", base.data.byte(DATA_PEEK));
    out.insert("Color", base.data.byte(DATA_COLOR));
}

pub fn finalize(
    base: &mut EntityBase,
    living: &mut LivingState,
    _environment: &SpawnEnvironment,
    group: Option<SpawnGroupData>,
    level: &mut impl RandomSource,
) -> Option<SpawnGroupData> {
    base.set_yaw(0.0);
    base.head_yaw = base.yaw;
    base.old_position = base.position;
    mob_finalize(base, living, group, level)
}

#[cfg(test)]
mod tests {
    use rspigot_blocks::BlockKind;
    use uuid::Uuid;

    use super::*;
    use crate::entity::save::{load_entity, save_entity};
    use crate::random::LegacyRandom;
    use crate::{BlockStateId, EntityId};

    struct Floor(i32);

    impl BlockView for Floor {
        fn block(&self, pos: BlockPos) -> Option<BlockStateId> {
            Some(if pos.y <= self.0 {
                BlockKind::Stone.default_state()
            } else {
                BlockStateId::AIR
            })
        }
    }

    fn shulker(nbt: Compound) -> crate::entity::Entity {
        load_entity(
            &nbt.with("id", "minecraft:shulker"),
            EntityId(1),
            Uuid::from_u128(1),
        )
        .unwrap()
    }

    fn state_of(entity: &mut crate::entity::Entity) -> (&mut EntityBase, &mut LivingState) {
        let crate::entity::EntityState::Living(living) = &mut entity.state else {
            panic!("a shulker is a living entity");
        };
        (&mut entity.base, living)
    }

    fn shulker_state(entity: &crate::entity::Entity) -> ShulkerState {
        match entity.living().and_then(|living| living.mob.as_ref()) {
            Some(mob) => match mob.species {
                crate::entity::species::Species::Shulker(state) => state,
                _ => panic!("not a shulker"),
            },
            None => panic!("not a mob"),
        }
    }

    #[test]
    fn the_synched_data_sits_at_sixteen_to_eighteen_with_vanilla_defaults() {
        let entity = shulker(Compound::new());
        assert_eq!(
            entity.base.data.get(DATA_ATTACH_FACE),
            Some(&SyncedValue::Direction(Face::Down))
        );
        assert_eq!(entity.base.data.get(DATA_PEEK), Some(&SyncedValue::Byte(0)));
        assert_eq!(
            entity.base.data.get(DATA_COLOR),
            Some(&SyncedValue::Byte(16))
        );
        assert_eq!(color(&entity.base), None);
    }

    #[test]
    fn the_attach_face_peek_and_colour_round_trip_through_the_save_format() {
        let mut entity = shulker(
            Compound::new()
                .with("AttachFace", 4i8)
                .with("Peek", 30i8)
                .with("Color", 5i8),
        );
        assert_eq!(attach_face(&entity.base), Face::West);
        assert_eq!(raw_peek(&entity.base), 30);
        assert_eq!(color(&entity.base), Some(5));
        let saved = save_entity(&entity, None).unwrap();
        assert_eq!(saved.get_i64("AttachFace"), Some(4));
        assert_eq!(saved.get_i64("Peek"), Some(30));
        assert_eq!(saved.get_i64("Color"), Some(5));
        let again = load_entity(&saved, EntityId(1), Uuid::from_u128(1)).unwrap();
        assert_eq!(save_entity(&again, None), Some(saved));
        let (base, _) = state_of(&mut entity);
        set_color(base, None);
        assert_eq!(base.data.byte(DATA_COLOR), 16);
        let defaults = shulker(Compound::new());
        let saved = save_entity(&defaults, None).unwrap();
        assert_eq!(saved.get_i64("AttachFace"), Some(0));
        assert_eq!(saved.get_i64("Color"), Some(16));
    }

    #[test]
    fn the_physical_peek_is_zero_closed_one_open_and_the_box_grows_along_the_lid() {
        assert_eq!(physical_peek(0.0), 0.0);
        assert!((physical_peek(1.0) - 1.0).abs() < 1.0e-3);
        let closed = progress_aabb(1.0, Face::Up, 0.0, Vec3::new(0.5, 64.0, 0.5));
        assert_eq!(closed, Aabb::new(0.0, 64.0, 0.0, 1.0, 65.0, 1.0));
        let open = progress_aabb(1.0, Face::Up, 1.0, Vec3::new(0.5, 64.0, 0.5));
        assert_eq!(open, Aabb::new(0.0, 64.0, 0.0, 1.0, 66.0, 1.0));
        let sideways = progress_aabb(1.0, Face::East, 0.5, Vec3::new(0.5, 64.0, 0.5));
        assert_eq!(sideways, Aabb::new(0.0, 64.0, 0.0, 1.5, 65.0, 1.0));
        let west = progress_aabb(1.0, Face::West, 1.0, Vec3::new(0.5, 64.0, 0.5));
        assert_eq!(west, Aabb::new(-1.0, 64.0, 0.0, 1.0, 65.0, 1.0));
    }

    #[test]
    fn the_delta_box_is_the_slab_swept_between_two_physical_peeks() {
        let slab = progress_delta_aabb(1.0, Face::Up, 0.25, 0.75, Vec3::new(0.5, 64.0, 0.5));
        assert_eq!(slab, Aabb::new(0.0, 65.25, 0.0, 1.0, 65.75, 1.0));
        let reversed = progress_delta_aabb(1.0, Face::Up, 0.75, 0.25, Vec3::new(0.5, 64.0, 0.5));
        assert_eq!(reversed, slab);
    }

    #[test]
    fn the_peek_amount_climbs_and_falls_in_steps_of_five_hundredths_towards_the_raw_value() {
        let mut entity = shulker(Compound::new().with("Peek", 30i8));
        let (base, _) = state_of(&mut entity);
        let mut state = ShulkerState::default();
        let mut seen = Vec::new();
        while update_peek(base, &mut state) {
            seen.push(state.peek);
        }
        assert_eq!(seen, vec![0.05, 0.1, 0.15, 0.2, 0.25, 0.29999998]);
        assert_eq!(state.peek, 30.0f32 * 0.01);
        base.data.set(DATA_PEEK, SyncedValue::Byte(0));
        let mut down = 0;
        while update_peek(base, &mut state) {
            down += 1;
            assert!(state.peek >= 0.0);
        }
        assert_eq!(down, 6);
        assert_eq!(state.peek, 0.0);
    }

    #[test]
    fn closing_adds_twenty_armor_and_opening_removes_it_with_the_matching_change() {
        let mut entity = shulker(Compound::new());
        let (base, living) = state_of(&mut entity);
        assert_eq!(living.attribute(ARMOR), 0.0);
        assert_eq!(set_raw_peek(base, living, 0), PeekChange::Closed);
        assert_eq!(living.attribute(ARMOR), 20.0);
        assert_eq!(set_raw_peek(base, living, 0), PeekChange::Closed);
        assert_eq!(living.attribute(ARMOR), 20.0);
        assert_eq!(set_raw_peek(base, living, 100), PeekChange::Opened);
        assert_eq!(living.attribute(ARMOR), 0.0);
        assert_eq!(raw_peek(base), 100);
        assert!(!is_closed(base));
    }

    #[test]
    fn the_standing_box_follows_the_attach_face_and_the_peek_and_a_down_shulker_grows_taller() {
        let mut entity = shulker(Compound::new());
        let state = shulker_state(&entity);
        entity.base.position = Vec3::new(0.5, 64.0, 0.5);
        assert_eq!(
            entity.base.bounding_box(),
            Aabb::new(0.0, 64.0, 0.0, 1.0, 65.0, 1.0)
        );
        let open = ShulkerState {
            peek: 1.0,
            peek_old: 1.0,
        };
        let (base, _) = state_of(&mut entity);
        set_attach_face(base, &open, Face::North);
        let area = base.bounding_box();
        assert_eq!(area.min_z, 0.0);
        assert!(area.max_z > 1.99 && area.max_z < 2.01);
        assert_eq!(base.dimensions.height, 1.0);
        set_attach_face(base, &open, Face::Down);
        assert!(base.dimensions.height > 1.99);
        assert!(base.dimensions.eye_height > 0.99);
        let _ = state;
    }

    #[test]
    fn a_spawn_snaps_to_the_centre_of_the_block_and_moving_to_another_block_closes_the_lid() {
        assert_eq!(
            snapped(Vec3::new(3.9, 64.4, -2.1)),
            Vec3::new(3.5, 64.0, -2.5)
        );
        assert_eq!(
            snapped(Vec3::new(0.0, 64.5, 0.0)),
            Vec3::new(0.5, 65.0, 0.5)
        );
        let mut entity = shulker(Compound::new().with("Peek", 30i8));
        let (base, _) = state_of(&mut entity);
        base.snap_to(Vec3::new(3.9, 64.0, 3.1), 90.0, 0.0);
        assert_eq!(base.position, Vec3::new(3.5, 64.0, 3.5));
        assert_eq!(base.old_position, base.position);
        base.tick_count = 5;
        set_position(base, Vec3::new(3.6, 64.0, 3.2));
        assert_eq!(raw_peek(base), 30);
        set_position(base, Vec3::new(9.5, 64.0, 3.5));
        assert_eq!(base.position, Vec3::new(9.5, 64.0, 3.5));
        assert_eq!(raw_peek(base), 0);
        assert!(base.needs_sync);
    }

    #[test]
    fn faces_are_full_only_when_the_collision_shape_covers_the_whole_face() {
        let full =
            |kind: BlockKind, face| is_face_full(kind.default_state().collision_shape(), face);
        for face in Face::ALL {
            assert!(full(BlockKind::Stone, face), "{face:?}");
            assert!(!full(BlockKind::Air, face));
        }
        assert!(full(BlockKind::OakSlab, Face::Down));
        assert!(!full(BlockKind::OakSlab, Face::Up));
        assert!(!full(BlockKind::OakFence, Face::Up));
        assert!(!full(BlockKind::Torch, Face::Up));
    }

    #[test]
    fn a_shulker_can_stay_on_a_full_face_with_room_for_the_open_lid_only() {
        let floor = Floor(63);
        let pos = BlockPos::new(0, 64, 0);
        let free = |_: &Aabb| true;
        assert!(can_stay_at(&floor, pos, pos, Face::Down, free));
        assert!(!can_stay_at(&floor, pos, pos, Face::Up, free));
        assert!(!can_stay_at(&floor, pos, pos, Face::North, free));
        assert!(!can_stay_at(&floor, pos, pos, Face::Down, |_| false));
        struct Roofed;
        impl BlockView for Roofed {
            fn block(&self, pos: BlockPos) -> Option<BlockStateId> {
                Some(if pos.y <= 63 || pos == BlockPos::new(0, 65, 0) {
                    BlockKind::Stone.default_state()
                } else {
                    BlockStateId::AIR
                })
            }
        }
        assert!(!can_stay_at(&Roofed, pos, pos, Face::Down, free));
        assert!(can_stay_at(
            &Roofed,
            pos,
            BlockPos::new(3, 64, 0),
            Face::Down,
            free
        ));
        assert_eq!(
            find_attachable_surface(&floor, pos, pos, &free),
            Some(Face::Down)
        );
        assert_eq!(
            find_attachable_surface(
                &floor,
                BlockPos::new(0, 80, 0),
                BlockPos::new(0, 80, 0),
                &free
            ),
            None
        );
    }

    #[test]
    fn a_blocked_own_cell_refuses_unless_it_is_the_shulkers_own_moving_piston() {
        struct Piston(BlockPos);
        impl BlockView for Piston {
            fn block(&self, pos: BlockPos) -> Option<BlockStateId> {
                Some(if pos == self.0 {
                    BlockKind::MovingPiston.default_state()
                } else if pos.y < 64 {
                    BlockKind::Stone.default_state()
                } else {
                    BlockStateId::AIR
                })
            }
        }
        let pos = BlockPos::new(0, 64, 0);
        let view = Piston(pos);
        assert!(!can_stay_at(
            &view,
            BlockPos::new(5, 64, 5),
            pos,
            Face::Down,
            |_| true
        ));
        let _ = view;
        assert!(!position_blocked(&Piston(pos), pos, pos));
        assert!(position_blocked(&Piston(pos), BlockPos::new(1, 64, 1), pos));
        assert!(position_blocked(&Floor(64), pos, pos));
    }

    #[test]
    fn teleport_draws_three_offsets_per_attempt_and_stops_on_the_first_valid_cell() {
        let floor = Floor(63);
        let origin = BlockPos::new(0, 70, 0);
        let mut found = None;
        for seed in 0..50 {
            let mut random = LegacyRandom::new(seed);
            let mut mirror = LegacyRandom::new(seed);
            let hop = find_teleport(&floor, &mut random, origin, -64, |_| true, |_| true);
            let mut expected = None;
            for _ in 0..5 {
                let dx = mirror.next_int(17) - 8;
                let dy = mirror.next_int(17) - 8;
                let dz = mirror.next_int(17) - 8;
                let target = origin.offset(dx, dy, dz);
                if target.y > 63
                    && can_stay_at(&floor, origin, target, Face::Down, |_| true)
                    && find_attachable_surface(&floor, origin, target, &|_: &Aabb| true)
                        == Some(Face::Down)
                {
                    expected = Some(Hop {
                        target,
                        face: Face::Down,
                    });
                    break;
                }
            }
            assert_eq!(hop, expected, "seed {seed}");
            found = found.or(hop);
        }
        assert!(found.is_some());
    }

    #[test]
    fn teleport_refuses_cells_below_the_world_outside_the_border_or_next_to_a_blocking_tag() {
        let floor = Floor(63);
        let origin = BlockPos::new(0, 70, 0);
        let mut random = LegacyRandom::new(3);
        assert_eq!(
            find_teleport(&floor, &mut random, origin, 200, |_| true, |_| true),
            None
        );
        let mut random = LegacyRandom::new(3);
        assert_eq!(
            find_teleport(&floor, &mut random, origin, -64, |_| false, |_| true),
            None
        );
        let bedrock = |_: i32| BlockKind::Bedrock;
        struct Ground(BlockKind);
        impl BlockView for Ground {
            fn block(&self, pos: BlockPos) -> Option<BlockStateId> {
                Some(if pos.y <= 63 {
                    self.0.default_state()
                } else {
                    BlockStateId::AIR
                })
            }
        }
        let mut hits = 0;
        for seed in 0..60 {
            let mut random = LegacyRandom::new(seed);
            if find_teleport(
                &Ground(bedrock(0)),
                &mut random,
                origin,
                -64,
                |_| true,
                |_| true,
            )
            .is_some()
            {
                hits += 1;
            }
        }
        let mut stone_hits = 0;
        for seed in 0..60 {
            let mut random = LegacyRandom::new(seed);
            if find_teleport(
                &Ground(BlockKind::Stone),
                &mut random,
                origin,
                -64,
                |_| true,
                |_| true,
            )
            .is_some()
            {
                stone_hits += 1;
            }
        }
        assert!(stone_hits > 0);
        let blocked = rspigot_data::tags::tag("minecraft:block", TELEPORT_BLOCKED_TAG).unwrap();
        let bedrock_blocked = blocked
            .binary_search(&u32::from(BlockKind::Bedrock.id().0))
            .is_ok();
        assert_eq!(hits == 0, bedrock_blocked);
    }

    #[test]
    fn a_shulker_ignores_every_velocity_and_never_travels() {
        let mut entity = shulker(Compound::new());
        let (base, living) = state_of(&mut entity);
        base.position = Vec3::new(0.5, 64.0, 0.5);
        base.velocity = Vec3::new(1.0, 2.0, 3.0);
        crate::entity::living::finish_ai_step(base, living, &Floor(60), false);
        assert_eq!(base.velocity, Vec3::ZERO);
        assert_eq!(base.position, Vec3::new(0.5, 64.0, 0.5));
    }

    #[test]
    fn hurt_teleports_below_half_health_on_a_one_in_four_roll_and_breeding_odds_follow_the_crowd() {
        assert!(should_teleport_when_hurt(14.0, 30.0, 0));
        assert!(!should_teleport_when_hurt(15.0, 30.0, 0));
        assert!(!should_teleport_when_hurt(14.0, 30.0, 1));
        assert_eq!(breeding_chance(1), 0.0);
        assert_eq!(breeding_chance(3), 0.4);
        assert_eq!(breeding_chance(6), 1.0);
    }
}
