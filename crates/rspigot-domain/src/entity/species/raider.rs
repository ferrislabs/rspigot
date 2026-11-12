use rspigot_nbt::{Compound, Tag};

use crate::BlockPos;
use crate::entity::base::EntityBase;
use crate::entity::data::SyncedValue;
use crate::random::RandomSource;
use crate::spawn::SpawnReason;

pub const DATA_IS_CELEBRATING: u8 = 16;
pub const KEYS: &[&str] = &[
    "Wave",
    "CanJoinRaid",
    "patrol_target",
    "PatrolLeader",
    "Patrolling",
];
pub const PATROL_LEADER_ODDS: f32 = 0.06;
pub const PATROL_KEEP_DISTANCE_SQR: f64 = 16_384.0;
pub const NO_ACTION_STEP: i32 = 2;
pub const PATROL_SPEED: f64 = 0.7;
pub const PATROL_LEADER_SPEED: f64 = 0.595;
pub const PATROL_RETARGET_RANGE: f64 = 10.0;
pub const PATROL_TARGET_SPAN: i32 = 1000;
pub const PATROL_COMPANION_RANGE: f64 = 16.0;
pub const PATROL_FAILURE_COOLDOWN: i64 = 200;
pub const PATROL_STEP: f64 = 10.0;
pub const PATROL_SIDE_STEP: f64 = 0.4;
pub const PATROL_SIDE_DEGREES: f64 = 90.0;
pub const PATROL_WANDER_SPAN: i32 = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RaiderState {
    pub patrol_target: Option<BlockPos>,
    pub patrol_leader: bool,
    pub patrolling: bool,
    pub wave: i32,
    pub can_join_raid: bool,
}

pub fn define(base: &mut EntityBase) {
    base.data
        .define(DATA_IS_CELEBRATING, SyncedValue::Boolean(false));
}

pub fn is_celebrating(base: &EntityBase) -> bool {
    base.data.boolean(DATA_IS_CELEBRATING)
}

pub fn set_celebrating(base: &mut EntityBase, celebrating: bool) {
    base.data
        .set(DATA_IS_CELEBRATING, SyncedValue::Boolean(celebrating));
}

impl RaiderState {
    pub fn set_patrol_target(&mut self, target: BlockPos) {
        self.patrol_target = Some(target);
        self.patrolling = true;
    }

    pub fn find_patrol_target(&mut self, here: BlockPos, random: &mut impl RandomSource) {
        let dx = random.next_int(PATROL_TARGET_SPAN) - PATROL_TARGET_SPAN / 2;
        let dz = random.next_int(PATROL_TARGET_SPAN) - PATROL_TARGET_SPAN / 2;
        self.patrol_target = Some(here.offset(dx, 0, dz));
        self.patrolling = true;
    }

    pub fn removes_when_far_away(&self, distance_sqr: f64) -> bool {
        !(self.patrolling && distance_sqr <= PATROL_KEEP_DISTANCE_SQR)
    }

    pub fn can_join_patrol(&self) -> bool {
        true
    }

    pub fn finalize(
        &mut self,
        reason: SpawnReason,
        can_be_leader: bool,
        join_raid: bool,
        level: &mut impl RandomSource,
    ) {
        self.can_join_raid = join_raid;
        if !matches!(
            reason,
            SpawnReason::Patrol | SpawnReason::Event | SpawnReason::Structure
        ) && level.next_float() < PATROL_LEADER_ODDS
            && can_be_leader
        {
            self.patrol_leader = true;
        }
        if reason == SpawnReason::Patrol {
            self.patrolling = true;
        }
    }
}

pub fn read(nbt: &Compound, state: &mut RaiderState) {
    state.wave = nbt.get_i32("Wave").unwrap_or(0);
    state.can_join_raid = nbt.get_bool("CanJoinRaid").unwrap_or(false);
    state.patrol_target = match nbt.get("patrol_target") {
        Some(Tag::IntArray(parts)) if parts.len() == 3 => {
            Some(BlockPos::new(parts[0], parts[1], parts[2]))
        }
        _ => None,
    };
    state.patrol_leader = nbt.get_bool("PatrolLeader").unwrap_or(false);
    state.patrolling = nbt.get_bool("Patrolling").unwrap_or(false);
}

