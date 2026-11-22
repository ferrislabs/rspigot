use rspigot_domain::item::ItemStack;
use rspigot_domain::player::PlayerId;

use crate::events::Hand;
use crate::ports::ClientLink;
use crate::server::Server;

impl<L: ClientLink> Server<L> {
    pub fn enchanted_item_damage(&mut self, stack: &ItemStack, amount: i32) -> i32 {
        crate::enchant_effects::modify_item_damage(stack, amount, &mut self.rng)
    }

    pub(crate) fn hurt_and_break(&mut self, id: PlayerId, hand: Hand, amount: i32) {
        self.wear(id, hand, amount);
    }

    pub(crate) fn wear_tool_for_mining(&mut self, id: PlayerId, hardness: f32) {
        if hardness == 0.0 {
            return;
        }
        let amount = self
            .players
            .get(&id)
            .and_then(|player| player.inventory.held())
            .and_then(|stack| stack.item.kind())
            .and_then(|kind| kind.tool())
            .and_then(|tool| i32::try_from(tool.damage_per_block).ok());
        if let Some(amount) = amount {
            self.hurt_and_break(id, Hand::Main, amount);
        }
    }
}
