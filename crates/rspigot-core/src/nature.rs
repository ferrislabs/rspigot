use rspigot_domain::{BlockPos, BlockStateId};

use crate::loot::{self, LootContext};
use crate::plugins::events::{
    BlockBurn, BlockFade, BlockForm, BlockIgnite, BlockIgniteCause, BlockSpread, LeavesDecay,
    MoistureChange,
};
use crate::plugins::steps::{Nature, NatureStep};
use crate::plugins::{Continuation, EventPayload, Lane, snapshot_of};
use crate::ports::ClientLink;
use crate::server::Server;

fn payload_of(step: NatureStep) -> EventPayload {
    let NatureStep {
        nature,
        pos,
        state,
        source,
        ..
    } = step;
    match nature {
        Nature::Fade => EventPayload::BlockFade(BlockFade {
            block: pos,
            new_state: snapshot_of(pos, state),
        }),
        Nature::Burn => EventPayload::BlockBurn(BlockBurn {
            block: pos,
            igniting_block: Some(source),
        }),
        Nature::Ignite => EventPayload::BlockIgnite(BlockIgnite {
            block: pos,
            cause: BlockIgniteCause::Spread,
            igniting_block: Some(source),
            igniting_entity: None,
            player: None,
        }),
        Nature::Spread => EventPayload::BlockSpread(BlockSpread {
            block: pos,
            new_state: snapshot_of(pos, state),
            source,
        }),
        Nature::Form => EventPayload::BlockForm(BlockForm {
            block: pos,
            new_state: snapshot_of(pos, state),
        }),
        Nature::Moisture => EventPayload::MoistureChange(MoistureChange {
            block: pos,
            new_state: snapshot_of(pos, state),
        }),
        Nature::Leaves => EventPayload::LeavesDecay(LeavesDecay { block: pos }),
    }
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn nature_event(&mut self, step: NatureStep) {
        self.fire(
            Lane::Block(step.pos),
            Continuation::Nature(step),
            move |_| Some(payload_of(step)),
        );
    }

    pub(crate) fn nature_change(
        &mut self,
        nature: Nature,
        pos: BlockPos,
        state: BlockStateId,
        source: BlockPos,
    ) {
        let expected = self.world.chunks.block(pos).unwrap_or_default();
        self.nature_event(NatureStep {
            nature,
            pos,
            expected,
            state,
            source,
        });
    }

    pub(crate) fn finish_nature(&mut self, step: NatureStep, cancelled: bool) {
        if cancelled {
            return;
        }
        if step.nature == Nature::Ignite {
            self.nature_event(NatureStep {
                nature: Nature::Spread,
                ..step
            });
            return;
        }
        if self.world.chunks.block(step.pos) != Some(step.expected) {
            return;
        }
        if step.nature == Nature::Leaves {
            self.drop_decayed_leaves(step.pos, step.expected);
            return;
        }
        self.set_block(step.pos, step.state);
    }

    fn drop_decayed_leaves(&mut self, pos: BlockPos, state: BlockStateId) {
        if self.config.rules.block_drops {
            let context = LootContext::block(state, None, None);
            for stack in loot::block_drops(&context, &mut self.rng) {
                self.spawn_item(pos, stack);
            }
        }
        self.set_block(pos, BlockStateId::AIR);
    }
}
