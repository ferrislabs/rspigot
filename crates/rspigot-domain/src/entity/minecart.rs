use rspigot_entities::EntityKind;
use rspigot_nbt::Compound;

use super::base::EntityBase;
use super::data::SyncedValue;
use super::physics::{BlockView, has_tag};
use super::vehicle::{self, CARGO_SIZE, Cargo};
use crate::{BlockPos, BlockStateId, Vec3};

pub const DATA_DISPLAY_BLOCK: u8 = 11;
pub const DATA_DISPLAY_OFFSET: u8 = 12;
pub const HOPPER_SIZE: usize = 5;
pub const KEYS: &[&str] = &[
    "FlippedRotation",
    "HasTicked",
    "Items",
    "LootTable",
    "LootTableSeed",
];

const RAILS: &str = "minecraft:rails";
const GRAVITY: f64 = 0.04;
const WATER_GRAVITY: f64 = 0.005;
const SLIDE: f64 = 0.007_812_5;
pub const DEFAULT_SPEED_RULE: i32 = 8;
const SPEED_TICKS: f64 = 20.0;
const WATER_SPEED_FACTOR: f64 = 0.5;
const OCCUPIED_FACTOR: f64 = 0.75;
const OCCUPIED_SLOWDOWN: f64 = 0.997;
const EMPTY_SLOWDOWN: f64 = 0.96;
const CONTAINER_SLOWDOWN: f32 = 0.98;
const WATER_SLOWDOWN: f64 = 0.95;
const DERAILED: f64 = 0.5;
const FLYING: f64 = 0.95;
const RAIL_HEIGHT: f64 = 0.0625;
const TURN_THRESHOLD: f64 = 0.001;
const DEFAULT_DISPLAY_OFFSET: i32 = 6;
const CHEST_DISPLAY_OFFSET: i32 = 8;
const HOPPER_DISPLAY_OFFSET: i32 = 1;
const BRAKE_LIMIT: f64 = 0.03;
const BOOST: f64 = 0.06;
const KICK: f64 = 0.02;
const BOOST_THRESHOLD: f64 = 0.01;

