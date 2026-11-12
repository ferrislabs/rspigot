use rspigot_domain::EntityId;
use rspigot_domain::damage::DamageType;
use rspigot_domain::effect::{EffectDuration, EffectId, EffectKind};
use rspigot_domain::entity::Entity;
use rspigot_domain::entity::living;

use crate::health::DamageSource;
use crate::mob_life::type_in_tag;
use crate::plugins::events::{EntityPotionEffectCause, EntityRegainHealthRegainReason};
use crate::ports::ClientLink;
use crate::server::Server;

const INVERTED_HEAL_AND_HARM: &str = "minecraft:inverted_healing_and_harm";
const REGENERATION_HEAL: f32 = 1.0;
const POISON_FLOOR: f32 = 1.0;
const WITHER_DAMAGE: f32 = 1.0;
const HEAL_BASE: i32 = 4;
const HARM_BASE: i32 = 6;

impl<L: ClientLink> Server<L> {
    pub(crate) fn tick_mob_effects(&mut self, id: EntityId) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let Some(state) = entity.living() else {
            return;
        };
        if living::is_dead_or_dying(&entity.base, state) {
            return;
        }
        let age = entity.base.tick_count;
        let due: Vec<(EffectId, EffectKind, u8)> = state
            .effects
            .iter()
            .filter(|effect| effect.duration != EffectDuration::Ticks(0))
            .filter_map(|effect| {
                let kind = effect.effect.kind()?;
                let check = match effect.duration {
                    EffectDuration::Infinite => age,
                    EffectDuration::Ticks(ticks) => i32::try_from(ticks).unwrap_or(i32::MAX),
                };
                kind.should_tick(check, effect.amplifier).then_some((
                    effect.effect,
                    kind,
                    effect.amplifier,
                ))
            })
            .collect();
        for (effect, kind, amplifier) in due {
            if !self.mob_effect_tick(id, kind, amplifier) {
                self.remove_entity_effects(id, Some(effect), EntityPotionEffectCause::Expiration);
            }
        }
    }

    fn mob_effect_tick(&mut self, id: EntityId, kind: EffectKind, amplifier: u8) -> bool {
        let shift = u32::from(amplifier) & 31;
        match kind {
            EffectKind::Regeneration => {
                let hurt = self
                    .mob_health_of(id)
                    .is_some_and(|(health, max)| health < max);
                if hurt {
                    self.heal_mob(
                        None,
                        id,
                        REGENERATION_HEAL,
                        EntityRegainHealthRegainReason::MagicRegen,
                    );
                }
            }
            EffectKind::Poison => {
                if self
                    .mob_health_of(id)
                    .is_some_and(|(health, _)| health > POISON_FLOOR)
                {
                    self.hurt_mob(id, &DamageSource::new(DamageType::Magic), 1.0);
                }
            }
            EffectKind::Wither => {
                self.hurt_mob(id, &DamageSource::new(DamageType::Wither), WITHER_DAMAGE);
            }
            EffectKind::InstantHealth | EffectKind::InstantDamage => {
                let harm = kind == EffectKind::InstantDamage;
                if harm == self.inverted_heal_and_harm(id) {
                    let amount = HEAL_BASE.wrapping_shl(shift).max(0) as f32;
                    self.heal_mob(None, id, amount, EntityRegainHealthRegainReason::Magic);
                } else {
                    let amount = HARM_BASE.wrapping_shl(shift) as f32;
                    self.hurt_mob(id, &DamageSource::new(DamageType::Magic), amount);
                }
            }
            _ => {}
        }
        true
    }

    pub(crate) fn mob_health_of(&self, id: EntityId) -> Option<(f32, f32)> {
        let entity = self.entities.store.get(id)?;
        let mut state = entity.living()?.clone();
        Some((living::health(&entity.base), state.max_health() as f32))
    }

    pub(crate) fn inverted_heal_and_harm(&self, id: EntityId) -> bool {
        self.entities
            .store
            .get(id)
            .is_some_and(|entity: &Entity| type_in_tag(entity.base.kind, INVERTED_HEAL_AND_HARM))
    }
}
