use std::num::NonZeroU8;

use rspigot_entities::EntityKind;
use rspigot_items::ItemKind;
use uuid::Uuid;

use super::base::EntityBase;
use super::data::SyncedValue;
use crate::item::ItemStack;
use crate::{Vec3, mth};

pub const DATA_ITEM_STACK: u8 = 8;
pub const DATA_DANGEROUS: u8 = 8;
pub const INITIAL_ACCELERATION_POWER: f64 = 0.1;
pub const DEFLECTION_SCALE: f64 = 0.5;
pub const AIR_INERTIA: f32 = 0.95;
pub const DANGEROUS_INERTIA: f32 = 0.73;
pub const LIQUID_INERTIA: f32 = 0.8;
pub const DEFAULT_EXPLOSION_POWER: i32 = 1;
pub const ROTATION_SPEED: f32 = 0.2;
pub const SMALL_FIREBALL_DAMAGE: f32 = 5.0;
pub const SMALL_FIREBALL_FIRE_SECONDS: f32 = 5.0;
pub const LARGE_FIREBALL_DAMAGE: f32 = 6.0;
pub const SKULL_DAMAGE: f32 = 8.0;
pub const SKULL_MAGIC_DAMAGE: f32 = 5.0;
pub const SKULL_HEAL: f32 = 5.0;
pub const SKULL_EXPLOSION_RADIUS: f32 = 1.0;
pub const SKULL_RESISTANCE_CAP: f32 = 0.8;
pub const SKULL_WITHER_SECONDS_NORMAL: i32 = 10;
pub const SKULL_WITHER_SECONDS_HARD: i32 = 40;
pub const BURN_SECONDS: f32 = 1.0;
const ROTATION_DEGREES: f64 = 57.295_776_367_187_5;

#[derive(Debug, Clone, PartialEq)]
pub struct HurtingState {
    pub owner: Option<Uuid>,
    pub left_owner: bool,
    pub left_owner_checked: bool,
    pub has_been_shot: bool,
    pub acceleration_power: f64,
    pub explosion_power: i32,
    pub incendiary: bool,
}

pub fn is_hurting_projectile(kind: EntityKind) -> bool {
    matches!(
        kind,
        EntityKind::SmallFireball | EntityKind::Fireball | EntityKind::WitherSkull
    )
}

pub fn is_fireball(kind: EntityKind) -> bool {
    matches!(kind, EntityKind::SmallFireball | EntityKind::Fireball)
}

pub fn default_item() -> ItemStack {
    ItemStack::new(ItemKind::FireCharge.id(), NonZeroU8::MIN)
}

impl HurtingState {
    pub fn new(base: &mut EntityBase) -> Self {
        if is_fireball(base.kind) {
            base.data
                .define(DATA_ITEM_STACK, SyncedValue::Item(Some(default_item())));
        } else if base.kind == EntityKind::WitherSkull {
            base.data
                .define(DATA_DANGEROUS, SyncedValue::Boolean(false));
        }
        Self {
            owner: None,
            left_owner: false,
            left_owner_checked: false,
            has_been_shot: false,
            acceleration_power: INITIAL_ACCELERATION_POWER,
            explosion_power: DEFAULT_EXPLOSION_POWER,
            incendiary: true,
        }
    }
}

pub fn item(base: &EntityBase) -> Option<ItemStack> {
    match base.data.get(DATA_ITEM_STACK) {
        Some(SyncedValue::Item(stack)) => stack.clone(),
        _ => None,
    }
}

pub fn set_item(base: &mut EntityBase, stack: &ItemStack) {
    let single = ItemStack {
        count: NonZeroU8::MIN,
        ..stack.clone()
    };
    base.data
        .set(DATA_ITEM_STACK, SyncedValue::Item(Some(single)));
}

pub fn is_dangerous(base: &EntityBase) -> bool {
    base.kind == EntityKind::WitherSkull && base.data.boolean(DATA_DANGEROUS)
}

pub fn set_dangerous(base: &mut EntityBase, dangerous: bool) {
    base.data
        .set(DATA_DANGEROUS, SyncedValue::Boolean(dangerous));
}

pub fn should_burn(base: &EntityBase) -> bool {
    base.kind != EntityKind::WitherSkull
}

