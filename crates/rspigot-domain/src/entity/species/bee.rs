use rspigot_nbt::{Compound, Tag};

use crate::BlockPos;
use crate::entity::base::EntityBase;
use crate::entity::data::SyncedValue;
use crate::random::{LegacyRandom, RandomSource};

pub const DATA_FLAGS: u8 = 17;
pub const DATA_ANGER_END_TIME: u8 = 18;
pub const FLAG_ROLL: u8 = 2;
pub const FLAG_STUNG: u8 = 4;
pub const FLAG_NECTAR: u8 = 8;
pub const KEYS: &[&str] = &[
    "hive_pos",
    "flower_pos",
    "HasNectar",
    "HasStung",
    "TicksSincePollination",
    "CannotEnterHiveTicks",
    "CropsGrownSincePollination",
];
pub const MAX_HEALTH: f32 = 10.0;
pub const FLYING_SPEED: f64 = 0.6;
pub const MOVEMENT_SPEED: f64 = 0.3;
pub const ATTACK_DAMAGE: f64 = 2.0;
pub const PATH_LENGTH: f32 = 48.0;
pub const FLY_MOVE_MAX_TURN: i32 = 20;
pub const TICKS_PER_FLAP: i32 = 2;
pub const LOCATE_COOLDOWN_MIN: i32 = 20;
pub const LOCATE_COOLDOWN_MAX: i32 = 60;
pub const HIVE_COOLDOWN: i32 = 200;
pub const FLOWER_COOLDOWN: i32 = 200;
pub const TIRED_OF_NECTAR_TICKS: i32 = 3600;
pub const MAX_CROPS_GROWN: i32 = 10;
pub const NECTAR_DRIP_CHANCE: f32 = 0.05;
pub const MAX_DISTANCE_TO_TARGET: i32 = 48;
pub const STAY_OUT_AFTER_REFUSAL: i32 = 400;
pub const STING_SOUND: &str = "minecraft:entity.bee.sting";
pub const POLLINATE_SOUND: &str = "minecraft:entity.bee.pollinate";
pub const DROWN_AFTER_TICKS: i32 = 20;
pub const STING_DEATH_PERIOD: i32 = 5;
pub const STING_DEATH_WINDOW: i32 = 1200;
pub const STING_POISON_NORMAL_SECONDS: i32 = 10;
pub const STING_POISON_HARD_SECONDS: i32 = 18;
pub const ANGER_MIN_SECONDS: i32 = 20;
pub const ANGER_MAX_SECONDS: i32 = 39;
pub const ROLL_DISTANCE_SQR: f64 = 4.0;
pub const HIVE_VALIDATION_INTERVAL: i32 = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BeeState {
    pub hive_pos: Option<BlockPos>,
    pub flower_pos: Option<BlockPos>,
    pub ticks_without_nectar: i32,
    pub stay_out_countdown: i32,
    pub crops_grown: i32,
    pub locate_hive_cooldown: i32,
    pub locate_flower_cooldown: i32,
    pub under_water_ticks: i32,
    pub time_since_sting: i32,
}

impl BeeState {
    pub fn new(random: &mut impl RandomSource) -> Self {
        Self {
            hive_pos: None,
            flower_pos: None,
            ticks_without_nectar: 0,
            stay_out_countdown: 0,
            crops_grown: 0,
            locate_hive_cooldown: 0,
            locate_flower_cooldown: random
                .next_int_between(LOCATE_COOLDOWN_MIN, LOCATE_COOLDOWN_MAX),
            under_water_ticks: 0,
            time_since_sting: 0,
        }
    }

    pub fn drop_hive(&mut self) {
        self.hive_pos = None;
        self.locate_hive_cooldown = HIVE_COOLDOWN;
    }

    pub fn drop_flower(&mut self, random: &mut impl RandomSource) {
        self.flower_pos = None;
        self.locate_flower_cooldown =
            random.next_int_between(LOCATE_COOLDOWN_MIN, LOCATE_COOLDOWN_MAX);
    }

    pub fn tick_cooldowns(&mut self) {
        if self.stay_out_countdown > 0 {
            self.stay_out_countdown -= 1;
        }
        if self.locate_hive_cooldown > 0 {
            self.locate_hive_cooldown -= 1;
        }
        if self.locate_flower_cooldown > 0 {
            self.locate_flower_cooldown -= 1;
        }
    }

