use std::fs;
use std::io;
use std::path::Path;

use serde_yaml_ng::{Mapping, Value};

pub const SPIGOT_DEFAULTS: &str = r#"config-version: 12
settings:
  debug: false
  bungeecord: false
  velocity:
    enabled: false
    secret: ''
  timeout-time: 60
  restart-on-crash: true
  restart-script: ./start.sh
  netty-threads: 4
  sample-count: 12
  player-shuffle: 0
  user-cache-size: 1000
  save-user-cache-on-stop-only: false
  moved-wrongly-threshold: 0.0625
  moved-too-quickly-multiplier: 10.0
  log-villager-deaths: true
  log-named-deaths: true
  attribute:
    maxAbsorption:
      max: 2048.0
    maxHealth:
      max: 1024.0
    movementSpeed:
      max: 1024.0
    attackDamage:
      max: 2048.0
messages:
  whitelist: You are not whitelisted on this server!
  unknown-command: Unknown command. Type "/help" for help.
  server-full: The server is full!
  outdated-client: Outdated client! Please use {0}
  outdated-server: Outdated server! I'm still on {0}
  restart: Server is restarting
commands:
  log: true
  tab-complete: 0
  send-namespaced: true
  spam-exclusions:
  - /skill
  silent-commandblock-console: false
  replace-commands:
  - setblock
  - summon
  - testforblock
  - tellraw
advancements:
  disable-saving: false
  disabled:
  - minecraft:story/disabled
players:
  disable-saving: false
stats:
  disable-saving: false
  forced-stats: {}
world-settings:
  default:
    below-zero-generation-in-existing-chunks: true
    view-distance: default
    simulation-distance: default
    merge-radius:
      item: 0.5
      exp: -1.0
    mob-spawn-range: 6
    item-despawn-rate: 6000
    arrow-despawn-rate: 1200
    trident-despawn-rate: 1200
    zombie-aggressive-towards-villager: true
    entity-activation-range:
      animals: 32
      monsters: 32
      raiders: 64
      misc: 16
      water: 16
      villagers: 32
      flying-monsters: 32
    entity-tracking-range:
      players: 128
      animals: 96
      monsters: 96
      misc: 96
      display: 128
      other: 64
    ticks-per:
      hopper-transfer: 8
      hopper-check: 1
    hunger:
      jump-walk-exhaustion: 0.05
      jump-sprint-exhaustion: 0.2
      combat-exhaustion: 0.1
      regen-exhaustion: 6.0
      swim-multiplier: 0.01
      sprint-multiplier: 0.1
      other-multiplier: 0.0
    max-tnt-per-tick: 100
    max-tick-time:
      tile: 50
      entity: 50
    verbose: false
"#;

pub const BUKKIT_DEFAULTS: &str = r#"settings:
  allow-nether: true
  allow-end: true
  warn-on-overload: true
  permissions-file: permissions.yml
  update-folder: update
  plugin-profiling: false
  connection-throttle: 4000
  query-plugins: true
  deprecated-verbose: default
  shutdown-message: Server closed
  minimum-api: none
  use-map-color-cache: true
  compatibility:
    allow-old-keys-in-registry: false
    enum-compatibility-mode: false
spawn-limits:
  monsters: 70
  animals: 10
  water-animals: 5
  water-ambient: 20
  water-underground-creature: 5
  axolotls: 5
  ambient: 15
chunk-gc:
  period-in-ticks: 600
ticks-per:
  animal-spawns: 400
  monster-spawns: 1
  water-spawns: 1
  water-ambient-spawns: 1
  water-underground-creature-spawns: 1
  axolotl-spawns: 1
  ambient-spawns: 1
  autosave: 6000
aliases: now-in-commands.yml
"#;

fn merge(target: &mut Mapping, defaults: &Mapping) -> bool {
    let mut changed = false;
    for (key, default) in defaults {
        match target.get_mut(key) {
            Some(Value::Mapping(existing)) => {
                if let Value::Mapping(nested) = default {
                    changed |= merge(existing, nested);
                }
            }
            Some(_) => {}
            None => {
                target.insert(key.clone(), default.clone());
                changed = true;
            }
        }
    }
    changed
}