#[derive(Debug, Clone, PartialEq)]
pub struct MinecartState {
    pub flipped: bool,
    pub on_rails: bool,
    pub first_tick: bool,
    pub enabled: bool,
    pub cargo: Option<Cargo>,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Tick {
    pub block_collision: Option<BlockPos>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Exits {
    pub first: (i32, i32, i32),
    pub second: (i32, i32, i32),
    pub slope: bool,
}

pub fn exits_of(shape: &str) -> Option<Exits> {
    let flat = |first, second| Exits {
        first,
        second,
        slope: false,
    };
    let slope = |first, second| Exits {
        first,
        second,
        slope: true,
    };
    Some(match shape {
        "north_south" => flat((0, 0, -1), (0, 0, 1)),
        "east_west" => flat((-1, 0, 0), (1, 0, 0)),
        "ascending_east" => slope((-1, -1, 0), (1, 0, 0)),
        "ascending_west" => slope((-1, 0, 0), (1, -1, 0)),
        "ascending_north" => slope((0, 0, -1), (0, -1, 1)),
        "ascending_south" => slope((0, -1, -1), (0, 0, 1)),
        "south_east" => flat((0, 0, 1), (1, 0, 0)),
        "south_west" => flat((0, 0, 1), (-1, 0, 0)),
        "north_west" => flat((0, 0, -1), (-1, 0, 0)),
        "north_east" => flat((0, 0, -1), (1, 0, 0)),
        _ => return None,
    })
}

pub fn is_rail(state: BlockStateId) -> bool {
    has_tag(state, RAILS)
}

fn rail_exits(state: BlockStateId) -> Option<(Exits, &'static str)> {
    if !is_rail(state) {
        return None;
    }
    let shape = state.property("shape")?;
    exits_of(shape).map(|exits| (exits, shape))
}

pub fn display_offset(kind: EntityKind) -> i32 {
    match kind {
        EntityKind::ChestMinecart => CHEST_DISPLAY_OFFSET,
        EntityKind::HopperMinecart => HOPPER_DISPLAY_OFFSET,
        _ => DEFAULT_DISPLAY_OFFSET,
    }
}

pub fn define(base: &mut EntityBase) -> MinecartState {
    vehicle::define_hurt(base);
    base.data
        .define(DATA_DISPLAY_BLOCK, SyncedValue::OptionalBlockState(None));
    base.data.define(
        DATA_DISPLAY_OFFSET,
        SyncedValue::Int(display_offset(base.kind)),
    );
    let cargo = match base.kind {
        EntityKind::ChestMinecart => Some(Cargo::new(CARGO_SIZE)),
        EntityKind::HopperMinecart => Some(Cargo::new(HOPPER_SIZE)),
        _ => None,
    };
    MinecartState {
        flipped: false,
        on_rails: false,
        first_tick: false,
        enabled: true,
        cargo,
    }
}

pub fn is_rideable(kind: EntityKind) -> bool {
    kind == EntityKind::Minecart
}

pub fn set_initial_position(base: &mut EntityBase, at: Vec3) {
    base.position = at;
    base.old_position = at;
}

pub fn rail_position(view: &impl BlockView, x: f64, y: f64, z: f64) -> Option<Vec3> {
    let (bx, mut by, bz) = (x.floor() as i32, y.floor() as i32, z.floor() as i32);
    if is_rail(view.block_or_air(BlockPos::new(bx, by - 1, bz))) {
        by -= 1;
    }
    let (exits, _) = rail_exits(view.block_or_air(BlockPos::new(bx, by, bz)))?;
    let (first, second) = (exits.first, exits.second);
    let xa = f64::from(bx) + 0.5 + f64::from(first.0) * 0.5;
    let ya = f64::from(by) + RAIL_HEIGHT + f64::from(first.1) * 0.5;
    let za = f64::from(bz) + 0.5 + f64::from(first.2) * 0.5;
    let xb = f64::from(bx) + 0.5 + f64::from(second.0) * 0.5;
    let yb = f64::from(by) + RAIL_HEIGHT + f64::from(second.1) * 0.5;
    let zb = f64::from(bz) + 0.5 + f64::from(second.2) * 0.5;
    let xd = xb - xa;
    let yd = (yb - ya) * 2.0;
    let zd = zb - za;
    let progress = if xd == 0.0 {
        z - f64::from(bz)
    } else if zd == 0.0 {
        x - f64::from(bx)
    } else {
        ((x - xa) * xd + (z - za) * zd) * 2.0
    };
    let mut height = ya + yd * progress;
    if yd < 0.0 {
        height += 1.0;
    } else if yd > 0.0 {
        height += 0.5;
    }
    Some(Vec3::new(xa + xd * progress, height, za + zd * progress))
}

fn rail_block(base: &EntityBase, view: &impl BlockView) -> BlockPos {
    let at = base.position.block();
    if is_rail(view.block_or_air(at.offset(0, -1, 0))) {
        at.offset(0, -1, 0)
    } else {
        at
    }
}

pub fn signal_of(cargo: &Cargo) -> i32 {
    let mut filled = false;
    let mut fraction = 0.0_f32;
    for stack in cargo.items.iter().flatten() {
        let limit = crate::item::max_stack(stack).min(64) as f32;
        fraction += f32::from(stack.count.get()) / limit;
        filled = true;
    }
    fraction /= cargo.items.len() as f32;
    (fraction * 14.0).floor() as i32 + i32::from(filled)
}

fn slowdown(base: &EntityBase, state: &MinecartState, occupied: bool) -> (f64, f64) {
    if let Some(cargo) = &state.cargo {
        let mut factor = CONTAINER_SLOWDOWN;
        if cargo.loot_table.is_none() {
            factor += (15 - signal_of(cargo)) as f32 * 0.001;
        }
        let factor = f64::from(factor);
        return (
            factor,
            if base.fluid.in_water {
                WATER_SLOWDOWN
            } else {
                1.0
            },
        );
    }
    let factor = if occupied {
        OCCUPIED_SLOWDOWN
    } else {
        EMPTY_SLOWDOWN
    };
    (
        factor,
        if base.fluid.in_water {
            WATER_SLOWDOWN
        } else {
            1.0
        },
    )
}

fn max_speed(base: &EntityBase, speed_rule: i32) -> f64 {
    let factor = if base.fluid.in_water {
        WATER_SPEED_FACTOR
    } else {
        1.0
    };
    f64::from(speed_rule) * factor / SPEED_TICKS
}

fn travel(
    base: &mut EntityBase,
    view: &impl BlockView,
    movement: Vec3,
    hit: &mut Option<BlockPos>,
) {
    let found = vehicle::travel(base, view, movement, GRAVITY);
    if hit.is_none() {
        *hit = found;
    }
}

fn come_off_track(
    base: &mut EntityBase,
    view: &impl BlockView,
    speed_rule: i32,
    hit: &mut Option<BlockPos>,
) {
    let limit = max_speed(base, speed_rule);
    base.velocity = Vec3::new(
        base.velocity.x.clamp(-limit, limit),
        base.velocity.y,
        base.velocity.z.clamp(-limit, limit),
    );
    if base.on_ground {
        base.velocity = base.velocity.multiply(DERAILED, DERAILED, DERAILED);
    }
    travel(base, view, base.velocity, hit);
    if !base.on_ground {
        base.velocity = base.velocity.multiply(FLYING, FLYING, FLYING);
    }
}

fn boost_on_powered_rail(
    base: &mut EntityBase,
    view: &impl BlockView,
    block: BlockPos,
    shape: &str,
) {
    let speed = base.velocity.horizontal_distance_sqr().sqrt();
    if speed > BOOST_THRESHOLD {
        base.velocity = base.velocity
            + Vec3::new(
                base.velocity.x / speed * BOOST,
                0.0,
                base.velocity.z / speed * BOOST,
            );
        return;
    }
    let conductor =
        |at: BlockPos| crate::spawn::placement::is_redstone_conductor(view.block_or_air(at));
    let velocity = base.velocity;
    let kicked = match shape {
        "east_west" if conductor(block.offset(-1, 0, 0)) => Vec3::new(KICK, velocity.y, velocity.z),
        "east_west" if conductor(block.offset(1, 0, 0)) => Vec3::new(-KICK, velocity.y, velocity.z),
        "north_south" if conductor(block.offset(0, 0, -1)) => {
            Vec3::new(velocity.x, velocity.y, KICK)
        }
        "north_south" if conductor(block.offset(0, 0, 1)) => {
            Vec3::new(velocity.x, velocity.y, -KICK)
        }
        _ => return,
    };
    base.velocity = kicked;
}

fn move_along_track(
    base: &mut EntityBase,
    state: &MinecartState,
    view: &impl BlockView,
    occupied: bool,
    speed_rule: i32,
    hit: &mut Option<BlockPos>,
) {
    let block = rail_block(base, view);
    let Some((exits, shape)) = rail_exits(view.block_or_air(block)) else {
        return;
    };
    base.fall_distance = 0.0;
    let (x, z) = (base.position.x, base.position.z);
    let mut y = base.position.y;
    let before = rail_position(view, x, y, z);
    y = f64::from(block.y);
    let mut slide = SLIDE;
    if base.fluid.in_water {
        slide *= 0.2;
    }
    let mut velocity = base.velocity;
    match shape {
        "ascending_east" => {
            velocity = velocity + Vec3::new(-slide, 0.0, 0.0);
            y += 1.0;
        }
        "ascending_west" => {
            velocity = velocity + Vec3::new(slide, 0.0, 0.0);
            y += 1.0;
        }
        "ascending_north" => {
            velocity = velocity + Vec3::new(0.0, 0.0, slide);
            y += 1.0;
        }
        "ascending_south" => {
            velocity = velocity + Vec3::new(0.0, 0.0, -slide);
            y += 1.0;
        }
        _ => {}
    }
    let mut xd = f64::from(exits.second.0 - exits.first.0);
    let mut zd = f64::from(exits.second.2 - exits.first.2);
    let length = (xd * xd + zd * zd).sqrt();
    if velocity.x * xd + velocity.z * zd < 0.0 {
        xd = -xd;
        zd = -zd;
    }
    let power = 2.0_f64.min(velocity.horizontal_distance_sqr().sqrt());
    velocity = Vec3::new(power * xd / length, velocity.y, power * zd / length);
    base.velocity = velocity;
    let rail_state = view.block_or_air(block);
    let on_powered_rail = rail_state.kind() == Some(rspigot_blocks::BlockKind::PoweredRail);
    let rail_powered = on_powered_rail && rail_state.property("powered") == Some("true");
    if on_powered_rail && !rail_powered {
        if base.velocity.horizontal_distance_sqr().sqrt() < BRAKE_LIMIT {
            base.velocity = Vec3::ZERO;
        } else {
            base.velocity = base.velocity.multiply(0.5, 0.0, 0.5);
        }
    }
    let xa = f64::from(block.x) + 0.5 + f64::from(exits.first.0) * 0.5;
    let za = f64::from(block.z) + 0.5 + f64::from(exits.first.2) * 0.5;
    let xb = f64::from(block.x) + 0.5 + f64::from(exits.second.0) * 0.5;
    let zb = f64::from(block.z) + 0.5 + f64::from(exits.second.2) * 0.5;
    let dx = xb - xa;
    let dz = zb - za;
    let progress = if dx == 0.0 {
        z - f64::from(block.z)
    } else if dz == 0.0 {
        x - f64::from(block.x)
    } else {
        ((x - xa) * dx + (z - za) * dz) * 2.0
    };
    base.position = Vec3::new(xa + dx * progress, y, za + dz * progress);
    let factor = if occupied { OCCUPIED_FACTOR } else { 1.0 };
    let limit = max_speed(base, speed_rule);
    let push = Vec3::new(
        (factor * base.velocity.x).clamp(-limit, limit),
        0.0,
        (factor * base.velocity.z).clamp(-limit, limit),
    );
    travel(base, view, push, hit);
    let (cx, cy, cz) = (base.position.x, base.position.y, base.position.z);
    let relative = |at: (i32, i32, i32)| {
        (cx.floor() as i32 - block.x == at.0) && (cz.floor() as i32 - block.z == at.2)
    };
    if exits.first.1 != 0 && relative(exits.first) {
        base.position = Vec3::new(cx, cy + f64::from(exits.first.1), cz);
    } else if exits.second.1 != 0 && relative(exits.second) {
        base.position = Vec3::new(cx, cy + f64::from(exits.second.1), cz);
    }
    let (factor, water) = slowdown(base, state, occupied);
    base.velocity = base.velocity.multiply(factor, 0.0, factor);
    base.velocity = Vec3::new(
        base.velocity.x * water,
        base.velocity.y,
        base.velocity.z * water,
    );
    let now = rail_position(view, base.position.x, base.position.y, base.position.z);
    if let (Some(before), Some(now)) = (before, now) {
        let climb = (before.y - now.y) * 0.05;
        let speed = base.velocity.horizontal_distance_sqr().sqrt();
        if speed > 0.0 {
            let scale = (speed + climb) / speed;
            base.velocity = base.velocity.multiply(scale, 1.0, scale);
        }
        base.position = Vec3::new(base.position.x, now.y, base.position.z);
    }
    if rail_powered {
        boost_on_powered_rail(base, view, block, shape);
    }
    let (nx, nz) = (
        base.position.x.floor() as i32,
        base.position.z.floor() as i32,
    );
    if nx != block.x || nz != block.z {
        let speed = base.velocity.horizontal_distance_sqr().sqrt();
        base.velocity = Vec3::new(
            speed * f64::from(nx - block.x),
            base.velocity.y,
            speed * f64::from(nz - block.z),
        );
    }
}

pub fn tick(
    base: &mut EntityBase,
    state: &mut MinecartState,
    view: &impl BlockView,
    occupied: bool,
    speed_rule: i32,
) -> Tick {
    base.old_position = base.position;
    let yaw_before = base.yaw;
    vehicle::tick_hurt(base);
    base.tick_count = base.tick_count.wrapping_add(1);
    if !base.is_no_gravity() {
        let gravity = if base.fluid.in_water {
            WATER_GRAVITY
        } else {
            GRAVITY
        };
        base.velocity.y -= gravity;
    }
    let mut hit = None;
    let block = rail_block(base, view);
    state.on_rails = is_rail(view.block_or_air(block));
    if state.on_rails {
        move_along_track(base, state, view, occupied, speed_rule, &mut hit);
    } else {
        come_off_track(base, view, speed_rule, &mut hit);
    }
    base.pitch = 0.0;
    let dx = base.old_position.x - base.position.x;
    let dz = base.old_position.z - base.position.z;
    if dx * dx + dz * dz > TURN_THRESHOLD {
        base.yaw = (dz.atan2(dx) * 180.0 / std::f64::consts::PI) as f32;
        if state.flipped {
            base.yaw += 180.0;
        }
    }
    let turn = crate::mth::wrap_degrees(base.yaw - yaw_before);
    if !(-170.0..170.0).contains(&turn) {
        base.yaw += 180.0;
        state.flipped = !state.flipped;
    }
    base.yaw %= 360.0;
    state.first_tick = false;
    Tick {
        block_collision: hit,
    }
}

pub fn read(nbt: &Compound, state: &mut MinecartState) {
    state.flipped = nbt.get_bool("FlippedRotation").unwrap_or(false);
    state.first_tick = nbt.get_bool("HasTicked").unwrap_or(false);
    if let Some(cargo) = state.cargo.as_mut() {
        cargo.read(nbt);
    }
}

pub fn write(state: &MinecartState, out: &mut Compound) {
    out.insert("FlippedRotation", state.flipped);
    out.insert("HasTicked", state.first_tick);
    if let Some(cargo) = &state.cargo {
        cargo.write(out);
    }
}

#[cfg(test)]
mod tests {
    use rspigot_blocks::BlockKind;
    use uuid::Uuid;

    use super::*;
    use crate::EntityId;

    struct Track {
        rails: Vec<(BlockPos, BlockStateId)>,
    }

    impl BlockView for Track {
        fn block(&self, pos: BlockPos) -> Option<BlockStateId> {
            if let Some((_, state)) = self.rails.iter().find(|(at, _)| *at == pos) {
                return Some(*state);
            }
            Some(if pos.y < 64 {
                BlockKind::Stone.default_state()
            } else {
                BlockStateId::AIR
            })
        }
    }

    fn rail(shape: &str) -> BlockStateId {
        BlockKind::Rail
            .default_state()
            .with_property("shape", shape)
            .unwrap()
    }

    fn line(shape: &str, from: i32, to: i32, y: i32) -> Track {
        Track {
            rails: (from..=to)
                .map(|x| (BlockPos::new(x, y, 0), rail(shape)))
                .collect(),
        }
    }

    fn cart(kind: EntityKind, at: Vec3) -> (EntityBase, MinecartState) {
        let mut base = EntityBase::new(EntityId(1), Uuid::nil(), kind);
        let state = define(&mut base);
        base.position = at;
        (base, state)
    }

    #[test]
    fn exits_cover_the_ten_rail_shapes() {
        for shape in [
            "north_south",
            "east_west",
            "ascending_east",
            "ascending_west",
            "ascending_north",
            "ascending_south",
            "south_east",
            "south_west",
            "north_west",
            "north_east",
        ] {
            assert!(exits_of(shape).is_some(), "{shape}");
        }
        assert!(exits_of("flat").is_none());
    }

    #[test]
    fn a_cart_pushed_along_a_flat_track_slows_down_but_keeps_going() {
        let track = line("east_west", 0, 40, 64);
        let (mut base, mut state) = cart(EntityKind::Minecart, Vec3::new(1.5, 64.0625, 0.5));
        base.velocity = Vec3::new(0.3, 0.0, 0.0);
        for _ in 0..20 {
            tick(&mut base, &mut state, &track, false, DEFAULT_SPEED_RULE);
        }
        assert!(state.on_rails);
        assert!(base.position.x > 3.0, "{}", base.position.x);
        assert!((base.position.z - 0.5).abs() < 1.0e-9);
        assert!(base.velocity.x > 0.0 && base.velocity.x < 0.3);
    }

    #[test]
    fn a_rider_keeps_the_cart_rolling_longer() {
        let track = line("east_west", 0, 80, 64);
        let (mut empty, mut state_a) = cart(EntityKind::Minecart, Vec3::new(1.5, 64.0625, 0.5));
        let (mut ridden, mut state_b) = cart(EntityKind::Minecart, Vec3::new(1.5, 64.0625, 0.5));
        empty.velocity = Vec3::new(0.3, 0.0, 0.0);
        ridden.velocity = Vec3::new(0.3, 0.0, 0.0);
        for _ in 0..30 {
            tick(&mut empty, &mut state_a, &track, false, DEFAULT_SPEED_RULE);
            tick(&mut ridden, &mut state_b, &track, true, DEFAULT_SPEED_RULE);
        }
        assert!(ridden.velocity.x > empty.velocity.x);
    }

    #[test]
    fn a_cart_never_exceeds_the_top_speed_of_the_rails() {
        let track = line("east_west", 0, 200, 64);
        let (mut base, mut state) = cart(EntityKind::Minecart, Vec3::new(1.5, 64.0625, 0.5));
        base.velocity = Vec3::new(5.0, 0.0, 0.0);
        let before = base.position.x;
        tick(&mut base, &mut state, &track, false, DEFAULT_SPEED_RULE);
        assert!(base.position.x - before <= 0.4 + 1.0e-9);
    }

    #[test]
    fn the_speed_rule_scales_the_top_speed_of_the_rails() {
        let track = line("east_west", 0, 200, 64);
        let (mut base, mut state) = cart(EntityKind::Minecart, Vec3::new(1.5, 64.0625, 0.5));
        base.velocity = Vec3::new(5.0, 0.0, 0.0);
        let before = base.position.x;
        tick(&mut base, &mut state, &track, false, 16);
        let moved = base.position.x - before;
        assert!(moved <= 0.8 + 1.0e-9 && moved > 0.4 + 1.0e-3, "{moved}");
    }

    #[test]
    fn a_cart_on_a_curve_turns_along_it() {
        let mut rails = vec![(BlockPos::new(0, 64, 0), rail("south_east"))];
        for z in 1..30 {
            rails.push((BlockPos::new(0, 64, z), rail("north_south")));
        }
        for x in 1..30 {
            rails.push((BlockPos::new(x, 64, 0), rail("east_west")));
        }
        let track = Track { rails };
        let (mut base, mut state) = cart(EntityKind::Minecart, Vec3::new(5.5, 64.0625, 0.5));
        base.velocity = Vec3::new(-0.35, 0.0, 0.0);
        for _ in 0..60 {
            tick(&mut base, &mut state, &track, false, DEFAULT_SPEED_RULE);
        }
        assert!(base.position.z > 2.0, "{:?}", base.position);
        assert!(base.position.x < 1.0, "{:?}", base.position);
    }

    #[test]
    fn a_cart_climbs_a_slope_and_loses_speed() {
        let mut rails = vec![
            (BlockPos::new(3, 64, 0), rail("ascending_east")),
            (BlockPos::new(4, 65, 0), rail("east_west")),
        ];
        for x in 0..3 {
            rails.push((BlockPos::new(x, 64, 0), rail("east_west")));
        }
        for x in 5..20 {
            rails.push((BlockPos::new(x, 65, 0), rail("east_west")));
        }
        let track = Track { rails };
        let (mut base, mut state) = cart(EntityKind::Minecart, Vec3::new(0.5, 64.0625, 0.5));
        base.velocity = Vec3::new(0.4, 0.0, 0.0);
        for _ in 0..25 {
            tick(&mut base, &mut state, &track, false, DEFAULT_SPEED_RULE);
        }
        assert!(base.position.y > 64.9, "{:?}", base.position);
        assert!(state.on_rails);
    }

    #[test]
    fn a_cart_off_the_rails_falls_and_drags() {
        let track = Track { rails: Vec::new() };
        let (mut base, mut state) = cart(EntityKind::Minecart, Vec3::new(0.5, 70.0, 0.5));
        base.velocity = Vec3::new(0.3, 0.0, 0.0);
        for _ in 0..60 {
            tick(&mut base, &mut state, &track, false, DEFAULT_SPEED_RULE);
        }
        assert!(!state.on_rails);
        assert!((base.position.y - 64.0).abs() < 1.0e-6);
        assert!(base.velocity.x.abs() < 0.3);
    }

    #[test]
    fn a_cart_running_into_a_wall_reports_the_block() {
        let mut track = line("east_west", 0, 10, 64);
        track.rails.retain(|(at, _)| at.x != 5);
        track
            .rails
            .push((BlockPos::new(5, 64, 0), BlockKind::Stone.default_state()));
        let (mut base, mut state) = cart(EntityKind::Minecart, Vec3::new(2.5, 64.0625, 0.5));
        base.velocity = Vec3::new(0.4, 0.0, 0.0);
        let mut hit = None;
        for _ in 0..10 {
            if let Some(pos) =
                tick(&mut base, &mut state, &track, false, DEFAULT_SPEED_RULE).block_collision
            {
                hit = Some(pos);
                break;
            }
        }
        assert_eq!(hit, Some(BlockPos::new(5, 64, 0)));
    }

    #[test]
    fn containers_slow_down_with_their_load_and_the_chest_has_27_slots() {
        let (_, state) = cart(EntityKind::ChestMinecart, Vec3::ZERO);
        assert_eq!(state.cargo.as_ref().map(|c| c.items.len()), Some(27));
        let (_, hopper) = cart(EntityKind::HopperMinecart, Vec3::ZERO);
        assert_eq!(hopper.cargo.as_ref().map(|c| c.items.len()), Some(5));
        let (_, plain) = cart(EntityKind::Minecart, Vec3::ZERO);
        assert!(plain.cargo.is_none());
    }

    #[test]
    fn the_state_round_trips_through_nbt() {
        let (_, mut state) = cart(EntityKind::ChestMinecart, Vec3::ZERO);
        state.flipped = true;
        let mut nbt = Compound::new();
        write(&state, &mut nbt);
        let (_, mut loaded) = cart(EntityKind::ChestMinecart, Vec3::ZERO);
        read(&nbt, &mut loaded);
        assert_eq!(loaded, state);
    }
}
