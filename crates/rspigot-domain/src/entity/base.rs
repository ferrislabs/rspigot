use rspigot_entities::{EntityDimensions, EntityKind};
use rspigot_nbt::Compound;
use uuid::Uuid;

use super::aabb::Aabb;
use super::data::{Pose, SyncedData, SyncedValue};
use super::fluid::FluidContact;
use crate::text::Component;
use crate::{ChunkPos, EntityId, Vec3};

pub const DATA_SHARED_FLAGS: u8 = 0;
pub const DATA_AIR_SUPPLY: u8 = 1;
pub const DATA_CUSTOM_NAME: u8 = 2;
pub const DATA_CUSTOM_NAME_VISIBLE: u8 = 3;
pub const DATA_SILENT: u8 = 4;
pub const DATA_NO_GRAVITY: u8 = 5;
pub const DATA_POSE: u8 = 6;
pub const DATA_TICKS_FROZEN: u8 = 7;
pub const FLAG_ON_FIRE: u8 = 1;
pub const FLAG_INVISIBLE: u8 = 1 << 5;
pub const FLAG_GLOWING: u8 = 1 << 6;
pub const MAX_AIR_SUPPLY: i32 = 300;
pub const DOLPHIN_MAX_AIR_SUPPLY: i32 = 4800;

