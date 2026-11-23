use rspigot_domain::EntityId;
use rspigot_domain::damage::DamageTag;
use rspigot_domain::effect::EffectId;
use rspigot_domain::entity::living::{self, slot_index};
use rspigot_domain::item::ItemStack;
use rspigot_domain::player::PlayerId;
use rspigot_domain::stack_components::{self, StackConsumeEffect};
use rspigot_items::{EquipmentSlot, ItemKind};

use crate::bucket::slot_of;
use crate::events::{ClientEvent, EntityStatus, Hand};
use crate::health::DamageSource;
use crate::plugins::events::{EntityPotionEffectCause, EntityResurrect};
use crate::plugins::steps::{Holder, ResurrectStep};
use crate::plugins::{Continuation, EventKind, EventPayload};
use crate::ports::ClientLink;
use crate::server::{Audience, Server};

const TOTEM_STATUS: i8 = 35;
const HEALTH_AFTER: f32 = 1.0;
const HANDS: [Hand; 2] = [Hand::Main, Hand::Off];

fn is_totem(stack: &ItemStack) -> bool {
    stack_components::death_effects(stack).is_some()
}

fn mob_slot(hand: Hand) -> EquipmentSlot {
    match hand {
        Hand::Main => EquipmentSlot::Mainhand,
        Hand::Off => EquipmentSlot::Offhand,
    }
}

impl<L: ClientLink> Server<L> {
    fn totem_of(&self, holder: Holder) -> Option<(Hand, ItemStack)> {
        HANDS.into_iter().find_map(|hand| {
            let stack = match holder {
                Holder::Player(id) => self.held(id, hand),
                Holder::Mob(id) => self
                    .entities
                    .store
                    .get(id)?
                    .living()?
                    .equipment(mob_slot(hand))
                    .cloned(),
            }?;
            is_totem(&stack).then_some((hand, stack))
        })
    }

    fn resurrectee_entity(&self, holder: Holder) -> Option<EntityId> {
        match holder {
            Holder::Player(id) => self.players.get(&id).map(|player| player.entity_id),
            Holder::Mob(id) => Some(id),
        }
    }

    pub(crate) fn ask_resurrect(
        &mut self,
        holder: Holder,
        source: &DamageSource,
        full: bool,
    ) -> bool {
        if source.kind.is(DamageTag::BypassesInvulnerability) {
            return false;
        }
        let totem = self.totem_of(holder);
        let heard = self.listened(EventKind::EntityResurrect);
        if totem.is_none() && !heard {
            return false;
        }
        let Some(entity) = self.resurrectee_entity(holder) else {
            return false;
        };
        let hand = totem.as_ref().map(|(hand, _)| *hand);
        let step = ResurrectStep {
            holder,
            hand,
            totem: totem.map(|(_, stack)| stack),
            source: source.clone(),
            full,
        };
        let lane = self.lane_for(None, entity);
        let then = Continuation::Resurrect(Box::new(step));
        let describe = move |server: &Self| {
            let view = server.subject_view(entity)?;
            Some(EventPayload::EntityResurrect(EntityResurrect {
                entity_type: view.kind.clone(),
                entity: view,
                hand: hand.map(slot_of),
            }))
        };
        if hand.is_some() {
            self.fire(lane, then, describe);
        } else {
            self.fire_refused(lane, then, describe);
        }
        true
    }

    pub(crate) fn finish_resurrect(&mut self, step: ResurrectStep, cancelled: bool) {
        let ResurrectStep {
            holder,
            hand,
            totem,
            source,
            full,
        } = step;
        if cancelled {
            match holder {
                Holder::Player(id) => self.finish_player_death(id, &source, full),
                Holder::Mob(id) => self.resume_mob_death(id, &source, full),
            }
            return;
        }
        if let (Some(hand), Some(totem)) = (hand, totem.as_ref()) {
            self.use_up_totem(holder, hand, totem);
        }
        self.restore_after_totem(holder, totem.as_ref());
        let Some(entity) = self.resurrectee_entity(holder) else {
            return;
        };
        let event = ClientEvent::EntityStatus {
            entity,
            status: EntityStatus::Event(TOTEM_STATUS),
        };
        if let Holder::Player(id) = holder {
            self.push(Audience::One(id), event.clone());
        }
        self.push(Audience::Trackers(entity), event);
    }

