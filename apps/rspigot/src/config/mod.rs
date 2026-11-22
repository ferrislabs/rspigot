mod offers;
pub mod permissions;
mod plugins;
mod properties;
mod unapplied;
mod yaml;

use std::path::Path;
use std::time::Duration;

use rspigot_core::admin::AdminLists;
use rspigot_core::chunks::ChunkManagerConfig;
use rspigot_core::config::{
    ActivationRanges, GameConfig, MobSettings, ServerMessages, SpawningConfig,
};
use rspigot_core::streaming::StreamConfig;
use rspigot_domain::entity::TrackingRanges;
use rspigot_domain::player::GameMode;
use rspigot_domain::spawn::state::{CategorySettings, SpawnSettings};
use rspigot_domain::world::Difficulty;
use rspigot_management::ManagementConfig;
use rspigot_network::{NetworkConfig, ProxyForwarding, QueryConfig};
use rspigot_plugins::PluginHostConfig;
use rspigot_rcon::RconConfig;

use plugins::PLUGINS_DEFAULTS;
pub use properties::Properties;
pub use yaml::YamlConfig;
use yaml::{BUKKIT_DEFAULTS, SPIGOT_DEFAULTS};

pub struct ServerFiles {
    pub properties: Properties,
    pub spigot: YamlConfig,
    pub bukkit: YamlConfig,
    pub plugins: YamlConfig,
}

impl ServerFiles {
    pub fn load(root: &Path) -> anyhow::Result<Self> {
        Ok(Self {
            properties: Properties::load_or_create(&root.join("server.properties"))?,
            spigot: YamlConfig::load_or_create(&root.join("spigot.yml"), SPIGOT_DEFAULTS)?,
            bukkit: YamlConfig::load_or_create(&root.join("bukkit.yml"), BUKKIT_DEFAULTS)?,
            plugins: YamlConfig::load_or_create(&root.join("plugins.yml"), PLUGINS_DEFAULTS)?,
        })
    }

    fn distance(&self, spigot_key: &str, property: &str) -> u8 {
        let base = self.properties.int(property, 10);
        let value = match self.spigot.string(spigot_key, "default").parse::<i64>() {
            Ok(v) if v > 0 => v,
            _ => base,
        };
        value.clamp(2, 32) as u8
    }

    pub fn secure_profile_requested(&self) -> bool {
        self.properties.bool("enforce-secure-profile") && self.properties.bool("online-mode")
    }

