use rspigot_blocks::{BlockKind, StateFlags};
use rspigot_nbt::Compound;

use super::aabb::Aabb;
use super::base::EntityBase;
use super::data::SyncedValue;
use super::fluid;
use super::physics::{BlockView, friction, mth_floor, water_height};
use super::vehicle::{self, CARGO_SIZE, Cargo};
use crate::{BlockPos, BlockStateId, Vec3};

pub const DATA_PADDLE_LEFT: u8 = 11;
pub const DATA_PADDLE_RIGHT: u8 = 12;
pub const DATA_BUBBLE_TIME: u8 = 13;
pub const OUT_OF_CONTROL_LIMIT: f32 = 60.0;
pub const SEATS: usize = 2;
pub const CHEST_SEATS: usize = 1;
pub const KEYS: &[&str] = &["Items", "LootTable", "LootTableSeed", "leash"];

const GRAVITY: f64 = 0.04;
const SUBMERGED_LIFT: f64 = 0.01;
const FLOWING_SINK: f64 = -0.0007;
const BUOYANCY_SCALE: f64 = 0.65;
const BUOYANCY_DRAG: f64 = 0.75;
const SPLASH_OFFSET: f64 = 0.101;
const WATER_DRAG: f64 = 0.9;
const SUBMERGED_DRAG: f64 = 0.45;
const PROBE: f64 = 0.001;
const CHEST_SEAT_SHIFT: f64 = 0.15;
const SEAT_FRONT: f64 = 0.2;
const SEAT_BACK: f64 = -0.6;
const ANIMAL_SEAT_SHIFT: f64 = 0.2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    InWater,
    UnderWater,
    UnderFlowingWater,
    OnLand,
    InAir,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BoatState {
    pub status: Status,
    pub old_status: Status,
    pub water_level: f64,
    pub land_friction: f32,
    pub last_yd: f64,
    pub out_of_control_ticks: f32,
    pub cargo: Option<Cargo>,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Tick {
    pub eject: bool,
    pub block_collision: Option<BlockPos>,
}

pub fn define(base: &mut EntityBase) -> BoatState {
    vehicle::define_hurt(base);
    base.data
        .define(DATA_PADDLE_LEFT, SyncedValue::Boolean(false));
    base.data
        .define(DATA_PADDLE_RIGHT, SyncedValue::Boolean(false));
    base.data.define(DATA_BUBBLE_TIME, SyncedValue::Int(0));
    let cargo = base
        .kind
        .extends("AbstractChestBoat")
        .then(|| Cargo::new(CARGO_SIZE));
    BoatState {
        status: Status::InAir,
        old_status: Status::InAir,
        water_level: 0.0,
        land_friction: 0.0,
        last_yd: 0.0,
        out_of_control_ticks: 0.0,
        cargo,
    }
}

pub fn max_passengers(base: &EntityBase) -> usize {
    if base.kind.extends("AbstractChestBoat") {
        CHEST_SEATS
    } else {
        SEATS
    }
}

pub fn set_initial_position(base: &mut EntityBase, at: Vec3) {
    base.position = at;
    base.old_position = at;
}

pub fn set_paddles(base: &mut EntityBase, left: bool, right: bool) {
    base.data.set(DATA_PADDLE_LEFT, SyncedValue::Boolean(left));
    base.data
        .set(DATA_PADDLE_RIGHT, SyncedValue::Boolean(right));
}

pub fn paddle(base: &EntityBase, side: usize) -> bool {
    base.data.boolean(if side == 0 {
        DATA_PADDLE_LEFT
    } else {
        DATA_PADDLE_RIGHT
    })
}

pub fn seat_offset(base: &EntityBase, index: usize, passengers: usize, animal: bool) -> Vec3 {
    let mut along = if base.kind.extends("AbstractChestBoat") {
        CHEST_SEAT_SHIFT
    } else {
        0.0
    };
    if passengers > 1 {
        along = if index == 0 { SEAT_FRONT } else { SEAT_BACK };
        if animal {
            along += ANIMAL_SEAT_SHIFT;
        }
    }
    let height = f64::from(base.dimensions.height);
    let ride_height = if base.kind.extends("Raft") || base.kind.extends("ChestRaft") {
        height * 0.888_888_9
    } else {
        height / 3.0
    };
    let angle = f64::from(-base.yaw * 0.017_453_292);
    let (sin, cos) = (angle.sin(), angle.cos());
    Vec3::new(along * sin, ride_height, along * cos)
}

fn water_top(view: &impl BlockView, pos: BlockPos) -> Option<f64> {
    water_height(view, pos).map(|height| f64::from(pos.y as f32 + height as f32))
}

fn is_water_source(state: BlockStateId) -> bool {
    if !state.flags().contains(StateFlags::WATER) {
        return false;
    }
    match state.kind() {
        Some(BlockKind::Water) => state
            .property("level")
            .and_then(|level| level.parse::<i32>().ok())
            .is_none_or(|level| level == 0),
        _ => true,
    }
}

fn under_water(bb: &Aabb, view: &impl BlockView) -> Option<Status> {
    let top = bb.max_y + PROBE;
    let mut under = false;
    for x in mth_floor(bb.min_x)..(bb.max_x.ceil() as i32) {
        for y in mth_floor(bb.max_y)..(top.ceil() as i32) {
            for z in mth_floor(bb.min_z)..(bb.max_z.ceil() as i32) {
                let pos = BlockPos::new(x, y, z);
                if let Some(surface) = water_top(view, pos)
                    && top < surface
                {
                    if !is_water_source(view.block_or_air(pos)) {
                        return Some(Status::UnderFlowingWater);
                    }
                    under = true;
                }
            }
        }
    }
    under.then_some(Status::UnderWater)
}

fn in_water(bb: &Aabb, state: &mut BoatState, view: &impl BlockView) -> bool {
    state.water_level = f64::MIN;
    let mut inside = false;
    for x in mth_floor(bb.min_x)..(bb.max_x.ceil() as i32) {
        for y in mth_floor(bb.min_y)..((bb.min_y + PROBE).ceil() as i32) {
            for z in mth_floor(bb.min_z)..(bb.max_z.ceil() as i32) {
                if let Some(surface) = water_top(view, BlockPos::new(x, y, z)) {
                    state.water_level = state.water_level.max(surface);
                    inside |= bb.min_y < surface;
                }
            }
        }
    }
    inside
}

fn ground_friction(bb: &Aabb, view: &impl BlockView) -> f32 {
    let probe = Aabb::new(
        bb.min_x,
        bb.min_y - PROBE,
        bb.min_z,
        bb.max_x,
        bb.min_y,
        bb.max_z,
    );
    let (x0, x1) = (mth_floor(probe.min_x) - 1, probe.max_x.ceil() as i32 + 1);
    let (y0, y1) = (mth_floor(probe.min_y) - 1, probe.max_y.ceil() as i32 + 1);
    let (z0, z1) = (mth_floor(probe.min_z) - 1, probe.max_z.ceil() as i32 + 1);
    let mut total = 0.0_f32;
    let mut count = 0_i32;
    for x in x0..x1 {
        for z in z0..z1 {
            let edges = i32::from(x == x0 || x == x1 - 1) + i32::from(z == z0 || z == z1 - 1);
            if edges == 2 {
                continue;
            }
            for y in y0..y1 {
                if edges > 0 && (y == y0 || y == y1 - 1) {
                    continue;
                }
                let state = view.block_or_air(BlockPos::new(x, y, z));
                if state.kind() == Some(BlockKind::LilyPad) {
                    continue;
                }
                let (dx, dy, dz) = (f64::from(x), f64::from(y), f64::from(z));
                let touches = state.collision_shape().iter().any(|shape| {
                    shape.min_x + dx < probe.max_x
                        && shape.max_x + dx > probe.min_x
                        && shape.min_y + dy < probe.max_y
                        && shape.max_y + dy > probe.min_y
                        && shape.min_z + dz < probe.max_z
                        && shape.max_z + dz > probe.min_z
                });
                if touches {
                    total += friction(state);
                    count += 1;
                }
            }
        }
    }
    total / count as f32
}

pub fn compute_status(base: &EntityBase, state: &mut BoatState, view: &impl BlockView) -> Status {
    let bb = base.bounding_box();
    if let Some(under) = under_water(&bb, view) {
        state.water_level = bb.max_y;
        return under;
    }
    if in_water(&bb, state, view) {
        return Status::InWater;
    }
    let friction = ground_friction(&bb, view);
    if friction > 0.0 {
        state.land_friction = friction;
        return Status::OnLand;
    }
    Status::InAir
}

fn water_level_above(base: &EntityBase, state: &BoatState, view: &impl BlockView) -> f32 {
    let bb = base.bounding_box();
    let (x0, x1) = (mth_floor(bb.min_x), bb.max_x.ceil() as i32);
    let (z0, z1) = (mth_floor(bb.min_z), bb.max_z.ceil() as i32);
    let mut y = mth_floor(bb.max_y);
    let floor = (bb.max_y - state.last_yd).ceil() as i32;
    loop {
        if floor > y {
            return (floor + 1) as f32;
        }
        let mut height = 0.0_f32;
        'scan: for x in x0..x1 {
            for z in z0..z1 {
                if let Some(h) = water_height(view, BlockPos::new(x, y, z)) {
                    height = height.max(h as f32);
                }
                if height >= 1.0 {
                    break 'scan;
                }
            }
        }
        if height < 1.0 {
            return y as f32 + height;
        }
        y += 1;
    }
}

