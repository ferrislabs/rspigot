use rand::Rng;
use rspigot_blocks::StateFlags;
use rspigot_domain::damage::DamageType;
use rspigot_domain::effect::{EffectDuration, EffectId, EffectInstance, EffectKind};
use rspigot_domain::entity::Aabb;
use rspigot_domain::game_event::GameEvent;
use rspigot_domain::item::ItemStack;
use rspigot_domain::player::PlayerId;
use rspigot_domain::potion::{self, PotionContents};
use rspigot_domain::spawn::rules;
use rspigot_domain::stack_components::{self, StackConsumable, StackConsumeEffect, StackMobEffect};
use rspigot_domain::{BlockPos, Vec3};
use rspigot_entities::SoundSource;
use rspigot_items::ConsumeAnimation;
use rspigot_nbt::Tag;

use crate::events::ClientEvent;
use crate::game_events::GameEventContext;
use crate::health::DamageSource;
use crate::plugins::events::{
    EntityPotionEffectCause, EntityRegainHealthRegainReason, PlayerTeleportCause,
};
use crate::ports::ClientLink;
use crate::server::{Audience, Server};

const STEW_EFFECTS: &str = "minecraft:suspicious_stew_effects";
const TELEPORT_ATTEMPTS: u32 = 16;
const PLAYER_WIDTH: f32 = 0.6;
const CHORUS_TELEPORT_SOUND: &str = "minecraft:item.chorus_fruit.teleport";

pub(crate) fn passes(probability: f32, roll: f32) -> bool {
    roll < probability
}

pub(crate) fn instance_of(spec: &StackMobEffect) -> Option<EffectInstance> {
    let effect = EffectId::from_name(&spec.effect)?;
    let duration = match spec.duration {
        -1 => EffectDuration::Infinite,
        ticks => EffectDuration::Ticks(u32::try_from(ticks).unwrap_or(0)),
    };
    let mut instance = EffectInstance::new(effect, duration, spec.amplifier, true);
    instance.show_icon = spec.show_icon;
    Some(instance)
}

pub(crate) fn stew_effects(stack: &ItemStack) -> Vec<EffectInstance> {
    let Some(list) = stack.components.get_list(STEW_EFFECTS) else {
        return Vec::new();
    };
    list.iter()
        .filter_map(Tag::as_compound)
        .filter_map(|entry| {
            let effect = EffectId::from_name(entry.get_str("id")?)?;
            let duration = u32::try_from(entry.get_i32("duration")?).ok()?;
            Some(EffectInstance::new(
                effect,
                EffectDuration::Ticks(duration),
                0,
                true,
            ))
        })
        .collect()
}

pub(crate) const fn game_event_of(animation: ConsumeAnimation) -> GameEvent {
    match animation {
        ConsumeAnimation::Drink => GameEvent::Drink,
        ConsumeAnimation::Eat => GameEvent::Eat,
    }
}

