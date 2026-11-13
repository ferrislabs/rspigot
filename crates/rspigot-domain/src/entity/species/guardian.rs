use rspigot_entities::EntityKind;

use crate::entity::base::EntityBase;
use crate::entity::data::SyncedValue;

pub const DATA_MOVING: u8 = 16;
pub const DATA_ATTACK_TARGET: u8 = 17;
pub const KEYS: &[&str] = &[];
pub const ATTACK_DURATION: i32 = 80;
pub const ELDER_ATTACK_DURATION: i32 = 60;
pub const ATTACK_EVENT: i8 = 21;
pub const AIR_SUPPLY: i32 = 300;
pub const FLOP_SPREAD: f32 = 0.4;
pub const FLOP_VERTICAL: f64 = 0.5;
pub const MAX_HEAD_X_ROT: i32 = 180;
pub const THORNS_DAMAGE: f32 = 2.0;
pub const HARD_BONUS: f32 = 2.0;
pub const ELDER_BONUS: f32 = 2.0;
pub const BASE_BEAM_DAMAGE: f32 = 1.0;
pub const MINIMUM_ATTACK_DISTANCE_SQR: f64 = 9.0;
pub const STROLL_INTERVAL: i32 = 80;
pub const ELDER_STROLL_INTERVAL: i32 = 400;
pub const AMBIENT_INTERVAL: i32 = 160;
pub const ELDER_HOME_RADIUS: i32 = 16;
pub const ELDER_EFFECT_PERIOD: i32 = 1200;
pub const ELDER_EFFECT_RADIUS: f64 = 50.0;
pub const ELDER_EFFECT_DURATION: u32 = 6000;
pub const ELDER_EFFECT_AMPLIFIER: u8 = 2;
pub const ELDER_EFFECT_REAPPLY: i32 = 1200;
pub const ELDER_EFFECT: &str = "minecraft:mining_fatigue";
pub const THORNS_AVOIDING_TAG: &str = "minecraft:avoids_guardian_thorns";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct GuardianState;

pub fn define(base: &mut EntityBase) -> GuardianState {
    base.data.define(DATA_MOVING, SyncedValue::Boolean(false));
    base.data.define(DATA_ATTACK_TARGET, SyncedValue::Int(0));
    GuardianState
}

pub fn is_guardian(kind: EntityKind) -> bool {
    kind.extends("Guardian")
}

pub fn is_elder(kind: EntityKind) -> bool {
    kind == EntityKind::ElderGuardian
}

pub fn attack_duration(kind: EntityKind) -> i32 {
    if is_elder(kind) {
        ELDER_ATTACK_DURATION
    } else {
        ATTACK_DURATION
    }
}

pub fn ambient_sound(kind: EntityKind, in_water: bool) -> &'static str {
    match (is_elder(kind), in_water) {
        (true, true) => "minecraft:entity.elder_guardian.ambient",
        (true, false) => "minecraft:entity.elder_guardian.ambient_land",
        (false, true) => "minecraft:entity.guardian.ambient",
        (false, false) => "minecraft:entity.guardian.ambient_land",
    }
}

pub fn is_moving(base: &EntityBase) -> bool {
    base.data.boolean(DATA_MOVING)
}

pub fn set_moving(base: &mut EntityBase, moving: bool) {
    base.data.set(DATA_MOVING, SyncedValue::Boolean(moving));
}

pub fn active_attack_target(base: &EntityBase) -> i32 {
    base.data.int(DATA_ATTACK_TARGET)
}

pub fn set_active_attack_target(base: &mut EntityBase, id: i32) {
    base.data.set(DATA_ATTACK_TARGET, SyncedValue::Int(id));
}

