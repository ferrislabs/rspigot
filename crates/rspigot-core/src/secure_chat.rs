use std::sync::Arc;

use rspigot_domain::chat_chain::MessageSignature;
use rspigot_domain::player::PlayerId;
use rspigot_domain::text::Component;

use crate::events::{ClientEvent, SignedChat};
use crate::ports::ClientLink;
use crate::server::{Audience, Server};

const HISTORY: usize = 256;
const MAX_PENDING: usize = 4096;

impl<L: ClientLink> Server<L> {
    pub(crate) fn relay_signed(&mut self, signed: Box<SignedChat>) {
        let sender = signed.sender;
        let signature = signed.signature;
        let shown = signed
            .unsigned_content
            .clone()
            .unwrap_or_else(|| signed.content.clone());
        let signed = Arc::new(*signed);
        self.chat_history.push_back((sender, signature));
        if self.chat_history.len() > HISTORY {
            self.chat_history.pop_front();
        }
        let recipients: Vec<PlayerId> = self.players.keys().copied().collect();
        for recipient in recipients {
            if self.visibility.hides(recipient, sender) {
                self.push(
                    Audience::One(recipient),
                    ClientEvent::PlayerChat {
                        sender,
                        sender_name: signed.sender_name.clone(),
                        message: shown.clone(),
                    },
                );
                continue;
            }
            let Some(player) = self.players.get_mut(&recipient) else {
                continue;
            };
            player.chat.last_seen.add_pending(signature);
            let pending = player.chat.last_seen.tracked_count();
            self.push(
                Audience::One(recipient),
                ClientEvent::SignedChat(signed.clone()),
            );
            if pending > MAX_PENDING {
                self.disconnect_player(
                    recipient,
                    Component::translatable(
                        "multiplayer.disconnect.too_many_pending_chats",
                        Vec::new(),
                    ),
                );
            }
        }
    }

    pub fn delete_chat(&mut self, sender: PlayerId, signature: MessageSignature) -> bool {
        if !self.chat_history.contains(&(sender, signature)) {
            return false;
        }
        self.chat_history
            .retain(|entry| *entry != (sender, signature));
        self.push(Audience::All, ClientEvent::DeleteChat { signature });
        true
    }
}
