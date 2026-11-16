use rspigot_data::trial_spawner as registry;
use rspigot_nbt::{Compound, Tag};
use uuid::Uuid;

use crate::entity::save::{read_uuid, uuid_tag};
use crate::random::RandomSource;
use crate::spawner::SpawnData;

pub const ENTITY: &str = "trial_spawner";
pub const DETECT_INTERVAL: i64 = 20;
pub const MOB_TRACKING_DISTANCE_SQR: i32 = 47 * 47;
pub const EJECTION_INTERVAL: f32 = 30.0;
pub const SHUTTER_DELAY: f32 = 40.0;
pub const ITEM_SPAWNER_INTERVAL: i64 = 160;
pub const DETECTION_GRACE: i64 = 40;
pub const DEFAULT_COOLDOWN: i32 = 36000;
pub const DEFAULT_PLAYER_RANGE: i32 = 14;
pub const BAD_OMEN_TO_TRIAL_OMEN_TICKS: i32 = 18000;
pub const SPAWN_MOB_AT_EVENT: i32 = 3011;
pub const SPAWN_MOB_EVENT: i32 = 3012;
pub const DETECT_PLAYER_EVENT: i32 = 3013;
pub const EJECT_ITEM_EVENT: i32 = 3014;
pub const DETECT_OMINOUS_PLAYER_EVENT: i32 = 3019;
pub const BECOME_OMINOUS_EVENT: i32 = 3020;
pub const ITEM_SPAWNED_EVENT: i32 = 3021;
pub const OPEN_SHUTTER_SOUND: &str = "minecraft:block.trial_spawner.open_shutter";
pub const CLOSE_SHUTTER_SOUND: &str = "minecraft:block.trial_spawner.close_shutter";
pub const SPAWN_ITEM_BEGIN_SOUND: &str = "minecraft:block.trial_spawner.spawn_item_begin";
pub const ABOUT_TO_SPAWN_ITEM_SOUND: &str =
    "minecraft:entity.ominous_item_spawner.about_to_spawn_item";
pub const DEFAULT_CONSUMABLES: &str = "minecraft:spawners/trial_chamber/consumables";
pub const DEFAULT_KEY: &str = "minecraft:spawners/trial_chamber/key";
pub const DEFAULT_OMINOUS_ITEMS: &str =
    "minecraft:spawners/trial_chamber/items_to_drop_when_ominous";

const REGISTERED_PLAYERS: &str = "registered_players";
const CURRENT_MOBS: &str = "current_mobs";
const COOLDOWN_ENDS_AT: &str = "cooldown_ends_at";
const NEXT_MOB_SPAWNS_AT: &str = "next_mob_spawns_at";
const TOTAL_MOBS_SPAWNED: &str = "total_mobs_spawned";
const SPAWN_DATA: &str = "spawn_data";
const EJECTING_LOOT_TABLE: &str = "ejecting_loot_table";
const NORMAL_CONFIG: &str = "normal_config";
const OMINOUS_CONFIG: &str = "ominous_config";
const TARGET_COOLDOWN_LENGTH: &str = "target_cooldown_length";
const REQUIRED_PLAYER_RANGE: &str = "required_player_range";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Inactive,
    WaitingForPlayers,
    Active,
    WaitingForRewardEjection,
    EjectingReward,
    Cooldown,
}

