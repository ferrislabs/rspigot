use rspigot_domain::player::{GameMode, PlayerId};
use rspigot_domain::{Rotation, Vec3};
use uuid::Uuid;

use crate::plugins::events::PlayerTeleportCause;
use crate::ports::ClientLink;
use crate::server::Server;
use crate::world_state::DimensionKey;

impl<L: ClientLink> Server<L> {
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
