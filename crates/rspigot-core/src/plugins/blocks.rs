use rspigot_domain::{BlockPos, BlockStateId};

use crate::explosion::BlockInteraction;
use crate::plugins::block::material_of;
use crate::plugins::block::snapshot_of;
use crate::plugins::decisions::{Continuation, Lane};
use crate::plugins::events::{
    BlockCook, BlockExp, BlockExpPatch, BlockExplode, BlockExplodePatch, BlockPhysics,
    ExplosionResult,
};
use crate::plugins::steps::{CookStep, ExpStep, ExplodeStep, PhysicsStep};
use crate::plugins::{EventKind, EventPayload, SetDiff, spec_of};
use crate::ports::ClientLink;
use crate::server::Server;
use rspigot_domain::campfire::Campfire;

impl<L: ClientLink> Server<L> {
    pub(crate) fn ask_physics(&mut self, pos: BlockPos, state: BlockStateId) {
        self.fire(
            Lane::Block(pos),
            Continuation::Physics(PhysicsStep { pos, state }),
            move |_| {
                Some(EventPayload::BlockPhysics(BlockPhysics {
                    block: pos,
                    changed_type: material_of(state).to_owned(),
                    source_block: pos,
                }))
            },
        );
    }

    pub(crate) fn finish_physics(&mut self, step: PhysicsStep) {
        if self.world.chunks.block(step.pos) == Some(step.state) {
            self.update_neighbour_shapes(step.pos);
        }
    }

    pub(crate) fn ask_cook(&mut self, step: CookStep) {
        let (pos, slot) = (step.pos, step.slot);
        self.fire(
            Lane::Block(pos),
            Continuation::Cook(Box::new(step)),
            move |server| {
                let campfire = server.campfire_at(pos)?;
                let source = campfire.items[slot].as_ref()?;
                Some(EventPayload::BlockCook(BlockCook {
                    block: pos,
                    result: spec_of(&Campfire::result_of(source)),
                    source: spec_of(source),
                }))
            },
        );
    }

    pub(crate) fn ask_block_exp(&mut self, step: ExpStep) {
        let ExpStep { pos, amount, .. } = step;
        self.fire(Lane::Block(pos), Continuation::Exp(step), move |_| {
            Some(EventPayload::BlockExp(BlockExp {
                block: pos,
                exp_to_drop: amount,
            }))
        });
    }

    pub(crate) fn finish_block_exp(&mut self, step: ExpStep, patch: BlockExpPatch) {
        let amount = patch.exp_to_drop.unwrap_or(step.amount);
        if self.world.chunks.is_loaded(step.pos.chunk()) {
            self.award_experience(step.at, amount);
        }
    }

    pub(crate) fn ask_explosion(&mut self, step: ExplodeStep) {
        let block = step.center.block();
        let block_list = self
            .listened(EventKind::BlockExplode)
            .then(|| step.blocks.clone());
        let explosion_result = match step.interaction {
            BlockInteraction::Keep => ExplosionResult::Keep,
            BlockInteraction::Destroy => ExplosionResult::Destroy,
            BlockInteraction::DestroyWithDecay => ExplosionResult::DestroyWithDecay,
        };
        let explosion_yield = step.explosion_yield;
        self.fire(
            Lane::Block(block),
            Continuation::Explode(Box::new(step)),
            move |server| {
                Some(EventPayload::BlockExplode(BlockExplode {
                    block,
                    block_list: block_list?,
                    exploded_block_state: snapshot_of(
                        block,
                        server
                            .world
                            .chunks
                            .block(block)
                            .unwrap_or(BlockStateId::AIR),
                    ),
                    explosion_result,
                    explosion_yield,
                }))
            },
        );
    }

    pub(crate) fn finish_explosion(
        &mut self,
        step: ExplodeStep,
        cancelled: bool,
        patch: BlockExplodePatch,
    ) {
        self.settle_explosion(step, cancelled, patch.block_list, patch.explosion_yield);
    }

    pub(crate) fn settle_explosion(
        &mut self,
        step: ExplodeStep,
        cancelled: bool,
        block_list: SetDiff<BlockPos>,
        explosion_yield: Option<f32>,
    ) {
        let chunks = &self.world.chunks;
        let blocks: Vec<BlockPos> = block_list
            .apply(step.blocks)
            .into_iter()
            .filter(|pos| chunks.block(*pos).is_some_and(|state| !state.is_air()))
            .collect();
        let explosion_yield = explosion_yield.unwrap_or(step.explosion_yield);
        let fire = step.fire;
        let discard = step.discard;
        let burning = if fire { blocks.clone() } else { Vec::new() };
        if !cancelled {
            self.blast(ExplodeStep {
                blocks,
                explosion_yield,
                ..step
            });
        }
        if fire {
            self.create_fire(&burning);
        }
        if let Some(source) = discard {
            let cause = self.discard_cause(source);
            self.remove_entity(
                source,
                rspigot_domain::entity::RemovalReason::Discarded,
                Some(cause),
            );
        }
    }
}
