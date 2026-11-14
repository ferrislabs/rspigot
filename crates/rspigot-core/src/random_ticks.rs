use rspigot_blocks::{BlockKind, StateFlags};
use rspigot_domain::random::{LegacyRandom, RandomSource};
use rspigot_domain::{BlockPos, BlockStateId, ChunkPos};

use crate::ports::ClientLink;
use crate::server::Server;

const SECTION_SIZE: i32 = 16;

fn sampler_seed(game_time: i64) -> i64 {
    let mut z = (game_time as u64).wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    (z ^ (z >> 31)) as i64
}

fn ticks_randomly(state: BlockStateId) -> bool {
    state.flags().contains(StateFlags::RANDOMLY_TICKS)
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn tick_random_blocks(&mut self) {
        let speed = self.config.rules.random_tick_speed;
        if speed <= 0 {
            return;
        }
        let chunks = self.ticking_chunks_near_players();
        if chunks.is_empty() {
            return;
        }
        let height = self.world.dimension.height;
        let sections = (height.min_y..height.min_y + height.height as i32)
            .step_by(SECTION_SIZE as usize)
            .collect::<Vec<i32>>();
        let mut sampler = LegacyRandom::new(sampler_seed(self.world.level.game_time));
        let mut picked: Vec<(BlockPos, BlockStateId)> = Vec::new();
        for chunk in chunks {
            let Some(column) = self.world.chunks.get(chunk) else {
                continue;
            };
            for base_y in &sections {
                for _ in 0..speed {
                    let pos = sampled(&mut sampler, chunk, *base_y);
                    if let Some(state) = column.block(pos).filter(|state| ticks_randomly(*state)) {
                        picked.push((pos, state));
                    }
                }
            }
        }
        for (pos, state) in picked {
            self.random_tick_block(pos, state);
        }
    }

    fn random_tick_block(&mut self, pos: BlockPos, picked: BlockStateId) {
        let Some(kind) = picked.kind() else {
            return;
        };
        match kind {
            BlockKind::TurtleEgg => self.turtle_egg_random_tick(pos),
            BlockKind::DriedGhast => self.dried_ghast_random_tick(pos),
            _ => {}
        }
    }
}

fn sampled(sampler: &mut LegacyRandom, chunk: ChunkPos, base_y: i32) -> BlockPos {
    let x = chunk.x * SECTION_SIZE + sampler.next_int(SECTION_SIZE);
    let y = base_y + sampler.next_int(SECTION_SIZE);
    let z = chunk.z * SECTION_SIZE + sampler.next_int(SECTION_SIZE);
    BlockPos::new(x, y, z)
}