    fn use_up_totem(&mut self, holder: Holder, hand: Hand, totem: &ItemStack) {
        match holder {
            Holder::Player(id) => {
                if self.held(id, hand).as_ref() == Some(totem) {
                    self.trigger_item(id, "minecraft:used_totem", totem);
                    self.shrink_held(id, hand, 1);
                    self.refresh_hands(id);
                }
            }
            Holder::Mob(id) => {
                let slot = mob_slot(hand);
                if let Some(state) = self
                    .entities
                    .store
                    .get_mut(id)
                    .and_then(|entity| entity.living_mut())
                    && state.equipment(slot) == Some(totem)
                {
                    let remaining = totem.count.get() - 1;
                    state.equipment[slot_index(slot)] =
                        std::num::NonZeroU8::new(remaining).map(|count| ItemStack {
                            count,
                            ..totem.clone()
                        });
                }
            }
        }
    }

    fn apply_totem_effect(&mut self, id: PlayerId, effect: &StackConsumeEffect) {
        let cause = EntityPotionEffectCause::Totem;
        match effect {
            StackConsumeEffect::ClearAllEffects => {
                self.remove_effects(id, None, cause);
            }
            StackConsumeEffect::ApplyEffects { effects, .. } => {
                for instance in effects.iter().filter_map(crate::consume::instance_of) {
                    self.apply_effect(id, instance, Some(cause));
                }
            }
            StackConsumeEffect::RemoveEffects(names) => {
                for effect in names.iter().filter_map(|name| EffectId::from_name(name)) {
                    self.remove_effects(id, Some(effect), cause);
                }
            }
            StackConsumeEffect::TeleportRandomly { .. } | StackConsumeEffect::PlaySound(_) => {}
        }
    }

    fn apply_mob_totem_effect(&mut self, id: EntityId, effect: &StackConsumeEffect) {
        let cause = EntityPotionEffectCause::Totem;
        match effect {
            StackConsumeEffect::ClearAllEffects => {
                self.remove_entity_effects(id, None, cause);
            }
            StackConsumeEffect::ApplyEffects { effects, .. } => {
                for instance in effects.iter().filter_map(crate::consume::instance_of) {
                    self.apply_entity_effect(id, instance, cause);
                }
            }
            StackConsumeEffect::RemoveEffects(names) => {
                for effect in names.iter().filter_map(|name| EffectId::from_name(name)) {
                    self.remove_entity_effects(id, Some(effect), cause);
                }
            }
            StackConsumeEffect::TeleportRandomly { .. } | StackConsumeEffect::PlaySound(_) => {}
        }
    }

    fn restore_after_totem(&mut self, holder: Holder, totem: Option<&ItemStack>) {
        let effects = totem
            .and_then(stack_components::death_effects)
            .unwrap_or_else(default_death_effects);
        match holder {
            Holder::Player(id) => {
                if let Some(player) = self.players.get_mut(&id) {
                    player.vitals.health = HEALTH_AFTER;
                }
                for effect in effects {
                    self.apply_totem_effect(id, &effect);
                }
            }
            Holder::Mob(id) => {
                if let Some(entity) = self.entities.store.get_mut(id)
                    && let rspigot_domain::entity::EntityState::Living(state) = &mut entity.state
                {
                    living::set_health(&mut entity.base, state, HEALTH_AFTER);
                }
                for effect in effects {
                    self.apply_mob_totem_effect(id, &effect);
                }
            }
        }
    }
}

fn default_death_effects() -> Vec<StackConsumeEffect> {
    ItemKind::TotemOfUndying
        .death_effects()
        .map(|effects| effects.iter().map(StackConsumeEffect::from).collect())
        .unwrap_or_default()
}
