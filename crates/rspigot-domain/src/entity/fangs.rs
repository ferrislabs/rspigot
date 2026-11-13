use uuid::Uuid;

pub const KEYS: &[&str] = &["Warmup", "Owner"];
pub const LIFE_TICKS: i32 = 22;
pub const HIT_TICK: i32 = -8;
pub const ATTACK_EVENT: i8 = 4;
pub const DAMAGE: f32 = 6.0;
pub const HIT_INFLATE_X: f64 = 0.2;
pub const HIT_INFLATE_Y: f64 = 0.0;
pub const HIT_INFLATE_Z: f64 = 0.2;
pub const RAD_TO_DEG: f32 = 57.295_776;
pub const ATTACK_SOUND: &str = "minecraft:entity.evoker_fangs.attack";
pub const CLIENT_PARTICLE_TICK: i32 = 14;
pub const CLIENT_PARTICLES: i32 = 12;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FangsState {
    pub warmup: i32,
    pub life_ticks: i32,
    pub sent_spike: bool,
    pub owner: Option<Uuid>,
}

impl Default for FangsState {
    fn default() -> Self {
        Self {
            warmup: 0,
            life_ticks: LIFE_TICKS,
            sent_spike: false,
            owner: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FangsStep {
    pub strike: bool,
    pub send_attack_event: bool,
    pub discard: bool,
}

impl FangsState {
    pub fn new(warmup: i32, owner: Option<Uuid>) -> Self {
        Self {
            warmup,
            owner,
            ..Self::default()
        }
    }

    pub fn tick(&mut self) -> FangsStep {
        self.warmup -= 1;
        if self.warmup >= 0 {
            return FangsStep::default();
        }
        let mut step = FangsStep {
            strike: self.warmup == HIT_TICK,
            ..FangsStep::default()
        };
        if !self.sent_spike {
            self.sent_spike = true;
            step.send_attack_event = true;
        }
        self.life_ticks -= 1;
        step.discard = self.life_ticks < 0;
        step
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_fang_lives_twenty_two_ticks_and_has_no_warmup_or_owner() {
        let state = FangsState::default();
        assert_eq!(state.warmup, 0);
        assert_eq!(state.life_ticks, 22);
        assert!(!state.sent_spike);
        assert_eq!(state.owner, None);
    }

    #[test]
    fn nothing_happens_while_the_warmup_runs_and_the_spike_event_goes_out_once() {
        let mut state = FangsState::new(3, None);
        for _ in 0..3 {
            assert_eq!(state.tick(), FangsStep::default());
        }
        let step = state.tick();
        assert!(step.send_attack_event);
        assert!(!step.strike);
        assert_eq!(state.warmup, -1);
        assert_eq!(state.life_ticks, 21);
        assert!(!state.tick().send_attack_event);
    }

    #[test]
    fn the_fang_strikes_exactly_eight_ticks_after_the_warmup_ends() {
        let mut state = FangsState::new(0, None);
        let mut strikes = Vec::new();
        for tick in 1..=30 {
            let step = state.tick();
            if step.strike {
                strikes.push(tick);
            }
            if step.discard {
                assert_eq!(tick, 1 + 22 + 1 - 1);
                break;
            }
        }
        assert_eq!(strikes, vec![8]);
    }

    #[test]
    fn the_fang_is_discarded_once_its_life_runs_below_zero() {
        let mut state = FangsState::new(0, None);
        let mut ticks = 0;
        loop {
            ticks += 1;
            if state.tick().discard {
                break;
            }
            assert!(ticks < 100);
        }
        assert_eq!(ticks, 23);
        assert_eq!(state.life_ticks, -1);
    }

    #[test]
    fn the_warmup_delays_the_first_strike() {
        let mut state = FangsState::new(5, None);
        let mut strike_tick = None;
        for tick in 1..=40 {
            if state.tick().strike {
                strike_tick = Some(tick);
                break;
            }
        }
        assert_eq!(strike_tick, Some(5 + 8));
    }

    #[test]
    fn the_warmup_and_the_owner_round_trip_in_the_vanilla_layout() {
        use crate::EntityId;
        use crate::entity::EntityState;
        use crate::entity::save::{load_entity, save_entity, uuid_tag};
        use rspigot_nbt::Compound;
        let owner = Uuid::from_u128(77);
        let nbt = Compound::new()
            .with("id", "minecraft:evoker_fangs")
            .with("Warmup", 6)
            .with("Owner", uuid_tag(owner));
        let entity = load_entity(&nbt, EntityId(1), Uuid::from_u128(1)).unwrap();
        let EntityState::Fangs(state) = &entity.state else {
            panic!("fangs are simulated");
        };
        assert_eq!(state.warmup, 6);
        assert_eq!(state.owner, Some(owner));
        assert_eq!(state.life_ticks, LIFE_TICKS);
        let saved = save_entity(&entity, None).unwrap();
        assert_eq!(saved.get_i32("Warmup"), Some(6));
        assert_eq!(saved.get("Owner"), Some(&uuid_tag(owner)));
        let again = load_entity(&saved, EntityId(1), Uuid::from_u128(1)).unwrap();
        assert_eq!(save_entity(&again, None), Some(saved));
    }

    #[test]
    fn fangs_without_an_owner_save_no_owner() {
        use crate::EntityId;
        use crate::entity::save::{load_entity, save_entity};
        use rspigot_nbt::Compound;
        let nbt = Compound::new().with("id", "minecraft:evoker_fangs");
        let entity = load_entity(&nbt, EntityId(1), Uuid::from_u128(1)).unwrap();
        let saved = save_entity(&entity, None).unwrap();
        assert_eq!(saved.get("Owner"), None);
        assert_eq!(saved.get_i32("Warmup"), Some(0));
    }
}