pub(crate) fn teleport_target(
    origin: Vec3,
    diameter: f32,
    roll: [f64; 3],
    min_y: i32,
    top: i32,
) -> Vec3 {
    let diameter = f64::from(diameter);
    let spread = diameter as i32;
    let offset = (roll[1] * f64::from(spread.max(1))) as i32 - spread / 2;
    let y = (origin.y + f64::from(offset)).clamp(f64::from(min_y), f64::from(top));
    Vec3::new(
        origin.x + (roll[0] - 0.5) * diameter,
        y,
        origin.z + (roll[2] - 0.5) * diameter,
    )
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn apply_consumable(
        &mut self,
        id: PlayerId,
        stack: &ItemStack,
        consumable: &StackConsumable,
    ) {
        let Some(kind) = stack.item.kind() else {
            return;
        };
        if kind.potion_contents().is_some() {
            self.drink_potion(id, stack);
        }
        if kind.suspicious_stew_effects().is_some() {
            for effect in stew_effects(stack) {
                self.add_player_effect(id, effect, EntityPotionEffectCause::Food);
            }
        }
        for effect in &consumable.on_consume_effects {
            self.consume_effect(id, effect);
        }
    }

    pub(crate) fn play_named_sound(
        &mut self,
        except: Option<PlayerId>,
        position: Vec3,
        sound: &str,
        source: SoundSource,
        volume: f32,
        pitch: f32,
    ) {
        if let Some(known) = rspigot_data::registries::static_entries("minecraft:sound_event")
            .and_then(|entries| entries.iter().copied().find(|entry| *entry == sound))
        {
            self.play_sound(except, position, known, source, volume, pitch);
            return;
        }
        let seed = self.rng.random::<i64>();
        self.push(
            Audience::Nearby {
                position,
                range: crate::sound::sound_range(volume),
                except,
            },
            ClientEvent::NamedSound {
                sound: sound.to_owned(),
                source,
                position,
                volume,
                pitch,
                seed,
            },
        );
    }

    fn add_player_effect(
        &mut self,
        id: PlayerId,
        effect: EffectInstance,
        cause: EntityPotionEffectCause,
    ) {
        self.apply_effect(id, effect, Some(cause));
    }

    fn drink_potion(&mut self, id: PlayerId, stack: &ItemStack) {
        let scale = potion::duration_scale(stack);
        let effects = PotionContents::of(stack).effects();
        for effect in effects {
            match effect.effect.kind() {
                Some(EffectKind::InstantHealth) => self.heal_player(
                    id,
                    potion::heal_amount(effect.amplifier, 1.0),
                    EntityRegainHealthRegainReason::Magic,
                ),
                Some(EffectKind::InstantDamage) => {
                    self.hurt(
                        id,
                        &DamageSource::new(DamageType::Magic),
                        potion::harm_amount(effect.amplifier, 1.0),
                    );
                }
                _ => {
                    let scaled = potion::with_scaled_duration(&effect, scale);
                    self.add_player_effect(id, scaled, EntityPotionEffectCause::PotionDrink);
                }
            }
        }
    }

    fn consume_effect(&mut self, id: PlayerId, effect: &StackConsumeEffect) {
        match effect {
            StackConsumeEffect::ApplyEffects {
                effects,
                probability,
            } => {
                let roll = self.rng.random::<f32>();
                if !passes(*probability, roll) {
                    return;
                }
                for spec in effects {
                    if let Some(instance) = instance_of(spec) {
                        self.add_player_effect(id, instance, EntityPotionEffectCause::Food);
                    }
                }
            }
            StackConsumeEffect::RemoveEffects(names) => {
                for effect in names.iter().filter_map(|name| EffectId::from_name(name)) {
                    self.remove_effects(id, Some(effect), EntityPotionEffectCause::Food);
                }
            }
            StackConsumeEffect::ClearAllEffects => {
                self.remove_effects(id, None, EntityPotionEffectCause::Milk);
            }
            StackConsumeEffect::TeleportRandomly { diameter } => {
                self.teleport_randomly(id, *diameter);
            }
            StackConsumeEffect::PlaySound(sound) => {
                let Some(position) = self.players.get(&id).map(|player| player.position) else {
                    return;
                };
                self.play_named_sound(None, position, sound, SoundSource::Players, 1.0, 1.0);
            }
        }
    }

    fn teleport_randomly(&mut self, id: PlayerId, diameter: f32) {
        let Some(player) = self.players.get(&id) else {
            return;
        };
        let origin = player.position;
        let entity = player.entity_id;
        let height = player.height() as f32;
        let min_y = self.world.dimension.height.min_y;
        let top = min_y + self.world.dimension.height.height as i32 - 1;
        for _ in 0..TELEPORT_ATTEMPTS {
            let roll = [
                self.rng.random::<f64>(),
                self.rng.random::<f64>(),
                self.rng.random::<f64>(),
            ];
            let target = teleport_target(origin, diameter, roll, min_y, top);
            let Some(landing) = self.landing_for(target, height) else {
                continue;
            };
            self.game_event(
                GameEvent::Teleport,
                origin,
                GameEventContext::of(Some(entity)),
            );
            self.play_sound(
                None,
                origin,
                CHORUS_TELEPORT_SOUND,
                SoundSource::Players,
                1.0,
                1.0,
            );
            self.ask_teleport(id, landing, None, PlayerTeleportCause::ChorusFruit, None);
            self.play_sound(
                None,
                landing,
                CHORUS_TELEPORT_SOUND,
                SoundSource::Players,
                1.0,
                1.0,
            );
            return;
        }
    }

    fn landing_for(&self, target: Vec3, height: f32) -> Option<Vec3> {
        let min_y = self.world.dimension.height.min_y;
        let mut pos = target.block();
        if !self.world.chunks.is_loaded(pos.chunk()) {
            return None;
        }
        let mut y = target.y;
        let mut ground = false;
        while !ground && pos.y > min_y {
            let below = BlockPos::new(pos.x, pos.y - 1, pos.z);
            let solid = self
                .world
                .chunks
                .block(below)
                .is_some_and(|state| state.flags().contains(StateFlags::BLOCKS_MOTION));
            if solid {
                ground = true;
            } else {
                y -= 1.0;
                pos = below;
            }
        }
        if !ground {
            return None;
        }
        let landing = Vec3::new(target.x, y, target.z);
        let area = Aabb::of_size(landing, PLAYER_WIDTH, height);
        (rules::no_collision(self, &area) && !rules::contains_any_liquid(self, &area))
            .then_some(landing)
    }
}

