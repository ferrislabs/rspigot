use rspigot_entities::{EntityDimensions, EntityKind};

use crate::entity::base::EntityBase;
use crate::entity::data::{StateKind, SyncedValue};
use crate::{BlockPos, Vec3};

pub const DATA_STATE: u8 = 18;
pub const DATA_DROP_SEED_AT_TICK: u8 = 19;
pub const KEYS: &[&str] = &[];
pub const BABY_START_AGE: i32 = -48_000;
pub const DIG_DROP_DELAY: i32 = 120;
pub const DIG_START_EVENT: i8 = 63;
pub const HEAD_FORWARD: f64 = 2.25;
pub const HEAD_BLOCK_Y_OFFSET: f64 = 0.2;
pub const DIGGING_HEIGHT_REDUCTION: f32 = 0.4;
pub const DIGGING_EYE_HEIGHT: f32 = 0.81;
pub const EXPLORED_LIMIT: usize = 20;
pub const EGG_PLOP_PITCH_SPREAD: f32 = 0.2;
pub const EGG_PLOP_PITCH_BASE: f32 = 0.5;
pub const BABY_SCENT_PITCH: f32 = 1.3;
pub const BABY_SCALE: f32 = 0.5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum State {
    #[default]
    Idling,
    FeelingHappy,
    Scenting,
    Sniffing,
    Searching,
    Digging,
    Rising,
}

impl State {
    pub const ALL: [Self; 7] = [
        Self::Idling,
        Self::FeelingHappy,
        Self::Scenting,
        Self::Sniffing,
        Self::Searching,
        Self::Digging,
        Self::Rising,
    ];

    pub const fn id(self) -> i32 {
        match self {
            Self::Idling => 0,
            Self::FeelingHappy => 1,
            Self::Scenting => 2,
            Self::Sniffing => 3,
            Self::Searching => 4,
            Self::Digging => 5,
            Self::Rising => 6,
        }
    }

    pub fn from_id(id: i32) -> Self {
        Self::ALL
            .iter()
            .copied()
            .find(|state| state.id() == id)
            .unwrap_or(Self::Idling)
    }

    pub const fn can_mate(self) -> bool {
        matches!(self, Self::Idling | Self::Scenting | Self::FeelingHappy)
    }

    pub const fn plays_digging_sound(self) -> bool {
        matches!(self, Self::Digging | Self::Searching)
    }

