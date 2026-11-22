use std::num::NonZeroU8;

use rspigot_blocks::BlockKind;
use rspigot_domain::chunk::BlockEntity;
use rspigot_domain::entity::save::{item_stack_from_nbt, item_stack_to_nbt};
use rspigot_domain::item::{ItemStack, same_item_same_components};
use rspigot_domain::player::GameMode;
use rspigot_domain::{BlockPos, BlockStateId, Identifier};
use rspigot_entities::SoundSource;
use rspigot_nbt::Compound;

use crate::events::ClientEvent;
use crate::plugins::steps::UseOnStep;
use crate::ports::ClientLink;
use crate::server::{Audience, Server};

const ENTITY: &str = "decorated_pot";
const ITEM: &str = "item";
const INSERT: &str = "minecraft:block.decorated_pot.insert";
const INSERT_FAIL: &str = "minecraft:block.decorated_pot.insert_fail";
const WOBBLE_EVENT: u8 = 1;
const WOBBLE_RANGE: f64 = 64.0;
const COMPARATOR_STEPS: f32 = 14.0;

pub(crate) fn is_pot(state: BlockStateId) -> bool {
    state.kind() == Some(BlockKind::DecoratedPot)
}

fn accepts(held: &ItemStack, inside: Option<&ItemStack>) -> bool {
    inside.is_none_or(|stack| {
        same_item_same_components(stack, held) && stack.count.get() < held.item.max_stack_size()
    })
}

fn insert_pitch(count: u8, max: u8) -> f32 {
    0.7 + 0.5 * (f32::from(count) / f32::from(max.max(1)))
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn pot_stack(&self, pos: BlockPos) -> Option<ItemStack> {
        self.world
            .chunks
            .get(pos.chunk())
            .and_then(|chunk| chunk.block_entity(pos))
            .and_then(|entity| entity.data.get_compound(ITEM))
            .and_then(item_stack_from_nbt)
    }

    pub(crate) fn set_pot_stack(&mut self, pos: BlockPos, stack: Option<&ItemStack>) {
        let Some(chunk) = self.world.chunks.chunk_mut(pos.chunk()) else {
            return;
        };
        let mut data = chunk
            .block_entity(pos)
            .map_or_else(Compound::new, |entity| entity.data.clone());
        match stack {
            Some(stack) => {
                data.insert(ITEM, item_stack_to_nbt(stack));
            }
            None => {
                data.remove(ITEM);
            }
        }
        chunk.set_block_entity(BlockEntity {
            pos,
            kind: Identifier::minecraft(ENTITY),
            data,
        });
        self.update_neighbour_for_output_signal(pos);
    }

    pub(crate) fn pot_comparator(&self, pos: BlockPos) -> i32 {
        let Some(stack) = self.pot_stack(pos) else {
            return 0;
        };
        let max = f32::from(stack.item.max_stack_size().max(1));
        (f32::from(stack.count.get()) / max * COMPARATOR_STEPS).floor() as i32 + 1
    }

    pub(crate) fn pot_items_lost_by(
        &self,
        pos: BlockPos,
        old: BlockStateId,
        new: BlockStateId,
    ) -> Vec<ItemStack> {
        if !is_pot(old) || old.kind() == new.kind() {
            return Vec::new();
        }
        self.pot_stack(pos).into_iter().collect()
    }

    pub(crate) fn pot_wobble(&mut self, pos: BlockPos, negative: bool) {
        self.push(
            Audience::Nearby {
                position: pos.center(),
                range: WOBBLE_RANGE,
                except: None,
            },
            ClientEvent::BlockEvent {
                pos,
                action: WOBBLE_EVENT,
                data: u8::from(negative),
                block: BlockKind::DecoratedPot.id().0,
            },
        );
    }

    pub(crate) fn use_decorated_pot(&mut self, step: &UseOnStep) -> bool {
        let (id, hand, pos) = (step.player, step.hand, step.pos);
        if !is_pot(step.clicked)
            || self
                .players
                .get(&id)
                .is_none_or(|player| player.game_mode == GameMode::Spectator)
        {
            return false;
        }
        let inside = self.pot_stack(pos);
        let held = self.held(id, hand);
        match held.filter(|held| accepts(held, inside.as_ref())) {
            Some(held) => {
                let count = inside.as_ref().map_or(0, |stack| stack.count.get()) + 1;
                let max = held.item.max_stack_size();
                let stored = ItemStack {
                    count: NonZeroU8::new(count).unwrap_or(NonZeroU8::MIN),
                    ..held
                };
                self.consume_held(id, hand, 1);
                self.set_pot_stack(pos, Some(&stored));
                self.pot_wobble(pos, false);
                self.play_sound(
                    None,
                    pos.center(),
                    INSERT,
                    SoundSource::Blocks,
                    1.0,
                    insert_pitch(count, max),
                );
            }
            None => {
                self.play_sound(
                    None,
                    pos.center(),
                    INSERT_FAIL,
                    SoundSource::Blocks,
                    1.0,
                    1.0,
                );
                self.pot_wobble(pos, true);
            }
        }
        self.player_swing(id, hand, false);
        self.refresh_hands(id);
        self.ack_block(id, step.sequence);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_insert_pitch_rises_with_the_fill() {
        assert!(insert_pitch(1, 64) < insert_pitch(64, 64));
        assert!((insert_pitch(64, 64) - 1.2).abs() < f32::EPSILON);
    }
}
