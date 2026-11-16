use rspigot_items::ItemKind;
use rspigot_nbt::{Compound, Tag};
use uuid::Uuid;

use crate::entity::save::{item_stack_from_nbt, item_stack_to_nbt, read_uuid, uuid_tag};
use crate::item::ItemStack;

pub const ENTITY: &str = "vault";
pub const DEFAULT_LOOT_TABLE: &str = "minecraft:chests/trial_chambers/reward";
pub const ACTIVATION_RANGE: f64 = 4.0;
pub const DEACTIVATION_RANGE: f64 = 4.5;
pub const MAX_REWARDED_PLAYERS: usize = 128;
pub const STATE_CHECK_INTERVAL: i64 = 20;
pub const UNLOCK_DELAY: i64 = 14;
pub const EJECT_DELAY: i64 = 20;
pub const INSERT_FAIL_COOLDOWN: i64 = 15;
pub const DISPLAY_CYCLE_INTERVAL: i64 = 20;
pub const DEACTIVATE_EVENT: i32 = 3016;
pub const ACTIVATE_EVENT: i32 = 3015;
pub const EJECT_ITEM_EVENT: i32 = 3017;
pub const INSERT_ITEM_SOUND: &str = "minecraft:block.vault.insert_item";
pub const INSERT_FAIL_SOUND: &str = "minecraft:block.vault.insert_item_fail";
pub const REJECT_SOUND: &str = "minecraft:block.vault.reject_rewarded_player";
pub const EJECT_SOUND: &str = "minecraft:block.vault.eject_item";
pub const OPEN_SHUTTER_SOUND: &str = "minecraft:block.vault.open_shutter";
pub const CLOSE_SHUTTER_SOUND: &str = "minecraft:block.vault.close_shutter";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Inactive,
    Active,
    Unlocking,
    Ejecting,
}

impl State {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Inactive => "inactive",
            Self::Active => "active",
            Self::Unlocking => "unlocking",
            Self::Ejecting => "ejecting",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "inactive" => Self::Inactive,
            "active" => Self::Active,
            "unlocking" => Self::Unlocking,
            "ejecting" => Self::Ejecting,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub loot_table: String,
    pub activation_range: f64,
    pub deactivation_range: f64,
    pub key_item: Option<ItemStack>,
    pub override_loot_table_to_display: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            loot_table: DEFAULT_LOOT_TABLE.to_owned(),
            activation_range: ACTIVATION_RANGE,
            deactivation_range: DEACTIVATION_RANGE,
            key_item: Some(ItemStack::new(
                ItemKind::TrialKey.id(),
                std::num::NonZeroU8::MIN,
            )),
            override_loot_table_to_display: None,
        }
    }
}

impl Config {
    pub fn read(tag: &Compound) -> Self {
        let base = Self::default();
        let key_item = match tag.get_compound("key_item") {
            Some(item) => item_stack_from_nbt(item),
            None => base.key_item,
        };
        Self {
            loot_table: tag
                .get_str("loot_table")
                .map_or(base.loot_table, str::to_owned),
            activation_range: tag
                .get_f64("activation_range")
                .unwrap_or(base.activation_range),
            deactivation_range: tag
                .get_f64("deactivation_range")
                .unwrap_or(base.deactivation_range),
            key_item,
            override_loot_table_to_display: tag
                .get_str("override_loot_table_to_display")
                .map(str::to_owned),
        }
    }

    pub fn write(&self) -> Compound {
        let mut tag = Compound::new()
            .with("loot_table", self.loot_table.as_str())
            .with("activation_range", self.activation_range)
            .with("deactivation_range", self.deactivation_range);
        if let Some(key) = &self.key_item {
            tag.insert("key_item", item_stack_to_nbt(key));
        }
        if let Some(table) = &self.override_loot_table_to_display {
            tag.insert("override_loot_table_to_display", table.as_str());
        }
        tag
    }

    pub fn can_eject_reward(&self, state: State) -> bool {
        self.key_item.is_some() && state != State::Inactive
    }

    pub fn is_valid_key(&self, stack: &ItemStack) -> bool {
        self.key_item.as_ref().is_some_and(|key| {
            stack.item == key.item && stack.components == key.components && stack.count >= key.count
        })
    }

