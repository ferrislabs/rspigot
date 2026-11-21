use std::num::NonZeroU8;

use rspigot_domain::Vec3;
use rspigot_domain::effect::{EffectDuration, EffectInstance, EffectKind};
use rspigot_domain::item::ItemStack;
use rspigot_domain::player::{GameMode, PlayerId};
use rspigot_entities::SoundSource;
use rspigot_items::{Food, ItemKind};

use crate::bow::{UsingItem, hand_slot};
use crate::consume::{consumable_of, game_event_of};
use crate::events::Hand;
use crate::game_events::GameEventContext;
use crate::player::Player;
use crate::plugins::events::{EntityPotionEffectCause, EquipmentSlot, PlayerItemConsume};
use crate::plugins::steps::{ConsumeStep, Continuation};
use crate::plugins::{EventPayload, ItemSpec, spec_of, stack_of};
use crate::ports::ClientLink;
use crate::server::Server;
use rspigot_domain::game_event::GameEvent;

const BURP_SOUND: &str = "minecraft:entity.player.burp";
const OMINOUS_BOTTLE_DISPOSE: &str = "minecraft:item.ominous_bottle.dispose";
const OMINOUS_BOTTLE_AMPLIFIER: &str = "minecraft:ominous_bottle_amplifier";
const MAX_OMINOUS_AMPLIFIER: i32 = 4;
const BAD_OMEN_TICKS: u32 = 120_000;

const fn slot_of(hand: Hand) -> EquipmentSlot {
    match hand {
        Hand::Main => EquipmentSlot::Hand,
        Hand::Off => EquipmentSlot::OffHand,
    }
}

fn eat_ticks(stack: &ItemStack) -> Option<u64> {
    consumable_of(stack).map(|consumable| consumable.consume_ticks())
}

fn is_ominous_bottle(stack: &ItemStack) -> bool {
    stack.item == ItemKind::OminousBottle.id()
}

fn ominous_amplifier(stack: &ItemStack) -> u8 {
    stack
        .components
        .get_i32(OMINOUS_BOTTLE_AMPLIFIER)
        .unwrap_or(0)
        .clamp(0, MAX_OMINOUS_AMPLIFIER) as u8
}

fn food_of(stack: &ItemStack) -> Option<Food> {
    stack.item.kind().and_then(ItemKind::food)
}

pub(crate) fn can_eat<L>(player: &Player<L>, food: Food) -> bool {
    player.game_mode == GameMode::Creative || food.can_always_eat || player.vitals.food.needs_food()
}

fn can_consume<L>(player: &Player<L>, stack: &ItemStack) -> bool {
    consumable_of(stack).is_some() && food_of(stack).is_none_or(|food| can_eat(player, food))
}

impl<L: ClientLink> Server<L> {
    fn drink_ominous_bottle(&mut self, id: PlayerId, slot: usize, stack: &ItemStack) {
        let Some(player) = self.players.get_mut(&id) else {
            return;
        };
        let creative = player.game_mode == GameMode::Creative;
        let position = player.position;
        let entity = player.entity_id;
        if !creative {
            let left = NonZeroU8::new(stack.count.get() - 1).map(|count| ItemStack {
                count,
                ..stack.clone()
            });
            player.inventory.set(slot, left);
        }
        let amplifier = ominous_amplifier(stack);
        let mut omen = EffectInstance::new(
            EffectKind::BadOmen.id(),
            EffectDuration::Ticks(BAD_OMEN_TICKS),
            amplifier,
            true,
        );
        omen.show_icon = true;
        self.apply_effect(id, omen, Some(EntityPotionEffectCause::PotionDrink));
        self.play_sound(
            None,
            position,
            OMINOUS_BOTTLE_DISPOSE,
            SoundSource::Players,
            1.0,
            1.0,
        );
        self.game_event(
            GameEvent::Drink,
            position,
            GameEventContext::of(Some(entity)),
        );
        self.send_inventory(id);
    }

    pub(crate) fn tick_eating(&mut self) {
        self.tick_use_effects();
        let tick = self.current_tick();
        let finished: Vec<(PlayerId, Hand)> = self
            .players
            .iter()
            .filter_map(|(id, player)| {
                let using = player.using_item?;
                let stack = player.inventory.get(hand_slot(player, using.hand))?;
                let ticks = eat_ticks(stack)?;
                (tick.saturating_sub(using.since) >= ticks).then_some((*id, using.hand))
            })
            .collect();
        for (id, hand) in finished {
            self.finish_eating(id, hand);
        }
    }

    fn finish_eating(&mut self, id: PlayerId, hand: Hand) {
        let Some(player) = self.players.get_mut(&id) else {
            return;
        };
        player.using_item = None;
        let slot = hand_slot(player, hand);
        let Some(stack) = player.inventory.get(slot).cloned() else {
            return;
        };
        if !can_consume(player, &stack) {
            return;
        }
        let step = ConsumeStep {
            player: id,
            hand,
            stack,
        };
        self.fire(id, Continuation::Consume(Box::new(step)), move |server| {
            let stack = server
                .players
                .get(&id)
                .and_then(|player| player.inventory.get(hand_slot(player, hand)))?;
            Some(EventPayload::PlayerItemConsume(PlayerItemConsume {
                hand: slot_of(hand),
                item: spec_of(stack),
                player: server.snapshot_of(id)?,
            }))
        });
    }