impl State {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Inactive => "inactive",
            Self::WaitingForPlayers => "waiting_for_players",
            Self::Active => "active",
            Self::WaitingForRewardEjection => "waiting_for_reward_ejection",
            Self::EjectingReward => "ejecting_reward",
            Self::Cooldown => "cooldown",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "inactive" => Self::Inactive,
            "waiting_for_players" => Self::WaitingForPlayers,
            "active" => Self::Active,
            "waiting_for_reward_ejection" => Self::WaitingForRewardEjection,
            "ejecting_reward" => Self::EjectingReward,
            "cooldown" => Self::Cooldown,
            _ => return None,
        })
    }

    pub const fn is_capable_of_spawning(self) -> bool {
        matches!(self, Self::WaitingForPlayers | Self::Active)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flame {
    Normal,
    Ominous,
}

impl Flame {
    pub const fn of(ominous: bool) -> Self {
        if ominous { Self::Ominous } else { Self::Normal }
    }

    pub const fn encode(self) -> i32 {
        match self {
            Self::Normal => 0,
            Self::Ominous => 1,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub spawn_range: i32,
    pub total_mobs: f32,
    pub simultaneous_mobs: f32,
    pub total_mobs_added_per_player: f32,
    pub simultaneous_mobs_added_per_player: f32,
    pub ticks_between_spawn: i32,
    pub spawn_potentials: Vec<(SpawnData, i32)>,
    pub loot_tables_to_eject: Vec<(String, i32)>,
    pub items_to_drop_when_ominous: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            spawn_range: 4,
            total_mobs: 6.0,
            simultaneous_mobs: 2.0,
            total_mobs_added_per_player: 2.0,
            simultaneous_mobs_added_per_player: 1.0,
            ticks_between_spawn: 40,
            spawn_potentials: Vec::new(),
            loot_tables_to_eject: vec![
                (DEFAULT_CONSUMABLES.to_owned(), 1),
                (DEFAULT_KEY.to_owned(), 1),
            ],
            items_to_drop_when_ominous: DEFAULT_OMINOUS_ITEMS.to_owned(),
        }
    }
}

fn pick<'a, T>(entries: &'a [(T, i32)], random: &mut impl RandomSource) -> Option<&'a T> {
    let total: i32 = entries.iter().map(|(_, weight)| *weight).sum();
    if total <= 0 {
        return None;
    }
    let mut roll = random.next_int(total);
    for (value, weight) in entries {
        roll -= weight;
        if roll < 0 {
            return Some(value);
        }
    }
    None
}

fn float_of(tag: &Compound, key: &str, default: f32) -> f32 {
    tag.get_f64(key).map_or(default, |value| value as f32)
}

impl Config {
    pub fn of_registry(entry: &registry::Config) -> Self {
        let base = Self::default();
        Self {
            spawn_range: entry.spawn_range.unwrap_or(base.spawn_range),
            total_mobs: entry.total_mobs.unwrap_or(base.total_mobs),
            simultaneous_mobs: entry.simultaneous_mobs.unwrap_or(base.simultaneous_mobs),
            total_mobs_added_per_player: entry
                .total_mobs_added_per_player
                .unwrap_or(base.total_mobs_added_per_player),
            simultaneous_mobs_added_per_player: entry
                .simultaneous_mobs_added_per_player
                .unwrap_or(base.simultaneous_mobs_added_per_player),
            ticks_between_spawn: entry
                .ticks_between_spawn
                .unwrap_or(base.ticks_between_spawn),
            spawn_potentials: entry
                .spawn_potentials
                .iter()
                .filter_map(|spawn| {
                    let (entity, _) = rspigot_nbt::parse_compound(spawn.entity).ok()?;
                    let data = SpawnData {
                        entity,
                        ..SpawnData::default()
                    };
                    Some((data, spawn.weight))
                })
                .collect(),
            loot_tables_to_eject: entry.loot_tables_to_eject.map_or(
                base.loot_tables_to_eject,
                |tables| {
                    tables
                        .iter()
                        .map(|(name, weight)| ((*name).to_owned(), *weight))
                        .collect()
                },
            ),
            items_to_drop_when_ominous: entry
                .items_to_drop_when_ominous
                .map_or(base.items_to_drop_when_ominous, str::to_owned),
        }
    }

