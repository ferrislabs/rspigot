use rspigot_protocol::play::commands::{
    ArgumentParser, CommandNode, CommandNodeKind, Commands, SimpleParser, StringKind, parser,
};

pub const SUGGEST_FROM_PROXY: &str = "minecraft:ask_server";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArgumentKind {
    Word,
    GreedyText,
    Player,
    Server,
    Integer,
    Choice,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandArgument {
    pub name: String,
    pub kind: ArgumentKind,
    pub optional: bool,
    pub choices: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandSpec {
    pub name: String,
    pub aliases: Vec<String>,
    pub permission: Option<String>,
    pub arguments: Vec<CommandArgument>,
}

impl CommandSpec {
    fn labels(&self) -> impl Iterator<Item = &str> {
        std::iter::once(self.name.as_str()).chain(self.aliases.iter().map(String::as_str))
    }

    fn answers(&self, label: &str) -> bool {
        self.labels().any(|own| own.eq_ignore_ascii_case(label))
    }

    fn allowed(&self, allows: &impl Fn(&str) -> bool) -> bool {
        self.permission.as_deref().is_none_or(allows)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invocation {
    pub owner: u64,
    pub command: String,
    pub label: String,
    pub arguments: Vec<String>,
    pub line: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Suggestions {
    pub start: usize,
    pub length: usize,
    pub matches: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct CommandRegistry {
    commands: Vec<(u64, CommandSpec)>,
}

impl CommandRegistry {
    pub fn register(&mut self, owner: u64, spec: CommandSpec) {
        self.commands
            .retain(|(_, existing)| !existing.name.eq_ignore_ascii_case(&spec.name));
        self.commands.push((owner, spec));
    }

    pub fn unregister(&mut self, owner: u64, name: &str) {
        self.commands.retain(|(existing_owner, spec)| {
            *existing_owner != owner || !spec.name.eq_ignore_ascii_case(name)
        });
    }

    pub fn remove_owner(&mut self, owner: u64) {
        self.commands
            .retain(|(existing_owner, _)| *existing_owner != owner);
    }

    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    pub fn parse(&self, line: &str, allows: impl Fn(&str) -> bool) -> Option<Invocation> {
        let line = line.strip_prefix('/').unwrap_or(line);
        let (label, rest) = line.split_once(' ').unwrap_or((line, ""));
        let (owner, spec) = self.find(label)?;
        if !spec.allowed(&allows) {
            return None;
        }
        Some(Invocation {
            owner,
            command: spec.name.clone(),
            label: label.to_owned(),
            arguments: split_arguments(rest, &spec.arguments),
            line: line.to_owned(),
        })
    }

    pub fn inject(&self, tree: &mut Commands, allows: impl Fn(&str) -> bool) {
        let Ok(root) = usize::try_from(tree.root_index) else {
            return;
        };
        if root >= tree.nodes.len() {
            return;
        }
        let visible: Vec<&CommandSpec> = self
            .commands
            .iter()
            .map(|(_, spec)| spec)
            .filter(|spec| spec.allowed(&allows))
            .collect();
        if visible.is_empty() {
            return;
        }
        let taken = |name: &str| visible.iter().any(|spec| spec.answers(name));
        let kept: Vec<i32> = tree.nodes[root]
            .children
            .iter()
            .copied()
            .filter(|child| {
                usize::try_from(*child)
                    .ok()
                    .and_then(|index| tree.nodes.get(index))
                    .is_none_or(|node| match &node.kind {
                        CommandNodeKind::Literal { name } => !taken(name),
                        _ => true,
                    })
            })
            .collect();
        let mut added = Vec::new();
        for spec in visible {
            for label in spec.labels() {
                added.push(push_chain(&mut tree.nodes, label, &spec.arguments));
            }
        }
        let root_node = &mut tree.nodes[root];
        root_node.children = kept;
        root_node.children.extend(added);
    }

    pub fn suggest(
        &self,
        text: &str,
        servers: &[String],
        players: &[String],
        allows: impl Fn(&str) -> bool,
    ) -> Option<Suggestions> {
        let offset = usize::from(text.starts_with('/'));
        let line = &text[offset..];
        let (label, rest) = line.split_once(' ')?;
        let (_, spec) = self.find(label)?;
        if !spec.allowed(&allows) {
            return None;
        }
        let words: Vec<&str> = rest.split(' ').collect();
        let index = words.len() - 1;
        let partial = words[index];
        let argument = spec.arguments.get(index)?;
        let pool: Vec<String> = match argument.kind {
            ArgumentKind::Server => servers.to_vec(),
            ArgumentKind::Player => players.to_vec(),
            ArgumentKind::Choice => argument.choices.clone(),
            _ => return None,
        };
        let lower = partial.to_ascii_lowercase();
        Some(Suggestions {
            start: text.len() - partial.len(),
            length: partial.len(),
            matches: pool
                .into_iter()
                .filter(|candidate| candidate.to_ascii_lowercase().starts_with(&lower))
                .collect(),
        })
    }

    fn find(&self, label: &str) -> Option<(u64, &CommandSpec)> {
        self.commands
            .iter()
            .find(|(_, spec)| spec.answers(label))
            .map(|(owner, spec)| (*owner, spec))
    }
}

fn split_arguments(rest: &str, arguments: &[CommandArgument]) -> Vec<String> {
    let rest = rest.trim_start();
    if rest.is_empty() {
        return Vec::new();
    }
    let greedy_at = arguments
        .iter()
        .position(|argument| argument.kind == ArgumentKind::GreedyText);
    match greedy_at {
        Some(at) => {
            let mut parts: Vec<String> = rest.splitn(at + 1, ' ').map(str::to_owned).collect();
            parts.retain(|part| !part.is_empty());
            parts
        }
        None => rest.split_whitespace().map(str::to_owned).collect(),
    }
}

fn push_chain(nodes: &mut Vec<CommandNode>, label: &str, arguments: &[CommandArgument]) -> i32 {
    let mut next: Option<i32> = None;
    for (index, argument) in arguments.iter().enumerate().rev() {
        let executable = arguments
            .get(index + 1)
            .is_none_or(|following| following.optional);
        let (parser, suggestions) = parser_for(argument.kind);
        nodes.push(CommandNode {
            kind: CommandNodeKind::Argument {
                name: argument.name.clone(),
                parser,
                suggestions,
            },
            executable,
            restricted: false,
            children: next.into_iter().collect(),
            redirect: None,
        });
        next = Some(last_index(nodes));
    }
    nodes.push(CommandNode {
        kind: CommandNodeKind::Literal {
            name: label.to_owned(),
        },
        executable: arguments.first().is_none_or(|first| first.optional),
        restricted: false,
        children: next.into_iter().collect(),
        redirect: None,
    });
    last_index(nodes)
}

fn last_index(nodes: &[CommandNode]) -> i32 {
    i32::try_from(nodes.len() - 1).unwrap_or(i32::MAX)
}

fn parser_for(kind: ArgumentKind) -> (ArgumentParser, Option<String>) {
    let ask = Some(SUGGEST_FROM_PROXY.to_owned());
    match kind {
        ArgumentKind::Word => (ArgumentParser::String(StringKind::SingleWord), None),
        ArgumentKind::GreedyText => (ArgumentParser::String(StringKind::GreedyPhrase), None),
        ArgumentKind::Player => (
            SimpleParser::new(parser::GAME_PROFILE).map_or(
                ArgumentParser::String(StringKind::SingleWord),
                ArgumentParser::Simple,
            ),
            None,
        ),
        ArgumentKind::Server | ArgumentKind::Choice => {
            (ArgumentParser::String(StringKind::SingleWord), ask)
        }
        ArgumentKind::Integer => (
            ArgumentParser::Integer {
                min: None,
                max: None,
            },
            None,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argument(name: &str, kind: ArgumentKind, optional: bool) -> CommandArgument {
        CommandArgument {
            name: name.to_owned(),
            kind,
            optional,
            choices: Vec::new(),
        }
    }

    fn registry() -> CommandRegistry {
        let mut registry = CommandRegistry::default();
        registry.register(
            1,
            CommandSpec {
                name: "hub".to_owned(),
                aliases: vec!["lobby".to_owned()],
                permission: None,
                arguments: Vec::new(),
            },
        );
        registry.register(
            1,
            CommandSpec {
                name: "server".to_owned(),
                aliases: Vec::new(),
                permission: Some("proxy.command.server".to_owned()),
                arguments: vec![argument("server", ArgumentKind::Server, true)],
            },
        );
        registry.register(
            2,
            CommandSpec {
                name: "msg".to_owned(),
                aliases: Vec::new(),
                permission: None,
                arguments: vec![
                    argument("player", ArgumentKind::Player, false),
                    argument("message", ArgumentKind::GreedyText, false),
                ],
            },
        );
        registry
    }

    fn everyone(_: &str) -> bool {
        false
    }

    fn staff(_: &str) -> bool {
        true
    }

    fn backend_tree() -> Commands {
        Commands {
            nodes: vec![
                CommandNode {
                    kind: CommandNodeKind::Root,
                    executable: false,
                    restricted: false,
                    children: vec![1, 2],
                    redirect: None,
                },
                CommandNode {
                    kind: CommandNodeKind::Literal {
                        name: "hub".to_owned(),
                    },
                    executable: true,
                    restricted: false,
                    children: Vec::new(),
                    redirect: None,
                },
                CommandNode {
                    kind: CommandNodeKind::Literal {
                        name: "gamemode".to_owned(),
                    },
                    executable: true,
                    restricted: false,
                    children: Vec::new(),
                    redirect: None,
                },
            ],
            root_index: 0,
        }
    }

    fn root_literals(tree: &Commands) -> Vec<String> {
        tree.nodes[0]
            .children
            .iter()
            .filter_map(|child| match &tree.nodes[*child as usize].kind {
                CommandNodeKind::Literal { name } => Some(name.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_command_line_resolves_to_its_owner_and_arguments() {
        let invocation = registry().parse("/lobby", everyone).unwrap();
        assert_eq!(invocation.owner, 1);
        assert_eq!(invocation.command, "hub");
        assert_eq!(invocation.label, "lobby");
        assert!(invocation.arguments.is_empty());

        let invocation = registry()
            .parse("msg Steve hello there  friend", everyone)
            .unwrap();
        assert_eq!(invocation.owner, 2);
        assert_eq!(invocation.arguments, ["Steve", "hello there  friend"]);
        assert_eq!(invocation.line, "msg Steve hello there  friend");
    }

    #[test]
    fn unknown_or_forbidden_commands_go_to_the_backend() {
        assert!(registry().parse("gamemode 1", staff).is_none());
        assert!(registry().parse("server lobby", everyone).is_none());
        let invocation = registry().parse("SERVER lobby", staff).unwrap();
        assert_eq!(invocation.arguments, ["lobby"]);
    }

    #[test]
    fn the_tree_gains_the_visible_commands_and_loses_the_backend_duplicates() {
        let mut tree = backend_tree();
        registry().inject(&mut tree, everyone);
        assert_eq!(root_literals(&tree), ["gamemode", "hub", "lobby", "msg"]);

        let mut tree = backend_tree();
        registry().inject(&mut tree, staff);
        assert_eq!(
            root_literals(&tree),
            ["gamemode", "hub", "lobby", "server", "msg"]
        );
    }

    #[test]
    fn optional_arguments_make_the_previous_node_executable() {
        let mut tree = backend_tree();
        registry().inject(&mut tree, staff);
        let server = tree
            .nodes
            .iter()
            .find(
                |node| matches!(&node.kind, CommandNodeKind::Literal { name } if name == "server"),
            )
            .unwrap();
        assert!(server.executable);
        let argument = &tree.nodes[server.children[0] as usize];
        assert!(argument.executable);
        assert!(matches!(
            &argument.kind,
            CommandNodeKind::Argument { suggestions: Some(ask), .. } if ask == SUGGEST_FROM_PROXY
        ));

        let msg = tree
            .nodes
            .iter()
            .find(|node| matches!(&node.kind, CommandNodeKind::Literal { name } if name == "msg"))
            .unwrap();
        assert!(!msg.executable);
        let player = &tree.nodes[msg.children[0] as usize];
        assert!(!player.executable);
        let message = &tree.nodes[player.children[0] as usize];
        assert!(message.executable);
    }

    #[test]
    fn an_empty_registry_leaves_the_tree_alone() {
        let mut tree = backend_tree();
        CommandRegistry::default().inject(&mut tree, staff);
        assert_eq!(tree, backend_tree());
    }

    #[test]
    fn server_arguments_are_suggested_from_the_proxy() {
        let servers = vec![
            "lobby-1".to_owned(),
            "lobby-2".to_owned(),
            "game".to_owned(),
        ];
        let suggestions = registry()
            .suggest("/server lo", &servers, &[], staff)
            .unwrap();
        assert_eq!(suggestions.start, 8);
        assert_eq!(suggestions.length, 2);
        assert_eq!(suggestions.matches, ["lobby-1", "lobby-2"]);

        let suggestions = registry()
            .suggest("/server ", &servers, &[], staff)
            .unwrap();
        assert_eq!(suggestions.start, 8);
        assert_eq!(suggestions.matches.len(), 3);

        assert!(
            registry()
                .suggest("/server lo", &servers, &[], everyone)
                .is_none()
        );
        assert!(
            registry()
                .suggest("/gamemode c", &servers, &[], staff)
                .is_none()
        );
    }

    #[test]
    fn plugins_lose_their_commands_when_they_leave() {
        let mut registry = registry();
        registry.remove_owner(1);
        assert!(registry.parse("hub", staff).is_none());
        assert!(registry.parse("msg a b", staff).is_some());
        registry.unregister(2, "msg");
        assert!(registry.is_empty());
    }
}