fn float(
    base: &mut EntityBase,
    state: &mut BoatState,
    view: &impl BlockView,
    player_piloted: bool,
) {
    let gravity = if base.is_no_gravity() { 0.0 } else { GRAVITY };
    let mut sink = -gravity;
    let mut lift = 0.0;
    if state.old_status == Status::InAir
        && state.status != Status::InAir
        && state.status != Status::OnLand
    {
        state.water_level = base.position.y;
        let top = f64::from(water_level_above(base, state, view) - base.dimensions.height)
            + SPLASH_OFFSET;
        let area = base
            .bounding_box()
            .move_by(Vec3::new(0.0, top - base.position.y, 0.0));
        if fluid::is_free_of_collision(view, &area) {
            base.position.y = top;
            base.velocity = base.velocity.multiply(1.0, 0.0, 1.0);
            state.last_yd = 0.0;
        }
        state.status = Status::InWater;
        return;
    }
    let drag = match state.status {
        Status::InWater => {
            lift = (state.water_level - base.position.y) / f64::from(base.dimensions.height);
            WATER_DRAG
        }
        Status::UnderFlowingWater => {
            sink = FLOWING_SINK;
            WATER_DRAG
        }
        Status::UnderWater => {
            lift = SUBMERGED_LIFT;
            SUBMERGED_DRAG
        }
        Status::InAir => WATER_DRAG,
        Status::OnLand => {
            let land = f64::from(state.land_friction);
            if player_piloted {
                state.land_friction /= 2.0;
            }
            land
        }
    };
    base.velocity = Vec3::new(
        base.velocity.x * drag,
        base.velocity.y + sink,
        base.velocity.z * drag,
    );
    if lift > 0.0 {
        base.velocity.y = (base.velocity.y + lift * (GRAVITY / BUOYANCY_SCALE)) * BUOYANCY_DRAG;
    }
}

