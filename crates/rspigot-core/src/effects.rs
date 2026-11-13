use rspigot_domain::damage::DamageType;
use rspigot_domain::effect::{
    ApplyOutcome, AttributeState, EffectDuration, EffectId, EffectInstance, EffectKind,
    affected_attribute,
};
use rspigot_domain::player::PlayerId;

use crate::events::ClientEvent;
use crate::health::DamageSource;
use crate::plugins::events::{
    EntityExhaustionReason, EntityPotionEffectCause, EntityRegainHealthRegainReason,
};
use crate::plugins::{Holder, Potion};
use crate::ports::ClientLink;
use crate::server::{Audience, Server};

const HEAL_BASE: i32 = 4;
const HARM_BASE: i32 = 6;
const HUNGER_EXHAUSTION: f32 = 0.005;

const FLAG_EFFECTS: [&str; 2] = ["minecraft:invisibility", "minecraft:glowing"];

fn changes_flags(effects: &[EffectId]) -> bool {
    effects
        .iter()
        .any(|e| e.name().is_some_and(|name| FLAG_EFFECTS.contains(&name)))
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn apply_effect(
        &mut self,
        id: PlayerId,
        instance: EffectInstance,
        cause: Option<EntityPotionEffectCause>,
    ) -> bool {
        match cause {
            Some(cause) => self.potion(Holder::Player(id), Potion::Add(instance), cause),
            None => self.apply_effect_now(id, instance),
        }
    }

    pub(crate) fn apply_effect_now(&mut self, id: PlayerId, instance: EffectInstance) -> bool {
        let Some(player) = self.players.get_mut(&id) else {
            return false;
        };
        let outcome = player.effects.apply(instance);
        if outcome == ApplyOutcome::Unchanged {
            self.start_effect(id, instance);
            return false;
        }
        if let Some(current) = player.effects.get(instance.effect).copied() {
            player.link.send(ClientEvent::EffectApplied {
                entity: player.entity_id,
                effect: current,
                blend: outcome == ApplyOutcome::Added,
            });
        }
        self.sync_effect_state(id, &[instance.effect]);
        self.start_effect(id, instance);
        true
    }

    pub(crate) fn sync_attributes(&mut self, id: PlayerId, names: &[&'static str]) {
        let Some(player) = self.players.get(&id) else {
            return;
        };
        let entity = player.entity_id;
        let current = crate::health::attributes(player);
        let mut attributes: Vec<AttributeState> = Vec::new();
        for name in names {
            if !attributes.iter().any(|a| a.attribute == *name) {
                attributes.push(current.state(name));
            }
        }
        if attributes.is_empty() {
            return;
        }
        player.link.send(ClientEvent::Attributes {
            entity,
            attributes: attributes.clone(),
        });
        self.push(
            Audience::Trackers(entity),
            ClientEvent::Attributes { entity, attributes },
        );
    }

    pub(crate) fn start_effect(&mut self, id: PlayerId, instance: EffectInstance) {
        if instance.effect.kind() == Some(EffectKind::Absorption) {
            self.start_absorption(id, instance.amplifier);
        }
    }

    fn effect_tick(&mut self, id: PlayerId, kind: EffectKind, amplifier: u8) -> bool {
        let shift = u32::from(amplifier);
        match kind {
            EffectKind::Regeneration => {
                let hurt = self
                    .players
                    .get(&id)
                    .is_some_and(|p| p.vitals.health < crate::health::max_health(p));
                if hurt {
                    self.heal_player(id, 1.0, EntityRegainHealthRegainReason::MagicRegen);
                }
            }
            EffectKind::Poison => {
                if self.player_health(id).is_some_and(|health| health > 1.0) {
                    self.hurt(id, &DamageSource::new(DamageType::Magic), 1.0);
                }
            }
            EffectKind::Wither => {
                self.hurt(id, &DamageSource::new(DamageType::Wither), 1.0);
            }
            EffectKind::InstantHealth => {
                self.heal_player(
                    id,
                    HEAL_BASE.wrapping_shl(shift).max(0) as f32,
                    EntityRegainHealthRegainReason::Magic,
                );
            }
            EffectKind::InstantDamage => {
                let amount = HARM_BASE.wrapping_shl(shift) as f32;
                self.hurt(id, &DamageSource::new(DamageType::Magic), amount);
            }
            EffectKind::Hunger => {
                self.exhaust(
                    id,
                    HUNGER_EXHAUSTION * (f32::from(amplifier) + 1.0),
                    EntityExhaustionReason::HungerEffect,
                );
            }
            EffectKind::Saturation => {
                if let Some(old) = self.players.get(&id).map(|p| p.vitals.food.food) {
                    self.change_food(id, old + i32::from(amplifier) + 1, Some(old));
                }
            }
            EffectKind::BadOmen => return self.bad_omen_tick(id, amplifier),
            EffectKind::RaidOmen => return self.raid_omen_tick(id, amplifier),
            EffectKind::Absorption => {
                return self.player_absorption(id).is_some_and(|value| value > 0.0);
            }
            _ => {}
        }
        true
    }

    pub(crate) fn remove_effects(
        &mut self,
        id: PlayerId,
        effect: Option<EffectId>,
        cause: EntityPotionEffectCause,
    ) -> bool {
        let Some(player) = self.players.get(&id) else {
            return false;
        };
        let targets: Vec<Potion> = match effect {
            Some(effect) => player
                .effects
                .get(effect)
                .copied()
                .map(Potion::Remove)
                .into_iter()
                .collect(),
            None => player.effects.iter().copied().map(Potion::Clear).collect(),
        };
        let found = !targets.is_empty();
        for potion in targets {
            self.potion(Holder::Player(id), potion, cause);
        }
        found
    }

    pub(crate) fn remove_effect_now(&mut self, id: PlayerId, effect: EffectId) -> bool {
        let Some(player) = self.players.get_mut(&id) else {
            return false;
        };
        let removed: Vec<EffectId> = player
            .effects
            .remove(effect)
            .map(|e| e.effect)
            .into_iter()
            .collect();
        if removed.is_empty() {
            return false;
        }
        for effect in &removed {
            player.link.send(ClientEvent::EffectRemoved {
                entity: player.entity_id,
                effect: *effect,
            });
        }
        self.sync_effect_state(id, &removed);
        true
    }

    pub(crate) fn tick_effects(&mut self) {
        let ids: Vec<PlayerId> = self.players.keys().copied().collect();
        for id in ids {
            let Some(player) = self.players.get(&id) else {
                continue;
            };
            if player.vitals.is_dead() {
                continue;
            }
            let age = player.vitals.tick_count as i32;
            let due: Vec<(EffectId, EffectKind, u8)> = player
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
                if !self.effect_tick(id, kind, amplifier) {
                    self.remove_effects(id, Some(effect), EntityPotionEffectCause::Expiration);
                }
            }
        }
        let mut expired = Vec::new();
        for player in self.players.values_mut() {
            if player.vitals.is_dead() {
                continue;
            }
            let spent = player.effects.countdown();
            if !spent.is_empty() {
                expired.push((player.id, spent));
            }
        }
        for (id, spent) in expired {
            for effect in spent {
                self.potion(
                    Holder::Player(id),
                    Potion::Expire(effect),
                    EntityPotionEffectCause::Expiration,
                );
            }
        }
    }

    fn sync_effect_state(&mut self, id: PlayerId, changed: &[EffectId]) {
        let Some(player) = self.players.get(&id) else {
            return;
        };
        let entity = player.entity_id;
        let names: Vec<&'static str> = changed
            .iter()
            .filter_map(|e| affected_attribute(*e))
            .collect();
        self.sync_attributes(id, &names);
        self.clamp_vitals(id);
        if changes_flags(changed) {
            self.broadcast_metadata(id);
            if let Some(player) = self.players.get(&id) {
                player.link.send(ClientEvent::EntityMetadata {
                    entity,
                    entries: Self::metadata(player),
                });
            }
        }
    }
}