pub(crate) fn consumable_of(stack: &ItemStack) -> Option<StackConsumable> {
    stack_components::consumable(stack)
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU8;

    use rspigot_items::ItemKind;
    use rspigot_nbt::Compound;

    use super::*;

    #[test]
    fn a_roll_below_the_probability_passes() {
        assert!(passes(0.3, 0.29));
        assert!(!passes(0.3, 0.3));
        assert!(!passes(0.3, 0.99));
        assert!(passes(1.0, 0.999_999));
        assert!(!passes(0.0, 0.0));
    }

    #[test]
    fn an_effect_spec_becomes_an_instance_with_its_icon_flag() {
        let spec = StackMobEffect {
            effect: "minecraft:poison".to_owned(),
            amplifier: 1,
            duration: 1200,
            show_icon: false,
        };
        let instance = instance_of(&spec).unwrap();
        assert_eq!(instance.amplifier, 1);
        assert_eq!(instance.duration, EffectDuration::Ticks(1200));
        assert!(!instance.show_icon);
        assert!(instance.visible);
        let forever = StackMobEffect {
            duration: -1,
            ..spec
        };
        assert_eq!(
            instance_of(&forever).unwrap().duration,
            EffectDuration::Infinite
        );
    }

    #[test]
    fn stew_entries_become_plain_effects() {
        let mut stew = ItemStack::new(ItemKind::SuspiciousStew.id(), NonZeroU8::MIN);
        assert!(stew_effects(&stew).is_empty());
        stew.components.insert(
            STEW_EFFECTS,
            Tag::List(vec![
                Tag::Compound(
                    Compound::new()
                        .with("id", "minecraft:blindness")
                        .with("duration", 160),
                ),
                Tag::Compound(Compound::new().with("id", "minecraft:nothing")),
            ]),
        );
        let effects = stew_effects(&stew);
        assert_eq!(effects.len(), 1);
        assert_eq!(effects[0].duration, EffectDuration::Ticks(160));
        assert_eq!(effects[0].amplifier, 0);
    }

    #[test]
    fn the_teleport_target_stays_within_half_the_diameter_and_the_world() {
        let origin = Vec3::new(10.0, 64.0, -4.0);
        let low = teleport_target(origin, 16.0, [0.0, 0.0, 0.0], -64, 319);
        assert_eq!((low.x, low.y, low.z), (2.0, 56.0, -12.0));
        let high = teleport_target(origin, 16.0, [0.999, 0.999, 0.999], -64, 319);
        assert!(high.x < 18.0 && high.y == 71.0 && high.z < 4.0);
        let clamped = teleport_target(Vec3::new(0.0, 318.0, 0.0), 16.0, [0.5, 0.99, 0.5], -64, 319);
        assert_eq!(clamped.y, 319.0);
    }

    #[test]
    fn drinking_is_a_drink_game_event_and_eating_an_eat_one() {
        assert_eq!(game_event_of(ConsumeAnimation::Drink), GameEvent::Drink);
        assert_eq!(game_event_of(ConsumeAnimation::Eat), GameEvent::Eat);
    }
}
