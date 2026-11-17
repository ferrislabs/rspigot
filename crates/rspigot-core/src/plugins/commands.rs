use std::sync::Arc;

use rspigot_domain::player::PlayerId;
use rspigot_domain::text::{Component, NamedColor};

use crate::commands::{self, CommandNodeKind, CommandSender, CommandTree};
use crate::events::ClientEvent;
use crate::player::Player;
use crate::plugins::decisions::{Continuation, Lane};
use crate::plugins::events::{ServerCommand, ServerCommandPatch, TabComplete, TabCompletePatch};
use crate::plugins::steps::SuggestStep;
use crate::plugins::{
    ArgumentKind, ArgumentValue, CommandSpec, EventKind, EventPayload, SenderView, ToPlugin,
};
use crate::ports::ClientLink;
use crate::server::{Audience, Server};

const OPERATOR_TREE: u8 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PluginTyping {
    Elsewhere,
    Silent,
    Dynamic,
}

enum Unfit {
    Incomplete,
    NoPlayer,
}

fn label_of(word: &str) -> String {
    word.trim_start_matches('/').to_ascii_lowercase()
}

fn item_key(word: &str) -> String {
    let word = word.to_ascii_lowercase();
    if word.contains(':') {
        word
    } else {
        format!("minecraft:{word}")
    }
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn console_line(&mut self, line: String) {
        if !self.listened(EventKind::ServerCommand) {
            self.finish_server_command(line, ServerCommandPatch::default());
            return;
        }
        let command = line.clone();
        self.fire(
            Lane::Console,
            Continuation::ServerCommand(line),
            move |_| {
                Some(EventPayload::ServerCommand(ServerCommand {
                    command,
                    sender: SenderView::Console,
                }))
            },
        );
    }

    fn declared_commands(&self) -> Vec<(u64, &CommandSpec)> {
        let vanilla = self.command_tree(OPERATOR_TREE);
        let mut taken: Vec<String> = vanilla_roots(&vanilla);
        let mut kept = Vec::new();
        for (connection, command) in self.plugins.commands() {
            let name = label_of(&command.name);
            if !taken.contains(&name) {
                taken.push(name);
                kept.push((connection, command));
            }
        }
        kept
    }

    fn plugin_command(&self, label: &str) -> Option<(u64, &CommandSpec)> {
        self.declared_commands()
            .into_iter()
            .find(|(_, command)| label_of(&command.name) == label)
    }

    fn may_run(&self, player: &Player<L>, command: &CommandSpec) -> bool {
        command.permission.as_deref().is_none_or(|node| {
            self.permissions
                .rights(player.id, player.name(), player.permission_level())
                .has(node)
        })
    }

    pub(crate) fn player_tree(&self, player: &Player<L>) -> Arc<CommandTree> {
        let base = self.command_tree(player.permission_level());
        let granted: Vec<&CommandSpec> = self
            .declared_commands()
            .into_iter()
            .map(|(_, command)| command)
            .filter(|command| self.may_run(player, command))
            .collect();
        if granted.is_empty() {
            base
        } else {
            Arc::new(base.with_plugin_commands(&granted))
        }
    }

    pub(crate) fn announce_commands(&mut self, connection: u64) {
        let vanilla = vanilla_roots(&self.command_tree(OPERATOR_TREE));
        let mut warnings = Vec::new();
        let mut seen: Vec<String> = Vec::new();
        for (owner, command) in self.plugins.commands() {
            if owner != connection {
                continue;
            }
            let name = label_of(&command.name);
            if vanilla.contains(&name) {
                warnings.push(format!(
                    "the command /{name} already exists in the server, the plugin's one is ignored"
                ));
            } else if seen.contains(&name)
                || self
                    .plugins
                    .commands()
                    .any(|(other, taken)| other < connection && label_of(&taken.name) == name)
            {
                warnings.push(format!(
                    "the command /{name} is already declared by another plugin, this one is ignored"
                ));
            }
            seen.push(name);
        }
        for warning in warnings {
            tracing::warn!("{warning}");
            self.plugins.send(connection, ToPlugin::Warning(warning));
        }
        self.resend_command_trees();
    }

    pub(crate) fn resend_command_trees(&mut self) {
        let online: Vec<PlayerId> = self.players.keys().copied().collect();
        for id in online {
            self.send_commands(id);
        }
    }

    pub(crate) fn plugin_typing(&self, id: PlayerId, input: &str) -> PluginTyping {
        let mut words = input.split(' ');
        let (Some(label), Some(player)) = (words.next(), self.players.get(&id)) else {
            return PluginTyping::Elsewhere;
        };
        let words = words.count();
        if words == 0 {
            return PluginTyping::Elsewhere;
        }
        let Some((_, command)) = self.plugin_command(&label_of(label)) else {
            return PluginTyping::Elsewhere;
        };
        let suggested = command
            .arguments
            .get(words - 1)
            .is_some_and(|argument| argument.suggested);
        if suggested && self.may_run(player, command) {
            PluginTyping::Dynamic
        } else {
            PluginTyping::Silent
        }
    }

    pub(crate) fn run_plugin_command(&mut self, id: PlayerId, line: &str) -> bool {
        let line = line.trim();
        let mut words = line.split_whitespace();
        let Some(label) = words.next().map(label_of) else {
            return false;
        };
        let Some((connection, command)) = self.plugin_command(&label) else {
            return false;
        };
        let command = command.clone();
        let Some(player) = self.players.get(&id) else {
            return true;
        };
        let sender = CommandSender::Player(id);
        if !self.may_run(player, &command) {
            commands::refuse(self, sender, line);
            return true;
        }
        let words: Vec<&str> = words.collect();
        match self.read_arguments(&command, &words) {
            Ok(arguments) => {
                if let Some(snapshot) = self.snapshot_of(id) {
                    self.plugins.send(
                        connection,
                        ToPlugin::Invocation {
                            command: command.id,
                            sender: snapshot,
                            arguments,
                        },
                    );
                }
            }
            Err(Unfit::Incomplete) => {
                commands::refuse(self, sender, line);
            }
            Err(Unfit::NoPlayer) => self.push(
                Audience::One(id),
                ClientEvent::system(
                    Component::translatable("argument.entity.notfound.player", Vec::new())
                        .color(NamedColor::Red),
                ),
            ),
        }
        true
    }

    fn read_arguments(
        &self,
        command: &CommandSpec,
        words: &[&str],
    ) -> Result<Vec<ArgumentValue>, Unfit> {
        let required = command
            .arguments
            .iter()
            .filter(|argument| !argument.optional)
            .count();
        if words.len() < required || words.len() > command.arguments.len() {
            return Err(Unfit::Incomplete);
        }
        command
            .arguments
            .iter()
            .zip(words)
            .map(|(argument, word)| match argument.kind {
                ArgumentKind::Player => self
                    .players
                    .values()
                    .find(|player| player.name().eq_ignore_ascii_case(word))
                    .and_then(|player| self.snapshot_of(player.id))
                    .map(ArgumentValue::Player)
                    .ok_or(Unfit::NoPlayer),
                ArgumentKind::Item => Ok(ArgumentValue::Item(item_key(word))),
                ArgumentKind::Integer => word
                    .parse()
                    .map(ArgumentValue::Integer)
                    .map_err(|_| Unfit::Incomplete),
                ArgumentKind::Text => Ok(ArgumentValue::Text((*word).to_owned())),
                ArgumentKind::Boolean => word
                    .parse()
                    .map(ArgumentValue::Flag)
                    .map_err(|_| Unfit::Incomplete),
            })
            .collect()
    }

    pub(super) fn finish_server_command(&mut self, line: String, patch: ServerCommandPatch) {
        if !self.running {
            return;
        }
        let line = patch.command.unwrap_or(line);
        commands::execute(self, CommandSender::Console, &line);
    }

    pub(crate) fn ask_tab_complete(&mut self, buffer: &str, step: SuggestStep) {
        if !self.listened(EventKind::TabComplete) {
            self.finish_tab_complete(step, false, TabCompletePatch::default());
            return;
        }
        let player = step.player;
        let buffer = buffer.to_owned();
        let completions = step.matches.clone();
        self.fire(
            player,
            Continuation::TabComplete(Box::new(step)),
            move |server| {
                let sender = SenderView::Player(server.snapshot_of(player)?);
                Some(EventPayload::TabComplete(TabComplete {
                    buffer,
                    completions,
                    sender,
                }))
            },
        );
    }

    pub(super) fn finish_tab_complete(
        &mut self,
        step: SuggestStep,
        cancelled: bool,
        patch: TabCompletePatch,
    ) {
        if cancelled {
            return;
        }
        let mut matches = patch.completions.apply(step.matches);
        if let Some(prefix) = &step.filter {
            matches.retain(|candidate| candidate.to_ascii_lowercase().starts_with(prefix));
            matches.sort();
            matches.dedup();
        }
        if matches.is_empty() {
            return;
        }
        self.push(
            Audience::One(step.player),
            ClientEvent::CommandSuggestions {
                id: step.request,
                start: step.start,
                length: step.length,
                matches,
            },
        );
    }
}

fn vanilla_roots(tree: &CommandTree) -> Vec<String> {
    tree.nodes[tree.root]
        .children
        .iter()
        .filter_map(|child| match &tree.nodes[*child].kind {
            CommandNodeKind::Literal(name) => Some(name.to_ascii_lowercase()),
            _ => None,
        })
        .collect()
}
