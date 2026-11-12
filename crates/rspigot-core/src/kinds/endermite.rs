use rspigot_ai::MobView;
use rspigot_domain::EntityId;
use rspigot_domain::entity::species::Species;
use rspigot_domain::entity::{Entity, EntityState, RemovalReason};
use rspigot_domain::item::ItemStack;
use rspigot_domain::player::PlayerId;

use crate::events::Hand;
use crate::health::DamageSource;
use crate::interact::Interaction;
use crate::plugins::events::EntityRemoveCause;
use crate::ports::ClientLink;
use crate::server::Server;

impl<L: ClientLink> Server<L> {
    pub(crate) fn endermite_view(&self, _entity: &Entity, _view: &mut MobView) {}

    pub(crate) fn endermite_pre_tick(&mut self, id: EntityId) -> bool {
        if let Some(Entity {
            base,
            state: EntityState::Living(state),
            ..
        }) = self.entities.store.get_mut(id)
        {
            state.body_yaw = base.yaw;
        }
        true
    }

    pub(crate) fn endermite_post_tick(&mut self, id: EntityId) {
        let Some(Entity {
            state: EntityState::Living(state),
            ..
        }) = self.entities.store.get_mut(id)
        else {
            return;
        };
        let persistent = state
            .mob
            .as_ref()
            .is_some_and(|mob| mob.persistence_required);
        let expired = match state.mob.as_mut().map(|mob| &mut mob.species) {
            Some(Species::Endermite(endermite)) => endermite.tick(persistent),
            _ => false,
        };
        if expired {
            self.remove_entity(
                id,
                RemovalReason::Discarded,
                Some(EntityRemoveCause::Despawn),
            );
        }
    }

    pub(crate) fn endermite_custom_ai_step(&mut self, _id: EntityId) {}

    pub(crate) fn endermite_interact(
        &mut self,
        _player: PlayerId,
        _target: EntityId,
        _hand: Hand,
        _held: Option<&ItemStack>,
    ) -> Option<Interaction> {
        None
    }

    pub(crate) fn endermite_hurt_amount(
        &mut self,
        _id: EntityId,
        _source: &DamageSource,
        amount: f32,
    ) -> Option<f32> {
        Some(amount)
    }

    pub(crate) fn endermite_after_hurt(
        &mut self,
        _id: EntityId,
        _source: &DamageSource,
        _dead: bool,
    ) {
    }
}
