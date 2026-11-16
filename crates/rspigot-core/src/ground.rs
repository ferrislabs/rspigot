use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use rspigot_blocks::{BlockKind, Face, StateFlags, Support};
use rspigot_domain::{BlockPos, BlockStateId};

use crate::lighting::light_level;
use crate::mobs::sky_darken;
use crate::plugins::steps::Nature;
use crate::ports::ClientLink;
use crate::server::Server;

const SPREAD_ATTEMPTS: i32 = 4;
const SPREAD_LIGHT: i32 = 9;
const SNOW_MELT_LIGHT: i32 = 11;
const ICE_MELT_LIGHT: i32 = 11;
const FREEZE_LIGHT: i32 = 10;
const MAX_LEAF_DISTANCE: i32 = 7;
const MAX_MOISTURE: i32 = 7;
const MAX_SNOW_LAYERS: i32 = 8;
const OPAQUE: u8 = 15;
const FARMLAND_REACH: (i32, i32) = (4, 1);
const FALLING_WATER: i32 = 8;

fn tagged(state: BlockStateId, tag: &str) -> bool {
    state.kind().is_some_and(|kind| {
        rspigot_data::tags::tag("minecraft:block", tag)
            .is_some_and(|ids| ids.binary_search(&u32::from(kind.id().0)).is_ok())
    })
}

pub(crate) fn is_leaves(kind: BlockKind) -> bool {
    rspigot_data::tags::tag("minecraft:block", "minecraft:leaves")
        .is_some_and(|ids| ids.binary_search(&u32::from(kind.id().0)).is_ok())
}

fn at(pos: BlockPos, face: Face) -> BlockPos {
    let (dx, dy, dz) = face.offset();
    pos.offset(dx, dy, dz)
}

fn number(state: BlockStateId, name: &str) -> i32 {
    state
        .property(name)
        .and_then(|value| value.parse().ok())
        .unwrap_or(0)
}

fn full_fluid(state: BlockStateId) -> bool {
    match state.kind() {
        Some(BlockKind::Water | BlockKind::Lava) => {
            let level = number(state, "level");
            level == 0 || level >= FALLING_WATER
        }
        _ => state.property("waterlogged") == Some("true"),
    }
}

fn has_water(state: BlockStateId) -> bool {
    state.flags().contains(StateFlags::WATER)
}

fn water_source(state: BlockStateId) -> bool {
    state.kind() == Some(BlockKind::Water) && number(state, "level") == 0
}

fn decaying(state: BlockStateId) -> bool {
    state.property("persistent") == Some("false") && number(state, "distance") == MAX_LEAF_DISTANCE
}