    pub fn game_config(&self, admin: AdminLists) -> GameConfig {
        let p = &self.properties;
        GameConfig {
            max_players: p.int("max-players", 20).clamp(0, i64::from(i32::MAX)) as u32,
            player_idle_timeout: p
                .int("player-idle-timeout", 0)
                .clamp(0, i64::from(i32::MAX)) as u32,
            view_distance: self.distance("world-settings.default.view-distance", "view-distance"),
            simulation_distance: self.distance(
                "world-settings.default.simulation-distance",
                "simulation-distance",
            ),
            default_game_mode: GameMode::from_name(p.string("gamemode"))
                .unwrap_or(GameMode::Survival),
            force_game_mode: p.bool("force-gamemode"),
            difficulty: Difficulty::from_name(p.string("difficulty")).unwrap_or(Difficulty::Easy),
            hardcore: p.bool("hardcore"),
            pvp: true,
            allow_flight: p.bool("allow-flight"),
            allow_nether: p.string("allow-nether") != "false"
                && self.bukkit.string("settings.allow-nether", "true") != "false",
            spawn_protection: p.int("spawn-protection", 16).max(0) as u32,
            op_permission_level: p.int("op-permission-level", 4).clamp(0, 4) as u8,
            function_permission_level: p.int("function-permission-level", 2).clamp(1, 4) as u8,
            player_tracking_range: self.spigot.float(
                "world-settings.default.entity-tracking-range.players",
                128.0,
            ) * self.broadcast_scale(),
            tracking: self.tracking_ranges(),
            activation: self.activation_ranges(),
            stream: StreamConfig::default(),
            chunks: ChunkManagerConfig {
                autosave_interval_ticks: self.bukkit.int("ticks-per.autosave", 6000).max(0) as u64,
                ..ChunkManagerConfig::default()
            },
            admin,
            whitelist_message: self.spigot.string(
                "messages.whitelist",
                "You are not whitelisted on this server!",
            ),
            is_flat: p.string("level-type").ends_with("flat"),
            spawning: self.spawning(),
            mobs: self.mob_settings(),
            plugins: plugins::settings(&self.plugins),
            enforce_whitelist: p.bool("enforce-whitelist"),
            log_ips: p.bool("log-ips"),
            log_commands: self.spigot.string("commands.log", "true") != "false",
            max_tick_time_ms: p.int("max-tick-time", 60_000),
            pause_when_empty_seconds: p
                .int("pause-when-empty-seconds", 60)
                .clamp(0, i64::from(u32::MAX)) as u32,
            item_despawn_ticks: self
                .spigot
                .int("world-settings.default.item-despawn-rate", 6000)
                .clamp(1, i64::from(u32::MAX)) as u32,
            item_merge_radius: self
                .spigot
                .float("world-settings.default.merge-radius.item", 0.5)
                .max(0.0),
            hopper_transfer_ticks: self
                .spigot
                .int("world-settings.default.ticks-per.hopper-transfer", 8)
                .clamp(1, i64::from(i32::MAX)) as i32,
            max_tnt_per_tick: self
                .spigot
                .int("world-settings.default.max-tnt-per-tick", 100)
                .clamp(0, i64::from(i32::MAX)) as i32,
            combat_exhaustion: self
                .spigot
                .float("world-settings.default.hunger.combat-exhaustion", 0.1)
                .max(0.0) as f32,
            hunger: self.hunger(),
            messages: self.messages(),
            motd: p.string("motd").to_owned(),
            hide_online_players: p.bool("hide-online-players"),
            accepts_transfers: p.bool("accepts-transfers"),
            entity_broadcast_range_percentage: p
                .int("entity-broadcast-range-percentage", 100)
                .clamp(0, i64::from(u32::MAX))
                as u32,
            status_heartbeat_interval: p
                .int("status-heartbeat-interval", 0)
                .clamp(0, i64::from(u32::MAX)) as u32,
            enable_status: p.bool("enable-status"),
            save_players: !self.spigot_flag("players.disable-saving", false),
            save_advancements: !self.spigot_flag("advancements.disable-saving", false),
            save_stats: !self.spigot_flag("stats.disable-saving", false),
            forced_stats: self
                .spigot
                .entries("stats.forced-stats")
                .into_iter()
                .filter_map(|(key, value)| Some((key, value.parse().ok()?)))
                .collect(),
            disabled_advancements: self.spigot.strings("advancements.disabled"),
            allow_end: self.bukkit.string("settings.allow-end", "true") != "false",
            warn_on_overload: self.bukkit.string("settings.warn-on-overload", "true") != "false",
            status_sample_count: self.spigot.int("settings.sample-count", 12).clamp(0, 1000)
                as usize,
            moved_too_quickly_multiplier: self
                .spigot
                .float("settings.moved-too-quickly-multiplier", 10.0)
                .max(0.0),
            server_port: p.int("server-port", 25565).clamp(1, 65535) as u16,
            chat: rspigot_core::config::ChatSecurity {
                spam_threshold_ticks: (p.int("chat-spam-threshold-seconds", 10).clamp(0, 3600) * 20)
                    as u32,
                command_spam_threshold_ticks: (p
                    .int("command-spam-threshold-seconds", 10)
                    .clamp(0, 3600)
                    * 20) as u32,
                ..Default::default()
            },
            ..GameConfig::default()
        }
    }

    fn hunger(&self) -> rspigot_domain::movement::HungerRates {
        let defaults = rspigot_domain::movement::HungerRates::default();
        let key = |name: &str, fallback: f32| {
            self.spigot
                .float(
                    &format!("world-settings.default.hunger.{name}"),
                    f64::from(fallback),
                )
                .max(0.0) as f32
        };
        rspigot_domain::movement::HungerRates {
            jump_walk: key("jump-walk-exhaustion", defaults.jump_walk),
            jump_sprint: key("jump-sprint-exhaustion", defaults.jump_sprint),
            swim_multiplier: key("swim-multiplier", defaults.swim_multiplier),
            sprint_multiplier: key("sprint-multiplier", defaults.sprint_multiplier),
            other_multiplier: key("other-multiplier", defaults.other_multiplier),
            regen: key("regen-exhaustion", defaults.regen),
        }
    }

