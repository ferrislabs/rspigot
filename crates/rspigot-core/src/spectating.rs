use crate::commands::{CommandContext, Target};
use rspigot_domain::EntityId;
use rspigot_domain::player::{GameMode, PlayerId};
use rspigot_domain::{Rotation, Vec3};
use uuid::Uuid;

use crate::plugins::events::PlayerTeleportCause;
use crate::ports::ClientLink;
use crate::server::Server;
use crate::world_state::DimensionKey;

const SPECTATE_REACH: f64 = 6.0;

impl<L: ClientLink> Server<L> {
    pub(crate) fn sync_spectator_chunks(&mut self) {
        let generate = self.config.rules.spectators_generate_chunks;
        for player in self.players.values_mut() {
            let skipped = !generate && player.game_mode == GameMode::Spectator;
            player.streamer.set_skipped(
                skipped,
                &mut self.world.chunks,
                &self.workers,
                &player.link,
            );
        }
    }

    pub(crate) fn handle_spectator_action(&mut self, id: PlayerId, target: Option<EntityId>) {
        let Some(player) = self.players.get(&id) else {
            return;
        };
        if player.game_mode != GameMode::Spectator {
            return;
        }
        let Some(target) = target else {
            return;
        };
        let from = player.position;
        let target = match self.players.values().find(|p| p.entity_id == target) {
            Some(other) => Some((Target::Player(other.id), other.position, 0.6)),
            None => self.entities.store.get(target).map(|entity| {
                (
                    Target::Entity(target),
                    entity.base.position,
                    f64::from(entity.base.dimensions.width),
                )
            }),
        };
        let Some((target, at, width)) = target else {
            return;
        };
        let reach = SPECTATE_REACH + width;
        if from.distance_squared(at) > reach * reach || !self.can_be_camera(target) {
            return;
        }
        self.set_camera_of(id, Some(target));
    }

    pub(crate) fn handle_teleport_to_entity(&mut self, id: PlayerId, target: Uuid) {
        if self
            .players
            .get(&id)
            .is_none_or(|player| player.game_mode != GameMode::Spectator)
        {
            return;
        }
        let Some((dimension, position, rotation)) = self.spectate_target(target) else {
            return;
        };
        let current = self.players.get(&id).map(|player| player.dim.clone());
        let dimension = (current.as_ref() != Some(&dimension)).then_some(dimension);
        self.ask_teleport_in(
            id,
            dimension,
            position,
            Some(rotation),
            PlayerTeleportCause::Spectate,
            None,
        );
    }

    fn spectate_target(&self, target: Uuid) -> Option<(DimensionKey, Vec3, Rotation)> {
        if let Some(player) = self.players.values().find(|p| p.profile.id.0 == target) {
            return Some((player.dim.clone(), player.position, player.rotation));
        }
        let entity = self.entities.store.iter().find(|e| e.base.uuid == target)?;
        Some((
            self.world.dimension.key.clone(),
            entity.base.position,
            Rotation::new(entity.base.yaw, entity.base.pitch),
        ))
    }
}
