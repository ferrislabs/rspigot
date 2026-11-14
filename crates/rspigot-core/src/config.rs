use rspigot_domain::BlockPos;
use rspigot_domain::entity::TrackingRanges;
use rspigot_domain::player::GameMode;
use rspigot_domain::spawn::state::SpawnSettings;
use rspigot_domain::world::Difficulty;
use sha2::{Digest, Sha256};

use crate::admin::AdminLists;
use crate::chunks::ChunkManagerConfig;
use crate::plugins::{EventKind, Priority};
use crate::streaming::StreamConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GameRules {
    pub natural_regeneration: bool,
    pub keep_inventory: bool,
    pub show_death_messages: bool,
    pub immediate_respawn: bool,
    pub fall_damage: bool,
    pub mob_drops: bool,
    pub mob_griefing: bool,
    pub mob_explosion_drop_decay: bool,
    pub tnt_explodes: bool,
    pub block_drops: bool,
    pub universal_anger: bool,
    pub forgive_dead_players: bool,
    pub spawn_patrols: bool,
    pub spawn_phantoms: bool,
    pub spawn_wandering_traders: bool,
    pub raids: bool,
}

impl Default for GameRules {
    fn default() -> Self {
        Self {
            natural_regeneration: true,
            keep_inventory: false,
            show_death_messages: true,
            immediate_respawn: false,
            fall_damage: true,
            mob_drops: true,
            mob_griefing: true,
            mob_explosion_drop_decay: true,
            tnt_explodes: true,
            block_drops: true,
            universal_anger: false,
            forgive_dead_players: true,
            spawn_patrols: true,
            spawn_phantoms: true,
            spawn_wandering_traders: true,
            raids: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActivationRanges {
    pub animals: i32,
    pub monsters: i32,
    pub raiders: i32,
    pub misc: i32,
    pub ignore_spectators: bool,
}

impl Default for ActivationRanges {
    fn default() -> Self {
        Self {
            animals: 32,
            monsters: 32,
            raiders: 48,
            misc: 16,
            ignore_spectators: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpawningConfig {
    pub categories: SpawnSettings,
    pub mob_spawn_range: i32,
    pub slime_seed: i64,
}

impl Default for SpawningConfig {
    fn default() -> Self {
        Self {
            categories: SpawnSettings::default(),
            mob_spawn_range: 6,
            slime_seed: rspigot_domain::spawn::slime::DEFAULT_SLIME_SEED,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MobSettings {
    pub zombie_aggressive_towards_villager: bool,
    pub arrow_despawn_rate: i32,
}

impl Default for MobSettings {
    fn default() -> Self {
        Self {
            zombie_aggressive_towards_villager: true,
            arrow_despawn_rate: 1200,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PriorityOverride {
    pub plugin: String,
    pub event: Option<EventKind>,
    pub priority: Priority,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginSettings {
    pub decision_ticks: u64,
    pub late_cancel_ticks: u64,
    pub order: Vec<String>,
    pub overrides: Vec<PriorityOverride>,
}

impl Default for PluginSettings {
    fn default() -> Self {
        Self {
            decision_ticks: 1,
            late_cancel_ticks: 20,
            order: Vec::new(),
            overrides: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct GameConfig {
    pub max_players: u32,
    pub view_distance: u8,
    pub simulation_distance: u8,
    pub default_game_mode: GameMode,
    pub force_game_mode: bool,
    pub difficulty: Difficulty,
    pub hardcore: bool,
    pub pvp: bool,
    pub allow_flight: bool,
    pub spawn_protection: u32,
    pub player_tracking_range: f64,
    pub tracking: TrackingRanges,
    pub activation: ActivationRanges,
    pub item_despawn_ticks: u32,
    pub chunk_workers: usize,
    pub stream: StreamConfig,
    pub chunks: ChunkManagerConfig,
    pub admin: AdminLists,
    pub whitelist_message: String,
    pub is_flat: bool,
    pub spawn_chunk_radius: i32,
    pub rules: GameRules,
    pub spawning: SpawningConfig,
    pub mobs: MobSettings,
    pub plugins: PluginSettings,
}

impl Default for GameConfig {
    fn default() -> Self {
        Self {
            max_players: 20,
            view_distance: 10,
            simulation_distance: 10,
            default_game_mode: GameMode::Survival,
            force_game_mode: false,
            difficulty: Difficulty::Easy,
            hardcore: false,
            pvp: true,
            allow_flight: false,
            spawn_protection: 16,
            player_tracking_range: 128.0,
            tracking: TrackingRanges::default(),
            activation: ActivationRanges::default(),
            item_despawn_ticks: 6000,
            chunk_workers: std::thread::available_parallelism()
                .map_or(4, |n| n.get().saturating_sub(2).max(2)),
            stream: StreamConfig::default(),
            chunks: ChunkManagerConfig::default(),
            admin: AdminLists::default(),
            whitelist_message: "You are not whitelisted on this server!".to_owned(),
            is_flat: false,
            spawn_chunk_radius: 2,
            rules: GameRules::default(),
            spawning: SpawningConfig::default(),
            mobs: MobSettings::default(),
            plugins: PluginSettings::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LevelData {
    pub name: String,
    pub seed: i64,
    pub spawn: Option<BlockPos>,
    pub spawn_angle: f32,
    pub game_time: i64,
    pub day_time: i64,
    pub daylight_cycle: bool,
    pub raining: bool,
    pub thundering: bool,
    pub spawn_mobs: bool,
    pub spawn_monsters: bool,
    pub uuid: Option<uuid::Uuid>,
    pub raids: Option<rspigot_nbt::Compound>,
}

impl Default for LevelData {
    fn default() -> Self {
        Self {
            name: "world".to_owned(),
            seed: 0,
            spawn: None,
            spawn_angle: 0.0,
            game_time: 0,
            day_time: 0,
            daylight_cycle: true,
            raining: false,
            thundering: false,
            spawn_mobs: true,
            spawn_monsters: true,
            uuid: None,
            raids: None,
        }
    }
}

pub fn hashed_seed(seed: i64) -> i64 {
    let digest = Sha256::digest(seed.to_le_bytes());
    let mut first = [0u8; 8];
    first.copy_from_slice(&digest[..8]);
    i64::from_le_bytes(first)
}
