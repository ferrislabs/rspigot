use rspigot_domain::player::PlayerId;
use rspigot_domain::text::Component;
use rspigot_nbt::Compound;

use crate::commands::Target;
use crate::commands::source::{DataTarget, Relation};
use crate::ports::ClientLink;
use crate::server::Server;

fn invalid(key: &str) -> Component {
    Component::translatable(key, Vec::new())
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn command_data(&self, target: &DataTarget) -> Result<Compound, Component> {
        match target {
            DataTarget::Storage(id) => {
                Ok(self.command_storage.get(id).cloned().unwrap_or_default())
            }
            DataTarget::Block(pos) => self
                .block_entity_nbt(*pos)
                .ok_or_else(|| invalid("commands.data.block.invalid")),
            DataTarget::Entity(Target::Player(id)) => self
                .players
                .get(id)
                .map(|player| crate::player_data::PlayerSave::capture(player).to_nbt())
                .ok_or_else(|| invalid("commands.data.entity.invalid")),
            DataTarget::Entity(Target::Entity(id)) => self
                .saved_entity(*id)
                .ok_or_else(|| invalid("commands.data.entity.invalid")),
        }
    }

    pub(crate) fn set_command_data(
        &mut self,
        target: &DataTarget,
        data: Compound,
    ) -> Result<(), Component> {
        match target {
            DataTarget::Storage(id) => {
                self.command_storage.insert(id.clone(), data);
                Ok(())
            }
            DataTarget::Block(pos) => {
                let chunk = self
                    .world
                    .chunks
                    .chunk_mut(pos.chunk())
                    .ok_or_else(|| invalid("commands.data.block.invalid"))?;
                let mut entity = chunk
                    .block_entity(*pos)
                    .cloned()
                    .ok_or_else(|| invalid("commands.data.block.invalid"))?;
                entity.data = data;
                chunk.set_block_entity(entity);
                Ok(())
            }
            DataTarget::Entity(_) => Err(invalid("commands.data.entity.invalid")),
        }
    }

    pub(crate) fn target_of_entity(&self, id: rspigot_domain::EntityId) -> Option<Target> {
        if let Some(player) = self.players.values().find(|p| p.entity_id == id) {
            return Some(Target::Player(player.id));
        }
        self.entities.store.get(id).map(|_| Target::Entity(id))
    }

    pub(crate) fn entity_id_of(&self, target: Target) -> Option<rspigot_domain::EntityId> {
        match target {
            Target::Player(id) => self.players.get(&id).map(|p| p.entity_id),
            Target::Entity(id) => Some(id),
        }
    }

    pub(crate) fn relations_of(&self, target: Target, relation: Relation) -> Vec<Target> {
        let Some(entity) = self.entity_id_of(target) else {
            return Vec::new();
        };
        let rides = &self.entities.rides;
        let found: Vec<rspigot_domain::EntityId> = match relation {
            Relation::Vehicle => rides.vehicle_of(entity).into_iter().collect(),
            Relation::Passengers => rides.passengers_of(entity).to_vec(),
            Relation::Controller => rides
                .passengers_of(entity)
                .iter()
                .copied()
                .find(|passenger| self.players.values().any(|p| p.entity_id == *passenger))
                .into_iter()
                .collect(),
            Relation::Leasher => self
                .entities
                .store
                .get(entity)
                .and_then(|e| self.leash_holder_of(e))
                .into_iter()
                .collect(),
            Relation::Owner => self
                .entities
                .store
                .get(entity)
                .and_then(|e| rspigot_domain::entity::tamable::owner(&e.base))
                .and_then(|uuid| {
                    self.entities
                        .store
                        .by_uuid(uuid)
                        .or_else(|| self.players.get(&PlayerId(uuid)).map(|p| p.entity_id))
                })
                .into_iter()
                .collect(),
            Relation::Target | Relation::Attacker | Relation::Origin => Vec::new(),
        };
        found
            .into_iter()
            .filter_map(|id| self.target_of_entity(id))
            .collect()
    }
}
