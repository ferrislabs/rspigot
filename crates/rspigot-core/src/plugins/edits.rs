use rspigot_domain::{EntityId, Vec3};
use uuid::Uuid;

use crate::plugins::ItemSpec;

#[derive(Debug, Clone, PartialEq)]
pub enum EntityEdit {
    SetCustomName(String),
    ClearCustomName,
    SetCustomNameVisible(bool),
    SetGlowing(bool),
    SetInvisible(bool),
    SetSilent(bool),
    SetGravity(bool),
    SetInvulnerable(bool),
    SetPersistent(bool),
    SetFireTicks(i32),
    SetFreezeTicks(i32),
    SetAir(i32),
    SetFallDistance(f32),
    SetRotation {
        yaw: f32,
        pitch: f32,
    },
    LookAt(Vec3),
    SetTicksLived(i32),
    AddTag(String),
    RemoveTag(String),
    Mount(EntityId),
    Dismount,
    EjectPassengers,
    SetHealth(f32),
    SetAbsorption(f32),
    SetAi(bool),
    SetCanPickUpLoot(bool),
    SetLeftHanded(bool),
    SetEquipment {
        slot: String,
        item: Option<ItemSpec>,
    },
    SetDropChance {
        slot: String,
        chance: f32,
    },
    SetLeashHolder(EntityId),
    ClearLeash,
    SetOwner(Uuid),
    ClearOwner,
    SetTamed(bool),
    SetSitting(bool),
    SetTarget(EntityId),
    ClearTarget,
    SetAge(i32),
    SetAgeLocked(bool),
    SetBreeding(bool),
}

impl EntityEdit {
    pub const fn name(&self) -> &'static str {
        match self {
            Self::SetCustomName(_) => "entity.set_custom_name",
            Self::ClearCustomName => "entity.clear_custom_name",
            Self::SetCustomNameVisible(_) => "entity.set_custom_name_visible",
            Self::SetGlowing(_) => "entity.set_glowing",
            Self::SetInvisible(_) => "entity.set_invisible",
            Self::SetSilent(_) => "entity.set_silent",
            Self::SetGravity(_) => "entity.set_gravity",
            Self::SetInvulnerable(_) => "entity.set_invulnerable",
            Self::SetPersistent(_) => "entity.set_persistent",
            Self::SetFireTicks(_) => "entity.set_fire_ticks",
            Self::SetFreezeTicks(_) => "entity.set_freeze_ticks",
            Self::SetAir(_) => "entity.set_air",
            Self::SetFallDistance(_) => "entity.set_fall_distance",
            Self::SetRotation { .. } => "entity.set_rotation",
            Self::LookAt(_) => "entity.look_at",
            Self::SetTicksLived(_) => "entity.set_ticks_lived",
            Self::AddTag(_) => "entity.add_tag",
            Self::RemoveTag(_) => "entity.remove_tag",
            Self::Mount(_) => "entity.mount",
            Self::Dismount => "entity.dismount",
            Self::EjectPassengers => "entity.eject_passengers",
            Self::SetHealth(_) => "entity.set_health",
            Self::SetAbsorption(_) => "entity.set_absorption",
            Self::SetAi(_) => "entity.set_ai",
            Self::SetCanPickUpLoot(_) => "entity.set_can_pick_up_loot",
            Self::SetLeftHanded(_) => "entity.set_left_handed",
            Self::SetEquipment { .. } => "entity.set_equipment",
            Self::SetDropChance { .. } => "entity.set_drop_chance",
            Self::SetLeashHolder(_) => "entity.set_leash_holder",
            Self::ClearLeash => "entity.clear_leash",
            Self::SetOwner(_) => "entity.set_owner",
            Self::ClearOwner => "entity.clear_owner",
            Self::SetTamed(_) => "entity.set_tamed",
            Self::SetSitting(_) => "entity.set_sitting",
            Self::SetTarget(_) => "entity.set_target",
            Self::ClearTarget => "entity.clear_target",
            Self::SetAge(_) => "entity.set_age",
            Self::SetAgeLocked(_) => "entity.set_age_locked",
            Self::SetBreeding(_) => "entity.set_breeding",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct EquipmentView {
    pub slot: &'static str,
    pub item: Option<ItemSpec>,
    pub drop_chance: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LivingDetails {
    pub max_health: f32,
    pub absorption: f32,
    pub ai: bool,
    pub can_pick_up_loot: bool,
    pub left_handed: bool,
    pub equipment: Vec<EquipmentView>,
    pub leash_holder: Option<EntityId>,
    pub owner: Option<Uuid>,
    pub tamed: bool,
    pub sitting: bool,
    pub target: Option<EntityId>,
    pub age: i32,
    pub age_locked: bool,
    pub baby: bool,
    pub in_love: bool,
}