    pub const fn silences_ambient(self) -> bool {
        matches!(self, Self::Digging | Self::Searching)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SnifferState;

pub fn define(base: &mut EntityBase) -> SnifferState {
    base.data.define(
        DATA_STATE,
        SyncedValue::State(StateKind::Sniffer, State::Idling.id()),
    );
    base.data
        .define(DATA_DROP_SEED_AT_TICK, SyncedValue::Int(0));
    SnifferState
}

pub fn state(base: &EntityBase) -> State {
    State::from_id(base.data.state(DATA_STATE))
}

pub fn is_digging(base: &EntityBase) -> bool {
    state(base) == State::Digging
}

pub fn set_state(base: &mut EntityBase, baby: bool, next: State) -> bool {
    let changed = base.data.set(
        DATA_STATE,
        SyncedValue::State(StateKind::Sniffer, next.id()),
    );
    if changed {
        crate::entity::ageable::refresh_dimensions(base, baby);
    }
    changed
}

pub fn drop_seed_at_tick(base: &EntityBase) -> i32 {
    base.data.int(DATA_DROP_SEED_AT_TICK)
}

pub fn set_drop_seed_at_tick(base: &mut EntityBase, tick: i32) {
    base.data
        .set(DATA_DROP_SEED_AT_TICK, SyncedValue::Int(tick));
}

pub fn digging_dimensions(baby: bool) -> EntityDimensions {
    let adult = EntityKind::Sniffer.dimensions();
    let scale = if baby { BABY_SCALE } else { 1.0 };
    EntityDimensions {
        width: adult.width * scale,
        height: (adult.height - DIGGING_HEIGHT_REDUCTION) * scale,
        eye_height: DIGGING_EYE_HEIGHT * scale,
    }
}

pub fn head_position(position: Vec3, forward: Vec3) -> Vec3 {
    position + forward.scale(HEAD_FORWARD)
}

pub fn head_block(position: Vec3, forward: Vec3) -> BlockPos {
    let head = head_position(position, forward);
    Vec3::new(head.x, position.y + HEAD_BLOCK_Y_OFFSET, head.z).block()
}

pub fn egg_plop_pitch(first: f32, second: f32) -> f32 {
    (first - second) * EGG_PLOP_PITCH_SPREAD + EGG_PLOP_PITCH_BASE
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;
    use crate::EntityId;

    fn sniffer() -> EntityBase {
        let mut base = EntityBase::new(EntityId(1), Uuid::from_u128(1), EntityKind::Sniffer);
        define(&mut base);
        base
    }

    #[test]
    fn the_seven_states_keep_their_vanilla_ids() {
        let ids = State::ALL.map(State::id);
        assert_eq!(ids, [0, 1, 2, 3, 4, 5, 6]);
        for state in State::ALL {
            assert_eq!(State::from_id(state.id()), state);
        }
        assert_eq!(State::from_id(99), State::Idling);
    }

    #[test]
    fn only_idle_scenting_and_happy_sniffers_can_mate() {
        let mating: Vec<State> = State::ALL.into_iter().filter(|s| s.can_mate()).collect();
        assert_eq!(
            mating,
            [State::Idling, State::FeelingHappy, State::Scenting]
        );
    }

    #[test]
    fn digging_and_searching_play_the_digging_sound_and_mute_the_ambient_one() {
        for state in State::ALL {
            let expected = matches!(state, State::Digging | State::Searching);
            assert_eq!(state.plays_digging_sound(), expected, "{state:?}");
            assert_eq!(state.silences_ambient(), expected, "{state:?}");
        }
    }

    #[test]
    fn a_fresh_sniffer_idles_and_has_no_seed_scheduled() {
        let base = sniffer();
        assert_eq!(state(&base), State::Idling);
        assert_eq!(drop_seed_at_tick(&base), 0);
    }

    #[test]
    fn digging_shrinks_the_box_by_four_tenths_and_lowers_the_eyes() {
        let mut base = sniffer();
        let standing = base.dimensions;
        assert!(set_state(&mut base, false, State::Digging));
        assert_eq!(base.dimensions.width, standing.width);
        assert!((base.dimensions.height - (standing.height - 0.4)).abs() < 1e-6);
        assert!((base.dimensions.eye_height - 0.81).abs() < 1e-6);
        assert!(set_state(&mut base, false, State::Rising));
        assert_eq!(base.dimensions, standing);
    }

    #[test]
    fn a_baby_digs_with_half_the_digging_box() {
        let adult = digging_dimensions(false);
        let baby = digging_dimensions(true);
        assert_eq!(baby.width, adult.width * 0.5);
        assert_eq!(baby.height, adult.height * 0.5);
        assert_eq!(baby.eye_height, adult.eye_height * 0.5);
    }

    #[test]
    fn setting_the_same_state_changes_nothing() {
        let mut base = sniffer();
        assert!(!set_state(&mut base, false, State::Idling));
    }

    #[test]
    fn the_head_block_is_two_and_a_quarter_blocks_ahead_and_a_fifth_above_the_feet() {
        let feet = Vec3::new(10.5, 64.0, 10.5);
        let south = Vec3::new(0.0, 0.0, 1.0);
        assert_eq!(head_block(feet, south), BlockPos::new(10, 64, 12));
        let east = Vec3::new(-1.0, 0.0, 0.0);
        assert_eq!(head_block(feet, east), BlockPos::new(8, 64, 10));
    }

    #[test]
    fn the_seed_tick_is_synced_data() {
        let mut base = sniffer();
        set_drop_seed_at_tick(&mut base, 321);
        assert_eq!(drop_seed_at_tick(&base), 321);
    }

    #[test]
    fn the_egg_plop_pitch_stays_within_a_fifth_of_one_half() {
        assert_eq!(egg_plop_pitch(0.5, 0.5), 0.5);
        assert!((egg_plop_pitch(1.0, 0.0) - 0.7).abs() < 1e-6);
        assert!((egg_plop_pitch(0.0, 1.0) - 0.3).abs() < 1e-6);
    }
}
