mod analog;
mod bell;
mod controls;
mod crafter;
mod daylight;
mod diode;
mod dispenser;
mod event;
mod hopper;
mod lamp;
mod lightning_rod;
mod note;
mod observer;
mod openers;
mod piston;
mod rails;
mod reactions;
mod resolver;
mod signals;
mod storage;
mod target;
mod torch;
mod tripwire;
mod wire;

use rspigot_blocks::{BlockKind, Face, StateFlags};
use rspigot_domain::{BlockPos, BlockStateId};
use rustc_hash::FxHashMap;

use crate::ports::ClientLink;
use crate::server::Server;

pub(crate) use bell::BellStep;
pub(crate) use crafter::CrafterStep;
pub(crate) use dispenser::DispenseStep;
pub(crate) use event::{RedstoneChange, RedstoneStep};
pub(crate) use hopper::{HopperStep, Stage as HopperStage};
pub(crate) use note::NoteStep;
pub(crate) use piston::PistonStep;
pub(crate) use signals::{connected_direction, is_conductor};
pub(crate) use tripwire::HookRun;

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
    pub(crate) moving: FxHashMap<BlockPos, piston::Moving>,
    pub(crate) events: Vec<piston::BlockEvent>,
    pub(crate) hoppers: FxHashMap<BlockPos, hopper::Clock>,
    pub(crate) bells: FxHashMap<BlockPos, bell::Ringing>,
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
    if kind == BlockKind::Hopper {
        let facing = match clicked.opposite() {
            Face::Up => "down",
            side => side.name(),
        };
        return state.with_property("facing", facing).unwrap_or(state);
    }
    if kind != BlockKind::Lever && !is_button(kind) && kind != BlockKind::TripwireHook {
        return state;
    }
    let looking = match ((f64::from(yaw) / 90.0 + 0.5).floor() as i64).rem_euclid(4) {
        0 => "south",
        1 => "west",
        2 => "north",
        _ => "east",
    };
    if kind == BlockKind::TripwireHook {
        let toward = if clicked.is_horizontal() {
            clicked.name()
        } else {
            face_named(looking).map_or("north", |face| face.opposite().name())
        };
        return state.with_property("facing", toward).unwrap_or(state);
    }
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

    pub(crate) fn neighbor_changed(&mut self, pos: BlockPos, source: BlockPos) {
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
            Some(BlockKind::Piston | BlockKind::StickyPiston) => {
                self.piston_neighbor_changed(pos, state);
            }
            Some(BlockKind::PistonHead) => self.piston_head_neighbor_changed(pos, state),
            Some(BlockKind::Hopper) => self.hopper_neighbor_changed(pos, state),
            Some(BlockKind::Tnt) => self.tnt_neighbor_changed(pos, source),
            Some(BlockKind::NoteBlock) => self.note_neighbor_changed(pos, state),
            Some(BlockKind::Bell) => self.bell_neighbor_changed(pos, state),
            Some(BlockKind::Crafter) => self.crafter_neighbor_changed(pos, state),
            Some(BlockKind::PoweredRail | BlockKind::ActivatorRail) => {
                self.powered_rail_changed(pos, state);
            }
            Some(_) if openers::is_opener(state) => {
                self.opener_neighbor_changed(pos, state);
            }
            Some(BlockKind::Dispenser | BlockKind::Dropper) => {
                self.dispenser_neighbor_changed(pos, state);
            }
            Some(BlockKind::MovingPiston) if !self.falls.redstone.moving.contains_key(&pos) => {
                self.schedule(pos, BlockKind::MovingPiston, 1);
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
        if new.kind() == Some(BlockKind::Hopper) && old.kind() != new.kind() {
            self.hopper_neighbor_changed(pos, new);
        }
        if old.kind() != new.kind() {
            match new.kind() {
                Some(BlockKind::PoweredRail | BlockKind::ActivatorRail) => {
                    self.powered_rail_changed(pos, new);
                }
                Some(BlockKind::DetectorRail) => self.detector_check(pos),
                _ => {}
            }
        }
        if new.kind().is_some_and(lightning_rod::is_lightning_rod) && old.kind() != new.kind() {
            self.lightning_rod_placed(pos, new);
        }
        self.tripwire_changed(pos, old, new);
        if new.kind() == Some(BlockKind::Target) && old.kind() != new.kind() {
            self.target_placed(pos, new);
        }
        if new.kind() == Some(BlockKind::Tnt) && old.kind() != new.kind() {
            self.tnt_neighbor_changed(pos, pos);
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
            BlockKind::Dispenser | BlockKind::Dropper => self.dispenser_tick(pos, state),
            BlockKind::DetectorRail => self.detector_tick(pos, state),
            BlockKind::Lectern => self.lectern_tick(pos),
            BlockKind::Target => self.target_tick(pos, state),
            BlockKind::TripwireHook => self.hook_tick(pos, state),
            BlockKind::Tripwire => self.tripwire_tick(pos, state),
            kind if lightning_rod::is_lightning_rod(kind) => self.lightning_rod_tick(pos, state),
            BlockKind::Crafter => self.crafter_tick(pos, state),
            BlockKind::MovingPiston => self.moving_tick(pos),
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
            Some(BlockKind::PistonHead) => self.piston_head_shape(pos, state, direction),
            Some(BlockKind::NoteBlock) => self.note_shape(pos, state, direction),
            Some(BlockKind::Lever) => self.attached_shape(pos, state, direction),
            Some(BlockKind::Tripwire) => self.tripwire_shape(pos, state, direction),
            Some(BlockKind::TripwireHook) => self.hook_shape(pos, state, direction),
            Some(kind) if is_button(kind) => self.attached_shape(pos, state, direction),
            _ => {}
        }
    }

    pub(crate) fn redstone_placement(&self, pos: BlockPos, state: BlockStateId) -> BlockStateId {
        match state.kind() {
            Some(BlockKind::RedstoneWire) => self.wire_placement(pos, state),
            Some(BlockKind::RedstoneLamp) => self.lamp_placement(pos, state),
            Some(BlockKind::Repeater) => self.repeater_placement(pos, state),
            Some(BlockKind::NoteBlock) => self.note_placement(pos, state),
            Some(BlockKind::Tripwire) => self.tripwire_placement(pos, state),
            Some(_) if openers::is_opener(state) => self.opener_placement(pos, state),
            _ => state,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hopper_faces_away_from_the_clicked_side_and_down_from_the_top() {
        let hopper = BlockKind::Hopper.default_state();
        assert_eq!(
            attached(hopper, Face::North, 0.0).property("facing"),
            Some("south")
        );
        assert_eq!(
            attached(hopper, Face::Up, 0.0).property("facing"),
            Some("down")
        );
        assert_eq!(
            attached(hopper, Face::Down, 0.0).property("facing"),
            Some("down")
        );
    }
}
