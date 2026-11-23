use futures_channel::oneshot;

use crate::plugins::decisions::{Continuation, Lane};
use crate::plugins::events::{ServerListPing, ServerListPingPatch};
use crate::plugins::steps::PingStep;
use crate::plugins::{Access, EventKind, EventPayload};
use crate::ports::ClientLink;
use crate::server::Server;
use crate::status::{PingHook, PlayerSample, StatusPing, StatusReply};

const SAMPLE_SIZE: usize = 12;

fn count(players: usize) -> i32 {
    i32::try_from(players).unwrap_or(i32::MAX)
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn refresh_ping_hook(&self) {
        let hook = if self
            .plugins
            .listens(EventKind::ServerListPing, Access::Decide)
        {
            PingHook::Ask
        } else if self
            .plugins
            .listens(EventKind::ServerListPing, Access::Observe)
        {
            PingHook::Notify
        } else {
            PingHook::Off
        };
        self.status.update(|status| status.ping = hook);
    }

    fn max_players_shown(&self) -> i32 {
        i32::try_from(self.config.max_players).unwrap_or(i32::MAX)
    }

    pub(crate) fn status_ping(
        &mut self,
        ping: StatusPing,
        reply: Option<oneshot::Sender<StatusReply>>,
    ) {
        let StatusPing {
            address,
            hostname,
            motd,
            legacy,
        } = ping;
        let step = PingStep {
            motd: motd.clone(),
            legacy,
            reply,
        };
        self.fire(
            Lane::Ping,
            Continuation::Ping(Box::new(step)),
            move |server| {
                let players = if legacy {
                    Vec::new()
                } else {
                    server.online_snapshots()
                };
                Some(EventPayload::ServerListPing(ServerListPing {
                    address,
                    hostname,
                    players,
                    max_players: server.max_players_shown(),
                    motd,
                    num_players: count(server.players.len()),
                    hidden_players: Vec::new(),
                }))
            },
        );
    }

    pub(super) fn finish_ping(&mut self, step: PingStep, patch: ServerListPingPatch) {
        let Some(reply) = step.reply else {
            return;
        };
        let (hidden, icon) = if step.legacy {
            (Vec::new(), None)
        } else {
            (patch.hidden_players.apply(Vec::new()), patch.server_icon)
        };
        let mut shown = self
            .players
            .values()
            .filter(|player| !hidden.contains(&player.id.0));
        let sample: Vec<PlayerSample> = crate::status::shuffled(
            shown
                .by_ref()
                .take(SAMPLE_SIZE)
                .map(|player| {
                    crate::status::sample_entry(
                        player.id,
                        &player.profile.name,
                        player.settings.allows_listing,
                    )
                })
                .collect(),
        );
        let online = count(sample.len() + shown.count());
        let _ = reply.send(StatusReply {
            motd: patch.motd.unwrap_or(step.motd),
            max_players: patch
                .max_players
                .unwrap_or_else(|| self.max_players_shown()),
            online,
            sample,
            icon,
        });
    }
}
