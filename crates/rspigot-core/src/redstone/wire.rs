use rspigot_blocks::{BlockKind, Face, Support};
use rspigot_domain::{BlockPos, BlockStateId};

use super::{MAX_POWER, RedstoneChange, RedstoneStep, UPDATE_ORDER, facing, is_conductor, step};
use crate::ports::ClientLink;
use crate::server::Server;
use crate::signal::level;

const WIRE: BlockKind = BlockKind::RedstoneWire;

fn power_of(state: BlockStateId) -> i32 {
    if state.kind() == Some(WIRE) {
        level(state, "power")
    } else {
        0
    }
}

fn connected(state: BlockStateId, face: Face) -> bool {
    state
        .property(face.name())
        .is_some_and(|side| side != "none")
}

fn is_dot(state: BlockStateId) -> bool {
    Face::HORIZONTAL.iter().all(|face| !connected(state, *face))
}

fn is_cross(state: BlockStateId) -> bool {
    Face::HORIZONTAL.iter().all(|face| connected(state, *face))
}

fn with_side(state: BlockStateId, face: Face, value: &str) -> BlockStateId {
    state.with_property(face.name(), value).unwrap_or(state)
}

fn with_power(state: BlockStateId, power: i32) -> BlockStateId {
    state
        .with_property("power", &power.to_string())
        .unwrap_or(state)
}

fn can_survive_on(state: BlockStateId) -> bool {
    state.is_face_sturdy(Face::Up, Support::Full) || state.kind() == Some(BlockKind::Hopper)
}

fn should_connect(state: BlockStateId, direction: Option<Face>) -> bool {
    match state.kind() {
        Some(WIRE) => true,
        Some(BlockKind::Repeater) => {
            let facing = facing(state);
            direction.is_some() && (facing == direction || facing == direction.map(Face::opposite))
        }
        Some(BlockKind::Observer) => direction.is_some() && facing(state) == direction,
        _ => {
            direction.is_some()
                && state
                    .flags()
                    .contains(rspigot_blocks::StateFlags::SIGNAL_SOURCE)
        }
    }
}

