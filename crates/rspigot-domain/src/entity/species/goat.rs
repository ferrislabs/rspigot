use rspigot_nbt::Compound;
use uuid::Uuid;

use crate::entity::base::EntityBase;
use crate::entity::data::SyncedValue;
use crate::entity::living::{LivingAttributes, LivingState};
use crate::entity::spawn::{SpawnEnvironment, ageable_finalize};
use crate::random::{LegacyRandom, RandomSource};
use crate::spawn::SpawnGroupData;

pub const DATA_IS_SCREAMING: u8 = 18;
pub const DATA_HAS_LEFT_HORN: u8 = 19;
pub const DATA_HAS_RIGHT_HORN: u8 = 20;
pub const ATTACK_DAMAGE: &str = "minecraft:attack_damage";
pub const ADULT_ATTACK_DAMAGE: f64 = 2.0;
pub const BABY_ATTACK_DAMAGE: f64 = 1.0;
pub const SCREAMING_CHANCE: f64 = 0.02;
pub const HORN_LOSS_CHANCE: f32 = 0.1;
pub const LONG_JUMP_COOLDOWN: (i32, i32) = (600, 1200);
pub const RAM_COOLDOWN: (i32, i32) = (600, 6000);
pub const EXTRA_FALL_DAMAGE_REDUCTION: i32 = 10;
pub const HORN: &str = "minecraft:goat_horn";
pub const INSTRUMENT: &str = "minecraft:instrument";
pub const REGULAR_HORNS: [&str; 4] = [
    "minecraft:ponder_goat_horn",
    "minecraft:sing_goat_horn",
    "minecraft:seek_goat_horn",
    "minecraft:feel_goat_horn",
];
pub const SCREAMING_HORNS: [&str; 4] = [
    "minecraft:admire_goat_horn",
    "minecraft:call_goat_horn",
    "minecraft:yearn_goat_horn",
    "minecraft:dream_goat_horn",
];
pub const KEYS: &[&str] = &["IsScreamingGoat", "HasLeftHorn", "HasRightHorn"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct GoatState {
    pub pending_memories: Option<(i32, i32)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Horn {
    Left,
    Right,
}

pub fn define(base: &mut EntityBase) -> GoatState {
    base.data
        .define(DATA_IS_SCREAMING, SyncedValue::Boolean(false));
    base.data
        .define(DATA_HAS_LEFT_HORN, SyncedValue::Boolean(true));
    base.data
        .define(DATA_HAS_RIGHT_HORN, SyncedValue::Boolean(true));
    GoatState::default()
}

pub fn is_screaming(base: &EntityBase) -> bool {
    base.data.boolean(DATA_IS_SCREAMING)
}

pub fn set_screaming(base: &mut EntityBase, screaming: bool) {
    base.data
        .set(DATA_IS_SCREAMING, SyncedValue::Boolean(screaming));
}

pub fn has_left_horn(base: &EntityBase) -> bool {
    base.data.boolean(DATA_HAS_LEFT_HORN)
}

pub fn has_right_horn(base: &EntityBase) -> bool {
    base.data.boolean(DATA_HAS_RIGHT_HORN)
}

pub fn set_horn(base: &mut EntityBase, horn: Horn, present: bool) {
    let index = match horn {
        Horn::Left => DATA_HAS_LEFT_HORN,
        Horn::Right => DATA_HAS_RIGHT_HORN,
    };
    base.data.set(index, SyncedValue::Boolean(present));
}

pub fn age_boundary_reached(attributes: &mut LivingAttributes, baby: bool) {
    let damage = if baby {
        BABY_ATTACK_DAMAGE
    } else {
        ADULT_ATTACK_DAMAGE
    };
    attributes.set_base(ATTACK_DAMAGE, damage);
}

pub fn sample_memories(random: &mut impl RandomSource) -> (i32, i32) {
    let long_jump = random.next_int_between(LONG_JUMP_COOLDOWN.0, LONG_JUMP_COOLDOWN.1);
    let ram = random.next_int_between(RAM_COOLDOWN.0, RAM_COOLDOWN.1);
    (long_jump, ram)
}

pub fn offspring_screaming(
    own_screaming: bool,
    partner_screaming: bool,
    random: &mut impl RandomSource,
) -> bool {
    let inherited = if random.next_boolean() {
        own_screaming
    } else {
        partner_screaming
    };
    inherited || random.next_double() < SCREAMING_CHANCE
}

pub fn drop_horn(base: &mut EntityBase, baby: bool, random: &mut impl RandomSource) -> bool {
    if baby {
        return false;
    }
    let (left, right) = (has_left_horn(base), has_right_horn(base));
    let horn = match (left, right) {
        (false, false) => return false,
        (false, true) => Horn::Right,
        (true, false) => Horn::Left,
        (true, true) => {
            if random.next_boolean() {
                Horn::Left
            } else {
                Horn::Right
            }
        }
    };
    set_horn(base, horn, false);
    true
}

pub fn horn_instrument(uuid: Uuid, screaming: bool) -> &'static str {
    let (most, least) = uuid.as_u64_pair();
    let both = most ^ least;
    let hash = ((both >> 32) as u32 ^ both as u32) as i32;
    let mut random = LegacyRandom::new(i64::from(hash));
    let horns = if screaming {
        &SCREAMING_HORNS
    } else {
        &REGULAR_HORNS
    };
    horns[random.next_int(horns.len() as i32) as usize]
}

pub fn read(nbt: &Compound, base: &mut EntityBase, _state: &mut GoatState) {
    set_screaming(base, nbt.get_bool("IsScreamingGoat").unwrap_or(false));
    set_horn(
        base,
        Horn::Left,
        nbt.get_bool("HasLeftHorn").unwrap_or(true),
    );
    set_horn(
        base,
        Horn::Right,
        nbt.get_bool("HasRightHorn").unwrap_or(true),
    );
}

pub fn write(base: &EntityBase, _state: &GoatState, out: &mut Compound) {
    out.insert("IsScreamingGoat", i8::from(is_screaming(base)));
    out.insert("HasLeftHorn", i8::from(has_left_horn(base)));
    out.insert("HasRightHorn", i8::from(has_right_horn(base)));
}

pub fn finalize(
    base: &mut EntityBase,
    living: &mut LivingState,
    _environment: &SpawnEnvironment,
    group: Option<SpawnGroupData>,
    level: &mut impl RandomSource,
) -> Option<SpawnGroupData> {
    let memories = sample_memories(level);
    let screaming = level.next_double() < SCREAMING_CHANCE;
    set_screaming(base, screaming);
    let baby = living
        .mob
        .as_ref()
        .and_then(|mob| mob.ageable.as_ref())
        .is_some_and(|ageable| crate::entity::ageable::is_baby(base, ageable));
    age_boundary_reached(&mut living.attributes, baby);
    if !baby && level.next_float() < HORN_LOSS_CHANCE {
        let horn = if level.next_boolean() {
            Horn::Left
        } else {
            Horn::Right
        };
        set_horn(base, horn, false);
    }
    if let Some(crate::entity::species::Species::Goat(state)) =
        living.mob.as_mut().map(|mob| &mut mob.species)
    {
        state.pending_memories = Some(memories);
    }
    ageable_finalize(base, living, group, level)
}

#[cfg(test)]
mod tests {
    use rspigot_entities::EntityKind;

    use super::*;
    use crate::EntityId;
    use crate::random::LegacyRandom;

    fn goat() -> EntityBase {
        let mut base = EntityBase::new(EntityId(1), Uuid::from_u128(7), EntityKind::Goat);
        let _ = define(&mut base);
        base
    }

    #[test]
    fn a_goat_starts_with_both_horns_and_a_regular_voice() {
        let base = goat();
        assert!(has_left_horn(&base) && has_right_horn(&base));
        assert!(!is_screaming(&base));
    }

    #[test]
    fn dropping_a_horn_removes_the_only_remaining_one_then_nothing_is_left() {
        let mut base = goat();
        let mut random = LegacyRandom::new(3);
        assert!(drop_horn(&mut base, false, &mut random));
        assert!(has_left_horn(&base) != has_right_horn(&base));
        assert!(drop_horn(&mut base, false, &mut random));
        assert!(!has_left_horn(&base) && !has_right_horn(&base));
        assert!(!drop_horn(&mut base, false, &mut random));
    }

    #[test]
    fn a_baby_goat_never_drops_a_horn() {
        let mut base = goat();
        assert!(!drop_horn(&mut base, true, &mut LegacyRandom::new(1)));
        assert!(has_left_horn(&base) && has_right_horn(&base));
    }

    #[test]
    fn the_horn_instrument_follows_the_voice_of_the_goat_and_is_stable_per_uuid() {
        let id = Uuid::from_u128(0x1234_5678_9abc_def0_1357_9bdf_0246_8ace);
        let regular = horn_instrument(id, false);
        let screaming = horn_instrument(id, true);
        assert!(REGULAR_HORNS.contains(&regular));
        assert!(SCREAMING_HORNS.contains(&screaming));
        assert_eq!(regular, horn_instrument(id, false));
    }

    #[test]
    fn offspring_inherit_the_screaming_voice_or_roll_two_percent() {
        let mut random = LegacyRandom::new(11);
        let all: Vec<bool> = (0..500)
            .map(|_| offspring_screaming(true, true, &mut random))
            .collect();
        assert!(all.iter().all(|screaming| *screaming));
        let none = (0..2000)
            .filter(|_| offspring_screaming(false, false, &mut random))
            .count();
        assert!(none > 0 && none < 100);
    }

    #[test]
    fn the_attack_damage_follows_the_age() {
        let mut base = goat();
        let mut living = LivingState::new(&mut base, 0.0);
        age_boundary_reached(&mut living.attributes, true);
        assert_eq!(living.attribute(ATTACK_DAMAGE), 1.0);
        age_boundary_reached(&mut living.attributes, false);
        assert_eq!(living.attribute(ATTACK_DAMAGE), 2.0);
    }
}
