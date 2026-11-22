use rand::Rng;
use rspigot_blocks::BlockKind;
use rspigot_domain::{BlockPos, BlockStateId};

use crate::plugins::steps::Nature;
use crate::ports::ClientLink;
use crate::server::Server;

const MELT_LIGHT: i32 = 11;
const MAX_AGE: i32 = 3;
const CROWD_LIMIT: usize = 4;
pub(crate) const FORM_DELAY: (u64, u64) = (60, 120);
const MELT_DELAY: (u64, u64) = (20, 40);
const NETHER: &str = "minecraft:the_nether";
const NEIGHBOURS: [(i32, i32, i32); 6] = [
    (0, -1, 0),
    (0, 1, 0),
    (-1, 0, 0),
    (1, 0, 0),
    (0, 0, -1),
    (0, 0, 1),
];

impl<L: ClientLink> Server<L> {
    fn frosted_neighbours(&self, pos: BlockPos) -> usize {
        NEIGHBOURS
            .iter()
            .filter(|(dx, dy, dz)| {
                self.world
                    .chunks
                    .block(pos.offset(*dx, *dy, *dz))
                    .and_then(BlockStateId::kind)
                    == Some(BlockKind::FrostedIce)
            })
            .count()
    }

    pub(crate) fn frosted_ice_tick(&mut self, pos: BlockPos, state: BlockStateId) {
        let crowded = self.frosted_neighbours(pos) >= CROWD_LIMIT;
        let age = state
            .property("age")
            .and_then(|value| value.parse::<i32>().ok())
            .unwrap_or(0);
        let threshold = MELT_LIGHT - age - i32::from(state.light_dampening());
        let melting =
            (self.rng.random_range(0..3) == 0 || !crowded) && self.raw_brightness(pos) > threshold;
        if melting && self.slightly_melt(pos, state, age) {
            for (dx, dy, dz) in NEIGHBOURS {
                let neighbour = pos.offset(dx, dy, dz);
                let Some(next) = self.world.chunks.block(neighbour) else {
                    continue;
                };
                if next.kind() != Some(BlockKind::FrostedIce) {
                    continue;
                }
                let next_age = next
                    .property("age")
                    .and_then(|value| value.parse::<i32>().ok())
                    .unwrap_or(0);
                if !self.slightly_melt(neighbour, next, next_age) {
                    self.schedule_frosted_ice(neighbour, MELT_DELAY);
                }
            }
            return;
        }
        self.schedule_frosted_ice(pos, MELT_DELAY);
    }

    pub(crate) fn schedule_frosted_ice(&mut self, pos: BlockPos, delay: (u64, u64)) {
        let wait = self.rng.random_range(delay.0..=delay.1);
        self.schedule(pos, BlockKind::FrostedIce, wait);
    }

    fn slightly_melt(&mut self, pos: BlockPos, state: BlockStateId, age: i32) -> bool {
        if age < MAX_AGE {
            if let Some(older) = state.with_property("age", &(age + 1).to_string()) {
                self.nature_change(Nature::Fade, pos, older, pos);
            }
            return false;
        }
        let melted = if self.world.dimension.key.as_str() == NETHER {
            BlockStateId::AIR
        } else {
            BlockKind::Water.default_state()
        };
        self.nature_change(Nature::Fade, pos, melted, pos);
        true
    }
}
