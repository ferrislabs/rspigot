use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct PlayerPermissionsHandle {
    nodes: Arc<[String]>,
}

impl PlayerPermissionsHandle {
    pub(crate) const fn new(nodes: Arc<[String]>) -> Self {
        Self { nodes }
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