    fn messages(&self) -> ServerMessages {
        let defaults = ServerMessages::default();
        let custom = |text: String, vanilla: &str| {
            if text == vanilla { String::new() } else { text }
        };
        ServerMessages {
            unknown_command: custom(
                self.spigot
                    .string("messages.unknown-command", unapplied::UNKNOWN_COMMAND),
                unapplied::UNKNOWN_COMMAND,
            ),
            server_full: custom(
                self.spigot
                    .string("messages.server-full", unapplied::SERVER_FULL),
                unapplied::SERVER_FULL,
            ),
            restart: self.spigot.string("messages.restart", &defaults.restart),
            shutdown: self
                .bukkit
                .string("settings.shutdown-message", &defaults.shutdown),
            restart_script: self.spigot.string("settings.restart-script", ""),
        }
    }

    pub fn permissions_file(&self) -> String {
        self.bukkit
            .string("settings.permissions-file", "permissions.yml")
    }

    pub fn unapplied_keys(&self) -> Vec<String> {
        unapplied::warnings(self)
    }

    fn spawning(&self) -> SpawningConfig {
        let defaults = SpawningConfig::default();
        let category = |limit: &str, ticks: &str, fallback: CategorySettings| CategorySettings {
            limit: self
                .bukkit
                .int(&format!("spawn-limits.{limit}"), i64::from(fallback.limit))
                .clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32,
            ticks_per: self
                .bukkit
                .int(&format!("ticks-per.{ticks}"), fallback.ticks_per)
                .clamp(i64::from(i32::MIN), i64::from(i32::MAX)),
        };
        let vanilla = defaults.categories;
        SpawningConfig {
            categories: SpawnSettings {
                monster: category("monsters", "monster-spawns", vanilla.monster),
                creature: category("animals", "animal-spawns", vanilla.creature),
                ambient: category("ambient", "ambient-spawns", vanilla.ambient),
                axolotls: category("axolotls", "axolotl-spawns", vanilla.axolotls),
                underground_water_creature: category(
                    "water-underground-creature",
                    "water-underground-creature-spawns",
                    vanilla.underground_water_creature,
                ),
                water_creature: category("water-animals", "water-spawns", vanilla.water_creature),
                water_ambient: category(
                    "water-ambient",
                    "water-ambient-spawns",
                    vanilla.water_ambient,
                ),
            },
            mob_spawn_range: i32::from(self.spigot.int(
                "world-settings.default.mob-spawn-range",
                i64::from(defaults.mob_spawn_range),
            ) as i32 as i8),
            slime_seed: self
                .spigot
                .int("world-settings.default.seed-slime", defaults.slime_seed),
        }
    }

    pub fn code_of_conduct(&self, root: &std::path::Path) -> Option<String> {
        if !self.properties.bool("enable-code-of-conduct") {
            return None;
        }
        let path = root.join("codeofconduct").join("en_us.txt");
        match std::fs::read_to_string(&path) {
            Ok(text) => Some(text),
            Err(error) => {
                tracing::warn!(
                    "enable-code-of-conduct is on but {} cannot be read: {error}",
                    path.display()
                );
                None
            }
        }
    }

    pub fn world_generator(&self, seed: i64) -> rspigot_worldgen::WorldGenerator {
        let generator = rspigot_worldgen::WorldGenerator::from_level_type(
            self.properties.string("level-type"),
            seed,
            self.properties.string("generator-settings"),
        );
        match generator {
            rspigot_worldgen::WorldGenerator::Noise(noise)
                if !self.properties.bool("generate-structures") =>
            {
                rspigot_worldgen::WorldGenerator::Noise(noise.without_structures())
            }
            other => other,
        }
    }

    fn spigot_flag(&self, path: &str, fallback: bool) -> bool {
        self.spigot
            .string(path, if fallback { "true" } else { "false" })
            .eq_ignore_ascii_case("true")
    }

    fn broadcast_scale(&self) -> f64 {
        self.properties
            .int("entity-broadcast-range-percentage", 100)
            .clamp(10, 1000) as f64
            / 100.0
    }

