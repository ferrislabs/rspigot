use rspigot_domain::player::PlayerId;
use rspigot_domain::text::{Component, Content, NamedColor};

use crate::commands::CommandSender;
use crate::console::console_text;
use crate::events::ClientEvent;
use crate::ports::ClientLink;
use crate::server::{Audience, Server};

const GAMEMASTER: u8 = 2;

pub(crate) fn is_relay(message: &Component) -> bool {
    matches!(
        &message.content,
        Content::Translatable { key, .. } if key == "chat.type.admin"
    )
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn broadcast_to_admins(&mut self, sender: CommandSender, message: &Component) {
        let name = match sender {
            CommandSender::Console => "Server".to_owned(),
            CommandSender::Player(id) => match self.players.get(&id) {
                Some(player) => player.name().to_owned(),
                None => return,
            },
        };
        let line = Component::translatable(
            "chat.type.admin",
            vec![Component::text(name), message.clone()],
        )
        .color(NamedColor::Gray)
        .italic(true);
        if self.config.rules.send_command_feedback {
            let operators: Vec<PlayerId> = self
                .players
                .values()
                .filter(|player| player.permission_level() >= GAMEMASTER)
                .map(|player| player.id)
                .filter(|id| sender != CommandSender::Player(*id))
                .collect();
            for id in operators {
                self.push(Audience::One(id), ClientEvent::system(line.clone()));
            }
        }
        if sender != CommandSender::Console && self.config.rules.log_admin_commands {
            tracing::info!("{}", console_text(&line));
        }
    }
}
