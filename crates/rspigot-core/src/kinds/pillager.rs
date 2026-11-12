use rspigot_ai::MobView;
use rspigot_domain::entity::Entity;

use crate::ports::ClientLink;
use crate::server::Server;

impl<L: ClientLink> Server<L> {
    pub(crate) fn pillager_view(&self, entity: &Entity, view: &mut MobView) {
        self.witch_view(entity, view);
        if let Some(charge) = self.crossbow_view_charge(entity) {
            view.species.crossbow.charge_ticks = charge;
        }
    }
}
