use rspigot_ai::MobView;
use rspigot_domain::EntityId;
use rspigot_domain::entity::Entity;
use rspigot_domain::item::ItemStack;
use rspigot_domain::player::PlayerId;
use rspigot_entities::EntityKind as Kind;

use crate::events::Hand;
use crate::health::DamageSource;
use crate::interact::Interaction;
use crate::ports::ClientLink;
use crate::server::Server;

mod cat;
mod enderman;
mod endermite;
mod fox;
mod iron_golem;
mod ocelot;
mod panda;
mod polar_bear;
mod rabbit;
mod silverfish;
mod wolf;
mod zombified_piglin;

impl<L: ClientLink> Server<L> {
    fn kind_of(&self, id: EntityId) -> Option<Kind> {
        self.entities.store.get(id).map(|entity| entity.base.kind)
    }

    pub(crate) fn kind_view(&self, entity: &Entity, view: &mut MobView) {
        match entity.base.kind {
            Kind::Wolf => self.wolf_view(entity, view),
            Kind::Cat => self.cat_view(entity, view),
            Kind::Fox => self.fox_view(entity, view),
            Kind::Panda => self.panda_view(entity, view),
            Kind::PolarBear => self.polar_bear_view(entity, view),
            Kind::Enderman => self.enderman_view(entity, view),
            Kind::Endermite => self.endermite_view(entity, view),
            Kind::Silverfish => self.silverfish_view(entity, view),
            Kind::IronGolem => self.iron_golem_view(entity, view),
            Kind::ZombifiedPiglin => self.zombified_piglin_view(entity, view),
            Kind::Rabbit => self.rabbit_view(entity, view),
            Kind::Ocelot => self.ocelot_view(entity, view),
            _ => {}
        }
    }

    pub(crate) fn kind_pre_tick(&mut self, id: EntityId) -> bool {
        match self.kind_of(id) {
            Some(Kind::Wolf) => self.wolf_pre_tick(id),
            Some(Kind::Cat) => self.cat_pre_tick(id),
            Some(Kind::Fox) => self.fox_pre_tick(id),
            Some(Kind::Panda) => self.panda_pre_tick(id),
            Some(Kind::PolarBear) => self.polar_bear_pre_tick(id),
            Some(Kind::Enderman) => self.enderman_pre_tick(id),
            Some(Kind::Endermite) => self.endermite_pre_tick(id),
            Some(Kind::Silverfish) => self.silverfish_pre_tick(id),
            Some(Kind::IronGolem) => self.iron_golem_pre_tick(id),
            Some(Kind::ZombifiedPiglin) => self.zombified_piglin_pre_tick(id),
            Some(Kind::Rabbit) => self.rabbit_pre_tick(id),
            Some(Kind::Ocelot) => self.ocelot_pre_tick(id),
            _ => true,
        }
    }

    pub(crate) fn kind_post_tick(&mut self, id: EntityId) {
        match self.kind_of(id) {
            Some(Kind::Wolf) => self.wolf_post_tick(id),
            Some(Kind::Cat) => self.cat_post_tick(id),
            Some(Kind::Fox) => self.fox_post_tick(id),
            Some(Kind::Panda) => self.panda_post_tick(id),
            Some(Kind::PolarBear) => self.polar_bear_post_tick(id),
            Some(Kind::Enderman) => self.enderman_post_tick(id),
            Some(Kind::Endermite) => self.endermite_post_tick(id),
            Some(Kind::Silverfish) => self.silverfish_post_tick(id),
            Some(Kind::IronGolem) => self.iron_golem_post_tick(id),
            Some(Kind::ZombifiedPiglin) => self.zombified_piglin_post_tick(id),
            Some(Kind::Rabbit) => self.rabbit_post_tick(id),
            Some(Kind::Ocelot) => self.ocelot_post_tick(id),
            _ => {}
        }
    }

    pub(crate) fn kind_ai_step(&mut self, id: EntityId) {
        if self.kind_of(id) == Some(Kind::Fox) {
            self.fox_ai_step(id);
        }
    }

    pub(crate) fn kind_death_item(&mut self, id: EntityId) -> Option<ItemStack> {
        match self.kind_of(id) {
            Some(Kind::Fox) => self.fox_death_item(id),
            _ => None,
        }
    }

    pub(crate) fn kind_target_cleared(&mut self, id: EntityId) {
        if self.kind_of(id) == Some(Kind::Fox) {
            self.fox_target_cleared(id);
        }
    }

    pub(crate) fn play_ambient_sound(&mut self, id: EntityId) {
        match self.kind_of(id) {
            Some(Kind::Fox) => self.fox_ambient(id),
            Some(Kind::Wolf) => self.wolf_ambient_voice(id),
            Some(Kind::Cat) => self.cat_ambient_voice(id),
            _ => self.play_mob_voice(id, crate::mob_life::ambient_sound),
        }
    }

