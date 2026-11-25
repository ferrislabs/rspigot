use std::collections::BTreeSet;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Permissions {
    granted: BTreeSet<String>,
    denied: BTreeSet<String>,
}

impl Permissions {
    pub fn new(
        granted: impl IntoIterator<Item = String>,
        denied: impl IntoIterator<Item = String>,
    ) -> Self {
        Self {
            granted: granted.into_iter().collect(),
            denied: denied.into_iter().collect(),
        }
    }

    pub fn allows(&self, node: &str) -> bool {
        if matches(&self.denied, node) {
            return false;
        }
        matches(&self.granted, node)
    }
}

fn matches(set: &BTreeSet<String>, node: &str) -> bool {
    if set.contains(node) || set.contains("*") {
        return true;
    }
    let mut prefix = node;
    while let Some((parent, _)) = prefix.rsplit_once('.') {
        if set.contains(&format!("{parent}.*")) {
            return true;
        }
        prefix = parent;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn permissions(granted: &[&str], denied: &[&str]) -> Permissions {
        Permissions::new(
            granted.iter().map(|node| (*node).to_owned()),
            denied.iter().map(|node| (*node).to_owned()),
        )
    }

    #[test]
    fn nothing_is_allowed_by_default() {
        assert!(!Permissions::default().allows("proxy.command.server"));
    }

    #[test]
    fn exact_nodes_and_wildcards_grant() {
        assert!(permissions(&["proxy.command.server"], &[]).allows("proxy.command.server"));
        assert!(permissions(&["proxy.command.*"], &[]).allows("proxy.command.server"));
        assert!(permissions(&["proxy.*"], &[]).allows("proxy.command.server"));
        assert!(permissions(&["*"], &[]).allows("anything"));
        assert!(!permissions(&["proxy.command.*"], &[]).allows("proxy.admin"));
    }

    #[test]
    fn a_denied_node_wins_over_a_grant() {
        let permissions = permissions(&["*"], &["proxy.command.send"]);
        assert!(!permissions.allows("proxy.command.send"));
        assert!(permissions.allows("proxy.command.server"));
    }
}
