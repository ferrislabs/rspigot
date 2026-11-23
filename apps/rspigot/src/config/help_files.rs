use std::fs;
use std::io;
use std::path::Path;

use rspigot_core::commands::help::{Amendment, HelpConfig, HelpTopic, TopicKind};
use rspigot_core::config::CommandSettings;
use serde_yaml_ng::Value;

pub const COMMANDS_DEFAULTS: &str = r#"# This is the commands configuration file for Bukkit.
# For documentation on how to make use of this file, check out the Bukkit Wiki at
# https://www.spigotmc.org/go/commands-yml

command-block-overrides: []
ignore-vanilla-permissions: false
aliases:
    icanhasbukkit:
      - "version $1-"
"#;

pub const HELP_DEFAULTS: &str = "# This is the help configuration file for Bukkit.\n\
#\n\
# By default you do not need to modify this file. Help topics for all plugin commands are automatically provided by\n\
# or extracted from your installed plugins.\n\
#\n\
# command-topics-in-master-index: false\n\
# general-topics:\n\
#     Rules:\n\
#         shortText: Rules of the server\n\
#         fullText: |\n\
#             &61. Be kind to your fellow players.\n\
#         permission: topics.rules\n\
# index-topics:\n\
#     Ban Commands:\n\
#         shortText: Player banning commands\n\
#         preamble: Moderator - do not abuse these commands\n\
#         permission: op\n\
#         commands:\n\
#             - /ban\n\
#             - /ban-ip\n\
# amended-topics:\n\
#     /stop:\n\
#         shortText: Stops the server cold....in its tracks!\n\
#         fullText: <text> - This kills the server.\n\
# ignore-plugins:\n\
#    - PluginNameOne\n";

pub fn load_or_create(path: &Path, defaults: &str) -> anyhow::Result<String> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(text),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            fs::write(path, defaults)?;
            Ok(defaults.to_owned())
        }
        Err(error) => Err(error.into()),
    }
}

fn text(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::String(text) => Some(text.trim_end_matches('\n').to_owned()),
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(flag) => Some(flag.to_string()),
        _ => None,
    }
}

fn list(value: Option<&Value>) -> Vec<String> {
    match value {
        Some(Value::Sequence(items)) => items.iter().filter_map(|i| text(Some(i))).collect(),
        Some(Value::String(single)) => vec![single.clone()],
        _ => Vec::new(),
    }
}

pub fn parse_commands(source: &str) -> anyhow::Result<CommandSettings> {
    let root: Value = serde_yaml_ng::from_str(source)?;
    let mut settings = CommandSettings {
        ignore_vanilla_permissions: root
            .get("ignore-vanilla-permissions")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        command_block_overrides: list(root.get("command-block-overrides")),
        aliases: Vec::new(),
    };
    if let Some(aliases) = root.get("aliases").and_then(Value::as_mapping) {
        for (name, targets) in aliases {
            if let Some(name) = name.as_str() {
                let targets = list(Some(targets));
                if !targets.is_empty() {
                    settings.aliases.push((name.to_ascii_lowercase(), targets));
                }
            }
        }
    }
    Ok(settings)
}

