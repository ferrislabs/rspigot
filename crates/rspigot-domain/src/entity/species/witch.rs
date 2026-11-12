use rspigot_nbt::Compound;

use super::raider::{self, RaiderState};
use crate::damage::DamageType;
use crate::entity::base::EntityBase;
use crate::entity::data::SyncedValue;
use crate::entity::living::LivingState;
use crate::entity::spawn::mob_finalize;
use crate::potion;
use crate::random::RandomSource;
use crate::spawn::{SpawnGroupData, SpawnReason};

pub const DATA_USING_ITEM: u8 = 17;
pub const KEYS: &[&str] = raider::KEYS;
pub const DRINK_MODIFIER: &str = "minecraft:drinking";
pub const DRINK_SPEED_PENALTY: f64 = -0.25;
pub const DRINK_SOUND: &str = "minecraft:entity.witch.drink";
pub const THROW_SOUND: &str = "minecraft:entity.witch.throw";
pub const CELEBRATE_SOUND: &str = "minecraft:entity.witch.celebrate";
pub const PARTICLE_EVENT: i8 = 15;
pub const PARTICLE_ODDS: f32 = 0.000_75;
pub const WATER_BREATHING_ODDS: f32 = 0.15;
pub const FIRE_RESISTANCE_ODDS: f32 = 0.15;
pub const HEALING_ODDS: f32 = 0.05;
pub const SWIFTNESS_ODDS: f32 = 0.5;
pub const SWIFTNESS_DISTANCE_SQR: f64 = 121.0;
pub const SLOWNESS_DISTANCE: f64 = 8.0;
pub const POISON_HEALTH: f32 = 8.0;
pub const WEAKNESS_DISTANCE: f64 = 3.0;
pub const WEAKNESS_ODDS: f32 = 0.25;
pub const RAIDER_HEAL_HEALTH: f32 = 4.0;
pub const FAR_POWER_DISTANCE: f64 = 2.0;
pub const FAR_POWER: f32 = 0.75;
pub const NEAR_POWER: f32 = 0.45;
pub const THROW_UNCERTAINTY: f32 = 8.0;
pub const THROW_LIFT: f64 = 0.2;
pub const TARGET_EYE_OFFSET: f64 = 1.100_000_023_841_858;
pub const THROW_PITCH_BASE: f32 = 0.8;
pub const THROW_PITCH_SPREAD: f32 = 0.4;
pub const DRINK_PITCH_BASE: f32 = 0.8;
pub const DRINK_PITCH_SPREAD: f32 = 0.4;
pub const FIRE_MEMORY_TICKS: i64 = 40;
pub const DRINK_USE_TICKS: i32 = 32;
pub const RESISTANCE_FACTOR: f32 = 0.15;
pub const RESISTANT_TAG: &str = "minecraft:witch_resistant_to";
pub const HEAL_RAIDER_COOLDOWN: i32 = 200;
pub const HEAL_RAIDER_INTERVAL: i32 = 500;
pub const PLAYER_INTERVAL: i32 = 10;
pub const PARTICLES_MIN: i32 = 10;
pub const PARTICLES_SPREAD: i32 = 35;
pub const PARTICLE_SCATTER: f64 = 0.129_999_995_231_628_42;
pub const PARTICLE_RISE: f64 = 0.5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WitchState {
    pub raider: RaiderState,
    pub using_time: i32,
}

pub fn define(base: &mut EntityBase) -> WitchState {
    raider::define(base);
    base.data
        .define(DATA_USING_ITEM, SyncedValue::Boolean(false));
    WitchState::default()
}

pub fn is_drinking_potion(base: &EntityBase) -> bool {
    base.data.boolean(DATA_USING_ITEM)
}

pub fn set_using_item(base: &mut EntityBase, using: bool) {
    base.data.set(DATA_USING_ITEM, SyncedValue::Boolean(using));
}

pub fn read(nbt: &Compound, _base: &mut EntityBase, state: &mut WitchState) {
    raider::read(nbt, &mut state.raider);
}

pub fn write(_base: &EntityBase, state: &WitchState, out: &mut Compound) {
    raider::write(&state.raider, out);
}

