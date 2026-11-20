use super::ServerFiles;

pub const UNKNOWN_COMMAND: &str = "Unknown command. Type \"/help\" for help.";
pub const SERVER_FULL: &str = "The server is full!";

struct Property {
    key: &'static str,
    default: &'static str,
}

const PROPERTIES: [Property; 14] = [
    Property {
        key: "generate-structures",
        default: "true",
    },
    Property {
        key: "max-world-size",
        default: "29999984",
    },
    Property {
        key: "rate-limit",
        default: "0",
    },
    Property {
        key: "chat-spam-threshold-seconds",
        default: "10",
    },
    Property {
        key: "command-spam-threshold-seconds",
        default: "10",
    },
    Property {
        key: "broadcast-console-to-ops",
        default: "true",
    },
    Property {
        key: "broadcast-rcon-to-ops",
        default: "true",
    },
    Property {
        key: "enable-jmx-monitoring",
        default: "false",
    },
    Property {
        key: "use-native-transport",
        default: "true",
    },
    Property {
        key: "initial-enabled-packs",
        default: "vanilla",
    },
    Property {
        key: "initial-disabled-packs",
        default: "",
    },
    Property {
        key: "max-chained-neighbor-updates",
        default: "1000000",
    },
    Property {
        key: "enable-code-of-conduct",
        default: "false",
    },
    Property {
        key: "debug",
        default: "false",
    },
];

const MORE_PROPERTIES: [Property; 2] = [
    Property {
        key: "text-filtering-config",
        default: "",
    },
    Property {
        key: "text-filtering-version",
        default: "0",
    },
];

struct Number {
    path: &'static str,
    default: f64,
}

const SPIGOT_NUMBERS: [Number; 10] = [
    Number {
        path: "settings.timeout-time",
        default: 60.0,
    },
    Number {
        path: "world-settings.default.merge-radius.exp",
        default: -1.0,
    },
    Number {
        path: "world-settings.default.ticks-per.hopper-check",
        default: 1.0,
    },
    Number {
        path: "world-settings.default.hunger.jump-walk-exhaustion",
        default: 0.05,
    },
    Number {
        path: "world-settings.default.hunger.jump-sprint-exhaustion",
        default: 0.2,
    },
    Number {
        path: "world-settings.default.hunger.regen-exhaustion",
        default: 6.0,
    },
    Number {
        path: "world-settings.default.hunger.swim-multiplier",
        default: 0.01,
    },
    Number {
        path: "world-settings.default.hunger.sprint-multiplier",
        default: 0.1,
    },
    Number {
        path: "world-settings.default.hunger.other-multiplier",
        default: 0.0,
    },
    Number {
        path: "commands.tab-complete",
        default: 0.0,
    },
];

fn differs(value: f64, default: f64) -> bool {
    (value - default).abs() > 1e-9
}

pub fn warnings(files: &ServerFiles) -> Vec<String> {
    let mut found = Vec::new();
    for property in PROPERTIES.iter().chain(&MORE_PROPERTIES) {
        let value = files.properties.string(property.key);
        if !value.eq_ignore_ascii_case(property.default) {
            found.push(format!(
                "server.properties key \"{}\" is read but has no effect in rspigot",
                property.key
            ));
        }
    }
    for number in &SPIGOT_NUMBERS {
        if differs(
            files.spigot.float(number.path, number.default),
            number.default,
        ) {
            found.push(format!(
                "spigot.yml key \"{}\" is read but has no effect in rspigot",
                number.path
            ));
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{BUKKIT_DEFAULTS, Properties, SPIGOT_DEFAULTS, YamlConfig};

    fn files(properties: &str, spigot: &str) -> ServerFiles {
        fn deep(root: &mut serde_yaml_ng::Value, user: serde_yaml_ng::Value) {
            match (root, user) {
                (serde_yaml_ng::Value::Mapping(root), serde_yaml_ng::Value::Mapping(user)) => {
                    for (key, value) in user {
                        match root.get_mut(&key) {
                            Some(existing) => deep(existing, value),
                            None => {
                                root.insert(key, value);
                            }
                        }
                    }
                }
                (root, user) => *root = user,
            }
        }
        let merged = |text: &str, defaults: &str| {
            let mut root: serde_yaml_ng::Value = serde_yaml_ng::from_str(defaults).unwrap();
            let user: serde_yaml_ng::Value = serde_yaml_ng::from_str(text).unwrap();
            deep(&mut root, user);
            YamlConfig::from_str(&serde_yaml_ng::to_string(&root).unwrap()).unwrap()
        };
        ServerFiles {
            properties: Properties::parse(properties),
            spigot: merged(spigot, SPIGOT_DEFAULTS),
            bukkit: merged("{}", BUKKIT_DEFAULTS),
            plugins: YamlConfig::from_str(super::super::PLUGINS_DEFAULTS).unwrap(),
        }
    }

    fn defaults() -> ServerFiles {
        defaults_with("{}")
    }

    fn defaults_with(spigot: &str) -> ServerFiles {
        let mut files = files("", spigot);
        for (key, value) in super::super::properties::DEFAULTS {
            files.properties.set(key, value);
        }
        files
    }

    #[test]
    fn a_default_server_warns_about_nothing() {
        assert_eq!(warnings(&defaults()), Vec::<String>::new());
    }

    #[test]
    fn every_unapplied_property_set_to_a_non_default_value_names_its_key() {
        for property in PROPERTIES.iter().chain(&MORE_PROPERTIES) {
            let mut files = defaults();
            files.properties.set(property.key, "changed-value");
            let found = warnings(&files);
            assert_eq!(found.len(), 1, "{}", property.key);
            assert!(found[0].contains(property.key), "{}", property.key);
        }
    }

    #[test]
    fn every_unapplied_spigot_number_names_its_path_when_changed() {
        for number in &SPIGOT_NUMBERS {
            let value = serde_yaml_ng::Value::from(number.default + 3.0);
            let tree = nest(number.path, value);
            let text = serde_yaml_ng::to_string(&tree).unwrap();
            let found = warnings(&defaults_with(&text));
            assert_eq!(found.len(), 1, "{}", number.path);
            assert!(found[0].contains(number.path), "{}", number.path);
        }
    }

    fn nest(path: &str, leaf: serde_yaml_ng::Value) -> serde_yaml_ng::Value {
        path.rsplit('.').fold(leaf, |inner, key| {
            let mut map = serde_yaml_ng::Mapping::new();
            map.insert(key.into(), inner);
            serde_yaml_ng::Value::Mapping(map)
        })
    }
}
