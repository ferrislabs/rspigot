use std::borrow::Cow;

use rspigot_data::variants::VariantFamily;
use rspigot_nbt::Compound;

use crate::effect::{AttributeModifier, AttributeOperation};
use crate::entity::base::DATA_SHARED_FLAGS;
use crate::entity::base::EntityBase;
use crate::entity::data::{Pose, SyncedValue};
use crate::entity::living::{LivingAttributes, LivingState};
use crate::entity::spawn::{SpawnEnvironment, ageable_finalize};
use crate::entity::species;
use crate::entity::variants::{
    CAT_DATA_COLLAR_COLOR, CAT_DATA_IS_LYING, CAT_DATA_RELAX_STATE_ONE, DEFAULT_COLLAR_COLOR,
    VariantContext,
};
use crate::random::RandomSource;
use crate::spawn::SpawnGroupData;

pub const KEYS: &[&str] = &["CollarColor"];
const MOVEMENT_SPEED: &str = "minecraft:movement_speed";
pub const SPRINTING_MODIFIER: &str = "minecraft:sprinting";
pub const AMBIENT_INTERVAL: i32 = 120;
pub const GIFT_CHANCE: f32 = 0.7;
pub const TEMPT_SPEED: f64 = 0.6;
pub const SPRINT_SPEED: f64 = 1.33;
const SPRINTING_BONUS: f64 = 0.300_000_011_920_928_96;
const FLAG_SPRINTING: u8 = 1 << 3;
const GIFT_START: i64 = 23_667;
const GIFT_END: i64 = 362;
const DAY_TICKS: i64 = 24_000;
const PURREOW_CHANCE: i32 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CatState;

pub fn define(base: &mut EntityBase) -> CatState {
    base.data
        .define(CAT_DATA_IS_LYING, SyncedValue::Boolean(false));
    base.data
        .define(CAT_DATA_RELAX_STATE_ONE, SyncedValue::Boolean(false));
    base.data.define(
        CAT_DATA_COLLAR_COLOR,
        SyncedValue::Int(DEFAULT_COLLAR_COLOR),
    );
    CatState
}

pub fn is_lying(base: &EntityBase) -> bool {
    base.data.boolean(CAT_DATA_IS_LYING)
}

pub fn set_lying(base: &mut EntityBase, lying: bool) {
    base.data
        .set(CAT_DATA_IS_LYING, SyncedValue::Boolean(lying));
}

pub fn is_relax_state_one(base: &EntityBase) -> bool {
    base.data.boolean(CAT_DATA_RELAX_STATE_ONE)
}

pub fn set_relax_state_one(base: &mut EntityBase, relaxed: bool) {
    base.data
        .set(CAT_DATA_RELAX_STATE_ONE, SyncedValue::Boolean(relaxed));
}

pub fn collar_color(base: &EntityBase) -> u8 {
    species::dye_by_id(base.data.int(CAT_DATA_COLLAR_COLOR))
}

pub fn set_collar_color(base: &mut EntityBase, color: u8) {
    base.data
        .set(CAT_DATA_COLLAR_COLOR, SyncedValue::Int(i32::from(color)));
}

pub fn is_crouching(base: &EntityBase) -> bool {
    base.pose() == Pose::Crouching
}

pub fn is_stepping_carefully(base: &EntityBase) -> bool {
    is_crouching(base)
}

pub fn is_sprinting(base: &EntityBase) -> bool {
    base.data.flag(DATA_SHARED_FLAGS, FLAG_SPRINTING)
}

pub fn set_sprinting(base: &mut EntityBase, attributes: &mut LivingAttributes, sprinting: bool) {
    base.set_shared_flag(FLAG_SPRINTING, sprinting);
    attributes.remove_modifier(MOVEMENT_SPEED, SPRINTING_MODIFIER);
    if sprinting {
        attributes.add_modifier(
            MOVEMENT_SPEED,
            AttributeModifier {
                id: Cow::Borrowed(SPRINTING_MODIFIER),
                amount: SPRINTING_BONUS,
                operation: AttributeOperation::AddMultipliedTotal,
            },
        );
    }
}

pub fn apply_move_pose(
    base: &mut EntityBase,
    attributes: &mut LivingAttributes,
    wanted_speed: Option<f64>,
) {
    let (pose, sprinting) = match wanted_speed {
        Some(speed) if speed == TEMPT_SPEED => (Pose::Crouching, false),
        Some(speed) if speed == SPRINT_SPEED => (Pose::Standing, true),
        _ => (Pose::Standing, false),
    };
    base.set_pose(pose);
    set_sprinting(base, attributes, sprinting);
}

pub fn waking_up_gift_chance(day_time: i64, has_day_cycle: bool) -> f32 {
    if !has_day_cycle {
        return 0.0;
    }
    let time = day_time.rem_euclid(DAY_TICKS);
    if !(GIFT_END..GIFT_START).contains(&time) {
        GIFT_CHANCE
    } else {
        0.0
    }
}

pub fn offspring_collar(first: u8, second: u8, random: &mut impl RandomSource) -> u8 {
    species::mixed_wool_color(first, second, random)
}

pub fn ambient_sound(
    base: &EntityBase,
    baby: bool,
    in_love: bool,
    random: &mut impl RandomSource,
) -> Option<&'static str> {
    let set = species::sound_set(base, VariantFamily::Cat, baby)?;
    if !crate::entity::tamable::is_tame(base) {
        return set.stray_ambient;
    }
    if in_love {
        return set.purr;
    }
    if random.next_int(PURREOW_CHANCE) == 0 {
        set.purreow
    } else {
        set.ambient
    }
}

