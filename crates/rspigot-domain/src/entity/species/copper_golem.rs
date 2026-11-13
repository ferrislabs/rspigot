use rspigot_nbt::Compound;
use uuid::Uuid;

use crate::BlockPos;
use crate::entity::base::EntityBase;
use crate::entity::data::{StateKind, SyncedValue};
use crate::entity::living::LivingState;
use crate::entity::spawn::mob_finalize;
use crate::entity::species::Species;
use crate::random::RandomSource;
use crate::spawn::SpawnGroupData;

pub const DATA_WEATHER_STATE: u8 = 16;
pub const DATA_GOLEM_STATE: u8 = 17;
pub const WEATHER_STATE_KEY: &str = "weather_state";
pub const NEXT_WEATHER_AGE_KEY: &str = "next_weather_age";
pub const KEYS: &[&str] = &[NEXT_WEATHER_AGE_KEY, WEATHER_STATE_KEY];
pub const WEATHERING_UNSET: i64 = -1;
pub const WEATHERING_WAXED: i64 = -2;
pub const MIN_WEATHERING_TICKS: i32 = 504_000;
pub const MAX_WEATHERING_TICKS: i32 = 552_000;
pub const STATUE_CHANCE: f32 = 0.0058;
pub const CONTAINER_INTERACTION_RANGE: f64 = 3.0;
pub const PATH_LENGTH: f32 = 48.0;
pub const MIN_INITIAL_TRANSPORT_COOLDOWN: i32 = 60;
pub const MAX_INITIAL_TRANSPORT_COOLDOWN: i32 = 99;
pub const WAX_ON_EVENT: i32 = 3003;
pub const WAX_OFF_EVENT: i32 = 3004;
pub const SCRAPE_EVENT: i32 = 3005;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum WeatherState {
    #[default]
    Unaffected,
    Exposed,
    Weathered,
    Oxidized,
}

impl WeatherState {
    pub const ALL: [Self; 4] = [
        Self::Unaffected,
        Self::Exposed,
        Self::Weathered,
        Self::Oxidized,
    ];

    pub const fn id(self) -> i32 {
        match self {
            Self::Unaffected => 0,
            Self::Exposed => 1,
            Self::Weathered => 2,
            Self::Oxidized => 3,
        }
    }

    pub const fn from_id(id: i32) -> Self {
        match id {
            i32::MIN..=0 => Self::Unaffected,
            1 => Self::Exposed,
            2 => Self::Weathered,
            _ => Self::Oxidized,
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Unaffected => "unaffected",
            Self::Exposed => "exposed",
            Self::Weathered => "weathered",
            Self::Oxidized => "oxidized",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|state| state.name() == name)
    }

    pub const fn next(self) -> Self {
        Self::from_id(self.id() + 1)
    }

