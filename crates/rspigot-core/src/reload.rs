use crossbeam_channel::Sender;

use crate::admin::AdminLists;
use crate::config::GameConfig;
use crate::events::ClientEvent;
use crate::permissions::{Permissions, Rights};
use crate::ports::ClientLink;
use crate::server::Server;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReloadRequest {
    pub whitelist_only: bool,
}

pub struct ReloadData {
    pub admin: AdminLists,
    pub config: Option<GameConfig>,
    pub permissions: Option<Permissions>,
}

impl<L: ClientLink> Server<L> {
    pub fn with_reload_sink(mut self, sink: Sender<ReloadRequest>) -> Self {
        self.reload_sink = Some(sink);
        self
    }

    pub fn with_permissions(mut self, permissions: Permissions) -> Self {
        self.permissions = permissions;
        self
    }

    pub fn rights_of(&self, id: rspigot_domain::player::PlayerId) -> Option<Rights> {
        self.players.get(&id).map(|player| {
            self.permissions
                .rights(id, player.name(), player.permission_level())
        })
    }

    pub(crate) fn ask_reload(&mut self, whitelist_only: bool) -> bool {
        self.reload_sink
            .as_ref()
            .is_some_and(|sink| sink.send(ReloadRequest { whitelist_only }).is_ok())
    }

    pub(crate) fn apply_reload(&mut self, data: ReloadData) {
        let ReloadData {
            admin,
            config,
            permissions,
        } = data;
        if let Some(permissions) = permissions {
            self.permissions = permissions;
        }
        match config {
            Some(config) => {
                self.config.max_players = config.max_players;
                self.config.player_idle_timeout = config.player_idle_timeout;
                self.config.default_game_mode = config.default_game_mode;
                self.config.force_game_mode = config.force_game_mode;
                self.config.allow_flight = config.allow_flight;
                self.config.spawn_protection = config.spawn_protection;
                self.config.op_permission_level = config.op_permission_level;
                self.config.function_permission_level = config.function_permission_level;
                self.admin = admin;
            }
            None => self.admin.whitelist = admin.whitelist,
        }
        self.publish_admin();
        self.refresh_operators();
        tracing::info!("Reloaded the server configuration and the player lists");
    }

    fn refresh_operators(&mut self) {
        let ids: Vec<_> = self.players.keys().copied().collect();
        for id in ids {
            let Some(player) = self.players.get_mut(&id) else {
                continue;
            };
            let operator = self.admin.is_operator(&player.profile);
            let level = self
                .admin
                .operator_entry(&player.profile)
                .and_then(|entry| entry.level)
                .unwrap_or(self.config.op_permission_level);
            if player.operator == operator && player.op_level == level {
                continue;
            }
            player.operator = operator;
            player.op_level = level;
            player.link.send(ClientEvent::OperatorLevel {
                entity: player.entity_id,
                level: player.permission_level(),
            });
            self.send_commands(id);
        }
    }
}