pub fn inertia(base: &EntityBase) -> f32 {
    if base.fluid.in_water {
        LIQUID_INERTIA
    } else if is_dangerous(base) {
        DANGEROUS_INERTIA
    } else {
        AIR_INERTIA
    }
}

pub fn assign_directional_movement(base: &mut EntityBase, direction: Vec3, power: f64) {
    base.velocity = direction.normalize().scale(power);
    base.needs_sync = true;
}

pub fn apply_inertia(base: &mut EntityBase, state: &HurtingState) {
    let movement = base.velocity;
    let accelerated = movement + movement.normalize().scale(state.acceleration_power);
    base.velocity = accelerated.scale(f64::from(inertia(base)));
}

fn wrap_towards(old: f32, current: f32) -> f32 {
    let mut old = old;
    while current - old < -180.0 {
        old -= 360.0;
    }
    while current - old >= 180.0 {
        old += 360.0;
    }
    old
}

pub fn rotate_towards_movement(base: &mut EntityBase, speed: f32) {
    let movement = base.velocity;
    if movement.length_sqr() == 0.0 {
        return;
    }
    let horizontal = (movement.x * movement.x + movement.z * movement.z).sqrt();
    let yaw = (mth::atan2(movement.z, movement.x) * ROTATION_DEGREES) as f32 + 90.0;
    let pitch = (mth::atan2(horizontal, movement.y) * ROTATION_DEGREES) as f32 - 90.0;
    let old_pitch = wrap_towards(base.pitch, pitch);
    let old_yaw = wrap_towards(base.yaw, yaw);
    base.set_pitch(old_pitch + speed * (pitch - old_pitch));
    base.set_yaw(old_yaw + speed * (yaw - old_yaw));
}

pub fn deflect_aim(base: &mut EntityBase, look: Vec3, scale: f64) {
    base.velocity = look.scale(scale);
    base.needs_sync = true;
}

pub fn on_deflection(state: &mut HurtingState, by_attack: bool) {
    if by_attack {
        state.acceleration_power = INITIAL_ACCELERATION_POWER;
    } else {
        state.acceleration_power *= DEFLECTION_SCALE;
    }
}

