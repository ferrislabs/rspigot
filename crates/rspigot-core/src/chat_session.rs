use std::time::UNIX_EPOCH;

use rspigot_domain::chat_chain::{
    ChainError, Decoder, MessageBody, MessageSignature, PublicKey, session_key_is_signed,
};
use rspigot_domain::player::PlayerId;
use rspigot_domain::text::{Component, NamedColor};

use crate::commands::signable::signable_arguments;
use crate::events::{ChatMessage, ChatSessionKey, ClientEvent, SignedChat, SignedCommand};
use crate::ports::ClientLink;
use crate::server::{Audience, Server};

const CHAT_VALIDATION_FAILED: &str = "multiplayer.disconnect.chat_validation_failed";
const INVALID_COMMAND_SIGNATURE: &str = "chat.disabled.invalid_command_signature";

fn translatable(key: &str) -> Component {
    Component::translatable(key, Vec::new())
}

fn illegal_chat(message: &str) -> bool {
    message.chars().any(|c| c == '§' || c.is_control())
}

impl<L: ClientLink> Server<L> {
    fn now_ms(&self) -> i64 {
        self.now().duration_since(UNIX_EPOCH).map_or(0, |elapsed| {
            i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX)
        })
    }

    pub(crate) fn handle_chat_session(&mut self, id: PlayerId, key: ChatSessionKey) {
        let services: Vec<PublicKey> = self
            .config
            .chat
            .services_keys
            .iter()
            .filter_map(|der| PublicKey::from_der(der))
            .collect();
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
                translatable("multiplayer.disconnect.expired_public_key"),
            );
            return;
        }
        if services.is_empty() {
            tracing::warn!(
                "Ignoring chat session from {} due to missing Services public key",
                player.profile.name
            );
            return;
        }
        let signed = session_key_is_signed(
            &services,
            player.profile.id.0,
            key.expires_at_ms,
            &key.public_key,
            &key.key_signature,
        );
        let Some(public) = PublicKey::from_der(&key.public_key).filter(|_| signed) else {
            tracing::error!("Failed to validate profile key for {}", player.profile.name);
            self.disconnect_player(
                id,
                translatable("multiplayer.disconnect.invalid_public_key_signature"),
            );
            return;
        };
        player.chat.decoder =
            Decoder::keyed(player.profile.id.0, key.session, public, key.expires_at_ms);
        player.chat_index = 0;
        player.chat_session = Some(key.clone());
        self.push(Audience::All, ClientEvent::ChatSessions(vec![(id, key)]));
    }

    pub(crate) fn handle_chat_ack(&mut self, id: PlayerId, offset: i32) {
        let Some(player) = self.players.get_mut(&id) else {
            return;
        };
        if let Err(error) = player.chat.last_seen.apply_offset(offset) {
            tracing::error!(
                "Failed to validate message acknowledgement offset from {}: {error}",
                player.profile.name
            );
            self.disconnect_player(id, translatable(CHAT_VALIDATION_FAILED));
        }
    }

    fn apply_last_seen(
        &mut self,
        id: PlayerId,
        update: rspigot_domain::chat_chain::LastSeenUpdate,
    ) -> Option<Vec<MessageSignature>> {
        let player = self.players.get_mut(&id)?;
        match player.chat.last_seen.apply_update(update) {
            Ok(seen) => Some(seen),
            Err(error) => {
                tracing::error!(
                    "Failed to validate message acknowledgements from {}: {error}",
                    player.profile.name
                );
                self.disconnect_player(id, translatable(CHAT_VALIDATION_FAILED));
                None
            }
        }
    }

    fn reject_chat(&mut self, id: PlayerId, reason: Component) {
        if let Some(player) = self.players.get(&id) {
            tracing::warn!(
                "Failed to update secure chat state for {}: '{}'",
                player.profile.name,
                reason.plain_text()
            );
        }
        self.push(
            Audience::One(id),
            ClientEvent::SystemChat {
                message: reason.color(NamedColor::Red),
                overlay: false,
            },
        );
    }

    pub(crate) fn handle_chat_message(&mut self, id: PlayerId, message: ChatMessage) {
        if !self.players.contains_key(&id) || !self.spam_check(id, false) {
            return;
        }
        let Some(seen) = self.apply_last_seen(id, message.last_seen) else {
            return;
        };
        if message.content.chars().count() > 256 || illegal_chat(&message.content) {
            self.disconnect_player(
                id,
                translatable("multiplayer.disconnect.illegal_characters"),
            );
            return;
        }
        let now = self.now_ms();
        let enforce = self.config.chat.enforce_secure_profile;
        let display = self
            .players
            .get(&id)
            .and_then(|player| self.holder_display_name(&player.profile.name));
        let Some(player) = self.players.get_mut(&id) else {
            return;
        };
        let keyed = player.chat_session.is_some();
        let body = MessageBody {
            content: &message.content,
            timestamp_ms: message.timestamp_ms,
            salt: message.salt,
            last_seen: &seen,
        };
        if let Err(error) =
            player
                .chat
                .decoder
                .unpack(enforce, now, message.signature.as_ref(), &body)
        {
            self.reject_chat(id, translatable(error.translation_key()));
            return;
        }
        let signed = match message.signature {
            Some(signature) if keyed => {
                let index = player.chat_index;
                player.chat_index = index.saturating_add(1);
                Some(Box::new(SignedChat {
                    sender: id,
                    sender_name: display
                        .unwrap_or_else(|| Component::text(player.profile.name.clone())),
                    index,
                    signature,
                    content: message.content.clone(),
                    timestamp_ms: message.timestamp_ms,
                    salt: message.salt,
                    last_seen: seen,
                    unsigned_content: None,
                }))
            }
            _ => None,
        };
        self.broadcast_chat(id, &message.content, signed);
    }

    pub(crate) fn handle_unsigned_command(&mut self, id: PlayerId, command: &str) {
        if !self.spam_check(id, true) {
            return;
        }
        if self.config.chat.enforce_secure_profile
            && let Some(player) = self.players.get(&id)
            && !signable_arguments(&self.command_tree(player.permission_level()), command)
                .is_empty()
        {
            tracing::error!(
                "Received unsigned command packet from {}, but the command requires signable arguments: {command}",
                player.profile.name
            );
            self.reject_chat(id, translatable(INVALID_COMMAND_SIGNATURE));
            return;
        }
        self.run_player_command(id, command);
    }

    pub(crate) fn handle_signed_command(&mut self, id: PlayerId, signed: SignedCommand) {
        if !self.players.contains_key(&id) || !self.spam_check(id, true) {
            return;
        }
        let Some(seen) = self.apply_last_seen(id, signed.last_seen) else {
            return;
        };
        if illegal_chat(&signed.command) {
            self.disconnect_player(
                id,
                translatable("multiplayer.disconnect.illegal_characters"),
            );
            return;
        }
        let now = self.now_ms();
        let enforce = self.config.chat.enforce_secure_profile;
        let Some(level) = self
            .players
            .get(&id)
            .map(|player| player.permission_level())
        else {
            return;
        };
        let arguments = signable_arguments(&self.command_tree(level), &signed.command);
        let Some(player) = self.players.get_mut(&id) else {
            return;
        };
        let verdict = verify_arguments(
            &mut player.chat.decoder,
            &mut player.chat_index,
            enforce,
            now,
            &signed,
            &arguments,
            &seen,
        );
        match verdict {
            Ok(()) => self.run_player_command(id, &signed.command),
            Err(Mismatch::Chain(error)) => {
                self.reject_chat(id, translatable(error.translation_key()));
            }
            Err(Mismatch::Command) => {
                tracing::error!(
                    "Signed command mismatch between server and client ('{}')",
                    signed.command
                );
                self.reject_chat(id, translatable(INVALID_COMMAND_SIGNATURE));
            }
        }
    }
}

