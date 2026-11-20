use std::sync::Arc;

use rspigot_plugin_protocol::v1::{self, answer, from_plugin, player_order, question};

use crate::Error;
use crate::link::Link;

#[derive(Debug, Clone)]
pub struct PlayerPermissionsHandle {
    nodes: Arc<[String]>,
    target: Arc<str>,
    link: Link,
}

impl PlayerPermissionsHandle {
    pub(crate) fn new(nodes: Arc<[String]>, target: Arc<str>, link: Link) -> Self {
        Self {
            nodes,
            target,
            link,
        }
    }

    pub(crate) fn nodes(&self) -> &[String] {
        &self.nodes
    }

    pub fn has(&self, node: &str) -> bool {
        let mut granted = false;
        for entry in &*self.nodes {
            match entry.strip_prefix('-') {
                Some(denied) if covers(denied, node) => return false,
                Some(_) => {}
                None => granted |= covers(entry, node),
            }
        }
        granted
    }

    pub fn set(&self, node: impl Into<String>, value: bool) {
        self.order(player_order::Order::SetPermission(v1::PermissionSet {
            node: node.into(),
            value,
        }));
    }

    pub fn unset(&self, node: impl Into<String>) {
        self.order(player_order::Order::UnsetPermission(node.into()));
    }

    pub fn add_group(&self, name: impl Into<String>) {
        self.order(player_order::Order::AddGroup(name.into()));
    }

    pub fn remove_group(&self, name: impl Into<String>) {
        self.order(player_order::Order::RemoveGroup(name.into()));
    }

    fn order(&self, order: player_order::Order) {
        self.link.order(from_plugin::Body::Player(v1::PlayerOrder {
            player: self.target.to_string(),
            order: Some(order),
        }));
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionGroup {
    pub name: String,
    pub permissions: Vec<String>,
    pub inherits: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ServerPermissionsHandle {
    link: Link,
}

impl ServerPermissionsHandle {
    pub(crate) const fn new(link: Link) -> Self {
        Self { link }
    }

    pub async fn get_groups(&self) -> Result<Vec<PermissionGroup>, Error> {
        let groups = self
            .link
            .ask(
                question::Question::PermissionGroups(v1::Empty {}),
                "a list of permission groups",
                |body| match body {
                    answer::Body::PermissionGroups(groups) => Some(groups),
                    _ => None,
                },
            )
            .await?;
        Ok(groups
            .groups
            .into_iter()
            .map(|group| PermissionGroup {
                name: group.name,
                permissions: group.permissions,
                inherits: group.inherits,
            })
            .collect())
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
