use rspigot_domain::player::PlayerId;
use rustc_hash::{FxHashMap, FxHashSet};

pub const DEFAULT_GROUP: &str = "default";
pub const OPERATOR_LEVEL: u8 = 4;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Group {
    permissions: Vec<String>,
    inherits: Vec<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum NodeDefault {
    True,
    #[default]
    False,
    Op,
    NotOp,
}

impl NodeDefault {
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "true" => Some(Self::True),
            "false" => Some(Self::False),
            "op" | "isop" | "operator" | "isoperator" | "admin" | "isadmin" => Some(Self::Op),
            "!op" | "notop" | "not op" | "not_op" | "!operator" | "notoperator"
            | "not operator" | "!admin" | "notadmin" | "not admin" => Some(Self::NotOp),
            _ => None,
        }
    }

    fn applies(self, op_level: u8) -> bool {
        match self {
            Self::True => true,
            Self::False => false,
            Self::Op => op_level > 0,
            Self::NotOp => op_level == 0,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Node {
    default: NodeDefault,
    description: String,
    children: Vec<(String, bool)>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Permissions {
    groups: FxHashMap<String, Group>,
    players: FxHashMap<String, Vec<String>>,
    nodes: FxHashMap<String, Node>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupView {
    pub name: String,
    pub permissions: Vec<String>,
    pub inherits: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Rights {
    nodes: Vec<String>,
}

impl Permissions {
    pub fn with_group(mut self, name: &str, permissions: &[&str], inherits: &[&str]) -> Self {
        self.groups.insert(
            name.to_owned(),
            Group {
                permissions: permissions.iter().map(|node| (*node).to_owned()).collect(),
                inherits: inherits.iter().map(|group| (*group).to_owned()).collect(),
            },
        );
        self
    }

    pub fn with_node(
        mut self,
        name: &str,
        default: NodeDefault,
        description: &str,
        children: &[(&str, bool)],
    ) -> Self {
        self.nodes.insert(
            name.to_ascii_lowercase(),
            Node {
                default,
                description: description.to_owned(),
                children: children
                    .iter()
                    .map(|(child, value)| (child.to_ascii_lowercase(), *value))
                    .collect(),
            },
        );
        self
    }

    pub fn has_node(&self, name: &str) -> bool {
        self.nodes.contains_key(&name.to_ascii_lowercase())
    }

    pub fn node_default(&self, name: &str) -> Option<NodeDefault> {
        self.nodes
            .get(&name.to_ascii_lowercase())
            .map(|node| node.default)
    }

    pub fn node_description(&self, name: &str) -> Option<&str> {
        self.nodes
            .get(&name.to_ascii_lowercase())
            .map(|node| node.description.as_str())
    }

    pub fn with_player(mut self, key: &str, groups: &[&str]) -> Self {
        self.players.insert(
            key.to_ascii_lowercase(),
            groups.iter().map(|group| (*group).to_owned()).collect(),
        );
        self
    }

    pub fn has_group(&self, name: &str) -> bool {
        self.groups.contains_key(name)
    }

    pub fn group_views(&self) -> Vec<GroupView> {
        let mut views: Vec<GroupView> = self
            .groups
            .iter()
            .map(|(name, group)| GroupView {
                name: name.clone(),
                permissions: group.permissions.clone(),
                inherits: group.inherits.clone(),
            })
            .collect();
        views.sort_by(|a, b| a.name.cmp(&b.name));
        views
    }

    pub fn rights(&self, id: PlayerId, name: &str, op_level: u8) -> Rights {
        self.rights_with(id, name, op_level, &[])
    }

    pub fn rights_with(
        &self,
        id: PlayerId,
        name: &str,
        op_level: u8,
        extra_groups: &[&str],
    ) -> Rights {
        if op_level >= OPERATOR_LEVEL {
            return Rights {
                nodes: vec!["*".to_owned()],
            };
        }
        let by_uuid = self.players.get(&id.0.hyphenated().to_string());
        let by_name = self.players.get(&name.to_ascii_lowercase());
        let mut seen = FxHashSet::default();
        let mut nodes = Vec::new();
        let mut defaults: Vec<&String> = self.nodes.keys().collect();
        defaults.sort();
        for name in defaults {
            if self.nodes[name].default.applies(op_level) {
                self.expand(name, true, &mut nodes, &mut FxHashSet::default());
            }
        }
        let roots = std::iter::once(DEFAULT_GROUP)
            .chain(by_uuid.into_iter().flatten().map(String::as_str))
            .chain(by_name.into_iter().flatten().map(String::as_str))
            .chain(extra_groups.iter().copied());
        for root in roots {
            self.collect(root, &mut seen, &mut nodes);
        }
        Rights { nodes }
    }

    fn expand(
        &self,
        name: &str,
        value: bool,
        nodes: &mut Vec<String>,
        visiting: &mut FxHashSet<String>,
    ) {
        nodes.push(if value {
            name.to_owned()
        } else {
            format!("-{name}")
        });
        let Some(node) = self.nodes.get(&name.to_ascii_lowercase()) else {
            return;
        };
        if !visiting.insert(name.to_ascii_lowercase()) {
            return;
        }
        for (child, child_value) in &node.children {
            self.expand(child, value == *child_value, nodes, visiting);
        }
    }

    fn collect(&self, name: &str, seen: &mut FxHashSet<String>, nodes: &mut Vec<String>) {
        if !seen.insert(name.to_owned()) {
            return;
        }
        let Some(group) = self.groups.get(name) else {
            return;
        };
        for permission in &group.permissions {
            match permission.strip_prefix('-') {
                Some(denied) => self.expand(denied, false, nodes, &mut FxHashSet::default()),
                None => self.expand(permission, true, nodes, &mut FxHashSet::default()),
            }
        }
        for parent in &group.inherits {
            self.collect(parent, seen, nodes);
        }
    }
}

impl Rights {
    pub fn from_nodes(nodes: Vec<String>) -> Self {
        Self { nodes }
    }

    pub fn nodes(&self) -> &[String] {
        &self.nodes
    }

    pub fn extended(mut self, overlay: impl IntoIterator<Item = String>) -> Self {
        self.nodes.extend(overlay);
        self
    }

    pub fn into_nodes(self) -> Vec<String> {
        self.nodes
    }

    pub fn decision(&self, node: &str) -> Option<bool> {
        let mut granted = false;
        for entry in &self.nodes {
            match entry.strip_prefix('-') {
                Some(denied) if covers(denied, node) => return Some(false),
                Some(_) => {}
                None => granted |= covers(entry, node),
            }
        }
        granted.then_some(true)
    }

    pub fn has(&self, node: &str) -> bool {
        let mut granted = false;
        for entry in &self.nodes {
            match entry.strip_prefix('-') {
                Some(denied) if covers(denied, node) => return false,
                Some(_) => {}
                None => granted |= covers(entry, node),
            }
        }
        granted
    }
}

fn covers(pattern: &str, node: &str) -> bool {
    if pattern.eq_ignore_ascii_case(node) {
        return true;
    }
    if pattern == "*" || pattern == node {
        return true;
    }
    pattern.strip_suffix(".*").is_some_and(|prefix| {
        node.strip_prefix(prefix)
            .is_some_and(|rest| rest.starts_with('.'))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alice() -> PlayerId {
        PlayerId::offline("Alice")
    }

    #[test]
    fn a_default_true_node_is_granted_to_everyone() {
        let permissions = Permissions::default().with_node("kit.use", NodeDefault::True, "", &[]);
        assert!(permissions.rights(alice(), "Alice", 0).has("kit.use"));
    }

    #[test]
    fn op_and_not_op_defaults_follow_the_op_level() {
        let permissions = Permissions::default()
            .with_node("only.ops", NodeDefault::Op, "", &[])
            .with_node("only.players", NodeDefault::NotOp, "", &[])
            .with_node("nobody", NodeDefault::False, "", &[]);
        let player = permissions.rights(alice(), "Alice", 0);
        assert!(!player.has("only.ops") && player.has("only.players") && !player.has("nobody"));
        let moderator = permissions.rights(alice(), "Alice", 1);
        assert!(moderator.has("only.ops") && !moderator.has("only.players"));
    }

    #[test]
    fn children_follow_their_parent_and_false_children_are_denied() {
        let permissions = Permissions::default()
            .with_node(
                "pack.all",
                NodeDefault::False,
                "",
                &[("pack.a", true), ("pack.b", false)],
            )
            .with_group("default", &["pack.all"], &[]);
        let rights = permissions.rights(alice(), "Alice", 0);
        assert!(rights.has("pack.all") && rights.has("pack.a") && !rights.has("pack.b"));
        assert_eq!(rights.decision("pack.b"), Some(false));
        assert_eq!(rights.decision("pack.c"), None);
    }

    #[test]
    fn an_explicit_denial_beats_a_default() {
        let permissions = Permissions::default()
            .with_node("kit.use", NodeDefault::True, "", &[])
            .with_group("default", &["-kit.use"], &[]);
        assert!(!permissions.rights(alice(), "Alice", 0).has("kit.use"));
    }

    #[test]
    fn child_cycles_terminate() {
        let permissions = Permissions::default()
            .with_node("a", NodeDefault::True, "", &[("b", true)])
            .with_node("b", NodeDefault::False, "", &[("a", true)]);
        assert!(permissions.rights(alice(), "Alice", 0).has("b"));
    }

    #[test]
    fn default_parsing_accepts_the_bukkit_spellings() {
        assert_eq!(NodeDefault::parse("true"), Some(NodeDefault::True));
        assert_eq!(NodeDefault::parse("op"), Some(NodeDefault::Op));
        assert_eq!(NodeDefault::parse("not op"), Some(NodeDefault::NotOp));
        assert_eq!(NodeDefault::parse("!op"), Some(NodeDefault::NotOp));
        assert_eq!(NodeDefault::parse("false"), Some(NodeDefault::False));
        assert_eq!(NodeDefault::parse("banana"), None);
    }
}
