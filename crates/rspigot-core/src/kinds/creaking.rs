use rspigot_ai::{CreakingAction, MobView};
use rspigot_blocks::BlockKind;
use rspigot_domain::EntityId;
use rspigot_domain::damage::{DamageTag, DamageType};
use rspigot_domain::entity::Entity;
use rspigot_domain::entity::species::{Species, creaking};
use rspigot_entities::EntityKind;

use crate::events::{ClientEvent, EntityStatus};
use crate::health::DamageSource;
use crate::ports::ClientLink;
use crate::server::{Audience, Server};

const HURT_SOUND: &str = "minecraft:entity.creaking.sway";

fn state_of(entity: &mut Entity) -> Option<&mut creaking::CreakingState> {
    match entity
        .living_mut()
        .and_then(|living| living.mob.as_mut())
        .map(|mob| &mut mob.species)
    {
        Some(Species::Creaking(state)) => Some(state),
        _ => None,
    }
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn creaking_view(&self, entity: &Entity, view: &mut MobView) {
        view.species.creaking.home_pos = creaking::home_pos(&entity.base);
    }

    pub(crate) fn creaking_action(&mut self, id: EntityId, action: CreakingAction) {
        let CreakingAction::SetState { can_move, active } = action;
        if let Some(entity) = self.entities.store.get_mut(id) {
            creaking::set_can_move(&mut entity.base, can_move);
            creaking::set_active(&mut entity.base, active);
        }
    }

    pub(crate) fn creaking_ai_step(&mut self, id: EntityId) {
        if let Some(state) = self.entities.store.get_mut(id).and_then(state_of) {
            state.tick();
        }
    }

    pub(crate) fn creaking_post_tick(&mut self, id: EntityId) {
        let Some(home) = self
            .entities
            .store
            .get(id)
            .and_then(|entity| creaking::home_pos(&entity.base))
        else {
            return;
        };
        let has_heart = self
            .world
            .chunks
            .block(home)
            .is_some_and(|state| state.kind() == Some(BlockKind::CreakingHeart));
        if !has_heart {
            self.hurt_mob(id, &DamageSource::new(DamageType::GenericKill), f32::MAX);
        }
    }

    pub(crate) fn creaking_attack_event(&mut self, id: EntityId) {
        if let Some(state) = self.entities.store.get_mut(id).and_then(state_of) {
            state.attack_ticks = creaking::ATTACK_ANIMATION_TICKS;
        }
        self.broadcast_creaking_event(id, creaking::ATTACK_EVENT);
    }

    fn broadcast_creaking_event(&mut self, id: EntityId, event: i8) {
        self.push(
            Audience::Trackers(id),
            ClientEvent::EntityStatus {
                entity: id,
                status: EntityStatus::Event(event),
            },
        );
    }

    pub(crate) fn creaking_can_move(&self, id: EntityId) -> bool {
        self.entities.store.get(id).is_none_or(|entity| {
            entity.base.kind != EntityKind::Creaking || creaking::can_move(&entity.base)
        })
    }

    pub(crate) fn creaking_hurt_amount(
        &mut self,
        id: EntityId,
        source: &DamageSource,
        amount: f32,
    ) -> Option<f32> {
        let Some(entity) = self.entities.store.get(id) else {
            return Some(amount);
        };
        if !creaking::is_heart_bound(&entity.base)
            || source.kind.is(DamageTag::BypassesInvulnerability)
        {
            return Some(amount);
        }
        let ticks = self
            .entities
            .store
            .get_mut(id)
            .and_then(state_of)
            .map_or(0, |state| state.invulnerability_ticks);
        if ticks >= 1 || !self.living_entity_alive(id) {
            return None;
        }
        let blamed_player = source
            .causing
            .as_ref()
            .is_some_and(|causing| causing.player.is_some());
        let direct_attacks = source.direct.as_ref().is_some_and(|direct| {
            self.owner_is_living(direct.entity)
                || self
                    .entities
                    .store
                    .get(direct.entity)
                    .is_some_and(|entity| {
                        matches!(
                            entity.state,
                            rspigot_domain::entity::EntityState::Hurting(_)
                        ) || matches!(entity.state, rspigot_domain::entity::EntityState::Arrow(_))
                            || matches!(
                                entity.state,
                                rspigot_domain::entity::EntityState::Thrown(_)
                            )
                    })
        });
        if !direct_attacks && !blamed_player {
            return None;
        }
        if let Some(state) = self.entities.store.get_mut(id).and_then(state_of) {
            state.invulnerability_ticks = creaking::INVULNERABILITY_TICKS;
        }
        self.broadcast_creaking_event(id, creaking::HURT_EVENT);
        self.entity_sound(id, HURT_SOUND, 1.0, 1.0);
        None
    }
}
