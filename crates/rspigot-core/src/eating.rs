use std::num::NonZeroU8;

use rspigot_domain::Vec3;
use rspigot_domain::item::ItemStack;
use rspigot_domain::player::{GameMode, PlayerId};
use rspigot_entities::SoundSource;
use rspigot_items::{Food, ItemKind};

use crate::bow::{UsingItem, hand_slot};
use crate::events::Hand;
use crate::player::Player;
use crate::ports::ClientLink;
use crate::server::Server;

const EAT_TICKS: u64 = 32;
const FAST_EAT_TICKS: u64 = 16;
const BURP_SOUND: &str = "minecraft:entity.player.burp";

fn eat_ticks(stack: &ItemStack) -> u64 {
    if stack.item == ItemKind::DriedKelp.id() {
        FAST_EAT_TICKS
    } else {
        EAT_TICKS
    }
}

fn food_of(stack: &ItemStack) -> Option<Food> {
    stack.item.kind().and_then(ItemKind::food)
}

pub(crate) fn can_eat<L>(player: &Player<L>, food: Food) -> bool {
    player.game_mode == GameMode::Creative || food.can_always_eat || player.vitals.food.needs_food()
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn tick_eating(&mut self) {
        let tick = self.current_tick();
        let finished: Vec<(PlayerId, Hand)> = self
            .players
            .iter()
            .filter_map(|(id, player)| {
                let using = player.using_item?;
                let stack = player.inventory.get(hand_slot(player, using.hand))?;
                food_of(stack)?;
                (tick.saturating_sub(using.since) >= eat_ticks(stack)).then_some((*id, using.hand))
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
        let Some(food) = food_of(&stack) else {
            return;
        };
        if !can_eat(player, food) {
            return;
        }
        player
            .vitals
            .food
            .add(food.nutrition as i32, food.saturation);
        let creative = player.game_mode == GameMode::Creative;
        let position = player.position;
        let entity = player.entity_id;
        let mut remainder = None;
        if !creative {
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
        self.play_sound(None, position, BURP_SOUND, SoundSource::Players, 0.5, 1.0);
        self.send_inventory(id);
    }
}

pub(crate) fn starts_eating<L>(player: &Player<L>, hand: Hand, since: u64) -> Option<UsingItem> {
    let stack = player.inventory.get(hand_slot(player, hand))?;
    can_eat(player, food_of(stack)?).then_some(UsingItem { hand, since })
}
