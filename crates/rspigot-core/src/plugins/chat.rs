use rspigot_domain::player::PlayerId;
use rspigot_domain::text::Component;

use crate::commands::{self, CommandSender};
use crate::console::console_text;
use crate::events::ClientEvent;
use crate::plugins::decisions::Continuation;
use crate::plugins::events::{
    AsyncPlayerChat, AsyncPlayerChatPatch, PlayerCommandPreprocess, PlayerCommandPreprocessPatch,
};
use crate::plugins::resolve;
use crate::plugins::steps::ChatStep;
use crate::plugins::{EventKind, EventPayload};
use crate::ports::ClientLink;
use crate::server::{Audience, Server};

pub(crate) const CHAT_FORMAT: &str = "<%1$s> %2$s";

pub(crate) fn java_format(format: &str, name: &str, message: &str) -> Option<String> {
    let arguments = [name, message];
    let mut out = String::with_capacity(format.len() + name.len() + message.len());
    let mut next = 0;
    let mut chars = format.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        let mut index = String::new();
        while let Some(digit) = chars.peek().filter(|d| d.is_ascii_digit()) {
            index.push(*digit);
            chars.next();
        }
        let explicit = if index.is_empty() {
            None
        } else {
            (chars.next() == Some('$')).then_some(())?;
            Some(index.parse::<usize>().ok()?.checked_sub(1)?)
        };
        match chars.next()? {
            's' | 'S' => {
                let at = explicit.unwrap_or_else(|| {
                    next += 1;
                    next - 1
                });
                out.push_str(arguments.get(at)?);
            }
            '%' if explicit.is_none() => out.push('%'),
            'n' if explicit.is_none() => out.push('\n'),
            _ => return None,
        }
    }
    Some(out)
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn broadcast_chat(&mut self, id: PlayerId, message: &str) {
        let Some(name) = self.players.get(&id).map(|p| p.profile.name.clone()) else {
            return;
        };
        if message.is_empty() {
            tracing::warn!("{name} tried to send an empty message");
            return;
        }
        let text = message.to_owned();
        let online = if self.listened(EventKind::AsyncPlayerChat) {
            self.online_snapshots()
        } else {
            Vec::new()
        };
        let offered = online.iter().map(|player| player.id).collect();
        self.fire(
            id,
            Continuation::Chat(
                ChatStep {
                    player: id,
                    message: message.to_owned(),
                },
                offered,
            ),
            move |server| {
                let player = server.snapshot_of(id)?;
                Some(EventPayload::AsyncPlayerChat(AsyncPlayerChat {
                    format: CHAT_FORMAT.to_owned(),
                    message: text,
                    player,
                    recipients: online,
                }))
            },
        );
    }

    pub(super) fn finish_chat(
        &mut self,
        step: ChatStep,
        offered: Vec<PlayerId>,
        patch: AsyncPlayerChatPatch,
    ) {
        let Some(name) = self
            .players
            .get(&step.player)
            .map(|p| p.profile.name.clone())
        else {
            return;
        };
        if patch == AsyncPlayerChatPatch::default() {
            tracing::info!("<{name}> {}", step.message);
            self.push(
                Audience::All,
                ClientEvent::PlayerChat {
                    sender: step.player,
                    sender_name: Component::text(name),
                    message: step.message,
                },
            );
            return;
        }
        let message = patch.message.unwrap_or(step.message);
        if message.is_empty() {
            return;
        }
        let format = patch.format.unwrap_or_else(|| CHAT_FORMAT.to_owned());
        let Some(text) = java_format(&format, &name, &message)
            .or_else(|| java_format(CHAT_FORMAT, &name, &message))
        else {
            return;
        };
        let line = resolve::text(&text);
        tracing::info!("{}", console_text(&line));
        let recipients: Vec<PlayerId> = if patch.recipients.is_empty() {
            self.players.keys().copied().collect()
        } else {
            patch
                .recipients
                .apply_with(
                    offered,
                    |_, player| Some(player.id),
                    |player| Some(player.id),
                )
                .into_iter()
                .filter(|id| self.players.contains_key(id))
                .collect()
        };
        for recipient in recipients {
            self.push(Audience::One(recipient), ClientEvent::system(line.clone()));
        }
    }

    pub(crate) fn run_player_command(&mut self, id: PlayerId, command: &str) {
        let Some(name) = self.players.get(&id).map(|p| p.profile.name.clone()) else {
            return;
        };
        if let Some(line) =
            crate::enforcement::command_log_line(self.config.log_commands, &name, command)
        {
            tracing::info!("{line}");
        }
        let message = format!("/{command}");
        let step = ChatStep {
            player: id,
            message: message.clone(),
        };
        self.fire(id, Continuation::Command(step), move |server| {
            server.snapshot_of(id).map(|player| {
                EventPayload::PlayerCommandPreprocess(PlayerCommandPreprocess { message, player })
            })
        });
    }

    pub(super) fn finish_command(&mut self, step: ChatStep, patch: PlayerCommandPreprocessPatch) {
        if !self.players.contains_key(&step.player) {
            return;
        }
        let message = patch.message.unwrap_or(step.message);
        let mut chars = message.chars();
        if chars.next().is_none() {
            return;
        }
        let line = chars.as_str();
        if self.run_plugin_command(step.player, line) {
            return;
        }
        commands::execute(self, CommandSender::Player(step.player), line);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_chat_format_follows_java_string_format() {
        assert_eq!(
            java_format(CHAT_FORMAT, "Alice", "hi").as_deref(),
            Some("<Alice> hi")
        );
        assert_eq!(
            java_format("%s says %s, 100%%", "Alice", "hi").as_deref(),
            Some("Alice says hi, 100%")
        );
        assert_eq!(
            java_format("%2$s from %1$s", "Alice", "hi").as_deref(),
            Some("hi from Alice")
        );
        assert_eq!(java_format("%d", "Alice", "hi"), None);
        assert_eq!(java_format("%3$s", "Alice", "hi"), None);
        assert_eq!(java_format("%s %s %s", "Alice", "hi"), None);
    }
}
