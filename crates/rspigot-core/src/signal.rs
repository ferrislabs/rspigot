use rspigot_blocks::{BlockKind, Face};
use rspigot_domain::{BlockPos, BlockStateId};

use crate::ports::ClientLink;
use crate::server::Server;

const MAX_SIGNAL: i32 = 15;

fn face_name(face: Face) -> &'static str {
    match face {
        Face::Down => "down",
        Face::Up => "up",
        Face::North => "north",
        Face::South => "south",
        Face::West => "west",
        Face::East => "east",
    }
}

fn flag(state: BlockStateId, name: &str) -> bool {
    state.property(name) == Some("true")
}

fn level(state: BlockStateId, name: &str) -> i32 {
    state
        .property(name)
        .and_then(|value| value.parse().ok())
        .unwrap_or(0)
}

fn faces(state: BlockStateId, direction: Face) -> bool {
    state.property("facing") == Some(face_name(direction))
}

pub(crate) fn signal(state: BlockStateId, direction: Face) -> i32 {
    let Some(kind) = state.kind() else {
        return 0;
    };
    let name = kind.name();
    match kind {
        BlockKind::RedstoneBlock => MAX_SIGNAL,
        BlockKind::RedstoneTorch if flag(state, "lit") && direction != Face::Up => MAX_SIGNAL,
        BlockKind::RedstoneWallTorch if flag(state, "lit") && !faces(state, direction) => {
            MAX_SIGNAL
        }
        BlockKind::Lever if flag(state, "powered") => MAX_SIGNAL,
        BlockKind::Observer | BlockKind::TripwireHook
            if flag(state, "powered") && faces(state, direction) =>
        {
            MAX_SIGNAL
        }
        BlockKind::DaylightDetector => level(state, "power"),
        _ if name.ends_with("_button") && flag(state, "powered") => MAX_SIGNAL,
        _ if name.ends_with("_pressure_plate") => {
            if name.contains("_weighted_") {
                level(state, "power")
            } else if flag(state, "powered") {
                MAX_SIGNAL
            } else {
                0
            }
        }
        _ => 0,
    }
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn best_own_or_neighbour_signal(&self, pos: BlockPos) -> i32 {
        let mut best = 0;
        for face in Face::ALL {
            let (dx, dy, dz) = face.offset();
            let Some(state) = self.world.chunks.block(pos.offset(dx, dy, dz)) else {
                continue;
            };
            let power = signal(state, face);
            if power >= MAX_SIGNAL {
                return MAX_SIGNAL;
            }
            best = best.max(power);
        }
        best
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(kind: BlockKind) -> BlockStateId {
        kind.default_state()
    }

    #[test]
    fn a_redstone_block_powers_every_side() {
        for face in Face::ALL {
            assert_eq!(signal(state(BlockKind::RedstoneBlock), face), 15);
        }
    }

    #[test]
    fn an_unpowered_lever_and_plain_blocks_give_no_signal() {
        assert_eq!(signal(state(BlockKind::Lever), Face::Up), 0);
        assert_eq!(signal(state(BlockKind::Stone), Face::Up), 0);
    }

    #[test]
    fn a_lit_torch_powers_everything_but_the_block_it_stands_under() {
        let torch = state(BlockKind::RedstoneTorch);
        assert_eq!(signal(torch, Face::Down), 15);
        assert_eq!(signal(torch, Face::North), 15);
        assert_eq!(signal(torch, Face::Up), 0);
    }

    #[test]
    fn a_powered_lever_and_pressure_plate_give_fifteen() {
        let lever = state(BlockKind::Lever)
            .with_property("powered", "true")
            .unwrap();
        assert_eq!(signal(lever, Face::North), 15);
        let plate = state(BlockKind::StonePressurePlate)
            .with_property("powered", "true")
            .unwrap();
        assert_eq!(signal(plate, Face::Up), 15);
        let heavy = state(BlockKind::HeavyWeightedPressurePlate)
            .with_property("power", "7")
            .unwrap();
        assert_eq!(signal(heavy, Face::Up), 7);
    }

    #[test]
    fn an_observer_only_powers_the_side_it_faces() {
        let observer = state(BlockKind::Observer)
            .with_property("powered", "true")
            .and_then(|state| state.with_property("facing", "north"))
            .unwrap();
        assert_eq!(signal(observer, Face::North), 15);
        assert_eq!(signal(observer, Face::South), 0);
    }
}
