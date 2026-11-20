use std::net::IpAddr;

use rspigot_domain::player::PlayerId;
use rspigot_domain::text::Component;

use crate::ports::ClientLink;
use crate::server::Server;

pub(crate) fn command_log_line(enabled: bool, name: &str, command: &str) -> Option<String> {
    enabled.then(|| format!("{name} issued server command: /{command}"))
}

pub(crate) fn login_origin(log_ips: bool, address: IpAddr) -> String {
    if log_ips {
        format!("/{address}")
    } else {
        "<ip address withheld>".to_owned()
    }
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn enforce_whitelist(&mut self) {
        if !self.config.enforce_whitelist || !self.admin.whitelist_enabled {
            return;
        }
        let refused: Vec<PlayerId> = self
            .players
            .values()
            .filter(|player| !self.admin.may_join(&player.profile))
            .map(|player| player.id)
            .collect();
        for id in refused {
            let reason = Component::text(self.config.whitelist_message.clone());
            self.disconnect_player(id, reason);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{command_log_line, login_origin};
    use crate::commands::{CommandContext, CommandOutcome, CommandSender, execute};
    use crate::end_tests::arrive;

    #[test]
    fn enabling_the_whitelist_kicks_the_players_it_does_not_list() {
        let (mut server, id, _) = arrive();
        server.config.enforce_whitelist = true;
        assert!(server.players.contains_key(&id));
        assert!(server.set_whitelist_enabled(true));
        server.enforce_whitelist();
        assert!(!server.players.contains_key(&id));
    }

    #[test]
    fn without_the_setting_a_listless_whitelist_keeps_everyone() {
        let (mut server, id, _) = arrive();
        assert!(server.set_whitelist_enabled(true));
        server.enforce_whitelist();
        assert!(server.players.contains_key(&id));
    }

    #[test]
    fn removing_a_listed_player_kicks_them_and_listed_players_stay() {
        let (mut server, id, _) = arrive();
        server.config.enforce_whitelist = true;
        assert!(server.whitelist_add("Alice"));
        assert!(server.set_whitelist_enabled(true));
        assert!(server.players.contains_key(&id));
        assert!(server.whitelist_remove("Alice"));
        assert!(!server.players.contains_key(&id));
    }

    #[test]
    fn the_whitelist_command_enforces_on_reload_and_removal() {
        let (mut server, id, _) = arrive();
        server.config.enforce_whitelist = true;
        let outcome = execute(&mut server, CommandSender::Console, "whitelist on");
        assert_eq!(outcome, CommandOutcome::Success);
        assert!(!server.players.contains_key(&id));
    }

    #[test]
    fn the_log_ips_setting_hides_the_address_in_the_login_line() {
        let address: std::net::IpAddr = [10, 0, 0, 7].into();
        assert_eq!(login_origin(true, address), "/10.0.0.7");
        assert_eq!(login_origin(false, address), "<ip address withheld>");
    }

    #[test]
    fn the_server_full_message_replaces_the_vanilla_refusal() {
        let (mut server, _, _) = arrive();
        server.config.max_players = 1;
        let stranger = rspigot_domain::player::GameProfile {
            id: rspigot_domain::player::PlayerId::offline("Bob"),
            name: "Bob".to_owned(),
            properties: Vec::new(),
        };
        let address: std::net::IpAddr = [10, 0, 0, 8].into();
        let vanilla = server.refusal(&stranger, address).expect("full");
        assert_eq!(
            vanilla.reason.plain_text(),
            "multiplayer.disconnect.server_full"
        );
        server.config.messages.server_full = "No room!".to_owned();
        let custom = server.refusal(&stranger, address).expect("full");
        assert_eq!(custom.reason.plain_text(), "No room!");
    }

    #[test]
    fn the_command_log_setting_silences_the_issued_command_line() {
        assert_eq!(
            command_log_line(true, "Alice", "gamemode creative").as_deref(),
            Some("Alice issued server command: /gamemode creative")
        );
        assert_eq!(command_log_line(false, "Alice", "gamemode creative"), None);
    }
}
