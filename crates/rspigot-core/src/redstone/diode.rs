use rspigot_blocks::{BlockKind, Face, Support};
use rspigot_domain::player::{GameMode, PlayerId};
use rspigot_domain::{BlockPos, BlockStateId};
use rspigot_entities::SoundSource;

use super::{MAX_POWER, RedstoneChange, RedstoneStep, TickPriority, facing, is_conductor, step};
use crate::ports::ClientLink;
use crate::server::Server;
use crate::signal::{flag, level};

const COMPARATOR_DELAY: u64 = 2;
const COMPARATOR_CLICK: &str = "minecraft:block.comparator.click";
const SUBTRACT_PITCH: f32 = 0.55;
const COMPARE_PITCH: f32 = 0.5;
const CLICK_VOLUME: f32 = 0.3;

pub(crate) fn is_diode(state: BlockStateId) -> bool {
    matches!(
        state.kind(),
        Some(BlockKind::Repeater | BlockKind::Comparator)
    )
}

fn clockwise(face: Face) -> Face {
    match face {
        Face::North => Face::East,
        Face::East => Face::South,
        Face::South => Face::West,
        _ => Face::North,
    }
}

fn counter_clockwise(face: Face) -> Face {
    match face {
        Face::North => Face::West,
        Face::West => Face::South,
        Face::South => Face::East,
        _ => Face::North,
    }
}

fn subtracts(state: BlockStateId) -> bool {
    state.property("mode") == Some("subtract")
}

fn turned(state: BlockStateId, on: bool) -> BlockStateId {
    state
        .with_property("powered", if on { "true" } else { "false" })
        .unwrap_or(state)
}

impl<L: ClientLink> Server<L> {
    fn diode_input(&self, pos: BlockPos, state: BlockStateId) -> i32 {
        let Some(toward) = facing(state) else {
            return 0;
        };
        let at = step(pos, toward);
        let signal = self.signal_toward(at, toward, true);
        if signal >= MAX_POWER {
            return signal;
        }
        let wire = self.block_or_air(at);
        if wire.kind() == Some(BlockKind::RedstoneWire) {
            signal.max(level(wire, "power"))
        } else {
            signal
        }
    }

    fn comparator_input(&self, pos: BlockPos, state: BlockStateId) -> i32 {
        let input = self.diode_input(pos, state);
        let Some(toward) = facing(state) else {
            return input;
        };
        let at = step(pos, toward);
        let block = self.block_or_air(at);
        if let Some(analog) = self.analog_signal(at, block) {
            return analog;
        }
        if input < MAX_POWER && is_conductor(block) {
            let beyond = step(at, toward);
            if let Some(analog) = self.analog_signal(beyond, self.block_or_air(beyond)) {
                return analog;
            }
        }
        input
    }

    fn control_input(&self, pos: BlockPos, direction: Face, diodes_only: bool) -> i32 {
        let state = self.block_or_air(pos);
        if diodes_only {
            return if is_diode(state) {
                self.state_direct_signal(state, pos, direction, true)
            } else {
                0
            };
        }
        match state.kind() {
            Some(BlockKind::RedstoneBlock) => MAX_POWER,
            Some(BlockKind::RedstoneWire) => level(state, "power"),
            _ if state
                .flags()
                .contains(rspigot_blocks::StateFlags::SIGNAL_SOURCE) =>
            {
                self.state_direct_signal(state, pos, direction, true)
            }
            _ => 0,
        }
    }

    fn alternate_signal(&self, pos: BlockPos, state: BlockStateId) -> i32 {
        let Some(toward) = facing(state) else {
            return 0;
        };
        let diodes_only = state.kind() == Some(BlockKind::Repeater);
        [clockwise(toward), counter_clockwise(toward)]
            .into_iter()
            .map(|side| self.control_input(step(pos, side), side, diodes_only))
            .max()
            .unwrap_or(0)
    }

    pub(crate) fn repeater_locked(&self, pos: BlockPos, state: BlockStateId) -> bool {
        self.alternate_signal(pos, state) > 0
    }

