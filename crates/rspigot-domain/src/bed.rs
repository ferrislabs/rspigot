use rspigot_blocks::{BlockKind, Face};
use rspigot_entities::EntityKind;
use rspigot_nbt::{Compound, Tag};

use crate::entity::physics::{BlockView, has_tag};
use crate::entity::{Aabb, fluid};
use crate::spawn::placement::is_block_dangerous;
use crate::{BlockPos, BlockStateId, Identifier, Vec3};

pub const DEEP_SLEEP_TICKS: i32 = 100;
pub const WAKE_UP_DAY_TICK: i64 = 0;
pub const DAY_LENGTH: i64 = 24_000;
pub const REACH_HORIZONTAL: f64 = 3.0;
pub const REACH_VERTICAL: f64 = 2.0;
pub const MONSTER_RANGE_HORIZONTAL: f64 = 8.0;
pub const MONSTER_RANGE_VERTICAL: f64 = 5.0;
pub const SLEEP_BODY_OFFSET: f64 = 0.125;
pub const STAND_UP_FALLBACK_OFFSET: f64 = 0.1;
pub const PLAYER_WIDTH: f32 = 0.6;
pub const PLAYER_HEIGHT: f32 = 1.8;
pub const SLEEPING_SIZE: f64 = 0.2;

const CLIMBABLE: &str = "minecraft:climbable";
const INVALID_SPAWN_INSIDE: &str = "minecraft:invalid_spawn_inside";
const BED_HEIGHT: f64 = 0.5625;
const STRAW_BED_HEIGHT: f64 = 0.25;
const RAD_TO_DEG: f64 = 57.295_776_367_187_5;
const DISMOUNT_EPSILON: f64 = 1.0;

pub const NO_SLEEP_MESSAGE: &str = "block.minecraft.bed.no_sleep";
pub const OCCUPIED_MESSAGE: &str = "block.minecraft.bed.occupied";
pub const TOO_FAR_MESSAGE: &str = "block.minecraft.bed.too_far_away";
pub const OBSTRUCTED_MESSAGE: &str = "block.minecraft.bed.obstructed";
pub const NOT_SAFE_MESSAGE: &str = "block.minecraft.bed.not_safe";
pub const SPAWN_SET_MESSAGE: &str = "block.minecraft.set_spawn";
pub const SLEEP_NOT_POSSIBLE_MESSAGE: &str = "sleep.not_possible";
pub const SLEEP_SKIPPING_MESSAGE: &str = "sleep.skipping_night";
pub const SLEEP_COUNT_MESSAGE: &str = "sleep.players_sleeping";

pub const fn is_bed(kind: BlockKind) -> bool {
    matches!(
        kind,
        BlockKind::WhiteBed
            | BlockKind::OrangeBed
            | BlockKind::MagentaBed
            | BlockKind::LightBlueBed
            | BlockKind::YellowBed
            | BlockKind::LimeBed
            | BlockKind::PinkBed
            | BlockKind::GrayBed
            | BlockKind::LightGrayBed
            | BlockKind::CyanBed
            | BlockKind::PurpleBed
            | BlockKind::BlueBed
            | BlockKind::BrownBed
            | BlockKind::GreenBed
            | BlockKind::RedBed
            | BlockKind::BlackBed
            | BlockKind::StrawBed
    )
}

pub fn bed_of(state: BlockStateId) -> Option<BlockKind> {
    state.kind().filter(|kind| is_bed(*kind))
}

