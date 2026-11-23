use rand::Rng;
use rspigot_domain::EntityId;
use rspigot_domain::damage::{DamageTag, DamageType};
use rspigot_domain::entity::minecart::{self, Tick};
use rspigot_domain::entity::{Entity, EntityState, RemovalReason, fire};
use rspigot_domain::player::PlayerId;
use rspigot_entities::{EntityKind as Kind, SoundSource};

use crate::events::Hand;
use crate::health::DamageSource;
use crate::interact::{Interaction, item_tag};
use crate::plugins::events::{EntityRemoveCause, ExplosionPrime, ExplosionPrimePatch};
use crate::plugins::steps::{PrimeKind, PrimeStep};
use crate::plugins::{Continuation, EventPayload};
use crate::ports::ClientLink;
use crate::server::Server;

fn minecart_state(entity: &Entity) -> Option<&minecart::MinecartState> {
    match &entity.state {
        EntityState::Minecart(state) => Some(state),
        _ => None,
    }
}

fn minecart_state_mut(entity: &mut Entity) -> Option<&mut minecart::MinecartState> {
    match &mut entity.state {
        EntityState::Minecart(state) => Some(state),
        _ => None,
    }
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn tick_special_minecart(&mut self, id: EntityId, outcome: &Tick) {
        match self.kind_of(id) {
            Some(Kind::TntMinecart) => self.tick_tnt_minecart(id, outcome),
            Some(Kind::FurnaceMinecart) => {
                if let Some(Entity {
                    base,
                    state: EntityState::Minecart(state),
                    ..
                }) = self.entities.store.get_mut(id)
                {
                    minecart::furnace_tick(base, state);
                }
            }
            Some(Kind::SpawnerMinecart) => self.tick_spawner_minecart(id),
            _ => {}
        }
    }

    fn tick_tnt_minecart(&mut self, id: EntityId, outcome: &Tick) {
        let Some(entity) = self.entities.store.get_mut(id) else {
            return;
        };
        let horizontal = entity.base.velocity.horizontal_distance_sqr();
        let Some(state) = minecart_state_mut(entity) else {
            return;
        };
        let mut blast = None;
        if state.fuse < 1 {
            if state.fuse == 0 {
                blast = Some(horizontal);
            }
        } else {
            state.fuse -= 1;
        }
        if outcome.block_collision.is_some() && horizontal >= minecart::TNT_COLLISION_SPEED_SQR {
            blast = Some(horizontal);
        }
        if let Some(distance) = outcome.fell
            && distance >= minecart::TNT_FALL_THRESHOLD
        {
            let scaled = distance / minecart::TNT_FALL_SCALE;
            blast = Some(scaled * scaled);
        }
        if let Some(speed_sqr) = blast {
            self.cart_explode(id, speed_sqr);
        }
    }

    pub(crate) fn cart_prime_fuse(&mut self, id: EntityId, by: Option<EntityId>) {
        if !self.config.rules.tnt_explodes {
            return;
        }
        let Some(entity) = self.entities.store.get_mut(id) else {
            return;
        };
        let silent = entity.base.is_silent();
        let position = entity.base.position;
        let Some(state) = minecart_state_mut(entity) else {
            return;
        };
        state.fuse = minecart::TNT_FUSE;
        if state.ignited_by.is_none() {
            state.ignited_by = by;
        }
        self.mob_entity_event_all(id, minecart::TNT_PRIMED_EVENT as i8);
        if !silent {
            self.play_sound(
                None,
                position,
                minecart::TNT_PRIMED_SOUND,
                SoundSource::Blocks,
                1.0,
                1.0,
            );
        }
    }

    pub(crate) fn cart_is_primed(&self, id: EntityId) -> bool {
        self.entities
            .store
            .get(id)
            .and_then(minecart_state)
            .is_some_and(|state| state.fuse > -1)
    }

    pub(crate) fn cart_explode(&mut self, id: EntityId, speed_sqr: f64) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        if entity.base.is_removed() {
            return;
        }
        let Some(state) = minecart_state(entity) else {
            return;
        };
        let (power, factor) = (state.explosion_power, state.explosion_speed);
        if !self.config.rules.tnt_explodes {
            if self.cart_is_primed(id) {
                self.remove_entity(
                    id,
                    RemovalReason::Discarded,
                    Some(EntityRemoveCause::Explode),
                );
            }
            return;
        }
        let roll = self.rng.random::<f64>();
        let speed = speed_sqr.sqrt().min(minecart::TNT_SPEED_CAP);
        let radius = (f64::from(power) + f64::from(factor) * roll * 1.5 * speed) as f32;
        let Some(view) = self.entity_view(id) else {
            return;
        };
        let lane = self.lane_for(None, id);
        let step = PrimeStep {
            source: id,
            radius,
            fire: false,
            kind: PrimeKind::Tnt,
        };
        self.fire(lane, Continuation::Prime(step), move |_| {
            Some(EventPayload::ExplosionPrime(ExplosionPrime {
                entity_type: view.kind.clone(),
                entity: view,
                fire: false,
                radius,
            }))
        });
    }

    pub(crate) fn finish_cart_explosion(
        &mut self,
        step: PrimeStep,
        cancelled: bool,
        patch: ExplosionPrimePatch,
    ) {
        let id = step.source;
        let Some(entity) = self.entities.store.get_mut(id) else {
            return;
        };
        if entity.base.is_removed() {
            return;
        }
        let center = entity.base.position;
        let Some(state) = minecart_state_mut(entity) else {
            return;
        };
        if cancelled {
            state.fuse = -1;
            return;
        }
        let ignited_by = state.ignited_by;
        let direct = self.mob_source(id);
        let causing = ignited_by.and_then(|by| self.source_for(by));
        let damage = match (causing, direct) {
            (Some(causing), Some(direct)) => DamageSource {
                kind: DamageType::Explosion,
                causing: Some(causing),
                direct: Some(direct),
                position: None,
                knockback: None,
                direct_block: None,
            },
            (None, Some(direct)) => DamageSource::by(DamageType::Explosion, direct),
            _ => DamageSource::new(DamageType::Explosion),
        };
        let interaction = self.tnt_block_interaction();
        self.explode(
            Some(id),
            &damage,
            center,
            patch.radius.unwrap_or(step.radius),
            interaction,
            patch.fire.unwrap_or(step.fire),
            Some(id),
        );
    }

    pub(crate) fn cart_source_ignites(&self, source: &DamageSource) -> bool {
        let projectile_on_fire = source
            .direct
            .as_ref()
            .and_then(|direct| self.entities.store.get(direct.entity))
            .is_some_and(|entity| {
                matches!(
                    entity.state,
                    EntityState::Arrow(_) | EntityState::Thrown(_) | EntityState::Hurting(_)
                ) && fire::is_on_fire(&entity.base)
            });
        let projectile = source
            .direct
            .as_ref()
            .and_then(|direct| self.entities.store.get(direct.entity))
            .is_some_and(|entity| {
                matches!(
                    entity.state,
                    EntityState::Arrow(_) | EntityState::Thrown(_) | EntityState::Hurting(_)
                )
            });
        if projectile {
            return projectile_on_fire;
        }
        source.kind.is(DamageTag::IsFire)
            || matches!(
                source.kind,
                DamageType::Explosion | DamageType::PlayerExplosion
            )
    }

    pub(crate) fn tnt_cart_struck(&mut self, id: EntityId, source: &DamageSource) {
        if self.kind_of(id) != Some(Kind::TntMinecart) {
            return;
        }
        let arrow = source
            .direct
            .as_ref()
            .and_then(|direct| self.entities.store.get(direct.entity))
            .filter(|entity| {
                matches!(entity.state, EntityState::Arrow(_)) && fire::is_on_fire(&entity.base)
            })
            .map(|entity| entity.base.velocity.length_sqr());
        if let Some(speed_sqr) = arrow {
            self.cart_explode(id, speed_sqr);
        }
    }

    pub(crate) fn tnt_cart_breaks(
        &mut self,
        id: EntityId,
        source: &DamageSource,
        attacker: Option<EntityId>,
    ) -> bool {
        let Some(entity) = self.entities.store.get(id) else {
            return false;
        };
        if entity.base.kind != Kind::TntMinecart {
            return false;
        }
        let speed_sqr = entity.base.velocity.horizontal_distance_sqr();
        if !self.cart_source_ignites(source) && speed_sqr < minecart::TNT_COLLISION_SPEED_SQR {
            return false;
        }
        if !self.cart_is_primed(id) {
            self.cart_prime_fuse(id, attacker);
            let first = self.rng.random_range(0..20);
            let second = self.rng.random_range(0..20);
            if let Some(state) = self.entities.store.get_mut(id).and_then(minecart_state_mut) {
                state.fuse = first + second;
            }
        }
        if let Some(entity) = self.entities.store.get_mut(id) {
            rspigot_domain::entity::vehicle::set_damage(&mut entity.base, 0.0);
        }
        true
    }

    pub(crate) fn cart_activated(&mut self, id: EntityId) {
        if self.kind_of(id) == Some(Kind::TntMinecart) && !self.cart_is_primed(id) {
            self.cart_prime_fuse(id, None);
        }
    }

    pub(crate) fn furnace_cart_interact(
        &mut self,
        player: PlayerId,
        target: EntityId,
    ) -> Interaction {
        let Some(from) = self.players.get(&player).map(|p| p.position) else {
            return Interaction::Pass;
        };
        for hand in [Hand::Main, Hand::Off] {
            let Some(stack) = self.held(player, hand) else {
                continue;
            };
            if !item_tag(stack.item, minecart::FUEL_TAG) {
                continue;
            }
            let Some(entity) = self.entities.store.get_mut(target) else {
                break;
            };
            let at = entity.base.position;
            let added =
                minecart_state_mut(entity).is_some_and(|state| minecart::add_fuel(state, from, at));
            if added {
                self.consume_held(player, hand, 1);
            }
            break;
        }
        Interaction::Predicted
    }
}