    fn diode_locked(&self, pos: BlockPos, state: BlockStateId) -> bool {
        state.kind() == Some(BlockKind::Repeater) && self.repeater_locked(pos, state)
    }

    fn comparator_output(&self, pos: BlockPos, state: BlockStateId) -> i32 {
        let input = self.comparator_input(pos, state);
        if input == 0 {
            return 0;
        }
        let side = self.alternate_signal(pos, state);
        if input < side {
            0
        } else if subtracts(state) {
            input - side
        } else {
            input
        }
    }

    fn diode_should_turn_on(&self, pos: BlockPos, state: BlockStateId) -> bool {
        if state.kind() != Some(BlockKind::Comparator) {
            return self.diode_input(pos, state) > 0;
        }
        let input = self.comparator_input(pos, state);
        if input == 0 {
            return false;
        }
        let side = self.alternate_signal(pos, state);
        input > side || (input == side && !subtracts(state))
    }

    fn diode_delay(&self, state: BlockStateId) -> u64 {
        if state.kind() == Some(BlockKind::Repeater) {
            level(state, "delay") as u64 * 2
        } else {
            COMPARATOR_DELAY
        }
    }

    fn should_prioritize(&self, pos: BlockPos, state: BlockStateId) -> bool {
        let Some(output) = facing(state).map(Face::opposite) else {
            return false;
        };
        let front = self.block_or_air(step(pos, output));
        is_diode(front) && facing(front) != Some(output)
    }

    pub(crate) fn update_neighbors_in_front(&mut self, pos: BlockPos, toward: Face) {
        let front = step(pos, toward.opposite());
        self.neighbor_changed(front, pos);
        self.update_neighbors_except(front, toward);
    }

    pub(crate) fn diode_neighbor_changed(&mut self, pos: BlockPos, state: BlockStateId) {
        let supported = self
            .block_or_air(step(pos, Face::Down))
            .is_face_sturdy(Face::Up, Support::Rigid);
        if !supported {
            self.destroy_block(pos, state);
            for face in Face::ALL {
                self.update_neighbors_at(step(pos, face));
            }
            return;
        }
        self.check_tick_on_neighbor(pos, state);
    }

    fn check_tick_on_neighbor(&mut self, pos: BlockPos, state: BlockStateId) {
        let Some(kind) = state.kind() else {
            return;
        };
        if self.diode_locked(pos, state) || self.will_tick_this_tick(pos, kind) {
            return;
        }
        let powered = flag(state, "powered");
        let wanted = self.diode_should_turn_on(pos, state);
        let prioritize = self.should_prioritize(pos, state);
        if kind == BlockKind::Comparator {
            let known = self.falls.redstone.outputs.get(&pos).copied().unwrap_or(0);
            if self.comparator_output(pos, state) != known || powered != wanted {
                let priority = if prioritize {
                    TickPriority::ExtremelyHigh
                } else {
                    TickPriority::High
                };
                self.schedule_with(pos, kind, COMPARATOR_DELAY, priority);
            }
        } else if powered != wanted {
            let priority = if prioritize {
                TickPriority::ExtremelyHigh
            } else if powered {
                TickPriority::VeryHigh
            } else {
                TickPriority::High
            };
            self.schedule_with(pos, kind, self.diode_delay(state), priority);
        }
    }

    pub(crate) fn diode_placed(&mut self, pos: BlockPos, state: BlockStateId) {
        if let Some(kind) = state.kind()
            && self.diode_should_turn_on(pos, state)
        {
            self.schedule(pos, kind, 1);
        }
    }

    pub(crate) fn repeater_placement(&self, pos: BlockPos, state: BlockStateId) -> BlockStateId {
        let locked = if self.repeater_locked(pos, state) {
            "true"
        } else {
            "false"
        };
        state.with_property("locked", locked).unwrap_or(state)
    }

