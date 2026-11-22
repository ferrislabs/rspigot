use rspigot_domain::entity::save;
use rspigot_domain::{ChunkPos, EntityId, Vec3};
use rspigot_entities::EntityKind;
use rspigot_nbt::{Compound, Tag};
use uuid::Uuid;

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
const PEARL_TICKET_TICKS: u64 = 40;
const PEARL_DIMENSION: &str = "ender_pearl_dimension";

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

    pub(crate) fn pearl_saves(&self, owner: Uuid) -> Vec<Compound> {
        let world = self.world.level.uuid;
        let dimension = self.world.dimension.key.to_string();
        self.entities
            .store
            .iter()
            .filter(|entity| entity.base.kind == EntityKind::EnderPearl)
            .filter(|entity| !entity.base.is_removed())
            .filter(|entity| {
                matches!(&entity.state, EntityState::Thrown(state) if state.owner == Some(owner))
            })
            .filter_map(|entity| self.entity_nbt(entity, world))
            .map(|mut nbt| {
                nbt.insert(PEARL_DIMENSION, dimension.as_str());
                nbt
            })
            .collect()
    }

    pub(crate) fn hold_pearl_chunks(&mut self, id: EntityId) {
        if self.thrown_owner_player(id).is_none() {
            return;
        }
        let Some(chunk) = self
            .entities
            .store
            .get(id)
            .map(|entity| entity.base.position.chunk())
        else {
            return;
        };
        self.hold_chunks_around(chunk);
    }

    fn hold_chunks_around(&mut self, chunk: ChunkPos) {
        for dx in -1..=1 {
            for dz in -1..=1 {
                let at = ChunkPos::new(chunk.x + dx, chunk.z + dz);
                self.world.chunks.hold_timed(
                    at,
                    PEARL_TICKET_TICKS,
                    dx == 0 && dz == 0,
                    &self.workers,
                );
            }
        }
    }

    pub(crate) fn queue_saved_pearls(&mut self, pearls: &[Compound]) {
        let here = self.world.dimension.key.to_string();
        for nbt in pearls {
            if nbt.get_str(PEARL_DIMENSION).is_some_and(|key| key != here) {
                tracing::warn!("Skipping a saved ender pearl from another dimension");
                continue;
            }
            let Some(position) = pearl_position(nbt) else {
                continue;
            };
            let chunk = position.chunk();
            self.hold_chunks_around(chunk);
            self.pending_pearls.push((chunk, nbt.clone()));
        }
    }

    pub(crate) fn spawn_pending_pearls(&mut self) {
        if self.pending_pearls.is_empty() {
            return;
        }
        let pending = std::mem::take(&mut self.pending_pearls);
        for (chunk, nbt) in pending {
            if !self.world.chunks.is_loaded(chunk) {
                self.hold_chunks_around(chunk);
                self.pending_pearls.push((chunk, nbt));
                continue;
            }
            let id = self.allocate_entity_id_for(EntityKind::EnderPearl);
            let uuid = self.random_uuid();
            match save::load_entity(&nbt, id, uuid) {
                Ok(entity) => {
                    let _ = self.place_entity(entity);
                    self.hold_chunks_around(chunk);
                }
                Err(error) => tracing::warn!(%error, "Skipping a saved ender pearl"),
            }
        }
    }
}

fn pearl_position(nbt: &Compound) -> Option<Vec3> {
    let list = nbt.get_list("Pos").filter(|list| list.len() == 3)?;
    let mut axes = [0.0; 3];
    for (axis, tag) in axes.iter_mut().zip(list) {
        match tag {
            Tag::Double(value) => *axis = *value,
            _ => return None,
        }
    }
    Some(Vec3::new(axes[0], axes[1], axes[2]))
}