    fn tracking_ranges(&self) -> TrackingRanges {
        let defaults = TrackingRanges::default();
        let percentage = self
            .properties
            .int("entity-broadcast-range-percentage", 100)
            .clamp(10, 1000);
        let range = |key: &str, fallback: i32| {
            let base = self
                .spigot
                .int(
                    &format!("world-settings.default.{key}"),
                    i64::from(fallback),
                )
                .clamp(0, i64::from(i32::MAX));
            (base * percentage / 100).min(i64::from(i32::MAX)) as i32
        };
        TrackingRanges {
            players: range("entity-tracking-range.players", defaults.players),
            animals: range("entity-tracking-range.animals", defaults.animals),
            monsters: range("entity-tracking-range.monsters", defaults.monsters),
            misc: range("entity-tracking-range.misc", defaults.misc),
            display: range("entity-tracking-range.display", defaults.display),
            other: range("entity-tracking-range.other", defaults.other),
            monster_activation: self
                .spigot
                .int(
                    "world-settings.default.entity-activation-range.monsters",
                    i64::from(defaults.monster_activation),
                )
                .clamp(0, i64::from(i32::MAX)) as i32,
        }
    }

    fn mob_settings(&self) -> MobSettings {
        let defaults = MobSettings::default();
        MobSettings {
            zombie_aggressive_towards_villager: self
                .spigot
                .string(
                    "world-settings.default.zombie-aggressive-towards-villager",
                    "true",
                )
                .eq_ignore_ascii_case("true"),
            arrow_despawn_rate: self
                .spigot
                .int(
                    "world-settings.default.arrow-despawn-rate",
                    i64::from(defaults.arrow_despawn_rate),
                )
                .clamp(i64::from(i32::MIN), i64::from(i32::MAX))
                as i32,
        }
    }

    fn activation_ranges(&self) -> ActivationRanges {
        let defaults = ActivationRanges::default();
        let range = |key: &str, fallback: i32| {
            self.spigot
                .int(
                    &format!("world-settings.default.entity-activation-range.{key}"),
                    i64::from(fallback),
                )
                .clamp(0, i64::from(i32::MAX)) as i32
        };
        ActivationRanges {
            animals: range("animals", defaults.animals),
            monsters: range("monsters", defaults.monsters),
            raiders: range("raiders", defaults.raiders),
            misc: range("misc", defaults.misc),
            ignore_spectators: self.spigot.string(
                "world-settings.default.entity-activation-range.ignore-spectators",
                &defaults.ignore_spectators.to_string(),
            ) == "true",
        }
    }

    pub fn plugin_host_config(
        &self,
        environment_token: Option<String>,
    ) -> anyhow::Result<PluginHostConfig> {
        plugins::host_config(&self.plugins, environment_token)
    }

    pub fn plugins_enabled(&self) -> bool {
        plugins::enabled(&self.plugins)
    }

    pub fn expected_plugins(&self) -> Vec<rspigot_plugins::ExpectedPlugin> {
        plugins::expected(&self.plugins)
    }

    pub fn management_config(&self) -> Option<ManagementConfig> {
        let p = &self.properties;
        if !p.bool("management-server-enabled") {
            return None;
        }
        Some(ManagementConfig {
            host: Some(p.string("management-server-host"))
                .filter(|h| !h.is_empty())
                .unwrap_or("localhost")
                .to_owned(),
            port: p.int("management-server-port", 0).clamp(0, 65535) as u16,
            secret: p.string("management-server-secret").to_owned(),
            tls_enabled: p.bool("management-server-tls-enabled"),
            tls_keystore: p.string("management-server-tls-keystore").to_owned(),
            tls_keystore_password: p
                .string("management-server-tls-keystore-password")
                .to_owned(),
            allowed_origins: p
                .string("management-server-allowed-origins")
                .split(',')
                .map(str::trim)
                .filter(|o| !o.is_empty())
                .map(str::to_owned)
                .collect(),
        })
    }

    pub fn rcon_config(&self) -> Option<RconConfig> {
        let p = &self.properties;
        if !p.bool("enable-rcon") {
            return None;
        }
        Some(RconConfig {
            host: p.string("server-ip").to_owned(),
            port: p.int("rcon.port", 25575).clamp(1, 65535) as u16,
            password: p.string("rcon.password").to_owned(),
        })
    }