    pub(crate) fn repeater_shape(&mut self, pos: BlockPos, state: BlockStateId, direction: Face) {
        let along = facing(state)
            .is_some_and(|toward| toward == direction || toward == direction.opposite());
        if along {
            return;
        }
        let locked = self.repeater_locked(pos, state);
        if locked != flag(state, "locked")
            && let Some(next) = state.with_property("locked", if locked { "true" } else { "false" })
        {
            self.set_block_quiet(pos, next);
        }
    }

    pub(crate) fn diode_tick(&mut self, pos: BlockPos, state: BlockStateId) {
        match state.kind() {
            Some(BlockKind::Repeater) => self.repeater_tick(pos, state),
            Some(BlockKind::Comparator) => self.refresh_comparator(pos, state),
            _ => {}
        }
    }

    fn repeater_tick(&mut self, pos: BlockPos, state: BlockStateId) {
        if self.diode_locked(pos, state) {
            return;
        }
        let powered = flag(state, "powered");
        let wanted = self.diode_should_turn_on(pos, state);
        if powered && !wanted {
            self.ask_redstone(RedstoneStep {
                pos,
                old: MAX_POWER,
                new: 0,
                change: RedstoneChange::Diode { on: false },
            });
        } else if !powered {
            self.ask_redstone(RedstoneStep {
                pos,
                old: 0,
                new: MAX_POWER,
                change: RedstoneChange::Diode { on: true },
            });
        }
    }

    fn refresh_comparator(&mut self, pos: BlockPos, state: BlockStateId) {
        let output = self.comparator_output(pos, state);
        let known = self.falls.redstone.outputs.insert(pos, output).unwrap_or(0);
        if known == output && subtracts(state) {
            return;
        }
        let powered = flag(state, "powered");
        let wanted = self.diode_should_turn_on(pos, state);
        if powered && !wanted {
            self.ask_redstone(RedstoneStep {
                pos,
                old: MAX_POWER,
                new: 0,
                change: RedstoneChange::Diode { on: false },
            });
        } else if !powered && wanted {
            self.ask_redstone(RedstoneStep {
                pos,
                old: 0,
                new: MAX_POWER,
                change: RedstoneChange::Diode { on: true },
            });
        } else if let Some(toward) = facing(state) {
            self.update_neighbors_in_front(pos, toward);
        }
    }

    pub(crate) fn finish_diode(&mut self, pos: BlockPos, new: i32, on: bool) {
        if new != if on { MAX_POWER } else { 0 } {
            return;
        }
        let state = self.block_or_air(pos);
        let Some(kind) = state.kind().filter(|_| is_diode(state)) else {
            return;
        };
        if flag(state, "powered") == on {
            return;
        }
        self.set_block(pos, turned(state, on));
        if kind == BlockKind::Repeater && on && !self.diode_should_turn_on(pos, state) {
            self.schedule_with(pos, kind, self.diode_delay(state), TickPriority::VeryHigh);
        }
        if kind == BlockKind::Comparator
            && let Some(toward) = facing(state)
        {
            self.update_neighbors_in_front(pos, toward);
        }
    }

    pub(crate) fn use_diode(&mut self, id: PlayerId, pos: BlockPos, state: BlockStateId) -> bool {
        let builds = self.players.get(&id).is_some_and(|player| {
            !matches!(player.game_mode, GameMode::Adventure | GameMode::Spectator)
        });
        if !builds {
            return false;
        }
        if state.kind() == Some(BlockKind::Repeater) {
            let delay = level(state, "delay") % 4 + 1;
            if let Some(next) = state.with_property("delay", &delay.to_string()) {
                self.set_block(pos, next);
            }
            return true;
        }
        let mode = if subtracts(state) {
            "compare"
        } else {
            "subtract"
        };
        let Some(next) = state.with_property("mode", mode) else {
            return false;
        };
        let pitch = if mode == "subtract" {
            SUBTRACT_PITCH
        } else {
            COMPARE_PITCH
        };
        self.play_sound(
            Some(id),
            pos.center(),
            COMPARATOR_CLICK,
            SoundSource::Blocks,
            CLICK_VOLUME,
            pitch,
        );
        self.set_block_quiet(pos, next);
        self.refresh_comparator(pos, next);
        true
    }
}
