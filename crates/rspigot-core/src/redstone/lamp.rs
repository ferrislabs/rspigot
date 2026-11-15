use rspigot_blocks::BlockKind;
use rspigot_domain::{BlockPos, BlockStateId};

use super::{MAX_POWER, RedstoneChange, RedstoneStep};
use crate::ports::ClientLink;
use crate::server::Server;
use crate::signal::flag;

const TURN_OFF_DELAY: u64 = 4;

impl<L: ClientLink> Server<L> {
    pub(crate) fn lamp_placement(&self, pos: BlockPos, state: BlockStateId) -> BlockStateId {
        let lit = if self.has_neighbor_signal(pos) {
            "true"
        } else {
            "false"
        };
        state.with_property("lit", lit).unwrap_or(state)
    }

    pub(crate) fn lamp_neighbor_changed(&mut self, pos: BlockPos, state: BlockStateId) {
        let lit = flag(state, "lit");
        if lit == self.has_neighbor_signal(pos) {
            return;
        }
        if lit {
            self.schedule(pos, BlockKind::RedstoneLamp, TURN_OFF_DELAY);
        } else {
            self.ask_redstone(RedstoneStep {
                pos,
                old: 0,
                new: MAX_POWER,
                change: RedstoneChange::Lamp { lit: true },
            });
        }
    }

    pub(crate) fn lamp_tick(&mut self, pos: BlockPos, state: BlockStateId) {
        if flag(state, "lit") && !self.has_neighbor_signal(pos) {
            self.ask_redstone(RedstoneStep {
                pos,
                old: MAX_POWER,
                new: 0,
                change: RedstoneChange::Lamp { lit: false },
            });
        }
    }

    pub(crate) fn finish_lamp(&mut self, pos: BlockPos, new: i32, lit: bool) {
        let state = self.block_or_air(pos);
        if state.kind() != Some(BlockKind::RedstoneLamp) || new != if lit { MAX_POWER } else { 0 } {
            return;
        }
        if let Some(next) = state.with_property("lit", if lit { "true" } else { "false" }) {
            self.set_block_quiet(pos, next);
        }
    }
}
