use rspigot_data::variants::VariantFamily;
use rspigot_nbt::Compound;

use crate::entity::base::EntityBase;
use crate::entity::data::SyncedValue;
use crate::entity::living::{LivingState, MAX_HEALTH};
use crate::entity::spawn::{SpawnEnvironment, ageable_finalize};
use crate::entity::species::{pick_sound_variant, select_variant, set_sound_variant, set_variant};
use crate::entity::variants::{
    DEFAULT_COLLAR_COLOR, VariantContext, WOLF_DATA_COLLAR_COLOR, WOLF_DATA_INTERESTED,
};
use crate::random::RandomSource;
use crate::spawn::SpawnGroupData;

pub const KEYS: &[&str] = &["CollarColor"];
pub const SHAKE_STEP: f32 = 0.05;
pub const SHAKE_SPLASH_START: f32 = 0.4;
pub const SHAKE_END: f32 = 2.0;
pub const SHAKE_START_EVENT: i8 = 8;
pub const SHAKE_CANCEL_EVENT: i8 = 56;
pub const WOLF_FAMILY: VariantFamily = VariantFamily::Wolf;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct WolfState {
    pub wet: bool,
    pub shaking: bool,
    pub shake_anim: f32,
    pub shake_anim_old: f32,
}

impl Eq for WolfState {}

impl WolfState {
    pub fn start_shaking(&mut self) {
        self.shaking = true;
        self.shake_anim = 0.0;
        self.shake_anim_old = 0.0;
    }

    pub fn cancel_shake(&mut self) {
        self.shaking = false;
        self.shake_anim = 0.0;
        self.shake_anim_old = 0.0;
    }

    pub fn die(&mut self) {
        self.wet = false;
        self.cancel_shake();
    }

