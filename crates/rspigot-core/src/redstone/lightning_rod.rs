use rspigot_blocks::BlockKind;
use rspigot_domain::{BlockPos, BlockStateId};

use super::{MAX_POWER, RedstoneChange, RedstoneStep, facing, step};
use crate::ports::ClientLink;
use crate::server::Server;
use crate::signal::flag;

const PULSE: u64 = 8;

pub(crate) fn is_lightning_rod(kind: BlockKind) -> bool {
    kind.name().ends_with("lightning_rod")
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn lightning_rod_struck(&mut self, pos: BlockPos) {
        let state = self.block_or_air(pos);
        if !state.kind().is_some_and(is_lightning_rod) {
            return;
        }
        let powered = flag(state, "powered");
        self.ask_redstone(RedstoneStep {
            pos,
            old: if powered { MAX_POWER } else { 0 },
            new: if powered { 0 } else { MAX_POWER },
            change: RedstoneChange::LightningRod,
        });
    }

    pub(crate) fn finish_lightning_rod(&mut self, pos: BlockPos, new: i32) {
        if new < 1 {
            return;
        }
        let state = self.block_or_air(pos);
        let Some(kind) = state.kind().filter(|kind| is_lightning_rod(*kind)) else {
            return;
        };
        self.set_rod_powered(pos, state, true);
        self.schedule(pos, kind, PULSE);
    }

    pub(crate) fn lightning_rod_tick(&mut self, pos: BlockPos, state: BlockStateId) {
        self.set_rod_powered(pos, state, false);
    }

    pub(crate) fn lightning_rod_placed(&mut self, pos: BlockPos, state: BlockStateId) {
        if let Some(kind) = state.kind()
            && flag(state, "powered")
            && !self.has_scheduled(pos, kind)
        {
            self.schedule(pos, kind, PULSE);
        }
    }

    fn set_rod_powered(&mut self, pos: BlockPos, state: BlockStateId, powered: bool) {
        let Some(next) = state.with_property("powered", if powered { "true" } else { "false" })
        else {
            return;
        };
        self.set_block(pos, next);
        if let Some(behind) = facing(state) {
            self.update_neighbors_at(step(pos, behind.opposite()));
        }
    }
}
