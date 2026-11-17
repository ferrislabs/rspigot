#[derive(Debug, Clone, Copy)]
pub struct PlayerPermissionsHandle<'a> {
    nodes: &'a [String],
}

impl<'a> PlayerPermissionsHandle<'a> {
    pub(crate) const fn new(nodes: &'a [String]) -> Self {
        Self { nodes }
    }

    pub fn has(&self, node: &str) -> bool {
        let mut granted = false;
        for entry in self.nodes {
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
