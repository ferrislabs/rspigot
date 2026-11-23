use rspigot_blocks::{BlockKind, StateFlags};

use crate::{BlockPos, BlockStateId, Vec3};

pub const STATE_PROPERTY: &str = "potent_sulfur_state";
pub const NAUSEA_DURATION_TICKS: u32 = 80;
pub const NAUSEA_INTERVAL_TICKS: i64 = 10;
pub const COUNTDOWN_INTERVAL_TICKS: i64 = 20;
pub const GAS_SEARCH_HEIGHT: i32 = 5;
pub const GAS_REACH_SQR: f64 = 9.0;
pub const NEARBY_INFLATE_XZ: f64 = 2.5;
pub const LAUNCH_IMPULSE: f64 = 0.2;
pub const LAUNCH_CEILING_BASE: f64 = f32::from_bits(0x3e99_999a) as f64;
pub const LAUNCH_CEILING_PER_WATER: f64 = 0.1;
pub const UNOBSTRUCTED_PER_WATER: i32 = 6;
pub const SEED_SALT: i64 = -904_011_478;
pub const ERUPTION_START_SOUND: &str = "minecraft:block.potent_sulfur.geyser_eruption";
pub const CONTINUOUS_START_SOUND: &str = "minecraft:block.potent_sulfur.geyser_continuous_eruption";
pub const ERUPTION_ACTIVE_SOUND: &str = "minecraft:block.potent_sulfur.geyser_eruption_active";
pub const CONTINUOUS_ACTIVE_SOUND: &str =
    "minecraft:block.potent_sulfur.geyser_continuous_eruption_active";
pub const NOXIOUS_GAS_SOUND: &str = "minecraft:block.potent_sulfur.noxious_gas";
pub const ACTIVE_SOUND_INTERVAL_TICKS: i64 = 40;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PotentState {
    Dry,
    Wet,
    Dormant,
    Erupting,
    Continuous,
}

impl PotentState {
    pub fn name(self) -> &'static str {
        match self {
            Self::Dry => "dry",
            Self::Wet => "wet",
            Self::Dormant => "dormant",
            Self::Erupting => "erupting",
            Self::Continuous => "continuous",
        }
    }

    fn of(name: &str) -> Option<Self> {
        Some(match name {
            "dry" => Self::Dry,
            "wet" => Self::Wet,
            "dormant" => Self::Dormant,
            "erupting" => Self::Erupting,
            "continuous" => Self::Continuous,
            _ => return None,
        })
    }

    pub fn is_erupting(self) -> bool {
        matches!(self, Self::Erupting | Self::Continuous)
    }
}

pub fn state_of(state: BlockStateId) -> Option<PotentState> {
    if state.kind()? != BlockKind::PotentSulfur {
        return None;
    }
    state.property(STATE_PROPERTY).and_then(PotentState::of)
}

