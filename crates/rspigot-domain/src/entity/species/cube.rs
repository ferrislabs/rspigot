use rspigot_entities::{EntityDimensions, EntityKind};
use rspigot_nbt::Compound;

use crate::entity::ageable;
use crate::entity::base::EntityBase;
use crate::entity::data::SyncedValue;
use crate::entity::living::{
    ARMOR, ATTACK_DAMAGE, LivingState, MAX_HEALTH, MOVEMENT_SPEED, set_health,
};
use crate::entity::spawn::{SpawnEnvironment, ageable_finalize};
use crate::entity::species::sulfur_cube::{self, SulfurState};
use crate::random::RandomSource;
use crate::spawn::SpawnGroupData;

pub const DATA_SIZE: u8 = 18;
pub const KEYS: &[&str] = &["Size", "wasOnGround"];
pub const SULFUR_KEYS: &[&str] = &["Size", "wasOnGround", "pickup_timer", "from_bucket", "fuse"];
pub const MIN_SIZE: i32 = 1;
pub const MAX_SIZE: i32 = 127;
pub const BASE_SPEED: f32 = 0.2;
pub const SPEED_PER_SIZE: f32 = 0.1;
pub const MAGMA_ARMOR_PER_SIZE: i32 = 3;
pub const MAGMA_ATTACK_BONUS: f32 = 2.0;
pub const MAGMA_JUMP_PER_SIZE: f32 = 0.1;
pub const MAGMA_LAVA_JUMP_BASE: f64 = 0.22;
pub const MAGMA_LAVA_JUMP_PER_SIZE: f32 = 0.05;
pub const MAGMA_JUMP_DELAY_FACTOR: i32 = 4;
pub const JUMP_DELAY_BASE: i32 = 10;
pub const JUMP_DELAY_RANGE: i32 = 20;
pub const AGGRESSIVE_JUMP_DIVISOR: i32 = 3;
pub const SPLIT_COUNT_BASE: i32 = 2;
pub const SPLIT_COUNT_RANGE: i32 = 3;
pub const SPLIT_LIFT: f64 = 0.5;
pub const SPAWN_SIZE_BOUND: i32 = 3;
pub const SPAWN_SIZE_BONUS: f32 = 0.5;
pub const ATTACK_SOUND: &str = "minecraft:entity.slime.attack";
pub const TARGET_Y_RANGE: f64 = 4.0;
pub const SOUND_VOLUME_PER_SIZE: f32 = 0.4;
pub const TINY_PITCH: f32 = 1.4;
pub const BIG_PITCH: f32 = 0.8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CubeState {
    pub was_on_ground: bool,
    pub sulfur: SulfurState,
}

pub fn is_cube(kind: EntityKind) -> bool {
    matches!(
        kind,
        EntityKind::Slime | EntityKind::MagmaCube | EntityKind::SulfurCube
    )
}

pub fn keys(kind: EntityKind) -> &'static [&'static str] {
    if kind == EntityKind::SulfurCube {
        SULFUR_KEYS
    } else {
        KEYS
    }
}

pub fn define(base: &mut EntityBase) -> CubeState {
    base.data.define(DATA_SIZE, SyncedValue::Int(MIN_SIZE));
    if base.kind == EntityKind::SulfurCube {
        base.data.define(
            sulfur_cube::DATA_MAX_FUSE,
            SyncedValue::Int(sulfur_cube::NO_FUSE),
        );
        base.data
            .define(sulfur_cube::DATA_FROM_BUCKET, SyncedValue::Boolean(false));
    }
    base.dimensions = scaled_dimensions(base.kind.dimensions(), MIN_SIZE);
    CubeState::default()
}

pub fn size(base: &EntityBase) -> i32 {
    match base.data.get(DATA_SIZE) {
        Some(SyncedValue::Int(size)) => *size,
        _ => MIN_SIZE,
    }
}

pub fn is_tiny(base: &EntityBase) -> bool {
    size(base) < 2
}

pub fn scaled_dimensions(dimensions: EntityDimensions, size: i32) -> EntityDimensions {
    let scale = size as f32;
    EntityDimensions {
        width: dimensions.width * scale,
        height: dimensions.height * scale,
        eye_height: dimensions.eye_height * scale,
    }
}

