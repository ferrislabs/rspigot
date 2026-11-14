use std::num::NonZeroU8;

use rspigot_entities::EntityKind;
use rspigot_items::ItemKind;
use uuid::Uuid;

use super::arrow;
use super::base::EntityBase;
use super::data::SyncedValue;
use crate::item::ItemStack;
use crate::potion;
use crate::random::{LegacyRandom, RandomSource};
use crate::{Vec3, mth};

pub const DATA_ITEM_STACK: u8 = 8;
pub const GRAVITY: f64 = 0.03;
pub const SPIT_GRAVITY: f64 = 0.06;
pub const SPIT_DAMAGE: f32 = 1.0;
pub const SPIT_OFFSET: f64 = 0.100_000_001_490_116_12;
pub const SPIT_POWER: f32 = 1.5;
pub const SPIT_UNCERTAINTY: f32 = 10.0;
pub const AIR_DRAG: f32 = 0.99;
pub const WATER_DRAG: f32 = 0.8;
pub const HIT_EVENT: i8 = 3;
pub const OWNER_EYE_OFFSET: f64 = 0.100_000_001_490_116_12;
pub const SNOWBALL_BLAZE_DAMAGE: f32 = 3.0;
pub const SNOWBALL_DAMAGE: f32 = 0.0;
const SHOOT_SPREAD: f64 = 0.017_227_5;

#[derive(Debug, Clone, PartialEq)]
pub struct ThrownState {
    pub owner: Option<Uuid>,
    pub left_owner: bool,
    pub left_owner_checked: bool,
    pub has_been_shot: bool,
    pub random: LegacyRandom,
}

pub fn default_item() -> ItemStack {
    ItemStack::new(ItemKind::Snowball.id(), NonZeroU8::MIN)
}

pub fn is_potion(kind: EntityKind) -> bool {
    kind == EntityKind::SplashPotion
}

pub fn is_thrown(kind: EntityKind) -> bool {
    kind == EntityKind::Snowball || kind == EntityKind::LlamaSpit || is_potion(kind)
}

pub fn is_spit(kind: EntityKind) -> bool {
    kind == EntityKind::LlamaSpit
}

pub fn carries_item(kind: EntityKind) -> bool {
    !is_spit(kind)
}

pub fn default_item_of(kind: EntityKind) -> ItemStack {
    match kind {
        EntityKind::SplashPotion => ItemStack::new(ItemKind::SplashPotion.id(), NonZeroU8::MIN),
        _ => default_item(),
    }
}

pub fn gravity(kind: EntityKind) -> f64 {
    match kind {
        EntityKind::SplashPotion => potion::GRAVITY,
        EntityKind::LlamaSpit => SPIT_GRAVITY,
        _ => GRAVITY,
    }
}

