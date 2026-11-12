use rspigot_nbt::Compound;

use crate::entity::base::EntityBase;
use crate::entity::data::{StateKind, SyncedValue};
use crate::random::RandomSource;

pub const DATA_STATE: u8 = 18;
pub const SCUTE_BASE_TICKS: i32 = 6000;
pub const ROLL_UP_STAY_TICKS: i32 = 6000;
pub const DANGER_TICKS: i64 = 80;
pub const PEEK_DANGER_THRESHOLD: i64 = 75;
pub const PEEK_EVENT: i8 = 64;
pub const KEYS: &[&str] = &["state", "scute_time"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum State {
    #[default]
    Idle,
    Rolling,
    Scared,
    Unrolling,
}

impl State {
    pub const ALL: [Self; 4] = [Self::Idle, Self::Rolling, Self::Scared, Self::Unrolling];

    pub const fn id(self) -> i32 {
        match self {
            Self::Idle => 0,
            Self::Rolling => 1,
            Self::Scared => 2,
            Self::Unrolling => 3,
        }
    }

    pub fn from_id(id: i32) -> Self {
        Self::ALL
            .iter()
            .copied()
            .find(|state| state.id() == id)
            .unwrap_or(Self::Idle)
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Rolling => "rolling",
            Self::Scared => "scared",
            Self::Unrolling => "unrolling",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|state| state.name() == name)
    }

    pub const fn is_threatened(self) -> bool {
        !matches!(self, Self::Idle)
    }

    pub const fn animation_duration(self) -> i32 {
        match self {
            Self::Idle => 0,
            Self::Rolling => 10,
            Self::Scared => 50,
            Self::Unrolling => 30,
        }
    }

    pub fn should_hide_in_shell(self, ticks_in_state: i64) -> bool {
        match self {
            Self::Idle => false,
            Self::Rolling => ticks_in_state > 5,
            Self::Scared => true,
            Self::Unrolling => ticks_in_state < 26,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ArmadilloState {
    pub in_state_ticks: i32,
    pub scute_time: Option<i32>,
}

pub fn define(base: &mut EntityBase) -> ArmadilloState {
    base.data.define(
        DATA_STATE,
        SyncedValue::State(StateKind::Armadillo, State::Idle.id()),
    );
    ArmadilloState::default()
}

pub fn state(base: &EntityBase) -> State {
    State::from_id(base.data.state(DATA_STATE))
}

pub fn is_scared(base: &EntityBase) -> bool {
    state(base).is_threatened()
}

pub fn switch_to_state(base: &mut EntityBase, armadillo: &mut ArmadilloState, next: State) {
    if state(base) != next {
        base.data.set(
            DATA_STATE,
            SyncedValue::State(StateKind::Armadillo, next.id()),
        );
        armadillo.in_state_ticks = 0;
    }
}

pub fn tick(armadillo: &mut ArmadilloState) {
    armadillo.in_state_ticks += 1;
}

pub fn pick_next_scute_time(random: &mut impl RandomSource) -> i32 {
    random.next_int(SCUTE_BASE_TICKS) + SCUTE_BASE_TICKS
}

pub fn hurt_amount(scared: bool, amount: f32) -> f32 {
    if scared { (amount - 1.0) / 2.0 } else { amount }
}

pub fn read(nbt: &Compound, base: &mut EntityBase, armadillo: &mut ArmadilloState) {
    let next = nbt
        .get_str("state")
        .and_then(State::from_name)
        .unwrap_or(State::Idle);
    switch_to_state(base, armadillo, next);
    armadillo.scute_time = nbt.get_i32("scute_time");
}

pub fn write(base: &EntityBase, armadillo: &ArmadilloState, out: &mut Compound) {
    out.insert("state", state(base).name());
    if let Some(scute_time) = armadillo.scute_time {
        out.insert("scute_time", scute_time);
    }
}

#[cfg(test)]
mod tests {
    use rspigot_entities::EntityKind;
    use uuid::Uuid;

    use super::*;
    use crate::EntityId;
    use crate::random::LegacyRandom;

    fn armadillo() -> (EntityBase, ArmadilloState) {
        let mut base = EntityBase::new(EntityId(1), Uuid::from_u128(1), EntityKind::Armadillo);
        let state = define(&mut base);
        (base, state)
    }

    #[test]
    fn the_four_states_keep_the_vanilla_durations_and_threat_flags() {
        let table = [
            (State::Idle, false, 0),
            (State::Rolling, true, 10),
            (State::Scared, true, 50),
            (State::Unrolling, true, 30),
        ];
        for (state, threatened, duration) in table {
            assert_eq!(state.is_threatened(), threatened, "{state:?}");
            assert_eq!(state.animation_duration(), duration, "{state:?}");
            assert_eq!(State::from_id(state.id()), state);
            assert_eq!(State::from_name(state.name()), Some(state));
        }
    }

    #[test]
    fn the_shell_closes_after_five_ticks_of_rolling_and_opens_before_the_unroll_ends() {
        assert!(!State::Idle.should_hide_in_shell(100));
        assert!(!State::Rolling.should_hide_in_shell(5));
        assert!(State::Rolling.should_hide_in_shell(6));
        assert!(State::Scared.should_hide_in_shell(0));
        assert!(State::Unrolling.should_hide_in_shell(25));
        assert!(!State::Unrolling.should_hide_in_shell(26));
    }

    #[test]
    fn switching_the_state_resets_the_time_spent_in_it_only_when_it_changes() {
        let (mut base, mut armadillo) = armadillo();
        tick(&mut armadillo);
        tick(&mut armadillo);
        switch_to_state(&mut base, &mut armadillo, State::Idle);
        assert_eq!(armadillo.in_state_ticks, 2);
        switch_to_state(&mut base, &mut armadillo, State::Rolling);
        assert_eq!(armadillo.in_state_ticks, 0);
        assert!(is_scared(&base));
    }

    #[test]
    fn a_rolled_up_armadillo_takes_half_of_the_damage_above_one() {
        assert_eq!(hurt_amount(false, 7.0), 7.0);
        assert_eq!(hurt_amount(true, 7.0), 3.0);
        assert_eq!(hurt_amount(true, 1.0), 0.0);
    }

    #[test]
    fn the_state_and_scute_timer_round_trip_through_the_save() {
        let (mut base, mut armadillo) = armadillo();
        switch_to_state(&mut base, &mut armadillo, State::Scared);
        armadillo.scute_time = Some(321);
        let mut out = Compound::new();
        write(&base, &armadillo, &mut out);
        assert_eq!(out.get_str("state"), Some("scared"));
        assert_eq!(out.get_i32("scute_time"), Some(321));
        let (mut fresh, mut restored) = self::armadillo();
        read(&out, &mut fresh, &mut restored);
        assert_eq!(state(&fresh), State::Scared);
        assert_eq!(restored.scute_time, Some(321));
    }

    #[test]
    fn the_next_scute_comes_between_five_and_ten_minutes_later() {
        let mut random = LegacyRandom::new(4);
        for _ in 0..200 {
            assert!((6000..12000).contains(&pick_next_scute_time(&mut random)));
        }
    }
}
