use std::net::IpAddr;
use std::sync::Arc;

use crossbeam_channel::Sender;
use rspigot_domain::player::{ClientSettings, GameProfile, MainHand as ClientHand, PlayerId};
use rspigot_domain::text::{Component, Content, NamedColor};

use crate::commands::{CommandNodeKind, CommandTree};
use crate::console::console_text;
use crate::events::{ClientEvent, PreLoginVerdict};
use crate::plugins::decisions::{Continuation, Lane};
use crate::plugins::events::{
    AsyncPlayerPreLogin, AsyncPlayerPreLoginPatch, AsyncPlayerPreLoginResult, MainHand,
    PlayerChangedMainHand, PlayerCommandSend, PlayerCommandSendPatch, PlayerJoin, PlayerJoinPatch,
    PlayerKick, PlayerKickPatch, PlayerLeave, PlayerLeavePatch, PlayerLocaleChange, PlayerLogin,
    PlayerLoginPatch, PlayerLoginResult,
};
use crate::plugins::resolve;
use crate::plugins::steps::{KickStep, LeaveStep, SettingsStep};
use crate::plugins::{Access, EventKind, EventPayload, Phase, PlayerSnapshot};
use crate::ports::ClientLink;
use crate::server::{Audience, Server};

pub(crate) struct Refusal {
    pub(crate) result: PlayerLoginResult,
    pub(crate) reason: Component,
}

pub(crate) struct Arrival<L> {
    pub(crate) profile: GameProfile,
    pub(crate) settings: ClientSettings,
    pub(crate) link: L,
    pub(crate) address: IpAddr,
    pub(crate) session: u64,
    pub(crate) refusal: Option<Refusal>,
}

pub(crate) fn legacy_of(component: &Component) -> String {
    if translated(component) {
        console_text(component)
    } else {
        component.to_legacy()
    }
}

fn translated(component: &Component) -> bool {
    matches!(component.content, Content::Translatable { .. })
        || component.extra.iter().any(translated)
}

const fn hand_of(hand: ClientHand) -> MainHand {
    match hand {
        ClientHand::Left => MainHand::Left,
        ClientHand::Right => MainHand::Right,
    }
}

fn root_names(tree: &CommandTree) -> Vec<String> {
    tree.nodes[tree.root]
        .children
        .iter()
        .filter_map(|child| match &tree.nodes[*child].kind {
            CommandNodeKind::Literal(name) => Some(name.clone()),
            _ => None,
        })
        .collect()
}