    pub(crate) fn kind_custom_ai_step(&mut self, id: EntityId) {
        match self.kind_of(id) {
            Some(Kind::Wolf) => self.wolf_custom_ai_step(id),
            Some(Kind::Cat) => self.cat_custom_ai_step(id),
            Some(Kind::Fox) => self.fox_custom_ai_step(id),
            Some(Kind::Panda) => self.panda_custom_ai_step(id),
            Some(Kind::PolarBear) => self.polar_bear_custom_ai_step(id),
            Some(Kind::Enderman) => self.enderman_custom_ai_step(id),
            Some(Kind::Endermite) => self.endermite_custom_ai_step(id),
            Some(Kind::Silverfish) => self.silverfish_custom_ai_step(id),
            Some(Kind::IronGolem) => self.iron_golem_custom_ai_step(id),
            Some(Kind::ZombifiedPiglin) => self.zombified_piglin_custom_ai_step(id),
            Some(Kind::Rabbit) => self.rabbit_custom_ai_step(id),
            Some(Kind::Ocelot) => self.ocelot_custom_ai_step(id),
            _ => {}
        }
    }

    pub(crate) fn kind_interact(
        &mut self,
        player: PlayerId,
        target: EntityId,
        hand: Hand,
        held: Option<&ItemStack>,
    ) -> Option<Interaction> {
        match self.kind_of(target) {
            Some(Kind::Wolf) => self.wolf_interact(player, target, hand, held),
            Some(Kind::Cat) => self.cat_interact(player, target, hand, held),
            Some(Kind::Fox) => self.fox_interact(player, target, hand, held),
            Some(Kind::Panda) => self.panda_interact(player, target, hand, held),
            Some(Kind::PolarBear) => self.polar_bear_interact(player, target, hand, held),
            Some(Kind::Enderman) => self.enderman_interact(player, target, hand, held),
            Some(Kind::Endermite) => self.endermite_interact(player, target, hand, held),
            Some(Kind::Silverfish) => self.silverfish_interact(player, target, hand, held),
            Some(Kind::IronGolem) => self.iron_golem_interact(player, target, hand, held),
            Some(Kind::ZombifiedPiglin) => {
                self.zombified_piglin_interact(player, target, hand, held)
            }
            Some(Kind::Rabbit) => self.rabbit_interact(player, target, hand, held),
            Some(Kind::Ocelot) => self.ocelot_interact(player, target, hand, held),
            _ => None,
        }
    }

    pub(crate) fn kind_hurt_amount(
        &mut self,
        id: EntityId,
        source: &DamageSource,
        amount: f32,
    ) -> Option<f32> {
        match self.kind_of(id) {
            Some(Kind::Wolf) => self.wolf_hurt_amount(id, source, amount),
            Some(Kind::Cat) => self.cat_hurt_amount(id, source, amount),
            Some(Kind::Fox) => self.fox_hurt_amount(id, source, amount),
            Some(Kind::Panda) => self.panda_hurt_amount(id, source, amount),
            Some(Kind::PolarBear) => self.polar_bear_hurt_amount(id, source, amount),
            Some(Kind::Enderman) => self.enderman_hurt_amount(id, source, amount),
            Some(Kind::Endermite) => self.endermite_hurt_amount(id, source, amount),
            Some(Kind::Silverfish) => self.silverfish_hurt_amount(id, source, amount),
            Some(Kind::IronGolem) => self.iron_golem_hurt_amount(id, source, amount),
            Some(Kind::ZombifiedPiglin) => self.zombified_piglin_hurt_amount(id, source, amount),
            Some(Kind::Rabbit) => self.rabbit_hurt_amount(id, source, amount),
            Some(Kind::Ocelot) => self.ocelot_hurt_amount(id, source, amount),
            _ => Some(amount),
        }
    }

    pub(crate) fn kind_absorbs_hurt(
        &mut self,
        id: EntityId,
        source: &DamageSource,
        amount: f32,
    ) -> bool {
        match self.kind_of(id) {
            Some(Kind::Wolf) => self.wolf_absorbs_hurt(id, source, amount),
            _ => false,
        }
    }

    pub(crate) fn kind_after_hurt(&mut self, id: EntityId, source: &DamageSource, dead: bool) {
        match self.kind_of(id) {
            Some(Kind::Wolf) => self.wolf_after_hurt(id, source, dead),
            Some(Kind::Cat) => self.cat_after_hurt(id, source, dead),
            Some(Kind::Fox) => self.fox_after_hurt(id, source, dead),
            Some(Kind::Panda) => self.panda_after_hurt(id, source, dead),
            Some(Kind::PolarBear) => self.polar_bear_after_hurt(id, source, dead),
            Some(Kind::Enderman) => self.enderman_after_hurt(id, source, dead),
            Some(Kind::Endermite) => self.endermite_after_hurt(id, source, dead),
            Some(Kind::Silverfish) => self.silverfish_after_hurt(id, source, dead),
            Some(Kind::IronGolem) => self.iron_golem_after_hurt(id, source, dead),
            Some(Kind::ZombifiedPiglin) => self.zombified_piglin_after_hurt(id, source, dead),
            Some(Kind::Rabbit) => self.rabbit_after_hurt(id, source, dead),
            Some(Kind::Ocelot) => self.ocelot_after_hurt(id, source, dead),
            _ => {}
        }
    }
}