    pub fn read(tag: &Compound) -> Self {
        let base = Self::default();
        let potentials = tag
            .get_list("spawn_potentials")
            .map_or(base.spawn_potentials, |list| {
                list.iter()
                    .filter_map(Tag::as_compound)
                    .filter_map(|entry| {
                        let data = entry.get_compound("data")?;
                        Some((SpawnData::read(data), entry.get_i32("weight").unwrap_or(1)))
                    })
                    .collect()
            });
        let ejects =
            tag.get_list("loot_tables_to_eject")
                .map_or(base.loot_tables_to_eject, |list| {
                    list.iter()
                        .filter_map(Tag::as_compound)
                        .filter_map(|entry| {
                            let table = entry.get_str("data")?.to_owned();
                            Some((table, entry.get_i32("weight").unwrap_or(1)))
                        })
                        .collect()
                });
        Self {
            spawn_range: tag.get_i32("spawn_range").unwrap_or(base.spawn_range),
            total_mobs: float_of(tag, "total_mobs", base.total_mobs),
            simultaneous_mobs: float_of(tag, "simultaneous_mobs", base.simultaneous_mobs),
            total_mobs_added_per_player: float_of(
                tag,
                "total_mobs_added_per_player",
                base.total_mobs_added_per_player,
            ),
            simultaneous_mobs_added_per_player: float_of(
                tag,
                "simultaneous_mobs_added_per_player",
                base.simultaneous_mobs_added_per_player,
            ),
            ticks_between_spawn: tag
                .get_i32("ticks_between_spawn")
                .unwrap_or(base.ticks_between_spawn),
            spawn_potentials: potentials,
            loot_tables_to_eject: ejects,
            items_to_drop_when_ominous: tag
                .get_str("items_to_drop_when_ominous")
                .map_or(base.items_to_drop_when_ominous, str::to_owned),
        }
    }

    pub fn write(&self) -> Compound {
        let potentials = self
            .spawn_potentials
            .iter()
            .map(|(data, weight)| {
                Tag::Compound(
                    Compound::new()
                        .with("data", data.write())
                        .with("weight", *weight),
                )
            })
            .collect();
        let ejects = self
            .loot_tables_to_eject
            .iter()
            .map(|(table, weight)| {
                Tag::Compound(
                    Compound::new()
                        .with("data", table.as_str())
                        .with("weight", *weight),
                )
            })
            .collect();
        Compound::new()
            .with("spawn_range", self.spawn_range)
            .with("total_mobs", self.total_mobs)
            .with("simultaneous_mobs", self.simultaneous_mobs)
            .with(
                "total_mobs_added_per_player",
                self.total_mobs_added_per_player,
            )
            .with(
                "simultaneous_mobs_added_per_player",
                self.simultaneous_mobs_added_per_player,
            )
            .with("ticks_between_spawn", self.ticks_between_spawn)
            .with("spawn_potentials", Tag::List(potentials))
            .with("loot_tables_to_eject", Tag::List(ejects))
            .with(
                "items_to_drop_when_ominous",
                self.items_to_drop_when_ominous.as_str(),
            )
    }

    pub fn target_total_mobs(&self, additional_players: i32) -> i32 {
        (self.total_mobs + self.total_mobs_added_per_player * additional_players as f32).floor()
            as i32
    }

    pub fn target_simultaneous_mobs(&self, additional_players: i32) -> i32 {
        (self.simultaneous_mobs
            + self.simultaneous_mobs_added_per_player * additional_players as f32)
            .floor() as i32
    }

    pub fn pick_spawn(&self, random: &mut impl RandomSource) -> Option<&SpawnData> {
        pick(&self.spawn_potentials, random)
    }

    pub fn pick_loot_table(&self, random: &mut impl RandomSource) -> Option<&String> {
        pick(&self.loot_tables_to_eject, random)
    }