pub fn write(state: &RaiderState, out: &mut Compound) {
    out.insert("Wave", state.wave);
    out.insert("CanJoinRaid", i8::from(state.can_join_raid));
    if let Some(target) = state.patrol_target {
        out.insert(
            "patrol_target",
            Tag::IntArray(vec![target.x, target.y, target.z]),
        );
    }
    out.insert("PatrolLeader", i8::from(state.patrol_leader));
    out.insert("Patrolling", i8::from(state.patrolling));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::random::LegacyRandom;

    #[test]
    fn a_patrolling_raider_stays_within_one_hundred_and_twenty_eight_blocks() {
        let mut state = RaiderState::default();
        assert!(state.removes_when_far_away(0.0));
        state.patrolling = true;
        assert!(!state.removes_when_far_away(16_384.0));
        assert!(state.removes_when_far_away(16_384.5));
    }

    #[test]
    fn a_patrol_target_is_picked_within_five_hundred_blocks_and_starts_the_patrol() {
        let mut state = RaiderState::default();
        let mut random = LegacyRandom::new(3);
        let mut mirror = LegacyRandom::new(3);
        let dx = mirror.next_int(1000) - 500;
        let dz = mirror.next_int(1000) - 500;
        state.find_patrol_target(BlockPos::new(10, 70, -20), &mut random);
        assert_eq!(
            state.patrol_target,
            Some(BlockPos::new(10 + dx, 70, -20 + dz))
        );
        assert!(state.patrolling);
    }

    #[test]
    fn finalizing_rolls_a_leader_unless_the_spawn_is_a_patrol_event_or_structure() {
        for reason in [
            SpawnReason::Patrol,
            SpawnReason::Event,
            SpawnReason::Structure,
        ] {
            let mut state = RaiderState::default();
            let mut level = LegacyRandom::new(1);
            let mut untouched = LegacyRandom::new(1);
            state.finalize(reason, true, true, &mut level);
            assert_eq!(level.next_int(1000), untouched.next_int(1000));
        }
        let mut state = RaiderState::default();
        let mut level = LegacyRandom::new(1);
        let mut mirror = LegacyRandom::new(1);
        let roll = mirror.next_float();
        state.finalize(SpawnReason::Natural, true, false, &mut level);
        assert_eq!(state.patrol_leader, roll < PATROL_LEADER_ODDS);
        assert!(!state.can_join_raid);
        assert!(!state.patrolling);
        assert_eq!(level.next_int(1000), mirror.next_int(1000));
    }

    #[test]
    fn a_leader_roll_is_consumed_even_when_the_raider_cannot_lead() {
        let mut state = RaiderState::default();
        let mut level = LegacyRandom::new(8);
        let mut mirror = LegacyRandom::new(8);
        mirror.next_float();
        state.finalize(SpawnReason::Natural, false, true, &mut level);
        assert!(!state.patrol_leader);
        assert_eq!(level.next_int(1000), mirror.next_int(1000));
    }

    #[test]
    fn a_patrol_spawn_starts_patrolling() {
        let mut state = RaiderState::default();
        state.finalize(SpawnReason::Patrol, false, true, &mut LegacyRandom::new(1));
        assert!(state.patrolling);
        assert!(state.can_join_raid);
    }

    #[test]
    fn the_raider_state_round_trips_through_nbt() {
        let state = RaiderState {
            patrol_target: Some(BlockPos::new(1, 2, 3)),
            patrol_leader: true,
            patrolling: true,
            wave: 4,
            can_join_raid: true,
        };
        let mut out = Compound::new();
        write(&state, &mut out);
        let mut read_back = RaiderState::default();
        read(&out, &mut read_back);
        assert_eq!(read_back, state);
        let mut empty = RaiderState::default();
        read(&Compound::new(), &mut empty);
        assert_eq!(empty, RaiderState::default());
    }
}
