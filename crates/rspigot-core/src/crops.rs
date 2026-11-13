use rspigot_domain::crop;
use rspigot_domain::player::PlayerId;
use rspigot_domain::{BlockPos, BlockStateId, EntityId};
use rspigot_items::ItemKind;

use crate::events::Hand;
use crate::mobs::ServerRandom;
use crate::plugins::events::BlockGrow;
use crate::plugins::steps::GrowStep;
use crate::plugins::{Continuation, EventPayload, Lane, snapshot_of};
use crate::ports::ClientLink;
use crate::server::Server;

pub(crate) const BONE_MEAL_EVENT_DATA: i32 = 15;

pub(crate) fn is_bone_meal(item: rspigot_domain::ItemId) -> bool {
    item == ItemKind::BoneMeal.id()
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn grow_crop(&mut self, lane: Lane, pos: BlockPos) -> bool {
        let Some(state) = self.world.chunks.block(pos) else {
            return false;
        };
        if !crop::is_valid_bonemeal_target(state) {
            return false;
        }
        let mut random = ServerRandom(&mut self.rng);
        if let Some(grown) = crop::bonemealed_state(state, &mut random) {
            self.grow_block(lane, pos, grown);
        }
        true
    }

    pub(crate) fn grow_block(&mut self, lane: Lane, pos: BlockPos, state: BlockStateId) {
        self.fire(
            lane,
            Continuation::Grow(GrowStep { pos, state }),
            move |_| {
                Some(EventPayload::BlockGrow(BlockGrow {
                    block: pos,
                    new_state: snapshot_of(pos, state),
                }))
            },
        );
    }

    pub(crate) fn finish_grow(&mut self, step: GrowStep, cancelled: bool) {
        if !cancelled {
            self.set_block(step.pos, step.state);
            if step.state.kind() == Some(rspigot_blocks::BlockKind::TurtleEgg) {
                self.finish_turtle_egg_crack(step.pos);
            }
        }
    }

    pub(crate) fn villager_use_bone_meal(&mut self, id: EntityId, pos: BlockPos) {
        let valid = self
            .world
            .chunks
            .block(pos)
            .is_some_and(crop::is_valid_bonemeal_target);
        if !valid || !self.villager_spend(id, ItemKind::BoneMeal) {
            return;
        }
        let lane = self.lane_for(None, id);
        self.grow_crop(lane, pos);
    }

    pub(crate) fn use_bone_meal(
        &mut self,
        id: PlayerId,
        hand: Hand,
        pos: BlockPos,
        sequence: i32,
    ) -> bool {
        let lane = Lane::Player(id);
        if !self.grow_crop(lane, pos) {
            return false;
        }
        self.consume_held(id, hand, 1);
        self.level_event(
            rspigot_domain::composter::BONE_MEAL_EVENT,
            pos,
            BONE_MEAL_EVENT_DATA,
        );
        self.player_swing(id, hand, false);
        self.refresh_hands(id);
        self.ack_block(id, sequence);
        true
    }
}
