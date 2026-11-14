use std::borrow::Cow;

use rspigot_items::ItemKind;

use crate::effect::{AttributeModifier, AttributeOperation};
use crate::entity::living::LivingAttributes;
use crate::random::RandomSource;

const MOVEMENT_SPEED: &str = "minecraft:movement_speed";

pub const ATTACKING_MODIFIER: &str = "minecraft:attacking";
pub const ATTACKING_BONUS: f64 = 0.05;
pub const FIRST_ANGER_SOUND_DELAY: (i32, i32) = (0, 20);
pub const ALERT_INTERVAL: (i32, i32) = (80, 120);
pub const ANGER_SOUND_VOLUME_FACTOR: f32 = 2.0;
pub const ANGER_SOUND_PITCH_FACTOR: f32 = 1.8;
pub const ANGER_DURATION: (i32, i32) = (400, 780);
const SPEAR_ODDS: i32 = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ZombifiedPiglinState {
    pub play_first_anger_sound_in: i32,
    pub ticks_until_next_alert: i32,
}

impl ZombifiedPiglinState {
    pub fn acquire_target(&mut self, random: &mut impl RandomSource) {
        self.play_first_anger_sound_in =
            random.next_int_between(FIRST_ANGER_SOUND_DELAY.0, FIRST_ANGER_SOUND_DELAY.1);
        self.ticks_until_next_alert = random.next_int_between(ALERT_INTERVAL.0, ALERT_INTERVAL.1);
    }

    pub fn first_anger_sound_due(&mut self) -> bool {
        if self.play_first_anger_sound_in > 0 {
            self.play_first_anger_sound_in -= 1;
            return self.play_first_anger_sound_in == 0;
        }
        false
    }

    pub fn alert_due(&mut self) -> bool {
        if self.ticks_until_next_alert > 0 {
            self.ticks_until_next_alert -= 1;
            return false;
        }
        true
    }

    pub fn restart_alert(&mut self, random: &mut impl RandomSource) {
        self.ticks_until_next_alert = random.next_int_between(ALERT_INTERVAL.0, ALERT_INTERVAL.1);
    }
}

pub fn update_attacking_speed(attributes: &mut LivingAttributes, angry: bool, baby: bool) {
    let applied = attributes.has_modifier(MOVEMENT_SPEED, ATTACKING_MODIFIER);
    if angry {
        if !baby && !applied {
            attributes.add_modifier(
                MOVEMENT_SPEED,
                AttributeModifier {
                    id: Cow::Borrowed(ATTACKING_MODIFIER),
                    amount: ATTACKING_BONUS,
                    operation: AttributeOperation::AddValue,
                },
            );
        }
    } else if applied {
        attributes.remove_modifier(MOVEMENT_SPEED, ATTACKING_MODIFIER);
    }
}

pub fn sample_anger(random: &mut impl RandomSource) -> i32 {
    random.next_int_between(ANGER_DURATION.0, ANGER_DURATION.1)
}

