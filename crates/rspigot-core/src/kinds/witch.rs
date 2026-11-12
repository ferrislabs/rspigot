use rspigot_ai::{MobView, RaiderAction, RaiderFacts};
use rspigot_domain::EntityId;
use rspigot_domain::entity::Entity;

use crate::ports::ClientLink;
use crate::server::Server;

impl<L: ClientLink> Server<L> {
    pub(crate) fn raider_action(&mut self, id: EntityId, action: RaiderAction) {
        match action {
            RaiderAction::StopPatrolling => {
                if let Some(raider) = self.raider_state_mut(id) {
                    raider.patrolling = false;
                }
            }
            RaiderAction::SetPatrolTarget { mob, target } => {
                if let Some(raider) = self.raider_state_mut(mob) {
                    raider.set_patrol_target(target);
                }
            }
        }
    }

    fn raider_state_mut(
        &mut self,
        id: EntityId,
    ) -> Option<&mut rspigot_domain::entity::species::raider::RaiderState> {
        self.entities
            .store
            .get_mut(id)?
            .living_mut()?
            .mob
            .as_mut()?
            .species
            .raider_mut()
    }

    pub(crate) fn witch_view(&self, entity: &Entity, view: &mut MobView) {
        let raider = entity
            .living()
            .and_then(|living| living.mob.as_ref())
            .and_then(|mob| mob.species.raider());
        if let Some(raider) = raider {
            view.species.raider = RaiderFacts {
                patrolling: raider.patrolling,
                patrol_leader: raider.patrol_leader,
                patrol_target: raider.patrol_target,
            };
        }
    }
}