impl<L: ClientLink> Server<L> {
    fn connecting_side(&self, pos: BlockPos, direction: Face, climb: bool) -> &'static str {
        let side = step(pos, direction);
        let state = self.block_or_air(side);
        if climb {
            let trapdoor = state
                .kind()
                .is_some_and(|kind| kind.name().ends_with("_trapdoor"));
            let above = self.block_or_air(step(side, Face::Up));
            if (trapdoor || can_survive_on(state)) && should_connect(above, None) {
                return if state.is_face_sturdy(direction.opposite(), Support::Full) {
                    "up"
                } else {
                    "side"
                };
            }
        }
        let below = self.block_or_air(step(side, Face::Down));
        if should_connect(state, Some(direction))
            || (!is_conductor(state) && should_connect(below, None))
        {
            "side"
        } else {
            "none"
        }
    }

    fn missing_connections(&self, pos: BlockPos, state: BlockStateId) -> BlockStateId {
        let climb = !is_conductor(self.block_or_air(step(pos, Face::Up)));
        let mut state = state;
        for face in Face::HORIZONTAL {
            if !connected(state, face) {
                state = with_side(state, face, self.connecting_side(pos, face, climb));
            }
        }
        state
    }

    fn connection_state(&self, pos: BlockPos, state: BlockStateId) -> BlockStateId {
        let dot = is_dot(state);
        let base = with_power(WIRE.default_state(), power_of(state));
        let mut state = self.missing_connections(pos, base);
        if dot && is_dot(state) {
            return state;
        }
        let north = connected(state, Face::North);
        let south = connected(state, Face::South);
        let east = connected(state, Face::East);
        let west = connected(state, Face::West);
        let no_line_ns = !north && !south;
        let no_line_ew = !east && !west;
        if !west && no_line_ns {
            state = with_side(state, Face::West, "side");
        }
        if !east && no_line_ns {
            state = with_side(state, Face::East, "side");
        }
        if !north && no_line_ew {
            state = with_side(state, Face::North, "side");
        }
        if !south && no_line_ew {
            state = with_side(state, Face::South, "side");
        }
        state
    }

    pub(crate) fn toggle_wire(
        &mut self,
        id: rspigot_domain::player::PlayerId,
        pos: BlockPos,
        state: BlockStateId,
    ) -> bool {
        use rspigot_domain::player::GameMode;
        let may_build = self.players.get(&id).is_some_and(|player| {
            !matches!(player.game_mode, GameMode::Adventure | GameMode::Spectator)
        });
        if !may_build || !(is_dot(state) || is_cross(state)) {
            return false;
        }
        let power = power_of(state);
        let toggled = if is_cross(state) {
            with_power(WIRE.default_state(), power)
        } else {
            Face::HORIZONTAL
                .iter()
                .fold(with_power(WIRE.default_state(), power), |next, face| {
                    with_side(next, *face, "side")
                })
        };
        let next = self.connection_state(pos, toggled);
        if next == state {
            return false;
        }
        self.set_block(pos, next);
        true
    }

    pub(crate) fn wire_placement(&self, pos: BlockPos, state: BlockStateId) -> BlockStateId {
        self.connection_state(pos, state)
    }

    pub(crate) fn wire_shape(&mut self, pos: BlockPos, state: BlockStateId, direction: Face) {
        let next = match direction {
            Face::Down => state,
            Face::Up => self.connection_state(pos, state),
            side => {
                let climb = !is_conductor(self.block_or_air(step(pos, Face::Up)));
                let shape = self.connecting_side(pos, side, climb);
                if (shape != "none") == connected(state, side) && !is_cross(state) {
                    with_side(state, side, shape)
                } else {
                    let base = with_power(WIRE.default_state(), power_of(state));
                    self.connection_state(pos, with_side(base, side, shape))
                }
            }
        };
        if next != state {
            self.set_block(pos, next);
        }
    }

    pub(crate) fn refresh_diagonal_wires(&mut self, pos: BlockPos) {
        for face in Face::HORIZONTAL {
            let (dx, _, dz) = face.offset();
            for dy in [-1, 1] {
                let at = pos.offset(dx, dy, dz);
                let state = self.block_or_air(at);
                if state.kind() == Some(WIRE) {
                    let shaped = self.connection_state(at, state);
                    if shaped != state {
                        self.set_block(at, shaped);
                    }
                }
            }
        }
    }

    pub(crate) fn wire_placed(&mut self, pos: BlockPos, state: BlockStateId) {
        let shaped = self.connection_state(pos, state);
        if shaped != state {
            self.set_block_quiet(pos, shaped);
        }
        let current = self.block_or_air(pos);
        self.update_power_strength(pos, current);
    }

    pub(crate) fn wire_neighbor_changed(&mut self, pos: BlockPos, state: BlockStateId) {
        if can_survive_on(self.block_or_air(step(pos, Face::Down))) {
            self.update_power_strength(pos, state);
        } else {
            self.destroy_block(pos, state);
        }
    }

    pub(crate) fn wire_target(&self, pos: BlockPos) -> i32 {
        let from_blocks = self.best_neighbor_signal(pos, false);
        if from_blocks >= MAX_POWER {
            return MAX_POWER;
        }
        let covered = is_conductor(self.block_or_air(step(pos, Face::Up)));
        let mut incoming = 0;
        for face in Face::HORIZONTAL {
            let side = step(pos, face);
            let state = self.block_or_air(side);
            incoming = incoming.max(power_of(state));
            if is_conductor(state) {
                if !covered {
                    incoming = incoming.max(power_of(self.block_or_air(step(side, Face::Up))));
                }
            } else {
                incoming = incoming.max(power_of(self.block_or_air(step(side, Face::Down))));
            }
        }
        from_blocks.max(incoming - 1)
    }

    fn update_power_strength(&mut self, pos: BlockPos, state: BlockStateId) {
        if state.kind() != Some(WIRE) {
            return;
        }
        let target = self.wire_target(pos);
        let current = power_of(state);
        if target != current {
            self.ask_redstone(RedstoneStep {
                pos,
                old: current,
                new: target,
                change: RedstoneChange::Wire,
            });
        }
    }

    pub(crate) fn finish_wire(&mut self, pos: BlockPos, old: i32, new: i32) {
        let new = new.clamp(0, MAX_POWER);
        if old == new {
            return;
        }
        let current = self.block_or_air(pos);
        if current.kind() == Some(WIRE) {
            self.set_block_quiet(pos, with_power(current, new));
        }
        self.update_neighbors_at(pos);
        for face in UPDATE_ORDER {
            self.update_neighbors_at(step(pos, face));
        }
    }
}