    pub fn advance_shake(&mut self) -> ShakeStep {
        let first = self.shake_anim == 0.0;
        self.shake_anim_old = self.shake_anim;
        self.shake_anim += SHAKE_STEP;
        if self.shake_anim_old >= SHAKE_END {
            self.wet = false;
            self.shaking = false;
            self.shake_anim_old = 0.0;
            self.shake_anim = 0.0;
        }
        let splashes = if self.shake_anim > SHAKE_SPLASH_START {
            let phase = (self.shake_anim - SHAKE_SPLASH_START) * std::f32::consts::PI;
            (crate::mth::sin(f64::from(phase)) * 7.0) as i32
        } else {
            0
        };
        ShakeStep { first, splashes }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShakeStep {
    pub first: bool,
    pub splashes: i32,
}

pub fn define(base: &mut EntityBase) -> WolfState {
    base.data
        .define(WOLF_DATA_INTERESTED, SyncedValue::Boolean(false));
    base.data.define(
        WOLF_DATA_COLLAR_COLOR,
        SyncedValue::Int(DEFAULT_COLLAR_COLOR),
    );
    WolfState::default()
}

pub const START_HEALTH: f64 = 8.0;
pub const TAME_HEALTH: f64 = 40.0;
pub const BODY_ARMOR_MODIFIER: &str = "minecraft:armor.body";

pub fn apply_taming_side_effects(base: &mut EntityBase, living: &mut LivingState) {
    if crate::entity::tamable::is_tame(base) {
        living.attributes.set_base(MAX_HEALTH, TAME_HEALTH);
        let max = living.max_health() as f32;
        crate::entity::living::set_health(base, living, max);
    } else {
        living.attributes.set_base(MAX_HEALTH, START_HEALTH);
        let health = crate::entity::living::health(base);
        crate::entity::living::set_health(base, living, health);
    }
}

pub fn is_interested(base: &EntityBase) -> bool {
    base.data.boolean(WOLF_DATA_INTERESTED)
}

pub fn set_interested(base: &mut EntityBase, interested: bool) {
    base.data
        .set(WOLF_DATA_INTERESTED, SyncedValue::Boolean(interested));
}

pub fn collar_color(base: &EntityBase) -> u8 {
    crate::entity::species::dye_by_id(base.data.int(WOLF_DATA_COLLAR_COLOR))
}

pub fn set_collar_color(base: &mut EntityBase, color: u8) {
    base.data
        .set(WOLF_DATA_COLLAR_COLOR, SyncedValue::Int(i32::from(color)));
}

pub fn read(nbt: &Compound, base: &mut EntityBase, _state: &mut WolfState) {
    let color = nbt
        .get_i64("CollarColor")
        .map_or(DEFAULT_COLLAR_COLOR, |id| id as i32);
    base.data
        .set(WOLF_DATA_COLLAR_COLOR, SyncedValue::Int(color));
    let sanitized = crate::entity::species::dye_by_id(color);
    set_collar_color(base, sanitized);
}

pub fn write(base: &EntityBase, _state: &WolfState, out: &mut Compound) {
    out.insert("CollarColor", collar_color(base) as i8);
}

pub fn finalize(
    base: &mut EntityBase,
    living: &mut LivingState,
    environment: &SpawnEnvironment,
    group: Option<SpawnGroupData>,
    level: &mut impl RandomSource,
) -> Option<SpawnGroupData> {
    let mut pack = None;
    let mut inherited = group;
    if let Some(SpawnGroupData::WolfPack {
        variant,
        group_size,
    }) = group
    {
        set_variant(base, WOLF_FAMILY, variant);
        pack = Some(variant);
        inherited = Some(SpawnGroupData::Ageable {
            group_size,
            should_spawn_baby: false,
            baby_chance: crate::entity::ageable::DEFAULT_BABY_CHANCE,
        });
    } else if let Some(variant) = select_variant(
        WOLF_FAMILY,
        &VariantContext::in_biome(environment.biome),
        level,
    ) {
        set_variant(base, WOLF_FAMILY, variant);
        pack = Some(variant);
        inherited = Some(SpawnGroupData::Ageable {
            group_size: 0,
            should_spawn_baby: false,
            baby_chance: crate::entity::ageable::DEFAULT_BABY_CHANCE,
        });
    }
    let sound = pick_sound_variant(WOLF_FAMILY, level);
    set_sound_variant(base, WOLF_FAMILY, sound);
    let result = ageable_finalize(base, living, inherited, level);
    match (pack, result) {
        (Some(variant), Some(SpawnGroupData::Ageable { group_size, .. })) => {
            Some(SpawnGroupData::WolfPack {
                variant,
                group_size,
            })
        }
        (_, result) => result,
    }
}

#[cfg(test)]
mod tests {
    use rspigot_entities::EntityKind;
    use uuid::Uuid;

    use super::*;
    use crate::EntityId;
    use crate::entity::species::{sound_variant, variant};
    use crate::random::LegacyRandom;

    fn wolf() -> (EntityBase, LivingState) {
        let mut base = EntityBase::new(EntityId(1), Uuid::from_u128(1), EntityKind::Wolf);
        let living = LivingState::new(&mut base, 0.0);
        (base, living)
    }

    fn environment() -> SpawnEnvironment {
        SpawnEnvironment {
            biome: crate::BiomeId(0),
            difficulty: crate::difficulty::DifficultyInstance::new(
                crate::world::Difficulty::Normal,
                0,
                0,
                0.0,
            ),
            halloween: false,
            moon_brightness: 0.0,
        }
    }

    #[test]
    fn a_wolf_defines_interest_and_collar_after_the_tamable_data() {
        let (base, _) = wolf();
        assert!(!is_interested(&base));
        assert_eq!(base.data.int(WOLF_DATA_COLLAR_COLOR), 14);
        assert_eq!(collar_color(&base), 14);
        assert_eq!(WOLF_DATA_INTERESTED, 20);
        assert_eq!(WOLF_DATA_COLLAR_COLOR, 21);
    }

    #[test]
    fn the_collar_round_trips_through_nbt_as_a_byte() {
        let (mut base, mut living) = wolf();
        let mut state = WolfState::default();
        read(
            &Compound::new().with("CollarColor", 3i8),
            &mut base,
            &mut state,
        );
        assert_eq!(collar_color(&base), 3);
        let mut out = Compound::new();
        write(&base, &state, &mut out);
        assert_eq!(out.get("CollarColor"), Some(&rspigot_nbt::Tag::Byte(3)));
        read(&Compound::new(), &mut base, &mut state);
        assert_eq!(collar_color(&base), 14);
        read(
            &Compound::new().with("CollarColor", 99i8),
            &mut base,
            &mut state,
        );
        assert_eq!(collar_color(&base), 0);
        let _ = &mut living;
    }

    #[test]
    fn shaking_lasts_forty_one_steps_and_dries_the_wolf() {
        let mut state = WolfState {
            wet: true,
            ..WolfState::default()
        };
        state.start_shaking();
        let first = state.advance_shake();
        assert!(first.first);
        assert_eq!(first.splashes, 0);
        let mut steps = 1;
        while state.shaking {
            let step = state.advance_shake();
            assert!(!step.first);
            steps += 1;
            assert!(steps < 100);
        }
        assert!(!state.wet);
        assert_eq!(state.shake_anim, 0.0);
        assert!(steps > 40);
    }

    #[test]
    fn splashes_follow_the_sine_of_the_shake_after_point_four() {
        let mut state = WolfState {
            shaking: true,
            shake_anim: 0.8,
            ..WolfState::default()
        };
        let step = state.advance_shake();
        assert!((state.shake_anim - 0.85).abs() < 1e-6);
        assert!((5..=7).contains(&step.splashes), "{}", step.splashes);
    }

    #[test]
    fn taming_raises_the_health_to_forty_and_untaming_lowers_it() {
        let (mut base, mut living) = wolf();
        crate::entity::tamable::set_tame(&mut base, true);
        apply_taming_side_effects(&mut base, &mut living);
        assert_eq!(living.max_health(), 40.0);
        assert_eq!(crate::entity::living::health(&base), 40.0);
        crate::entity::tamable::set_tame(&mut base, false);
        apply_taming_side_effects(&mut base, &mut living);
        assert_eq!(living.max_health(), 8.0);
        assert_eq!(crate::entity::living::health(&base), 8.0);
    }

    #[test]
    fn dying_clears_the_wetness() {
        let mut state = WolfState {
            wet: true,
            shaking: true,
            shake_anim: 1.0,
            shake_anim_old: 0.9,
        };
        state.die();
        assert_eq!(state, WolfState::default());
    }

    #[test]
    fn a_wolf_pack_shares_its_variant_and_never_spawns_babies() {
        let mut level = LegacyRandom::new(11);
        let (mut first, mut first_living) = wolf();
        let mut group = finalize(
            &mut first,
            &mut first_living,
            &environment(),
            None,
            &mut level,
        );
        let Some(SpawnGroupData::WolfPack {
            variant: chosen,
            group_size,
        }) = group
        else {
            panic!("a pack was started: {group:?}");
        };
        assert_eq!(group_size, 1);
        assert_eq!(variant(&first, WOLF_FAMILY), chosen);
        for expected in 2..12 {
            let (mut next, mut next_living) = wolf();
            group = finalize(
                &mut next,
                &mut next_living,
                &environment(),
                group,
                &mut level,
            );
            assert_eq!(variant(&next, WOLF_FAMILY), chosen);
            assert!(!crate::entity::ageable::is_baby(
                &next,
                next_living
                    .mob
                    .as_ref()
                    .and_then(|mob| mob.ageable.as_ref())
                    .unwrap()
            ));
            let Some(SpawnGroupData::WolfPack { group_size, .. }) = group else {
                panic!("the pack goes on");
            };
            assert_eq!(group_size, expected);
        }
    }

    #[test]
    fn spawning_picks_a_sound_variant_for_every_wolf() {
        let mut level = LegacyRandom::new(5);
        let mut seen = std::collections::HashSet::new();
        for _ in 0..200 {
            let (mut base, mut living) = wolf();
            finalize(&mut base, &mut living, &environment(), None, &mut level);
            seen.insert(sound_variant(&base, WOLF_FAMILY));
        }
        assert_eq!(seen.len(), WOLF_FAMILY.sound_variants().len());
    }
}