pub fn skull_wither_seconds(difficulty: u8) -> i32 {
    match difficulty {
        2 => SKULL_WITHER_SECONDS_NORMAL,
        3 => SKULL_WITHER_SECONDS_HARD,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EntityId;
    use crate::entity::save::{is_simulated, load_entity, save_entity};
    use crate::entity::{Entity, EntityState};
    use rspigot_nbt::Compound;

    fn projectile(kind: EntityKind) -> (EntityBase, HurtingState) {
        let mut base = EntityBase::new(EntityId(1), Uuid::from_u128(1), kind);
        let state = HurtingState::new(&mut base);
        (base, state)
    }

    fn load(nbt: &Compound) -> Entity {
        load_entity(nbt, EntityId(1), Uuid::from_u128(1)).unwrap()
    }

    #[test]
    fn a_fireball_carries_a_fire_charge_and_a_skull_a_danger_flag_in_the_first_data_slot() {
        let (small, _) = projectile(EntityKind::SmallFireball);
        let (large, _) = projectile(EntityKind::Fireball);
        let (skull, _) = projectile(EntityKind::WitherSkull);
        assert_eq!(DATA_ITEM_STACK, 8);
        for fireball in [&small, &large] {
            assert_eq!(
                item(fireball).map(|stack| stack.item),
                Some(ItemKind::FireCharge.id())
            );
        }
        assert!(!is_dangerous(&skull));
        assert_eq!(
            skull.data.get(DATA_DANGEROUS),
            Some(&SyncedValue::Boolean(false))
        );
    }

    #[test]
    fn the_launch_velocity_is_the_unit_direction_scaled_by_the_acceleration_power() {
        let (mut base, state) = projectile(EntityKind::SmallFireball);
        assign_directional_movement(
            &mut base,
            Vec3::new(0.0, 0.0, 8.0),
            state.acceleration_power,
        );
        assert_eq!(base.velocity, Vec3::new(0.0, 0.0, 0.1));
        assert!(base.needs_sync);
        assign_directional_movement(&mut base, Vec3::new(3.0, 4.0, 0.0), 2.0);
        assert_eq!(base.velocity, Vec3::new(1.2, 1.6, 0.0));
    }

    #[test]
    fn the_flight_adds_the_acceleration_along_the_motion_then_applies_the_air_inertia() {
        let (mut base, state) = projectile(EntityKind::Fireball);
        base.velocity = Vec3::new(0.0, 0.0, 0.1);
        let mut expected = base.velocity;
        for tick in 0..30 {
            apply_inertia(&mut base, &state);
            expected = (expected + expected.normalize().scale(0.1)).scale(f64::from(0.95f32));
            assert_eq!(base.velocity, expected, "tick {tick}");
        }
        assert!(base.velocity.z > 0.1 && base.velocity.z < 2.0);
        assert_eq!(base.velocity.x, 0.0);
        assert_eq!(base.velocity.y, 0.0);
    }

    #[test]
    fn water_slows_a_fireball_to_four_fifths_and_a_dangerous_skull_to_seventy_three_percent() {
        let (mut fireball, state) = projectile(EntityKind::Fireball);
        fireball.velocity = Vec3::new(1.0, 0.0, 0.0);
        fireball.fluid.in_water = true;
        apply_inertia(&mut fireball, &state);
        assert_eq!(fireball.velocity.x, 1.1 * f64::from(0.8f32));
        let (mut skull, state) = projectile(EntityKind::WitherSkull);
        skull.velocity = Vec3::new(1.0, 0.0, 0.0);
        apply_inertia(&mut skull, &state);
        assert_eq!(skull.velocity.x, 1.1 * f64::from(0.95f32));
        set_dangerous(&mut skull, true);
        skull.velocity = Vec3::new(1.0, 0.0, 0.0);
        apply_inertia(&mut skull, &state);
        assert_eq!(skull.velocity.x, 1.1 * f64::from(0.73f32));
        skull.fluid.in_water = true;
        skull.velocity = Vec3::new(1.0, 0.0, 0.0);
        apply_inertia(&mut skull, &state);
        assert_eq!(skull.velocity.x, 1.1 * f64::from(0.8f32));
    }

    #[test]
    fn a_standing_projectile_keeps_its_rotation() {
        let (mut base, _) = projectile(EntityKind::Fireball);
        base.set_rotation(12.0, 5.0);
        rotate_towards_movement(&mut base, ROTATION_SPEED);
        assert_eq!((base.yaw, base.pitch), (12.0, 5.0));
    }

    #[test]
    fn the_rotation_chases_the_motion_a_fifth_of_the_way_each_tick() {
        let (mut base, _) = projectile(EntityKind::Fireball);
        base.set_rotation(90.0, 0.0);
        base.velocity = Vec3::new(0.0, 0.0, 1.0);
        rotate_towards_movement(&mut base, ROTATION_SPEED);
        assert!((base.yaw - 108.0).abs() < 1.0e-3);
        assert!(base.pitch.abs() < 1.0e-4);
        base.set_rotation(0.0, 0.0);
        base.velocity = Vec3::new(0.0, -1.0, 1.0);
        rotate_towards_movement(&mut base, ROTATION_SPEED);
        assert!((base.pitch - 9.0).abs() < 1.0e-3, "{}", base.pitch);
        base.velocity = Vec3::new(0.0, 1.0, 1.0);
        base.set_rotation(0.0, 0.0);
        rotate_towards_movement(&mut base, ROTATION_SPEED);
        assert!((base.pitch + 9.0).abs() < 1.0e-3, "{}", base.pitch);
    }

    #[test]
    fn the_rotation_unwraps_the_previous_angle_before_the_blend() {
        let (mut base, _) = projectile(EntityKind::Fireball);
        base.set_rotation(-170.0, 0.0);
        base.velocity = Vec3::new(-1.0, 0.0, 0.0);
        rotate_towards_movement(&mut base, ROTATION_SPEED);
        assert!((base.yaw - 206.0).abs() < 1.0e-3, "{}", base.yaw);
        base.set_rotation(170.0, 0.0);
        base.velocity = Vec3::new(0.0, 0.0, 1.0);
        rotate_towards_movement(&mut base, ROTATION_SPEED);
        assert!((base.yaw - 172.0).abs() < 1.0e-3, "{}", base.yaw);
    }

    #[test]
    fn an_aimed_deflection_replaces_the_motion_with_the_look_and_resets_the_acceleration() {
        let (mut base, mut state) = projectile(EntityKind::Fireball);
        state.acceleration_power = 0.4;
        deflect_aim(&mut base, Vec3::new(0.0, 0.6, 0.8), 1.0);
        assert_eq!(base.velocity, Vec3::new(0.0, 0.6, 0.8));
        on_deflection(&mut state, true);
        assert_eq!(state.acceleration_power, 0.1);
        state.acceleration_power = 0.4;
        on_deflection(&mut state, false);
        assert_eq!(state.acceleration_power, 0.2);
    }

    #[test]
    fn the_wither_effect_of_a_skull_lasts_ten_seconds_on_normal_and_forty_on_hard() {
        assert_eq!(skull_wither_seconds(0), 0);
        assert_eq!(skull_wither_seconds(1), 0);
        assert_eq!(skull_wither_seconds(2), 10);
        assert_eq!(skull_wither_seconds(3), 40);
    }

    #[test]
    fn the_three_projectiles_are_simulated_and_the_dragon_fireball_is_not() {
        for kind in [
            EntityKind::SmallFireball,
            EntityKind::Fireball,
            EntityKind::WitherSkull,
        ] {
            assert!(is_simulated(kind), "{kind:?}");
            assert!(is_hurting_projectile(kind), "{kind:?}");
        }
        assert!(!is_hurting_projectile(EntityKind::DragonFireball));
        assert!(!is_simulated(EntityKind::DragonFireball));
    }

    #[test]
    fn a_large_fireball_round_trips_through_nbt() {
        let owner = Uuid::from_u128(77);
        let nbt = Compound::new()
            .with("id", "minecraft:fireball")
            .with("Owner", crate::entity::save::uuid_tag(owner))
            .with("LeftOwner", true)
            .with("HasBeenShot", true)
            .with("acceleration_power", 0.3f64)
            .with("ExplosionPower", 3i8);
        let entity = load(&nbt);
        let EntityState::Hurting(state) = &entity.state else {
            panic!("a fireball loads as a hurting projectile");
        };
        assert_eq!(state.owner, Some(owner));
        assert!(state.left_owner && state.has_been_shot);
        assert_eq!(state.acceleration_power, 0.3);
        assert_eq!(state.explosion_power, 3);
        let saved = save_entity(&entity, None).unwrap();
        assert_eq!(saved.get_f64("acceleration_power"), Some(0.3));
        assert_eq!(saved.get_i64("ExplosionPower"), Some(3));
        assert_eq!(
            saved
                .get_compound("Item")
                .and_then(|item| item.get_str("id")),
            Some("minecraft:fire_charge")
        );
        assert_eq!(save_entity(&load(&saved), None), Some(saved));
    }

    #[test]
    fn a_missing_acceleration_and_power_read_as_the_defaults() {
        let entity = load(&Compound::new().with("id", "minecraft:fireball"));
        let EntityState::Hurting(state) = &entity.state else {
            panic!("a hurting projectile");
        };
        assert_eq!(state.acceleration_power, 0.1);
        assert_eq!(state.explosion_power, 1);
        assert!(state.incendiary);
    }

    #[test]
    fn a_small_fireball_has_no_explosion_power_and_a_wither_skull_keeps_its_danger() {
        let small = load(&Compound::new().with("id", "minecraft:small_fireball"));
        let saved = save_entity(&small, None).unwrap();
        assert!(!saved.contains_key("ExplosionPower"));
        assert!(saved.contains_key("acceleration_power"));
        let skull = load(
            &Compound::new()
                .with("id", "minecraft:wither_skull")
                .with("dangerous", true),
        );
        assert!(is_dangerous(&skull.base));
        let saved = save_entity(&skull, None).unwrap();
        assert_eq!(saved.get_bool("dangerous"), Some(true));
        assert!(!saved.contains_key("Item"));
        assert_eq!(save_entity(&load(&saved), None), Some(saved));
    }

    #[test]
    fn only_the_skull_does_not_burn() {
        let (small, _) = projectile(EntityKind::SmallFireball);
        let (skull, _) = projectile(EntityKind::WitherSkull);
        assert!(should_burn(&small));
        assert!(!should_burn(&skull));
    }
}
