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
pub enum RuleValue {
    Flag(bool),
    Number(i32),
}

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
    pub players_sleeping_percentage: i32,
    pub random_tick_speed: i32,
    pub fire_spread_radius: i32,
    pub max_snow_accumulation_height: i32,
    pub max_command_sequence_length: i32,
    pub command_blocks_work: bool,
    pub command_block_output: bool,
    pub send_command_feedback: bool,
    pub advance_weather: bool,
    pub allow_entering_nether_using_portals: bool,
    pub block_explosion_drop_decay: bool,
    pub drowning_damage: bool,
    pub elytra_movement_check: bool,
    pub ender_pearls_vanish_on_death: bool,
    pub entity_drops: bool,
    pub fire_damage: bool,
    pub freeze_damage: bool,
    pub global_sound_events: bool,
    pub lava_source_conversion: bool,
    pub limited_crafting: bool,
    pub locator_bar: bool,
    pub log_admin_commands: bool,
    pub player_movement_check: bool,
    pub projectiles_can_break_blocks: bool,
    pub reduced_debug_info: bool,
    pub show_advancement_messages: bool,
    pub spawner_blocks_work: bool,
    pub spawn_wardens: bool,
    pub spectators_generate_chunks: bool,
    pub spread_vines: bool,
    pub tnt_explosion_drop_decay: bool,
    pub water_source_conversion: bool,
    pub max_block_modifications: i32,
    pub max_command_forks: i32,
    pub max_entity_cramming: i32,
    pub max_minecart_speed: i32,
    pub players_nether_portal_creative_delay: i32,
    pub players_nether_portal_default_delay: i32,
    pub respawn_radius: i32,
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
            players_sleeping_percentage: 100,
            random_tick_speed: 3,
            fire_spread_radius: 128,
            max_snow_accumulation_height: 1,
            max_command_sequence_length: 65_536,
            command_blocks_work: true,
            command_block_output: true,
            send_command_feedback: true,
            advance_weather: true,
            allow_entering_nether_using_portals: true,
            block_explosion_drop_decay: true,
            drowning_damage: true,
            elytra_movement_check: true,
            ender_pearls_vanish_on_death: true,
            entity_drops: true,
            fire_damage: true,
            freeze_damage: true,
            global_sound_events: true,
            lava_source_conversion: false,
            limited_crafting: false,
            locator_bar: true,
            log_admin_commands: true,
            player_movement_check: true,
            projectiles_can_break_blocks: true,
            reduced_debug_info: false,
            show_advancement_messages: true,
            spawner_blocks_work: true,
            spawn_wardens: true,
            spectators_generate_chunks: true,
            spread_vines: true,
            tnt_explosion_drop_decay: false,
            water_source_conversion: true,
            max_block_modifications: 32768,
            max_command_forks: 65536,
            max_entity_cramming: 24,
            max_minecart_speed: 8,
            players_nether_portal_creative_delay: 0,
            players_nether_portal_default_delay: 80,
            respawn_radius: 10,
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
    pub player_idle_timeout: u32,
    pub view_distance: u8,
    pub simulation_distance: u8,
    pub default_game_mode: GameMode,
    pub force_game_mode: bool,
    pub difficulty: Difficulty,
    pub hardcore: bool,
    pub pvp: bool,
    pub allow_flight: bool,
    pub allow_nether: bool,
    pub spawn_protection: u32,
    pub op_permission_level: u8,
    pub function_permission_level: u8,
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
    pub chat: ChatSecurity,
    pub thunder_chance: i32,
    pub enforce_whitelist: bool,
    pub log_ips: bool,
    pub log_commands: bool,
    pub max_tick_time_ms: i64,
    pub pause_when_empty_seconds: u32,
    pub item_merge_radius: f64,
    pub hopper_transfer_ticks: i32,
    pub max_tnt_per_tick: i32,
    pub combat_exhaustion: f32,
    pub messages: ServerMessages,
    pub motd: String,
    pub hide_online_players: bool,
    pub accepts_transfers: bool,
    pub entity_broadcast_range_percentage: u32,
    pub status_heartbeat_interval: u32,
    pub enable_status: bool,
    pub server_port: u16,
    pub user_cache: crate::user_cache::UserCache,
    pub save_players: bool,
    pub save_advancements: bool,
    pub save_stats: bool,
    pub forced_stats: Vec<(String, i32)>,
    pub disabled_advancements: Vec<String>,
    pub allow_end: bool,
    pub warn_on_overload: bool,
    pub status_sample_count: usize,
    pub moved_too_quickly_multiplier: f64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerMessages {
    pub unknown_command: String,
    pub server_full: String,
    pub restart: String,
    pub shutdown: String,
    pub restart_script: String,
}

