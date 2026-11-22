use rspigot_domain::player::{GameProfile, PlayerId};
use rspigot_domain::text::Component;
use rspigot_domain::{BlockPos, Identifier};

use crate::dialogs::Resolved;
use crate::events::{ChatKind, ClientEvent};
use crate::ports::ClientLink;
use crate::server::{Audience, Server};
use crate::server_surface::PerfReport;
use crate::waypoints::WaypointIcon;

pub trait Surface {
    fn show_dialog(&mut self, _player: PlayerId, _dialog: &Resolved) {}

    fn clear_dialog(&mut self, _player: PlayerId) {}

    fn send_chat(
        &mut self,
        _to: PlayerId,
        _kind: ChatKind,
        _sender: Component,
        _target: Option<Component>,
        _message: Component,
    ) {
    }

    fn waypoint_icon(&self, _player: PlayerId) -> WaypointIcon {
        WaypointIcon::default()
    }

    fn set_waypoint_icon(&mut self, _player: PlayerId, _icon: WaypointIcon) {}

    fn waypoint_transmitters(&self, _dimension: &Identifier) -> Vec<(PlayerId, BlockPos)> {
        Vec::new()
    }

    fn server_port(&self) -> Option<u16> {
        None
    }

    fn perf_start(&mut self, _requester: Option<PlayerId>) -> bool {
        false
    }

    fn perf_stop(&mut self) -> Option<PerfReport> {
        None
    }

    fn post_effects_of(&self, _player: PlayerId) -> Vec<Identifier> {
        Vec::new()
    }

    fn set_post_effects(&mut self, _player: PlayerId, _effects: Vec<Identifier>) {}

    fn resolve_profile(&self, _query: ProfileQuery<'_>) -> Option<GameProfile> {
        None
    }
}

#[derive(Debug, Clone, Copy)]
pub enum ProfileQuery<'a> {
    Name(&'a str),
    Id(uuid::Uuid),
}

impl<L: ClientLink> Surface for Server<L> {
    fn resolve_profile(&self, query: ProfileQuery<'_>) -> Option<GameProfile> {
        let online = self.players.values().find(|p| match query {
            ProfileQuery::Name(name) => p.profile.name.eq_ignore_ascii_case(name),
            ProfileQuery::Id(id) => p.profile.id.0 == id,
        });
        if let Some(player) = online {
            return Some(player.profile.clone());
        }
        let now = self.now();
        let cached = match query {
            ProfileQuery::Name(name) => self.config.user_cache.by_name(name, now),
            ProfileQuery::Id(id) => self.config.user_cache.by_uuid(id, now),
        }?;
        Some(GameProfile {
            id: PlayerId(cached.uuid),
            name: cached.name,
            properties: Vec::new(),
        })
    }

    fn show_dialog(&mut self, player: PlayerId, dialog: &Resolved) {
        let event = match dialog {
            Resolved::Registry(id) => ClientEvent::ShowDialogRegistry(id.clone()),
            Resolved::Inline(tag) => ClientEvent::ShowDialogInline(tag.clone()),
        };
        self.push(Audience::One(player), event);
    }

    fn clear_dialog(&mut self, player: PlayerId) {
        self.push(Audience::One(player), ClientEvent::ClearDialog);
    }

    fn send_chat(
        &mut self,
        to: PlayerId,
        kind: ChatKind,
        sender: Component,
        target: Option<Component>,
        message: Component,
    ) {
        self.push(
            Audience::One(to),
            ClientEvent::ChatMessage {
                chat_type: kind,
                sender_name: sender,
                target_name: target,
                message,
            },
        );
    }

    fn waypoint_icon(&self, player: PlayerId) -> WaypointIcon {
        self.surface.waypoints.icon(player)
    }

    fn set_waypoint_icon(&mut self, player: PlayerId, icon: WaypointIcon) {
        self.surface.waypoints.set_icon(player, icon);
        self.sync_waypoints();
    }

    fn waypoint_transmitters(&self, dimension: &Identifier) -> Vec<(PlayerId, BlockPos)> {
        Server::waypoint_transmitters(self, dimension)
    }

    fn server_port(&self) -> Option<u16> {
        Some(self.config.server_port)
    }

    fn perf_start(&mut self, requester: Option<PlayerId>) -> bool {
        self.begin_perf_run(requester)
    }

    fn perf_stop(&mut self) -> Option<PerfReport> {
        self.end_perf_run().map(|(report, _)| report)
    }

    fn post_effects_of(&self, player: PlayerId) -> Vec<Identifier> {
        self.surface
            .post_effects
            .get(&player)
            .cloned()
            .unwrap_or_default()
    }

    fn set_post_effects(&mut self, player: PlayerId, effects: Vec<Identifier>) {
        if effects.is_empty() {
            self.surface.post_effects.remove(&player);
        } else {
            self.surface.post_effects.insert(player, effects.clone());
        }
        self.push(Audience::One(player), ClientEvent::PostEffects(effects));
    }
}