    pub fn displayed_table(&self) -> &str {
        self.override_loot_table_to_display
            .as_deref()
            .unwrap_or(&self.loot_table)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SharedData {
    pub display_item: Option<ItemStack>,
    pub connected_players: Vec<Uuid>,
    pub connected_particles_range: f64,
}

impl Default for SharedData {
    fn default() -> Self {
        Self {
            display_item: None,
            connected_players: Vec::new(),
            connected_particles_range: DEACTIVATION_RANGE,
        }
    }
}

impl SharedData {
    pub fn read(tag: &Compound) -> Self {
        Self {
            display_item: tag
                .get_compound("display_item")
                .and_then(item_stack_from_nbt),
            connected_players: read_uuids(tag, "connected_players"),
            connected_particles_range: tag
                .get_f64("connected_particles_range")
                .unwrap_or(DEACTIVATION_RANGE),
        }
    }

    pub fn write(&self) -> Compound {
        let mut tag = Compound::new();
        if let Some(item) = &self.display_item {
            tag.insert("display_item", item_stack_to_nbt(item));
        }
        write_uuids(&mut tag, "connected_players", &self.connected_players);
        tag.insert("connected_particles_range", self.connected_particles_range);
        tag
    }

    pub fn set_display_item(&mut self, item: Option<ItemStack>) -> bool {
        if self.display_item == item {
            return false;
        }
        self.display_item = item;
        true
    }

    pub fn has_connected_players(&self) -> bool {
        !self.connected_players.is_empty()
    }

    pub fn connect(&mut self, players: Vec<Uuid>) -> bool {
        if self.connected_players == players {
            return false;
        }
        self.connected_players = players;
        true
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ServerData {
    pub rewarded_players: Vec<Uuid>,
    pub state_updating_resumes_at: i64,
    pub items_to_eject: Vec<ItemStack>,
    pub total_ejections_needed: i32,
    pub last_insert_fail: i64,
}

impl ServerData {
    pub fn read(tag: &Compound) -> Self {
        Self {
            rewarded_players: read_uuids(tag, "rewarded_players"),
            state_updating_resumes_at: tag.get_i64("state_updating_resumes_at").unwrap_or(0),
            items_to_eject: tag
                .get_list("items_to_eject")
                .unwrap_or_default()
                .iter()
                .filter_map(Tag::as_compound)
                .filter_map(item_stack_from_nbt)
                .collect(),
            total_ejections_needed: tag.get_i32("total_ejections_needed").unwrap_or(0),
            last_insert_fail: 0,
        }
    }

    pub fn write(&self) -> Compound {
        let mut tag = Compound::new();
        write_uuids(&mut tag, "rewarded_players", &self.rewarded_players);
        tag.insert("state_updating_resumes_at", self.state_updating_resumes_at);
        tag.insert(
            "items_to_eject",
            Tag::List(
                self.items_to_eject
                    .iter()
                    .map(|item| Tag::Compound(item_stack_to_nbt(item)))
                    .collect(),
            ),
        );
        tag.insert("total_ejections_needed", self.total_ejections_needed);
        tag
    }

    pub fn has_rewarded(&self, player: Uuid) -> bool {
        self.rewarded_players.contains(&player)
    }

    pub fn add_rewarded(&mut self, player: Uuid) {
        if !self.rewarded_players.contains(&player) {
            self.rewarded_players.push(player);
        }
        if self.rewarded_players.len() > MAX_REWARDED_PLAYERS {
            self.rewarded_players.remove(0);
        }
    }

    pub fn set_items_to_eject(&mut self, items: Vec<ItemStack>) {
        self.total_ejections_needed = i32::try_from(items.len()).unwrap_or(i32::MAX);
        self.items_to_eject = items;
    }

    pub fn next_item_to_eject(&self) -> Option<&ItemStack> {
        self.items_to_eject.last()
    }

    pub fn pop_next_item_to_eject(&mut self) -> Option<ItemStack> {
        self.items_to_eject.pop()
    }

    pub fn mark_ejection_finished(&mut self) {
        self.total_ejections_needed = 0;
    }

    pub fn ejection_progress(&self) -> f32 {
        if self.total_ejections_needed == 1 {
            return 1.0;
        }
        let left = self.items_to_eject.len() as f32;
        1.0 - (left - 1.0) / (self.total_ejections_needed as f32 - 1.0)
    }

    pub fn can_play_insert_fail(&self, game_time: i64) -> bool {
        self.last_insert_fail + INSERT_FAIL_COOLDOWN <= game_time
    }
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

#[cfg(test)]
mod tests {
    use std::num::NonZeroU8;

    use super::*;

    fn stack(kind: ItemKind, count: u8) -> ItemStack {
        ItemStack::new(kind.id(), NonZeroU8::new(count).unwrap())
    }

    #[test]
    fn the_default_vault_wants_one_trial_key() {
        let config = Config::default();
        assert!(config.is_valid_key(&stack(ItemKind::TrialKey, 1)));
        assert!(config.is_valid_key(&stack(ItemKind::TrialKey, 3)));
        assert!(!config.is_valid_key(&stack(ItemKind::OminousTrialKey, 1)));
    }

    #[test]
    fn a_key_with_other_components_is_not_the_key() {
        let config = Config::default();
        let mut renamed = stack(ItemKind::TrialKey, 1);
        renamed.components.insert("minecraft:custom_name", "x");
        assert!(!config.is_valid_key(&renamed));
    }

    #[test]
    fn an_inactive_vault_or_one_with_no_key_cannot_eject() {
        let mut config = Config::default();
        assert!(!config.can_eject_reward(State::Inactive));
        assert!(config.can_eject_reward(State::Active));
        config.key_item = None;
        assert!(!config.can_eject_reward(State::Active));
    }

    #[test]
    fn the_display_table_falls_back_on_the_loot_table() {
        let mut config = Config::default();
        assert_eq!(config.displayed_table(), DEFAULT_LOOT_TABLE);
        config.override_loot_table_to_display = Some("minecraft:chests/x".to_owned());
        assert_eq!(config.displayed_table(), "minecraft:chests/x");
    }

    #[test]
    fn the_config_survives_a_write_and_a_read() {
        let config = Config {
            loot_table: "minecraft:chests/trial_chambers/reward_ominous".to_owned(),
            key_item: Some(stack(ItemKind::OminousTrialKey, 1)),
            override_loot_table_to_display: Some(
                "minecraft:chests/trial_chambers/reward".to_owned(),
            ),
            ..Config::default()
        };
        assert_eq!(Config::read(&config.write()), config);
        assert_eq!(Config::read(&Compound::new()), Config::default());
    }

    #[test]
    fn ejections_come_last_item_first_and_report_their_progress() {
        let mut data = ServerData::default();
        data.set_items_to_eject(vec![
            stack(ItemKind::Arrow, 1),
            stack(ItemKind::Apple, 1),
            stack(ItemKind::Stick, 1),
        ]);
        assert_eq!(data.total_ejections_needed, 3);
        assert_eq!(data.ejection_progress(), 0.0);
        assert_eq!(
            data.pop_next_item_to_eject(),
            Some(stack(ItemKind::Stick, 1))
        );
        assert_eq!(data.ejection_progress(), 0.5);
        data.pop_next_item_to_eject();
        assert_eq!(data.ejection_progress(), 1.0);
    }

    #[test]
    fn a_single_ejection_is_complete_at_once() {
        let mut data = ServerData::default();
        data.set_items_to_eject(vec![stack(ItemKind::Arrow, 1)]);
        assert_eq!(data.ejection_progress(), 1.0);
    }

    #[test]
    fn the_rewarded_players_forget_the_oldest_past_the_cap() {
        let mut data = ServerData::default();
        for index in 0..=MAX_REWARDED_PLAYERS as u128 {
            data.add_rewarded(Uuid::from_u128(index));
        }
        assert_eq!(data.rewarded_players.len(), MAX_REWARDED_PLAYERS);
        assert!(!data.has_rewarded(Uuid::from_u128(0)));
        assert!(data.has_rewarded(Uuid::from_u128(MAX_REWARDED_PLAYERS as u128)));
    }

    #[test]
    fn the_server_data_survives_a_write_and_a_read() {
        let mut data = ServerData::default();
        data.add_rewarded(Uuid::from_u128(4));
        data.state_updating_resumes_at = 99;
        data.set_items_to_eject(vec![stack(ItemKind::Arrow, 2)]);
        assert_eq!(ServerData::read(&data.write()), data);
    }

    #[test]
    fn the_shared_data_survives_a_write_and_a_read() {
        let shared = SharedData {
            display_item: Some(stack(ItemKind::Apple, 1)),
            connected_players: vec![Uuid::from_u128(8)],
            connected_particles_range: 4.5,
        };
        assert_eq!(SharedData::read(&shared.write()), shared);
    }

    #[test]
    fn the_insert_failure_sound_is_throttled() {
        let mut data = ServerData::default();
        data.last_insert_fail = 100;
        assert!(!data.can_play_insert_fail(114));
        assert!(data.can_play_insert_fail(115));
    }

    #[test]
    fn the_states_round_trip_their_names() {
        for state in [
            State::Inactive,
            State::Active,
            State::Unlocking,
            State::Ejecting,
        ] {
            assert_eq!(State::from_name(state.name()), Some(state));
        }
    }
}