fn leaf_distance(state: BlockStateId) -> i32 {
    if tagged(state, "minecraft:logs") {
        0
    } else if state.kind().is_some_and(is_leaves) {
        number(state, "distance")
    } else {
        MAX_LEAF_DISTANCE
    }
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn block_brightness(&self, pos: BlockPos) -> i32 {
        self.world
            .chunks
            .get(pos.chunk())
            .map_or(0, |chunk| light_level(chunk, pos, false))
    }

    pub(crate) fn raw_brightness(&self, pos: BlockPos) -> i32 {
        let sky = if self.world.dimension.has_skylight {
            self.chunk_view().sky_light(pos) - sky_darken(&self.world.level)
        } else {
            0
        };
        self.block_brightness(pos).max(sky)
    }

    fn can_stay_alive(&self, pos: BlockPos) -> bool {
        let above = self.block_or_air(at(pos, Face::Up));
        if above.kind() == Some(BlockKind::Snow) && number(above, "layers") == 1 {
            return true;
        }
        !full_fluid(above) && above.light_dampening() < OPAQUE
    }

    fn can_propagate(&self, pos: BlockPos) -> bool {
        self.can_stay_alive(pos) && !has_water(self.block_or_air(at(pos, Face::Up)))
    }

    fn spread_seed(&self, pos: BlockPos) -> u64 {
        let mut seed = self.world.level.game_time as u64;
        for part in [pos.x, pos.y, pos.z] {
            seed = seed
                .wrapping_mul(0x9E37_79B9_7F4A_7C15)
                .wrapping_add(part as u64);
        }
        seed
    }

    pub(crate) fn spreading_snowy_tick(&mut self, pos: BlockPos, kind: BlockKind) {
        if !self.can_stay_alive(pos) {
            self.nature_change(Nature::Fade, pos, BlockKind::Dirt.default_state(), pos);
            return;
        }
        if self.raw_brightness(at(pos, Face::Up)) < SPREAD_LIGHT {
            return;
        }
        let mut random = StdRng::seed_from_u64(self.spread_seed(pos));
        for _ in 0..SPREAD_ATTEMPTS {
            let dx = random.random_range(0..3) - 1;
            let dy = random.random_range(0..5) - 3;
            let dz = random.random_range(0..3) - 1;
            let target = pos.offset(dx, dy, dz);
            if self.block_or_air(target).kind() != Some(BlockKind::Dirt)
                || !self.can_propagate(target)
            {
                continue;
            }
            let snowy = tagged(self.block_or_air(at(target, Face::Up)), "minecraft:snow");
            let state = kind
                .default_state()
                .with_property("snowy", if snowy { "true" } else { "false" })
                .unwrap_or_else(|| kind.default_state());
            self.set_block(target, state);
        }
    }

    pub(crate) fn snow_layer_tick(&mut self, pos: BlockPos) {
        if self.block_brightness(pos) > SNOW_MELT_LIGHT {
            self.nature_break(Nature::Fade, pos);
        }
    }

    pub(crate) fn ice_tick(&mut self, pos: BlockPos, state: BlockStateId) {
        let threshold = ICE_MELT_LIGHT - i32::from(state.light_dampening());
        if self.block_brightness(pos) <= threshold {
            return;
        }
        let melted = if self.world.dimension.key.as_str() == "minecraft:the_nether" {
            BlockStateId::AIR
        } else {
            BlockKind::Water.default_state()
        };
        self.nature_change(Nature::Fade, pos, melted, pos);
    }

    fn near_water(&self, pos: BlockPos) -> bool {
        let (reach, height) = FARMLAND_REACH;
        (-reach..=reach).any(|dx| {
            (0..=height).any(|dy| {
                (-reach..=reach).any(|dz| has_water(self.block_or_air(pos.offset(dx, dy, dz))))
            })
        })
    }

    pub(crate) fn farmland_tick(&mut self, pos: BlockPos, state: BlockStateId) {
        let moisture = number(state, "moisture");
        let above = at(pos, Face::Up);
        let wet = self.near_water(pos) || self.raining_at(above);
        if !wet {
            if moisture > 0 {
                self.set_moisture(pos, state, moisture - 1);
            } else if !tagged(self.block_or_air(above), "minecraft:maintains_farmland") {
                self.nature_change(Nature::Fade, pos, BlockKind::Dirt.default_state(), pos);
            }
        } else if moisture < MAX_MOISTURE {
            self.set_moisture(pos, state, MAX_MOISTURE);
        }
    }

    fn set_moisture(&mut self, pos: BlockPos, state: BlockStateId, moisture: i32) {
        if let Some(wetter) = state.with_property("moisture", &moisture.to_string()) {
            self.nature_change(Nature::Moisture, pos, wetter, pos);
        }
    }

    pub(crate) fn leaves_random_tick(&mut self, pos: BlockPos, state: BlockStateId) {
        if decaying(state) {
            self.nature_break(Nature::Leaves, pos);
        }
    }

    pub(crate) fn leaves_neighbour_changed(&mut self, pos: BlockPos, kind: BlockKind, from: Face) {
        let state = self.block_or_air(pos);
        let distance = leaf_distance(self.block_or_air(at(pos, from))) + 1;
        if distance != 1 || number(state, "distance") != distance {
            self.schedule(pos, kind, 1);
        }
    }

    pub(crate) fn leaves_distance_tick(&mut self, pos: BlockPos, state: BlockStateId) {
        let mut distance = MAX_LEAF_DISTANCE;
        for face in Face::ALL {
            distance = distance.min(leaf_distance(self.block_or_air(at(pos, face))) + 1);
            if distance == 1 {
                break;
            }
        }
        if let Some(updated) = state.with_property("distance", &distance.to_string())
            && updated != state
        {
            self.set_block(pos, updated);
        }
    }

    pub(crate) fn motion_blocking_top(&self, x: i32, z: i32) -> i32 {
        let height = self.world.dimension.height;
        let pos = BlockPos::new(x, height.min_y, z);
        let Some(chunk) = self.world.chunks.get(pos.chunk()) else {
            return height.min_y;
        };
        chunk
            .highest_block((x & 15) as usize, (z & 15) as usize, |state| {
                state.flags().contains(StateFlags::BLOCKS_MOTION)
                    || state.flags().has_fluid()
                    || has_water(state)
            })
            .map_or(height.min_y, |top| top + 1)
    }

    fn snow_survives(&self, pos: BlockPos) -> bool {
        let below = self.block_or_air(at(pos, Face::Down));
        if tagged(below, "minecraft:cannot_support_snow_layer") {
            return false;
        }
        tagged(below, "minecraft:support_override_snow_layer")
            || below.is_face_sturdy(Face::Up, Support::Full)
            || (below.kind() == Some(BlockKind::Snow) && number(below, "layers") == MAX_SNOW_LAYERS)
    }

    fn in_world(&self, pos: BlockPos) -> bool {
        let height = self.world.dimension.height;
        pos.y >= height.min_y && pos.y < height.max_y()
    }

    fn should_freeze(&self, pos: BlockPos) -> bool {
        if self.warm_enough_to_rain(pos)
            || !self.in_world(pos)
            || self.block_brightness(pos) >= FREEZE_LIGHT
            || !water_source(self.block_or_air(pos))
        {
            return false;
        }
        ![Face::West, Face::East, Face::North, Face::South]
            .into_iter()
            .all(|face| has_water(self.block_or_air(at(pos, face))))
    }

    fn should_snow(&self, pos: BlockPos) -> bool {
        let precipitation = self
            .climate_at(pos)
            .is_some_and(|(precipitation, _)| precipitation);
        if !precipitation
            || self.warm_enough_to_rain(pos)
            || !self.in_world(pos)
            || self.block_brightness(pos) >= FREEZE_LIGHT
        {
            return false;
        }
        let state = self.block_or_air(pos);
        (state.is_air() || state.kind() == Some(BlockKind::Snow)) && self.snow_survives(pos)
    }

    pub(crate) fn tick_precipitation(&mut self, x: i32, z: i32) {
        let top = self.motion_blocking_top(x, z);
        let above = BlockPos::new(x, top, z);
        let below = BlockPos::new(x, top - 1, z);
        if self.should_freeze(below) {
            self.nature_change(Nature::Form, below, BlockKind::Ice.default_state(), below);
        }
        if !self.world.level.raining {
            return;
        }
        self.accumulate_snow(above);
        self.cauldron_precipitation(below);
    }

    fn accumulate_snow(&mut self, above: BlockPos) {
        let limit = self.config.rules.max_snow_accumulation_height;
        if limit <= 0 || !self.should_snow(above) {
            return;
        }
        let state = self.block_or_air(above);
        if state.kind() == Some(BlockKind::Snow) {
            let layers = number(state, "layers");
            if layers < limit.min(MAX_SNOW_LAYERS)
                && let Some(deeper) = state.with_property("layers", &(layers + 1).to_string())
            {
                self.nature_change(Nature::Form, above, deeper, above);
            }
        } else {
            self.nature_change(Nature::Form, above, BlockKind::Snow.default_state(), above);
        }
    }
}
