use std::net::IpAddr;

use anyhow::Context;
use rspigot_core::config::{PluginSettings, PriorityOverride};
use rspigot_core::plugins::{EventKind, Priority};
use rspigot_plugins::{ExpectedPlugin, Launch, PluginHostConfig};

use super::yaml::YamlConfig;

pub const PLUGINS_DEFAULTS: &str = r#"enabled: true
host: 127.0.0.1
port: 25580
token: ''
decision-ticks: 1
late-cancel-ticks: 20
order: []
priorities: {}
expected: []
commands: {}
"#;

pub fn settings(file: &YamlConfig) -> PluginSettings {
    let settings = PluginSettings {
        decision_ticks: file.int("decision-ticks", 1).clamp(1, 20) as u64,
        late_cancel_ticks: file.int("late-cancel-ticks", 20).clamp(0, 100) as u64,
        order: file.strings("order"),
        overrides: file
            .entries("priorities")
            .iter()
            .filter_map(|(key, value)| priority_override(key, value))
            .collect(),
    };
    if refuses_every_late_cancel(&settings) {
        tracing::warn!(
            "plugins.yml: late-cancel-ticks ({}) is not above decision-ticks ({}), so every late cancel will be refused",
            settings.late_cancel_ticks,
            settings.decision_ticks
        );
    }
    settings
}

fn refuses_every_late_cancel(settings: &PluginSettings) -> bool {
    settings.late_cancel_ticks <= settings.decision_ticks
}

pub fn host_config(
    file: &YamlConfig,
    environment_token: Option<String>,
) -> anyhow::Result<PluginHostConfig> {
    let host = file.string("host", "127.0.0.1");
    let token = environment_token
        .filter(|token| !token.is_empty())
        .or_else(|| Some(file.string("token", "")).filter(|token| !token.is_empty()));
    Ok(PluginHostConfig {
        host: host
            .parse::<IpAddr>()
            .with_context(|| format!("plugins.yml: host `{host}` is not an IP address"))?,
        port: file.int("port", 25580).clamp(0, 65535) as u16,
        token,
    })
}

pub fn enabled(file: &YamlConfig) -> bool {
    !file.string("enabled", "true").eq_ignore_ascii_case("false")
}

pub fn expected(file: &YamlConfig) -> Vec<ExpectedPlugin> {
    let mut launches: Vec<(String, Option<Launch>)> = Vec::new();
    for name in file.strings("expected") {
        launches.push((name, None));
    }
    for (name, command) in file.entries("commands") {
        let Some(launch) = Launch::parse(&command) else {
            tracing::warn!("plugins.yml: `commands.{name}` is empty, ignoring it");
            continue;
        };
        match launches.iter_mut().find(|(known, _)| *known == name) {
            Some((_, slot)) => *slot = Some(launch),
            None => launches.push((name, Some(launch))),
        }
    }
    launches
        .into_iter()
        .map(|(name, launch)| ExpectedPlugin { name, launch })
        .collect()
}

fn priority_override(key: &str, value: &str) -> Option<PriorityOverride> {
    let Some(priority) = priority(value) else {
        tracing::warn!("plugins.yml: `{key}` has an unknown priority `{value}`");
        return None;
    };
    let (plugin, event) = match key.rsplit_once('.') {
        None => (key, None),
        Some((plugin, event)) => match event_kind(event) {
            Some(event) => (plugin, Some(event)),
            None => {
                tracing::warn!("plugins.yml: `{key}` names an unknown event `{event}`");
                return None;
            }
        },
    };
    Some(PriorityOverride {
        plugin: plugin.to_owned(),
        event,
        priority,
    })
}

fn priority(name: &str) -> Option<Priority> {
    Some(match name {
        "lowest" => Priority::Lowest,
        "low" => Priority::Low,
        "normal" => Priority::Normal,
        "high" => Priority::High,
        "highest" => Priority::Highest,
        _ => return None,
    })
}