pub fn set_size(base: &mut EntityBase, living: &mut LivingState, size: i32, update_health: bool) {
    let before = self::size(base);
    let clamped = size.clamp(MIN_SIZE, MAX_SIZE);
    base.data.set(DATA_SIZE, SyncedValue::Int(clamped));
    base.dimensions = scaled_dimensions(base.kind.dimensions(), clamped);
    if before != clamped {
        base.set_yaw(base.head_yaw);
        living.body_yaw = base.head_yaw;
    }
    let health = if base.kind == EntityKind::SulfurCube {
        clamped * sulfur_cube::HEALTH_PER_SIZE
    } else {
        clamped * clamped
    };
    living.attributes.set_base(MAX_HEALTH, f64::from(health));
    living.attributes.set_base(
        MOVEMENT_SPEED,
        f64::from(BASE_SPEED + SPEED_PER_SIZE * clamped as f32),
    );
    if update_health {
        let max = living.max_health() as f32;
        set_health(base, living, max);
    }
    if base.kind != EntityKind::SulfurCube {
        living
            .attributes
            .set_base(ATTACK_DAMAGE, f64::from(self::size(base)));
    } else if update_health
        && clamped == sulfur_cube::BABY_SIZE
        && let Some(ageable) = living.mob.as_mut().and_then(|mob| mob.ageable.as_mut())
        && !ageable::is_baby(base, ageable)
    {
        ageable::set_baby(base, ageable, true);
    }
    if base.kind == EntityKind::MagmaCube {
        living
            .attributes
            .set_base(ARMOR, f64::from(size.wrapping_mul(MAGMA_ARMOR_PER_SIZE)));
    }
}

pub fn read_size(nbt: &Compound, base: &mut EntityBase, living: &mut LivingState) {
    let stored = nbt.get_i32("Size").unwrap_or(0);
    set_size(base, living, stored.saturating_add(1), false);
}

pub fn read(nbt: &Compound, base: &mut EntityBase, state: &mut CubeState) {
    state.was_on_ground = nbt.get_bool("wasOnGround").unwrap_or(false);
    if base.kind == EntityKind::SulfurCube {
        sulfur_cube::read(nbt, base, &mut state.sulfur);
    }
}

pub fn write(base: &EntityBase, state: &CubeState, out: &mut Compound) {
    out.insert("Size", size(base) - 1);
    out.insert("wasOnGround", state.was_on_ground);
    if base.kind == EntityKind::SulfurCube {
        sulfur_cube::write(base, &state.sulfur, out);
    }
}

pub fn jump_delay(kind: EntityKind, random: &mut impl RandomSource) -> i32 {
    let delay = random.next_int(JUMP_DELAY_RANGE) + JUMP_DELAY_BASE;
    if kind == EntityKind::MagmaCube {
        delay * MAGMA_JUMP_DELAY_FACTOR
    } else {
        delay
    }
}

pub fn split_count(random: &mut impl RandomSource) -> i32 {
    random.next_int(SPLIT_COUNT_RANGE) + SPLIT_COUNT_BASE
}

pub fn split_offset(width: f32, index: i32) -> (f32, f32) {
    let half = width / 2.0;
    (
        ((index % 2) as f32 - 0.5) * half,
        ((index / 2) as f32 - 0.5) * half,
    )
}

pub fn attack_damage(base: &EntityBase, living: &mut LivingState) -> f32 {
    let value = living.attribute(ATTACK_DAMAGE) as f32;
    if base.kind == EntityKind::MagmaCube {
        value + MAGMA_ATTACK_BONUS
    } else {
        value
    }
}

pub fn can_deal_damage(base: &EntityBase, effective_ai: bool) -> bool {
    if base.kind == EntityKind::SulfurCube {
        false
    } else if base.kind == EntityKind::MagmaCube {
        effective_ai
    } else {
        !is_tiny(base) && effective_ai
    }
}

pub fn sound_volume(base: &EntityBase) -> f32 {
    size(base) as f32 * SOUND_VOLUME_PER_SIZE
}

pub fn sound_pitch(base: &EntityBase, random: &mut impl RandomSource) -> f32 {
    let scale = if is_tiny(base) { TINY_PITCH } else { BIG_PITCH };
    ((random.next_float() - random.next_float()) * 0.2 + 1.0) * scale
}

pub fn jump_sound(kind: EntityKind, tiny: bool) -> &'static str {
    match (kind, tiny) {
        (EntityKind::MagmaCube, _) => "minecraft:entity.magma_cube.jump",
        (_, true) => "minecraft:entity.slime.jump_small",
        (_, false) => "minecraft:entity.slime.jump",
    }
}

pub fn squish_sound(kind: EntityKind, tiny: bool) -> &'static str {
    match (kind, tiny) {
        (EntityKind::MagmaCube, true) => "minecraft:entity.magma_cube.squish_small",
        (EntityKind::MagmaCube, false) => "minecraft:entity.magma_cube.squish",
        (_, true) => "minecraft:entity.slime.squish_small",
        (_, false) => "minecraft:entity.slime.squish",
    }
}