impl Default for ServerMessages {
    fn default() -> Self {
        Self {
            unknown_command: String::new(),
            server_full: String::new(),
            restart: "Server is restarting".to_owned(),
            shutdown: "Server closed".to_owned(),
            restart_script: String::new(),
        }
    }
}

impl Default for GameConfig {
    fn default() -> Self {
        Self {
            max_players: 20,
            player_idle_timeout: 0,
            view_distance: 10,
            simulation_distance: 10,
            default_game_mode: GameMode::Survival,
            force_game_mode: false,
            difficulty: Difficulty::Easy,
            hardcore: false,
            pvp: true,
            allow_flight: false,
            allow_nether: true,
            spawn_protection: 16,
            op_permission_level: 4,
            function_permission_level: 2,
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
            chat: ChatSecurity::default(),
            thunder_chance: 100_000,
            enforce_whitelist: false,
            log_ips: true,
            log_commands: true,
            max_tick_time_ms: 0,
            pause_when_empty_seconds: 0,
            item_merge_radius: 0.5,
            hopper_transfer_ticks: 8,
            max_tnt_per_tick: 100,
            combat_exhaustion: 0.1,
            messages: ServerMessages::default(),
            motd: "A Minecraft Server".to_owned(),
            hide_online_players: false,
            accepts_transfers: false,
            entity_broadcast_range_percentage: 100,
            status_heartbeat_interval: 0,
            enable_status: true,
            server_port: 25565,
            user_cache: crate::user_cache::UserCache::default(),
            save_players: true,
            save_advancements: true,
            save_stats: true,
            forced_stats: Vec::new(),
            disabled_advancements: Vec::new(),
            allow_end: true,
            warn_on_overload: true,
            status_sample_count: 12,
            moved_too_quickly_multiplier: 10.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ChatSecurity {
    pub enforce_secure_profile: bool,
    pub services_keys: Vec<Vec<u8>>,
    pub spam_threshold_ticks: u32,
    pub command_spam_threshold_ticks: u32,
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
    pub clear_weather_time: i32,
    pub rain_time: i32,
    pub thunder_time: i32,
    pub spawn_mobs: bool,
    pub spawn_monsters: bool,
    pub rules: Option<GameRules>,
    pub pvp: Option<bool>,
    pub uuid: Option<uuid::Uuid>,
    pub raids: Option<rspigot_nbt::Compound>,
    pub scoreboard: Option<rspigot_nbt::Compound>,
    pub dragon_fight: Option<rspigot_nbt::Compound>,
    pub custom_boss_events: Option<rspigot_nbt::Compound>,
    pub forced_chunks: Option<rspigot_nbt::Compound>,
    pub world_border: Option<rspigot_nbt::Compound>,
    pub scheduled_events: Option<rspigot_nbt::Compound>,
    pub data_packs: Option<crate::functions::DataPackLists>,
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
            clear_weather_time: 0,
            rain_time: 0,
            thunder_time: 0,
            spawn_mobs: true,
            spawn_monsters: true,
            rules: None,
            pvp: None,
            uuid: None,
            raids: None,
            scoreboard: None,
            dragon_fight: None,
            custom_boss_events: None,
            forced_chunks: None,
            world_border: None,
            scheduled_events: None,
            data_packs: None,
        }
    }
}

pub fn hashed_seed(seed: i64) -> i64 {
    let digest = Sha256::digest(seed.to_le_bytes());
    let mut first = [0u8; 8];
    first.copy_from_slice(&digest[..8]);
    i64::from_le_bytes(first)
}
