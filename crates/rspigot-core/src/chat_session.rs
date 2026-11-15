use rspigot_domain::player::PlayerId;
use rspigot_domain::text::Component;

use crate::events::ChatSessionKey;
use crate::ports::ClientLink;
use crate::server::Server;

impl<L: ClientLink> Server<L> {
    pub(crate) fn handle_chat_session(&mut self, id: PlayerId, key: ChatSessionKey) {
        let Some(player) = self.players.get_mut(&id) else {
            return;
        };
        let current = player.chat_session.as_ref();
        let same_key = current.is_some_and(|known| {
            known.expires_at_ms == key.expires_at_ms
                && known.public_key == key.public_key
                && known.key_signature == key.key_signature
        });
        if same_key {
            return;
        }
        if current.is_some_and(|known| key.expires_at_ms < known.expires_at_ms) {
            self.disconnect_player(
                id,
                Component::translatable("multiplayer.disconnect.expired_public_key", Vec::new()),
            );
            return;
        }
        player.chat_session = Some(key);
    }
}
