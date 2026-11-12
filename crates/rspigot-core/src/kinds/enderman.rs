use rspigot_ai::{EndermanAction, MobView};
use rspigot_domain::EntityId;
use rspigot_domain::entity::Entity;
use rspigot_domain::item::ItemStack;
use rspigot_domain::player::PlayerId;

use crate::events::Hand;
use crate::health::DamageSource;
use crate::interact::Interaction;
use crate::ports::ClientLink;
use crate::server::Server;

impl<L: ClientLink> Server<L> {
    pub(crate) fn enderman_view(&self, _entity: &Entity, _view: &mut MobView) {}

    pub(crate) fn enderman_pre_tick(&mut self, _id: EntityId) -> bool {
        true
    }

    pub(crate) fn enderman_post_tick(&mut self, _id: EntityId) {}

    pub(crate) fn enderman_custom_ai_step(&mut self, _id: EntityId) {}

    pub(crate) fn enderman_interact(
        &mut self,
        _player: PlayerId,
        _target: EntityId,
        _hand: Hand,
        _held: Option<&ItemStack>,
    ) -> Option<Interaction> {
        None
    }

    pub(crate) fn enderman_hurt_amount(
        &mut self,
        _id: EntityId,
        _source: &DamageSource,
        amount: f32,
    ) -> Option<f32> {
        Some(amount)
    }

    pub(crate) fn enderman_after_hurt(
        &mut self,
        _id: EntityId,
        _source: &DamageSource,
        _dead: bool,
    ) {
    }

    pub(crate) fn enderman_action(&mut self, _id: EntityId, action: EndermanAction) {
        match action {}
    }
}