pub fn default_max_air(kind: EntityKind) -> i32 {
    if kind == EntityKind::Dolphin {
        DOLPHIN_MAX_AIR_SUPPLY
    } else if kind == EntityKind::Axolotl {
        super::species::axolotl::MAX_AIR_SUPPLY
    } else {
        MAX_AIR_SUPPLY
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemovalReason {
    Killed,
    Discarded,
    UnloadedToChunk,
    UnloadedWithPlayer,
    ChangedDimension,
}

impl RemovalReason {
    pub const fn should_save(self) -> bool {
        matches!(self, Self::UnloadedToChunk)
    }

    pub const fn should_destroy(self) -> bool {
        matches!(self, Self::Killed | Self::Discarded)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct EntityBase {
    pub id: EntityId,
    pub uuid: Uuid,
    pub kind: EntityKind,
    pub dimensions: EntityDimensions,
    pub custom_box: Option<Aabb>,
    pub position: Vec3,
    pub old_position: Vec3,
    pub velocity: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub head_yaw: f32,
    pub on_ground: bool,
    pub horizontal_collision: bool,
    pub vertical_collision: bool,
    pub vertical_collision_below: bool,
    pub minor_horizontal_collision: bool,
    pub fall_distance: f64,
    pub remaining_fire_ticks: i32,
    pub invulnerable: bool,
    pub invulnerable_time: i32,
    pub portal_cooldown: i32,
    pub has_glowing_tag: bool,
    pub has_visual_fire: bool,
    pub tags: Vec<String>,
    pub custom_data: Option<Compound>,
    pub tick_count: i32,
    pub removal: Option<RemovalReason>,
    pub persist: bool,
    pub visible_by_default: bool,
    pub persistent_invisibility: bool,
    pub max_air_ticks: i32,
    pub stuck_speed_multiplier: Vec3,
    pub supporting_block: Option<crate::BlockPos>,
    pub on_ground_no_blocks: bool,
    pub no_physics: bool,
    pub needs_sync: bool,
    pub sync_position: bool,
    pub sync_velocity: bool,
    pub fluid: FluidContact,
    pub eye_in_water: bool,
    pub in_powder_snow: bool,
    pub was_in_powder_snow: bool,
    pub data: SyncedData,
}

impl EntityBase {
    pub fn new(id: EntityId, uuid: Uuid, kind: EntityKind) -> Self {
        let mut data = SyncedData::default();
        data.define(DATA_SHARED_FLAGS, SyncedValue::Byte(0));
        data.define(DATA_AIR_SUPPLY, SyncedValue::Int(default_max_air(kind)));
        data.define(DATA_CUSTOM_NAME, SyncedValue::OptionalComponent(None));
        data.define(DATA_CUSTOM_NAME_VISIBLE, SyncedValue::Boolean(false));
        data.define(DATA_SILENT, SyncedValue::Boolean(false));
        data.define(DATA_NO_GRAVITY, SyncedValue::Boolean(false));
        data.define(DATA_POSE, SyncedValue::Pose(Pose::Standing));
        data.define(DATA_TICKS_FROZEN, SyncedValue::Int(0));
        Self {
            id,
            uuid,
            kind,
            dimensions: kind.dimensions(),
            custom_box: None,
            position: Vec3::ZERO,
            old_position: Vec3::ZERO,
            velocity: Vec3::ZERO,
            yaw: 0.0,
            pitch: 0.0,
            head_yaw: 0.0,
            on_ground: false,
            horizontal_collision: false,
            vertical_collision: false,
            vertical_collision_below: false,
            minor_horizontal_collision: false,
            fall_distance: 0.0,
            remaining_fire_ticks: 0,
            invulnerable: false,
            invulnerable_time: 0,
            portal_cooldown: 0,
            has_glowing_tag: false,
            has_visual_fire: false,
            tags: Vec::new(),
            custom_data: None,
            tick_count: 0,
            removal: None,
            persist: true,
            visible_by_default: true,
            persistent_invisibility: false,
            max_air_ticks: default_max_air(kind),
            stuck_speed_multiplier: Vec3::ZERO,
            supporting_block: None,
            on_ground_no_blocks: false,
            no_physics: false,
            needs_sync: false,
            sync_position: false,
            sync_velocity: false,
            fluid: FluidContact::default(),
            eye_in_water: false,
            in_powder_snow: false,
            was_in_powder_snow: false,
            data,
        }
    }

    pub fn bounding_box(&self) -> Aabb {
        self.bounding_box_at(self.position)
    }

    pub fn bounding_box_at(&self, position: Vec3) -> Aabb {
        match self.custom_box {
            Some(relative) => relative.move_by(position),
            None => Aabb::of_size(position, self.dimensions.width, self.dimensions.height),
        }
    }

    pub fn eye_height(&self) -> f32 {
        self.dimensions.eye_height
    }

    pub fn eye_y(&self) -> f64 {
        self.position.y + f64::from(self.dimensions.eye_height)
    }

    pub fn chunk(&self) -> ChunkPos {
        self.position.chunk()
    }

    pub fn air_supply(&self) -> i32 {
        self.data.int(DATA_AIR_SUPPLY)
    }

    pub fn set_air_supply(&mut self, air: i32) {
        self.data.set(DATA_AIR_SUPPLY, SyncedValue::Int(air));
    }

    pub fn custom_name(&self) -> Option<&Component> {
        match self.data.get(DATA_CUSTOM_NAME) {
            Some(SyncedValue::OptionalComponent(name)) => name.as_ref(),
            _ => None,
        }
    }

    pub fn set_custom_name(&mut self, name: Option<Component>) {
        self.data
            .set(DATA_CUSTOM_NAME, SyncedValue::OptionalComponent(name));
    }

    pub fn custom_name_visible(&self) -> bool {
        self.data.boolean(DATA_CUSTOM_NAME_VISIBLE)
    }

    pub fn set_custom_name_visible(&mut self, visible: bool) {
        self.data
            .set(DATA_CUSTOM_NAME_VISIBLE, SyncedValue::Boolean(visible));
    }

    pub fn is_silent(&self) -> bool {
        self.data.boolean(DATA_SILENT)
    }

    pub fn set_silent(&mut self, silent: bool) {
        self.data.set(DATA_SILENT, SyncedValue::Boolean(silent));
    }

    pub fn is_no_gravity(&self) -> bool {
        self.data.boolean(DATA_NO_GRAVITY)
    }

    pub fn set_no_gravity(&mut self, no_gravity: bool) {
        self.data
            .set(DATA_NO_GRAVITY, SyncedValue::Boolean(no_gravity));
    }

    pub fn pose(&self) -> Pose {
        match self.data.get(DATA_POSE) {
            Some(SyncedValue::Pose(pose)) => *pose,
            _ => Pose::Standing,
        }
    }

    pub fn set_pose(&mut self, pose: Pose) {
        self.data.set(DATA_POSE, SyncedValue::Pose(pose));
    }

    pub fn ticks_frozen(&self) -> i32 {
        self.data.int(DATA_TICKS_FROZEN)
    }

    pub fn set_ticks_frozen(&mut self, ticks: i32) {
        self.data.set(DATA_TICKS_FROZEN, SyncedValue::Int(ticks));
    }

    pub fn set_glowing_tag(&mut self, glowing: bool) {
        self.has_glowing_tag = glowing;
        self.data.set_flag(DATA_SHARED_FLAGS, FLAG_GLOWING, glowing);
    }

    pub fn set_shared_flag(&mut self, mask: u8, value: bool) {
        self.data.set_flag(DATA_SHARED_FLAGS, mask, value);
    }

    pub fn is_removed(&self) -> bool {
        self.removal.is_some()
    }

    pub fn snap_to(&mut self, position: Vec3, yaw: f32, pitch: f32) {
        let position = if self.kind == EntityKind::Shulker {
            super::species::shulker::snapped(position)
        } else {
            position
        };
        self.position = position;
        self.set_yaw(yaw);
        self.set_pitch(pitch);
        self.old_position = position;
    }

    pub fn set_yaw(&mut self, yaw: f32) {
        if yaw.is_finite() {
            self.yaw = yaw;
        }
    }

    pub fn set_pitch(&mut self, pitch: f32) {
        if pitch.is_finite() {
            self.pitch = (pitch % 360.0).clamp(-90.0, 90.0);
        }
    }

    pub fn set_rotation(&mut self, yaw: f32, pitch: f32) {
        let yaw = if yaw.is_finite() { yaw } else { 0.0 };
        let pitch = if pitch.is_finite() { pitch } else { 0.0 };
        self.set_yaw(yaw % 360.0);
        self.set_pitch(pitch % 360.0);
    }
}
