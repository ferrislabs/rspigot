use rspigot_domain::player::PlayerId;
use rustc_hash::{FxHashMap, FxHashSet};

pub const DEFAULT_GROUP: &str = "default";
pub const OPERATOR_LEVEL: u8 = 4;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Group {
    permissions: Vec<String>,
    inherits: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Permissions {
    groups: FxHashMap<String, Group>,
    players: FxHashMap<String, Vec<String>>,
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
        let roots = std::iter::once(DEFAULT_GROUP)
            .chain(by_uuid.into_iter().flatten().map(String::as_str))
            .chain(by_name.into_iter().flatten().map(String::as_str))
            .chain(extra_groups.iter().copied());
        for root in roots {
            self.collect(root, &mut seen, &mut nodes);
        }
        Rights { nodes }
    }

    fn collect(&self, name: &str, seen: &mut FxHashSet<String>, nodes: &mut Vec<String>) {
        if !seen.insert(name.to_owned()) {
            return;
        }
        let Some(group) = self.groups.get(name) else {
            return;
        };
        nodes.extend(group.permissions.iter().cloned());
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
    if pattern == "*" || pattern == node {
        return true;
    }
    pattern.strip_suffix(".*").is_some_and(|prefix| {
        node.strip_prefix(prefix)
            .is_some_and(|rest| rest.starts_with('.'))
    })
}
