use rspigot_entities::{EntityDimensions, EntityKind};
use rspigot_nbt::Compound;

use crate::entity::base::EntityBase;
use crate::entity::data::{Pose, SyncedValue};
use crate::entity::living::LivingState;
use crate::entity::species::Species;
use crate::spawn::SpawnReason;

pub const DATA_CLIENT_ANGER_LEVEL: u8 = 16;
pub const KEYS: &[&str] = &[];
pub const ATTACK_EVENT: i8 = 4;
pub const TENDRIL_EVENT: i8 = 61;
pub const SONIC_BOOM_EVENT: i8 = 62;
pub const BURROWED_HEIGHT: f32 = 1.0;
const EYE_HEIGHT_FACTOR: f32 = 0.85;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WardenState {
    pub pending_spawn: Option<bool>,
}

pub fn define(base: &mut EntityBase) -> WardenState {
    base.data
        .define(DATA_CLIENT_ANGER_LEVEL, SyncedValue::Int(0));
    WardenState::default()
}

pub fn client_anger_level(base: &EntityBase) -> i32 {
    base.data.int(DATA_CLIENT_ANGER_LEVEL)
}

pub fn set_client_anger_level(base: &mut EntityBase, level: i32) {
    base.data
        .set(DATA_CLIENT_ANGER_LEVEL, SyncedValue::Int(level));
}

pub const fn is_digging_or_emerging(pose: Pose) -> bool {
    matches!(pose, Pose::Digging | Pose::Emerging)
}

pub fn dimensions_in_pose(pose: Pose) -> EntityDimensions {
    let standing = EntityKind::Warden.dimensions();
    if is_digging_or_emerging(pose) {
        EntityDimensions {
            width: standing.width,
            height: BURROWED_HEIGHT,
            eye_height: BURROWED_HEIGHT * EYE_HEIGHT_FACTOR,
        }
    } else {
        standing
    }
}

pub fn finalize(base: &mut EntityBase, living: &mut LivingState, reason: SpawnReason) {
    let triggered = reason == SpawnReason::Triggered;
    if triggered {
        base.set_pose(Pose::Emerging);
        base.dimensions = dimensions_in_pose(Pose::Emerging);
    }
    if let Some(Species::Warden(state)) = living.mob.as_mut().map(|mob| &mut mob.species) {
        state.pending_spawn = Some(triggered);
    }
}

pub fn read(_nbt: &Compound, _base: &mut EntityBase, _state: &mut WardenState) {}

pub fn write(_base: &EntityBase, _state: &WardenState, _out: &mut Compound) {}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;
    use crate::EntityId;

    fn base() -> EntityBase {
        EntityBase::new(EntityId(1), Uuid::from_u128(1), EntityKind::Warden)
    }

    #[test]
    fn the_client_anger_level_is_the_first_synced_slot_of_the_warden() {
        let mut base = base();
        define(&mut base);
        assert_eq!(DATA_CLIENT_ANGER_LEVEL, 16);
        assert_eq!(client_anger_level(&base), 0);
        set_client_anger_level(&mut base, 85);
        assert_eq!(client_anger_level(&base), 85);
    }

    #[test]
    fn a_triggered_spawn_starts_emerging_and_other_spawns_only_arm_the_dig_cooldown() {
        for (reason, pose, pending) in [
            (SpawnReason::Triggered, Pose::Emerging, Some(true)),
            (SpawnReason::Natural, Pose::Standing, Some(false)),
        ] {
            let mut base = base();
            let mut living = LivingState::new(&mut base, 0.0);
            let species = Species::Warden(define(&mut base));
            living.mob.as_mut().expect("mob").species = species;
            finalize(&mut base, &mut living, reason);
            assert_eq!(base.pose(), pose);
            let state = living.mob.as_ref().map(|mob| mob.species);
            assert_eq!(
                state,
                Some(Species::Warden(WardenState {
                    pending_spawn: pending
                }))
            );
        }
    }

    #[test]
    fn a_burrowing_warden_is_one_block_tall() {
        let standing = dimensions_in_pose(Pose::Standing);
        assert_eq!(standing, EntityKind::Warden.dimensions());
        for pose in [Pose::Digging, Pose::Emerging] {
            let burrowed = dimensions_in_pose(pose);
            assert_eq!(burrowed.width, standing.width);
            assert_eq!(burrowed.height, 1.0);
            assert_eq!(burrowed.eye_height, 0.85);
        }
        assert_eq!(dimensions_in_pose(Pose::Roaring), standing);
        assert_eq!(dimensions_in_pose(Pose::Sniffing), standing);
    }
}
