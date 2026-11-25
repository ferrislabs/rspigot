use std::collections::HashMap;

use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Presence {
    pub name: String,
    pub server: Option<String>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RosterError {
    #[error("{0} is already connected to this proxy")]
    AlreadyConnected(String),
}

#[derive(Debug, Default)]
pub struct Roster {
    players: HashMap<Uuid, Presence>,
}

impl Roster {
    pub fn login(&mut self, id: Uuid, name: &str) -> Result<(), RosterError> {
        if let Some(existing) = self.players.get(&id) {
            return Err(RosterError::AlreadyConnected(existing.name.clone()));
        }
        self.players.insert(
            id,
            Presence {
                name: name.to_owned(),
                server: None,
            },
        );
        Ok(())
    }

    pub fn connected(&mut self, id: Uuid, server: &str) {
        if let Some(presence) = self.players.get_mut(&id) {
            presence.server = Some(server.to_owned());
        }
    }

    pub fn logout(&mut self, id: Uuid) -> Option<Presence> {
        self.players.remove(&id)
    }

    pub fn get(&self, id: Uuid) -> Option<&Presence> {
        self.players.get(&id)
    }

    pub fn online(&self) -> usize {
        self.players.len()
    }

    pub fn on_server(&self, server: &str) -> usize {
        self.players
            .values()
            .filter(|presence| presence.server.as_deref() == Some(server))
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_player_is_counted_on_the_server_it_reached() {
        let mut roster = Roster::default();
        let steve = Uuid::from_u128(1);
        roster.login(steve, "Steve").unwrap();
        assert_eq!(roster.online(), 1);
        assert_eq!(roster.on_server("lobby"), 0);
        roster.connected(steve, "lobby");
        assert_eq!(roster.on_server("lobby"), 1);
        roster.connected(steve, "game");
        assert_eq!(roster.on_server("lobby"), 0);
        assert_eq!(roster.on_server("game"), 1);
    }

    #[test]
    fn the_same_account_cannot_join_twice() {
        let mut roster = Roster::default();
        let steve = Uuid::from_u128(1);
        roster.login(steve, "Steve").unwrap();
        assert_eq!(
            roster.login(steve, "Steve"),
            Err(RosterError::AlreadyConnected("Steve".to_owned()))
        );
    }

    #[test]
    fn logging_out_frees_the_account() {
        let mut roster = Roster::default();
        let steve = Uuid::from_u128(1);
        roster.login(steve, "Steve").unwrap();
        roster.connected(steve, "lobby");
        let presence = roster.logout(steve).unwrap();
        assert_eq!(presence.server.as_deref(), Some("lobby"));
        assert_eq!(roster.online(), 0);
        assert!(roster.login(steve, "Steve").is_ok());
    }
}