pub fn lava_jump_velocity(size: i32) -> f64 {
    f64::from(size as f32 * MAGMA_LAVA_JUMP_PER_SIZE + MAGMA_LAVA_JUMP_BASE as f32)
}

pub fn magma_jump_bonus(size: i32) -> f32 {
    size as f32 * MAGMA_JUMP_PER_SIZE
}

pub fn set_spawn_size(
    base: &mut EntityBase,
    living: &mut LivingState,
    special_multiplier: f32,
    level: &mut impl RandomSource,
) {
    let mut exponent = level.next_int(SPAWN_SIZE_BOUND);
    if exponent < 2 && level.next_float() < special_multiplier * SPAWN_SIZE_BONUS {
        exponent += 1;
    }
    set_size(base, living, 1 << exponent, true);
}

pub fn finalize(
    base: &mut EntityBase,
    living: &mut LivingState,
    environment: &SpawnEnvironment,
    group: Option<SpawnGroupData>,
    level: &mut impl RandomSource,
) -> Option<SpawnGroupData> {
    if base.kind == EntityKind::SulfurCube {
        let group = ageable_finalize(base, living, group, level);
        let baby = living
            .mob
            .as_ref()
            .and_then(|mob| mob.ageable.as_ref())
            .is_some_and(|ageable| ageable::is_baby(base, ageable));
        let size = if baby {
            sulfur_cube::BABY_SIZE
        } else {
            sulfur_cube::ADULT_SIZE
        };
        set_size(base, living, size, true);
        return group;
    }
    let group = group.or(Some(SpawnGroupData::ageable(false)));
    let group = ageable_finalize(base, living, group, level);
    set_spawn_size(
        base,
        living,
        environment.difficulty.special_multiplier(),
        level,
    );
    group
}

#[cfg(test)]
mod tests {
    use rspigot_nbt::Compound;
    use uuid::Uuid;

    use super::*;
    use crate::EntityId;
    use crate::entity::Entity;
    use crate::entity::living::health;
    use crate::entity::save::{load_entity, save_entity};
    use crate::random::LegacyRandom;

    fn cube(kind: &str, nbt: Compound) -> Entity {
        load_entity(&nbt.with("id", kind), EntityId(1), Uuid::from_u128(1)).unwrap()
    }

    fn slime(nbt: Compound) -> Entity {
        cube("minecraft:slime", nbt)
    }

    fn magma(nbt: Compound) -> Entity {
        cube("minecraft:magma_cube", nbt)
    }

    #[test]
    fn the_size_sits_at_index_eighteen_after_the_ageable_flags() {
        let entity = slime(Compound::new());
        assert_eq!(entity.base.data.get(DATA_SIZE), Some(&SyncedValue::Int(1)));
        assert_eq!(
            entity
                .base
                .data
                .get(crate::entity::ageable::DATA_AGE_LOCKED),
            Some(&SyncedValue::Boolean(false))
        );
    }

    #[test]
    fn the_stored_size_is_one_less_and_round_trips_with_was_on_ground() {
        let entity = slime(Compound::new().with("Size", 3).with("wasOnGround", true));
        assert_eq!(size(&entity.base), 4);
        let saved = save_entity(&entity, None).unwrap();
        assert_eq!(saved.get_i32("Size"), Some(3));
        assert_eq!(saved.get_bool("wasOnGround"), Some(true));
        let again = load_entity(&saved, EntityId(1), Uuid::from_u128(1)).unwrap();
        assert_eq!(save_entity(&again, None), Some(saved));
    }

    #[test]
    fn a_missing_size_is_the_smallest_and_a_huge_one_is_clamped() {
        assert_eq!(size(&slime(Compound::new()).base), 1);
        assert_eq!(size(&slime(Compound::new().with("Size", 500)).base), 127);
        assert_eq!(size(&slime(Compound::new().with("Size", -9)).base), 1);
    }

    #[test]
    fn the_size_scales_the_box_the_health_the_speed_and_the_attack() {
        let mut entity = slime(Compound::new().with("Size", 3));
        let base = EntityKind::Slime.dimensions();
        assert_eq!(entity.base.dimensions.width, base.width * 4.0);
        assert_eq!(entity.base.dimensions.height, base.height * 4.0);
        assert_eq!(entity.base.dimensions.eye_height, base.eye_height * 4.0);
        let living = entity.living_mut().unwrap();
        assert_eq!(living.attribute(MAX_HEALTH), 16.0);
        assert_eq!(living.attribute(ATTACK_DAMAGE), 4.0);
        assert_eq!(
            living.attribute(MOVEMENT_SPEED),
            f64::from(0.2f32 + 0.1f32 * 4.0)
        );
    }

