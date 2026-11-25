use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Server {
    pub name: String,
    pub address: String,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RegistryError {
    #[error("`try` must name at least one server")]
    EmptyTry,
    #[error("`try` names `{0}`, which is not a known server")]
    UnknownTry(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerRegistry {
    servers: BTreeMap<String, String>,
    try_order: Vec<String>,
    configured_try: Vec<String>,
}

impl ServerRegistry {
    pub fn new(
        servers: BTreeMap<String, String>,
        try_order: Vec<String>,
    ) -> Result<Self, RegistryError> {
        if try_order.is_empty() {
            return Err(RegistryError::EmptyTry);
        }
        if let Some(unknown) = try_order.iter().find(|name| !servers.contains_key(*name)) {
            return Err(RegistryError::UnknownTry(unknown.clone()));
        }
        Ok(Self {
            servers,
            configured_try: try_order.clone(),
            try_order,
        })
    }

    pub fn register(&mut self, name: &str, address: &str) -> Option<String> {
        self.servers.insert(name.to_owned(), address.to_owned())
    }

    pub fn register_fallback(&mut self, name: &str, address: &str) -> Option<String> {
        if !self.try_order.iter().any(|existing| existing == name) {
            self.try_order.push(name.to_owned());
        }
        self.register(name, address)
    }

    pub fn unregister(&mut self, name: &str) -> Option<String> {
        if !self
            .configured_try
            .iter()
            .any(|configured| configured == name)
        {
            self.try_order.retain(|existing| existing != name);
        }
        self.servers.remove(name)
    }

    pub fn get(&self, name: &str) -> Option<Server> {
        self.servers.get(name).map(|address| Server {
            name: name.to_owned(),
            address: address.clone(),
        })
    }

    pub fn servers(&self) -> impl Iterator<Item = Server> + '_ {
        self.servers.iter().map(|(name, address)| Server {
            name: name.clone(),
            address: address.clone(),
        })
    }

    pub fn join_candidates(&self) -> Vec<Server> {
        self.try_order
            .iter()
            .filter_map(|name| self.get(name))
            .collect()
    }

    pub fn fallbacks_from(&self, current: &str) -> Vec<Server> {
        self.join_candidates()
            .into_iter()
            .filter(|server| server.name != current)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registry() -> ServerRegistry {
        ServerRegistry::new(
            BTreeMap::from([
                ("lobby-a".to_owned(), "10.0.0.1:25565".to_owned()),
                ("lobby-b".to_owned(), "10.0.0.2:25565".to_owned()),
                ("game".to_owned(), "10.0.0.3:25565".to_owned()),
            ]),
            vec!["lobby-a".to_owned(), "lobby-b".to_owned()],
        )
        .unwrap()
    }

    fn names(servers: &[Server]) -> Vec<&str> {
        servers.iter().map(|server| server.name.as_str()).collect()
    }

    #[test]
    fn players_join_the_try_servers_in_order() {
        assert_eq!(names(&registry().join_candidates()), ["lobby-a", "lobby-b"]);
    }

    #[test]
    fn an_unregistered_try_server_is_skipped() {
        let mut registry = registry();
        registry.unregister("lobby-a");
        assert_eq!(names(&registry.join_candidates()), ["lobby-b"]);
        registry.register("lobby-a", "10.0.0.9:25565");
        assert_eq!(names(&registry.join_candidates()), ["lobby-a", "lobby-b"]);
        assert_eq!(registry.get("lobby-a").unwrap().address, "10.0.0.9:25565");
    }

    #[test]
    fn fallbacks_never_include_the_current_server() {
        assert_eq!(names(&registry().fallbacks_from("lobby-a")), ["lobby-b"]);
        assert_eq!(
            names(&registry().fallbacks_from("game")),
            ["lobby-a", "lobby-b"]
        );
    }

    #[test]
    fn fallback_servers_join_the_try_order_until_they_leave() {
        let mut registry = registry();
        registry.register_fallback("lobby-c", "10.0.0.4:25565");
        assert_eq!(
            names(&registry.join_candidates()),
            ["lobby-a", "lobby-b", "lobby-c"]
        );
        registry.unregister("lobby-c");
        assert_eq!(names(&registry.join_candidates()), ["lobby-a", "lobby-b"]);
        registry.unregister("lobby-a");
        registry.register("lobby-a", "10.0.0.1:25565");
        assert_eq!(names(&registry.join_candidates()), ["lobby-a", "lobby-b"]);
    }

    #[test]
    fn try_must_name_known_servers() {
        let servers = BTreeMap::from([("lobby".to_owned(), "a:1".to_owned())]);
        assert_eq!(
            ServerRegistry::new(servers.clone(), Vec::new()),
            Err(RegistryError::EmptyTry)
        );
        assert_eq!(
            ServerRegistry::new(servers, vec!["hub".to_owned()]),
            Err(RegistryError::UnknownTry("hub".to_owned()))
        );
    }
}
