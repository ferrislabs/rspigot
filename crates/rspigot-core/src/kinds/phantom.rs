use rspigot_ai::{MobView, PhantomAction, PhantomFacts};
use rspigot_domain::EntityId;
use rspigot_domain::entity::Entity;
use rspigot_domain::entity::species::Species;

use crate::ports::ClientLink;
use crate::server::Server;

const CAT_HISS: &str = "minecraft:entity.cat.hiss";

impl<L: ClientLink> Server<L> {
    pub(crate) fn phantom_view(&self, entity: &Entity, view: &mut MobView) {
        let Some(living) = entity.living() else {
            return;
        };
        let anchor = match living.mob.as_ref().map(|mob| mob.species) {
            Some(Species::Phantom(state)) => state.anchor,
            _ => None,
        };
        view.species.phantom = PhantomFacts {
            anchor,
            hurt_recently: living.hurt_time > 0,
        };
    }

    pub(crate) fn phantom_action(&mut self, _id: EntityId, action: PhantomAction) {
        let PhantomAction::Hiss(cat) = action;
        self.entity_sound(cat, CAT_HISS, 1.0, 1.0);
    }
}
