use std::fs;
use std::io;
use std::path::Path;

use rspigot_core::permissions::Permissions;
use serde_yaml_ng::Value;

const EXAMPLE: &str = "# Groups and rights.\n\
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
