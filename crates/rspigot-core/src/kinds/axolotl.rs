use rspigot_ai::{AxolotlAction, MemoryModuleType, MemoryValue};
use rspigot_domain::EntityId;
use rspigot_domain::damage::DamageType;
use rspigot_domain::effect::{EffectDuration, EffectId, EffectInstance};
use rspigot_domain::entity::living;
use rspigot_domain::entity::species::axolotl;
use rspigot_domain::entity::{Entity, EntityState};
use rspigot_domain::random::{LegacyRandom, RandomSource};

use crate::health::DamageSource;
use crate::plugins::events::EntityPotionEffectCause;
use crate::ports::ClientLink;
use crate::server::Server;

const REGENERATION: &str = "minecraft:regeneration";
const MINING_FATIGUE: &str = "minecraft:mining_fatigue";
const SUPPORT_TICKS_MAX: u32 = 2400;
const SUPPORT_TICKS_STEP: u32 = 100;
const SUPPORT_THRESHOLD: u32 = 2399;

fn regeneration() -> Option<EffectId> {
    EffectId::from_name(REGENERATION)
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn axolotl_action(&mut self, id: EntityId, action: AxolotlAction) {
        match action {
            AxolotlAction::PlayDeadStarted => {
                if let Some(effect) = regeneration() {
                    self.add_effect_to(
                        id,
                        EffectInstance::new(
                            effect,
                            EffectDuration::Ticks(axolotl::PLAY_DEAD_TICKS as u32),
                            0,
                            true,
                        ),
                        EntityPotionEffectCause::Unknown,
                    );
                }
            }
            AxolotlAction::TargetErased(target) => self.axolotl_target_erased(id, target),
        }
    }

    fn axolotl_target_erased(&mut self, id: EntityId, target: EntityId) {
        let Some(axolotl_at) = self
            .entities
            .store
            .get(id)
            .map(|entity| entity.base.bounding_box())
        else {
            return;
        };
        let Some(victim) = self.entities.store.get(target) else {
            return;
        };
        let dead = victim
            .living()
            .is_some_and(|state| living::is_dead_or_dying(&victim.base, state));
        if !dead {
            return;
        }
        let killer = victim
            .living()
            .and_then(|state| state.damage.last_source_entity);
        let Some(killer) = killer else {
            return;
        };
        let Some(player) = self
            .players
            .values()
            .find(|player| player.entity_id == killer)
            .map(|player| (player.id, player.entity_id, player.position))
        else {
            return;
        };
        let reach = axolotl_at.inflate(axolotl::SUPPORT_RANGE);
        let body = rspigot_domain::entity::Aabb::of_size(player.2, 0.6, 1.8);
        if reach.intersects(&body) {
            self.axolotl_supports(player.0, id);
        }
    }

    fn axolotl_supports(&mut self, player: rspigot_domain::player::PlayerId, source: EntityId) {
        let Some(effect) = regeneration() else {
            return;
        };
        let current = self
            .players
            .get(&player)
            .and_then(|player| player.effects.get(effect))
            .map(|instance| match instance.duration {
                EffectDuration::Ticks(ticks) => ticks,
                EffectDuration::Infinite => u32::MAX,
            });
        let ends_within = current.is_none_or(|ticks| ticks <= SUPPORT_THRESHOLD);
        if ends_within {
            let base = current.unwrap_or(0);
            let ticks = (base + SUPPORT_TICKS_STEP).min(SUPPORT_TICKS_MAX);
            let entity = self
                .players
                .get(&player)
                .map(|player| player.entity_id)
                .unwrap_or(source);
            self.add_effect_to(
                entity,
                EffectInstance::new(effect, EffectDuration::Ticks(ticks), 0, true),
                EntityPotionEffectCause::Axolotl,
            );
        }
        if let Some(fatigue) = EffectId::from_name(MINING_FATIGUE) {
            self.remove_effect_now(player, fatigue);
        }
    }

    pub(crate) fn axolotl_hurt_amount(
        &mut self,
        id: EntityId,
        source: &DamageSource,
        amount: f32,
    ) -> Option<f32> {
        let Some(entity) = self.entities.store.get_mut(id) else {
            return Some(amount);
        };
        let health = living::health(&entity.base);
        let Some(max_health) = entity
            .living_mut()
            .map(|state| state.attribute(living::MAX_HEALTH) as f32)
        else {
            return Some(amount);
        };
        let in_water = entity.base.fluid.in_water;
        let no_ai = living::is_no_ai(&entity.base);
        let playing_dead = axolotl::is_playing_dead(&entity.base);
        let caused = source.causing.is_some() || source.direct.is_some();
        if no_ai {
            return Some(amount);
        }
        let mut random = self.mob_random(id);
        let mut play_dead = false;
        if random.next_int(3) == 0 {
            let roll = random.next_int(3) as f32;
            play_dead = (roll < amount || health / max_health < 0.5)
                && amount < health
                && in_water
                && caused
                && !playing_dead;
        }
        self.store_mob_random(id, random);
        if play_dead
            && let Some(brain) = self
                .entities
                .mobs
                .entries
                .get_mut(&id)
                .and_then(|entry| entry.ai.behavior_brain_mut())
        {
            brain.memories_mut().set(
                MemoryModuleType::PlayDeadTicks,
                MemoryValue::Int(axolotl::PLAY_DEAD_TICKS),
            );
        }
        Some(amount)
    }

    pub(crate) fn axolotl_post_brain(&mut self, id: EntityId) {
        let ticks = self
            .entities
            .mobs
            .entries
            .get(&id)
            .and_then(|entry| entry.ai.behavior_brain())
            .and_then(|brain| brain.memories().get(MemoryModuleType::PlayDeadTicks))
            .and_then(MemoryValue::as_int)
            .unwrap_or(0);
        let Some(entity) = self.entities.store.get_mut(id) else {
            return;
        };
        if living::is_no_ai(&entity.base) {
            return;
        }
        let playing = ticks > 0;
        if axolotl::is_playing_dead(&entity.base) != playing {
            axolotl::set_playing_dead(&mut entity.base, playing);
        }
    }

    pub(crate) fn water_creature_air(&mut self, id: EntityId, air_before: i32) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        if living::is_no_ai(&entity.base) {
            return;
        }
        let kind = entity.base.kind;
        let wet = entity.base.fluid.in_water
            || (kind == rspigot_entities::EntityKind::Axolotl && self.is_in_rain(&entity.base));
        let alive = !entity.base.is_removed()
            && entity
                .living()
                .is_some_and(|state| !living::is_dead_or_dying(&entity.base, state));
        let Some(entity) = self.entities.store.get_mut(id) else {
            return;
        };
        let max = entity.base.max_air_ticks;
        if alive && !wet {
            entity.base.set_air_supply(air_before - 1);
            if entity.base.air_supply() <= DROWNING_AIR {
                entity.base.set_air_supply(0);
                self.hurt_mob(
                    id,
                    &DamageSource::new(DamageType::DryOut),
                    axolotl::DRY_OUT_DAMAGE,
                );
            }
        } else {
            entity.base.set_air_supply(max);
        }
    }

    pub(crate) fn axolotl_offspring(
        &mut self,
        child: &mut Entity,
        parent: EntityId,
        partner: EntityId,
        random: &mut LegacyRandom,
    ) {
        let own = self
            .entities
            .store
            .get(parent)
            .map(|entity| axolotl::variant(&entity.base));
        let other = self
            .entities
            .store
            .get(partner)
            .map(|entity| axolotl::variant(&entity.base));
        let (Some(own), Some(other)) = (own, other) else {
            return;
        };
        let chosen = axolotl::offspring_variant(random, own, other);
        axolotl::set_variant(&mut child.base, chosen);
        if let EntityState::Living(state) = &mut child.state
            && let Some(mob) = state.mob.as_mut()
        {
            mob.persistence_required = true;
        }
    }
}

const DROWNING_AIR: i32 = -20;