fn event_kind(name: &str) -> Option<EventKind> {
    EventKind::from_key(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(text: &str) -> YamlConfig {
        YamlConfig::from_str(text).unwrap()
    }

    fn token(config: &YamlConfig, environment: Option<&str>) -> Option<String> {
        host_config(config, environment.map(str::to_owned))
            .unwrap()
            .token
    }

    #[test]
    fn defaults_listen_on_loopback_25580() {
        let defaults = file(PLUGINS_DEFAULTS);
        let host = host_config(&defaults, None).unwrap();
        assert_eq!(
            host,
            PluginHostConfig {
                host: IpAddr::from([127, 0, 0, 1]),
                port: 25580,
                token: None,
            }
        );
        assert!(host.validate().is_ok());
        assert_eq!(settings(&defaults), PluginSettings::default());
    }

    #[test]
    fn the_host_is_on_by_default_and_enabled_false_turns_it_off() {
        assert!(enabled(&file(PLUGINS_DEFAULTS)));
        assert!(enabled(&file("port: 1\n")));
        assert!(!enabled(&file("enabled: false\n")));
        assert!(!enabled(&file("enabled: 'False'\n")));
        assert!(enabled(&file("enabled: true\n")));
    }

    #[test]
    fn port_zero_asks_for_an_ephemeral_port() {
        let host = host_config(&file("port: 0\n"), None).unwrap();
        assert_eq!(host.port, 0);
        assert_eq!(host_config(&file("port: -4\n"), None).unwrap().port, 0);
        assert_eq!(
            host_config(&file("port: 70000\n"), None).unwrap().port,
            65535
        );
    }

    #[test]
    fn expected_plugins_come_from_the_list_and_from_declared_commands() {
        let config = file(
            "expected: [guard, welcome]\ncommands:\n  welcome: java -jar welcome.jar\n  shop: ./shop\n  empty: ''\n",
        );
        assert_eq!(
            expected(&config),
            vec![
                ExpectedPlugin {
                    name: "guard".to_owned(),
                    launch: None
                },
                ExpectedPlugin {
                    name: "welcome".to_owned(),
                    launch: Launch::parse("java -jar welcome.jar"),
                },
                ExpectedPlugin {
                    name: "shop".to_owned(),
                    launch: Launch::parse("./shop"),
                },
            ]
        );
        assert!(expected(&file(PLUGINS_DEFAULTS)).is_empty());
    }

    #[test]
    fn priorities_parse_per_plugin_and_per_event() {
        let config = file(
            "order: [guard, welcome]\npriorities:\n  welcome: high\n  welcome.player_move: lowest\n  guard.player_join: highest\n  guard.player_leave: low\n  guard.entity_damage_by_entity: lowest\n  guard.player_fly: high\n  ghost: urgent\n  guard:\n    player_move: high\n",
        );
        let settings = settings(&config);
        assert_eq!(settings.order, vec!["guard", "welcome"]);
        let rule = |plugin: &str, event, priority| PriorityOverride {
            plugin: plugin.to_owned(),
            event,
            priority,
        };
        assert_eq!(
            settings.overrides,
            vec![
                rule("welcome", None, Priority::High),
                rule("welcome", Some(EventKind::PlayerMove), Priority::Lowest),
                rule("guard", Some(EventKind::PlayerJoin), Priority::Highest),
                rule("guard", Some(EventKind::PlayerLeave), Priority::Low),
                rule(
                    "guard",
                    Some(EventKind::EntityDamageByEntity),
                    Priority::Lowest
                ),
            ]
        );
    }

    #[test]
    fn the_environment_token_wins_over_the_file() {
        let config = file("host: 0.0.0.0\ntoken: from-file\n");
        assert_eq!(
            token(&config, Some("from-env")).as_deref(),
            Some("from-env")
        );
        assert_eq!(token(&config, Some("")).as_deref(), Some("from-file"));
        assert_eq!(token(&config, None).as_deref(), Some("from-file"));
        let tokenless = file(PLUGINS_DEFAULTS);
        assert_eq!(
            token(&tokenless, Some("from-env")).as_deref(),
            Some("from-env")
        );
        assert_eq!(token(&tokenless, Some("")), None);
    }

    #[test]
    fn a_late_cancel_window_that_ends_by_the_deadline_is_empty() {
        let empty = |decision_ticks, late_cancel_ticks| {
            refuses_every_late_cancel(&PluginSettings {
                decision_ticks,
                late_cancel_ticks,
                ..PluginSettings::default()
            })
        };
        assert!(empty(20, 20));
        assert!(empty(5, 0));
        assert!(empty(1, 1));
        assert!(!empty(20, 21));
        assert!(!empty(1, 2));
        assert!(!refuses_every_late_cancel(&settings(&file(
            PLUGINS_DEFAULTS
        ))));
    }

    #[test]
    fn decision_windows_are_clamped() {
        let low = settings(&file("decision-ticks: 0\nlate-cancel-ticks: -5\n"));
        assert_eq!((low.decision_ticks, low.late_cancel_ticks), (1, 0));
        let high = settings(&file("decision-ticks: 400\nlate-cancel-ticks: 1000\n"));
        assert_eq!((high.decision_ticks, high.late_cancel_ticks), (20, 100));
    }
}
