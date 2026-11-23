use rspigot_blocks::{BlockKind, StateFlags};
use rspigot_domain::random::{LegacyRandom, RandomSource};
use rspigot_domain::{BlockPos, BlockStateId, ChunkPos};

use rand::Rng;

use crate::ground::is_leaves;
use crate::ports::ClientLink;
use crate::server::Server;
use crate::tree_growth::is_sapling;

const SECTION_SIZE: i32 = 16;
const PRECIPITATION_ODDS: i32 = 48;
const PRECIPITATION_SALT: i64 = 0x5DEE_CE66_D1CE_4E5B;

fn sampler_seed(game_time: i64) -> i64 {
    let mut z = (game_time as u64).wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    (z ^ (z >> 31)) as i64
}

fn ticks_randomly(state: BlockStateId) -> bool {
    state.flags().contains(StateFlags::RANDOMLY_TICKS) || crate::lava_fire::is_lava(state)
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn tick_random_blocks(&mut self) {
        self.tick_tree_growth();
        self.tick_bubble_pushes();
        self.tick_conduits();
        self.tick_potent_sulfur();
        self.tick_block_extras();
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
        let mut precipitation =
            LegacyRandom::new(sampler_seed(self.world.level.game_time) ^ PRECIPITATION_SALT);
        let mut weather: Vec<(i32, i32)> = Vec::new();
        let mut picked: Vec<(BlockPos, BlockStateId)> = Vec::new();
        for chunk in chunks {
            let Some(column) = self.world.chunks.get(chunk) else {
                continue;
            };
            for _ in 0..speed {
                if precipitation.next_int(PRECIPITATION_ODDS) == 0 {
                    weather.push((
                        chunk.x * SECTION_SIZE + precipitation.next_int(SECTION_SIZE),
                        chunk.z * SECTION_SIZE + precipitation.next_int(SECTION_SIZE),
                    ));
                }
            }
            for base_y in &sections {
                for _ in 0..speed {
                    let pos = sampled(&mut sampler, chunk, *base_y);
                    if let Some(state) = column.block(pos).filter(|state| ticks_randomly(*state)) {
                        picked.push((pos, state));
                    }
                }
            }
        }
        for (x, z) in weather {
            self.tick_precipitation(x, z);
        }
        for (pos, state) in picked {
            self.random_tick_block(pos, state);
        }
    }

    fn random_tick_block(&mut self, pos: BlockPos, picked: BlockStateId) {
        let repeats = match crate::world_keys::Crop::of(picked) {
            Some(crop) if !self.config.world_keys.growth.is_vanilla(crop) => {
                let roll: f32 = self.rng.random();
                self.config.world_keys.growth.repeats(crop, roll)
            }
            _ => 1,
        };
        for _ in 0..repeats {
            if self.block_or_air(pos) != picked {
                break;
            }
            self.random_tick_once(pos, picked);
        }
    }

    fn random_tick_once(&mut self, pos: BlockPos, picked: BlockStateId) {
        let Some(kind) = picked.kind() else {
            return;
        };
        match kind {
            BlockKind::TurtleEgg => self.turtle_egg_random_tick(pos),
            BlockKind::DriedGhast => self.dried_ghast_random_tick(pos),
            BlockKind::PointedDripstone => self.dripstone_random_tick(pos, picked),
            BlockKind::SulfurSpike => self.sulfur_spike_random_tick(pos, picked),
            BlockKind::ChorusFlower => self.chorus_random_tick(pos, picked),
            BlockKind::GrassBlock | BlockKind::Mycelium => self.spreading_snowy_tick(pos, kind),
            BlockKind::Lava => self.lava_random_tick(pos),
            BlockKind::RedstoneOre | BlockKind::DeepslateRedstoneOre => {
                self.redstone_ore_random_tick(pos, picked);
            }
            BlockKind::BuddingAmethyst => self.budding_amethyst_tick(pos),
            BlockKind::BrownMushroom | BlockKind::RedMushroom => {
                self.mushroom_random_tick(pos, picked);
            }
            BlockKind::CrimsonNylium | BlockKind::WarpedNylium => self.nylium_random_tick(pos),
            BlockKind::OpenEyeblossom
            | BlockKind::ClosedEyeblossom
            | BlockKind::PottedOpenEyeblossom
            | BlockKind::PottedClosedEyeblossom => self.eyeblossom_random_tick(pos, picked),
            BlockKind::NetherPortal => self.nether_portal_random_tick(pos),
            BlockKind::Cactus if self.cactus_flower_tick(pos, picked) => {}
            BlockKind::Snow => self.snow_layer_tick(pos),
            BlockKind::Ice => self.ice_tick(pos, picked),
            BlockKind::Farmland => self.farmland_tick(pos, picked),
            kind if is_leaves(kind) => self.leaves_random_tick(pos, picked),
            _ if is_sapling(picked) => self.sapling_random_tick(pos, picked),
            _ if self.random_tick_crop(pos, picked) => {}
            _ if self.random_tick_stalk(pos, picked) => {}
            _ if self.random_tick_copper(pos, picked) => {}
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
