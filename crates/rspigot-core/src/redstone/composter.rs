use std::num::NonZeroU8;

use rspigot_domain::BlockStateId;
use rspigot_domain::composter;
use rspigot_domain::item::{ItemStack, Slot};
use rspigot_entities::SoundSource;
use rspigot_items::ItemKind;

use super::storage::Holder;
use crate::mobs::ServerRandom;
use crate::ports::ClientLink;
use crate::server::Server;

impl<L: ClientLink> Server<L> {
    pub(crate) fn composter_holder_size(&self, state: BlockStateId) -> usize {
        match composter::level(state) {
            Some(level) if level == composter::MAX_FILL_LEVEL => 0,
            _ => 1,
        }
    }

    pub(crate) fn composter_holder_items(&self, holder: &Holder) -> Vec<Slot> {
        let state = self.block_or_air(holder.pos);
        match composter::level(state) {
            Some(composter::READY_LEVEL) => vec![Some(ItemStack::new(
                ItemKind::BoneMeal.id(),
                NonZeroU8::MIN,
            ))],
            Some(level) if level < composter::MAX_FILL_LEVEL => vec![None],
            _ => Vec::new(),
        }
    }

    pub(crate) fn composter_holder_stored(&mut self, holder: &Holder, items: &[Slot]) {
        let pos = holder.pos;
        let state = self.block_or_air(pos);
        let Some(level) = composter::level(state) else {
            return;
        };
        let first = items.first().and_then(Option::as_ref);
        if level == composter::READY_LEVEL {
            if first.is_none() {
                self.set_block(pos, composter::emptied(state));
                self.play_sound(
                    None,
                    pos.center(),
                    composter::EMPTY_SOUND,
                    SoundSource::Blocks,
                    1.0,
                    1.0,
                );
            }
            return;
        }
        let Some(stack) = first.filter(|_| level < composter::MAX_FILL_LEVEL) else {
            return;
        };
        let count = {
            let mut random = ServerRandom(&mut self.rng);
            composter::layer_count(stack.item, level, &mut random).unwrap_or(0)
        };
        let to = composter::add_layer(state, level, count);
        self.set_block(pos, to);
        self.level_event(composter::FILL_EVENT, pos, i32::from(to != state));
    }
}
