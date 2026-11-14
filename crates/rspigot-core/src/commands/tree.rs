#[derive(Debug, Clone, PartialEq)]
pub enum ArgumentParser {
    Bool,
    Integer { min: Option<i32>, max: Option<i32> },
    Float { min: Option<f32>, max: Option<f32> },
    Double { min: Option<f64>, max: Option<f64> },
    Word,
    QuotedString,
    GreedyString,
    Entity { single: bool, players_only: bool },
    GameProfile,
    BlockPos,
    Vec3,
    BlockState,
    ItemStack,
    Message,
    Time { min: i32 },
    GameMode,
    Dimension,
    ResourceLocation,
    Resource { registry: String },
    NbtCompoundTag,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CommandNodeKind {
    Root,
    Literal(String),
    Argument {
        name: String,
        parser: ArgumentParser,
        suggests: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct CommandNode {
    pub kind: CommandNodeKind,
    pub executable: bool,
    pub children: Vec<usize>,
    pub redirect: Option<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CommandTree {
    pub nodes: Vec<CommandNode>,
    pub root: usize,
}

struct TreeBuilder {
    nodes: Vec<CommandNode>,
}

impl TreeBuilder {
    fn add(&mut self, parent: usize, kind: CommandNodeKind, executable: bool) -> usize {
        self.nodes.push(CommandNode {
            kind,
            executable,
            children: Vec::new(),
            redirect: None,
        });
        let index = self.nodes.len() - 1;
        self.nodes[parent].children.push(index);
        index
    }

    fn literal(&mut self, parent: usize, name: &str, executable: bool) -> usize {
        self.add(
            parent,
            CommandNodeKind::Literal(name.to_owned()),
            executable,
        )
    }

    fn argument(
        &mut self,
        parent: usize,
        name: &str,
        parser: ArgumentParser,
        executable: bool,
    ) -> usize {
        self.add(
            parent,
            CommandNodeKind::Argument {
                name: name.to_owned(),
                parser,
                suggests: None,
            },
            executable,
        )
    }

    fn suggesting(
        &mut self,
        parent: usize,
        name: &str,
        parser: ArgumentParser,
        suggests: &str,
        executable: bool,
    ) -> usize {
        self.add(
            parent,
            CommandNodeKind::Argument {
                name: name.to_owned(),
                parser,
                suggests: Some(suggests.to_owned()),
            },
            executable,
        )
    }

    fn alias(&mut self, parent: usize, name: &str, target: usize) {
        let index = self.literal(parent, name, false);
        self.nodes[index].redirect = Some(target);
    }
}

fn entities() -> ArgumentParser {
    ArgumentParser::Entity {
        single: false,
        players_only: false,
    }
}

fn single_entity() -> ArgumentParser {
    ArgumentParser::Entity {
        single: true,
        players_only: false,
    }
}

fn mob_effect() -> ArgumentParser {
    ArgumentParser::Resource {
        registry: "minecraft:mob_effect".to_owned(),
    }
}

fn player_arg() -> ArgumentParser {
    ArgumentParser::Entity {
        single: true,
        players_only: true,
    }
}

pub fn build_tree(operator: bool) -> CommandTree {
    let mut b = TreeBuilder {
        nodes: vec![CommandNode {
            kind: CommandNodeKind::Root,
            executable: false,
            children: Vec::new(),
            redirect: None,
        }],
    };
    let root = 0;
    for name in ["help", "list", "seed", "version", "plugins", "tps"] {
        b.literal(root, name, true);
    }
    let me = b.literal(root, "me", false);
    b.argument(me, "action", ArgumentParser::Message, true);
    let msg = b.literal(root, "msg", false);
    let target = b.argument(msg, "targets", player_arg(), false);
    b.argument(target, "message", ArgumentParser::Message, true);
    b.alias(root, "tell", msg);
    b.alias(root, "w", msg);
    if operator {
        let say = b.literal(root, "say", false);
        b.argument(say, "message", ArgumentParser::Message, true);
        let gamemode = b.literal(root, "gamemode", false);
        for mode in ["survival", "creative", "adventure", "spectator"] {
            let m = b.literal(gamemode, mode, true);
            b.argument(m, "target", player_arg(), true);
        }
        let teleport = b.literal(root, "teleport", false);
        let targets = b.argument(teleport, "targets", entities(), false);
        b.argument(targets, "location", ArgumentParser::Vec3, true);
        b.argument(targets, "destination", single_entity(), true);
        b.argument(teleport, "location", ArgumentParser::Vec3, true);
        b.argument(teleport, "destination", single_entity(), true);
        b.alias(root, "tp", teleport);
        let execute = b.literal(root, "execute", false);
        let within = b.literal(execute, "in", false);
        let dimension = b.argument(within, "dimension", ArgumentParser::Dimension, false);
        let run = b.literal(dimension, "run", false);
        b.nodes[run].redirect = Some(root);
        let summon = b.literal(root, "summon", false);
        let entity = b.suggesting(
            summon,
            "entity",
            ArgumentParser::Resource {
                registry: "minecraft:entity_type".to_owned(),
            },
            "minecraft:summonable_entities",
            true,
        );
        let pos = b.argument(entity, "pos", ArgumentParser::Vec3, true);
        b.argument(pos, "nbt", ArgumentParser::NbtCompoundTag, true);
        let time = b.literal(root, "time", false);
        let set = b.literal(time, "set", false);
        for preset in ["day", "noon", "night", "midnight"] {
            b.literal(set, preset, true);
        }
        b.argument(set, "time", ArgumentParser::Time { min: 0 }, true);
        let add = b.literal(time, "add", false);
        b.argument(add, "time", ArgumentParser::Time { min: 0 }, true);
        let query = b.literal(time, "query", false);
        for q in ["daytime", "gametime", "day"] {
            b.literal(query, q, true);
        }
        let give = b.literal(root, "give", false);
        let who = b.argument(give, "targets", player_arg(), false);
        let item = b.argument(who, "item", ArgumentParser::ItemStack, true);
        b.argument(
            item,
            "count",
            ArgumentParser::Integer {
                min: Some(1),
                max: Some(6400),
            },
            true,
        );
        let clear = b.literal(root, "clear", true);
        b.argument(clear, "targets", player_arg(), true);
        let kill = b.literal(root, "kill", true);
        b.argument(kill, "targets", entities(), true);
        let kick = b.literal(root, "kick", false);
        let kicked = b.argument(kick, "targets", player_arg(), true);
        b.argument(kicked, "reason", ArgumentParser::Message, true);
        for name in ["op", "deop"] {
            let node = b.literal(root, name, false);
            b.argument(node, "targets", ArgumentParser::GameProfile, true);
        }
        let difficulty = b.literal(root, "difficulty", true);
        for d in ["peaceful", "easy", "normal", "hard"] {
            b.literal(difficulty, d, true);
        }
        let weather = b.literal(root, "weather", false);
        for w in ["clear", "rain", "thunder"] {
            b.literal(weather, w, true);
        }
        let spawn = b.literal(root, "setworldspawn", true);
        b.argument(spawn, "pos", ArgumentParser::BlockPos, true);
        let effect = b.literal(root, "effect", false);
        let clear = b.literal(effect, "clear", true);
        let clear_targets = b.argument(clear, "targets", entities(), true);
        b.argument(clear_targets, "effect", mob_effect(), true);
        let give = b.literal(effect, "give", false);
        let give_targets = b.argument(give, "targets", entities(), false);
        let give_effect = b.argument(give_targets, "effect", mob_effect(), true);
        let seconds = b.argument(
            give_effect,
            "seconds",
            ArgumentParser::Integer {
                min: Some(1),
                max: Some(1_000_000),
            },
            true,
        );
        let infinite = b.literal(give_effect, "infinite", true);
        for duration in [seconds, infinite] {
            let amplifier = b.argument(
                duration,
                "amplifier",
                ArgumentParser::Integer {
                    min: Some(0),
                    max: Some(255),
                },
                true,
            );
            b.argument(amplifier, "hideParticles", ArgumentParser::Bool, true);
        }
        let whitelist = b.literal(root, "whitelist", false);
        for action in ["on", "off", "list", "reload"] {
            b.literal(whitelist, action, true);
        }
        for action in ["add", "remove"] {
            let node = b.literal(whitelist, action, false);
            b.argument(node, "targets", ArgumentParser::GameProfile, true);
        }
        let ban = b.literal(root, "ban", false);
        let banned = b.argument(ban, "targets", ArgumentParser::GameProfile, true);
        b.argument(banned, "reason", ArgumentParser::Message, true);
        let ban_ip = b.literal(root, "ban-ip", false);
        let address = b.argument(ban_ip, "target", ArgumentParser::Word, true);
        b.argument(address, "reason", ArgumentParser::Message, true);
        let pardon = b.literal(root, "pardon", false);
        b.argument(pardon, "targets", ArgumentParser::GameProfile, true);
        let pardon_ip = b.literal(root, "pardon-ip", false);
        b.argument(pardon_ip, "target", ArgumentParser::Word, true);
        let banlist = b.literal(root, "banlist", true);
        for list in ["ips", "players"] {
            b.literal(banlist, list, true);
        }
        b.literal(root, "save-all", true);
        b.literal(root, "stop", true);
    }
    CommandTree {
        nodes: b.nodes,
        root,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operator_tree_contains_gamemode_and_redirects_resolve() {
        let tree = build_tree(true);
        let names: Vec<&str> = tree.nodes[tree.root]
            .children
            .iter()
            .filter_map(|i| match &tree.nodes[*i].kind {
                CommandNodeKind::Literal(name) => Some(name.as_str()),
                _ => None,
            })
            .collect();
        assert!(names.contains(&"gamemode"));
        assert!(names.contains(&"tp"));
        assert!(names.contains(&"summon"));
        assert!(names.contains(&"execute"));
        for name in ["ban", "ban-ip", "pardon", "pardon-ip", "banlist"] {
            assert!(names.contains(&name), "{name}");
        }
        let summon_entity = tree.nodes.iter().find(|n| {
            matches!(&n.kind, CommandNodeKind::Argument { name, suggests, .. }
                if name == "entity" && suggests.as_deref() == Some("minecraft:summonable_entities"))
        });
        assert!(summon_entity.is_some());
        for node in &tree.nodes {
            if let Some(target) = node.redirect {
                assert!(target < tree.nodes.len());
            }
            assert!(node.children.iter().all(|c| *c < tree.nodes.len()));
        }
    }

    #[test]
    fn non_operator_tree_hides_admin_commands() {
        let tree = build_tree(false);
        let has_stop = tree
            .nodes
            .iter()
            .any(|n| n.kind == CommandNodeKind::Literal("stop".to_owned()));
        assert!(!has_stop);
        let has_ban = tree
            .nodes
            .iter()
            .any(|n| n.kind == CommandNodeKind::Literal("ban".to_owned()));
        assert!(!has_ban);
    }
}