    pub fn query_config(&self) -> Option<QueryConfig> {
        let p = &self.properties;
        if !p.bool("enable-query") {
            return None;
        }
        let ip = p.string("server-ip");
        let host = if ip.is_empty() { "0.0.0.0" } else { ip };
        let port = p.int("query.port", 25565).clamp(1, 65535) as u16;
        let bind = format!("{host}:{port}")
            .parse()
            .or_else(|_| format!("[{host}]:{port}").parse())
            .ok()?;
        Some(QueryConfig {
            bind,
            motd: p.string("motd").to_owned(),
            map: p.string("level-name").to_owned(),
            version: rspigot_data::VERSION_NAME.to_owned(),
            server_port: p.int("server-port", 25565).clamp(1, 65535) as u16,
            server_ip: ip.to_owned(),
        })
    }

    fn proxy_forwarding(&self) -> anyhow::Result<ProxyForwarding> {
        if self.spigot.string("settings.velocity.enabled", "false") == "true" {
            let secret = self.spigot.string("settings.velocity.secret", "");
            anyhow::ensure!(
                !secret.is_empty(),
                "settings.velocity.secret in spigot.yml is empty while Velocity forwarding is enabled"
            );
            return Ok(ProxyForwarding::Velocity(secret.into_bytes()));
        }
        if self.spigot.string("settings.bungeecord", "false") == "true" {
            return Ok(ProxyForwarding::BungeeCord);
        }
        Ok(ProxyForwarding::Off)
    }

