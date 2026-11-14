use rspigot_ai::{GuardianAction, GuardianFacts, MobView};
use rspigot_domain::EntityId;
use rspigot_domain::damage::DamageType;
use rspigot_domain::effect::{EffectDuration, EffectId, EffectInstance};
use rspigot_domain::entity::living;
use rspigot_domain::entity::species::guardian;
use rspigot_domain::entity::{Entity, EntityState};
use rspigot_domain::player::GameMode;

use crate::events::{ClientEvent, GameEvent};
use crate::health::DamageSource;
use crate::plugins::events::EntityPotionEffectCause;
use crate::ports::ClientLink;
use crate::server::{Audience, Server};

pub(crate) fn guardian_facts_of(entity: &Entity) -> GuardianFacts {
    GuardianFacts {
        attack_target: guardian::active_attack_target(&entity.base),
    }
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn guardian_view(&self, entity: &Entity, view: &mut MobView) {
        view.species.guardian = guardian_facts_of(entity);
    }

    pub(crate) fn guardian_action(&mut self, id: EntityId, action: GuardianAction) {
        match action {
            GuardianAction::Beam { target, damage } => {
                let Some(source) = self.mob_source(id) else {
                    return;
                };
                let source = DamageSource::by(DamageType::IndirectMagic, source);
                self.hurt_entity(target, &source, damage);
            }
        }
    }

    pub(crate) fn guardian_ai_step(&mut self, id: EntityId) {
        let mut random = self.mob_random(id);
        let Some(Entity {
            base,
            state: EntityState::Living(state),
            ..
        }) = self.entities.store.get_mut(id)
        else {
            return;
        };
        if living::is_dead_or_dying(base, state) {
            return;
        }
        if base.fluid.in_water {
            base.set_air_supply(guardian::AIR_SUPPLY);
        } else if base.on_ground {
            guardian::flop(base, &mut random);
        }
        if guardian::active_attack_target(base) != 0 {
            base.set_yaw(base.head_yaw);
        }
        self.store_mob_random(id, random);
    }

    pub(crate) fn guardian_hurt_amount(
        &mut self,
        id: EntityId,
        source: &DamageSource,
        amount: f32,
    ) -> Option<f32> {
        let moving = self
            .entities
            .store
            .get(id)
            .is_some_and(|entity| guardian::is_moving(&entity.base));
        let avoided =
            source.kind.in_tag(guardian::THORNS_AVOIDING_TAG) || source.kind == DamageType::Thorns;
        if !moving
            && !avoided
            && let Some(direct) = source.direct.as_ref()
            && direct.kind.living()
            && let Some(guardian_source) = self.mob_source(id)
        {
            let thorns = DamageSource::by(DamageType::Thorns, guardian_source);
            self.hurt_entity(direct.entity, &thorns, guardian::THORNS_DAMAGE);
        }
        if let Some(entry) = self.entities.mobs.entries.get_mut(&id) {
            entry.ai.trigger_guardian_stroll();
        }
        Some(amount)
    }

    pub(crate) fn elder_guardian_custom_ai_step(&mut self, id: EntityId) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let phase = i64::from(entity.base.tick_count) + i64::from(id.0);
        let position = entity.base.position;
        let silent = entity.base.is_silent();
        let block = position.block();
        if phase % i64::from(guardian::ELDER_EFFECT_PERIOD) == 0 {
            self.elder_curses_players(position, silent);
        }
        if let Some(EntityState::Living(state)) = self
            .entities
            .store
            .get_mut(id)
            .map(|entity| &mut entity.state)
            && let Some(mob) = state.mob.as_mut()
            && mob.home_radius == -1
        {
            mob.home_pos = block;
            mob.home_radius = guardian::ELDER_HOME_RADIUS;
        }
    }

    fn elder_curses_players(&mut self, from: rspigot_domain::Vec3, silent: bool) {
        let Some(fatigue) = EffectId::from_name(guardian::ELDER_EFFECT) else {
            return;
        };
        let effect = EffectInstance::new(
            fatigue,
            EffectDuration::Ticks(guardian::ELDER_EFFECT_DURATION),
            guardian::ELDER_EFFECT_AMPLIFIER,
            false,
        );
        let radius = guardian::ELDER_EFFECT_RADIUS;
        let reapply = (guardian::ELDER_EFFECT_REAPPLY - 1) as u32;
        let affected: Vec<_> = self
            .players
            .values()
            .filter(|player| {
                matches!(player.game_mode, GameMode::Survival | GameMode::Adventure)
                    && player.position.distance_squared(from) < radius * radius
                    && player.effects.get(fatigue).is_none_or(|current| {
                        current.amplifier < guardian::ELDER_EFFECT_AMPLIFIER
                            || rspigot_domain::potion::ends_within(current, reapply)
                    })
            })
            .map(|player| (player.id, player.entity_id))
            .collect();
        let value = if silent { 0.0 } else { 1.0 };
        for (player, entity) in affected {
            self.add_effect_to(entity, effect, EntityPotionEffectCause::Attack);
            self.push(
                Audience::One(player),
                ClientEvent::GameEvent(GameEvent::GuardianElderEffect(value)),
            );
        }
    }
}
