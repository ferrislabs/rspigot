mod aabb;
pub mod ageable;
pub mod arrow;
mod base;
pub mod bullet;
mod data;
pub mod falling;
pub mod fire;
pub mod fluid;
pub mod hurting;
pub mod item;
pub mod living;
pub mod neutral;
pub mod orb;
pub mod physics;
#[cfg(test)]
mod physics_tests;
pub mod raycast;
pub mod reward;
pub mod save;
pub mod spawn;
pub mod species;
mod store;
pub mod tamable;
pub mod thrown;
pub mod tracker;
mod tracking_range;
pub mod variants;

use rspigot_nbt::Compound;

pub use aabb::{Aabb, Axis};
pub use base::{
    DATA_AIR_SUPPLY, DATA_CUSTOM_NAME, DATA_CUSTOM_NAME_VISIBLE, DATA_NO_GRAVITY, DATA_POSE,
    DATA_SHARED_FLAGS, DATA_SILENT, DATA_TICKS_FROZEN, EntityBase, FLAG_GLOWING, FLAG_INVISIBLE,
    FLAG_ON_FIRE, MAX_AIR_SUPPLY, RemovalReason,
};
pub use data::{Pose, StateKind, SyncedData, SyncedValue, VariantKind};
pub use falling::FallingState;
pub use item::ItemState;
pub use living::{LivingState, MobState};
pub use orb::OrbState;
pub use physics::BlockView;
pub use store::{DuplicateUuid, Entities};
pub use tracker::EntityTracker;
pub use tracking_range::{ActivationType, TrackingRanges, activation_type, tracking_range};

#[derive(Debug, Clone, PartialEq)]
pub enum EntityState {
    Item(ItemState),
    Arrow(Box<arrow::ArrowState>),
    Thrown(Box<thrown::ThrownState>),
    Hurting(Box<hurting::HurtingState>),
    Bullet(Box<bullet::BulletState>),
    Falling(FallingState),
    Living(Box<LivingState>),
    Orb(OrbState),
    Inert(Compound),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Entity {
    pub base: EntityBase,
    pub state: EntityState,
    pub extra: Compound,
}

impl Entity {
    pub fn new(base: EntityBase, state: EntityState) -> Self {
        Self {
            base,
            state,
            extra: Compound::new(),
        }
    }

    pub fn is_simulated(&self) -> bool {
        !matches!(self.state, EntityState::Inert(_))
    }

    pub fn living(&self) -> Option<&LivingState> {
        match &self.state {
            EntityState::Living(living) => Some(living),
            _ => None,
        }
    }

    pub fn living_mut(&mut self) -> Option<&mut LivingState> {
        match &mut self.state {
            EntityState::Living(living) => Some(living),
            _ => None,
        }
    }

    pub fn is_baby(&self) -> bool {
        let Some(mob) = self.living().and_then(|living| living.mob.as_ref()) else {
            return false;
        };
        if let Some(ageable) = &mob.ageable {
            return ageable::is_baby(&self.base, ageable);
        }
        match mob.species {
            species::Species::Zombie(_) => species::zombie_is_baby(&self.base),
            species::Species::Piglin(_) => species::piglin::is_baby(&self.base),
            species::Species::Hoglin(_) => species::hoglin::is_zoglin_baby(&self.base),
            _ => false,
        }
    }

    pub fn add_entity_data(&self) -> i32 {
        match &self.state {
            EntityState::Falling(falling) => i32::from(falling.state.0),
            _ => 0,
        }
    }
}