pub fn read(nbt: &Compound, base: &mut EntityBase, _state: &mut CatState) {
    let color = nbt
        .get_i64("CollarColor")
        .map_or(species::dye_by_id(DEFAULT_COLLAR_COLOR), |value| {
            species::dye_by_id(i32::from(value as i8))
        });
    set_collar_color(base, color);
}

pub fn write(base: &EntityBase, _state: &CatState, out: &mut Compound) {
    out.insert("CollarColor", collar_color(base) as i8);
}

pub fn finalize(
    base: &mut EntityBase,
    living: &mut LivingState,
    environment: &SpawnEnvironment,
    group: Option<SpawnGroupData>,
    level: &mut impl RandomSource,
) -> Option<SpawnGroupData> {
    let group = ageable_finalize(base, living, group, level);
    let mut context = VariantContext::in_biome(environment.biome);
    context.moon_brightness = environment.moon_brightness;
    if let Some(variant) = species::select_variant(VariantFamily::Cat, &context, level) {
        species::set_variant(base, VariantFamily::Cat, variant);
    }
    let sound = species::pick_sound_variant(VariantFamily::Cat, level);
    species::set_sound_variant(base, VariantFamily::Cat, sound);
    group
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EntityId;
    use crate::entity::species::Species;
    use crate::random::LegacyRandom;
    use rspigot_entities::EntityKind;
    use uuid::Uuid;

    fn cat() -> EntityBase {
        let mut base = EntityBase::new(EntityId(1), Uuid::from_u128(1), EntityKind::Cat);
        Species::new(&mut base);
        base
    }

    #[test]
    fn data_is_defined_at_the_java_indexes() {
        let base = cat();
        assert_eq!(base.data.get(21), Some(&SyncedValue::Boolean(false)));
        assert_eq!(base.data.get(22), Some(&SyncedValue::Boolean(false)));
        assert_eq!(base.data.get(23), Some(&SyncedValue::Int(14)));
        assert_eq!(collar_color(&base), 14);
    }

    #[test]
    fn lying_relax_and_collar_are_synchronised() {
        let mut base = cat();
        set_lying(&mut base, true);
        set_relax_state_one(&mut base, true);
        set_collar_color(&mut base, 3);
        assert!(is_lying(&base) && is_relax_state_one(&base));
        assert_eq!(base.data.get(21), Some(&SyncedValue::Boolean(true)));
        assert_eq!(base.data.get(23), Some(&SyncedValue::Int(3)));
    }

    #[test]
    fn collar_round_trips_as_a_legacy_byte() {
        let mut base = cat();
        let mut state = CatState;
        read(
            &Compound::new().with("CollarColor", 5i8),
            &mut base,
            &mut state,
        );
        assert_eq!(collar_color(&base), 5);
        let mut out = Compound::new();
        write(&base, &state, &mut out);
        assert_eq!(out.get_i64("CollarColor"), Some(5));
        read(&Compound::new(), &mut base, &mut state);
        assert_eq!(collar_color(&base), 14);
        read(
            &Compound::new().with("CollarColor", 99i8),
            &mut base,
            &mut state,
        );
        assert_eq!(collar_color(&base), 0);
    }

    #[test]
    fn move_speed_picks_the_pose_and_the_sprint() {
        let mut base = cat();
        let mut attributes = LivingAttributes::new(EntityKind::Cat);
        apply_move_pose(&mut base, &mut attributes, Some(0.6));
        assert!(is_crouching(&base) && is_stepping_carefully(&base));
        assert!(!is_sprinting(&base));
        apply_move_pose(&mut base, &mut attributes, Some(1.33));
        assert!(!is_crouching(&base) && is_sprinting(&base));
        assert!(attributes.has_modifier(MOVEMENT_SPEED, SPRINTING_MODIFIER));
        apply_move_pose(&mut base, &mut attributes, Some(0.8));
        assert!(!is_crouching(&base) && !is_sprinting(&base));
        assert!(!attributes.has_modifier(MOVEMENT_SPEED, SPRINTING_MODIFIER));
        apply_move_pose(&mut base, &mut attributes, Some(0.6));
        apply_move_pose(&mut base, &mut attributes, None);
        assert!(!is_crouching(&base));
    }

    #[test]
    fn the_morning_gift_chance_follows_the_day_timeline() {
        assert_eq!(waking_up_gift_chance(0, true), 0.7);
        assert_eq!(waking_up_gift_chance(361, true), 0.7);
        assert_eq!(waking_up_gift_chance(362, true), 0.0);
        assert_eq!(waking_up_gift_chance(23_666, true), 0.0);
        assert_eq!(waking_up_gift_chance(23_667 + 24_000, true), 0.7);
        assert_eq!(waking_up_gift_chance(0, false), 0.0);
    }

    #[test]
    fn ambient_sound_depends_on_tame_and_love() {
        let mut base = cat();
        let mut random = LegacyRandom::new(1);
        let wild = ambient_sound(&base, false, false, &mut random).unwrap();
        assert!(wild.contains("stray_ambient"));
        base.data.define(18, SyncedValue::Byte(0));
        crate::entity::tamable::set_tame(&mut base, true);
        let purr = ambient_sound(&base, false, true, &mut random).unwrap();
        assert!(purr.contains("purr") && !purr.contains("purreow"));
        let tame = ambient_sound(&base, false, false, &mut random).unwrap();
        assert!(tame.contains("ambient") || tame.contains("purreow"));
    }
}
