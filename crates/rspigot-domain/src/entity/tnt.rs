use rspigot_nbt::Compound;
use uuid::Uuid;

use super::base::EntityBase;
use super::data::SyncedValue;
use super::physics::{BlockView, MoveTraits, apply_inside_blocks, move_entity};
use super::save::{read_uuid, uuid_tag};
use crate::Vec3;

pub const DATA_FUSE: u8 = 8;
pub const DEFAULT_FUSE: i32 = 80;
pub const DEFAULT_POWER: f32 = 4.0;
pub const KEYS: &[&str] = &["fuse", "block_state", "explosion_power", "owner"];
const GRAVITY: f64 = 0.04;
const DRAG: f64 = 0.98;
const EXPLOSION_HEIGHT: f64 = 0.0625;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TntState {
    pub owner: Option<Uuid>,
    pub power: f32,
}

pub fn define(base: &mut EntityBase) -> TntState {
    base.data.define(DATA_FUSE, SyncedValue::Int(DEFAULT_FUSE));
    TntState {
        owner: None,
        power: DEFAULT_POWER,
    }
}

pub fn fuse(base: &EntityBase) -> i32 {
    base.data.int(DATA_FUSE)
}

pub fn set_fuse(base: &mut EntityBase, fuse: i32) {
    base.data.set(DATA_FUSE, SyncedValue::Int(fuse));
}

pub fn explosion_center(base: &EntityBase) -> Vec3 {
    Vec3::new(
        base.position.x,
        base.position.y + EXPLOSION_HEIGHT,
        base.position.z,
    )
}

pub fn ignite_velocity(angle: f64) -> Vec3 {
    Vec3::new(-angle.sin() * 0.02, 0.2, -angle.cos() * 0.02)
}

pub fn tick<V: BlockView>(base: &mut EntityBase, view: &V) -> bool {
    base.old_position = base.position;
    base.tick_count = base.tick_count.wrapping_add(1);
    let gravity = if base.is_no_gravity() { 0.0 } else { GRAVITY };
    if gravity != 0.0 {
        base.velocity = base.velocity + Vec3::new(0.0, -gravity, 0.0);
    }
    move_entity(base, view, base.velocity, &MoveTraits::plain(gravity));
    apply_inside_blocks(base, view, false);
    base.velocity = base.velocity.scale(DRAG);
    if base.on_ground {
        base.velocity = base.velocity.multiply(0.7, -0.5, 0.7);
    }
    let remaining = fuse(base) - 1;
    set_fuse(base, remaining);
    remaining <= 0
}

pub fn read(nbt: &Compound, base: &mut EntityBase, state: &mut TntState) {
    set_fuse(
        base,
        nbt.get_i64("fuse")
            .map_or(DEFAULT_FUSE, |value| i32::from(value as i16)),
    );
    state.power = nbt
        .get_f64("explosion_power")
        .map_or(DEFAULT_POWER, |value| value as f32);
    state.owner = nbt.get("owner").and_then(read_uuid);
}

pub fn write(base: &EntityBase, state: &TntState, out: &mut Compound) {
    out.insert("fuse", fuse(base) as i16);
    out.insert("explosion_power", state.power);
    if let Some(owner) = state.owner {
        out.insert("owner", uuid_tag(owner));
    }
}
