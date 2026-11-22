use rand::Rng;
use rspigot_blocks::{BlockKind, StateFlags};
use rspigot_domain::damage::DamageType;
use rspigot_domain::effect::{EffectDuration, EffectId, EffectInstance, EffectKind};
use rspigot_domain::entity::living::{self, DEATH_DURATION, MAX_ABSORPTION};
use rspigot_domain::entity::species::cube;
use rspigot_domain::entity::{Entity, EntityState, save};
use rspigot_domain::random::LegacyRandom;
use rspigot_domain::{BlockPos, EntityId, Vec3};
use rspigot_entities::EntityKind as Kind;
use rspigot_nbt::Compound;

use crate::health::DamageSource;
use crate::mob_life::type_in_tag;
use crate::plugins::Birth;
use crate::plugins::events::{
    CreatureSpawnReason, EntityPotionEffectCause, EntityRegainHealthRegainReason,
};
use crate::ports::ClientLink;
use crate::server::Server;

const INVERTED_HEAL_AND_HARM: &str = "minecraft:inverted_healing_and_harm";
const REGENERATION_HEAL: f32 = 1.0;
const POISON_FLOOR: f32 = 1.0;
const WITHER_DAMAGE: f32 = 1.0;
const HEAL_BASE: i32 = 4;
const HARM_BASE: i32 = 6;
const ABSORPTION_PER_LEVEL: f32 = 4.0;
const WIND_BURST_SOUND: &str = "minecraft:entity.wind_charge.wind_burst";
const SLIME_NEIGHBOURHOOD: f64 = 2.0;
const COBWEB_REACH: i32 = 1;
const FULL_TURN: f32 = 360.0;
const SILVERFISH_PUSH: f64 = 0.2;