pub fn finalize(
    base: &mut EntityBase,
    living: &mut LivingState,
    reason: SpawnReason,
    group: Option<SpawnGroupData>,
    level: &mut impl RandomSource,
) -> Option<SpawnGroupData> {
    if let Some(crate::entity::species::Species::Witch(state)) =
        living.mob.as_mut().map(|mob| &mut mob.species)
    {
        let joins_raids = reason != SpawnReason::Natural;
        state.raider.finalize(reason, false, joins_raids, level);
    }
    mob_finalize(base, living, group, level)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DrinkFacts {
    pub eye_in_water: bool,
    pub has_water_breathing: bool,
    pub on_fire: bool,
    pub recent_fire_damage: bool,
    pub has_fire_resistance: bool,
    pub health: f32,
    pub max_health: f32,
    pub target_distance_sqr: Option<f64>,
    pub has_speed: bool,
}

pub fn recent_fire_damage(last: Option<(DamageType, i64)>, game_time: i64) -> bool {
    last.is_some_and(|(kind, at)| {
        game_time - at <= FIRE_MEMORY_TICKS && kind.is(crate::damage::DamageTag::IsFire)
    })
}

pub fn choose_drink(facts: &DrinkFacts, random: &mut impl RandomSource) -> Option<&'static str> {
    if random.next_float() < WATER_BREATHING_ODDS
        && facts.eye_in_water
        && !facts.has_water_breathing
    {
        return Some(potion::WATER_BREATHING);
    }
    if random.next_float() < FIRE_RESISTANCE_ODDS
        && (facts.on_fire || facts.recent_fire_damage)
        && !facts.has_fire_resistance
    {
        return Some(potion::FIRE_RESISTANCE);
    }
    if random.next_float() < HEALING_ODDS && facts.health < facts.max_health {
        return Some(potion::HEALING);
    }
    if random.next_float() < SWIFTNESS_ODDS
        && !facts.has_speed
        && facts
            .target_distance_sqr
            .is_some_and(|distance| distance > SWIFTNESS_DISTANCE_SQR)
    {
        return Some(potion::SWIFTNESS);
    }
    None
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ThrowFacts {
    pub target_is_raider: bool,
    pub target_health: f32,
    pub target_has_slowness: bool,
    pub target_has_poison: bool,
    pub target_has_weakness: bool,
    pub distance: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Throw {
    pub potion: &'static str,
    pub clear_target: bool,
}

pub fn choose_throw(facts: &ThrowFacts, random: &mut impl RandomSource) -> Throw {
    if facts.target_is_raider {
        let potion = if facts.target_health <= RAIDER_HEAL_HEALTH {
            potion::HEALING
        } else {
            potion::REGENERATION
        };
        return Throw {
            potion,
            clear_target: true,
        };
    }
    let potion = if facts.distance >= SLOWNESS_DISTANCE && !facts.target_has_slowness {
        potion::SLOWNESS
    } else if facts.target_health >= POISON_HEALTH && !facts.target_has_poison {
        potion::POISON
    } else if facts.distance <= WEAKNESS_DISTANCE
        && !facts.target_has_weakness
        && random.next_float() < WEAKNESS_ODDS
    {
        potion::WEAKNESS
    } else {
        potion::HARMING
    };
    Throw {
        potion,
        clear_target: false,
    }
}

pub fn throw_power(distance: f64) -> f32 {
    if distance > FAR_POWER_DISTANCE {
        FAR_POWER
    } else {
        NEAR_POWER
    }
}

pub fn throw_aim(delta: (f64, f64, f64)) -> (crate::Vec3, f64) {
    let (dx, dy, dz) = delta;
    let distance = (dx * dx + dz * dz).sqrt();
    (
        crate::Vec3::new(dx, dy + distance * THROW_LIFT, dz),
        distance,
    )
}

pub fn magic_absorb(amount: f32, kind: DamageType, self_inflicted: bool) -> f32 {
    let mut amount = amount;
    if self_inflicted {
        amount = 0.0;
    }
    if kind.in_tag(RESISTANT_TAG) {
        amount *= RESISTANCE_FACTOR;
    }
    amount
}

#[cfg(test)]
mod tests {
    use rspigot_entities::EntityKind;
    use uuid::Uuid;

    use super::*;
    use crate::EntityId;
    use crate::entity::save::{load_entity, save_entity};
    use crate::entity::spawn::SpawnEnvironment;
    use crate::random::LegacyRandom;

    struct Rolls(Vec<f32>);

    impl RandomSource for Rolls {
        fn next_float(&mut self) -> f32 {
            self.0.remove(0)
        }

        fn next_int(&mut self, _: i32) -> i32 {
            0
        }

        fn next_double(&mut self) -> f64 {
            0.0
        }

        fn next_boolean(&mut self) -> bool {
            false
        }

        fn next_gaussian(&mut self) -> f64 {
            0.0
        }
    }

    fn calm() -> DrinkFacts {
        DrinkFacts {
            eye_in_water: false,
            has_water_breathing: false,
            on_fire: false,
            recent_fire_damage: false,
            has_fire_resistance: false,
            health: 26.0,
            max_health: 26.0,
            target_distance_sqr: None,
            has_speed: false,
        }
    }

    #[test]
    fn nothing_is_drunk_when_no_condition_holds_and_every_roll_is_spent() {
        let mut random = Rolls(vec![0.0; 4]);
        assert_eq!(choose_drink(&calm(), &mut random), None);
        assert!(random.0.is_empty());
    }

    #[test]
    fn a_drowning_witch_drinks_water_breathing_on_a_low_first_roll() {
        let facts = DrinkFacts {
            eye_in_water: true,
            ..calm()
        };
        let mut random = Rolls(vec![0.149]);
        assert_eq!(
            choose_drink(&facts, &mut random),
            Some(potion::WATER_BREATHING)
        );
        assert!(random.0.is_empty());
        let mut random = Rolls(vec![0.15, 0.9, 0.9, 0.9]);
        assert_eq!(choose_drink(&facts, &mut random), None);
        let breathing = DrinkFacts {
            has_water_breathing: true,
            ..facts
        };
        let mut random = Rolls(vec![0.0, 0.9, 0.9, 0.9]);
        assert_eq!(choose_drink(&breathing, &mut random), None);
    }

    #[test]
    fn a_burning_witch_drinks_fire_resistance_after_the_second_roll() {
        let facts = DrinkFacts {
            on_fire: true,
            ..calm()
        };
        let mut random = Rolls(vec![0.9, 0.1]);
        assert_eq!(
            choose_drink(&facts, &mut random),
            Some(potion::FIRE_RESISTANCE)
        );
        let resistant = DrinkFacts {
            has_fire_resistance: true,
            ..facts
        };
        let mut random = Rolls(vec![0.9, 0.1, 0.9, 0.9]);
        assert_eq!(choose_drink(&resistant, &mut random), None);
        let scorched = DrinkFacts {
            recent_fire_damage: true,
            ..calm()
        };
        let mut random = Rolls(vec![0.9, 0.1]);
        assert_eq!(
            choose_drink(&scorched, &mut random),
            Some(potion::FIRE_RESISTANCE)
        );
    }

    #[test]
    fn a_hurt_witch_drinks_healing_one_time_in_twenty() {
        let facts = DrinkFacts {
            health: 25.0,
            ..calm()
        };
        let mut random = Rolls(vec![0.9, 0.9, 0.04]);
        assert_eq!(choose_drink(&facts, &mut random), Some(potion::HEALING));
        let mut random = Rolls(vec![0.9, 0.9, 0.05, 0.9]);
        assert_eq!(choose_drink(&facts, &mut random), None);
        let healthy = calm();
        let mut random = Rolls(vec![0.9, 0.9, 0.0, 0.9]);
        assert_eq!(choose_drink(&healthy, &mut random), None);
    }

    #[test]
    fn a_witch_far_from_its_target_drinks_swiftness_beyond_eleven_blocks() {
        let far = DrinkFacts {
            target_distance_sqr: Some(121.5),
            ..calm()
        };
        let mut random = Rolls(vec![0.9, 0.9, 0.9, 0.49]);
        assert_eq!(choose_drink(&far, &mut random), Some(potion::SWIFTNESS));
        let mut random = Rolls(vec![0.9, 0.9, 0.9, 0.5]);
        assert_eq!(choose_drink(&far, &mut random), None);
        let edge = DrinkFacts {
            target_distance_sqr: Some(121.0),
            ..calm()
        };
        let mut random = Rolls(vec![0.9, 0.9, 0.9, 0.0]);
        assert_eq!(choose_drink(&edge, &mut random), None);
        let fast = DrinkFacts {
            has_speed: true,
            ..far
        };
        let mut random = Rolls(vec![0.9, 0.9, 0.9, 0.0]);
        assert_eq!(choose_drink(&fast, &mut random), None);
    }

    #[test]
    fn the_first_matching_branch_wins_without_rolling_the_later_ones() {
        let facts = DrinkFacts {
            eye_in_water: true,
            on_fire: true,
            health: 1.0,
            target_distance_sqr: Some(400.0),
            ..calm()
        };
        let mut random = Rolls(vec![0.0, 0.0]);
        assert_eq!(
            choose_drink(&facts, &mut random),
            Some(potion::WATER_BREATHING)
        );
        assert_eq!(random.0, vec![0.0]);
    }

    fn enemy() -> ThrowFacts {
        ThrowFacts {
            target_is_raider: false,
            target_health: 20.0,
            target_has_slowness: false,
            target_has_poison: false,
            target_has_weakness: false,
            distance: 5.0,
        }
    }

    #[test]
    fn a_far_enemy_is_slowed_then_poisoned_then_harmed() {
        let mut random = Rolls(vec![]);
        let far = ThrowFacts {
            distance: 8.0,
            ..enemy()
        };
        assert_eq!(choose_throw(&far, &mut random).potion, potion::SLOWNESS);
        let slowed = ThrowFacts {
            target_has_slowness: true,
            ..far
        };
        assert_eq!(choose_throw(&slowed, &mut random).potion, potion::POISON);
        let poisoned = ThrowFacts {
            target_has_poison: true,
            ..slowed
        };
        assert_eq!(choose_throw(&poisoned, &mut random).potion, potion::HARMING);
        let frail = ThrowFacts {
            target_health: 7.5,
            ..slowed
        };
        assert_eq!(choose_throw(&frail, &mut random).potion, potion::HARMING);
        let near = ThrowFacts {
            distance: 7.9,
            ..enemy()
        };
        assert_eq!(choose_throw(&near, &mut random).potion, potion::POISON);
    }

    #[test]
    fn a_close_enemy_may_be_weakened_with_a_quarter_chance() {
        let close = ThrowFacts {
            distance: 3.0,
            target_has_poison: true,
            ..enemy()
        };
        let mut random = Rolls(vec![0.24]);
        assert_eq!(choose_throw(&close, &mut random).potion, potion::WEAKNESS);
        let mut random = Rolls(vec![0.25]);
        assert_eq!(choose_throw(&close, &mut random).potion, potion::HARMING);
        let weakened = ThrowFacts {
            target_has_weakness: true,
            ..close
        };
        let mut random = Rolls(vec![]);
        assert_eq!(choose_throw(&weakened, &mut random).potion, potion::HARMING);
        let mid = ThrowFacts {
            distance: 3.1,
            ..close
        };
        assert_eq!(choose_throw(&mid, &mut random).potion, potion::HARMING);
    }

    #[test]
    fn a_raider_target_is_healed_and_the_witch_forgets_it() {
        let mut random = Rolls(vec![]);
        let hurt = ThrowFacts {
            target_is_raider: true,
            target_health: 4.0,
            ..enemy()
        };
        assert_eq!(
            choose_throw(&hurt, &mut random),
            Throw {
                potion: potion::HEALING,
                clear_target: true
            }
        );
        let fine = ThrowFacts {
            target_health: 4.5,
            ..hurt
        };
        assert_eq!(
            choose_throw(&fine, &mut random),
            Throw {
                potion: potion::REGENERATION,
                clear_target: true
            }
        );
    }

    #[test]
    fn the_throw_is_slower_at_two_blocks_and_closer_and_lifts_by_a_fifth_of_the_range() {
        assert_eq!(throw_power(2.0), 0.45);
        assert_eq!(throw_power(2.01), 0.75);
        let (aim, distance) = throw_aim((3.0, -1.0, 4.0));
        assert_eq!(distance, 5.0);
        assert_eq!(aim, crate::Vec3::new(3.0, -1.0 + 5.0 * 0.2, 4.0));
    }

    #[test]
    fn magic_is_cut_to_fifteen_percent_and_self_damage_is_absorbed() {
        assert_eq!(magic_absorb(10.0, DamageType::Magic, false), 10.0 * 0.15);
        assert_eq!(
            magic_absorb(10.0, DamageType::IndirectMagic, false),
            10.0 * 0.15
        );
        assert_eq!(magic_absorb(10.0, DamageType::MobAttack, false), 10.0);
        assert_eq!(magic_absorb(10.0, DamageType::MobAttack, true), 0.0);
        assert_eq!(magic_absorb(10.0, DamageType::Magic, true), 0.0);
    }

    #[test]
    fn recent_fire_damage_is_remembered_for_forty_ticks() {
        assert!(recent_fire_damage(Some((DamageType::OnFire, 100)), 140));
        assert!(!recent_fire_damage(Some((DamageType::OnFire, 100)), 141));
        assert!(!recent_fire_damage(Some((DamageType::MobAttack, 100)), 100));
        assert!(!recent_fire_damage(None, 100));
    }

    fn witch() -> crate::entity::Entity {
        load_entity(
            &Compound::new().with("id", "minecraft:witch"),
            EntityId(1),
            Uuid::from_u128(1),
        )
        .unwrap()
    }

    #[test]
    fn a_witch_defines_the_celebrating_and_drinking_data_slots() {
        let entity = witch();
        assert!(!raider::is_celebrating(&entity.base));
        assert!(!is_drinking_potion(&entity.base));
        assert_eq!(raider::DATA_IS_CELEBRATING, 16);
        assert_eq!(DATA_USING_ITEM, 17);
    }

    #[test]
    fn a_witch_round_trips_its_raider_state_through_nbt() {
        let nbt = Compound::new()
            .with("id", "minecraft:witch")
            .with("Wave", 2)
            .with("CanJoinRaid", true)
            .with("Patrolling", true)
            .with("PatrolLeader", false);
        let entity = load_entity(&nbt, EntityId(1), Uuid::from_u128(1)).unwrap();
        let Some(crate::entity::species::Species::Witch(state)) = entity
            .living()
            .and_then(|living| living.mob.as_ref())
            .map(|mob| mob.species)
        else {
            panic!("a witch has the witch species");
        };
        assert_eq!(state.raider.wave, 2);
        assert!(state.raider.can_join_raid && state.raider.patrolling);
        let saved = save_entity(&entity, None).unwrap();
        assert_eq!(saved.get_i32("Wave"), Some(2));
        assert_eq!(saved.get_bool("Patrolling"), Some(true));
        let again = load_entity(&saved, EntityId(1), Uuid::from_u128(1)).unwrap();
        assert_eq!(save_entity(&again, None), Some(saved));
    }

    #[test]
    fn a_natural_witch_cannot_join_raids_but_a_summoned_one_can() {
        use crate::entity::spawn::finalize_spawn;
        use crate::spawn::SpawnReason;
        let environment = SpawnEnvironment {
            biome: crate::BiomeId(0),
            difficulty: crate::difficulty::DifficultyInstance::new(
                crate::world::Difficulty::Normal,
                0,
                0,
                1.0,
            ),
            halloween: false,
            moon_brightness: 0.0,
        };
        for (reason, joins) in [
            (SpawnReason::Natural, false),
            (SpawnReason::Spawner, true),
            (SpawnReason::Command, true),
        ] {
            let mut entity = witch();
            let mut level = LegacyRandom::new(2);
            let mut own = LegacyRandom::new(3);
            finalize_spawn(
                &mut entity,
                &environment,
                reason,
                None,
                &mut level,
                &mut own,
            );
            let Some(crate::entity::species::Species::Witch(state)) = entity
                .living()
                .and_then(|living| living.mob.as_ref())
                .map(|mob| mob.species)
            else {
                panic!("a witch");
            };
            assert_eq!(state.raider.can_join_raid, joins, "{reason:?}");
            assert!(!state.raider.patrol_leader);
        }
        assert_eq!(EntityKind::Witch.name(), "minecraft:witch");
    }
}