pub fn with_state(state: PotentState) -> BlockStateId {
    BlockKind::PotentSulfur
        .state_with(&[(STATE_PROPERTY, state.name())])
        .unwrap_or_else(|| BlockKind::PotentSulfur.default_state())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Surroundings {
    pub water_above: bool,
    pub below_continuous: bool,
    pub below_periodic: bool,
    pub below_fluid_ok: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Valid {
    pub state: PotentState,
    pub reset_countdown: bool,
}

pub fn valid_state(current: PotentState, around: Surroundings) -> Valid {
    let settled = |state| Valid {
        state,
        reset_countdown: false,
    };
    if !around.water_above {
        return settled(PotentState::Dry);
    }
    if around.below_continuous && around.below_fluid_ok {
        return settled(PotentState::Continuous);
    }
    if !(around.below_periodic && around.below_fluid_ok) {
        return settled(PotentState::Wet);
    }
    let cycling = matches!(current, PotentState::Erupting | PotentState::Dormant);
    Valid {
        state: if current == PotentState::Erupting {
            PotentState::Erupting
        } else {
            PotentState::Dormant
        },
        reset_countdown: !cycling,
    }
}

fn is_water_source(state: BlockStateId) -> bool {
    match state.kind() {
        Some(BlockKind::Water) => state.property("level") == Some("0"),
        Some(_) => state.flags().contains(StateFlags::WATER),
        None => false,
    }
}

pub fn is_passable(state: BlockStateId) -> bool {
    state.is_air()
        || state.kind() == Some(BlockKind::Water)
        || !state.flags().contains(StateFlags::HAS_COLLISION)
}

pub fn find_gas_source(
    block: &impl Fn(BlockPos) -> BlockStateId,
    origin: BlockPos,
) -> Option<BlockPos> {
    let limit = origin.y + GAS_SEARCH_HEIGHT;
    let mut at = origin.offset(0, 1, 0);
    loop {
        if at.y > limit {
            return None;
        }
        let state = block(at);
        let water = state.kind() == Some(BlockKind::Water);
        if !is_water_source(state) || (!water && !is_passable(state)) {
            return is_passable(state).then_some(at);
        }
        at = at.offset(0, 1, 0);
    }
}

pub fn geyser_height(origin: BlockPos, source: BlockPos) -> i32 {
    source.y - origin.y - 1
}

pub fn unobstructed_blocks(
    block: &impl Fn(BlockPos) -> BlockStateId,
    above: BlockPos,
    water_blocks: i32,
) -> i32 {
    let limit = water_blocks * UNOBSTRUCTED_PER_WATER;
    for step in 0..limit {
        if !is_passable(block(above.offset(0, step, 0))) {
            return step;
        }
    }
    limit
}

pub fn launches(vertical_speed: f64, water_blocks: i32) -> bool {
    vertical_speed < f64::from(water_blocks) * LAUNCH_CEILING_PER_WATER + LAUNCH_CEILING_BASE
}

pub fn gas_reaches(
    source: BlockPos,
    eye: Vec3,
    eye_is_passable: bool,
    water_below_eye: bool,
    line_of_sight: bool,
) -> bool {
    eye_is_passable
        && eye.distance_squared(source.center()) <= GAS_REACH_SQR
        && water_below_eye
        && line_of_sight
}

const GOLDEN_RATIO: u64 = 0x9e37_79b9_7f4a_7c15;
const SILVER_RATIO: u64 = 0x6a09_e667_f3bc_c909;

fn mix(mut value: u64) -> u64 {
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GeyserRandom {
    lo: u64,
    hi: u64,
}

impl GeyserRandom {
    fn new(lo: u64, hi: u64) -> Self {
        if lo | hi == 0 {
            Self {
                lo: GOLDEN_RATIO,
                hi: SILVER_RATIO,
            }
        } else {
            Self { lo, hi }
        }
    }

    fn next_long(&mut self) -> u64 {
        let (lo, hi) = (self.lo, self.hi);
        let result = lo.wrapping_add(hi).rotate_left(17).wrapping_add(lo);
        let hi = hi ^ lo;
        self.lo = lo.rotate_left(49) ^ hi ^ (hi << 21);
        self.hi = hi.rotate_left(28);
        result
    }

    pub fn at(world_seed: i64, pos: BlockPos) -> Self {
        let seed = (world_seed ^ SEED_SALT) as u64;
        let lo = seed ^ SILVER_RATIO;
        let hi = lo.wrapping_add(GOLDEN_RATIO);
        let mut root = Self::new(mix(lo), mix(hi));
        let (seed_lo, seed_hi) = (root.next_long(), root.next_long());
        let mut position = i64::from(pos.x.wrapping_mul(3_129_871))
            ^ i64::from(pos.z).wrapping_mul(116_129_781)
            ^ i64::from(pos.y);
        position = position
            .wrapping_mul(position)
            .wrapping_mul(42_317_861)
            .wrapping_add(position.wrapping_mul(11));
        position >>= 16;
        Self::new((position as u64) ^ seed_lo, seed_hi)
    }

    pub fn next_int_any(&mut self) -> i32 {
        self.next_long() as i32
    }

    pub fn next_int(&mut self, bound: i32) -> i32 {
        let bound = bound as u32;
        let mut sample = u64::from(self.next_int_any() as u32);
        let mut product = sample * u64::from(bound);
        let mut low = product & 0xffff_ffff;
        if low < u64::from(bound) {
            let threshold = u64::from(bound.wrapping_neg() % bound);
            while low < threshold {
                sample = u64::from(self.next_int_any() as u32);
                product = sample * u64::from(bound);
                low = product & 0xffff_ffff;
            }
        }
        (product >> 32) as i32
    }

    pub fn next_int_between_inclusive(&mut self, min: i32, max: i32) -> i32 {
        self.next_int(max - min + 1) + min
    }
}

pub fn next_countdown(state: PotentState, water_blocks: i32, random: &mut GeyserRandom) -> i32 {
    if state == PotentState::Dormant {
        (water_blocks - 1) * 10 + random.next_int_between_inclusive(15, 30)
    } else {
        random.next_int_any();
        water_blocks - 1 + random.next_int_between_inclusive(1, 2)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use rspigot_blocks::BlockKind;

    use super::*;
    use crate::{BlockPos, BlockStateId};

    const ORIGIN: BlockPos = BlockPos::new(0, 64, 0);

    fn surroundings() -> Surroundings {
        Surroundings {
            water_above: true,
            below_continuous: false,
            below_periodic: false,
            below_fluid_ok: true,
        }
    }

    #[test]
    fn without_a_water_source_above_the_block_is_dry() {
        for state in [
            PotentState::Dry,
            PotentState::Wet,
            PotentState::Dormant,
            PotentState::Erupting,
            PotentState::Continuous,
        ] {
            let valid = valid_state(
                state,
                Surroundings {
                    water_above: false,
                    ..surroundings()
                },
            );
            assert_eq!(valid.state, PotentState::Dry);
            assert!(!valid.reset_countdown);
        }
    }

    #[test]
    fn water_alone_makes_it_wet() {
        let valid = valid_state(PotentState::Dry, surroundings());
        assert_eq!(valid.state, PotentState::Wet);
    }

    #[test]
    fn lava_below_makes_it_erupt_continuously_when_the_lava_is_a_source() {
        let valid = valid_state(
            PotentState::Wet,
            Surroundings {
                below_continuous: true,
                ..surroundings()
            },
        );
        assert_eq!(valid.state, PotentState::Continuous);
        let flowing = valid_state(
            PotentState::Wet,
            Surroundings {
                below_continuous: true,
                below_fluid_ok: false,
                ..surroundings()
            },
        );
        assert_eq!(flowing.state, PotentState::Wet);
    }

    #[test]
    fn magma_below_goes_dormant_and_resets_the_countdown_when_it_was_not_cycling() {
        let periodic = Surroundings {
            below_periodic: true,
            ..surroundings()
        };
        let fresh = valid_state(PotentState::Wet, periodic);
        assert_eq!(fresh.state, PotentState::Dormant);
        assert!(fresh.reset_countdown);
        let erupting = valid_state(PotentState::Erupting, periodic);
        assert_eq!(erupting.state, PotentState::Erupting);
        assert!(!erupting.reset_countdown);
        let dormant = valid_state(PotentState::Dormant, periodic);
        assert_eq!(dormant.state, PotentState::Dormant);
        assert!(!dormant.reset_countdown);
    }

    #[test]
    fn the_block_state_round_trips() {
        for state in [
            PotentState::Dry,
            PotentState::Wet,
            PotentState::Dormant,
            PotentState::Erupting,
            PotentState::Continuous,
        ] {
            assert_eq!(state_of(with_state(state)), Some(state));
        }
        assert_eq!(state_of(BlockKind::Stone.default_state()), None);
    }

    fn column(water: i32, top: BlockStateId) -> HashMap<BlockPos, BlockStateId> {
        let mut world = HashMap::new();
        for dy in 1..=water {
            world.insert(ORIGIN.offset(0, dy, 0), BlockKind::Water.default_state());
        }
        world.insert(ORIGIN.offset(0, water + 1, 0), top);
        world
    }

    fn lookup(map: &HashMap<BlockPos, BlockStateId>) -> impl Fn(BlockPos) -> BlockStateId + '_ {
        |pos| map.get(&pos).copied().unwrap_or(BlockStateId::AIR)
    }

    #[test]
    fn the_gas_source_is_the_first_open_block_above_the_water_column() {
        let world = column(3, BlockStateId::AIR);
        let block = lookup(&world);
        assert_eq!(
            find_gas_source(&block, ORIGIN),
            Some(ORIGIN.offset(0, 4, 0))
        );
        assert_eq!(geyser_height(ORIGIN, ORIGIN.offset(0, 4, 0)), 3);
    }

    #[test]
    fn a_solid_cap_or_a_column_taller_than_five_has_no_gas_source() {
        let capped = column(3, BlockKind::Stone.default_state());
        let block = lookup(&capped);
        assert_eq!(find_gas_source(&block, ORIGIN), None);
        let tall = column(6, BlockStateId::AIR);
        let block = lookup(&tall);
        assert_eq!(find_gas_source(&block, ORIGIN), None);
        let five = column(4, BlockStateId::AIR);
        let block = lookup(&five);
        assert_eq!(
            find_gas_source(&block, ORIGIN),
            Some(ORIGIN.offset(0, 5, 0))
        );
    }

    #[test]
    fn the_unobstructed_height_is_six_per_water_block_unless_something_solid_is_in_the_way() {
        let clear = HashMap::new();
        let block = lookup(&clear);
        assert_eq!(unobstructed_blocks(&block, ORIGIN.offset(0, 1, 0), 3), 18);
        let mut blocked = HashMap::new();
        blocked.insert(ORIGIN.offset(0, 9, 0), BlockKind::Stone.default_state());
        let block = lookup(&blocked);
        assert_eq!(unobstructed_blocks(&block, ORIGIN.offset(0, 1, 0), 3), 8);
    }

    #[test]
    fn an_entity_is_launched_only_while_slower_than_the_geyser_ceiling() {
        assert!(launches(0.0, 3));
        assert!(launches(0.59, 3));
        assert!(!launches(0.61, 3));
        assert!(!launches(0.31, 0));
    }

    #[test]
    fn the_countdown_after_an_eruption_is_short_and_after_a_dormant_phase_long() {
        let mut random = GeyserRandom::at(1234, ORIGIN);
        for _ in 0..50 {
            let erupting = next_countdown(PotentState::Erupting, 3, &mut random);
            assert!((3..=4).contains(&erupting), "{erupting}");
            let dormant = next_countdown(PotentState::Dormant, 3, &mut random);
            assert!((35..=50).contains(&dormant), "{dormant}");
        }
    }

    #[test]
    fn the_geyser_random_depends_on_seed_and_position_only() {
        let mut a = GeyserRandom::at(7, ORIGIN);
        let mut b = GeyserRandom::at(7, ORIGIN);
        assert_eq!(a.next_int(1000), b.next_int(1000));
        let mut other = GeyserRandom::at(7, ORIGIN.offset(1, 0, 0));
        let mut c = GeyserRandom::at(7, ORIGIN);
        let differs = (0..8).any(|_| c.next_int(1_000_000) != other.next_int(1_000_000));
        assert!(differs);
    }

    #[test]
    fn nausea_reaches_only_eyes_in_open_air_within_three_blocks_of_the_source() {
        let source = ORIGIN.offset(0, 4, 0);
        assert!(gas_reaches(
            source,
            crate::Vec3::new(0.5, 66.4, 0.5),
            true,
            true,
            true
        ));
        assert!(!gas_reaches(
            source,
            crate::Vec3::new(0.5, 66.4, 0.5),
            false,
            true,
            true
        ));
        assert!(!gas_reaches(
            source,
            crate::Vec3::new(0.5, 66.4, 0.5),
            true,
            false,
            true
        ));
        assert!(!gas_reaches(
            source,
            crate::Vec3::new(0.5, 66.4, 0.5),
            true,
            true,
            false
        ));
        assert!(!gas_reaches(
            source,
            crate::Vec3::new(5.5, 68.5, 0.5),
            true,
            true,
            true
        ));
    }
}