pub const fn sleep_height(kind: BlockKind) -> f64 {
    if matches!(kind, BlockKind::StrawBed) {
        STRAW_BED_HEIGHT
    } else {
        BED_HEIGHT
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Part {
    Head,
    Foot,
}

pub fn part_of(state: BlockStateId) -> Option<Part> {
    match state.property("part")? {
        "head" => Some(Part::Head),
        "foot" => Some(Part::Foot),
        _ => None,
    }
}

pub fn face_named(name: &str) -> Option<Face> {
    Face::HORIZONTAL
        .into_iter()
        .find(|face| face.name() == name)
}

pub fn facing_of(state: BlockStateId) -> Option<Face> {
    face_named(state.property("facing")?)
}

pub fn is_occupied(state: BlockStateId) -> bool {
    state.property("occupied") == Some("true")
}

pub fn with_occupied(state: BlockStateId, occupied: bool) -> BlockStateId {
    state
        .with_property("occupied", if occupied { "true" } else { "false" })
        .unwrap_or(state)
}

pub fn step(pos: BlockPos, face: Face) -> BlockPos {
    let (dx, dy, dz) = face.offset();
    pos.offset(dx, dy, dz)
}

pub fn neighbour_direction(part: Part, facing: Face) -> Face {
    match part {
        Part::Foot => facing,
        Part::Head => facing.opposite(),
    }
}

pub fn partner_pos(pos: BlockPos, state: BlockStateId) -> Option<BlockPos> {
    Some(step(
        pos,
        neighbour_direction(part_of(state)?, facing_of(state)?),
    ))
}

pub fn head_pos(pos: BlockPos, state: BlockStateId) -> Option<BlockPos> {
    match part_of(state)? {
        Part::Head => Some(pos),
        Part::Foot => Some(step(pos, facing_of(state)?)),
    }
}

pub fn is_partner(state: BlockStateId, other: BlockStateId) -> bool {
    state.kind() == other.kind()
        && match (part_of(state), part_of(other)) {
            (Some(a), Some(b)) => a != b,
            _ => false,
        }
}

pub fn orphaned(state: BlockStateId, neighbour: BlockStateId, towards: Face) -> bool {
    let (Some(part), Some(facing)) = (part_of(state), facing_of(state)) else {
        return false;
    };
    towards == neighbour_direction(part, facing) && !is_partner(state, neighbour)
}

pub fn heal_from_partner(state: BlockStateId, partner: BlockStateId) -> BlockStateId {
    with_occupied(state, is_occupied(partner))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rule {
    Always,
    WhenDark,
    Never,
}

impl Rule {
    pub const fn test(self, dark_outside: bool) -> bool {
        match self {
            Self::Always => true,
            Self::WhenDark => dark_outside,
            Self::Never => false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BedRule {
    pub can_sleep: Rule,
    pub can_set_spawn: Rule,
    pub destroy_on_use: bool,
    pub destroy_on_leave: bool,
    pub error_message: Option<&'static str>,
}

impl BedRule {
    pub const CAN_SLEEP_WHEN_DARK: Self = Self {
        can_sleep: Rule::WhenDark,
        can_set_spawn: Rule::Always,
        destroy_on_use: false,
        destroy_on_leave: false,
        error_message: Some(NO_SLEEP_MESSAGE),
    };
    pub const DESTROY_ON_USE: Self = Self {
        can_sleep: Rule::Never,
        can_set_spawn: Rule::Never,
        destroy_on_use: true,
        destroy_on_leave: false,
        error_message: None,
    };
    pub const DESTROY_ON_LEAVE: Self = Self {
        can_sleep: Rule::WhenDark,
        can_set_spawn: Rule::Never,
        destroy_on_use: false,
        destroy_on_leave: true,
        error_message: Some(NO_SLEEP_MESSAGE),
    };

    pub const fn problem(self) -> Problem {
        if self.destroy_on_use {
            Problem::NotPossibleHere
        } else {
            Problem::NotPossibleNow(self.error_message)
        }
    }
}

pub fn rule_of(dimension_type: &Identifier, straw: bool) -> BedRule {
    match dimension_type.path() {
        "the_nether" | "the_end" => BedRule::DESTROY_ON_USE,
        _ if straw => BedRule::DESTROY_ON_LEAVE,
        _ => BedRule::CAN_SLEEP_WHEN_DARK,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Problem {
    TooFarAway,
    Obstructed,
    NotSafe,
    NotPossibleHere,
    NotPossibleNow(Option<&'static str>),
    Other,
}

impl Problem {
    pub const fn message(self) -> Option<&'static str> {
        match self {
            Self::TooFarAway => Some(TOO_FAR_MESSAGE),
            Self::Obstructed => Some(OBSTRUCTED_MESSAGE),
            Self::NotSafe => Some(NOT_SAFE_MESSAGE),
            Self::NotPossibleNow(message) => message,
            Self::NotPossibleHere | Self::Other => None,
        }
    }
}

pub fn sleepers_needed(active: i32, percent: i32) -> i32 {
    let wanted = ((active * percent) as f32 / 100.0).ceil() as i32;
    wanted.max(1)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SleepStatus {
    pub active: i32,
    pub sleeping: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sleeper {
    pub spectator: bool,
    pub sleeping: bool,
    pub deep: bool,
}

impl SleepStatus {
    pub fn enough_sleeping(&self, percent: i32) -> bool {
        sleepers_needed(self.active, percent) <= self.sleeping
    }

    pub fn enough_deep_sleeping(&self, percent: i32, players: &[Sleeper]) -> bool {
        let deep = players
            .iter()
            .filter(|player| !player.spectator && player.deep)
            .count() as i32;
        players.iter().any(|player| player.deep) && sleepers_needed(self.active, percent) <= deep
    }

    pub fn clear(&mut self) {
        self.sleeping = 0;
    }

    pub fn update(&mut self, players: &[Sleeper]) -> bool {
        let (before_active, before_sleeping) = (self.active, self.sleeping);
        self.active = 0;
        self.sleeping = 0;
        let mut anyone = false;
        for player in players.iter().filter(|player| !player.spectator) {
            self.active += 1;
            if player.sleeping {
                self.sleeping += 1;
                anyone = true;
            }
        }
        anyone
            && (before_sleeping >= 1 || self.sleeping >= 1)
            && (before_active != self.active || before_sleeping != self.sleeping)
    }
}

pub fn skip_target(day_time: i64) -> i64 {
    let day = day_time.div_euclid(DAY_LENGTH);
    (day + 1) * DAY_LENGTH + WAKE_UP_DAY_TICK
}

fn non_climbable_top(view: &impl BlockView, pos: BlockPos) -> f64 {
    let state = view.block_or_air(pos);
    if has_tag(state, CLIMBABLE) {
        return f64::NEG_INFINITY;
    }
    state
        .collision_shape()
        .iter()
        .map(|shape| shape.max_y)
        .fold(f64::NEG_INFINITY, f64::max)
}

fn floor_height(view: &impl BlockView, pos: BlockPos) -> f64 {
    let top = non_climbable_top(view, pos);
    if top.is_finite() {
        return top;
    }
    let below = non_climbable_top(view, pos.offset(0, -1, 0));
    if below >= DISMOUNT_EPSILON {
        below - 1.0
    } else {
        f64::NEG_INFINITY
    }
}

pub fn safe_stand_location(
    view: &impl BlockView,
    pos: BlockPos,
    check_dangerous: bool,
) -> Option<Vec3> {
    if check_dangerous && is_block_dangerous(EntityKind::Player, view.block_or_air(pos)) {
        return None;
    }
    let height = floor_height(view, pos);
    if height.is_infinite() || height >= 1.0 {
        return None;
    }
    if check_dangerous
        && height <= 0.0
        && is_block_dangerous(EntityKind::Player, view.block_or_air(pos.offset(0, -1, 0)))
    {
        return None;
    }
    let spot = Vec3::new(
        f64::from(pos.x) + 0.5,
        f64::from(pos.y) + height,
        f64::from(pos.z) + 0.5,
    );
    let body = Aabb::of_size(spot, PLAYER_WIDTH, PLAYER_HEIGHT);
    if !fluid::is_free_of_collision(view, &body) {
        return None;
    }
    if has_tag(view.block_or_air(pos), INVALID_SPAWN_INSIDE)
        || has_tag(view.block_or_air(pos.offset(0, 1, 0)), INVALID_SPAWN_INSIDE)
    {
        return None;
    }
    Some(spot)
}

type Offset = (i32, i32);

fn facing_step(face: Face) -> Offset {
    let (dx, _, dz) = face.offset();
    (dx, dz)
}

fn surround_offsets(forward: Face, side: Face) -> [Offset; 10] {
    let f = facing_step(forward);
    let s = facing_step(side);
    [
        s,
        (s.0 - f.0, s.1 - f.1),
        (s.0 - 2 * f.0, s.1 - 2 * f.1),
        (-2 * f.0, -2 * f.1),
        (-2 * f.0 - s.0, -2 * f.1 - s.1),
        (-f.0 - s.0, -f.1 - s.1),
        (-s.0, -s.1),
        (f.0 - s.0, f.1 - s.1),
        f,
        (s.0 + f.0, s.1 + f.1),
    ]
}

fn above_offsets(forward: Face) -> [Offset; 2] {
    let f = facing_step(forward);
    [(0, 0), (-f.0, -f.1)]
}

fn at_offsets(
    view: &impl BlockView,
    pos: BlockPos,
    offsets: &[Offset],
    check_dangerous: bool,
) -> Option<Vec3> {
    offsets
        .iter()
        .find_map(|(dx, dz)| safe_stand_location(view, pos.offset(*dx, 0, *dz), check_dangerous))
}

fn clockwise(face: Face) -> Face {
    match face {
        Face::North => Face::East,
        Face::East => Face::South,
        Face::South => Face::West,
        _ => Face::North,
    }
}

fn is_facing_angle(face: Face, yaw: f32) -> bool {
    let radians = yaw * (std::f32::consts::PI / 180.0);
    let (dx, dz) = facing_step(face);
    dx as f32 * -radians.sin() + dz as f32 * radians.cos() > 0.0
}

fn is_bed_block(view: &impl BlockView, pos: BlockPos) -> bool {
    bed_of(view.block_or_air(pos)).is_some()
}

pub fn find_stand_up(
    view: &impl BlockView,
    pos: BlockPos,
    forward: Face,
    yaw: f32,
) -> Option<Vec3> {
    let mut side = clockwise(forward);
    if is_facing_angle(side, yaw) {
        side = side.opposite();
    }
    let surround = surround_offsets(forward, side);
    let above = above_offsets(forward);
    if !is_bed_block(view, pos.offset(0, -1, 0)) {
        let all: Vec<Offset> = surround.iter().chain(above.iter()).copied().collect();
        return at_offsets(view, pos, &all, true).or_else(|| at_offsets(view, pos, &all, false));
    }
    let below = pos.offset(0, -1, 0);
    [true, false].into_iter().find_map(|check| {
        at_offsets(view, pos, &surround, check)
            .or_else(|| at_offsets(view, below, &surround, check))
            .or_else(|| at_offsets(view, pos, &above, check))
    })
}

pub fn stand_up_or_fallback(view: &impl BlockView, pos: BlockPos, forward: Face, yaw: f32) -> Vec3 {
    find_stand_up(view, pos, forward, yaw).unwrap_or_else(|| {
        Vec3::new(
            f64::from(pos.x) + 0.5,
            f64::from(pos.y + 1) + STAND_UP_FALLBACK_OFFSET,
            f64::from(pos.z) + 0.5,
        )
    })
}

pub fn look_at_yaw(from: Vec3, towards: BlockPos) -> f32 {
    let direction = (towards.center() - from).normalize();
    let degrees = direction.z.atan2(direction.x) * RAD_TO_DEG - 90.0;
    wrap_degrees(degrees) as f32
}

fn wrap_degrees(degrees: f64) -> f64 {
    let mut wrapped = degrees % 360.0;
    if wrapped >= 180.0 {
        wrapped -= 360.0;
    }
    if wrapped < -180.0 {
        wrapped += 360.0;
    }
    wrapped
}

pub fn sleep_position(pos: BlockPos, kind: BlockKind) -> Vec3 {
    Vec3::new(
        f64::from(pos.x) + 0.5,
        f64::from(pos.y) + sleep_height(kind) + SLEEP_BODY_OFFSET,
        f64::from(pos.z) + 0.5,
    )
}

pub fn in_reach(player: Vec3, bed: BlockPos) -> bool {
    let centre = bed.center();
    (player.x - centre.x).abs() <= REACH_HORIZONTAL
        && (player.y - centre.y).abs() <= REACH_VERTICAL
        && (player.z - centre.z).abs() <= REACH_HORIZONTAL
}

pub fn monster_search_area(bed: BlockPos) -> Aabb {
    let centre = bed.center();
    Aabb::new(
        centre.x - MONSTER_RANGE_HORIZONTAL,
        centre.y - MONSTER_RANGE_VERTICAL,
        centre.z - MONSTER_RANGE_HORIZONTAL,
        centre.x + MONSTER_RANGE_HORIZONTAL,
        centre.y + MONSTER_RANGE_VERTICAL,
        centre.z + MONSTER_RANGE_HORIZONTAL,
    )
}

#[derive(Debug, Clone, PartialEq)]
pub struct RespawnConfig {
    pub dimension: Identifier,
    pub pos: BlockPos,
    pub yaw: f32,
    pub pitch: f32,
    pub forced: bool,
}

impl RespawnConfig {
    pub fn is_same_position(&self, other: &Self) -> bool {
        self.dimension == other.dimension && self.pos == other.pos
    }

    pub fn to_nbt(&self) -> Compound {
        Compound::new()
            .with("dimension", self.dimension.as_str())
            .with(
                "pos",
                Tag::IntArray(vec![self.pos.x, self.pos.y, self.pos.z]),
            )
            .with("yaw", self.yaw)
            .with("pitch", self.pitch)
            .with("forced", self.forced)
    }

    pub fn from_nbt(data: &Compound) -> Option<Self> {
        let pos = match data.get_int_array("pos")? {
            [x, y, z] => BlockPos::new(*x, *y, *z),
            _ => return None,
        };
        let angle = |key: &str| {
            data.get_f64(key)
                .filter(|value| value.is_finite())
                .map_or(0.0, |value| value as f32)
        };
        Some(Self {
            dimension: data
                .get_str("dimension")
                .and_then(|raw| Identifier::parse(raw).ok())
                .unwrap_or_else(|| Identifier::minecraft("overworld")),
            pos,
            yaw: angle("yaw").clamp(-180.0, 180.0),
            pitch: angle("pitch").clamp(-90.0, 90.0),
            forced: data.get_bool("forced").unwrap_or(false),
        })
    }
}

#[cfg(test)]
mod tests {
    use rustc_hash::FxHashMap;

    use super::*;

    struct Room(FxHashMap<BlockPos, BlockStateId>);

    impl BlockView for Room {
        fn block(&self, pos: BlockPos) -> Option<BlockStateId> {
            Some(self.0.get(&pos).copied().unwrap_or_else(|| {
                if pos.y <= 63 {
                    BlockKind::Stone.default_state()
                } else {
                    BlockStateId::AIR
                }
            }))
        }
    }

    fn bed(part: &str, facing: &str) -> BlockStateId {
        BlockKind::RedBed
            .state_with(&[("part", part), ("facing", facing), ("occupied", "false")])
            .expect("a bed state")
    }

    fn room() -> Room {
        let mut blocks = FxHashMap::default();
        blocks.insert(BlockPos::new(0, 64, 0), bed("foot", "south"));
        blocks.insert(BlockPos::new(0, 64, 1), bed("head", "south"));
        Room(blocks)
    }

    #[test]
    fn a_bed_head_and_foot_find_each_other() {
        let foot = bed("foot", "south");
        let head = bed("head", "south");
        assert_eq!(
            partner_pos(BlockPos::new(0, 64, 0), foot),
            Some(BlockPos::new(0, 64, 1))
        );
        assert_eq!(
            partner_pos(BlockPos::new(0, 64, 1), head),
            Some(BlockPos::new(0, 64, 0))
        );
        assert_eq!(
            head_pos(BlockPos::new(0, 64, 0), foot),
            Some(BlockPos::new(0, 64, 1))
        );
        assert!(is_partner(foot, head));
        assert!(!is_partner(foot, foot));
    }

    #[test]
    fn a_bed_half_without_its_partner_is_orphaned_only_towards_the_partner() {
        let foot = bed("foot", "south");
        assert!(orphaned(foot, BlockStateId::AIR, Face::South));
        assert!(!orphaned(foot, BlockStateId::AIR, Face::North));
        assert!(!orphaned(foot, bed("head", "south"), Face::South));
    }

    #[test]
    fn the_overworld_beds_work_when_dark_and_the_other_dimensions_explode_them() {
        let overworld = Identifier::minecraft("overworld");
        let nether = Identifier::minecraft("the_nether");
        assert_eq!(rule_of(&overworld, false), BedRule::CAN_SLEEP_WHEN_DARK);
        assert_eq!(rule_of(&overworld, true), BedRule::DESTROY_ON_LEAVE);
        assert_eq!(rule_of(&nether, false), BedRule::DESTROY_ON_USE);
        assert_eq!(
            rule_of(&Identifier::minecraft("the_end"), true),
            BedRule::DESTROY_ON_USE
        );
        assert_eq!(
            BedRule::CAN_SLEEP_WHEN_DARK.problem().message(),
            Some(NO_SLEEP_MESSAGE)
        );
        assert_eq!(BedRule::DESTROY_ON_USE.problem().message(), None);
    }

    #[test]
    fn the_sleepers_needed_round_up_and_are_at_least_one() {
        assert_eq!(sleepers_needed(4, 50), 2);
        assert_eq!(sleepers_needed(3, 50), 2);
        assert_eq!(sleepers_needed(0, 100), 1);
        assert_eq!(sleepers_needed(10, 100), 10);
    }

    #[test]
    fn the_sleep_status_announces_only_a_real_change_with_someone_in_bed() {
        let asleep = Sleeper {
            spectator: false,
            sleeping: true,
            deep: false,
        };
        let awake = Sleeper {
            sleeping: false,
            ..asleep
        };
        let mut status = SleepStatus::default();
        assert!(status.update(&[asleep, awake]));
        assert_eq!((status.active, status.sleeping), (2, 1));
        assert!(!status.update(&[asleep, awake]));
        assert!(!status.update(&[awake, awake]));
        assert!(status.enough_sleeping(50) || !status.enough_sleeping(100));
    }

    #[test]
    fn deep_sleep_needs_someone_who_slept_long_enough() {
        let mut status = SleepStatus::default();
        let light = Sleeper {
            spectator: false,
            sleeping: true,
            deep: false,
        };
        let deep = Sleeper {
            deep: true,
            ..light
        };
        status.update(&[light, deep]);
        assert!(status.enough_sleeping(100));
        assert!(!status.enough_deep_sleeping(100, &[light, deep]));
        assert!(status.enough_deep_sleeping(50, &[light, deep]));
    }

    #[test]
    fn the_night_skip_lands_on_the_next_morning() {
        assert_eq!(skip_target(13_000), 24_000);
        assert_eq!(skip_target(0), 24_000);
        assert_eq!(skip_target(47_999), 48_000);
    }

    #[test]
    fn a_player_stands_up_beside_the_bed_on_the_side_it_faces() {
        let spot = find_stand_up(&room(), BlockPos::new(0, 64, 1), Face::South, 0.0);
        assert_eq!(spot, Some(Vec3::new(-0.5, 64.0, 1.5)));
        let other = find_stand_up(&room(), BlockPos::new(0, 64, 1), Face::South, 90.0);
        assert_eq!(other, Some(Vec3::new(1.5, 64.0, 1.5)));
    }

    #[test]
    fn a_walled_in_bed_stands_the_player_on_the_bed() {
        let mut room = room();
        for x in -2..=2 {
            for z in -2..=4 {
                if x == 0 && (z == 0 || z == 1) {
                    continue;
                }
                for y in [64, 65] {
                    room.0
                        .insert(BlockPos::new(x, y, z), BlockKind::Stone.default_state());
                }
            }
        }
        let spot = find_stand_up(&room, BlockPos::new(0, 64, 1), Face::South, 0.0);
        assert_eq!(spot, Some(Vec3::new(0.5, 64.5625, 1.5)));
    }

    #[test]
    fn the_stand_up_yaw_looks_at_the_bed() {
        let yaw = look_at_yaw(Vec3::new(-0.5, 64.0, 1.5), BlockPos::new(0, 64, 1));
        assert!((yaw + 90.0).abs() < 0.001);
    }

    #[test]
    fn the_respawn_config_round_trips_through_nbt() {
        let config = RespawnConfig {
            dimension: Identifier::minecraft("the_nether"),
            pos: BlockPos::new(4, 70, -9),
            yaw: 90.0,
            pitch: 12.5,
            forced: true,
        };
        let data = config.to_nbt();
        assert_eq!(data.get_int_array("pos"), Some(&[4, 70, -9][..]));
        assert_eq!(RespawnConfig::from_nbt(&data), Some(config));
        assert!(RespawnConfig::from_nbt(&Compound::new()).is_none());
    }
}
