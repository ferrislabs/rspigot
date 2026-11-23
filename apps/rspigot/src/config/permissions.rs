use std::fs;
use std::io;
use std::path::Path;

use rspigot_core::permissions::{NodeDefault, Permissions};
use serde_yaml_ng::Value;

const EXAMPLE: &str = "# Bukkit format: one entry per permission node.\n\
# my.plugin.use:\n\
#   description: Lets a player use the thing\n\
#   default: true            # true, false, op or not op\n\
# my.plugin.admin:\n\
#   default: op\n\
#   children:\n\
#     my.plugin.use: true\n\
#\n\
# rspigot groups (still read, they sit next to Bukkit nodes in the same file).\n\
# Groups and rights.\n\
# Every player belongs to the `default` group. A group lists its rights and the\n\
# groups it inherits. `-node` removes a right (it wins over any grant), `*`\n\
# grants everything and `kit.*` everything below `kit`. Operators of level 4\n\
# have every right.\n\
#\n\
# groups:\n\
#   default:\n\
#     permissions: [\"chat.use\"]\n\
#   vip:\n\
#     permissions: [\"kit.use\", \"chat.color\"]\n\
#     inherits: [default]\n\
#\n\
# A player is found by UUID or by name.\n\
# players:\n\
#   \"Steve\":\n\
#     groups: [vip]\n";

pub fn load_or_create(path: &Path) -> anyhow::Result<Permissions> {
    match fs::read_to_string(path) {
        Ok(text) => parse(&text),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            fs::write(path, EXAMPLE)?;
            Ok(Permissions::default())
        }
        Err(error) => Err(error.into()),
    }
}

pub fn parse(text: &str) -> anyhow::Result<Permissions> {
    let root: Value = serde_yaml_ng::from_str(text)?;
    let mut permissions = Permissions::default();
    if let Some(entries) = root.as_mapping() {
        for (name, node) in entries {
            let Some(name) = name.as_str() else { continue };
            if matches!(name, "groups" | "players") {
                continue;
            }
            if let Some(parsed) = bukkit_node(node) {
                let children: Vec<(&str, bool)> = parsed
                    .children
                    .iter()
                    .map(|(child, value)| (child.as_str(), *value))
                    .collect();
                permissions =
                    permissions.with_node(name, parsed.default, &parsed.description, &children);
            }
        }
    }
    if let Some(groups) = root.get("groups").and_then(Value::as_mapping) {
        for (name, group) in groups {
            let Some(name) = name.as_str() else { continue };
            permissions = permissions.with_group(
                name,
                &strings(group.get("permissions")),
                &strings(group.get("inherits")),
            );
        }
    }
    if let Some(players) = root.get("players").and_then(Value::as_mapping) {
        for (key, player) in players {
            let Some(key) = key.as_str() else { continue };
            permissions = permissions.with_player(key, &strings(player.get("groups")));
        }
    }
    Ok(permissions)
}

struct BukkitNode {
    default: NodeDefault,
    description: String,
    children: Vec<(String, bool)>,
}

fn bukkit_node(node: &Value) -> Option<BukkitNode> {
    match node {
        Value::Mapping(_) => {}
        Value::Null => {
            return Some(BukkitNode {
                default: NodeDefault::default(),
                description: String::new(),
                children: Vec::new(),
            });
        }
        _ => return None,
    }
    let default = match node.get("default") {
        Some(Value::Bool(true)) => NodeDefault::True,
        Some(Value::Bool(false)) | None => NodeDefault::False,
        Some(Value::String(text)) => NodeDefault::parse(text).unwrap_or_default(),
        Some(_) => NodeDefault::default(),
    };
    let description = node
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let mut children = Vec::new();
    match node.get("children") {
        Some(Value::Mapping(map)) => {
            for (child, value) in map {
                if let Some(child) = child.as_str() {
                    children.push((child.to_owned(), value.as_bool().unwrap_or(true)));
                }
            }
        }
        Some(Value::Sequence(list)) => {
            children.extend(
                list.iter()
                    .filter_map(Value::as_str)
                    .map(|c| (c.to_owned(), true)),
            );
        }
        _ => {}
    }
    Some(BukkitNode {
        default,
        description,
        children,
    })
}

fn strings(value: Option<&Value>) -> Vec<&str> {
    value
        .and_then(Value::as_sequence)
        .map(|items| items.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rspigot_domain::player::PlayerId;

    const FILE: &str = "groups:\n  default:\n    permissions: [chat.use]\n  vip:\n    permissions: [kit.use, chat.color]\n    inherits: [default]\nplayers:\n  Steve:\n    groups: [vip]\n";

    #[test]
    fn a_file_gives_its_groups_and_players() {
        let permissions = parse(FILE).unwrap();
        let steve = permissions.rights(PlayerId::offline("Steve"), "Steve", 0);
        assert!(steve.has("kit.use") && steve.has("chat.use"));
        let alex = permissions.rights(PlayerId::offline("Alex"), "Alex", 0);
        assert!(alex.has("chat.use") && !alex.has("kit.use"));
    }

    const BUKKIT: &str = "my.plugin.use:\n  description: Use it\n  default: true\nmy.plugin.admin:\n  default: op\n  children:\n    my.plugin.use: true\n    my.plugin.ban: false\nmy.plugin.vip:\n  default: \"not op\"\nmy.plugin.listed:\n  default: false\n  children:\n    - my.plugin.a\n    - my.plugin.b\nmy.plugin.bare:\n";

    #[test]
    fn a_bukkit_file_gives_defaults_children_and_descriptions() {
        let permissions = parse(BUKKIT).unwrap();
        let player = permissions.rights(PlayerId::offline("A"), "A", 0);
        assert!(player.has("my.plugin.use") && player.has("my.plugin.vip"));
        assert!(!player.has("my.plugin.admin") && !player.has("my.plugin.bare"));
        let operator = permissions.rights(PlayerId::offline("O"), "O", 2);
        assert!(operator.has("my.plugin.admin") && operator.has("my.plugin.use"));
        assert!(!operator.has("my.plugin.ban") && !operator.has("my.plugin.vip"));
        assert_eq!(
            permissions.node_description("my.plugin.use"),
            Some("Use it")
        );
        assert!(permissions.has_node("my.plugin.bare"));
    }

    #[test]
    fn bukkit_children_can_be_listed_and_follow_their_parent_through_a_group() {
        let text = format!("{BUKKIT}groups:\n  default:\n    permissions: [my.plugin.listed]\n");
        let permissions = parse(&text).unwrap();
        let rights = permissions.rights(PlayerId::offline("A"), "A", 0);
        assert!(rights.has("my.plugin.a") && rights.has("my.plugin.b"));
    }

    #[test]
    fn the_rspigot_format_still_loads_next_to_bukkit_nodes() {
        let text = format!("{BUKKIT}{FILE}");
        let permissions = parse(&text).unwrap();
        let steve = permissions.rights(PlayerId::offline("Steve"), "Steve", 0);
        assert!(steve.has("kit.use") && steve.has("my.plugin.use"));
    }

    #[test]
    fn a_missing_file_is_created_with_an_example_and_grants_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("permissions.yml");
        let permissions = load_or_create(&path).unwrap();
        assert!(path.exists());
        assert!(
            !permissions
                .rights(PlayerId::offline("A"), "A", 0)
                .has("chat.use")
        );
        assert_eq!(load_or_create(&path).unwrap(), Permissions::default());
    }
}