    pub(crate) fn finish_consume(
        &mut self,
        step: ConsumeStep,
        cancelled: bool,
        patched: Option<ItemSpec>,
    ) {
        let ConsumeStep {
            player: id,
            hand,
            stack,
        } = step;
        let Some(player) = self.players.get(&id) else {
            return;
        };
        let slot = hand_slot(player, hand);
        if player.inventory.get(slot) != Some(&stack) {
            return;
        }
        if cancelled {
            self.send_inventory(id);
            return;
        }
        let replacement = patched.and_then(|spec| stack_of(&spec).ok());
        let replaced = replacement.is_some();
        let Some(effective) = replacement.unwrap_or(Some(stack)) else {
            if let Some(player) = self.players.get_mut(&id) {
                player.inventory.set(slot, None);
            }
            self.send_inventory(id);
            return;
        };
        self.consume_stack(id, slot, effective, replaced);
    }

    fn consume_stack(&mut self, id: PlayerId, slot: usize, stack: ItemStack, replaced: bool) {
        if is_ominous_bottle(&stack) {
            self.drink_ominous_bottle(id, slot, &stack);
            return;
        }
        let Some(player) = self.players.get_mut(&id) else {
            return;
        };
        let Some(consumable) = consumable_of(&stack) else {
            if replaced {
                player.inventory.set(slot, Some(stack));
                self.send_inventory(id);
            }
            return;
        };
        let food = food_of(&stack);
        if let Some(food) = food {
            player
                .vitals
                .food
                .add(food.nutrition as i32, food.saturation);
        }
        let creative = player.game_mode == GameMode::Creative;
        let position = player.position;
        let entity = player.entity_id;
        let mut remainder = None;
        if creative {
            if replaced {
                player.inventory.set(slot, Some(stack.clone()));
            }
        } else {
            let left = NonZeroU8::new(stack.count.get() - 1).map(|count| ItemStack {
                count,
                ..stack.clone()
            });
            let replacement = stack
                .item
                .kind()
                .and_then(ItemKind::use_remainder)
                .map(|item| ItemStack::new(item, NonZeroU8::MIN));
            match (left, replacement) {
                (None, replacement) => {
                    player.inventory.set(slot, replacement);
                }
                (left, replacement) => {
                    player.inventory.set(slot, left);
                    remainder = replacement;
                }
            }
        }
        if let Some(spare) = remainder {
            let mut spare = Some(spare);
            if let Some(player) = self.players.get_mut(&id) {
                player.inventory.add(&mut spare, false);
            }
            if let Some(spare) = spare {
                self.drop_at_location(entity, 1.0, spare, Vec3::ZERO);
            }
        }
        if food.is_some() {
            self.play_sound(None, position, BURP_SOUND, SoundSource::Players, 0.5, 1.0);
        } else {
            self.play_sound(
                None,
                position,
                consumable.sound,
                SoundSource::Players,
                1.0,
                1.0,
            );
        }
        self.game_event(
            game_event_of(consumable.animation),
            position,
            GameEventContext::of(Some(entity)),
        );
        self.apply_consumable(id, &stack, &consumable);
        if let Some(ticks) = stack.item.kind().and_then(crate::cooldown::use_ticks) {
            self.start_cooldown(id, stack.item, ticks);
        }
        self.count_used(id, &stack);
        self.trigger_consume_item(id, &stack);
        self.send_inventory(id);
    }
}

pub(crate) fn starts_eating<L>(player: &Player<L>, hand: Hand, since: u64) -> Option<UsingItem> {
    let stack = player.inventory.get(hand_slot(player, hand))?;
    can_consume(player, stack).then_some(UsingItem { hand, since })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bottle(amplifier: Option<i32>) -> ItemStack {
        let mut stack = ItemStack::new(ItemKind::OminousBottle.id(), NonZeroU8::MIN);
        if let Some(level) = amplifier {
            stack.components.insert(OMINOUS_BOTTLE_AMPLIFIER, level);
        }
        stack
    }

    #[test]
    fn the_bottle_amplifier_defaults_to_zero_and_is_clamped_to_four() {
        assert_eq!(ominous_amplifier(&bottle(None)), 0);
        assert_eq!(ominous_amplifier(&bottle(Some(2))), 2);
        assert_eq!(ominous_amplifier(&bottle(Some(9))), 4);
        assert_eq!(ominous_amplifier(&bottle(Some(-3))), 0);
    }

    #[test]
    fn only_the_ominous_bottle_is_drunk_for_bad_omen() {
        assert!(is_ominous_bottle(&bottle(None)));
        let apple = ItemStack::new(ItemKind::Apple.id(), NonZeroU8::MIN);
        assert!(!is_ominous_bottle(&apple));
    }
}