pub fn parse_help(source: &str) -> anyhow::Result<HelpConfig> {
    let root: Value = serde_yaml_ng::from_str(source)?;
    let mut config = HelpConfig {
        command_topics_in_master_index: root
            .get("command-topics-in-master-index")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        ignore_plugins: list(root.get("ignore-plugins")),
        ..HelpConfig::default()
    };
    if let Some(topics) = root.get("general-topics").and_then(Value::as_mapping) {
        for (name, topic) in topics {
            let Some(name) = name.as_str() else { continue };
            let short = text(topic.get("shortText")).unwrap_or_default();
            let full = text(topic.get("fullText")).unwrap_or_else(|| short.clone());
            config.general.push(HelpTopic {
                name: name.to_owned(),
                short,
                full,
                permission: text(topic.get("permission")),
                kind: TopicKind::General,
                ..HelpTopic::default()
            });
        }
    }
    if let Some(topics) = root.get("index-topics").and_then(Value::as_mapping) {
        for (name, topic) in topics {
            let Some(name) = name.as_str() else { continue };
            config.indexes.push(HelpTopic {
                name: name.to_owned(),
                short: text(topic.get("shortText")).unwrap_or_default(),
                full: text(topic.get("preamble")).unwrap_or_default(),
                permission: text(topic.get("permission")),
                commands: list(topic.get("commands")),
                kind: TopicKind::Index,
                ..HelpTopic::default()
            });
        }
    }
    if let Some(topics) = root.get("amended-topics").and_then(Value::as_mapping) {
        for (name, topic) in topics {
            let Some(name) = name.as_str() else { continue };
            config.amended.push(Amendment {
                name: name.to_owned(),
                short: text(topic.get("shortText")),
                full: text(topic.get("fullText")),
                permission: text(topic.get("permission")),
            });
        }
    }
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_commands_file_defines_the_icanhasbukkit_alias() {
        let settings = parse_commands(COMMANDS_DEFAULTS).unwrap();
        assert_eq!(
            settings.alias("icanhasbukkit"),
            Some(vec!["version $1-".to_owned()])
        );
        assert!(!settings.ignore_vanilla_permissions);
        assert!(settings.command_block_overrides.is_empty());
    }

    #[test]
    fn the_default_help_file_changes_nothing() {
        assert_eq!(parse_help(HELP_DEFAULTS).unwrap(), HelpConfig::default());
    }

    #[test]
    fn a_help_file_reads_every_section() {
        let source = "command-topics-in-master-index: true\ngeneral-topics:\n  Rules:\n    shortText: Rules\n    fullText: |\n      &61. Be kind.\n    permission: topics.rules\nindex-topics:\n  Ban Commands:\n    shortText: Bans\n    preamble: Careful\n    permission: op\n    commands:\n      - /ban\namended-topics:\n  /stop:\n    shortText: Stops it\n    fullText: <text> - dead\nignore-plugins:\n  - Bukkit\n";
        let config = parse_help(source).unwrap();
        assert!(config.command_topics_in_master_index);
        assert_eq!(config.general[0].name, "Rules");
        assert_eq!(config.general[0].full, "&61. Be kind.");
        assert_eq!(
            config.general[0].permission.as_deref(),
            Some("topics.rules")
        );
        assert_eq!(config.indexes[0].commands, ["/ban"]);
        assert_eq!(config.indexes[0].full, "Careful");
        assert_eq!(config.amended[0].full.as_deref(), Some("<text> - dead"));
        assert_eq!(config.ignore_plugins, ["Bukkit"]);
    }

    #[test]
    fn aliases_accept_one_line_or_a_list_and_overrides_are_read() {
        let source = "command-block-overrides:\n  - tp\nignore-vanilla-permissions: true\naliases:\n  Home: spawn\n  two:\n    - say a\n    - say b\n";
        let settings = parse_commands(source).unwrap();
        assert!(settings.ignore_vanilla_permissions);
        assert_eq!(settings.command_block_overrides, ["tp"]);
        assert_eq!(settings.alias("home"), Some(vec!["spawn".to_owned()]));
        assert_eq!(settings.alias("TWO").unwrap().len(), 2);
    }

    #[test]
    fn a_missing_file_is_created_with_its_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("commands.yml");
        let first = load_or_create(&path, COMMANDS_DEFAULTS).unwrap();
        assert!(path.exists());
        assert_eq!(first, COMMANDS_DEFAULTS);
        std::fs::write(&path, "aliases: {}\n").unwrap();
        assert_eq!(
            load_or_create(&path, COMMANDS_DEFAULTS).unwrap(),
            "aliases: {}\n"
        );
    }
}