enum Mismatch {
    Chain(ChainError),
    Command,
}

fn verify_arguments(
    decoder: &mut Decoder,
    chain_index: &mut i32,
    enforce: bool,
    now: i64,
    signed: &SignedCommand,
    arguments: &[crate::commands::signable::SignableArgument],
    seen: &[MessageSignature],
) -> Result<(), Mismatch> {
    if signed.arguments.is_empty() {
        for argument in arguments {
            let body = MessageBody {
                content: &argument.value,
                timestamp_ms: now,
                salt: 0,
                last_seen: &[],
            };
            decoder
                .unpack(enforce, now, None, &body)
                .map_err(Mismatch::Chain)?;
        }
        return Ok(());
    }
    let mut verified = Vec::with_capacity(signed.arguments.len());
    for (name, signature) in &signed.arguments {
        let Some(argument) = arguments.iter().find(|argument| &argument.name == name) else {
            decoder.set_chain_broken();
            return Err(Mismatch::Command);
        };
        let body = MessageBody {
            content: &argument.value,
            timestamp_ms: signed.timestamp_ms,
            salt: signed.salt,
            last_seen: seen,
        };
        decoder
            .unpack(enforce, now, Some(signature), &body)
            .map_err(Mismatch::Chain)?;
        *chain_index = chain_index.saturating_add(1);
        verified.push(name);
    }
    if arguments
        .iter()
        .any(|argument| !verified.contains(&&argument.name))
    {
        return Err(Mismatch::Command);
    }
    Ok(())
}
