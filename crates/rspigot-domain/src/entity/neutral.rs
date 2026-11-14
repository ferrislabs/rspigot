use rspigot_entities::EntityKind;
use uuid::Uuid;

use super::base::EntityBase;
use super::data::SyncedValue;
use crate::random::RandomSource;

pub const NO_ANGER_END_TIME: i64 = -1;
pub const ANGER_MIN_TICKS: i32 = 400;
pub const ANGER_MAX_TICKS: i32 = 780;
pub const WOLF_DATA_ANGER_END_TIME: u8 = 22;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NeutralState {
    pub end_time: i64,
    pub target: Option<Uuid>,
    pub legacy_remaining: Option<i32>,
}

impl Default for NeutralState {
    fn default() -> Self {
        Self {
            end_time: NO_ANGER_END_TIME,
            target: None,
            legacy_remaining: None,
        }
    }
}

impl NeutralState {
    pub fn is_angry(&self, game_time: i64) -> bool {
        self.end_time > 0 && self.end_time - game_time > 0
    }

    pub fn set_time_to_remain_angry(&mut self, game_time: i64, remaining: i64) {
        self.end_time = game_time + remaining;
    }

    pub fn start_timer(&mut self, game_time: i64, random: &mut impl RandomSource) {
        let remaining = random.next_int_between(ANGER_MIN_TICKS, ANGER_MAX_TICKS);
        self.set_time_to_remain_angry(game_time, i64::from(remaining));
    }

    pub fn clear(&mut self) {
        self.target = None;
        self.end_time = NO_ANGER_END_TIME;
    }

    pub fn angry_at_all_players(&self, game_time: i64, universal_anger: bool) -> bool {
        universal_anger && self.is_angry(game_time) && self.target.is_none()
    }

    pub fn resolve_legacy(&mut self, game_time: i64) {
        if let Some(remaining) = self.legacy_remaining.take() {
            self.set_time_to_remain_angry(game_time, i64::from(remaining));
        }
    }
}

pub fn sync(base: &mut EntityBase, state: &NeutralState) {
    if let Some(index) = synced_end_time_index(base.kind) {
        base.data.set(index, SyncedValue::Long(state.end_time));
    }
}

pub fn is_neutral(kind: EntityKind) -> bool {
    matches!(
        kind,
        EntityKind::Wolf
            | EntityKind::Bee
            | EntityKind::PolarBear
            | EntityKind::IronGolem
            | EntityKind::Enderman
            | EntityKind::ZombifiedPiglin
    )
}

pub fn synced_end_time_index(kind: EntityKind) -> Option<u8> {
    match kind {
        EntityKind::Wolf => Some(WOLF_DATA_ANGER_END_TIME),
        EntityKind::Bee => Some(super::species::bee::DATA_ANGER_END_TIME),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::random::LegacyRandom;

    #[test]
    fn anger_lasts_until_the_end_time() {
        let mut state = NeutralState::default();
        assert!(!state.is_angry(0));
        state.set_time_to_remain_angry(100, 50);
        assert!(state.is_angry(100));
        assert!(state.is_angry(149));
        assert!(!state.is_angry(150));
    }

    #[test]
    fn a_fresh_timer_lasts_between_twenty_and_thirty_nine_seconds() {
        let mut random = LegacyRandom::new(3);
        for _ in 0..200 {
            let mut state = NeutralState::default();
            state.start_timer(0, &mut random);
            assert!((400..=780).contains(&state.end_time));
        }
    }

    #[test]
    fn universal_anger_needs_the_rule_an_anger_and_no_target() {
        let mut state = NeutralState::default();
        state.set_time_to_remain_angry(0, 100);
        assert!(state.angry_at_all_players(10, true));
        assert!(!state.angry_at_all_players(10, false));
        state.target = Some(Uuid::from_u128(1));
        assert!(!state.angry_at_all_players(10, true));
    }

    #[test]
    fn legacy_remaining_time_is_resolved_against_the_game_time() {
        let mut state = NeutralState {
            legacy_remaining: Some(40),
            ..NeutralState::default()
        };
        state.resolve_legacy(1000);
        assert_eq!(state.end_time, 1040);
        assert_eq!(state.legacy_remaining, None);
    }
}
