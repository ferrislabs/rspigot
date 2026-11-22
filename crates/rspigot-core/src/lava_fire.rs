use rand::Rng;
use rspigot_blocks::{Face, StateFlags};
use rspigot_domain::{BlockPos, BlockStateId};

use crate::plugins::events::BlockIgniteCause;
use crate::ports::ClientLink;
use crate::server::Server;

const NEIGHBOURS: [Face; 6] = [
    Face::Down,
    Face::Up,
    Face::North,
    Face::South,
    Face::West,
    Face::East,
];

fn ignited_by_lava(state: BlockStateId) -> bool {
    state
        .kind()
        .is_some_and(|kind| kind.settings().ignited_by_lava)
}

pub(crate) fn is_lava(state: BlockStateId) -> bool {
    state.flags().contains(StateFlags::LAVA)
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn lava_random_tick(&mut self, pos: BlockPos) {
        if !self.can_spread_fire_around(pos) {
            return;
        }
        let strokes = self.rng.random_range(0..3);
        if strokes > 0 {
            let mut at = pos;
            for _ in 0..strokes {
                at = at.offset(
                    self.rng.random_range(-1..=1),
                    1,
                    self.rng.random_range(-1..=1),
                );
                let Some(state) = self.world.chunks.block(at) else {
                    return;
                };
                if state.is_air() {
                    if self.has_lava_fuel_around(at) {
                        self.light_by_lava(at);
                        return;
                    }
                } else if state.flags().contains(StateFlags::BLOCKS_MOTION) {
                    return;
                }
            }
        } else {
            for _ in 0..3 {
                let at = pos.offset(
                    self.rng.random_range(-1..=1),
                    0,
                    self.rng.random_range(-1..=1),
                );
                let Some(state) = self.world.chunks.block(at) else {
                    return;
                };
                let above = at.offset(0, 1, 0);
                if ignited_by_lava(state)
                    && self
                        .world
                        .chunks
                        .block(above)
                        .is_some_and(BlockStateId::is_air)
                {
                    self.light_by_lava(above);
                }
            }
        }
    }

    fn has_lava_fuel_around(&self, pos: BlockPos) -> bool {
        NEIGHBOURS.iter().any(|face| {
            let (dx, dy, dz) = face.offset();
            self.world
                .chunks
                .block(pos.offset(dx, dy, dz))
                .is_some_and(ignited_by_lava)
        })
    }

    fn light_by_lava(&mut self, pos: BlockPos) {
        let state = self.fire_state_with_age(pos, 0);
        self.ignite_by(pos, state, BlockIgniteCause::Lava, None);
    }
}
