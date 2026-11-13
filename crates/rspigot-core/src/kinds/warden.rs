use rspigot_ai::WardenAction;
use rspigot_domain::EntityId;
use rspigot_domain::entity::RemovalReason;
use rspigot_domain::entity::species::warden;

use crate::plugins::events::EntityRemoveCause;
use crate::ports::ClientLink;
use crate::server::Server;

impl<L: ClientLink> Server<L> {
    pub(crate) fn warden_action(&mut self, id: EntityId, action: WardenAction) {
        match action {
            WardenAction::Discard => {
                self.remove_entity(
                    id,
                    RemovalReason::Discarded,
                    Some(EntityRemoveCause::Despawn),
                );
            }
            WardenAction::ClientAngerLevel(level) => {
                if let Some(entity) = self.entities.store.get_mut(id) {
                    warden::set_client_anger_level(&mut entity.base, level);
                }
            }
            WardenAction::Unmount | WardenAction::Darkness | WardenAction::SonicBoom { .. } => {}
        }
    }
}
