use rspigot_nbt::{Compound, Tag};

use crate::BlockPos;
use crate::entity::base::EntityBase;
use crate::entity::data::SyncedValue;

pub const DATA_CAN_MOVE: u8 = 16;
pub const DATA_IS_ACTIVE: u8 = 17;
pub const DATA_IS_TEARING_DOWN: u8 = 18;
pub const DATA_HOME_POS: u8 = 19;
pub const HOME_POS_KEY: &str = "home_pos";
pub const KEYS: &[&str] = &[HOME_POS_KEY];
pub const INVULNERABILITY_TICKS: i32 = 8;
pub const ATTACK_ANIMATION_TICKS: i32 = 15;
pub const HURT_EVENT: i8 = 66;
pub const ATTACK_EVENT: i8 = 4;
pub const FIRE_IMMUNE_WHEN_BOUND: bool = true;
pub const TEARDOWN_TICKS: i32 = 46;
pub const STUCK_LIMIT: i32 = 4;
pub const CRUMBLE_WOOD_COUNT: i32 = 100;
pub const CRUMBLE_HEART_COUNT: i32 = 10;
pub const CRUMBLE_SPREAD: f64 = 0.3;
pub const TWITCH_SOUND: &str = "minecraft:entity.creaking.twitch";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CreakingState {
    pub invulnerability_ticks: i32,
    pub attack_ticks: i32,
    pub stuck_counter: i32,
}

pub fn define(base: &mut EntityBase) -> CreakingState {
    base.data.define(DATA_CAN_MOVE, SyncedValue::Boolean(true));
    base.data
        .define(DATA_IS_ACTIVE, SyncedValue::Boolean(false));
    base.data
        .define(DATA_IS_TEARING_DOWN, SyncedValue::Boolean(false));
    base.data
        .define(DATA_HOME_POS, SyncedValue::OptionalBlockPos(None));
    CreakingState::default()
}

pub fn can_move(base: &EntityBase) -> bool {
    base.data.boolean(DATA_CAN_MOVE)
}

pub fn set_can_move(base: &mut EntityBase, can_move: bool) {
    base.data.set(DATA_CAN_MOVE, SyncedValue::Boolean(can_move));
}

pub fn is_active(base: &EntityBase) -> bool {
    base.data.boolean(DATA_IS_ACTIVE)
}

pub fn set_active(base: &mut EntityBase, active: bool) {
    base.data.set(DATA_IS_ACTIVE, SyncedValue::Boolean(active));
}

pub fn is_tearing_down(base: &EntityBase) -> bool {
    base.data.boolean(DATA_IS_TEARING_DOWN)
}

pub fn set_tearing_down(base: &mut EntityBase) {
    base.data
        .set(DATA_IS_TEARING_DOWN, SyncedValue::Boolean(true));
}

pub fn home_pos(base: &EntityBase) -> Option<BlockPos> {
    match base.data.get(DATA_HOME_POS) {
        Some(SyncedValue::OptionalBlockPos(pos)) => *pos,
        _ => None,
    }
}

pub fn set_home_pos(base: &mut EntityBase, pos: BlockPos) {
    base.data
        .set(DATA_HOME_POS, SyncedValue::OptionalBlockPos(Some(pos)));
}

pub fn is_heart_bound(base: &EntityBase) -> bool {
    home_pos(base).is_some()
}

pub fn death_duration(base: &EntityBase) -> i32 {
    if is_heart_bound(base) && is_tearing_down(base) {
        TEARDOWN_TICKS
    } else {
        crate::entity::living::DEATH_DURATION
    }
}

impl CreakingState {
    pub fn player_is_stuck(&mut self, eyes_inside: &[bool]) -> bool {
        if !eyes_inside.iter().any(|inside| *inside) {
            self.stuck_counter = 0;
            return false;
        }
        self.stuck_counter += 1;
        self.stuck_counter > STUCK_LIMIT
    }

    pub fn tick(&mut self) {
        if self.invulnerability_ticks > 0 {
            self.invulnerability_ticks -= 1;
        }
        if self.attack_ticks > 0 {
            self.attack_ticks -= 1;
        }
    }
}

pub fn read(nbt: &Compound, base: &mut EntityBase, _state: &mut CreakingState) {
    if let Some(pos) = nbt.get_int_array(HOME_POS_KEY).filter(|pos| pos.len() == 3) {
        set_home_pos(base, BlockPos::new(pos[0], pos[1], pos[2]));
    }
}