pub fn flop(base: &mut EntityBase, random: &mut impl crate::random::RandomSource) {
    let x = f64::from((random.next_float() * 2.0 - 1.0) * FLOP_SPREAD);
    let z = f64::from((random.next_float() * 2.0 - 1.0) * FLOP_SPREAD);
    base.velocity = base.velocity + crate::Vec3::new(x, FLOP_VERTICAL, z);
    base.set_yaw(random.next_float() * 360.0);
    base.on_ground = false;
    base.needs_sync = true;
}

pub fn beam_damage(hard: bool, elder: bool) -> f32 {
    let mut damage = BASE_BEAM_DAMAGE;
    if hard {
        damage += HARD_BONUS;
    }
    if elder {
        damage += ELDER_BONUS;
    }
    damage
}

pub fn target_selector(target_is_player_squid_or_axolotl: bool, distance_sqr: f64) -> bool {
    target_is_player_squid_or_axolotl && distance_sqr > MINIMUM_ATTACK_DISTANCE_SQR
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;
    use crate::EntityId;
    use crate::entity::living::LivingState;
    use crate::entity::species::Species;
    use crate::random::RandomSource;

    fn guardian(kind: EntityKind) -> (EntityBase, LivingState) {
        let mut base = EntityBase::new(EntityId(1), Uuid::from_u128(1), kind);
        let living = LivingState::new(&mut base, 0.0);
        (base, living)
    }

    #[test]
    fn a_guardian_syncs_its_moving_flag_and_beam_target_after_the_mob_flags() {
        let (mut base, living) = guardian(EntityKind::Guardian);
        assert!(matches!(
            living.mob.as_ref().map(|mob| mob.species),
            Some(Species::Guardian(_))
        ));
        assert_eq!(
            base.data.get(DATA_MOVING),
            Some(&SyncedValue::Boolean(false))
        );
        assert_eq!(
            base.data.get(DATA_ATTACK_TARGET),
            Some(&SyncedValue::Int(0))
        );
        set_moving(&mut base, true);
        set_active_attack_target(&mut base, 42);
        assert!(is_moving(&base));
        assert_eq!(active_attack_target(&base), 42);
    }

    #[test]
    fn the_elder_guardian_is_a_guardian_with_a_sixty_tick_beam_and_is_persistent() {
        let (base, living) = guardian(EntityKind::ElderGuardian);
        assert!(is_guardian(base.kind) && is_elder(base.kind));
        assert_eq!(attack_duration(EntityKind::Guardian), 80);
        assert_eq!(attack_duration(EntityKind::ElderGuardian), 60);
        assert!(living.mob.as_ref().unwrap().persistence_required);
        let (_, plain) = guardian(EntityKind::Guardian);
        assert!(!plain.mob.as_ref().unwrap().persistence_required);
    }

    #[test]
    fn the_beam_hurts_one_plus_two_on_hard_plus_two_for_the_elder() {
        assert_eq!(beam_damage(false, false), 1.0);
        assert_eq!(beam_damage(true, false), 3.0);
        assert_eq!(beam_damage(false, true), 3.0);
        assert_eq!(beam_damage(true, true), 5.0);
    }

    #[test]
    fn a_guardian_out_of_water_flops_up_and_spins() {
        let (mut base, _) = guardian(EntityKind::Guardian);
        base.on_ground = true;
        let mut random = crate::random::LegacyRandom::new(11);
        let mut mirror = crate::random::LegacyRandom::new(11);
        flop(&mut base, &mut random);
        let x = f64::from((mirror.next_float() * 2.0 - 1.0) * 0.4);
        let z = f64::from((mirror.next_float() * 2.0 - 1.0) * 0.4);
        assert_eq!(base.velocity, crate::Vec3::new(x, 0.5, z));
        assert_eq!(base.yaw, mirror.next_float() * 360.0);
        assert!(!base.on_ground);
    }

    #[test]
    fn the_attack_selector_only_takes_far_players_squids_and_axolotls() {
        assert!(target_selector(true, 9.5));
        assert!(!target_selector(true, 9.0));
        assert!(!target_selector(false, 100.0));
    }
}
