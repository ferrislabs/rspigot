use std::sync::Arc;

use rspigot_domain::player::PlayerId;
use rspigot_domain::text::{Component, NamedColor};

use crate::commands::{self, CommandNodeKind, CommandSender, CommandTree};
use crate::events::ClientEvent;
use crate::player::Player;
use crate::plugins::command_args::{Unfit, argument_at};
use crate::plugins::decisions::{Continuation, Lane};
use crate::plugins::events::{ServerCommand, ServerCommandPatch, TabComplete, TabCompletePatch};
use crate::plugins::steps::SuggestStep;
use crate::plugins::{
    ArgumentKind, CommandSpec, EventKind, EventPayload, PluginRoot, SenderView, ToPlugin,
};
use crate::ports::ClientLink;
use crate::server::{Audience, Server};

const OPERATOR_TREE: u8 = 4;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PluginTyping {
    Elsewhere,
    Silent,
    Dynamic,
    Choices(Vec<String>),
}

fn label_of(word: &str) -> String {
    word.trim_start_matches('/').to_ascii_lowercase()
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

    pub(crate) fn plugin_roots(&self) -> Vec<PluginRoot<'_>> {
        let vanilla = vanilla_roots(&self.command_tree(OPERATOR_TREE));
        let mut roots: Vec<PluginRoot<'_>> = Vec::new();
        for (connection, spec) in self.plugins.commands() {
            let name = label_of(&spec.name);
            if vanilla.contains(&name) {
                continue;
            }
            match roots.iter_mut().find(|root| root.name == name) {
                Some(root) if root.connection == connection => root.specs.push(spec),
                Some(_) => {}
                None => roots.push(PluginRoot {
                    name,
                    connection,
                    aliases: Vec::new(),
                    specs: vec![spec],
                }),
            }
        }
        for (connection, spec) in self.plugins.commands() {
            let name = label_of(&spec.name);
            for alias in &spec.aliases {
                let alias = label_of(alias);
                let free = !vanilla.contains(&alias)
                    && !roots
                        .iter()
                        .any(|root| root.name == alias || root.aliases.contains(&alias));
                if free
                    && let Some(root) = roots
                        .iter_mut()
                        .find(|root| root.name == name && root.connection == connection)
                {
                    root.aliases.push(alias);
                }
            }
        }
        roots
    }

    fn plugin_root(&self, label: &str) -> Option<PluginRoot<'_>> {
        self.plugin_roots()
            .into_iter()
            .find(|root| root.name == label || root.aliases.iter().any(|alias| alias == label))
    }

    fn permitted<'a>(&self, player: &Player<L>, root: &PluginRoot<'a>) -> Vec<&'a CommandSpec> {
        let rights = self.rights_for(player);
        root.specs
            .iter()
            .copied()
            .filter(|spec| {
                spec.permission
                    .as_deref()
                    .is_none_or(|node| rights.has(node))
            })
            .collect()
    }

    pub(crate) fn player_tree(&self, player: &Player<L>) -> Arc<CommandTree> {
        let base = self.command_tree(player.permission_level());
        let granted: Vec<PluginRoot<'_>> = self
            .plugin_roots()
            .into_iter()
            .filter_map(|root| {
                let specs = self.permitted(player, &root);
                (!specs.is_empty()).then_some(PluginRoot { specs, ..root })
            })
            .collect();
        if granted.is_empty() {
            base
        } else {
            Arc::new(base.with_plugin_commands(&granted))
        }
    }

    fn command_conflicts(&self, connection: u64, only: Option<&str>) -> Vec<String> {
        let vanilla = vanilla_roots(&self.command_tree(OPERATOR_TREE));
        let roots = self.plugin_roots();
        let mut warnings: Vec<String> = Vec::new();
        let mut seen: Vec<String> = Vec::new();
        for (owner, command) in self.plugins.commands() {
            if owner != connection {
                continue;
            }
            let name = label_of(&command.name);
            if only.is_some_and(|only| only != name) || seen.contains(&name) {
                continue;
            }
            seen.push(name.clone());
            if vanilla.contains(&name) {
                warnings.push(format!(
                    "the command /{name} already exists in the server, the plugin's one is ignored"
                ));
            } else if roots
                .iter()
                .any(|root| root.name == name && root.connection != connection)
            {
                warnings.push(format!(
                    "the command /{name} is already declared by another plugin, this one is ignored"
                ));
            }
            for alias in self
                .plugins
                .commands()
                .filter(|(other, spec)| *other == connection && label_of(&spec.name) == name)
                .flat_map(|(_, spec)| spec.aliases.iter())
            {
                let alias = label_of(alias);
                let kept = roots
                    .iter()
                    .any(|root| root.name == name && root.aliases.contains(&alias));
                let own = roots
                    .iter()
                    .any(|root| root.name == name && root.connection == connection);
                if own && !kept && alias != name {
                    warnings.push(format!(
                        "the alias /{alias} of /{name} is already taken, the plugin's one is ignored"
                    ));
                }
            }
        }
        warnings
    }

    pub(crate) fn announce_commands(&mut self, connection: u64) {
        for warning in self.command_conflicts(connection, None) {
            tracing::warn!("{warning}");
            self.plugins.send(connection, ToPlugin::Warning(warning));
        }
        self.resend_command_trees();
    }

    pub(super) fn register_command(&mut self, connection: u64, spec: CommandSpec) {
        let name = label_of(&spec.name);
        if let Err(error) = self.plugins.add_command(connection, spec) {
            self.warn_plugin(connection, "server.command.register", error);
            return;
        }
        for warning in self.command_conflicts(connection, Some(&name)) {
            tracing::warn!("{warning}");
            self.plugins.send(connection, ToPlugin::Warning(warning));
        }
        self.resend_command_trees();
    }

    pub(super) fn unregister_command(&mut self, connection: u64, name: &str) {
        if self.plugins.remove_command(connection, name) {
            self.resend_command_trees();
        } else {
            self.warn_plugin(
                connection,
                "server.command.unregister",
                format!("the plugin has no command {name}"),
            );
        }
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
        let mut typed: Vec<&str> = words.collect();
        if typed.pop().is_none() {
            return PluginTyping::Elsewhere;
        }
        let Some(root) = self.plugin_root(&label_of(label)) else {
            return PluginTyping::Elsewhere;
        };
        let mut dynamic = false;
        let mut choices: Vec<String> = Vec::new();
        for spec in self.permitted(player, &root) {
            let Some(argument) = argument_at(spec, &typed) else {
                continue;
            };
            if argument.kind == ArgumentKind::Choice {
                choices.extend(argument.choices.iter().cloned());
            } else if argument.suggested {
                dynamic = true;
            }
        }
        if !choices.is_empty() {
            choices.sort();
            choices.dedup();
            PluginTyping::Choices(choices)
        } else if dynamic {
            PluginTyping::Dynamic
        } else {
            PluginTyping::Silent
        }
    }

    pub(crate) fn run_plugin_command(&mut self, id: PlayerId, line: &str) -> bool {
        let line = line.trim();
        let (label, arguments) = line
            .split_once(char::is_whitespace)
            .map_or((line, ""), |(label, rest)| (label, rest.trim_start()));
        let label = label_of(label);
        let Some(root) = self.plugin_root(&label) else {
            return false;
        };
        let connection = root.connection;
        let Some(player) = self.players.get(&id) else {
            return true;
        };
        let sender = CommandSender::Player(id);
        let permitted = self.permitted(player, &root);
        if permitted.is_empty() {
            commands::refuse(self, sender, line);
            return true;
        }
        let mut best: Option<(&CommandSpec, Vec<_>, usize)> = None;
        let mut failure = Unfit::Incomplete;
        for spec in permitted {
            match self.read_arguments(id, spec, arguments) {
                Ok((values, literals)) => {
                    if best.as_ref().is_none_or(|(_, _, most)| literals > *most) {
                        best = Some((spec, values, literals));
                    }
                }
                Err(Unfit::NoPlayer) => failure = Unfit::NoPlayer,
                Err(_) => {}
            }
        }
        match best {
            Some((spec, arguments, _)) => {
                let command = spec.id;
                if let Some(snapshot) = self.snapshot_of(id) {
                    self.plugins.send(
                        connection,
                        ToPlugin::Invocation {
                            command,
                            sender: snapshot,
                            arguments,
                        },
                    );
                }
            }
            None if failure == Unfit::NoPlayer => {
                self.push(
                    Audience::One(id),
                    ClientEvent::system(
                        Component::translatable("argument.entity.notfound.player", Vec::new())
                            .color(NamedColor::Red),
                    ),
                );
            }
            None => {
                commands::refuse(self, sender, line);
            }
        }
        true
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