impl ThrownState {
    pub fn new(base: &mut EntityBase, seed: i64) -> Self {
        if carries_item(base.kind) {
            base.data.define(
                DATA_ITEM_STACK,
                SyncedValue::Item(Some(default_item_of(base.kind))),
            );
        }
        Self {
            owner: None,
            left_owner: false,
            left_owner_checked: false,
            has_been_shot: false,
            random: LegacyRandom::new(seed),
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

pub fn movement_to_shoot(
    random: &mut impl RandomSource,
    direction: Vec3,
    power: f32,
    uncertainty: f32,
) -> Vec3 {
    let spread = SHOOT_SPREAD * f64::from(uncertainty);
    let jitter = Vec3::new(
        random.triangle(0.0, spread),
        random.triangle(0.0, spread),
        random.triangle(0.0, spread),
    );
    (direction.normalize() + jitter).scale(f64::from(power))
}

pub fn shoot(
    base: &mut EntityBase,
    state: &mut ThrownState,
    direction: Vec3,
    power: f32,
    uncertainty: f32,
) {
    let movement = movement_to_shoot(&mut state.random, direction, power, uncertainty);
    base.velocity = movement;
    base.needs_sync = true;
    let horizontal = (movement.x * movement.x + movement.z * movement.z).sqrt();
    base.set_yaw((mth::atan2(movement.x, movement.z) * mth::RAD_TO_DEG) as f32);
    base.set_pitch((mth::atan2(movement.y, horizontal) * mth::RAD_TO_DEG) as f32);
}

pub fn smothered(blocks: &impl crate::entity::BlockView, base: &EntityBase) -> bool {
    let bounds = base.bounding_box();
    let low = |value: f64| value.floor() as i32;
    for x in low(bounds.min_x)..=low(bounds.max_x) {
        for y in low(bounds.min_y)..=low(bounds.max_y) {
            for z in low(bounds.min_z)..=low(bounds.max_z) {
                let state = blocks.block(crate::BlockPos::new(x, y, z));
                if state.is_none_or(|state| state.is_air()) {
                    return false;
                }
            }
        }
    }
    true
}

pub fn apply_gravity(base: &mut EntityBase) {
    if !base.is_no_gravity() {
        base.velocity = base.velocity + Vec3::new(0.0, -gravity(base.kind), 0.0);
    }
}

pub fn apply_inertia(base: &mut EntityBase) {
    let drag = if base.fluid.in_water {
        WATER_DRAG
    } else {
        AIR_DRAG
    };
    base.velocity = base.velocity.scale(f64::from(drag));
}

pub fn update_rotation(base: &mut EntityBase) {
    let movement = base.velocity;
    let horizontal = (movement.x * movement.x + movement.z * movement.z).sqrt();
    let pitch = (mth::atan2(movement.y, horizontal) * mth::RAD_TO_DEG) as f32;
    let yaw = (mth::atan2(movement.x, movement.z) * mth::RAD_TO_DEG) as f32;
    let next_pitch = arrow::lerp_rotation(base.pitch, pitch);
    let next_yaw = arrow::lerp_rotation(base.yaw, yaw);
    base.set_pitch(next_pitch);
    base.set_yaw(next_yaw);
}

pub fn knockback_direction(base: &EntityBase) -> (f64, f64) {
    (-base.velocity.x, -base.velocity.z)
}

#[cfg(test)]
mod tests {
    use rspigot_entities::EntityKind;

    use super::*;
    use crate::EntityId;

    fn snowball() -> (EntityBase, ThrownState) {
        let mut base = EntityBase::new(EntityId(1), Uuid::from_u128(1), EntityKind::Snowball);
        let state = ThrownState::new(&mut base, 9);
        (base, state)
    }

    #[test]
    fn the_snowball_carries_a_snowball_item_in_its_first_data_slot() {
        let (base, _) = snowball();
        assert_eq!(DATA_ITEM_STACK, 8);
        assert_eq!(
            item(&base).map(|stack| stack.item),
            Some(ItemKind::Snowball.id())
        );
    }

    #[test]
    fn shooting_without_uncertainty_moves_along_the_direction_at_the_power() {
        let (mut base, mut state) = snowball();
        shoot(&mut base, &mut state, Vec3::new(0.0, 0.0, 5.0), 1.6, 0.0);
        assert_eq!(base.velocity, Vec3::new(0.0, 0.0, f64::from(1.6f32)));
        assert_eq!(base.yaw, 0.0);
        assert_eq!(base.pitch, 0.0);
        assert!(base.needs_sync);
    }

    #[test]
    fn shooting_adds_a_triangle_jitter_of_the_vanilla_spread() {
        let mut random = LegacyRandom::new(5);
        let mut mirror = LegacyRandom::new(5);
        let direction = Vec3::new(1.0, 2.0, 3.0);
        let spread = 0.0172275 * f64::from(12.0f32);
        let jitter = Vec3::new(
            mirror.triangle(0.0, spread),
            mirror.triangle(0.0, spread),
            mirror.triangle(0.0, spread),
        );
        let expected = (direction.normalize() + jitter).scale(f64::from(1.6f32));
        assert_eq!(
            movement_to_shoot(&mut random, direction, 1.6, 12.0),
            expected
        );
        assert!(expected != direction.normalize().scale(f64::from(1.6f32)));
    }

    #[test]
    fn shooting_sets_the_rotation_from_the_movement() {
        let (mut base, mut state) = snowball();
        shoot(&mut base, &mut state, Vec3::new(1.0, 0.0, 0.0), 1.0, 0.0);
        assert!((base.yaw - 90.0).abs() < 1.0e-4);
        shoot(&mut base, &mut state, Vec3::new(0.0, 1.0, 1.0), 1.0, 0.0);
        assert!((base.pitch - 45.0).abs() < 1.0e-4);
    }

    #[test]
    fn gravity_then_drag_follow_the_vanilla_formula_over_many_ticks() {
        let (mut base, _) = snowball();
        base.velocity = Vec3::new(0.5, 0.2, -0.3);
        let drag = f64::from(0.99f32);
        let mut expected = base.velocity;
        for _ in 0..40 {
            apply_gravity(&mut base);
            apply_inertia(&mut base);
            expected = (expected + Vec3::new(0.0, -0.03, 0.0)).scale(drag);
            assert_eq!(base.velocity, expected);
        }
    }

    #[test]
    fn water_slows_the_projectile_to_four_fifths_instead_of_the_air_drag() {
        let (mut base, _) = snowball();
        base.velocity = Vec3::new(1.0, 0.0, 0.0);
        base.fluid.in_water = true;
        apply_inertia(&mut base);
        assert_eq!(base.velocity.x, f64::from(0.8f32));
    }

    #[test]
    fn no_gravity_keeps_the_vertical_motion() {
        let (mut base, _) = snowball();
        base.set_no_gravity(true);
        base.velocity = Vec3::new(0.0, 0.1, 0.0);
        apply_gravity(&mut base);
        assert_eq!(base.velocity.y, 0.1);
    }

    #[test]
    fn the_rotation_follows_the_motion_with_a_fifth_lerp() {
        let (mut base, _) = snowball();
        base.velocity = Vec3::new(1.0, 0.0, 0.0);
        update_rotation(&mut base);
        assert!((base.yaw - 18.0).abs() < 1.0e-4);
        assert_eq!(base.pitch, 0.0);
    }

    #[test]
    fn the_hit_knockback_pushes_along_the_motion() {
        let (mut base, _) = snowball();
        base.velocity = Vec3::new(0.4, -0.1, -0.2);
        assert_eq!(knockback_direction(&base), (-0.4, 0.2));
    }

    #[test]
    fn a_snowball_is_simulated_and_round_trips_through_nbt() {
        use crate::entity::save::{is_simulated, load_entity, save_entity};
        use rspigot_nbt::Compound;
        assert!(is_simulated(EntityKind::Snowball));
        let owner = Uuid::from_u128(77);
        let nbt = Compound::new()
            .with("id", "minecraft:snowball")
            .with("Owner", crate::entity::save::uuid_tag(owner))
            .with("LeftOwner", true)
            .with("HasBeenShot", true);
        let entity = load_entity(&nbt, EntityId(1), Uuid::from_u128(1)).unwrap();
        let crate::entity::EntityState::Thrown(state) = &entity.state else {
            panic!("a snowball loads as a thrown projectile");
        };
        assert_eq!(state.owner, Some(owner));
        assert!(state.left_owner && state.has_been_shot);
        let saved = save_entity(&entity, None).unwrap();
        assert_eq!(saved.get_bool("LeftOwner"), Some(true));
        assert_eq!(saved.get_bool("HasBeenShot"), Some(true));
        assert_eq!(
            saved
                .get_compound("Item")
                .and_then(|item| item.get_str("id")),
            Some("minecraft:snowball")
        );
        let again = load_entity(&saved, EntityId(1), Uuid::from_u128(1)).unwrap();
        assert_eq!(save_entity(&again, None), Some(saved));
    }

    fn splash_potion() -> (EntityBase, ThrownState) {
        let mut base = EntityBase::new(EntityId(1), Uuid::from_u128(1), EntityKind::SplashPotion);
        let state = ThrownState::new(&mut base, 9);
        (base, state)
    }

    fn spit() -> (EntityBase, ThrownState) {
        let mut base = EntityBase::new(EntityId(1), Uuid::from_u128(1), EntityKind::LlamaSpit);
        let state = ThrownState::new(&mut base, 9);
        (base, state)
    }

    #[test]
    fn a_llama_spit_carries_no_item_and_falls_at_six_hundredths() {
        let (mut base, _) = spit();
        assert!(item(&base).is_none());
        assert!(base.data.get(DATA_ITEM_STACK).is_none());
        assert!(is_thrown(EntityKind::LlamaSpit));
        assert!(is_spit(EntityKind::LlamaSpit));
        assert!(!carries_item(EntityKind::LlamaSpit));
        base.velocity = Vec3::new(0.5, 0.2, -0.3);
        let drag = f64::from(0.99f32);
        let mut expected = base.velocity;
        for _ in 0..40 {
            apply_gravity(&mut base);
            apply_inertia(&mut base);
            expected = (expected + Vec3::new(0.0, -0.06, 0.0)).scale(drag);
            assert_eq!(base.velocity, expected);
        }
    }

    #[test]
    fn a_llama_spit_round_trips_without_an_item() {
        use crate::entity::save::{is_simulated, load_entity, save_entity};
        use rspigot_nbt::Compound;
        assert!(is_simulated(EntityKind::LlamaSpit));
        let owner = Uuid::from_u128(77);
        let nbt = Compound::new()
            .with("id", "minecraft:llama_spit")
            .with("Owner", crate::entity::save::uuid_tag(owner))
            .with("HasBeenShot", true);
        let entity = load_entity(&nbt, EntityId(1), Uuid::from_u128(1)).unwrap();
        let crate::entity::EntityState::Thrown(state) = &entity.state else {
            panic!("a llama spit loads as a thrown projectile");
        };
        assert_eq!(state.owner, Some(owner));
        let saved = save_entity(&entity, None).unwrap();
        assert!(!saved.contains_key("Item"));
        assert_eq!(saved.get_bool("HasBeenShot"), Some(true));
    }

    #[test]
    fn a_splash_potion_carries_a_splash_potion_item_in_the_same_data_slot() {
        let (base, _) = splash_potion();
        assert_eq!(
            item(&base).map(|stack| stack.item),
            Some(ItemKind::SplashPotion.id())
        );
        assert!(is_potion(EntityKind::SplashPotion));
        assert!(is_thrown(EntityKind::SplashPotion));
        assert!(!is_potion(EntityKind::Snowball));
    }

    #[test]
    fn a_splash_potion_falls_at_five_hundredths_and_slows_with_the_same_drag() {
        let (mut base, _) = splash_potion();
        base.velocity = Vec3::new(0.5, 0.2, -0.3);
        let drag = f64::from(0.99f32);
        let mut expected = base.velocity;
        for _ in 0..40 {
            apply_gravity(&mut base);
            apply_inertia(&mut base);
            expected = (expected + Vec3::new(0.0, -0.05, 0.0)).scale(drag);
            assert_eq!(base.velocity, expected);
        }
    }

    #[test]
    fn a_splash_potion_launches_like_any_thrown_projectile() {
        let (mut base, mut state) = splash_potion();
        shoot(&mut base, &mut state, Vec3::new(0.0, 0.0, 5.0), 0.75, 0.0);
        assert_eq!(base.velocity, Vec3::new(0.0, 0.0, f64::from(0.75f32)));
        let mut random = LegacyRandom::new(5);
        let mut mirror = LegacyRandom::new(5);
        let spread = 0.0172275 * f64::from(8.0f32);
        let jitter = Vec3::new(
            mirror.triangle(0.0, spread),
            mirror.triangle(0.0, spread),
            mirror.triangle(0.0, spread),
        );
        let direction = Vec3::new(1.0, 2.0, 3.0);
        assert_eq!(
            movement_to_shoot(&mut random, direction, 0.75, 8.0),
            (direction.normalize() + jitter).scale(f64::from(0.75f32))
        );
    }

    #[test]
    fn a_splash_potion_round_trips_its_potion_through_nbt() {
        use crate::entity::save::{is_simulated, load_entity, save_entity};
        use rspigot_nbt::Compound;
        assert!(is_simulated(EntityKind::SplashPotion));
        let nbt = Compound::new().with("id", "minecraft:splash_potion").with(
            "Item",
            Compound::new()
                .with("id", "minecraft:splash_potion")
                .with("count", 1)
                .with(
                    "components",
                    Compound::new().with(
                        "minecraft:potion_contents",
                        Compound::new().with("potion", "minecraft:healing"),
                    ),
                ),
        );
        let entity = load_entity(&nbt, EntityId(1), Uuid::from_u128(1)).unwrap();
        let stack = match &entity.state {
            crate::entity::EntityState::Thrown(_) => item(&entity.base).unwrap(),
            _ => panic!("a splash potion loads as a thrown projectile"),
        };
        assert_eq!(stack.item, ItemKind::SplashPotion.id());
        assert!(crate::potion::PotionContents::of(&stack).is(crate::potion::HEALING));
        let saved = save_entity(&entity, None).unwrap();
        let again = load_entity(&saved, EntityId(1), Uuid::from_u128(1)).unwrap();
        assert_eq!(save_entity(&again, None), Some(saved));
    }
}
