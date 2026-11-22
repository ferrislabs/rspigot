use rspigot_domain::player::ClientSettings;
use rspigot_domain::text::{Component, NamedColor};

use crate::events::ClientEvent;
use crate::player::Player;
use crate::ports::ClientLink;
use crate::server::Server;

pub(crate) const DISABLED_BY_OPTIONS: &str = "chat.disabled.options";

pub(crate) fn delivers(settings: &ClientSettings, event: &ClientEvent) -> bool {
    match event {
        ClientEvent::PlayerChat { .. }
        | ClientEvent::SignedChat(_)
        | ClientEvent::ChatMessage { .. } => settings.chat_visibility.accepts_chat(),
        ClientEvent::SystemChat { overlay, .. } => {
            settings.chat_visibility.accepts_system(*overlay)
        }
        _ => true,
    }
}

pub(crate) fn send<L: ClientLink>(player: &Player<L>, event: ClientEvent) {
    if delivers(&player.settings, &event) {
        player.link.send(event);
    }
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn refuse_hidden_chat(&mut self, id: rspigot_domain::player::PlayerId) -> bool {
        let hidden = self
            .players
            .get(&id)
            .is_some_and(|player| !player.settings.chat_visibility.accepts_system(false));
        if hidden && let Some(player) = self.players.get(&id) {
            player.link.send(ClientEvent::SystemChat {
                message: Component::translatable(DISABLED_BY_OPTIONS, Vec::new())
                    .color(NamedColor::Red),
                overlay: false,
            });
        }
        hidden
    }
}
