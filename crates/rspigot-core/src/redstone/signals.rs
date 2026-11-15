use rspigot_blocks::{BlockKind, Face, StateFlags};
use rspigot_domain::{BlockPos, BlockStateId};

use super::{MAX_POWER, facing, is_button, is_plate, step};
use crate::ports::ClientLink;
use crate::server::Server;
use crate::signal::{flag, level, signal};

pub(crate) fn is_conductor(state: BlockStateId) -> bool {
    state.flags().contains(StateFlags::REDSTONE_CONDUCTOR)
}

pub(crate) fn connected_direction(state: BlockStateId) -> Face {
    match state.property("face") {
        Some("ceiling") => Face::Down,
        Some("floor") => Face::Up,
        _ => facing(state).unwrap_or(Face::North),
    }
}

fn wire_signal(state: BlockStateId, direction: Face, wires: bool) -> i32 {
    let power = level(state, "power");
    if !wires || direction == Face::Down || power == 0 {
        return 0;
    }
    let connected = direction == Face::Up
        || state
            .property(direction.opposite().name())
            .is_some_and(|side| side != "none");
    if connected { power } else { 0 }
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn state_signal(
        &self,
        state: BlockStateId,
        pos: BlockPos,
        direction: Face,
        wires: bool,
    ) -> i32 {
        match state.kind() {
            Some(BlockKind::RedstoneWire) => wire_signal(state, direction, wires),
            Some(BlockKind::Comparator) => {
                if flag(state, "powered") && facing(state) == Some(direction) {
                    self.falls.redstone.outputs.get(&pos).copied().unwrap_or(0)
                } else {
                    0
                }
            }
            _ => signal(state, direction),
        }
    }

    pub(crate) fn state_direct_signal(
        &self,
        state: BlockStateId,
        pos: BlockPos,
        direction: Face,
        wires: bool,
    ) -> i32 {
        let Some(kind) = state.kind() else {
            return 0;
        };
        match kind {
            BlockKind::RedstoneTorch | BlockKind::RedstoneWallTorch => {
                if direction == Face::Down {
                    self.state_signal(state, pos, direction, wires)
                } else {
                    0
                }
            }
            BlockKind::Lever => {
                if flag(state, "powered") && connected_direction(state) == direction {
                    MAX_POWER
                } else {
                    0
                }
            }
            BlockKind::TripwireHook => {
                if flag(state, "powered") && facing(state) == Some(direction) {
                    MAX_POWER
                } else {
                    0
                }
            }
            BlockKind::DetectorRail | BlockKind::Lectern => {
                if flag(state, "powered") && direction == Face::Up {
                    MAX_POWER
                } else {
                    0
                }
            }
            BlockKind::Observer | BlockKind::Repeater | BlockKind::Comparator => {
                self.state_signal(state, pos, direction, wires)
            }
            BlockKind::RedstoneWire => {
                if wires {
                    self.state_signal(state, pos, direction, wires)
                } else {
                    0
                }
            }
            _ if is_button(kind) => {
                if flag(state, "powered") && connected_direction(state) == direction {
                    MAX_POWER
                } else {
                    0
                }
            }
            _ if is_plate(kind) && direction == Face::Up => signal(state, direction),
            _ => 0,
        }
    }

    pub(crate) fn direct_signal_to(&self, pos: BlockPos, wires: bool) -> i32 {
        let mut best = 0;
        for face in Face::ALL {
            let at = step(pos, face);
            let state = self.block_or_air(at);
            best = best.max(self.state_direct_signal(state, at, face, wires));
            if best >= MAX_POWER {
                return best;
            }
        }
        best
    }

    pub(crate) fn signal_toward(&self, pos: BlockPos, direction: Face, wires: bool) -> i32 {
        let state = self.block_or_air(pos);
        let own = self.state_signal(state, pos, direction, wires);
        if is_conductor(state) {
            own.max(self.direct_signal_to(pos, wires))
        } else {
            own
        }
    }

    pub(crate) fn has_signal(&self, pos: BlockPos, direction: Face) -> bool {
        self.signal_toward(pos, direction, true) > 0
    }

    pub(crate) fn best_neighbor_signal(&self, pos: BlockPos, wires: bool) -> i32 {
        let mut best = 0;
        for face in Face::ALL {
            best = best.max(self.signal_toward(step(pos, face), face, wires));
            if best >= MAX_POWER {
                return best;
            }
        }
        best
    }

    pub(crate) fn has_neighbor_signal(&self, pos: BlockPos) -> bool {
        Face::ALL
            .into_iter()
            .any(|face| self.has_signal(step(pos, face), face))
    }
}
