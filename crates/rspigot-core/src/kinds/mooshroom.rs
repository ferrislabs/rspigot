use std::num::NonZeroU8;

use rspigot_domain::entity::species::Species;
use rspigot_domain::entity::species::mooshroom::{
    self, EAT_SOUND, MILK_SOUND, MILK_SUSPICIOUSLY_SOUND, MooshroomState, SHEAR_SOUND,
    SMALL_FLOWERS, StewEffect, Variant,
};
use rspigot_domain::entity::{Entity, EntityState};
use rspigot_domain::item::ItemStack;
use rspigot_domain::player::PlayerId;
use rspigot_domain::random::RandomSource;
use rspigot_domain::{EntityId, Vec3};
use rspigot_entities::{EntityKind as Kind, SoundSource};
use rspigot_items::{Craft, ItemKind, ItemSet, ProductComponent, StewEffect as FlowerEffect};

use crate::events::Hand;
use crate::interact::{Interaction, item_tag};
use crate::loot::{self, LootContext};
use crate::plugins::steps::Follow;
use crate::ports::ClientLink;
use crate::server::Server;

const STEW_EFFECTS: &str = "minecraft:suspicious_stew_effects";
const SHEAR_TABLE: &str = "minecraft:shearing/mooshroom";

fn mooshroom_state(entity: &mut Entity) -> Option<&mut MooshroomState> {
    match entity
        .living_mut()
        .and_then(|state| state.mob.as_mut())
        .map(|mob| &mut mob.species)
    {
        Some(Species::Mooshroom(state)) => Some(state),
        _ => None,
    }
}

fn flower_effect(stack: &ItemStack) -> Option<StewEffect> {
    if !item_tag(stack.item, SMALL_FLOWERS) {
        return None;
    }
    ItemKind::SuspiciousStew
        .crafts()
        .iter()
        .find_map(|craft| match craft {
            Craft::Shapeless {
                ingredients,
                product,
                ..
            } if matches!(
                ingredients.last(),
                Some(ItemSet::Direct(flowers)) if flowers.contains(&stack.item)
            ) =>
            {
                product.components.iter().find_map(|component| {
                    let ProductComponent::SuspiciousStewEffects(effects) = component;
                    let FlowerEffect { effect, duration } = *effects.first()?;
                    StewEffect::named(effect, duration)
                })
            }
            _ => None,
        })
}

fn stew_of(effect: Option<StewEffect>) -> ItemStack {
    let Some(effect) = effect else {
        return ItemStack::new(ItemKind::MushroomStew.id(), NonZeroU8::MIN);
    };
    let mut stew = ItemStack::new(ItemKind::SuspiciousStew.id(), NonZeroU8::MIN);
    stew.components.insert(
        STEW_EFFECTS,
        rspigot_nbt::Tag::List(vec![rspigot_nbt::Tag::Compound(
            rspigot_nbt::Compound::new()
                .with("id", effect.name())
                .with("duration", effect.duration),
        )]),
    );
    stew
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn mooshroom_offspring(
        &mut self,
        child: &mut Entity,
        parent: EntityId,
        partner: EntityId,
        random: &mut impl RandomSource,
    ) {
        let variant = |server: &Self, id: EntityId| {
            server
                .entities
                .store
                .get(id)
                .map_or(Variant::Red, |entity| mooshroom::variant(&entity.base))
        };
        let chosen =
            mooshroom::offspring_variant(variant(self, parent), variant(self, partner), random);
        mooshroom::set_variant(&mut child.base, chosen);
    }

    pub(crate) fn mooshroom_interact(
        &mut self,
        player: PlayerId,
        target: EntityId,
        hand: Hand,
        held: Option<&ItemStack>,
    ) -> Option<Interaction> {
        let stack = held?;
        let entity = self.entities.store.get(target)?;
        let baby = entity.is_baby();
        let variant = mooshroom::variant(&entity.base);
        if stack.item == ItemKind::Bowl.id() && !baby {
            return Some(self.mooshroom_milk(player, target, hand));
        }
        if variant == Variant::Brown
            && let Some(effect) = flower_effect(stack)
        {
            let stored = self
                .entities
                .store
                .get_mut(target)
                .and_then(mooshroom_state)?;
            if stored.stew.is_none() {
                stored.stew = Some(effect);
                self.consume_held(player, hand, 1);
                self.entity_sound(target, EAT_SOUND, 1.0, 1.0);
            }
            return Some(Interaction::Predicted);
        }
        None
    }

    fn mooshroom_milk(&mut self, player: PlayerId, target: EntityId, hand: Hand) -> Interaction {
        let effect = self
            .entities
            .store
            .get_mut(target)
            .and_then(mooshroom_state)
            .and_then(|state| state.stew.take());
        let sound = if effect.is_some() {
            MILK_SUSPICIOUSLY_SOUND
        } else {
            MILK_SOUND
        };
        self.entity_sound(target, sound, 1.0, 1.0);
        let result = stew_of(effect);
        self.consume_held(player, hand, 1);
        if self.held(player, hand).is_none() {
            self.set_held(player, hand, Some(result));
        } else {
            self.give_or_drop(player, result);
        }
        Interaction::Predicted
    }

    pub(crate) fn mooshroom_struck(&mut self, bolt: EntityId, victim: EntityId) {
        let Some(uuid) = self.entities.store.get(bolt).map(|bolt| bolt.base.uuid) else {
            return;
        };
        let toggled = match self.entities.store.get_mut(victim) {
            Some(entity) => {
                let base = &mut entity.base;
                match entity.state {
                    EntityState::Living(ref mut living) => {
                        match living.mob.as_mut().map(|mob| &mut mob.species) {
                            Some(Species::Mooshroom(state)) => {
                                mooshroom::struck_by_lightning(base, state, uuid)
                            }
                            _ => false,
                        }
                    }
                    _ => false,
                }
            }
            None => false,
        };
        if toggled {
            self.entity_sound(victim, mooshroom::LIGHTNING_CONVERT_SOUND, 2.0, 1.0);
        }
    }

    pub(crate) fn shear_mooshroom(&mut self, target: EntityId) {
        self.play_entity_sound(target, SHEAR_SOUND, SoundSource::Players, 1.0, 1.0);
        self.convert_with(target, Kind::Cow, None, Follow::Shear);
    }

    pub(crate) fn drop_mooshroom_shearings(&mut self, target: EntityId) {
        let Some(entity) = self.entities.store.get(target) else {
            return;
        };
        let facts = crate::mobs::loot_facts_of(entity);
        let height = f64::from(entity.base.dimensions.height);
        let drops = rspigot_data::loot::table(SHEAR_TABLE).map_or_else(Vec::new, |table| {
            let context = LootContext {
                this: Some(&facts),
                ..LootContext::default()
            };
            loot::table_drops(table, &context, &mut self.rng)
        });
        for stack in drops {
            for _ in 0..stack.count.get() {
                let single = ItemStack {
                    count: NonZeroU8::MIN,
                    ..stack.clone()
                };
                let mut random = self.mob_random(target);
                let push = Vec3::new(
                    f64::from((random.next_float() - random.next_float()) * 0.1),
                    f64::from(random.next_float() * 0.05),
                    f64::from((random.next_float() - random.next_float()) * 0.1),
                );
                self.store_mob_random(target, random);
                self.drop_at_location(target, height, single, push);
            }
        }
    }
}