    #[test]
    fn a_stored_health_survives_the_size_attributes() {
        let entity = slime(Compound::new().with("Size", 3).with("Health", 5.0f32));
        assert_eq!(health(&entity.base), 5.0);
    }

    #[test]
    fn a_magma_cube_gains_three_armor_per_size() {
        let mut entity = magma(Compound::new().with("Size", 1));
        let kind_base = entity.base.clone();
        let living = entity.living_mut().unwrap();
        assert_eq!(living.attribute(ARMOR), 6.0);
        assert_eq!(living.attribute(ATTACK_DAMAGE), 2.0);
        assert_eq!(attack_damage(&kind_base, living), 4.0);
    }

    #[test]
    fn only_a_big_slime_deals_contact_damage_but_every_magma_cube_does() {
        let tiny = slime(Compound::new());
        let big = slime(Compound::new().with("Size", 1));
        assert!(!can_deal_damage(&tiny.base, true));
        assert!(can_deal_damage(&big.base, true));
        assert!(!can_deal_damage(&big.base, false));
        let magma = magma(Compound::new());
        assert!(can_deal_damage(&magma.base, true));
        assert!(!can_deal_damage(&magma.base, false));
    }

    #[test]
    fn the_jump_delay_is_ten_to_twenty_nine_and_four_times_that_for_magma() {
        let mut seen_slime = std::collections::BTreeSet::new();
        let mut random = LegacyRandom::new(7);
        for _ in 0..400 {
            seen_slime.insert(jump_delay(EntityKind::Slime, &mut random));
        }
        assert_eq!(seen_slime.first(), Some(&10));
        assert_eq!(seen_slime.last(), Some(&29));
        let mut random = LegacyRandom::new(7);
        let mut other = LegacyRandom::new(7);
        for _ in 0..50 {
            assert_eq!(
                jump_delay(EntityKind::MagmaCube, &mut random),
                jump_delay(EntityKind::Slime, &mut other) * 4
            );
        }
    }

    #[test]
    fn the_split_count_is_two_to_four_and_the_children_sit_on_a_two_by_two_grid() {
        let mut random = LegacyRandom::new(3);
        for _ in 0..100 {
            let count = split_count(&mut random);
            assert!((2..=4).contains(&count));
        }
        assert_eq!(split_offset(2.0, 0), (-0.5, -0.5));
        assert_eq!(split_offset(2.0, 1), (0.5, -0.5));
        assert_eq!(split_offset(2.0, 2), (-0.5, 0.5));
        assert_eq!(split_offset(2.0, 3), (0.5, 0.5));
    }

    #[test]
    fn the_spawn_size_is_a_power_of_two_and_a_harder_difficulty_grows_it() {
        let mut totals = [0usize; 3];
        let mut random = LegacyRandom::new(11);
        for special in [0.0f32, 1.0] {
            for _ in 0..600 {
                let mut entity = slime(Compound::new());
                let (base, living) = match &mut entity.state {
                    crate::entity::EntityState::Living(living) => (&mut entity.base, living),
                    _ => unreachable!(),
                };
                set_spawn_size(base, living, special, &mut random);
                let size = size(base);
                assert!(matches!(size, 1 | 2 | 4 | 8), "{size}");
                if special == 0.0 {
                    assert!(size <= 4);
                }
                totals[usize::from(special > 0.0)] += size as usize;
                assert_eq!(health(base), (size * size) as f32);
            }
        }
        assert!(totals[1] > totals[0]);
    }

    #[test]
    fn the_jump_and_squish_sounds_follow_the_kind_and_the_size() {
        assert_eq!(
            jump_sound(EntityKind::Slime, true),
            "minecraft:entity.slime.jump_small"
        );
        assert_eq!(
            jump_sound(EntityKind::Slime, false),
            "minecraft:entity.slime.jump"
        );
        assert_eq!(
            jump_sound(EntityKind::MagmaCube, true),
            "minecraft:entity.magma_cube.jump"
        );
        assert_eq!(
            squish_sound(EntityKind::MagmaCube, true),
            "minecraft:entity.magma_cube.squish_small"
        );
        assert_eq!(
            squish_sound(EntityKind::Slime, false),
            "minecraft:entity.slime.squish"
        );
    }

    #[test]
    fn the_lava_jump_and_the_magma_boost_scale_with_the_size() {
        assert_eq!(lava_jump_velocity(4), f64::from(4.0f32 * 0.05 + 0.22f32));
        assert_eq!(magma_jump_bonus(4), 4.0 * 0.1);
    }
}
