use rspigot_domain::text::Component;

use crate::events::ClientEvent;
use crate::ports::ClientLink;
use crate::server::{Audience, Server};

pub const LATENCY_BROADCAST_TICKS: u64 = 600;

impl<L: ClientLink> Server<L> {
    pub(crate) fn broadcast_latency(&mut self) {
        if self.tick == 0
            || !self.tick.is_multiple_of(LATENCY_BROADCAST_TICKS)
            || self.players.is_empty()
        {
            return;
        }
        let latencies = self
            .players
            .values()
            .map(|player| (player.id, player.latency))
            .collect();
        self.push(Audience::All, ClientEvent::PlayerInfoLatency(latencies));
    }

    pub(crate) fn server_data(&self) -> ClientEvent {
        ClientEvent::ServerData {
            motd: Component::text(self.config.motd.clone()),
            icon: self.config.server_icon.clone(),
        }
    }

    pub(crate) fn propagate_view_distance(&mut self) {
        let max = self.config.view_distance;
        self.push(Audience::All, ClientEvent::ViewDistance(max));
        for player in self.players.values_mut() {
            let view = max.min(player.settings.view_distance.max(2));
            player.streamer.set_view_distance(
                view,
                &mut self.world.chunks,
                &self.workers,
                &player.link,
            );
        }
    }

    pub(crate) fn propagate_simulation_distance(&mut self) {
        let distance = self.config.simulation_distance;
        self.push(Audience::All, ClientEvent::SimulationDistance(distance));
    }
}