fn scalar(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

#[derive(Debug, Clone)]
pub struct YamlConfig {
    root: Value,
}

impl YamlConfig {
    pub fn load_or_create(path: &Path, defaults: &str) -> anyhow::Result<Self> {
        let defaults: Value = serde_yaml_ng::from_str(defaults)?;
        let mut root = match fs::read_to_string(path) {
            Ok(text) => serde_yaml_ng::from_str(&text)?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => Value::Mapping(Mapping::new()),
            Err(error) => return Err(error.into()),
        };
        if !matches!(root, Value::Mapping(_)) {
            root = Value::Mapping(Mapping::new());
        }
        let changed = match (&mut root, &defaults) {
            (Value::Mapping(target), Value::Mapping(defaults)) => merge(target, defaults),
            _ => false,
        };
        if changed || !path.exists() {
            fs::write(path, serde_yaml_ng::to_string(&root)?)?;
        }
        Ok(Self { root })
    }

    #[cfg(test)]
    pub fn from_str(text: &str) -> anyhow::Result<Self> {
        Ok(Self {
            root: serde_yaml_ng::from_str(text)?,
        })
    }

    fn get(&self, path: &str) -> Option<&Value> {
        path.split('.')
            .try_fold(&self.root, |node, key| node.get(key))
    }

    pub fn string(&self, path: &str, fallback: &str) -> String {
        self.get(path)
            .and_then(scalar)
            .unwrap_or_else(|| fallback.to_owned())
    }

    pub fn strings(&self, path: &str) -> Vec<String> {
        match self.get(path) {
            Some(Value::Sequence(items)) => items.iter().filter_map(scalar).collect(),
            _ => Vec::new(),
        }
    }

    pub fn entries(&self, path: &str) -> Vec<(String, String)> {
        let Some(Value::Mapping(entries)) = self.get(path) else {
            return Vec::new();
        };
        entries
            .iter()
            .filter_map(|(key, value)| {
                let entry = scalar(key).zip(scalar(value));
                if entry.is_none() {
                    let name = scalar(key).unwrap_or_else(|| format!("{key:?}"));
                    tracing::warn!("Ignoring `{path}.{name}`: expected a single value");
                }
                entry
            })
            .collect()
    }

    pub fn int(&self, path: &str, fallback: i64) -> i64 {
        self.get(path).and_then(Value::as_i64).unwrap_or(fallback)
    }

    pub fn float(&self, path: &str, fallback: f64) -> f64 {
        self.get(path).and_then(Value::as_f64).unwrap_or(fallback)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_parse_and_expose_spigot_values() {
        let spigot = YamlConfig::from_str(SPIGOT_DEFAULTS).unwrap();
        assert_eq!(spigot.int("settings.timeout-time", 0), 60);
        assert_eq!(
            spigot.int("world-settings.default.entity-tracking-range.players", 0),
            128
        );
        assert_eq!(
            spigot.string("world-settings.default.view-distance", ""),
            "default"
        );
        assert_eq!(
            spigot.string("messages.server-full", ""),
            "The server is full!"
        );
        assert_eq!(spigot.int("world-settings.default.mob-spawn-range", 0), 6);
        let bukkit = YamlConfig::from_str(BUKKIT_DEFAULTS).unwrap();
        assert_eq!(bukkit.int("ticks-per.autosave", 0), 6000);
        assert_eq!(bukkit.int("spawn-limits.monsters", 0), 70);
        assert_eq!(bukkit.int("ticks-per.animal-spawns", 0), 400);
    }

    #[test]
    fn merge_adds_missing_keys_and_keeps_user_values() {
        let mut target: Mapping =
            serde_yaml_ng::from_str("settings:\n  timeout-time: 30\n").unwrap();
        let defaults: Mapping = serde_yaml_ng::from_str(SPIGOT_DEFAULTS).unwrap();
        assert!(merge(&mut target, &defaults));
        let config = YamlConfig {
            root: Value::Mapping(target),
        };
        assert_eq!(config.int("settings.timeout-time", 0), 30);
        assert_eq!(config.int("settings.netty-threads", 0), 4);
    }
}
