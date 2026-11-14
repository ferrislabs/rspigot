use rspigot_ai::{DrownedFacts, MobView};
use rspigot_domain::EntityId;
use rspigot_domain::entity::living;
use rspigot_domain::entity::species::{Species, drowned};
use rspigot_domain::entity::{Entity, EntityState};

use crate::plugins::EventPayload;
use crate::plugins::events::EntityToggleSwim;
use crate::plugins::steps::Continuation;
use crate::ports::ClientLink;
use crate::server::Server;

pub(crate) fn drowned_facts_of(entity: &Entity) -> DrownedFacts {
    let searching = match &entity.state {
        EntityState::Living(living) => match living.mob.as_ref().map(|mob| mob.species) {
            Some(Species::Zombie(zombie)) => zombie.searching_for_land,
            _ => false,
        },
        _ => false,
    };
    DrownedFacts {
        searching_for_land: searching,
    }
}

pub(crate) fn ambient_sound(entity: &Entity) -> &'static str {
    if entity.base.fluid.in_water {
        drowned::AMBIENT_WATER_SOUND
    } else {
        drowned::AMBIENT_SOUND
    }
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn drowned_view(&self, entity: &Entity, view: &mut MobView) {
        view.species.drowned = drowned_facts_of(entity);
    }

    pub(crate) fn drowned_post_tick(&mut self, id: EntityId) {
        let Some(Entity {
            base,
            state: EntityState::Living(state),
            ..
        }) = self.entities.store.get_mut(id)
        else {
            return;
        };
        let swimming = living::effective_ai(base, state)
            && base.eye_in_water
            && base.fluid.in_water
            && living::drowned_wants_to_swim(state);
        if base.shared_flag(drowned::SWIMMING_FLAG) == swimming {
            return;
        }
        let Some(view) = self.entity_view(id) else {
            return;
        };
        let lane = self.lane_for(None, id);
        self.fire(
            lane,
            Continuation::Swim {
                entity: id,
                swimming,
            },
            move |_| {
                Some(EventPayload::EntityToggleSwim(EntityToggleSwim {
                    entity_type: view.kind.clone(),
                    entity: view,
                    swimming,
                }))
            },
        );
    }

    pub(crate) fn finish_swim_toggle(&mut self, id: EntityId, swimming: bool) {
        if let Some(entity) = self.entities.store.get_mut(id) {
            entity
                .base
                .set_shared_flag(drowned::SWIMMING_FLAG, swimming);
        }
    }
}