    pub fn with_entity(&self, id: &str) -> Self {
        Self {
            spawn_potentials: vec![(SpawnData::of_entity(id), 1)],
            ..self.clone()
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Holder {
    Key(String),
    Direct(Config),
}

impl Holder {
    fn read(tag: &Tag) -> Option<Self> {
        match tag {
            Tag::String(name) => Some(Self::Key(name.clone())),
            Tag::Compound(compound) => Some(Self::Direct(Config::read(compound))),
            _ => None,
        }
    }

    fn write(&self) -> Tag {
        match self {
            Self::Key(name) => Tag::String(name.clone()),
            Self::Direct(config) => Tag::Compound(config.write()),
        }
    }

    pub fn resolve(&self) -> Config {
        match self {
            Self::Key(name) => {
                registry::config(name).map_or_else(Config::default, Config::of_registry)
            }
            Self::Direct(config) => config.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FullConfig {
    pub normal: Holder,
    pub ominous: Holder,
    pub target_cooldown_length: i32,
    pub required_player_range: i32,
}

impl Default for FullConfig {
    fn default() -> Self {
        Self {
            normal: Holder::Direct(Config::default()),
            ominous: Holder::Direct(Config::default()),
            target_cooldown_length: DEFAULT_COOLDOWN,
            required_player_range: DEFAULT_PLAYER_RANGE,
        }
    }
}

impl FullConfig {
    pub fn read(tag: &Compound) -> Self {
        let base = Self::default();
        Self {
            normal: tag
                .get(NORMAL_CONFIG)
                .and_then(Holder::read)
                .unwrap_or(base.normal),
            ominous: tag
                .get(OMINOUS_CONFIG)
                .and_then(Holder::read)
                .unwrap_or(base.ominous),
            target_cooldown_length: tag
                .get_i32(TARGET_COOLDOWN_LENGTH)
                .filter(|length| *length >= 0)
                .unwrap_or(base.target_cooldown_length),
            required_player_range: tag
                .get_i32(REQUIRED_PLAYER_RANGE)
                .filter(|range| (1..=128).contains(range))
                .unwrap_or(base.required_player_range),
        }
    }

    pub fn write(&self, tag: &mut Compound) {
        tag.insert(NORMAL_CONFIG, self.normal.write());
        tag.insert(OMINOUS_CONFIG, self.ominous.write());
        tag.insert(TARGET_COOLDOWN_LENGTH, self.target_cooldown_length);
        tag.insert(REQUIRED_PLAYER_RANGE, self.required_player_range);
    }

    pub fn active(&self, ominous: bool) -> Config {
        if ominous {
            self.ominous.resolve()
        } else {
            self.normal.resolve()
        }
    }

    pub fn overriding_entity(&self, id: &str) -> Self {
        Self {
            normal: Holder::Direct(self.normal.resolve().with_entity(id)),
            ominous: Holder::Direct(self.ominous.resolve().with_entity(id)),
            ..self.clone()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Data {
    pub detected_players: Vec<Uuid>,
    pub current_mobs: Vec<Uuid>,
    pub cooldown_ends_at: i64,
    pub next_mob_spawns_at: i64,
    pub total_mobs_spawned: i32,
    pub next_spawn_data: Option<SpawnData>,
    pub ejecting_loot_table: Option<String>,
}

fn read_uuids(tag: &Compound, key: &str) -> Vec<Uuid> {
    tag.get_list(key)
        .unwrap_or_default()
        .iter()
        .filter_map(read_uuid)
        .collect()
}

fn write_uuids(tag: &mut Compound, key: &str, uuids: &[Uuid]) {
    tag.insert(
        key,
        Tag::List(uuids.iter().copied().map(uuid_tag).collect()),
    );
}

impl Data {
    pub fn read(tag: &Compound) -> Self {
        Self {
            detected_players: read_uuids(tag, REGISTERED_PLAYERS),
            current_mobs: read_uuids(tag, CURRENT_MOBS),
            cooldown_ends_at: tag.get_i64(COOLDOWN_ENDS_AT).unwrap_or(0),
            next_mob_spawns_at: tag.get_i64(NEXT_MOB_SPAWNS_AT).unwrap_or(0),
            total_mobs_spawned: tag.get_i32(TOTAL_MOBS_SPAWNED).unwrap_or(0),
            next_spawn_data: tag.get_compound(SPAWN_DATA).map(SpawnData::read),
            ejecting_loot_table: tag.get_str(EJECTING_LOOT_TABLE).map(str::to_owned),
        }
    }

    pub fn write(&self, tag: &mut Compound) {
        write_uuids(tag, REGISTERED_PLAYERS, &self.detected_players);
        write_uuids(tag, CURRENT_MOBS, &self.current_mobs);
        tag.insert(COOLDOWN_ENDS_AT, self.cooldown_ends_at);
        tag.insert(NEXT_MOB_SPAWNS_AT, self.next_mob_spawns_at);
        tag.insert(TOTAL_MOBS_SPAWNED, self.total_mobs_spawned);
        match &self.next_spawn_data {
            Some(data) => tag.insert(SPAWN_DATA, data.write()),
            None => tag.remove(SPAWN_DATA),
        };
        match &self.ejecting_loot_table {
            Some(table) => tag.insert(EJECTING_LOOT_TABLE, table.as_str()),
            None => tag.remove(EJECTING_LOOT_TABLE),
        };
    }

    pub fn update_tag(&self, state: State) -> Compound {
        let mut tag = Compound::new();
        if state == State::Active {
            tag.insert(NEXT_MOB_SPAWNS_AT, self.next_mob_spawns_at);
        }
        if let Some(data) = &self.next_spawn_data {
            tag.insert(SPAWN_DATA, data.write());
        }
        tag
    }

    pub fn reset_statistics(&mut self) {
        self.detected_players.clear();
        self.total_mobs_spawned = 0;
        self.next_mob_spawns_at = 0;
        self.cooldown_ends_at = 0;
    }

    pub fn reset(&mut self) {
        self.current_mobs.clear();
        self.next_spawn_data = None;
        self.reset_statistics();
    }

    pub fn ensure_next_spawn(&mut self, config: &Config, random: &mut impl RandomSource) -> bool {
        if self.next_spawn_data.is_some() {
            return false;
        }
        let picked = config.pick_spawn(random).cloned().unwrap_or_default();
        self.next_spawn_data = Some(picked);
        true
    }

    pub fn has_mob_to_spawn(&mut self, config: &Config, random: &mut impl RandomSource) -> bool {
        self.ensure_next_spawn(config, random);
        let named = self
            .next_spawn_data
            .as_ref()
            .and_then(SpawnData::entity_id)
            .is_some();
        named || !config.spawn_potentials.is_empty()
    }

    pub fn has_finished_spawning_all_mobs(&self, config: &Config, additional_players: i32) -> bool {
        config.target_total_mobs(additional_players) <= self.total_mobs_spawned
    }

    pub fn have_all_current_mobs_died(&self) -> bool {
        self.current_mobs.is_empty()
    }

    pub fn is_ready_to_spawn_next_mob(
        &self,
        game_time: i64,
        config: &Config,
        additional_players: i32,
    ) -> bool {
        let simultaneous = config.target_simultaneous_mobs(additional_players);
        self.next_mob_spawns_at <= game_time
            && i32::try_from(self.current_mobs.len()).is_ok_and(|alive| alive < simultaneous)
    }

    pub fn count_additional_players(&self) -> i32 {
        i32::try_from(self.detected_players.len().saturating_sub(1)).unwrap_or(i32::MAX)
    }

    pub fn is_ready_to_open_shutter(
        &self,
        game_time: i64,
        delay_before_open: f32,
        target_cooldown_length: i32,
    ) -> bool {
        let start = self.cooldown_ends_at - i64::from(target_cooldown_length);
        start as f32 + delay_before_open <= game_time as f32
    }

    pub fn is_ready_to_eject_items(
        &self,
        game_time: i64,
        time_between_ejections: f32,
        target_cooldown_length: i32,
    ) -> bool {
        let start = self.cooldown_ends_at - i64::from(target_cooldown_length);
        (game_time - start) as f32 % time_between_ejections == 0.0
    }

    pub fn is_cooldown_finished(&self, game_time: i64) -> bool {
        self.cooldown_ends_at <= game_time
    }

    pub fn finish_spawning_wave(&mut self, game_time: i64, target_cooldown_length: i32) {
        self.cooldown_ends_at = game_time + i64::from(target_cooldown_length);
        self.total_mobs_spawned = 0;
        self.next_mob_spawns_at = 0;
    }

    pub fn restart_wave(&mut self) {
        self.total_mobs_spawned = 0;
        self.next_mob_spawns_at = 0;
    }

    pub fn mob_spawned(
        &mut self,
        mob: Uuid,
        game_time: i64,
        config: &Config,
        random: &mut impl RandomSource,
    ) {
        if !self.current_mobs.contains(&mob) {
            self.current_mobs.push(mob);
        }
        self.total_mobs_spawned += 1;
        self.next_mob_spawns_at = game_time + i64::from(config.ticks_between_spawn);
        if let Some(next) = config.pick_spawn(random) {
            self.next_spawn_data = Some(next.clone());
        }
    }

    pub fn lost_track_of_a_mob(&mut self, game_time: i64, config: &Config) {
        self.next_mob_spawns_at = game_time + i64::from(config.ticks_between_spawn);
    }

    pub fn detected(&mut self, players: impl IntoIterator<Item = Uuid>, game_time: i64) -> bool {
        let mut added = false;
        for player in players {
            if !self.detected_players.contains(&player) {
                self.detected_players.push(player);
                added = true;
            }
        }
        if added {
            self.next_mob_spawns_at = (game_time + DETECTION_GRACE).max(self.next_mob_spawns_at);
        }
        added
    }

    pub fn reset_after_becoming_ominous(&mut self, game_time: i64, ominous: &Config) {
        if !ominous.spawn_potentials.is_empty() {
            self.next_spawn_data = None;
        }
        self.total_mobs_spawned = 0;
        self.current_mobs.clear();
        self.next_mob_spawns_at = game_time + i64::from(ominous.ticks_between_spawn);
        self.cooldown_ends_at = game_time + ITEM_SPAWNER_INTERVAL;
    }

    pub fn eject_next_player(&mut self) {
        if !self.detected_players.is_empty() {
            self.detected_players.remove(0);
        }
    }
}

pub fn low_resolution_key(x: i32, y: i32, z: i32) -> (i32, i32, i32) {
    (
        (x as f32 / 30.0).floor() as i32,
        (y as f32 / 20.0).floor() as i32,
        (z as f32 / 30.0).floor() as i32,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Roll(i32);

    impl RandomSource for Roll {
        fn next_float(&mut self) -> f32 {
            0.0
        }
        fn next_int(&mut self, bound: i32) -> i32 {
            self.0.min(bound - 1)
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

    fn zombie_config() -> Config {
        Config {
            spawn_potentials: vec![(SpawnData::of_entity("minecraft:zombie"), 1)],
            ..Config::default()
        }
    }

    #[test]
    fn extra_players_raise_the_targets_by_the_per_player_amounts() {
        let config = Config::default();
        assert_eq!(config.target_total_mobs(0), 6);
        assert_eq!(config.target_total_mobs(2), 10);
        assert_eq!(config.target_simultaneous_mobs(0), 2);
        assert_eq!(config.target_simultaneous_mobs(3), 5);
    }

    #[test]
    fn the_ominous_breeze_scales_with_half_a_mob_per_extra_player() {
        let config = Config::of_registry(
            registry::config("trial_chamber/breeze/ominous").expect("registry config"),
        );
        assert_eq!(config.target_total_mobs(0), 4);
        assert_eq!(config.target_total_mobs(3), 7);
        assert_eq!(config.target_simultaneous_mobs(0), 2);
        assert_eq!(config.target_simultaneous_mobs(2), 3);
        assert_eq!(config.ticks_between_spawn, 20);
        assert_eq!(
            config.spawn_potentials[0].0.entity_id(),
            Some("minecraft:breeze")
        );
    }

    #[test]
    fn only_extra_players_count_as_additional() {
        let mut data = Data::default();
        assert_eq!(data.count_additional_players(), 0);
        data.detected_players.push(Uuid::from_u128(1));
        assert_eq!(data.count_additional_players(), 0);
        data.detected_players.push(Uuid::from_u128(2));
        data.detected_players.push(Uuid::from_u128(3));
        assert_eq!(data.count_additional_players(), 2);
    }

    #[test]
    fn a_new_mob_waits_for_the_delay_and_for_a_free_slot() {
        let config = Config::default();
        let mut data = Data::default();
        data.next_mob_spawns_at = 100;
        assert!(!data.is_ready_to_spawn_next_mob(99, &config, 0));
        assert!(data.is_ready_to_spawn_next_mob(100, &config, 0));
        data.current_mobs = vec![Uuid::from_u128(1), Uuid::from_u128(2)];
        assert!(!data.is_ready_to_spawn_next_mob(100, &config, 0));
        assert!(data.is_ready_to_spawn_next_mob(100, &config, 1));
    }

    #[test]
    fn a_spawned_mob_is_tracked_and_delays_the_next_one() {
        let config = zombie_config();
        let mut data = Data::default();
        data.mob_spawned(Uuid::from_u128(7), 500, &config, &mut Roll(0));
        assert_eq!(data.current_mobs, vec![Uuid::from_u128(7)]);
        assert_eq!(data.total_mobs_spawned, 1);
        assert_eq!(data.next_mob_spawns_at, 540);
        assert_eq!(
            data.next_spawn_data.as_ref().and_then(SpawnData::entity_id),
            Some("minecraft:zombie")
        );
    }

    #[test]
    fn the_wave_is_over_once_every_target_mob_was_spawned() {
        let config = Config::default();
        let mut data = Data::default();
        data.total_mobs_spawned = 5;
        assert!(!data.has_finished_spawning_all_mobs(&config, 0));
        data.total_mobs_spawned = 6;
        assert!(data.has_finished_spawning_all_mobs(&config, 0));
        assert!(!data.has_finished_spawning_all_mobs(&config, 1));
    }

    #[test]
    fn finishing_the_wave_starts_the_cooldown_and_clears_the_counters() {
        let mut data = Data {
            total_mobs_spawned: 6,
            next_mob_spawns_at: 90,
            ..Data::default()
        };
        data.finish_spawning_wave(1000, 36000);
        assert_eq!(data.cooldown_ends_at, 37000);
        assert_eq!((data.total_mobs_spawned, data.next_mob_spawns_at), (0, 0));
    }

    #[test]
    fn the_shutter_opens_forty_ticks_into_the_cooldown_and_items_come_every_thirty() {
        let data = Data {
            cooldown_ends_at: 37000,
            ..Data::default()
        };
        assert!(!data.is_ready_to_open_shutter(1039, SHUTTER_DELAY, 36000));
        assert!(data.is_ready_to_open_shutter(1040, SHUTTER_DELAY, 36000));
        assert!(data.is_ready_to_eject_items(1030, EJECTION_INTERVAL, 36000));
        assert!(!data.is_ready_to_eject_items(1031, EJECTION_INTERVAL, 36000));
        assert!(data.is_ready_to_eject_items(1060, EJECTION_INTERVAL, 36000));
    }

    #[test]
    fn detecting_a_new_player_pushes_the_next_spawn_back_two_seconds() {
        let mut data = Data::default();
        assert!(data.detected([Uuid::from_u128(1)], 100));
        assert_eq!(data.next_mob_spawns_at, 140);
        assert!(!data.detected([Uuid::from_u128(1)], 200));
        assert_eq!(data.next_mob_spawns_at, 140);
        data.next_mob_spawns_at = 500;
        assert!(data.detected([Uuid::from_u128(2)], 200));
        assert_eq!(data.next_mob_spawns_at, 500);
    }

    #[test]
    fn becoming_ominous_forgets_the_mobs_and_schedules_the_first_item() {
        let mut data = Data {
            current_mobs: vec![Uuid::from_u128(1)],
            total_mobs_spawned: 3,
            next_spawn_data: Some(SpawnData::of_entity("minecraft:zombie")),
            ..Data::default()
        };
        data.reset_after_becoming_ominous(1000, &zombie_config());
        assert!(data.current_mobs.is_empty());
        assert_eq!(data.total_mobs_spawned, 0);
        assert_eq!(data.next_mob_spawns_at, 1040);
        assert_eq!(data.cooldown_ends_at, 1160);
        assert!(data.next_spawn_data.is_none());
    }

    #[test]
    fn a_spawner_with_no_potential_has_no_mob_to_spawn() {
        let mut data = Data::default();
        assert!(!data.has_mob_to_spawn(&Config::default(), &mut Roll(0)));
        assert!(data.has_mob_to_spawn(&zombie_config(), &mut Roll(0)));
    }

    #[test]
    fn the_state_data_survives_a_write_and_a_read() {
        let data = Data {
            detected_players: vec![Uuid::from_u128(5), Uuid::from_u128(6)],
            current_mobs: vec![Uuid::from_u128(9)],
            cooldown_ends_at: 123,
            next_mob_spawns_at: 456,
            total_mobs_spawned: 3,
            next_spawn_data: Some(SpawnData::of_entity("minecraft:zombie")),
            ejecting_loot_table: Some(DEFAULT_KEY.to_owned()),
        };
        let mut tag = Compound::new();
        data.write(&mut tag);
        assert_eq!(Data::read(&tag), data);
    }

    #[test]
    fn a_full_config_keeps_registry_keys_and_direct_configs() {
        let mut full = FullConfig {
            normal: Holder::Key("trial_chamber/melee/zombie/normal".to_owned()),
            ominous: Holder::Direct(zombie_config()),
            target_cooldown_length: 1200,
            required_player_range: 20,
        };
        let mut tag = Compound::new();
        full.write(&mut tag);
        assert_eq!(
            tag.get_str(NORMAL_CONFIG),
            Some("trial_chamber/melee/zombie/normal")
        );
        assert_eq!(FullConfig::read(&tag), full);
        full.required_player_range = 14;
        let mut tag = Compound::new();
        full.write(&mut tag);
        assert_eq!(FullConfig::read(&tag).required_player_range, 14);
    }

    #[test]
    fn a_missing_config_reads_as_the_vanilla_default() {
        assert_eq!(FullConfig::read(&Compound::new()), FullConfig::default());
    }

    #[test]
    fn overriding_the_entity_replaces_the_potentials_of_both_configs() {
        let full = FullConfig::default().overriding_entity("minecraft:skeleton");
        for ominous in [false, true] {
            let config = full.active(ominous);
            assert_eq!(config.spawn_potentials.len(), 1);
            assert_eq!(
                config.spawn_potentials[0].0.entity_id(),
                Some("minecraft:skeleton")
            );
            assert_eq!(config.total_mobs, 6.0);
        }
    }

    #[test]
    fn the_states_round_trip_their_names() {
        for state in [
            State::Inactive,
            State::WaitingForPlayers,
            State::Active,
            State::WaitingForRewardEjection,
            State::EjectingReward,
            State::Cooldown,
        ] {
            assert_eq!(State::from_name(state.name()), Some(state));
        }
        assert!(State::Active.is_capable_of_spawning());
        assert!(!State::Cooldown.is_capable_of_spawning());
    }

    #[test]
    fn the_low_resolution_key_floors_each_axis() {
        assert_eq!(low_resolution_key(59, 39, -1), (1, 1, -1));
        assert_eq!(low_resolution_key(0, -1, 30), (0, -1, 1));
    }
}
