use rspigot_ai::{MobView, VexAction};
use rspigot_domain::EntityId;
use rspigot_domain::damage::DamageType;
use rspigot_domain::entity::Entity;
use rspigot_domain::entity::species::{Species, vex};

use crate::health::DamageSource;
use crate::ports::ClientLink;
use crate::server::Server;

fn state_of(entity: &mut Entity) -> Option<&mut vex::VexState> {
    match entity
        .living_mut()
        .and_then(|living| living.mob.as_mut())
        .map(|mob| &mut mob.species)
    {
        Some(Species::Vex(state)) => Some(state),
        _ => None,
    }
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn vex_view(&self, entity: &Entity, view: &mut MobView) {
        let Some(Species::Vex(state)) = entity
            .living()
            .and_then(|living| living.mob.as_ref())
            .map(|mob| mob.species)
        else {
            return;
        };
        let owner_target = state
            .owner
            .and_then(|uuid| self.entities.store.by_uuid(uuid))
            .and_then(|owner| self.mob_target(owner));
        view.species.vex = rspigot_ai::VexFacts {
            bound_origin: state.bound_origin,
            charging: vex::is_charging(&entity.base),
            owner_target,
            has_owner_target: owner_target.is_some(),
        };
    }

    pub(crate) fn vex_action(&mut self, id: EntityId, action: VexAction) {
        let VexAction::SetCharging(charging) = action;
        if let Some(entity) = self.entities.store.get_mut(id) {
            vex::set_charging(&mut entity.base, charging);
        }
    }

    pub(crate) fn vex_pre_tick(&mut self, id: EntityId) -> bool {
        if let Some(entity) = self.entities.store.get_mut(id) {
            entity.base.no_physics = true;
        }
        true
    }

    pub(crate) fn vex_post_tick(&mut self, id: EntityId) {
        let Some(entity) = self.entities.store.get_mut(id) else {
            return;
        };
        entity.base.no_physics = false;
        entity.base.set_no_gravity(true);
        let starved = state_of(entity).is_some_and(vex::tick_life);
        if starved {
            self.hurt_mob(id, &DamageSource::new(DamageType::Starve), vex::LIFE_DAMAGE);
        }
    }
}