    pub const fn previous(self) -> Self {
        Self::from_id(self.id() - 1)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum GolemState {
    #[default]
    Idle,
    GettingItem,
    GettingNoItem,
    DroppingItem,
    DroppingNoItem,
}

impl GolemState {
    pub const ALL: [Self; 5] = [
        Self::Idle,
        Self::GettingItem,
        Self::GettingNoItem,
        Self::DroppingItem,
        Self::DroppingNoItem,
    ];

    pub const fn id(self) -> i32 {
        match self {
            Self::Idle => 0,
            Self::GettingItem => 1,
            Self::GettingNoItem => 2,
            Self::DroppingItem => 3,
            Self::DroppingNoItem => 4,
        }
    }

    pub const fn from_id(id: i32) -> Self {
        match id {
            1 => Self::GettingItem,
            2 => Self::GettingNoItem,
            3 => Self::DroppingItem,
            4 => Self::DroppingNoItem,
            _ => Self::Idle,
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::GettingItem => "getting_item",
            Self::GettingNoItem => "getting_no_item",
            Self::DroppingItem => "dropping_item",
            Self::DroppingNoItem => "dropping_no_item",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum StatuePose {
    #[default]
    Standing,
    Sitting,
    Running,
    Star,
}

impl StatuePose {
    pub const ALL: [Self; 4] = [Self::Standing, Self::Sitting, Self::Running, Self::Star];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Standing => "standing",
            Self::Sitting => "sitting",
            Self::Running => "running",
            Self::Star => "star",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|pose| pose.name() == name)
    }

    pub const fn ordinal(self) -> i32 {
        match self {
            Self::Standing => 0,
            Self::Sitting => 1,
            Self::Running => 2,
            Self::Star => 3,
        }
    }

    pub const fn next(self) -> Self {
        match self {
            Self::Standing => Self::Sitting,
            Self::Sitting => Self::Running,
            Self::Running => Self::Star,
            Self::Star => Self::Standing,
        }
    }

    pub fn random(random: &mut impl RandomSource) -> Self {
        Self::ALL[random.next_int(Self::ALL.len() as i32) as usize]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CopperGolemState {
    pub next_weathering_tick: i64,
    pub last_lightning_bolt: Option<Uuid>,
    pub opened_chest: Option<BlockPos>,
    pub spawn_sound_pending: bool,
}

impl Default for CopperGolemState {
    fn default() -> Self {
        Self {
            next_weathering_tick: WEATHERING_UNSET,
            last_lightning_bolt: None,
            opened_chest: None,
            spawn_sound_pending: false,
        }
    }
}

pub fn define(base: &mut EntityBase) -> CopperGolemState {
    base.data.define(
        DATA_WEATHER_STATE,
        SyncedValue::State(StateKind::WeatheringCopper, WeatherState::Unaffected.id()),
    );
    base.data.define(
        DATA_GOLEM_STATE,
        SyncedValue::State(StateKind::CopperGolem, GolemState::Idle.id()),
    );
    CopperGolemState::default()
}

pub fn weather_state(base: &EntityBase) -> WeatherState {
    WeatherState::from_id(base.data.state(DATA_WEATHER_STATE))
}

pub fn set_weather_state(base: &mut EntityBase, state: WeatherState) {
    base.data.set(
        DATA_WEATHER_STATE,
        SyncedValue::State(StateKind::WeatheringCopper, state.id()),
    );
}

pub fn golem_state(base: &EntityBase) -> GolemState {
    GolemState::from_id(base.data.state(DATA_GOLEM_STATE))
}

pub fn set_golem_state(base: &mut EntityBase, state: GolemState) {
    base.data.set(
        DATA_GOLEM_STATE,
        SyncedValue::State(StateKind::CopperGolem, state.id()),
    );
}

pub fn is_waxed(state: &CopperGolemState) -> bool {
    state.next_weathering_tick == WEATHERING_WAXED
}

pub fn wax(state: &mut CopperGolemState) -> bool {
    if is_waxed(state) {
        return false;
    }
    state.next_weathering_tick = WEATHERING_WAXED;
    true
}

pub fn unwax(state: &mut CopperGolemState) -> bool {
    if !is_waxed(state) {
        return false;
    }
    state.next_weathering_tick = WEATHERING_UNSET;
    true
}

pub fn scrape(base: &mut EntityBase, state: &mut CopperGolemState) -> Option<i32> {
    if is_waxed(state) {
        unwax(state);
        return Some(WAX_OFF_EVENT);
    }
    let weather = weather_state(base);
    if weather == WeatherState::Unaffected {
        return None;
    }
    state.next_weathering_tick = WEATHERING_UNSET;
    set_weather_state(base, weather.previous());
    Some(SCRAPE_EVENT)
}

pub fn struck_by_lightning(base: &mut EntityBase, state: &mut CopperGolemState, bolt: Uuid) {
    if state.last_lightning_bolt == Some(bolt) {
        return;
    }
    state.last_lightning_bolt = Some(bolt);
    let weather = weather_state(base);
    if weather != WeatherState::Unaffected {
        state.next_weathering_tick = WEATHERING_UNSET;
        set_weather_state(base, weather.previous());
    }
}

pub fn initial_transport_cooldown(random: &mut impl RandomSource) -> i32 {
    random.next_int_between(
        MIN_INITIAL_TRANSPORT_COOLDOWN,
        MAX_INITIAL_TRANSPORT_COOLDOWN,
    )
}

pub fn update_weathering(
    base: &mut EntityBase,
    state: &mut CopperGolemState,
    game_time: i64,
    random: &mut impl RandomSource,
) -> bool {
    if state.next_weathering_tick == WEATHERING_WAXED {
        return false;
    }
    if state.next_weathering_tick == WEATHERING_UNSET {
        state.next_weathering_tick = game_time
            + i64::from(random.next_int_between(MIN_WEATHERING_TICKS, MAX_WEATHERING_TICKS));
        return false;
    }
    let weather = weather_state(base);
    let oxidized = weather == WeatherState::Oxidized;
    if state.next_weathering_tick <= game_time && !oxidized {
        let next = weather.next();
        set_weather_state(base, next);
        state.next_weathering_tick = if next == WeatherState::Oxidized {
            0
        } else {
            state.next_weathering_tick
                + i64::from(random.next_int_between(MIN_WEATHERING_TICKS, MAX_WEATHERING_TICKS))
        };
    }
    oxidized
}

pub fn statue_roll(random: &mut impl RandomSource) -> bool {
    random.next_float() <= STATUE_CHANCE
}

pub fn finalize(
    base: &mut EntityBase,
    living: &mut LivingState,
    group: Option<SpawnGroupData>,
    level: &mut impl RandomSource,
) -> Option<SpawnGroupData> {
    if let Some(Species::CopperGolem(state)) = living.mob.as_mut().map(|mob| &mut mob.species) {
        state.spawn_sound_pending = true;
    }
    mob_finalize(base, living, group, level)
}

pub fn read(nbt: &Compound, base: &mut EntityBase, state: &mut CopperGolemState) {
    state.next_weathering_tick = nbt
        .get_i64(NEXT_WEATHER_AGE_KEY)
        .unwrap_or(WEATHERING_UNSET);
    set_weather_state(
        base,
        nbt.get_str(WEATHER_STATE_KEY)
            .and_then(WeatherState::from_name)
            .unwrap_or_default(),
    );
}

pub fn write(base: &EntityBase, state: &CopperGolemState, out: &mut Compound) {
    out.insert(NEXT_WEATHER_AGE_KEY, state.next_weathering_tick);
    out.insert(WEATHER_STATE_KEY, weather_state(base).name());
}

#[cfg(test)]
mod tests {
    use rspigot_entities::EntityKind;

    use super::*;
    use crate::EntityId;
    use crate::entity::save::{load_entity, save_entity};
    use crate::random::LegacyRandom;

    fn base() -> EntityBase {
        let mut base = EntityBase::new(EntityId(1), Uuid::from_u128(1), EntityKind::CopperGolem);
        define(&mut base);
        base
    }

    #[test]
    fn the_synced_slots_follow_the_vanilla_data_order() {
        assert_eq!([DATA_WEATHER_STATE, DATA_GOLEM_STATE], [16, 17]);
        let base = base();
        assert_eq!(weather_state(&base), WeatherState::Unaffected);
        assert_eq!(golem_state(&base), GolemState::Idle);
    }

    #[test]
    fn weather_states_step_and_clamp_like_the_vanilla_enum() {
        assert_eq!(WeatherState::Unaffected.next(), WeatherState::Exposed);
        assert_eq!(WeatherState::Weathered.next(), WeatherState::Oxidized);
        assert_eq!(WeatherState::Oxidized.next(), WeatherState::Oxidized);
        assert_eq!(WeatherState::Exposed.previous(), WeatherState::Unaffected);
        assert_eq!(
            WeatherState::Unaffected.previous(),
            WeatherState::Unaffected
        );
        assert_eq!(
            WeatherState::from_name("weathered"),
            Some(WeatherState::Weathered)
        );
        assert_eq!(WeatherState::from_name("rusty"), None);
    }

    #[test]
    fn golem_states_map_their_ids_and_fall_back_to_idle() {
        for state in GolemState::ALL {
            assert_eq!(GolemState::from_id(state.id()), state);
        }
        assert_eq!(GolemState::from_id(9), GolemState::Idle);
        assert_eq!(GolemState::DroppingNoItem.name(), "dropping_no_item");
    }

    #[test]
    fn statue_poses_cycle_and_wrap_to_the_first() {
        assert_eq!(StatuePose::Standing.next(), StatuePose::Sitting);
        assert_eq!(StatuePose::Running.next(), StatuePose::Star);
        assert_eq!(StatuePose::Star.next(), StatuePose::Standing);
        assert_eq!(StatuePose::Star.ordinal(), 3);
    }

    #[test]
    fn the_first_weathering_tick_only_draws_the_next_age() {
        let mut base = base();
        let mut state = CopperGolemState::default();
        let mut random = LegacyRandom::new(5);
        assert!(!update_weathering(
            &mut base,
            &mut state,
            1_000,
            &mut random
        ));
        assert!(state.next_weathering_tick >= 1_000 + i64::from(MIN_WEATHERING_TICKS));
        assert!(state.next_weathering_tick <= 1_000 + i64::from(MAX_WEATHERING_TICKS));
        assert_eq!(weather_state(&base), WeatherState::Unaffected);
    }

    #[test]
    fn a_due_golem_oxidizes_one_stage_and_redraws_until_oxidized() {
        let mut base = base();
        let mut state = CopperGolemState {
            next_weathering_tick: 100,
            ..CopperGolemState::default()
        };
        let mut random = LegacyRandom::new(5);
        assert!(!update_weathering(&mut base, &mut state, 99, &mut random));
        assert_eq!(weather_state(&base), WeatherState::Unaffected);
        assert!(!update_weathering(&mut base, &mut state, 100, &mut random));
        assert_eq!(weather_state(&base), WeatherState::Exposed);
        let after = state.next_weathering_tick;
        assert!(after >= 100 + i64::from(MIN_WEATHERING_TICKS));
        assert!(after <= 100 + i64::from(MAX_WEATHERING_TICKS));
        set_weather_state(&mut base, WeatherState::Weathered);
        assert!(!update_weathering(
            &mut base,
            &mut state,
            after,
            &mut random
        ));
        assert_eq!(weather_state(&base), WeatherState::Oxidized);
        assert_eq!(state.next_weathering_tick, 0);
    }

    #[test]
    fn only_an_already_oxidized_golem_asks_for_a_statue() {
        let mut base = base();
        set_weather_state(&mut base, WeatherState::Oxidized);
        let mut state = CopperGolemState {
            next_weathering_tick: 0,
            ..CopperGolemState::default()
        };
        let mut random = LegacyRandom::new(5);
        assert!(update_weathering(&mut base, &mut state, 5, &mut random));
        assert_eq!(state.next_weathering_tick, 0);
    }

    #[test]
    fn a_waxed_golem_never_weathers() {
        let mut base = base();
        let mut state = CopperGolemState {
            next_weathering_tick: WEATHERING_WAXED,
            ..CopperGolemState::default()
        };
        let mut random = LegacyRandom::new(5);
        assert!(!update_weathering(
            &mut base,
            &mut state,
            i64::MAX,
            &mut random
        ));
        assert_eq!(state.next_weathering_tick, WEATHERING_WAXED);
        assert_eq!(weather_state(&base), WeatherState::Unaffected);
    }

    #[test]
    fn scraping_removes_wax_first_then_one_oxidation_stage() {
        let mut base = base();
        let mut state = CopperGolemState::default();
        assert_eq!(scrape(&mut base, &mut state), None);
        wax(&mut state);
        set_weather_state(&mut base, WeatherState::Weathered);
        assert_eq!(scrape(&mut base, &mut state), Some(WAX_OFF_EVENT));
        assert_eq!(state.next_weathering_tick, WEATHERING_UNSET);
        assert_eq!(weather_state(&base), WeatherState::Weathered);
        assert_eq!(scrape(&mut base, &mut state), Some(SCRAPE_EVENT));
        assert_eq!(weather_state(&base), WeatherState::Exposed);
        assert!(!unwax(&mut state));
        assert!(wax(&mut state));
        assert!(!wax(&mut state));
    }

    #[test]
    fn a_lightning_bolt_cleans_one_stage_once_per_bolt() {
        let mut base = base();
        set_weather_state(&mut base, WeatherState::Oxidized);
        let mut state = CopperGolemState {
            next_weathering_tick: 77,
            ..CopperGolemState::default()
        };
        let bolt = Uuid::from_u128(9);
        struck_by_lightning(&mut base, &mut state, bolt);
        assert_eq!(weather_state(&base), WeatherState::Weathered);
        assert_eq!(state.next_weathering_tick, WEATHERING_UNSET);
        struck_by_lightning(&mut base, &mut state, bolt);
        assert_eq!(weather_state(&base), WeatherState::Weathered);
        struck_by_lightning(&mut base, &mut state, Uuid::from_u128(10));
        assert_eq!(weather_state(&base), WeatherState::Exposed);
    }

    #[test]
    fn weather_and_next_age_round_trip_through_nbt() {
        let nbt = Compound::new()
            .with("id", "minecraft:copper_golem")
            .with(NEXT_WEATHER_AGE_KEY, 1234_i64)
            .with(WEATHER_STATE_KEY, "weathered");
        let entity = load_entity(&nbt, EntityId(1), Uuid::from_u128(1)).unwrap();
        assert_eq!(weather_state(&entity.base), WeatherState::Weathered);
        let saved = save_entity(&entity, None).unwrap();
        assert_eq!(saved.get_i64(NEXT_WEATHER_AGE_KEY), Some(1234));
        assert_eq!(saved.get_str(WEATHER_STATE_KEY), Some("weathered"));
        let blank = load_entity(
            &Compound::new().with("id", "minecraft:copper_golem"),
            EntityId(1),
            Uuid::from_u128(1),
        )
        .unwrap();
        let saved = save_entity(&blank, None).unwrap();
        assert_eq!(saved.get_i64(NEXT_WEATHER_AGE_KEY), Some(WEATHERING_UNSET));
        assert_eq!(saved.get_str(WEATHER_STATE_KEY), Some("unaffected"));
    }

    #[test]
    fn the_statue_roll_uses_the_vanilla_threshold() {
        let mut random = LegacyRandom::new(1);
        let hits = (0..20_000).filter(|_| statue_roll(&mut random)).count();
        assert!(hits > 60 && hits < 180, "{hits}");
    }
}