fn kinds_of<'a>(effects: impl Iterator<Item = &'a EffectInstance>) -> Vec<EffectKind> {
    effects.filter_map(|effect| effect.effect.kind()).collect()
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn tick_mob_effects(&mut self, id: EntityId) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let Some(state) = entity.living() else {
            return;
        };
        if living::is_dead_or_dying(&entity.base, state) {
            if state.death_time >= DEATH_DURATION && !entity.base.is_removed() {
                let kinds = kinds_of(state.effects.iter());
                self.mob_removed_by_death(id, &kinds);
            }
            return;
        }
        let keeps_air = state
            .effects
            .iter()
            .any(|effect| effect.effect.kind().is_some_and(EffectKind::keeps_air));
        if keeps_air && let Some(entity) = self.entities.store.get_mut(id) {
            let max = entity.base.max_air_ticks;
            entity.base.set_air_supply(max);
        }
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let Some(state) = entity.living() else {
            return;
        };
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
            EffectKind::Absorption => {
                return self
                    .entities
                    .store
                    .get(id)
                    .and_then(|entity| entity.living())
                    .is_some_and(|state| state.absorption > 0.0);
            }
            _ => {}
        }
        true
    }

    fn mob_removed_by_death(&mut self, id: EntityId, kinds: &[EffectKind]) {
        if let Some(state) = self
            .entities
            .store
            .get_mut(id)
            .and_then(|entity| entity.living_mut())
        {
            for kind in kinds {
                if kind.slimes_on_death().is_some()
                    || kind.cobwebs_on_death().is_some()
                    || kind.burst_on_death().is_some()
                {
                    state.effects.remove(kind.id());
                }
            }
        }
        for kind in kinds {
            if let Some((count, size)) = kind.slimes_on_death() {
                self.ooze_slimes(id, count, size);
            }
            if let Some((min, max)) = kind.cobwebs_on_death() {
                self.weave_cobwebs(id, min, max);
            }
            if let Some(radius) = kind.burst_on_death() {
                self.burst_from(id, radius);
            }
        }
    }

    pub(crate) fn mob_hurt_reactions(&mut self, id: EntityId) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let Some(state) = entity.living() else {
            return;
        };
        let kinds = kinds_of(state.effects.iter());
        let center = Self::body_center(&entity.base);
        self.release_silverfish(&kinds, center);
    }

    pub(crate) fn player_hurt_reactions(&mut self, id: rspigot_domain::player::PlayerId) {
        let Some(player) = self.players.get(&id) else {
            return;
        };
        let kinds = kinds_of(player.effects.iter());
        let center = player.position + Vec3::new(0.0, player.height() / 2.0, 0.0);
        self.release_silverfish(&kinds, center);
    }

    fn body_center(base: &rspigot_domain::entity::EntityBase) -> Vec3 {
        base.position + Vec3::new(0.0, f64::from(base.dimensions.height) / 2.0, 0.0)
    }

    fn release_silverfish(&mut self, kinds: &[EffectKind], center: Vec3) {
        for kind in kinds {
            let Some((chance, min, max)) = kind.silverfish_on_hurt() else {
                continue;
            };
            if self.rng.random::<f32>() > chance {
                continue;
            }
            for _ in 0..self.rng.random_range(min..=max) {
                self.spawn_silverfish(center);
            }
        }
    }

    fn spawn_silverfish(&mut self, at: Vec3) {
        let id = self.allocate_entity_id();
        let uuid = self.random_uuid();
        let compound = Compound::new().with("id", Kind::Silverfish.name());
        let Ok(mut entity) = save::load_entity(&compound, id, uuid) else {
            return;
        };
        let yaw = self.rng.random::<f32>() * FULL_TURN;
        entity.base.snap_to(at, yaw, 0.0);
        let angle = f64::from(yaw).to_radians();
        entity.base.velocity = Vec3::new(
            -angle.sin() * SILVERFISH_PUSH,
            0.0,
            angle.cos() * SILVERFISH_PUSH,
        );
        let own = LegacyRandom::new(self.rng.random());
        self.spawn_entity(
            entity,
            Birth::of(CreatureSpawnReason::PotionEffect).with_random(own),
        );
    }

    fn ooze_slimes(&mut self, id: EntityId, count: i32, size: i32) {
        let Some(base) = self
            .entities
            .store
            .get(id)
            .map(|entity| entity.base.clone())
        else {
            return;
        };
        let area = base.bounding_box().inflate(SLIME_NEIGHBOURHOOD);
        let nearby = self
            .entities
            .store
            .iter()
            .filter(|other| {
                other.base.kind == Kind::Slime
                    && !other.base.is_removed()
                    && other.base.bounding_box().intersects(&area)
            })
            .count() as i32;
        let cramming = self.config.rules.max_entity_cramming;
        let wanted = if cramming < 1 {
            count
        } else {
            (cramming - nearby).clamp(0, count)
        };
        for _ in 0..wanted {
            let mut slime = self.new_entity_base(Kind::Slime);
            let mut state = living::LivingState::new(&mut slime, base.yaw);
            cube::set_size(&mut slime, &mut state, size, true);
            let dx = (self.rng.random::<f32>() - 0.5) * base.dimensions.width;
            let dz = (self.rng.random::<f32>() - 0.5) * base.dimensions.width;
            let yaw = self.rng.random::<f32>() * FULL_TURN;
            slime.snap_to(
                base.position + Vec3::new(f64::from(dx), 0.5, f64::from(dz)),
                yaw,
                0.0,
            );
            let entity = Entity::new(slime, EntityState::Living(Box::new(state)));
            let own = LegacyRandom::new(self.rng.random());
            self.spawn_entity(
                entity,
                Birth::of(CreatureSpawnReason::PotionEffect).with_random(own),
            );
        }
    }

    fn weave_cobwebs(&mut self, id: EntityId, min: i32, max: i32) {
        let Some(origin) = self
            .entities
            .store
            .get(id)
            .map(|entity| entity.base.position.block())
        else {
            return;
        };
        for _ in 0..self.rng.random_range(min..=max) {
            let pos = BlockPos::new(
                origin.x + self.rng.random_range(-COBWEB_REACH..=COBWEB_REACH),
                origin.y + self.rng.random_range(-COBWEB_REACH..=COBWEB_REACH),
                origin.z + self.rng.random_range(-COBWEB_REACH..=COBWEB_REACH),
            );
            let Some(state) = self.world.chunks.block(pos) else {
                continue;
            };
            let free = state.is_air()
                || (state.flags().contains(StateFlags::REPLACEABLE) && !state.flags().has_fluid());
            if free {
                self.set_block(pos, BlockKind::Cobweb.default_state());
            }
        }
    }

    fn burst_from(&mut self, id: EntityId, radius: f32) {
        let Some(center) = self
            .entities
            .store
            .get(id)
            .map(|entity| Self::body_center(&entity.base))
        else {
            return;
        };
        self.wind_burst(Some(id), center, radius, 1.0, WIND_BURST_SOUND);
    }

    pub(crate) fn mob_effect_started(&mut self, id: EntityId, effect: EffectInstance) {
        if effect.effect.kind() != Some(EffectKind::Absorption) {
            return;
        }
        let Some(state) = self
            .entities
            .store
            .get_mut(id)
            .and_then(|entity| entity.living_mut())
        else {
            return;
        };
        let max = state.attribute(MAX_ABSORPTION) as f32;
        let wanted = ABSORPTION_PER_LEVEL * (f32::from(effect.amplifier) + 1.0);
        state.absorption = state.absorption.max(wanted).clamp(0.0, max.max(0.0));
    }

    pub(crate) fn mob_effect_ended(&mut self, id: EntityId) {
        let Some(state) = self
            .entities
            .store
            .get_mut(id)
            .and_then(|entity| entity.living_mut())
        else {
            return;
        };
        let max = state.attribute(MAX_ABSORPTION) as f32;
        state.absorption = state.absorption.clamp(0.0, max.max(0.0));
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