pub fn tick(
    base: &mut EntityBase,
    state: &mut BoatState,
    view: &impl BlockView,
    controlled_by_client: bool,
    player_piloted: bool,
    first_passenger_is_player: bool,
) -> Tick {
    state.old_status = state.status;
    state.status = compute_status(base, state, view);
    if matches!(state.status, Status::UnderWater | Status::UnderFlowingWater) {
        state.out_of_control_ticks += 1.0;
    } else {
        state.out_of_control_ticks = 0.0;
    }
    let mut outcome = Tick {
        eject: state.out_of_control_ticks >= OUT_OF_CONTROL_LIMIT,
        block_collision: None,
    };
    base.old_position = base.position;
    vehicle::tick_hurt(base);
    base.tick_count = base.tick_count.wrapping_add(1);
    if controlled_by_client {
        base.velocity = Vec3::ZERO;
        return outcome;
    }
    if !first_passenger_is_player {
        set_paddles(base, false, false);
    }
    float(base, state, view, player_piloted);
    let requested = base.velocity;
    state.last_yd = requested.y;
    let gravity = if base.is_no_gravity() { 0.0 } else { GRAVITY };
    outcome.block_collision = vehicle::travel(base, view, requested, gravity);
    outcome
}

pub fn read(nbt: &Compound, state: &mut BoatState) {
    if let Some(cargo) = state.cargo.as_mut() {
        cargo.read(nbt);
    }
}