    pub fn network_config(&self, favicon: Option<String>) -> anyhow::Result<NetworkConfig> {
        let p = &self.properties;
        let proxy = self.proxy_forwarding()?;
        let ip = p.string("server-ip");
        let port = p.int("server-port", 25565).clamp(1, 65535) as u16;
        let host = if ip.is_empty() { "0.0.0.0" } else { ip };
        let bind = format!("{host}:{port}")
            .parse()
            .or_else(|_| format!("[{host}]:{port}").parse())?;
        let threshold = p.int("network-compression-threshold", 256);
        Ok(NetworkConfig {
            bind,
            online_mode: p.bool("online-mode"),
            prevent_proxy_connections: p.bool("prevent-proxy-connections"),
            compression_threshold: usize::try_from(threshold).ok(),
            motd: p.string("motd").to_owned(),
            favicon,
            resource_pack: offers::resource_pack(p),
            server_links: offers::server_links(p, &self.spigot),
            hide_online_players: p.bool("hide-online-players"),
            enable_status: p.bool("enable-status"),
            proxy,
            enforce_secure_profile: false,
            accepts_transfers: p.bool("accepts-transfers"),
            keep_alive_timeout: Duration::from_secs(30),
            rate_limit: p.int("rate-limit", 0).clamp(0, i64::from(u32::MAX)) as u32,
            connection_throttle: Duration::from_millis(
                self.bukkit.int("settings.connection-throttle", 4000).max(0) as u64,
            ),
            outdated_client_message: self.spigot.string(
                "messages.outdated-client",
                "Outdated client! Please use {0}",
            ),
            outdated_server_message: self.spigot.string(
                "messages.outdated-server",
                "Outdated server! I'm still on {0}",
            ),
            ..NetworkConfig::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(spigot: &str, bukkit: &str) -> ServerFiles {
        let merged = |text: &str, defaults: &str| {
            let mut root: serde_yaml_ng::Value = serde_yaml_ng::from_str(defaults).unwrap();
            let user: serde_yaml_ng::Value = serde_yaml_ng::from_str(text).unwrap();
            if let (serde_yaml_ng::Value::Mapping(root), serde_yaml_ng::Value::Mapping(user)) =
                (&mut root, user)
            {
                for (key, value) in user {
                    root.insert(key, value);
                }
            }
            YamlConfig::from_str(&serde_yaml_ng::to_string(&root).unwrap()).unwrap()
        };
        ServerFiles {
            properties: Properties::parse(""),
            spigot: merged(spigot, SPIGOT_DEFAULTS),
            bukkit: merged(bukkit, BUKKIT_DEFAULTS),
            plugins: YamlConfig::from_str(PLUGINS_DEFAULTS).unwrap(),
        }
    }

    #[test]
    fn secure_profiles_are_requested_only_in_online_mode_with_the_property_on() {
        let mut server_files = files("{}", "{}");
        server_files
            .properties
            .set("enforce-secure-profile", "true");
        server_files.properties.set("online-mode", "true");
        assert!(server_files.secure_profile_requested());
        server_files
            .properties
            .set("enforce-secure-profile", "false");
        assert!(!server_files.secure_profile_requested());
        server_files
            .properties
            .set("enforce-secure-profile", "true");
        server_files.properties.set("online-mode", "false");
        assert!(!server_files.secure_profile_requested());
    }

    #[test]
    fn spawning_defaults_match_bukkit_and_spigot() {
        let config = files("{}", "{}").game_config(AdminLists::default());
        assert_eq!(config.spawning, SpawningConfig::default());
        assert_eq!(config.spawning.mob_spawn_range, 6);
        assert_eq!(config.spawning.categories.creature.ticks_per, 400);
        assert_eq!(config.spawning.categories.water_ambient.limit, 20);
    }

    #[test]
    fn spawn_limits_and_ticks_per_are_read_per_category() {
        let bukkit = "spawn-limits:\n  monsters: 12\n  water-animals: 0\nticks-per:\n  animal-spawns: 1\n  ambient-spawns: 0\n";
        let config = files("{}", bukkit).game_config(AdminLists::default());
        let categories = config.spawning.categories;
        assert_eq!(categories.monster.limit, 12);
        assert_eq!(categories.water_creature.limit, 0);
        assert_eq!(categories.creature.ticks_per, 1);
        assert_eq!(categories.ambient.ticks_per, 0);
        assert_eq!(categories.axolotls.limit, 5);
        let spigot = "world-settings:\n  default:\n    mob-spawn-range: 300\n";
        assert_eq!(
            files(spigot, "{}")
                .game_config(AdminLists::default())
                .spawning
                .mob_spawn_range,
            44
        );
    }

    #[test]
    fn the_broadcast_percentage_scales_the_player_tracking_range() {
        let mut files = files("{}", "{}");
        files
            .properties
            .set("entity-broadcast-range-percentage", "50");
        let config = files.game_config(AdminLists::default());
        assert_eq!(config.player_tracking_range, 64.0);
    }

    #[test]
    fn the_broadcast_percentage_scales_tracking_ranges() {
        let mut files = files("{}", "{}");
        files
            .properties
            .set("entity-broadcast-range-percentage", "50");
        let ranges = files.tracking_ranges();
        assert_eq!(ranges.animals, 48);
        assert_eq!(ranges.players, 64);
        assert_eq!(ranges.monster_activation, 32);
        files
            .properties
            .set("entity-broadcast-range-percentage", "1");
        assert_eq!(files.tracking_ranges().animals, 9);
    }

    #[test]
    fn the_management_server_is_off_by_default_and_reads_its_keys() {
        let mut files = files("{}", "{}");
        assert_eq!(files.management_config(), None);
        files.properties.set("management-server-enabled", "true");
        files
            .properties
            .set("management-server-tls-enabled", "false");
        files
            .properties
            .set("management-server-allowed-origins", "http://a, http://b");
        files.properties.set("management-server-port", "25585");
        let config = files.management_config().expect("enabled");
        assert_eq!(config.host, "localhost");
        assert_eq!(config.port, 25585);
        assert_eq!(config.allowed_origins, ["http://a", "http://b"]);
        assert!(!config.tls_enabled);
    }

    #[test]
    fn rcon_is_off_by_default_and_reads_its_keys() {
        let mut files = files("{}", "{}");
        assert_eq!(files.rcon_config(), None);
        files.properties.set("enable-rcon", "true");
        files.properties.set("rcon.password", "pw");
        files.properties.set("rcon.port", "25576");
        files.properties.set("server-ip", "10.0.0.2");
        let config = files.rcon_config().expect("enabled");
        assert_eq!(config.port, 25576);
        assert_eq!(config.password, "pw");
        assert_eq!(config.host, "10.0.0.2");
    }

    #[test]
    fn the_management_keystore_keys_are_read() {
        let mut files = files("{}", "{}");
        files.properties.set("management-server-enabled", "true");
        files
            .properties
            .set("management-server-tls-enabled", "true");
        files
            .properties
            .set("management-server-tls-keystore", "identity.pem");
        let config = files.management_config().expect("enabled");
        assert!(config.tls_enabled);
        assert_eq!(config.tls_keystore, "identity.pem");
    }

    #[test]
    fn proxy_forwarding_is_off_until_a_key_turns_it_on() {
        let off = files("{}", "{}");
        assert_eq!(off.proxy_forwarding().unwrap(), ProxyForwarding::Off);
        let bungee = files("settings:\n  bungeecord: true\n", "{}");
        assert_eq!(
            bungee.proxy_forwarding().unwrap(),
            ProxyForwarding::BungeeCord
        );
        let velocity = files(
            "settings:\n  bungeecord: true\n  velocity:\n    enabled: true\n    secret: abc\n",
            "{}",
        );
        assert_eq!(
            velocity.proxy_forwarding().unwrap(),
            ProxyForwarding::Velocity(b"abc".to_vec())
        );
    }

    #[test]
    fn velocity_without_a_secret_is_a_configuration_error() {
        let velocity = files("settings:\n  velocity:\n    enabled: true\n", "{}");
        assert!(velocity.proxy_forwarding().is_err());
    }

    #[test]
    fn the_query_listener_is_off_by_default_and_uses_its_own_port() {
        let mut files = files("{}", "{}");
        assert!(files.query_config().is_none());
        files.properties.set("enable-query", "true");
        files.properties.set("query.port", "25570");
        files.properties.set("level-name", "lobby");
        let config = files.query_config().expect("enabled");
        assert_eq!(config.bind.port(), 25570);
        assert_eq!(config.map, "lobby");
        assert_eq!(config.server_port, 25565);
    }

    #[test]
    fn the_applied_properties_reach_the_game_config() {
        let mut files = files("{}", "{}");
        for (key, value) in [
            ("enforce-whitelist", "true"),
            ("log-ips", "false"),
            ("max-tick-time", "-1"),
            ("pause-when-empty-seconds", "5"),
        ] {
            files.properties.set(key, value);
        }
        let config = files.game_config(AdminLists::default());
        assert!(config.enforce_whitelist);
        assert!(!config.log_ips);
        assert_eq!(config.max_tick_time_ms, -1);
        assert_eq!(config.pause_when_empty_seconds, 5);
        let mut shipped = super::ServerFiles {
            properties: Properties::parse(""),
            ..files
        };
        for (key, value) in properties::DEFAULTS {
            shipped.properties.set(key, value);
        }
        let shipped = shipped.game_config(AdminLists::default());
        assert!(!shipped.enforce_whitelist);
        assert!(shipped.log_ips);
        assert_eq!(shipped.max_tick_time_ms, 60_000);
        assert_eq!(shipped.pause_when_empty_seconds, 60);
    }

    #[test]
    fn generate_structures_false_removes_the_structures_from_the_noise_generator() {
        use rspigot_core::ports::ChunkGenerator;
        let find = |value: &str| {
            let mut files = files("{}", "{}");
            files.properties.set("level-type", "minecraft:normal");
            files.properties.set("generate-structures", value);
            files.world_generator(1).nearest_structure(
                &rspigot_domain::world::Dimension::overworld(),
                rspigot_domain::Vec3::ZERO,
                "minecraft:village_plains",
            )
        };
        assert!(find("true").is_some());
        assert!(find("false").is_none());
    }

    #[test]
    fn rate_limit_reaches_the_network_config() {
        let mut files = files("{}", "{}");
        assert_eq!(files.network_config(None).unwrap().rate_limit, 0);
        files.properties.set("rate-limit", "500");
        assert_eq!(files.network_config(None).unwrap().rate_limit, 500);
    }

    #[test]
    fn the_code_of_conduct_is_read_from_its_folder_only_when_enabled() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("codeofconduct")).unwrap();
        std::fs::write(dir.path().join("codeofconduct/en_us.txt"), "be nice").unwrap();
        let mut files = files("{}", "{}");
        assert_eq!(files.code_of_conduct(dir.path()), None);
        files.properties.set("enable-code-of-conduct", "true");
        assert_eq!(
            files.code_of_conduct(dir.path()).as_deref(),
            Some("be nice")
        );
        let empty = tempfile::tempdir().unwrap();
        assert_eq!(files.code_of_conduct(empty.path()), None);
    }

    #[test]
    fn the_yaml_keys_reach_the_game_config() {
        let spigot = "settings:\n  sample-count: 3\n  moved-too-quickly-multiplier: 20.0\nadvancements:\n  disable-saving: true\n  disabled:\n  - minecraft:story/root\nplayers:\n  disable-saving: true\nstats:\n  disable-saving: true\n  forced-stats:\n    minecraft:leave_game: 4\n";
        let bukkit = "settings:\n  allow-end: false\n  warn-on-overload: false\n";
        let config = files(spigot, bukkit).game_config(AdminLists::default());
        assert_eq!(config.status_sample_count, 3);
        assert_eq!(config.moved_too_quickly_multiplier, 20.0);
        assert!(!config.save_advancements && !config.save_players && !config.save_stats);
        assert_eq!(config.disabled_advancements, ["minecraft:story/root"]);
        assert_eq!(
            config.forced_stats,
            [("minecraft:leave_game".to_owned(), 4)]
        );
        assert!(!config.allow_end);
        assert!(!config.warn_on_overload);
        let vanilla = files("{}", "{}").game_config(AdminLists::default());
        assert!(vanilla.save_players && vanilla.allow_end && vanilla.warn_on_overload);
        assert_eq!(vanilla.status_sample_count, 12);
    }

    #[test]
    fn the_hunger_keys_reach_the_game_config() {
        let spigot = "world-settings:\n  default:\n    hunger:\n      jump-walk-exhaustion: 0.5\n      jump-sprint-exhaustion: 0.75\n      regen-exhaustion: 3.0\n      swim-multiplier: 0.02\n      sprint-multiplier: 0.3\n      other-multiplier: 0.04\n";
        let hunger = files(spigot, "{}")
            .game_config(AdminLists::default())
            .hunger;
        assert_eq!(hunger.jump_walk, 0.5);
        assert_eq!(hunger.jump_sprint, 0.75);
        assert_eq!(hunger.regen, 3.0);
        assert_eq!(hunger.swim_multiplier, 0.02);
        assert_eq!(hunger.sprint_multiplier, 0.3);
        assert_eq!(hunger.other_multiplier, 0.04);
        let vanilla = files("{}", "{}").game_config(AdminLists::default()).hunger;
        assert_eq!(vanilla, rspigot_domain::movement::HungerRates::default());
    }

    #[test]
    fn the_applied_spigot_keys_reach_the_game_config() {
        let spigot = "world-settings:\n  default:\n    item-despawn-rate: 300\n    merge-radius:\n      item: 3.5\n    ticks-per:\n      hopper-transfer: 2\n    max-tnt-per-tick: 4\n    hunger:\n      combat-exhaustion: 0.25\n";
        let config = files(spigot, "{}").game_config(AdminLists::default());
        assert_eq!(config.item_despawn_ticks, 300);
        assert_eq!(config.item_merge_radius, 3.5);
        assert_eq!(config.hopper_transfer_ticks, 2);
        assert_eq!(config.max_tnt_per_tick, 4);
        assert!((config.combat_exhaustion - 0.25).abs() < 1e-6);
        let vanilla = files("{}", "{}").game_config(AdminLists::default());
        assert_eq!(vanilla.item_despawn_ticks, 6000);
        assert_eq!(vanilla.item_merge_radius, 0.5);
        assert_eq!(vanilla.hopper_transfer_ticks, 8);
        assert_eq!(vanilla.max_tnt_per_tick, 100);
        assert!((vanilla.combat_exhaustion - 0.1).abs() < 1e-6);
    }

    #[test]
    fn the_messages_keep_the_vanilla_text_until_they_are_customised() {
        let vanilla = files("{}", "{}").game_config(AdminLists::default());
        assert_eq!(vanilla.messages.unknown_command, "");
        assert_eq!(vanilla.messages.server_full, "");
        assert_eq!(vanilla.messages.restart, "Server is restarting");
        assert_eq!(vanilla.messages.shutdown, "Server closed");
        let spigot = "messages:\n  unknown-command: Nope\n  server-full: Full up\n  restart: Back soon\nsettings:\n  restart-script: ./again.sh\n";
        let bukkit = "settings:\n  shutdown-message: Bye now\n  permissions-file: perms.yml\n";
        let configured = files(spigot, bukkit);
        let config = configured.game_config(AdminLists::default());
        assert_eq!(config.messages.unknown_command, "Nope");
        assert_eq!(config.messages.server_full, "Full up");
        assert_eq!(config.messages.restart, "Back soon");
        assert_eq!(config.messages.shutdown, "Bye now");
        assert_eq!(config.messages.restart_script, "./again.sh");
        assert!(config.log_commands);
        let quiet = files("commands:\n  log: false\n", "{}").game_config(AdminLists::default());
        assert!(!quiet.log_commands);
        assert_eq!(configured.permissions_file(), "perms.yml");
    }
}