fn pruned(tree: &CommandTree, kept: &[String]) -> CommandTree {
    let mut pruned = tree.clone();
    let root = pruned.root;
    let names: Vec<Option<String>> = pruned.nodes[root]
        .children
        .iter()
        .map(|child| match &tree.nodes[*child].kind {
            CommandNodeKind::Literal(name) => Some(name.clone()),
            _ => None,
        })
        .collect();
    let mut names = names.into_iter();
    pruned.nodes[root].children.retain(|_| {
        names
            .next()
            .flatten()
            .is_none_or(|name| kept.contains(&name))
    });
    pruned
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn notify_now(&mut self, payload: EventPayload) {
        if self.plugins.listens(payload.kind(), Access::Observe) {
            let event = self.next_event_id();
            self.notify_plugins(event, &payload, false, Phase::Settled);
        }
    }

    pub(crate) fn pre_login(
        &mut self,
        name: String,
        id: PlayerId,
        address: IpAddr,
        transferred: bool,
        reply: Sender<PreLoginVerdict>,
    ) {
        let attempt = self.next_login_attempt();
        let payload = EventPayload::AsyncPlayerPreLogin(AsyncPlayerPreLogin {
            address,
            kick_message: String::new(),
            login_result: AsyncPlayerPreLoginResult::Allowed,
            name,
            transferred,
            unique_id: id.0,
        });
        self.fire(
            Lane::Login(attempt),
            Continuation::PreLogin(reply),
            move |_| Some(payload),
        );
    }

    pub(super) fn answer_pre_login(
        &mut self,
        reply: &Sender<PreLoginVerdict>,
        patch: AsyncPlayerPreLoginPatch,
    ) {
        let verdict = match patch.login_result {
            None | Some(AsyncPlayerPreLoginResult::Allowed) => PreLoginVerdict::Allowed,
            Some(_) => PreLoginVerdict::Refused(patch.kick_message.unwrap_or_default()),
        };
        let _ = reply.send(verdict);
    }

    pub(crate) fn ask_login(&mut self, arrival: Arrival<L>) {
        let id = arrival.profile.id;
        let key = self.park_arrival(arrival);
        self.fire_deferred(id, Continuation::Login(key));
    }

    pub(crate) fn login_at_head(&mut self, key: u64) -> Option<u64> {
        let arrival = self.arrival(key)?;
        let (profile, address) = (arrival.profile.clone(), arrival.address);
        let refusal = self.refusal(&profile, address);
        self.arrival_mut(key)?.refusal = refusal;
        Some(key)
    }

    pub(crate) fn login_payload(&self, key: u64) -> Option<EventPayload> {
        let arrival = self.arrival(key)?;
        let (result, kick_message) = arrival
            .refusal
            .as_ref()
            .map_or((PlayerLoginResult::Allowed, String::new()), |refusal| {
                (refusal.result, legacy_of(&refusal.reason))
            });
        Some(EventPayload::PlayerLogin(PlayerLogin {
            address: arrival.address,
            hostname: String::new(),
            kick_message,
            player: PlayerSnapshot {
                permissions: Vec::new(),
                id: arrival.profile.id,
                name: arrival.profile.name.clone(),
                entity_id: 0,
                world: self.world.dimension.key.to_string(),
                position: self.world.spawn,
                rotation: rspigot_domain::Rotation::new(self.world.level.spawn_angle, 0.0),
                game_mode: self.config.default_game_mode,
            },
            real_address: arrival.address,
            result,
        }))
    }

    pub(super) fn finish_login(&mut self, key: u64, patch: PlayerLoginPatch) {
        let Some(arrival) = self.take_arrival(key) else {
            return;
        };
        if !arrival.link.is_connected() {
            return;
        }
        let (default, default_text) = arrival
            .refusal
            .as_ref()
            .map_or((PlayerLoginResult::Allowed, String::new()), |refusal| {
                (refusal.result, legacy_of(&refusal.reason))
            });
        let result = patch.result.unwrap_or(default);
        if result != PlayerLoginResult::Allowed {
            let message = patch.kick_message.unwrap_or(default_text);
            let reason = match &arrival.refusal {
                Some(refusal) if legacy_of(&refusal.reason) == message => refusal.reason.clone(),
                _ => Component::text(message),
            };
            tracing::info!(
                "Disconnecting {} ({}): {}",
                arrival.profile.name,
                arrival.address,
                console_text(&reason)
            );
            arrival.link.disconnect(reason);
            return;
        }
        if self.players.contains_key(&arrival.profile.id) {
            arrival.link.disconnect(Component::translatable(
                "multiplayer.disconnect.duplicate_login",
                Vec::new(),
            ));
            return;
        }
        self.arrive(arrival);
    }

    pub(crate) fn ask_join(&mut self, id: PlayerId) {
        self.fire(id, Continuation::Join(id), move |server| {
            server.snapshot_of(id).map(|player| {
                EventPayload::PlayerJoin(PlayerJoin {
                    join_message: Some(format!("§e{} joined the game", player.name)),
                    player,
                })
            })
        });
    }

    pub(super) fn announce_join(&mut self, id: PlayerId, patch: PlayerJoinPatch) {
        let Some(player) = self.players.get(&id) else {
            return;
        };
        if !player.link.is_connected() {
            return;
        }
        let name = player.profile.name.clone();
        let who = crate::management::PlayerRef {
            id: Some(player.profile.id.0),
            name: Some(name.clone()),
        };
        self.notify_management(crate::management::ManagementNotice::PlayerJoined(who));
        let default = format!("§e{name} joined the game");
        let message = patch.join_message.unwrap_or_else(|| Some(default.clone()));
        let joined = || {
            Component::translatable("multiplayer.player.joined", vec![Component::text(name)])
                .color(NamedColor::Yellow)
        };
        self.announce(message.as_deref(), Some(&default), joined);
    }

    fn announce(
        &mut self,
        message: Option<&str>,
        vanilla_text: Option<&str>,
        vanilla: impl FnOnce() -> Component,
    ) {
        let Some(message) = message.filter(|message| !message.is_empty()) else {
            return;
        };
        let lines = if Some(message) == vanilla_text {
            vec![vanilla()]
        } else {
            message.split('\n').map(resolve::text).collect()
        };
        for line in lines {
            tracing::info!("{}", console_text(&line));
            self.push(Audience::All, ClientEvent::system(line));
        }
    }

    pub(crate) fn leave_event(
        &mut self,
        player: PlayerSnapshot,
        kicked: Option<String>,
        announce: bool,
    ) {
        let id = player.id;
        let name = player.name.clone();
        let vanilla = kicked
            .as_ref()
            .is_none_or(|kicked| *kicked == format!("§e{name} left the game."));
        let default = kicked.unwrap_or_else(|| format!("§e{name} left the game"));
        let payload = EventPayload::PlayerLeave(PlayerLeave {
            player,
            quit_message: Some(default.clone()),
        });
        if !announce {
            self.notify_now(payload);
            return;
        }
        self.fire(
            id,
            Continuation::Leave(LeaveStep {
                name,
                default,
                vanilla,
            }),
            move |_| Some(payload),
        );
    }

    pub(super) fn announce_leave(&mut self, step: LeaveStep, patch: PlayerLeavePatch) {
        let message = patch
            .quit_message
            .unwrap_or_else(|| Some(step.default.clone()));
        let name = step.name;
        self.notify_management(crate::management::ManagementNotice::PlayerLeft(
            crate::management::PlayerRef::named(&name),
        ));
        let left = || {
            Component::translatable("multiplayer.player.left", vec![Component::text(name)])
                .color(NamedColor::Yellow)
        };
        let vanilla = step.vanilla.then_some(step.default.as_str());
        self.announce(message.as_deref(), vanilla, left);
    }

    pub(crate) fn disconnect_player(&mut self, id: PlayerId, reason: Component) {
        let Some(player) = self.players.get(&id) else {
            return;
        };
        let kicking = self.decisions.in_flight(Lane::Player(id), |then| {
            matches!(then, Continuation::Kick(_))
        });
        if !player.link.is_connected() || kicking {
            return;
        }
        let leave = format!("§e{} left the game.", player.profile.name);
        let text = legacy_of(&reason);
        let payload = {
            let leave = leave.clone();
            let text = text.clone();
            move |server: &Self| {
                server.snapshot_of(id).map(|player| {
                    EventPayload::PlayerKick(PlayerKick {
                        leave_message: leave,
                        player,
                        reason: text,
                    })
                })
            }
        };
        self.fire(
            id,
            Continuation::Kick(KickStep {
                player: id,
                reason,
                text,
                leave,
            }),
            payload,
        );
    }

    pub(super) fn finish_kick(&mut self, step: KickStep, patch: PlayerKickPatch) {
        let Some(player) = self.players.get_mut(&step.player) else {
            return;
        };
        if !player.link.is_connected() {
            return;
        }
        player.kick_leave_message = Some(patch.leave_message.unwrap_or(step.leave));
        let reason = match patch.reason {
            Some(reason) if reason != step.text => resolve::text(&reason),
            _ => step.reason,
        };
        tracing::info!("Kicked {}: {}", player.profile.name, console_text(&reason));
        player.link.disconnect(reason);
        self.remove_player(step.player, true);
    }

    pub(crate) fn change_settings(&mut self, id: PlayerId, settings: ClientSettings) {
        let Some(player) = self.players.get(&id) else {
            return;
        };
        let hand =
            (player.settings.main_hand != settings.main_hand).then_some(player.settings.main_hand);
        let locale = player.settings.locale != settings.locale;
        if !locale && hand.is_none() {
            self.apply_settings(id, settings);
            return;
        }
        let (on_hand, on_locale) = if locale {
            (None, Some(settings))
        } else {
            (Some(settings), None)
        };
        self.fire_settings(
            id,
            hand,
            on_hand,
            on_locale.map(|settings| (settings.locale.clone(), Some(settings))),
        );
    }

    pub(crate) fn announce_settings(&mut self, id: PlayerId, settings: &ClientSettings) {
        let defaults = ClientSettings::default();
        let hand = (settings.main_hand != defaults.main_hand).then_some(defaults.main_hand);
        let locale = (settings.locale != defaults.locale).then(|| (settings.locale.clone(), None));
        self.fire_settings(id, hand, None, locale);
    }

    fn fire_settings(
        &mut self,
        id: PlayerId,
        old_hand: Option<ClientHand>,
        on_hand: Option<ClientSettings>,
        locale: Option<(String, Option<ClientSettings>)>,
    ) {
        if let Some(old) = old_hand {
            let step = SettingsStep {
                kind: EventKind::PlayerChangedMainHand,
                player: id,
                settings: on_hand,
            };
            self.fire(id, Continuation::Settings(step), move |server| {
                server.snapshot_of(id).map(|player| {
                    EventPayload::PlayerChangedMainHand(PlayerChangedMainHand {
                        main_hand: hand_of(old),
                        player,
                    })
                })
            });
        }
        if let Some((locale, settings)) = locale {
            let step = SettingsStep {
                kind: EventKind::PlayerLocaleChange,
                player: id,
                settings,
            };
            self.fire(id, Continuation::Settings(step), move |server| {
                server.snapshot_of(id).map(|player| {
                    EventPayload::PlayerLocaleChange(PlayerLocaleChange { locale, player })
                })
            });
        }
    }

    pub(crate) fn apply_settings(&mut self, id: PlayerId, settings: ClientSettings) {
        let max = self.config.view_distance;
        if let Some(player) = self.players.get_mut(&id) {
            let view = max.min(settings.view_distance.max(2));
            player.settings = settings;
            player.streamer.set_view_distance(
                view,
                &mut self.world.chunks,
                &self.workers,
                &player.link,
            );
        }
    }

    pub(crate) fn send_commands(&mut self, id: PlayerId) {
        let offered = if self.listened(EventKind::PlayerCommandSend) {
            self.players
                .get(&id)
                .map(|player| root_names(&self.player_tree(player)))
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let described = offered.clone();
        self.fire(id, Continuation::CommandSend(id, offered), move |server| {
            Some(EventPayload::PlayerCommandSend(PlayerCommandSend {
                commands: described,
                player: server.snapshot_of(id)?,
            }))
        });
    }

    pub(super) fn finish_command_send(
        &mut self,
        id: PlayerId,
        offered: Vec<String>,
        patch: PlayerCommandSendPatch,
    ) {
        let Some(player) = self.players.get(&id) else {
            return;
        };
        let tree = self.player_tree(player);
        let tree = if patch.commands.is_empty() {
            tree
        } else {
            let settled = patch.commands.apply(offered.clone());
            let kept: Vec<String> = root_names(&tree)
                .into_iter()
                .filter(|name| settled.contains(name) || !offered.contains(name))
                .collect();
            Arc::new(pruned(&tree, &kept))
        };
        player.link.send(ClientEvent::Commands(tree));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::build_tree;

    #[test]
    fn a_pruned_tree_keeps_only_the_named_roots() {
        let tree = build_tree(0);
        let names = root_names(&tree);
        assert!(names.contains(&"help".to_owned()));
        let kept = pruned(&tree, &["help".to_owned()]);
        assert_eq!(root_names(&kept), ["help"]);
        assert_eq!(kept.nodes.len(), tree.nodes.len());
    }

    #[test]
    fn a_legacy_reason_keeps_its_colours_unless_it_must_be_translated() {
        assert_eq!(legacy_of(&Component::from_section_codes("§cBye")), "§cBye");
        assert_eq!(
            legacy_of(&Component::translatable(
                "multiplayer.disconnect.duplicate_login",
                Vec::new()
            )),
            "You logged in from another location"
        );
    }
}