pub fn write(state: &BoatState, out: &mut Compound) {
    if let Some(cargo) = &state.cargo {
        cargo.write(out);
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU8;

    use rspigot_entities::EntityKind;
    use rspigot_items::ItemKind;
    use uuid::Uuid;

    use super::*;
    use crate::EntityId;
    use crate::item::ItemStack;

    struct Lake {
        surface: i32,
        floor: i32,
    }

    impl BlockView for Lake {
        fn block(&self, pos: BlockPos) -> Option<BlockStateId> {
            Some(if pos.y < self.floor {
                BlockKind::Stone.default_state()
            } else if pos.y <= self.surface {
                BlockKind::Water.default_state()
            } else {
                BlockStateId::AIR
            })
        }
    }

    struct Field;

    impl BlockView for Field {
        fn block(&self, pos: BlockPos) -> Option<BlockStateId> {
            Some(if pos.y < 64 {
                BlockKind::Stone.default_state()
            } else {
                BlockStateId::AIR
            })
        }
    }

    fn boat(kind: EntityKind, at: Vec3) -> (EntityBase, BoatState) {
        let mut base = EntityBase::new(EntityId(1), Uuid::nil(), kind);
        let state = define(&mut base);
        base.position = at;
        (base, state)
    }

    fn run(base: &mut EntityBase, state: &mut BoatState, view: &impl BlockView, ticks: usize) {
        for _ in 0..ticks {
            tick(base, state, view, false, false, false);
        }
    }

    #[test]
    fn a_boat_dropped_on_a_lake_floats_at_the_surface() {
        let lake = Lake {
            surface: 63,
            floor: 55,
        };
        let (mut base, mut state) = boat(EntityKind::OakBoat, Vec3::new(0.5, 62.0, 0.5));
        run(&mut base, &mut state, &lake, 200);
        assert_eq!(state.status, Status::InWater);
        assert!(
            (base.position.y - 63.52).abs() < 0.05,
            "settled at {}",
            base.position.y
        );
    }

    #[test]
    fn a_boat_over_dry_land_falls_then_rests_on_land() {
        let (mut base, mut state) = boat(EntityKind::OakBoat, Vec3::new(0.5, 70.0, 0.5));
        run(&mut base, &mut state, &Field, 100);
        assert_eq!(state.status, Status::OnLand);
        assert!((base.position.y - 64.0).abs() < 1.0e-6);
        assert!(base.on_ground);
    }

    #[test]
    fn a_boat_in_the_air_is_in_air() {
        let (base, mut state) = boat(EntityKind::OakBoat, Vec3::new(0.5, 70.0, 0.5));
        assert_eq!(compute_status(&base, &mut state, &Field), Status::InAir);
    }

    #[test]
    fn a_boat_pushed_under_still_water_counts_as_under_water() {
        let lake = Lake {
            surface: 70,
            floor: 55,
        };
        let (base, mut state) = boat(EntityKind::OakBoat, Vec3::new(0.5, 60.0, 0.5));
        assert_eq!(compute_status(&base, &mut state, &lake), Status::UnderWater);
    }

    #[test]
    fn staying_under_water_for_three_seconds_ejects_the_passengers() {
        let lake = Lake {
            surface: 70,
            floor: 55,
        };
        let (mut base, mut state) = boat(EntityKind::OakBoat, Vec3::new(0.5, 60.0, 0.5));
        let mut ejected = false;
        for _ in 0..60 {
            ejected = tick(&mut base, &mut state, &lake, true, false, false).eject;
        }
        assert!(ejected);
    }

    #[test]
    fn a_boat_driven_by_a_client_keeps_the_position_it_was_given() {
        let (mut base, mut state) = boat(EntityKind::OakBoat, Vec3::new(0.5, 70.0, 0.5));
        base.velocity = Vec3::new(1.0, 0.0, 0.0);
        tick(&mut base, &mut state, &Field, true, true, true);
        assert_eq!(base.position, Vec3::new(0.5, 70.0, 0.5));
        assert_eq!(base.velocity, Vec3::ZERO);
    }

    #[test]
    fn a_boat_sliding_into_a_wall_reports_the_block_it_hit() {
        struct Wall;
        impl BlockView for Wall {
            fn block(&self, pos: BlockPos) -> Option<BlockStateId> {
                Some(if pos.y < 64 || pos.x >= 2 {
                    BlockKind::Stone.default_state()
                } else {
                    BlockStateId::AIR
                })
            }
        }
        let (mut base, mut state) = boat(EntityKind::OakBoat, Vec3::new(0.5, 64.0, 0.5));
        base.velocity = Vec3::new(1.0, 0.0, 0.0);
        let mut hit = None;
        for _ in 0..10 {
            if let Some(pos) =
                tick(&mut base, &mut state, &Wall, false, false, false).block_collision
            {
                hit = Some(pos);
                break;
            }
            base.velocity.x = 1.0;
        }
        assert_eq!(hit.map(|pos| pos.x), Some(2));
    }

    #[test]
    fn chest_boats_carry_a_chest_and_one_seat() {
        let (base, state) = boat(EntityKind::OakChestBoat, Vec3::ZERO);
        assert!(state.cargo.is_some());
        assert_eq!(max_passengers(&base), 1);
        let (base, state) = boat(EntityKind::BambooRaft, Vec3::ZERO);
        assert!(state.cargo.is_none());
        assert_eq!(max_passengers(&base), 2);
    }

    #[test]
    fn two_riders_sit_front_and_back_along_the_boat() {
        let (mut base, _) = boat(EntityKind::OakBoat, Vec3::ZERO);
        base.yaw = 0.0;
        assert_eq!(seat_offset(&base, 0, 1, false).z, 0.0);
        assert!((seat_offset(&base, 0, 2, false).z - 0.2).abs() < 1.0e-9);
        assert!((seat_offset(&base, 1, 2, false).z + 0.6).abs() < 1.0e-9);
        assert!((seat_offset(&base, 0, 2, true).z - 0.4).abs() < 1.0e-9);
    }

    #[test]
    fn the_chest_survives_the_save_round_trip() {
        let (_, mut state) = boat(EntityKind::OakChestBoat, Vec3::ZERO);
        if let Some(cargo) = state.cargo.as_mut() {
            cargo.items[0] = Some(ItemStack::new(
                ItemKind::Stick.id(),
                NonZeroU8::new(2).unwrap(),
            ));
        }
        let mut nbt = Compound::new();
        write(&state, &mut nbt);
        let (_, mut loaded) = boat(EntityKind::OakChestBoat, Vec3::ZERO);
        read(&nbt, &mut loaded);
        assert_eq!(loaded.cargo, state.cargo);
    }
}
