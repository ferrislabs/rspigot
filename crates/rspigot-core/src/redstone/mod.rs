mod analog;
mod controls;
mod diode;
mod event;
mod lamp;
mod observer;
mod signals;
mod torch;
mod wire;

use rspigot_blocks::{BlockKind, Face, StateFlags};
use rspigot_domain::{BlockPos, BlockStateId};
use rustc_hash::FxHashMap;

use crate::ports::ClientLink;
use crate::server::Server;

pub(crate) use event::{RedstoneChange, RedstoneStep};
pub(crate) use signals::{connected_direction, is_conductor};

pub(crate) const MAX_POWER: i32 = 15;
pub(crate) const UPDATE_ORDER: [Face; 6] = [
    Face::West,
    Face::East,
    Face::Down,
    Face::Up,
    Face::North,
    Face::South,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i8)]
pub(crate) enum TickPriority {
    ExtremelyHigh = -3,
    VeryHigh = -2,
    High = -1,
    Normal = 0,
}

#[derive(Debug, Default)]
pub(crate) struct Runtime {
    pub(crate) toggles: Vec<(BlockPos, u64)>,
    pub(crate) outputs: FxHashMap<BlockPos, i32>,
    quiet: bool,
}

pub(crate) fn step(pos: BlockPos, face: Face) -> BlockPos {
    let (dx, dy, dz) = face.offset();
    pos.offset(dx, dy, dz)
}

pub(crate) fn face_named(name: &str) -> Option<Face> {
    Face::ALL.into_iter().find(|face| face.name() == name)
}

pub(crate) fn facing(state: BlockStateId) -> Option<Face> {
    face_named(state.property("facing")?)
}

pub(crate) fn attached(state: BlockStateId, clicked: Face, yaw: f32) -> BlockStateId {
    let Some(kind) = state.kind() else {
        return state;
    };
    if kind != BlockKind::Lever && !is_button(kind) {
        return state;
    }
    let looking = match ((f64::from(yaw) / 90.0 + 0.5).floor() as i64).rem_euclid(4) {
        0 => "south",
        1 => "west",
        2 => "north",
        _ => "east",
    };
    let (face, facing) = match clicked {
        Face::Up => ("floor", looking),
        Face::Down => ("ceiling", looking),
        side => ("wall", side.name()),
    };
    state
        .with_property("face", face)
        .and_then(|state| state.with_property("facing", facing))
        .unwrap_or(state)
}

pub(crate) fn is_button(kind: BlockKind) -> bool {
    kind.name().ends_with("_button")
}

pub(crate) fn is_plate(kind: BlockKind) -> bool {
    kind.name().ends_with("_pressure_plate")
}

fn is_source(state: BlockStateId) -> bool {
    state.flags().contains(StateFlags::SIGNAL_SOURCE)
        || state.kind() == Some(BlockKind::RedstoneWire)
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn update_neighbors_at(&mut self, pos: BlockPos) {
        for face in UPDATE_ORDER {
            self.neighbor_changed(step(pos, face), pos);
        }
    }

    pub(crate) fn update_neighbors_except(&mut self, pos: BlockPos, skip: Face) {
        for face in UPDATE_ORDER {
            if face != skip {
                self.neighbor_changed(step(pos, face), pos);
            }
        }
    }

    fn update_neighbors_ring(&mut self, pos: BlockPos) {
        self.update_neighbors_at(pos);
        for face in UPDATE_ORDER {
            self.update_neighbors_at(step(pos, face));
        }
    }

    pub(crate) fn set_block_quiet(&mut self, pos: BlockPos, state: BlockStateId) -> bool {
        self.falls.redstone.quiet = true;
        let changed = self.set_block(pos, state);
        self.falls.redstone.quiet = false;
        changed
    }

    pub(crate) fn neighbor_changed(&mut self, pos: BlockPos, _source: BlockPos) {
        let state = self.block_or_air(pos);
        match state.kind() {
            Some(BlockKind::RedstoneWire) => self.wire_neighbor_changed(pos, state),
            Some(BlockKind::RedstoneTorch | BlockKind::RedstoneWallTorch) => {
                self.torch_neighbor_changed(pos, state);
            }
            Some(BlockKind::RedstoneLamp) => self.lamp_neighbor_changed(pos, state),
            Some(BlockKind::Repeater | BlockKind::Comparator) => {
                self.diode_neighbor_changed(pos, state);
            }
            _ => {}
        }
    }

    pub(crate) fn redstone_block_changed(
        &mut self,
        pos: BlockPos,
        old: BlockStateId,
        new: BlockStateId,
    ) {
        if new.kind() == Some(BlockKind::RedstoneWire) && old.kind() != new.kind() {
            self.wire_placed(pos, new);
        }
        if old.kind() == Some(BlockKind::RedstoneWire)
            || new.kind() == Some(BlockKind::RedstoneWire)
        {
            self.refresh_diagonal_wires(pos);
        }
        if matches!(
            new.kind(),
            Some(BlockKind::Repeater | BlockKind::Comparator)
        ) && old.kind() != new.kind()
        {
            self.diode_placed(pos, new);
        }
        if old.kind() == Some(BlockKind::Comparator) && new.kind() != old.kind() {
            self.falls.redstone.outputs.remove(&pos);
        }
        if let Some(toward) = [new, old]
            .into_iter()
            .find(|state| diode::is_diode(*state) || state.kind() == Some(BlockKind::Observer))
            .and_then(facing)
        {
            self.update_neighbors_in_front(pos, toward);
            return;
        }
        if self.falls.redstone.quiet {
            return;
        }
        if is_source(old) || is_source(new) {
            self.update_neighbors_ring(pos);
        } else if old.kind() != new.kind() {
            self.update_neighbors_at(pos);
        }
    }

    pub(crate) fn redstone_tick(
        &mut self,
        pos: BlockPos,
        state: BlockStateId,
        kind: BlockKind,
    ) -> bool {
        match kind {
            BlockKind::RedstoneTorch | BlockKind::RedstoneWallTorch => self.torch_tick(pos, state),
            BlockKind::RedstoneLamp => self.lamp_tick(pos, state),
            BlockKind::Repeater | BlockKind::Comparator => self.diode_tick(pos, state),
            BlockKind::Observer => self.observer_tick(pos, state),
            _ if is_button(kind) => self.button_tick(pos, state),
            _ if is_plate(kind) => self.plate_tick(pos, state),
            _ => return false,
        }
        true
    }

    pub(crate) fn redstone_shape(&mut self, pos: BlockPos, state: BlockStateId, direction: Face) {
        match state.kind() {
            Some(BlockKind::RedstoneWire) => self.wire_shape(pos, state, direction),
            Some(BlockKind::Repeater) => self.repeater_shape(pos, state, direction),
            Some(BlockKind::Observer) => self.observer_shape(pos, state, direction),
            _ => {}
        }
    }

    pub(crate) fn redstone_placement(&self, pos: BlockPos, state: BlockStateId) -> BlockStateId {
        match state.kind() {
            Some(BlockKind::RedstoneWire) => self.wire_placement(pos, state),
            Some(BlockKind::RedstoneLamp) => self.lamp_placement(pos, state),
            Some(BlockKind::Repeater) => self.repeater_placement(pos, state),
            _ => state,
        }
    }
}
