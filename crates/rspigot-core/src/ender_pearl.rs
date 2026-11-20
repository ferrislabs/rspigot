use rspigot_domain::EntityId;
use rspigot_domain::damage::DamageType;
use rspigot_domain::entity::{EntityState, RemovalReason};
use rspigot_domain::player::PlayerId;
use rspigot_entities::SoundSource;

use crate::health::DamageSource;
use crate::plugins::events::{EntityRemoveCause, PlayerTeleportCause};
use crate::ports::ClientLink;
use crate::server::Server;

pub(crate) const PEARL_DAMAGE: f32 = 5.0;
const TELEPORT_SOUND: &str = "minecraft:entity.player.teleport";

impl<L: ClientLink> Server<L> {
    pub(crate) fn pearl_orphaned(&self, id: EntityId) -> bool {
        let Some(entity) = self.entities.store.get(id) else {
            return false;
        };
        let EntityState::Thrown(state) = &entity.state else {
            return false;
        };
        if state.owner.is_none() {
            return false;
        }
        match self
            .thrown_owner_player(id)
            .and_then(|player| self.players.get(&player))
        {
            Some(player) => {
                player.vitals.is_dead() && self.config.rules.ender_pearls_vanish_on_death
            }
            None => true,
        }
    }

    pub(crate) fn drop_orphaned_pearl(&mut self, id: EntityId) {
        self.remove_entity(
            id,
            RemovalReason::Discarded,
            Some(EntityRemoveCause::Despawn),
        );
    }

    pub(crate) fn pearl_hit(&mut self, id: EntityId) {
        let Some(position) = self.entities.store.get(id).map(|e| e.base.position) else {
            return;
        };
        let Some(player) = self.thrown_owner_player(id) else {
            return;
        };
        let ready = self
            .players
            .get(&player)
            .is_some_and(|p| !p.vitals.is_dead() && p.sleeping.is_none());
        if ready {
            self.ask_teleport(
                player,
                position,
                None,
                PlayerTeleportCause::EnderPearl,
                None,
            );
        }
    }

    pub(crate) fn pearl_landed(&mut self, player: PlayerId) {
        let Some(owner) = self.players.get_mut(&player) else {
            return;
        };
        owner.vitals.fall_distance = 0.0;
        let position = owner.position;
        self.hurt(
            player,
            &DamageSource::new(DamageType::EnderPearl),
            PEARL_DAMAGE,
        );
        self.play_sound(
            None,
            position,
            TELEPORT_SOUND,
            SoundSource::Players,
            1.0,
            1.0,
        );
    }
}
