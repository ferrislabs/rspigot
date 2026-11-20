use rspigot_domain::player::PlayerId;

#[derive(Debug, Clone, PartialEq, Eq)]
struct NodeGrant {
    connection: u64,
    player: PlayerId,
    node: String,
    value: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct GroupGrant {
    connection: u64,
    player: PlayerId,
    group: String,
}

#[derive(Debug, Default)]
pub(crate) struct Attachments {
    nodes: Vec<NodeGrant>,
    groups: Vec<GroupGrant>,
}

impl Attachments {
    pub(crate) fn set(&mut self, connection: u64, player: PlayerId, node: &str, value: bool) {
        self.unset(connection, player, node);
        self.nodes.push(NodeGrant {
            connection,
            player,
            node: node.to_owned(),
            value,
        });
    }

    pub(crate) fn unset(&mut self, connection: u64, player: PlayerId, node: &str) {
        self.nodes.retain(|grant| {
            !(grant.connection == connection && grant.player == player && grant.node == node)
        });
    }

    pub(crate) fn add_group(&mut self, connection: u64, player: PlayerId, group: &str) {
        self.remove_group(connection, player, group);
        self.groups.push(GroupGrant {
            connection,
            player,
            group: group.to_owned(),
        });
    }

    pub(crate) fn remove_group(&mut self, connection: u64, player: PlayerId, group: &str) {
        self.groups.retain(|grant| {
            !(grant.connection == connection && grant.player == player && grant.group == group)
        });
    }

    pub(crate) fn forget_player(&mut self, player: PlayerId) {
        self.nodes.retain(|grant| grant.player != player);
        self.groups.retain(|grant| grant.player != player);
    }

    pub(crate) fn forget_plugin(&mut self, connection: u64) -> bool {
        let before = self.nodes.len() + self.groups.len();
        self.nodes.retain(|grant| grant.connection != connection);
        self.groups.retain(|grant| grant.connection != connection);
        before != self.nodes.len() + self.groups.len()
    }

    pub(crate) fn groups_of(&self, player: PlayerId) -> Vec<&str> {
        self.groups
            .iter()
            .filter(|grant| grant.player == player)
            .map(|grant| grant.group.as_str())
            .collect()
    }

    pub(crate) fn nodes_of(&self, player: PlayerId) -> impl Iterator<Item = String> + '_ {
        self.nodes
            .iter()
            .filter(move |grant| grant.player == player)
            .map(|grant| {
                if grant.value {
                    grant.node.clone()
                } else {
                    format!("-{}", grant.node)
                }
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn steve() -> PlayerId {
        PlayerId::offline("Steve")
    }

    #[test]
    fn a_second_set_replaces_the_first_of_the_same_plugin() {
        let mut grants = Attachments::default();
        grants.set(1, steve(), "kit.use", false);
        grants.set(1, steve(), "kit.use", true);
        assert_eq!(grants.nodes_of(steve()).collect::<Vec<_>>(), ["kit.use"]);
    }

    #[test]
    fn two_plugins_keep_their_own_entries() {
        let mut grants = Attachments::default();
        grants.set(1, steve(), "kit.use", true);
        grants.set(2, steve(), "kit.use", false);
        grants.unset(2, steve(), "kit.use");
        assert_eq!(grants.nodes_of(steve()).collect::<Vec<_>>(), ["kit.use"]);
    }

    #[test]
    fn a_plugin_leaving_drops_its_nodes_and_groups_only() {
        let mut grants = Attachments::default();
        grants.set(1, steve(), "a", true);
        grants.add_group(1, steve(), "vip");
        grants.set(2, steve(), "b", true);
        assert!(grants.forget_plugin(1));
        assert!(!grants.forget_plugin(1));
        assert_eq!(grants.nodes_of(steve()).collect::<Vec<_>>(), ["b"]);
        assert!(grants.groups_of(steve()).is_empty());
    }

    #[test]
    fn a_negation_is_written_with_a_dash() {
        let mut grants = Attachments::default();
        grants.set(1, steve(), "kit.ban", false);
        assert_eq!(grants.nodes_of(steve()).collect::<Vec<_>>(), ["-kit.ban"]);
    }
}