pub fn default_weapon(random: &mut impl RandomSource) -> ItemKind {
    if random.next_int(SPEAR_ODDS) == 0 {
        ItemKind::GoldenSpear
    } else {
        ItemKind::GoldenSword
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::random::LegacyRandom;

    #[test]
    fn acquiring_a_target_arms_both_timers_in_their_ranges() {
        let mut random = LegacyRandom::new(11);
        for _ in 0..200 {
            let mut state = ZombifiedPiglinState::default();
            state.acquire_target(&mut random);
            assert!((0..=20).contains(&state.play_first_anger_sound_in));
            assert!((80..=120).contains(&state.ticks_until_next_alert));
        }
    }

    #[test]
    fn the_first_anger_sound_fires_once_when_the_delay_runs_out() {
        let mut state = ZombifiedPiglinState {
            play_first_anger_sound_in: 3,
            ticks_until_next_alert: 0,
        };
        assert!(!state.first_anger_sound_due());
        assert!(!state.first_anger_sound_due());
        assert!(state.first_anger_sound_due());
        assert!(!state.first_anger_sound_due());
        state.play_first_anger_sound_in = 0;
        assert!(!state.first_anger_sound_due());
    }

    #[test]
    fn alerts_wait_for_their_countdown_then_rearm() {
        let mut state = ZombifiedPiglinState {
            play_first_anger_sound_in: 0,
            ticks_until_next_alert: 2,
        };
        assert!(!state.alert_due());
        assert!(!state.alert_due());
        assert!(state.alert_due());
        assert!(state.alert_due());
        let mut random = LegacyRandom::new(5);
        state.restart_alert(&mut random);
        assert!((80..=120).contains(&state.ticks_until_next_alert));
        assert!(!state.alert_due());
    }

    fn piglin() -> crate::entity::living::LivingState {
        let mut base = crate::entity::EntityBase::new(
            crate::EntityId(1),
            uuid::Uuid::from_u128(1),
            rspigot_entities::EntityKind::ZombifiedPiglin,
        );
        crate::entity::living::LivingState::new(&mut base, 0.0)
    }

    fn speed(living: &mut crate::entity::living::LivingState) -> f64 {
        living.attribute("minecraft:movement_speed")
    }

    #[test]
    fn an_angry_adult_runs_five_hundredths_faster_and_calms_back_down() {
        let mut living = piglin();
        let calm = speed(&mut living);
        update_attacking_speed(&mut living.attributes, true, false);
        let angry = speed(&mut living);
        assert!((angry - calm - ATTACKING_BONUS).abs() < 1e-9);
        update_attacking_speed(&mut living.attributes, true, false);
        assert_eq!(speed(&mut living), angry);
        update_attacking_speed(&mut living.attributes, false, false);
        assert_eq!(speed(&mut living), calm);
    }

    #[test]
    fn angry_babies_get_no_speed_bonus() {
        let mut living = piglin();
        let calm = speed(&mut living);
        update_attacking_speed(&mut living.attributes, true, true);
        assert_eq!(speed(&mut living), calm);
    }

    #[test]
    fn one_piglin_in_twenty_carries_a_golden_spear() {
        let mut random = LegacyRandom::new(2);
        let spears = (0..4000)
            .filter(|_| default_weapon(&mut random) == ItemKind::GoldenSpear)
            .count();
        assert!((140..=260).contains(&spears), "{spears}");
    }

    fn spawned(seed: i64, reason: crate::spawn::SpawnReason) -> crate::entity::Entity {
        use crate::BiomeId;
        use crate::difficulty::DifficultyInstance;
        use crate::entity::base::EntityBase;
        use crate::entity::living::LivingState;
        use crate::entity::spawn::{SpawnEnvironment, finalize_spawn};
        use crate::entity::{Entity, EntityState};
        use crate::world::Difficulty;
        use rspigot_entities::EntityKind;
        use uuid::Uuid;

        let mut base = EntityBase::new(
            crate::EntityId(1),
            Uuid::from_u128(1),
            EntityKind::ZombifiedPiglin,
        );
        let living = LivingState::new(&mut base, 0.0);
        let mut piglin = Entity::new(base, EntityState::Living(Box::new(living)));
        let environment = SpawnEnvironment {
            biome: BiomeId(0),
            difficulty: DifficultyInstance::new(Difficulty::Hard, 0, 0, 1.0),
            halloween: false,
            moon_brightness: 0.0,
        };
        let mut level = LegacyRandom::new(seed);
        let mut own = LegacyRandom::new(seed + 1);
        finalize_spawn(
            &mut piglin,
            &environment,
            reason,
            None,
            &mut level,
            &mut own,
        );
        piglin
    }

    #[test]
    fn a_spawned_piglin_holds_a_golden_weapon_and_no_armor() {
        use crate::spawn::SpawnReason;
        use rspigot_items::EquipmentSlot;

        for seed in 0..40 {
            let piglin = spawned(seed, SpawnReason::Natural);
            let living = piglin.living().unwrap();
            let weapon = living.equipment(EquipmentSlot::Mainhand).map(|s| s.item);
            assert!(
                weapon == Some(ItemKind::GoldenSword.id())
                    || weapon == Some(ItemKind::GoldenSpear.id()),
                "{weapon:?}"
            );
            for slot in [
                EquipmentSlot::Head,
                EquipmentSlot::Chest,
                EquipmentSlot::Legs,
                EquipmentSlot::Feet,
            ] {
                assert!(living.equipment(slot).is_none());
            }
        }
    }

    #[test]
    fn a_converted_piglin_keeps_its_hands_empty() {
        use crate::spawn::SpawnReason;
        use rspigot_items::EquipmentSlot;

        let piglin = spawned(3, SpawnReason::Conversion);
        let living = piglin.living().unwrap();
        assert!(living.equipment(EquipmentSlot::Mainhand).is_none());
    }

    #[test]
    fn piglins_never_call_for_reinforcements_unless_they_lead() {
        use crate::entity::spawn::SPAWN_REINFORCEMENTS;
        use crate::spawn::SpawnReason;

        let mut leaders = 0;
        for seed in 0..400 {
            let mut piglin = spawned(seed, SpawnReason::Natural);
            let chance = piglin.living_mut().unwrap().attribute(SPAWN_REINFORCEMENTS);
            if chance > 0.0 {
                leaders += 1;
                assert!((0.5..=0.75).contains(&chance), "{chance}");
            }
        }
        assert!(leaders < 400 / 5);
    }
}