    pub fn is_tired_of_looking_for_nectar(&self) -> bool {
        self.ticks_without_nectar > TIRED_OF_NECTAR_TICKS
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ServerStep {
    pub drown: bool,
    pub die_of_sting: bool,
}

pub fn server_step(
    state: &mut BeeState,
    base: &EntityBase,
    in_water: bool,
    random: &mut impl RandomSource,
) -> ServerStep {
    let mut step = ServerStep::default();
    if in_water {
        state.under_water_ticks += 1;
    } else {
        state.under_water_ticks = 0;
    }
    if state.under_water_ticks > DROWN_AFTER_TICKS {
        step.drown = true;
    }
    if has_stung(base) {
        state.time_since_sting += 1;
        if state.time_since_sting % STING_DEATH_PERIOD == 0 {
            let window = (STING_DEATH_WINDOW - state.time_since_sting).clamp(1, STING_DEATH_WINDOW);
            if random.next_int(window) == 0 {
                step.die_of_sting = true;
            }
        }
    }
    if !has_nectar(base) {
        state.ticks_without_nectar += 1;
    }
    step
}

pub fn sting_poison_seconds(difficulty: crate::world::Difficulty) -> i32 {
    match difficulty {
        crate::world::Difficulty::Normal => STING_POISON_NORMAL_SECONDS,
        crate::world::Difficulty::Hard => STING_POISON_HARD_SECONDS,
        _ => 0,
    }
}

pub fn anger_ticks(random: &mut impl RandomSource) -> i32 {
    random.next_int_between(ANGER_MIN_SECONDS, ANGER_MAX_SECONDS) * 20
}

pub fn define(base: &mut EntityBase) -> BeeState {
    base.data.define(DATA_FLAGS, SyncedValue::Byte(0));
    base.data.define(DATA_ANGER_END_TIME, SyncedValue::Long(-1));
    let seed = base.uuid.as_u64_pair().0 as i64 ^ i64::from(base.id.0);
    BeeState::new(&mut LegacyRandom::new(seed))
}

fn flags(base: &EntityBase) -> u8 {
    match base.data.get(DATA_FLAGS) {
        Some(SyncedValue::Byte(flags)) => *flags as u8,
        _ => 0,
    }
}

fn flag(base: &EntityBase, flag: u8) -> bool {
    flags(base) & flag != 0
}

fn set_flag(base: &mut EntityBase, flag: u8, value: bool) {
    let flags = flags(base);
    let flags = if value { flags | flag } else { flags & !flag };
    base.data.set(DATA_FLAGS, SyncedValue::Byte(flags as i8));
}

pub fn has_nectar(base: &EntityBase) -> bool {
    flag(base, FLAG_NECTAR)
}

pub fn set_has_nectar(base: &mut EntityBase, state: &mut BeeState, value: bool) {
    if value {
        state.ticks_without_nectar = 0;
    }
    set_flag(base, FLAG_NECTAR, value);
}

pub fn has_stung(base: &EntityBase) -> bool {
    flag(base, FLAG_STUNG)
}

pub fn set_has_stung(base: &mut EntityBase, value: bool) {
    set_flag(base, FLAG_STUNG, value);
}

pub fn is_rolling(base: &EntityBase) -> bool {
    flag(base, FLAG_ROLL)
}

pub fn set_rolling(base: &mut EntityBase, value: bool) {
    set_flag(base, FLAG_ROLL, value);
}

pub fn drop_off_nectar(base: &mut EntityBase, state: &mut BeeState) {
    set_has_nectar(base, state, false);
    state.crops_grown = 0;
}

fn pos_tag(pos: BlockPos) -> Tag {
    Tag::IntArray(vec![pos.x, pos.y, pos.z])
}

fn read_pos(nbt: &Compound, key: &str) -> Option<BlockPos> {
    nbt.get_int_array(key)
        .filter(|values| values.len() == 3)
        .map(|values| BlockPos::new(values[0], values[1], values[2]))
}

pub fn read(nbt: &Compound, base: &mut EntityBase, state: &mut BeeState) {
    set_has_nectar(base, state, nbt.get_bool("HasNectar").unwrap_or(false));
    set_has_stung(base, nbt.get_bool("HasStung").unwrap_or(false));
    state.ticks_without_nectar = nbt.get_i32("TicksSincePollination").unwrap_or(0);
    state.stay_out_countdown = nbt.get_i32("CannotEnterHiveTicks").unwrap_or(0);
    state.crops_grown = nbt.get_i32("CropsGrownSincePollination").unwrap_or(0);
    state.hive_pos = read_pos(nbt, "hive_pos");
    state.flower_pos = read_pos(nbt, "flower_pos");
}

pub fn write(base: &EntityBase, state: &BeeState, out: &mut Compound) {
    if let Some(pos) = state.hive_pos {
        out.insert("hive_pos", pos_tag(pos));
    }
    if let Some(pos) = state.flower_pos {
        out.insert("flower_pos", pos_tag(pos));
    }
    out.insert("HasNectar", has_nectar(base));
    out.insert("HasStung", has_stung(base));
    out.insert("TicksSincePollination", state.ticks_without_nectar);
    out.insert("CannotEnterHiveTicks", state.stay_out_countdown);
    out.insert("CropsGrownSincePollination", state.crops_grown);
}

#[cfg(test)]
mod tests {
    use rspigot_nbt::Compound;
    use uuid::Uuid;

    use super::*;
    use crate::EntityId;
    use crate::entity::save::{load_entity, save_entity};
    use crate::random::LegacyRandom;

    fn bee(nbt: Compound) -> crate::entity::Entity {
        load_entity(
            &nbt.with("id", "minecraft:bee"),
            EntityId(1),
            Uuid::from_u128(1),
        )
        .unwrap()
    }

    fn state_of(entity: &crate::entity::Entity) -> BeeState {
        match entity
            .living()
            .and_then(|l| l.mob.as_ref())
            .unwrap()
            .species
        {
            crate::entity::species::Species::Bee(state) => state,
            _ => panic!("not a bee"),
        }
    }

    #[test]
    fn the_flags_and_the_anger_time_sit_at_indices_seventeen_and_eighteen() {
        let entity = bee(Compound::new());
        assert_eq!(
            entity.base.data.get(DATA_FLAGS),
            Some(&SyncedValue::Byte(0))
        );
        assert_eq!(
            entity.base.data.get(DATA_ANGER_END_TIME),
            Some(&SyncedValue::Long(-1))
        );
    }

    #[test]
    fn the_vanilla_layout_round_trips() {
        let nbt = Compound::new()
            .with("HasNectar", true)
            .with("HasStung", true)
            .with("TicksSincePollination", 77)
            .with("CannotEnterHiveTicks", 12)
            .with("CropsGrownSincePollination", 3)
            .with("hive_pos", Tag::IntArray(vec![1, 2, 3]))
            .with("flower_pos", Tag::IntArray(vec![4, 5, 6]));
        let entity = bee(nbt);
        assert!(has_nectar(&entity.base));
        assert!(has_stung(&entity.base));
        let state = state_of(&entity);
        assert_eq!(state.ticks_without_nectar, 77);
        assert_eq!(state.stay_out_countdown, 12);
        assert_eq!(state.crops_grown, 3);
        assert_eq!(state.hive_pos, Some(BlockPos::new(1, 2, 3)));
        assert_eq!(state.flower_pos, Some(BlockPos::new(4, 5, 6)));
        let saved = save_entity(&entity, None).unwrap();
        assert_eq!(saved.get_bool("HasNectar"), Some(true));
        assert_eq!(saved.get_bool("HasStung"), Some(true));
        assert_eq!(saved.get_i32("CannotEnterHiveTicks"), Some(12));
        assert_eq!(saved.get_i32("CropsGrownSincePollination"), Some(3));
        assert_eq!(saved.get("hive_pos"), Some(&Tag::IntArray(vec![1, 2, 3])));
        assert_eq!(saved.get("flower_pos"), Some(&Tag::IntArray(vec![4, 5, 6])));
        assert!(saved.contains_key("TicksSincePollination"));
    }

    #[test]
    fn an_empty_bee_saves_the_flags_and_counters_without_positions() {
        let saved = save_entity(&bee(Compound::new()), None).unwrap();
        assert_eq!(saved.get_bool("HasNectar"), Some(false));
        assert_eq!(saved.get_bool("HasStung"), Some(false));
        assert_eq!(saved.get_i32("TicksSincePollination"), Some(0));
        assert!(!saved.contains_key("hive_pos"));
        assert!(!saved.contains_key("flower_pos"));
    }

    #[test]
    fn nectar_resets_the_ticks_since_pollination() {
        let mut entity = bee(Compound::new());
        let mut state = state_of(&entity);
        state.ticks_without_nectar = 500;
        set_has_nectar(&mut entity.base, &mut state, true);
        assert_eq!(state.ticks_without_nectar, 0);
        state.crops_grown = 4;
        drop_off_nectar(&mut entity.base, &mut state);
        assert!(!has_nectar(&entity.base));
        assert_eq!(state.crops_grown, 0);
    }

    #[test]
    fn a_bee_gets_tired_of_looking_for_nectar_after_three_thousand_six_hundred_ticks() {
        let mut state = BeeState::new(&mut LegacyRandom::new(1));
        state.ticks_without_nectar = 3600;
        assert!(!state.is_tired_of_looking_for_nectar());
        state.ticks_without_nectar = 3601;
        assert!(state.is_tired_of_looking_for_nectar());
    }

    #[test]
    fn dropping_the_hive_and_the_flower_sets_their_cooldowns() {
        let mut random = LegacyRandom::new(5);
        let mut state = BeeState::new(&mut random);
        assert!((20..=60).contains(&state.locate_flower_cooldown));
        state.hive_pos = Some(BlockPos::new(0, 0, 0));
        state.flower_pos = Some(BlockPos::new(0, 0, 0));
        state.drop_hive();
        assert_eq!((state.hive_pos, state.locate_hive_cooldown), (None, 200));
        state.drop_flower(&mut random);
        assert_eq!(state.flower_pos, None);
        assert!((20..=60).contains(&state.locate_flower_cooldown));
    }

    #[test]
    fn the_cooldowns_count_down_to_zero() {
        let mut state = BeeState::new(&mut LegacyRandom::new(1));
        state.stay_out_countdown = 1;
        state.locate_hive_cooldown = 2;
        state.locate_flower_cooldown = 0;
        state.tick_cooldowns();
        state.tick_cooldowns();
        assert_eq!(
            (
                state.stay_out_countdown,
                state.locate_hive_cooldown,
                state.locate_flower_cooldown
            ),
            (0, 0, 0)
        );
    }

    #[test]
    fn a_bee_in_water_drowns_after_twenty_ticks_and_counts_the_ticks_without_nectar() {
        let mut entity = bee(Compound::new());
        let mut state = state_of(&entity);
        let mut random = LegacyRandom::new(1);
        let mut drowned_at = None;
        for tick in 1..=25 {
            let step = server_step(&mut state, &entity.base, true, &mut random);
            if step.drown && drowned_at.is_none() {
                drowned_at = Some(tick);
            }
        }
        assert_eq!(drowned_at, Some(21));
        assert_eq!(state.ticks_without_nectar, 25);
        server_step(&mut state, &entity.base, false, &mut random);
        assert_eq!(state.under_water_ticks, 0);
        set_has_nectar(&mut entity.base, &mut state, true);
        let before = state.ticks_without_nectar;
        server_step(&mut state, &entity.base, false, &mut random);
        assert_eq!(state.ticks_without_nectar, before);
    }

    #[test]
    fn a_stung_bee_dies_at_random_with_a_window_that_shrinks() {
        let mut entity = bee(Compound::new());
        let mut state = state_of(&entity);
        set_has_stung(&mut entity.base, true);
        let mut random = LegacyRandom::new(2);
        let mut died = false;
        for _ in 0..1300 {
            if server_step(&mut state, &entity.base, false, &mut random).die_of_sting {
                died = true;
                break;
            }
        }
        assert!(died);
        state.time_since_sting = 1195;
        let step = server_step(&mut state, &entity.base, false, &mut random);
        assert!(step.die_of_sting || state.time_since_sting == 1196);
        state.time_since_sting = 1199;
        assert!(server_step(&mut state, &entity.base, false, &mut random).die_of_sting);
    }

    #[test]
    fn the_sting_poison_depends_on_the_difficulty() {
        use crate::world::Difficulty;
        assert_eq!(sting_poison_seconds(Difficulty::Peaceful), 0);
        assert_eq!(sting_poison_seconds(Difficulty::Easy), 0);
        assert_eq!(sting_poison_seconds(Difficulty::Normal), 10);
        assert_eq!(sting_poison_seconds(Difficulty::Hard), 18);
    }

    #[test]
    fn the_anger_lasts_between_twenty_and_thirty_nine_seconds() {
        let mut random = LegacyRandom::new(3);
        for _ in 0..200 {
            let ticks = anger_ticks(&mut random);
            assert!((400..=780).contains(&ticks));
        }
    }

    #[test]
    fn the_anger_round_trips_and_is_synced_at_index_eighteen() {
        let entity = bee(Compound::new().with("anger_end_time", 5000i64).with(
            "angry_at",
            crate::entity::save::uuid_tag(Uuid::from_u128(5)),
        ));
        assert_eq!(
            entity.base.data.get(DATA_ANGER_END_TIME),
            Some(&SyncedValue::Long(5000))
        );
        let saved = save_entity(&entity, None).unwrap();
        assert_eq!(saved.get_i64("anger_end_time"), Some(5000));
    }
}