pub fn write(base: &EntityBase, _state: &CreakingState, out: &mut Compound) {
    if let Some(pos) = home_pos(base) {
        out.insert(HOME_POS_KEY, Tag::IntArray(vec![pos.x, pos.y, pos.z]));
    }
}

#[cfg(test)]
mod tests {
    use rspigot_entities::EntityKind;
    use uuid::Uuid;

    use super::*;
    use crate::EntityId;
    use crate::entity::save::{load_entity, save_entity};

    fn base() -> EntityBase {
        EntityBase::new(EntityId(1), Uuid::from_u128(1), EntityKind::Creaking)
    }

    #[test]
    fn a_creaking_starts_movable_inactive_and_unbound() {
        let mut base = base();
        define(&mut base);
        assert!(can_move(&base));
        assert!(!is_active(&base));
        assert!(!is_tearing_down(&base));
        assert_eq!(home_pos(&base), None);
        assert!(!is_heart_bound(&base));
    }

    #[test]
    fn the_synced_slots_follow_the_vanilla_data_order() {
        assert_eq!(
            [
                DATA_CAN_MOVE,
                DATA_IS_ACTIVE,
                DATA_IS_TEARING_DOWN,
                DATA_HOME_POS
            ],
            [16, 17, 18, 19]
        );
    }

    #[test]
    fn the_animation_counters_count_down_to_zero() {
        let mut state = CreakingState {
            invulnerability_ticks: INVULNERABILITY_TICKS,
            attack_ticks: ATTACK_ANIMATION_TICKS,
            stuck_counter: 0,
        };
        for _ in 0..20 {
            state.tick();
        }
        assert_eq!(state, CreakingState::default());
    }

    #[test]
    fn a_player_with_the_eyes_inside_the_body_is_stuck_after_five_checks() {
        let mut state = CreakingState::default();
        for _ in 0..4 {
            assert!(!state.player_is_stuck(&[true]));
        }
        assert!(state.player_is_stuck(&[true]));
        assert!(state.player_is_stuck(&[false, true]));
    }

    #[test]
    fn the_stuck_counter_resets_when_nobody_is_inside_or_nobody_is_near() {
        let mut state = CreakingState::default();
        for _ in 0..3 {
            state.player_is_stuck(&[true]);
        }
        assert!(!state.player_is_stuck(&[false]));
        assert_eq!(state.stuck_counter, 0);
        for _ in 0..3 {
            state.player_is_stuck(&[true]);
        }
        assert!(!state.player_is_stuck(&[]));
        assert_eq!(state.stuck_counter, 0);
    }

    #[test]
    fn a_heart_bound_creaking_that_is_tearing_down_twitches_for_forty_six_ticks() {
        let mut base = base();
        define(&mut base);
        assert_eq!(death_duration(&base), 20);
        set_tearing_down(&mut base);
        assert_eq!(death_duration(&base), 20);
        set_home_pos(&mut base, BlockPos::new(1, 2, 3));
        assert_eq!(death_duration(&base), TEARDOWN_TICKS);
        assert_eq!(TEARDOWN_TICKS, 46);
    }

    #[test]
    fn the_twitch_and_crumble_effects_use_the_vanilla_numbers() {
        assert_eq!(CRUMBLE_WOOD_COUNT, 100);
        assert_eq!(CRUMBLE_HEART_COUNT, 10);
        assert_eq!(CRUMBLE_SPREAD, 0.3);
        assert_eq!(TWITCH_SOUND, "minecraft:entity.creaking.twitch");
    }

    #[test]
    fn the_home_position_round_trips_through_nbt_as_an_int_array() {
        let nbt = Compound::new()
            .with("id", "minecraft:creaking")
            .with(HOME_POS_KEY, Tag::IntArray(vec![4, 70, -9]));
        let entity = load_entity(&nbt, EntityId(1), Uuid::from_u128(1)).unwrap();
        assert_eq!(home_pos(&entity.base), Some(BlockPos::new(4, 70, -9)));
        let saved = save_entity(&entity, None).unwrap();
        assert_eq!(saved.get_int_array(HOME_POS_KEY), Some(&[4, 70, -9][..]));
        let unbound = load_entity(
            &Compound::new().with("id", "minecraft:creaking"),
            EntityId(1),
            Uuid::from_u128(1),
        )
        .unwrap();
        assert!(
            !save_entity(&unbound, None)
                .unwrap()
                .contains_key(HOME_POS_KEY)
        );
    }
}
