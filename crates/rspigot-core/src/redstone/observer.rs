use rspigot_blocks::{BlockKind, Face};
use rspigot_domain::{BlockPos, BlockStateId};

use super::{MAX_POWER, RedstoneChange, RedstoneStep, facing};
use crate::ports::ClientLink;
use crate::server::Server;
use crate::signal::flag;

const PULSE_DELAY: u64 = 2;

impl<L: ClientLink> Server<L> {
    pub(crate) fn observer_shape(&mut self, pos: BlockPos, state: BlockStateId, direction: Face) {
        if facing(state) == Some(direction)
            && !flag(state, "powered")
            && !self.has_scheduled(pos, BlockKind::Observer)
        {
            self.schedule(pos, BlockKind::Observer, PULSE_DELAY);
        }
    }

    pub(crate) fn observer_tick(&mut self, pos: BlockPos, state: BlockStateId) {
        let on = !flag(state, "powered");
        self.ask_redstone(RedstoneStep {
            pos,
            old: if on { 0 } else { MAX_POWER },
            new: if on { MAX_POWER } else { 0 },
            change: RedstoneChange::Observer { on },
        });
    }

    pub(crate) fn finish_observer(&mut self, pos: BlockPos, new: i32, on: bool) {
        if new != if on { MAX_POWER } else { 0 } {
            return;
        }
        let state = self.block_or_air(pos);
        if state.kind() != Some(BlockKind::Observer) || flag(state, "powered") == on {
            return;
        }
        let Some(next) = state.with_property("powered", if on { "true" } else { "false" }) else {
            return;
        };
        self.set_block(pos, next);
        if on {
            self.schedule(pos, BlockKind::Observer, PULSE_DELAY);
        }
    }
}
