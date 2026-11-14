use rspigot_blocks::{BlockKind, Face, StateFlags};
use rspigot_domain::entity::EntityBase;
use rspigot_domain::entity::living::LivingState;
use rspigot_domain::entity::physics::has_tag;
use rspigot_domain::entity::{Aabb, raycast};
use rspigot_domain::{BlockPos, Vec3, mth};

use crate::BlockView;
use crate::random::{MobRandom, RandomSource};
use crate::sweep::all_blocks_intersected_between;
use crate::world::MOVEMENT_SPEED;

pub const MAX_HEAD_X_ROT: f32 = 40.0;
pub const MAX_HEAD_Y_ROT: f32 = 75.0;
pub const HEAD_ROT_SPEED: f32 = 10.0;
const MIN_SPEED_SQR: f64 = 2.500_000_277_905_201e-7;
const MAX_TURN: f32 = 90.0;
const LOOK_EPSILON: f64 = 9.999_999_747_378_752e-6;
const HEAD_STABLE_ANGLE: f32 = 15.0;
const DELAY_UNTIL_FACING_FORWARD: i32 = 10;
const TIME_TO_FACE_FORWARD: f32 = 10.0;
const STRAFE_SPEED_MODIFIER: f64 = 0.25;
const HOP_WATER_SPEED: f64 = 1.5;
const FLYING_SPEED: &str = "minecraft:flying_speed";
const GHAST_FLOAT_BASE: i32 = 2;
const GHAST_FLOAT_RANGE: i32 = 5;
const GHAST_THRUST_NUMERATOR: f64 = 5.0;
const GHAST_THRUST_DENOMINATOR: f64 = 3.0;
const SHAPE_REACH_MARGIN: f64 = 1.0e-7;
const SWIM_BUOYANCY: f64 = 0.005;
const SWIM_PITCH_SPEED: f32 = 5.0;
const SWIM_TURN_FREE_ANGLE: f32 = 10.0;
const SWIM_TURN_RANGE: f32 = 50.0;
const SMOOTH_LOOK_YAW_OFFSET: f32 = 20.0;
const SMOOTH_LOOK_PITCH_OFFSET: f32 = 10.0;
const SMOOTH_BODY_TURN: f32 = 4.0;
const FISH_SPEED_LERP: f32 = 0.125;
const FISH_VERTICAL_PUSH: f64 = 0.1;
const FISH_YAW_TURN: f32 = 90.0;
const TURTLE_BUOYANCY: f64 = 0.005;
const DROWNED_BUOYANCY: f64 = 0.002;
const DROWNED_SINK: f64 = -0.008;
const DROWNED_HORIZONTAL_PUSH: f64 = 0.005;
const GUARDIAN_DEGREES_TO_RADIANS: f32 = 0.017_453_292;
const GUARDIAN_SWAY: f64 = 0.5;
const GUARDIAN_BOB: f64 = 0.75;
const GUARDIAN_SWAY_AMPLITUDE: f64 = 0.05;
const GUARDIAN_LOOK_Y: f32 = 10.0;
const GUARDIAN_LOOK_X: f32 = 40.0;
const TURTLE_HOME_RADIUS: f64 = 16.0;
const TURTLE_FAR_WATER_SPEED: f32 = 0.08;
const TURTLE_BABY_WATER_SPEED: f32 = 0.06;
const TURTLE_LAND_SPEED: f32 = 0.06;

pub fn rotate_towards(from: f32, to: f32, max: f32) -> f32 {
    let difference = mth::degrees_difference(from, to);
    from + mth::clamp(difference, -max, max)
}

fn set_speed(living: &mut LivingState, speed: f32) {
    living.speed = speed;
    living.zza = speed;
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LookControl {
    y_max_rot_speed: f32,
    x_max_rot_angle: f32,
    look_at_cooldown: i32,
    wanted: Vec3,
    max_head_x_rot: f32,
    shulker: Option<Face>,
    smooth_max_y_rot_from_center: Option<i32>,
}

impl Default for LookControl {
    fn default() -> Self {
        Self {
            y_max_rot_speed: 0.0,
            x_max_rot_angle: 0.0,
            look_at_cooldown: 0,
            wanted: Vec3::ZERO,
            max_head_x_rot: MAX_HEAD_X_ROT,
            shulker: None,
            smooth_max_y_rot_from_center: None,
        }
    }
}

impl LookControl {
    pub fn set_smooth_swimming(&mut self, max_y_rot_from_center: i32) {
        self.smooth_max_y_rot_from_center = Some(max_y_rot_from_center);
    }

    pub fn smooth_swimming(&self) -> Option<i32> {
        self.smooth_max_y_rot_from_center
    }

    pub fn tick_smooth(
        &mut self,
        base: &mut EntityBase,
        living: &mut LivingState,
        navigation_done: bool,
    ) {
        let Some(max_from_center) = self.smooth_max_y_rot_from_center else {
            return;
        };
        if self.look_at_cooldown > 0 {
            self.look_at_cooldown -= 1;
            if let Some(yaw) = self.y_rot_d(base) {
                base.head_yaw = rotate_towards(
                    base.head_yaw,
                    yaw + SMOOTH_LOOK_YAW_OFFSET,
                    self.y_max_rot_speed,
                );
            }
            if let Some(pitch) = self.x_rot_d(base) {
                let pitch = rotate_towards(
                    base.pitch,
                    pitch + SMOOTH_LOOK_PITCH_OFFSET,
                    self.x_max_rot_angle,
                );
                base.set_pitch(pitch);
            }
        } else {
            if navigation_done {
                let pitch = rotate_towards(base.pitch, 0.0, SWIM_PITCH_SPEED);
                base.set_pitch(pitch);
            }
            base.head_yaw = rotate_towards(base.head_yaw, living.body_yaw, self.y_max_rot_speed);
        }
        let angle = mth::wrap_degrees(base.head_yaw - living.body_yaw);
        let limit = max_from_center as f32;
        if angle < -limit {
            living.body_yaw -= SMOOTH_BODY_TURN;
        } else if angle > limit {
            living.body_yaw += SMOOTH_BODY_TURN;
        }
    }

    pub fn tick_happy_ghast(
        &mut self,
        base: &mut EntityBase,
        living: &mut LivingState,
        stays_still: bool,
    ) {
        if stays_still {
            let yaw = base.yaw;
            base.set_yaw(yaw - wrap_degrees_90(yaw));
            base.head_yaw = base.yaw;
            return;
        }
        if self.look_at_cooldown < 1 {
            let velocity = base.velocity;
            let yaw = -(mth::atan2(velocity.x, velocity.z) as f32) * RAD_TO_DEG_F32;
            base.set_yaw(yaw);
            living.body_yaw = base.yaw;
            return;
        }
        self.look_at_cooldown -= 1;
        let dx = self.wanted.x - base.position.x;
        let dz = self.wanted.z - base.position.z;
        base.set_yaw(-(mth::atan2(dx, dz) as f32) * RAD_TO_DEG_F32);
        living.body_yaw = base.yaw;
        base.head_yaw = living.body_yaw;
    }

    pub fn snap_to_quarter_turn(&mut self, base: &mut EntityBase) {
        let yaw = base.yaw;
        base.set_yaw(yaw - wrap_degrees_90(yaw));
        base.head_yaw = base.yaw;
    }

    pub fn set_shulker_face(&mut self, face: Option<Face>) {
        self.shulker = face;
    }

    pub fn set_max_head_x_rot(&mut self, max: f32) {
        self.max_head_x_rot = max;
    }

    pub fn set_look_at(&mut self, at: Vec3, y_max_rot_speed: f32, x_max_rot_angle: f32) {
        self.wanted = at;
        self.y_max_rot_speed = y_max_rot_speed;
        self.x_max_rot_angle = x_max_rot_angle;
        self.look_at_cooldown = 2;
    }

    pub fn look_at(&mut self, at: Vec3) {
        self.set_look_at(at, HEAD_ROT_SPEED, self.max_head_x_rot);
    }

    pub fn is_looking_at_target(&self) -> bool {
        self.look_at_cooldown > 0
    }

    pub fn wanted(&self) -> Vec3 {
        self.wanted
    }

    fn x_rot_d(&self, base: &EntityBase) -> Option<f32> {
        if self.shulker.is_some() {
            return Some(0.0);
        }
        let eye_y = base.position.y + f64::from(base.eye_height());
        let dx = self.wanted.x - base.position.x;
        let dy = self.wanted.y - eye_y;
        let dz = self.wanted.z - base.position.z;
        let horizontal = (dx * dx + dz * dz).sqrt();
        (dy.abs() > LOOK_EPSILON || horizontal.abs() > LOOK_EPSILON)
            .then(|| (-(mth::atan2(dy, horizontal) * mth::RAD_TO_DEG)) as f32)
    }

    fn y_rot_d(&self, base: &EntityBase) -> Option<f32> {
        if let Some(face) = self.shulker {
            return shulker_y_rot(face, base, self.wanted);
        }
        let dx = self.wanted.x - base.position.x;
        let dz = self.wanted.z - base.position.z;
        (dz.abs() > LOOK_EPSILON || dx.abs() > LOOK_EPSILON)
            .then(|| (mth::atan2(dz, dx) * mth::RAD_TO_DEG) as f32 - 90.0)
    }

    pub fn tick(&mut self, base: &mut EntityBase, living: &LivingState, navigating: bool) {
        self.tick_with(base, living, navigating, true);
    }

    pub fn tick_with(
        &mut self,
        base: &mut EntityBase,
        living: &LivingState,
        navigating: bool,
        reset_x_rot: bool,
    ) {
        if reset_x_rot {
            base.set_pitch(0.0);
        }
        if self.look_at_cooldown > 0 {
            self.look_at_cooldown -= 1;
            if let Some(yaw) = self.y_rot_d(base) {
                base.head_yaw = rotate_towards(base.head_yaw, yaw, self.y_max_rot_speed);
            }
            if let Some(pitch) = self.x_rot_d(base) {
                let pitch = rotate_towards(base.pitch, pitch, self.x_max_rot_angle);
                base.set_pitch(pitch);
            }
        } else {
            base.head_yaw = rotate_towards(base.head_yaw, living.body_yaw, HEAD_ROT_SPEED);
        }
        if navigating && self.shulker.is_none() {
            base.head_yaw =
                mth::rotate_if_necessary(base.head_yaw, living.body_yaw, MAX_HEAD_Y_ROT);
        }
    }
}

const SHULKER_LOOK_EPSILON: f32 = 1.0e-5;
const RAD_TO_DEG_F: f64 = 57.295_776_367_187_5;

fn shulker_forward(lid: Face) -> [f32; 3] {
    match lid {
        Face::Up => [0.0, 0.0, 1.0],
        Face::Down => [0.0, 0.0, -1.0],
        _ => [0.0, -1.0, 0.0],
    }
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub(crate) fn shulker_y_rot(face: Face, base: &EntityBase, wanted: Vec3) -> Option<f32> {
    let lid = face.opposite();
    let forward = shulker_forward(lid);
    let (sx, sy, sz) = lid.offset();
    let side = cross([sx as f32, sy as f32, sz as f32], forward);
    let delta = [
        (wanted.x - base.position.x) as f32,
        (wanted.y - base.eye_y()) as f32,
        (wanted.z - base.position.z) as f32,
    ];
    let lateral = dot(side, delta);
    let ahead = dot(forward, delta);
    if lateral.abs() <= SHULKER_LOOK_EPSILON && ahead.abs() <= SHULKER_LOOK_EPSILON {
        return None;
    }
    Some((mth::atan2(f64::from(-lateral), f64::from(ahead)) * RAD_TO_DEG_F) as f32)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Operation {
    #[default]
    Wait,
    MoveTo,
    Strafe,
    Jumping,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MoveStyle {
    #[default]
    Ground,
    Ghast,
    Flying,
    Vex,
    Phantom,
    SmoothSwimming,
    Fish,
    Turtle,
    Guardian,
    Drowned,
    Cube,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct CubeMove {
    pub y_rot: f32,
    pub aggressive: bool,
    pub jump_delay: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DrownedSwim {
    pub wants_to_swim: bool,
    pub searching_for_land: bool,
    pub target_above: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SmoothSwimming {
    pub max_turn_x: i32,
    pub max_turn_y: i32,
    pub in_water_speed_modifier: f32,
    pub outside_water_speed_modifier: f32,
    pub apply_gravity: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct MoveControl {
    smooth: SmoothSwimming,
    style: MoveStyle,
    float_duration: i32,
    wanted: Vec3,
    speed_modifier: f64,
    strafe_forwards: f32,
    strafe_right: f32,
    operation: Operation,
    hop: bool,
    in_water: bool,
    next_jump_speed: f64,
    max_turn: i32,
    hovers_in_place: bool,
    phantom_speed: f32,
    cube: CubeMove,
    careful: bool,
}

const RAD_TO_DEG_F32: f32 = 57.295_776;

pub fn wrap_degrees_90(degrees: f32) -> f32 {
    let mut wrapped = degrees % 90.0;
    if wrapped >= 45.0 {
        wrapped -= 90.0;
    }
    if wrapped < -45.0 {
        wrapped += 90.0;
    }
    wrapped
}

fn rotlerp(current: f32, target: f32, max: f32) -> f32 {
    let difference = mth::clamp(mth::wrap_degrees(target - current), -max, max);
    let mut rotated = current + difference;
    if rotated < 0.0 {
        rotated += 360.0;
    } else if rotated > 360.0 {
        rotated -= 360.0;
    }
    rotated
}

fn collided_with_shape_moving_from(
    base: &EntityBase,
    from: Vec3,
    to: Vec3,
    shapes: &[Aabb],
) -> bool {
    let size = Aabb::of_size(from, base.dimensions.width, base.dimensions.height);
    let center = size.center();
    let end = center + (to - from);
    let margin = |extent: f64| extent * 0.5 - SHAPE_REACH_MARGIN;
    let (x, y, z) = (
        margin(size.max_x - size.min_x),
        margin(size.max_y - size.min_y),
        margin(size.max_z - size.min_z),
    );
    shapes.iter().any(|shape| {
        let grown = Aabb::new(
            shape.min_x - x,
            shape.min_y - y,
            shape.min_z - z,
            shape.max_x + x,
            shape.max_y + y,
            shape.max_z + z,
        );
        raycast::contains(&grown, end)
            || raycast::contains(&grown, center)
            || raycast::clip_box(&grown, center, end).is_some()
    })
}

fn block_traversal_possible(
    base: &EntityBase,
    view: &impl BlockView,
    from: Vec3,
    to: Vec3,
    pos: BlockPos,
) -> bool {
    let Some(state) = view.state(pos).filter(|state| !state.is_air()) else {
        return true;
    };
    let origin = Vec3::new(f64::from(pos.x), f64::from(pos.y), f64::from(pos.z));
    let shapes: Vec<Aabb> = state
        .collision_shape()
        .iter()
        .map(|shape| {
            Aabb::new(
                shape.min_x,
                shape.min_y,
                shape.min_z,
                shape.max_x,
                shape.max_y,
                shape.max_z,
            )
            .move_by(origin)
        })
        .collect();
    !collided_with_shape_moving_from(base, from, to, &shapes)
}

const REACH_INFLATE: f64 = 9.999_999_747_378_752e-6;
const HAPPY_GHAST_AVOIDS: &str = "minecraft:happy_ghast_avoids";

fn fluid_height(
    view: &impl BlockView,
    pos: BlockPos,
    flag: StateFlags,
    kind: BlockKind,
) -> Option<f64> {
    let state = view
        .state(pos)
        .filter(|state| state.flags().contains(flag))?;
    if view
        .state(pos.offset(0, 1, 0))
        .is_some_and(|above| above.flags().contains(flag))
    {
        return Some(1.0);
    }
    let amount = if state.kind() == Some(kind) {
        let level = state
            .property("level")
            .and_then(|level| level.parse::<i32>().ok())
            .unwrap_or(0);
        if level >= 8 { 8 } else { 8 - level }
    } else {
        8
    };
    Some(f64::from(amount as f32 / 9.0))
}

fn fluid_box(view: &impl BlockView, pos: BlockPos) -> Option<(Aabb, bool)> {
    let origin = Vec3::new(f64::from(pos.x), f64::from(pos.y), f64::from(pos.z));
    let (height, is_water) = match fluid_height(view, pos, StateFlags::WATER, BlockKind::Water) {
        Some(height) => (height, true),
        None => (
            fluid_height(view, pos, StateFlags::LAVA, BlockKind::Lava)?,
            false,
        ),
    };
    Some((
        Aabb::new(0.0, 0.0, 0.0, 1.0, height, 1.0).move_by(origin),
        is_water,
    ))
}

fn careful_traversal(
    base: &EntityBase,
    view: &impl BlockView,
    path: Option<(Vec3, Vec3)>,
    pos: BlockPos,
    through_water: bool,
    through_lava: bool,
) -> bool {
    let Some(state) = view.state(pos).filter(|state| !state.is_air()) else {
        return true;
    };
    let clear = match path {
        None => state.collision_shape().is_empty(),
        Some((from, to)) => block_traversal_possible(base, view, from, to, pos),
    };
    if has_tag(state, HAPPY_GHAST_AVOIDS) {
        return false;
    }
    let Some((fluid, is_water)) = fluid_box(view, pos) else {
        return clear;
    };
    if let Some((from, to)) = path
        && !collided_with_shape_moving_from(base, from, to, &[fluid])
    {
        return clear;
    }
    if is_water {
        through_water
    } else {
        through_lava
    }
}

pub fn can_reach_carefully(base: &EntityBase, view: &impl BlockView, delta: Vec3) -> bool {
    let current = base.bounding_box();
    let moved = current.move_by(delta);
    let grown = moved.inflate(REACH_INFLATE);
    for x in mth::floor(grown.min_x)..=mth::floor(grown.max_x) {
        for y in mth::floor(grown.min_y)..=mth::floor(grown.max_y) {
            for z in mth::floor(grown.min_z)..=mth::floor(grown.max_z) {
                if !careful_traversal(base, view, None, BlockPos::new(x, y, z), false, false) {
                    return false;
                }
            }
        }
    }
    let in_water = base.fluid.in_water;
    let in_lava = base.fluid.in_lava();
    let from = base.position;
    let to = from + delta;
    all_blocks_intersected_between(from, to, &moved, |pos| {
        current.intersects(&Aabb::block(pos))
            || careful_traversal(base, view, Some((from, to)), pos, in_water, in_lava)
    })
}

pub fn can_reach(base: &EntityBase, view: &impl BlockView, delta: Vec3) -> bool {
    let current = base.bounding_box();
    let moved = current.move_by(delta);
    let from = base.position;
    let to = from + delta;
    all_blocks_intersected_between(from, to, &moved, |pos| {
        current.intersects(&Aabb::block(pos)) || block_traversal_possible(base, view, from, to, pos)
    })
}

impl MoveControl {
    pub fn style(&self) -> MoveStyle {
        self.style
    }

    pub fn set_careful(&mut self, careful: bool) {
        self.careful = careful;
    }

    pub fn set_style(&mut self, style: MoveStyle) {
        self.style = style;
        if style == MoveStyle::Phantom {
            self.phantom_speed = crate::phantom::INITIAL_SPEED;
        }
    }

    pub fn phantom_speed(&self) -> f32 {
        self.phantom_speed
    }

    pub fn tick_phantom(
        &mut self,
        base: &mut EntityBase,
        living: &mut LivingState,
        move_target: Vec3,
    ) {
        crate::phantom::tick_move_control(base, living, &mut self.phantom_speed, move_target);
    }

    pub fn set_cube(&mut self) {
        self.style = MoveStyle::Cube;
    }

    pub fn cube(&self) -> CubeMove {
        self.cube
    }

    pub fn set_cube_direction(&mut self, y_rot: f32, aggressive: bool) {
        self.cube.y_rot = y_rot;
        self.cube.aggressive = aggressive;
    }

    pub fn set_cube_wanted(&mut self, speed_modifier: f64) {
        self.speed_modifier = speed_modifier;
        self.operation = Operation::MoveTo;
    }

    pub fn tick_cube(
        &mut self,
        base: &mut EntityBase,
        living: &mut LivingState,
        jump: &mut JumpControl,
        random: &mut MobRandom,
    ) -> Option<crate::cube::CubeSound> {
        use rspigot_domain::entity::species::cube;
        let yaw = rotlerp(base.yaw, self.cube.y_rot, MAX_TURN);
        base.set_yaw(yaw);
        base.head_yaw = base.yaw;
        living.body_yaw = base.yaw;
        if self.operation != Operation::MoveTo {
            living.zza = 0.0;
            return None;
        }
        self.operation = Operation::Wait;
        let speed = (self.speed_modifier * living.attribute(MOVEMENT_SPEED)) as f32;
        if !base.on_ground {
            set_speed(living, speed);
            return None;
        }
        set_speed(living, speed);
        let delay = self.cube.jump_delay;
        self.cube.jump_delay -= 1;
        if delay >= 1 {
            living.xxa = 0.0;
            living.zza = 0.0;
            set_speed(living, 0.0);
            return None;
        }
        self.cube.jump_delay = cube::jump_delay(base.kind, random);
        if self.cube.aggressive {
            self.cube.jump_delay /= cube::AGGRESSIVE_JUMP_DIVISOR;
        }
        jump.jump();
        Some(crate::cube::CubeSound {
            sound: cube::jump_sound(base.kind, cube::is_tiny(base)),
            volume: cube::sound_volume(base),
            pitch: cube::sound_pitch(base, random),
        })
    }

    pub fn set_smooth_swimming(&mut self, params: SmoothSwimming) {
        self.style = MoveStyle::SmoothSwimming;
        self.smooth = params;
    }

    pub fn smooth_swimming(&self) -> SmoothSwimming {
        self.smooth
    }

    pub fn set_fish(&mut self) {
        self.style = MoveStyle::Fish;
    }

    pub fn tick_smooth_swimming(
        &mut self,
        base: &mut EntityBase,
        living: &mut LivingState,
        navigation_done: bool,
    ) {
        let params = self.smooth;
        if params.apply_gravity && base.fluid.in_water {
            base.velocity = base.velocity + Vec3::new(0.0, SWIM_BUOYANCY, 0.0);
        }
        if self.operation != Operation::MoveTo || navigation_done {
            set_speed(living, 0.0);
            living.xxa = 0.0;
            living.yya = 0.0;
            living.zza = 0.0;
            return;
        }
        let dx = self.wanted.x - base.position.x;
        let dy = self.wanted.y - base.position.y;
        let dz = self.wanted.z - base.position.z;
        if dx * dx + dy * dy + dz * dz < MIN_SPEED_SQR {
            living.zza = 0.0;
            return;
        }
        let yaw = (mth::atan2(dz, dx) * mth::RAD_TO_DEG) as f32 - 90.0;
        base.set_yaw(rotlerp(base.yaw, yaw, params.max_turn_y as f32));
        living.body_yaw = base.yaw;
        base.head_yaw = base.yaw;
        let speed = (self.speed_modifier * living.attribute(MOVEMENT_SPEED)) as f32;
        if base.fluid.in_water {
            set_speed(living, speed * params.in_water_speed_modifier);
            let horizontal = (dx * dx + dz * dz).sqrt();
            if dy.abs() > LOOK_EPSILON || horizontal.abs() > LOOK_EPSILON {
                let pitch =
                    mth::wrap_degrees(-((mth::atan2(dy, horizontal) * mth::RAD_TO_DEG) as f32));
                let limit = params.max_turn_x as f32;
                let pitch = mth::clamp(pitch, -limit, limit);
                base.set_pitch(rotate_towards(base.pitch, pitch, SWIM_PITCH_SPEED));
            }
            let radians = f64::from(base.pitch * mth::DEG_TO_RAD);
            let cos = mth::cos(radians);
            let sin = mth::sin(radians);
            living.zza = cos * speed;
            living.yya = -sin * speed;
        } else {
            let angle = mth::wrap_degrees(base.yaw - yaw).abs();
            let factor =
                1.0 - mth::clamp((angle - SWIM_TURN_FREE_ANGLE) / SWIM_TURN_RANGE, 0.0, 1.0);
            set_speed(living, speed * params.outside_water_speed_modifier * factor);
        }
    }

    pub fn set_drowned(&mut self) {
        self.style = MoveStyle::Drowned;
    }

    pub fn tick_drowned(
        &mut self,
        base: &mut EntityBase,
        living: &mut LivingState,
        navigation_done: bool,
        swimming: DrownedSwim,
    ) -> bool {
        if !(swimming.wants_to_swim && base.fluid.in_water) {
            if !base.on_ground {
                base.velocity = base.velocity + Vec3::new(0.0, DROWNED_SINK, 0.0);
            }
            return false;
        }
        if swimming.target_above || swimming.searching_for_land {
            base.velocity = base.velocity + Vec3::new(0.0, DROWNED_BUOYANCY, 0.0);
        }
        if self.operation != Operation::MoveTo || navigation_done {
            set_speed(living, 0.0);
            return true;
        }
        let dx = self.wanted.x - base.position.x;
        let dy = self.wanted.y - base.position.y;
        let dz = self.wanted.z - base.position.z;
        let length = (dx * dx + dy * dy + dz * dz).sqrt();
        let vertical = dy / length;
        let yaw = (mth::atan2(dz, dx) * mth::RAD_TO_DEG) as f32 - 90.0;
        base.set_yaw(rotlerp(base.yaw, yaw, FISH_YAW_TURN));
        living.body_yaw = base.yaw;
        let target = (self.speed_modifier * living.attribute(MOVEMENT_SPEED)) as f32;
        let lerped = living.speed + FISH_SPEED_LERP * (target - living.speed);
        set_speed(living, lerped);
        let speed = f64::from(living.speed);
        base.velocity = base.velocity
            + Vec3::new(
                speed * dx * DROWNED_HORIZONTAL_PUSH,
                speed * vertical * FISH_VERTICAL_PUSH,
                speed * dz * DROWNED_HORIZONTAL_PUSH,
            );
        true
    }

    pub fn set_guardian(&mut self) {
        self.style = MoveStyle::Guardian;
    }

    pub fn tick_guardian(
        &mut self,
        base: &mut EntityBase,
        living: &mut LivingState,
        navigation_done: bool,
        look: &mut LookControl,
    ) {
        if self.operation != Operation::MoveTo || navigation_done {
            set_speed(living, 0.0);
            rspigot_domain::entity::species::guardian::set_moving(base, false);
            return;
        }
        let delta = Vec3::new(
            self.wanted.x - base.position.x,
            self.wanted.y - base.position.y,
            self.wanted.z - base.position.z,
        );
        let length = delta.length();
        let (x, y, z) = (delta.x / length, delta.y / length, delta.z / length);
        let yaw = (mth::atan2(delta.z, delta.x) * mth::RAD_TO_DEG) as f32 - 90.0;
        base.set_yaw(rotlerp(base.yaw, yaw, FISH_YAW_TURN));
        living.body_yaw = base.yaw;
        let target = (self.speed_modifier * living.attribute(MOVEMENT_SPEED)) as f32;
        let lerped = living.speed + FISH_SPEED_LERP * (target - living.speed);
        set_speed(living, lerped);
        let phase = f64::from(base.tick_count.wrapping_add(base.id.0));
        let sway = (phase * GUARDIAN_SWAY).sin() * GUARDIAN_SWAY_AMPLITUDE;
        let radians = f64::from(base.yaw * GUARDIAN_DEGREES_TO_RADIANS);
        let (cos, sin) = (radians.cos(), radians.sin());
        let bob = (phase * GUARDIAN_BOB).sin() * GUARDIAN_SWAY_AMPLITUDE;
        base.velocity = base.velocity
            + Vec3::new(
                sway * cos,
                bob * (sin + cos) * 0.25 + f64::from(living.speed) * y * FISH_VERTICAL_PUSH,
                sway * sin,
            );
        let ahead = Vec3::new(
            base.position.x + x * 2.0,
            base.eye_y() + y / length,
            base.position.z + z * 2.0,
        );
        let mut from = look.wanted();
        if !look.is_looking_at_target() {
            from = ahead;
        }
        let blended = Vec3::new(
            mth::lerp(f64::from(FISH_SPEED_LERP), from.x, ahead.x),
            mth::lerp(f64::from(FISH_SPEED_LERP), from.y, ahead.y),
            mth::lerp(f64::from(FISH_SPEED_LERP), from.z, ahead.z),
        );
        look.set_look_at(blended, GUARDIAN_LOOK_Y, GUARDIAN_LOOK_X);
        rspigot_domain::entity::species::guardian::set_moving(base, true);
    }

    pub fn set_turtle(&mut self) {
        self.style = MoveStyle::Turtle;
    }

    pub fn tick_turtle(
        &mut self,
        base: &mut EntityBase,
        living: &mut LivingState,
        navigation_done: bool,
        home: BlockPos,
        baby: bool,
    ) {
        if base.fluid.in_water {
            base.velocity = base.velocity + Vec3::new(0.0, TURTLE_BUOYANCY, 0.0);
            if !crate::scan::closer_to_center_than(home, base.position, TURTLE_HOME_RADIUS) {
                set_speed(living, (living.speed / 2.0).max(TURTLE_FAR_WATER_SPEED));
            }
            if baby {
                set_speed(living, (living.speed / 3.0).max(TURTLE_BABY_WATER_SPEED));
            }
        } else if base.on_ground {
            set_speed(living, (living.speed / 2.0).max(TURTLE_LAND_SPEED));
        }
        if self.operation != Operation::MoveTo || navigation_done {
            set_speed(living, 0.0);
            return;
        }
        let dx = self.wanted.x - base.position.x;
        let dy = self.wanted.y - base.position.y;
        let dz = self.wanted.z - base.position.z;
        let length = (dx * dx + dy * dy + dz * dz).sqrt();
        if length < LOOK_EPSILON {
            set_speed(living, 0.0);
            return;
        }
        let vertical = dy / length;
        let yaw = (mth::atan2(dz, dx) * mth::RAD_TO_DEG) as f32 - 90.0;
        base.set_yaw(rotlerp(base.yaw, yaw, FISH_YAW_TURN));
        living.body_yaw = base.yaw;
        let target = (self.speed_modifier * living.attribute(MOVEMENT_SPEED)) as f32;
        let lerped = living.speed + FISH_SPEED_LERP * (target - living.speed);
        set_speed(living, lerped);
        base.velocity =
            base.velocity + Vec3::new(0.0, f64::from(living.speed) * vertical * 0.1, 0.0);
    }

    pub fn tick_fish(
        &mut self,
        base: &mut EntityBase,
        living: &mut LivingState,
        navigation_done: bool,
    ) {
        if base.eye_in_water {
            base.velocity = base.velocity + Vec3::new(0.0, SWIM_BUOYANCY, 0.0);
        }
        if self.operation != Operation::MoveTo || navigation_done {
            set_speed(living, 0.0);
            return;
        }
        let target = (self.speed_modifier * living.attribute(MOVEMENT_SPEED)) as f32;
        let lerped = living.speed + FISH_SPEED_LERP * (target - living.speed);
        set_speed(living, lerped);
        let dx = self.wanted.x - base.position.x;
        let dy = self.wanted.y - base.position.y;
        let dz = self.wanted.z - base.position.z;
        if dy != 0.0 {
            let length = (dx * dx + dy * dy + dz * dz).sqrt();
            base.velocity = base.velocity
                + Vec3::new(
                    0.0,
                    f64::from(living.speed) * (dy / length) * FISH_VERTICAL_PUSH,
                    0.0,
                );
        }
        if dx != 0.0 || dz != 0.0 {
            let yaw = (mth::atan2(dz, dx) * mth::RAD_TO_DEG) as f32 - 90.0;
            base.set_yaw(rotlerp(base.yaw, yaw, FISH_YAW_TURN));
            living.body_yaw = base.yaw;
        }
    }

    pub fn set_flying(&mut self, max_turn: i32, hovers_in_place: bool) {
        self.style = MoveStyle::Flying;
        self.max_turn = max_turn;
        self.hovers_in_place = hovers_in_place;
    }

    pub fn max_turn(&self) -> i32 {
        self.max_turn
    }

    pub fn hovers_in_place(&self) -> bool {
        self.hovers_in_place
    }

    pub fn tick_vex(
        &mut self,
        base: &mut EntityBase,
        living: &mut LivingState,
        target: Option<Vec3>,
    ) {
        if self.operation != Operation::MoveTo {
            return;
        }
        if !crate::vex::tick_move_control(base, living, self.wanted, self.speed_modifier, target) {
            self.operation = Operation::Wait;
        }
    }

    pub fn tick_flying(&mut self, base: &mut EntityBase, living: &mut LivingState) {
        if self.operation != Operation::MoveTo {
            if !self.hovers_in_place {
                base.set_no_gravity(false);
            }
            living.yya = 0.0;
            living.zza = 0.0;
            return;
        }
        self.operation = Operation::Wait;
        base.set_no_gravity(true);
        let dx = self.wanted.x - base.position.x;
        let dy = self.wanted.y - base.position.y;
        let dz = self.wanted.z - base.position.z;
        if dx * dx + dy * dy + dz * dz < MIN_SPEED_SQR {
            living.yya = 0.0;
            living.zza = 0.0;
            return;
        }
        let yaw = (mth::atan2(dz, dx) * mth::RAD_TO_DEG) as f32 - 90.0;
        base.set_yaw(rotlerp(base.yaw, yaw, MAX_TURN));
        let attribute = if base.on_ground {
            MOVEMENT_SPEED
        } else {
            FLYING_SPEED
        };
        let speed = (self.speed_modifier * living.attribute(attribute)) as f32;
        set_speed(living, speed);
        let horizontal = (dx * dx + dz * dz).sqrt();
        if dy.abs() > LOOK_EPSILON || horizontal.abs() > LOOK_EPSILON {
            let pitch = (-(mth::atan2(dy, horizontal) * mth::RAD_TO_DEG)) as f32;
            base.set_pitch(rotlerp(base.pitch, pitch, self.max_turn as f32));
            living.yya = if dy > 0.0 { speed } else { -speed };
        }
    }

    pub fn float_duration(&self) -> i32 {
        self.float_duration
    }

    pub fn tick_ghast(
        &mut self,
        base: &mut EntityBase,
        living: &mut LivingState,
        view: &impl BlockView,
        random: &mut MobRandom,
    ) {
        if self.operation != Operation::MoveTo {
            return;
        }
        let ticks = self.float_duration;
        self.float_duration -= 1;
        if ticks > 0 {
            return;
        }
        self.float_duration += random.next_int(GHAST_FLOAT_RANGE) + GHAST_FLOAT_BASE;
        let delta = self.wanted - base.position;
        let reachable = if self.careful {
            can_reach_carefully(base, view, delta)
        } else {
            can_reach(base, view, delta)
        };
        if reachable {
            let thrust =
                living.attribute(FLYING_SPEED) * GHAST_THRUST_NUMERATOR / GHAST_THRUST_DENOMINATOR;
            base.velocity = base.velocity + delta.normalize().scale(thrust);
        } else {
            self.operation = Operation::Wait;
        }
    }

    pub fn has_wanted(&self) -> bool {
        self.operation == Operation::MoveTo
    }

    pub fn operation(&self) -> Operation {
        self.operation
    }

    pub fn wanted(&self) -> Vec3 {
        self.wanted
    }

    pub fn speed_modifier(&self) -> f64 {
        self.speed_modifier
    }

    pub fn enable_hop(&mut self) {
        self.hop = true;
    }

    pub fn set_in_water(&mut self, in_water: bool) {
        self.in_water = in_water;
    }

    pub fn next_jump_speed(&self) -> f64 {
        self.next_jump_speed
    }

    pub fn stop_in_place(&mut self) {
        self.operation = Operation::Wait;
        self.strafe_forwards = 0.0;
        self.strafe_right = 0.0;
    }

    pub fn set_wanted_position(&mut self, position: Vec3, speed_modifier: f64) {
        let speed_modifier = if self.hop && self.in_water {
            HOP_WATER_SPEED
        } else {
            speed_modifier
        };
        if self.hop && speed_modifier > 0.0 {
            self.next_jump_speed = speed_modifier;
        }
        self.wanted = position;
        self.speed_modifier = speed_modifier;
        if self.operation != Operation::Jumping {
            self.operation = Operation::MoveTo;
        }
    }

    pub fn strafe(&mut self, forwards: f32, right: f32) {
        self.operation = Operation::Strafe;
        self.strafe_forwards = forwards;
        self.strafe_right = right;
        self.speed_modifier = STRAFE_SPEED_MODIFIER;
    }

    pub fn set_wait(&mut self) {
        self.operation = Operation::Wait;
    }

    pub fn tick(
        &mut self,
        base: &mut EntityBase,
        living: &mut LivingState,
        view: &impl BlockView,
        jump: &mut JumpControl,
        walkable: impl Fn(BlockPos) -> bool,
    ) {
        match self.operation {
            Operation::Strafe => self.tick_strafe(base, living, walkable),
            Operation::MoveTo => self.tick_move_to(base, living, view, jump),
            Operation::Jumping => {
                let speed = self.speed_modifier * living.attribute(MOVEMENT_SPEED);
                set_speed(living, speed as f32);
                if base.on_ground || base.fluid.in_liquid() {
                    self.operation = Operation::Wait;
                }
            }
            Operation::Wait => living.zza = 0.0,
        }
    }

    fn tick_strafe(
        &mut self,
        base: &EntityBase,
        living: &mut LivingState,
        walkable: impl Fn(BlockPos) -> bool,
    ) {
        let attribute = living.attribute(MOVEMENT_SPEED) as f32;
        let speed = self.speed_modifier as f32 * attribute;
        let mut forwards = self.strafe_forwards;
        let mut right = self.strafe_right;
        let mut length = (forwards * forwards + right * right).sqrt();
        if length < 1.0 {
            length = 1.0;
        }
        length = speed / length;
        forwards *= length;
        right *= length;
        let radians = f64::from(base.yaw * mth::DEG_TO_RAD);
        let sin = mth::sin(radians);
        let cos = mth::cos(radians);
        let x = forwards * cos - right * sin;
        let z = right * cos + forwards * sin;
        let target = Vec3::new(
            base.position.x + f64::from(x),
            f64::from(base.position.block().y),
            base.position.z + f64::from(z),
        );
        if !walkable(target.block()) {
            self.strafe_forwards = 1.0;
            self.strafe_right = 0.0;
        }
        set_speed(living, speed);
        living.zza = self.strafe_forwards;
        living.xxa = self.strafe_right;
        self.operation = Operation::Wait;
    }

    fn tick_move_to(
        &mut self,
        base: &mut EntityBase,
        living: &mut LivingState,
        view: &impl BlockView,
        jump: &mut JumpControl,
    ) {
        self.operation = Operation::Wait;
        let dx = self.wanted.x - base.position.x;
        let dz = self.wanted.z - base.position.z;
        let dy = self.wanted.y - base.position.y;
        let distance = dx * dx + dy * dy + dz * dz;
        if distance < MIN_SPEED_SQR {
            living.zza = 0.0;
            return;
        }
        let yaw = (mth::atan2(dz, dx) * mth::RAD_TO_DEG) as f32 - 90.0;
        base.set_yaw(rotlerp(base.yaw, yaw, MAX_TURN));
        let speed = self.speed_modifier * living.attribute(MOVEMENT_SPEED);
        set_speed(living, speed as f32);
        let pos = base.position.block();
        let state = view.state(pos);
        let shape = state.map_or(&[][..], |state| state.collision_shape());
        let step = living.attribute(rspigot_domain::entity::living::STEP_HEIGHT) as f32;
        let width = base.dimensions.width;
        let climb = dy > f64::from(step) && dx * dx + dz * dz < f64::from(1.0f32.max(width));
        let top = shape
            .iter()
            .map(|b| b.max_y)
            .reduce(f64::max)
            .unwrap_or(0.0);
        let blocked = !shape.is_empty()
            && base.position.y < top + f64::from(pos.y)
            && state.is_some_and(|state| {
                !has_tag(state, "minecraft:doors") && !has_tag(state, "minecraft:fences")
            });
        if climb || blocked {
            jump.jump();
            self.operation = Operation::Jumping;
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct JumpControl {
    jump: bool,
}

impl JumpControl {
    pub fn jump(&mut self) {
        self.jump = true;
    }

    pub fn is_jumping(&self) -> bool {
        self.jump
    }

    pub fn take(&mut self) -> bool {
        std::mem::take(&mut self.jump)
    }

    pub fn tick(&mut self, living: &mut LivingState) {
        living.jumping = self.jump;
        self.jump = false;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct BodyRotationControl {
    head_stable_time: i32,
    last_stable_y_head_rot: f32,
}

impl BodyRotationControl {
    pub fn client_tick(&mut self, base: &mut EntityBase, living: &mut LivingState) {
        self.client_tick_max(base, living, MAX_HEAD_Y_ROT);
    }

    pub fn client_tick_max(
        &mut self,
        base: &mut EntityBase,
        living: &mut LivingState,
        max_head_y_rot: f32,
    ) {
        let dx = base.position.x - base.old_position.x;
        let dz = base.position.z - base.old_position.z;
        if dx * dx + dz * dz > MIN_SPEED_SQR {
            living.body_yaw = base.yaw;
            base.head_yaw =
                mth::rotate_if_necessary(base.head_yaw, living.body_yaw, max_head_y_rot);
            self.last_stable_y_head_rot = base.head_yaw;
            self.head_stable_time = 0;
            return;
        }
        if (base.head_yaw - self.last_stable_y_head_rot).abs() > HEAD_STABLE_ANGLE {
            self.head_stable_time = 0;
            self.last_stable_y_head_rot = base.head_yaw;
            living.body_yaw =
                mth::rotate_if_necessary(living.body_yaw, base.head_yaw, max_head_y_rot);
        } else {
            self.head_stable_time += 1;
            if self.head_stable_time > DELAY_UNTIL_FACING_FORWARD {
                let elapsed = self.head_stable_time - DELAY_UNTIL_FACING_FORWARD;
                let progress = mth::clamp(elapsed as f32 / TIME_TO_FACE_FORWARD, 0.0, 1.0);
                let max = max_head_y_rot * (1.0 - progress);
                living.body_yaw = mth::rotate_if_necessary(living.body_yaw, base.head_yaw, max);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use rspigot_blocks::{BlockKind, StateId};
    use rspigot_domain::EntityId;
    use rspigot_domain::entity::living::LivingState;
    use rspigot_entities::EntityKind;
    use uuid::Uuid;

    use super::*;

    struct Floor;

    impl BlockView for Floor {
        fn state(&self, pos: BlockPos) -> Option<StateId> {
            Some(if pos.y < 64 {
                BlockKind::Stone.default_state()
            } else {
                StateId::AIR
            })
        }
    }

    fn cow() -> (EntityBase, LivingState) {
        let mut base = EntityBase::new(EntityId(1), Uuid::from_u128(1), EntityKind::Cow);
        base.position = Vec3::new(0.5, 64.0, 0.5);
        base.old_position = base.position;
        base.on_ground = true;
        let living = LivingState::new(&mut base, 0.0);
        (base, living)
    }

    #[test]
    fn look_control_turns_the_head_at_its_speed_then_back_to_the_body() {
        let (mut base, mut living) = cow();
        let mut look = LookControl::default();
        look.look_at(Vec3::new(10.5, 65.3, 0.5));
        assert!(look.is_looking_at_target());
        look.tick(&mut base, &living, false);
        assert_eq!(base.head_yaw, -10.0);
        assert!(base.pitch.abs() < 1.0e-5, "{}", base.pitch);
        look.tick(&mut base, &living, false);
        assert_eq!(base.head_yaw, -20.0);
        assert!(!look.is_looking_at_target());
        look.tick(&mut base, &living, false);
        assert_eq!(base.head_yaw, -10.0);
        look.set_look_at(Vec3::new(0.5, 75.3, 10.5), 180.0, 40.0);
        look.tick(&mut base, &living, false);
        assert!(base.head_yaw.abs() < 1.0e-5, "{}", base.head_yaw);
        assert_eq!(base.pitch, -40.0);
        living.body_yaw = 0.0;
        base.head_yaw = 120.0;
        look.set_look_at(Vec3::new(-10.5, 65.3, 0.5), 0.0, 0.0);
        look.tick(&mut base, &living, true);
        assert_eq!(base.head_yaw, 75.0);
    }

    #[test]
    fn move_control_turns_sets_speed_and_waits() {
        let (mut base, mut living) = cow();
        let mut movement = MoveControl::default();
        let mut jump = JumpControl::default();
        movement.set_wanted_position(Vec3::new(5.5, 64.0, 0.5), 1.25);
        assert!(movement.has_wanted());
        movement.tick(&mut base, &mut living, &Floor, &mut jump, |_| true);
        assert_eq!(base.yaw, 270.0);
        let expected = (1.25 * f64::from(0.2f32)) as f32;
        assert_eq!(living.speed, expected);
        assert_eq!(living.zza, expected);
        assert_eq!(movement.operation(), Operation::Wait);
        assert!(!jump.is_jumping());
        movement.tick(&mut base, &mut living, &Floor, &mut jump, |_| true);
        assert_eq!(living.zza, 0.0);
        movement.set_wanted_position(base.position, 1.0);
        living.zza = 1.0;
        movement.tick(&mut base, &mut living, &Floor, &mut jump, |_| true);
        assert_eq!(living.zza, 0.0);
    }

    #[test]
    fn move_control_jumps_towards_higher_targets_until_landing() {
        let (mut base, mut living) = cow();
        let mut movement = MoveControl::default();
        let mut jump = JumpControl::default();
        movement.set_wanted_position(Vec3::new(0.9, 65.0, 0.5), 1.0);
        movement.tick(&mut base, &mut living, &Floor, &mut jump, |_| true);
        assert!(jump.is_jumping());
        assert_eq!(movement.operation(), Operation::Jumping);
        movement.set_wanted_position(Vec3::new(3.5, 65.0, 0.5), 1.0);
        assert_eq!(movement.operation(), Operation::Jumping);
        base.on_ground = false;
        movement.tick(&mut base, &mut living, &Floor, &mut jump, |_| true);
        assert_eq!(movement.operation(), Operation::Jumping);
        base.on_ground = true;
        movement.tick(&mut base, &mut living, &Floor, &mut jump, |_| true);
        assert_eq!(movement.operation(), Operation::Wait);
        jump.tick(&mut living);
        assert!(living.jumping);
        jump.tick(&mut living);
        assert!(!living.jumping);
    }

    #[test]
    fn strafing_falls_back_to_forwards_when_the_side_is_not_walkable() {
        let (mut base, mut living) = cow();
        let mut movement = MoveControl::default();
        let mut jump = JumpControl::default();
        movement.strafe(0.0, 1.0);
        movement.tick(&mut base, &mut living, &Floor, &mut jump, |_| false);
        assert_eq!(living.zza, 1.0);
        assert_eq!(living.xxa, 0.0);
        assert_eq!(living.speed, 0.25 * 0.2f32);
        movement.strafe(-1.0, 0.5);
        movement.tick(&mut base, &mut living, &Floor, &mut jump, |_| true);
        assert_eq!(living.zza, -1.0);
        assert_eq!(living.xxa, 0.5);
        assert_eq!(movement.operation(), Operation::Wait);
    }

    #[test]
    fn body_follows_movement_then_a_stable_head() {
        let (mut base, mut living) = cow();
        let mut body = BodyRotationControl::default();
        base.yaw = 30.0;
        base.head_yaw = 140.0;
        base.position = Vec3::new(0.6, 64.0, 0.5);
        body.client_tick(&mut base, &mut living);
        assert_eq!(living.body_yaw, 30.0);
        assert_eq!(base.head_yaw, 105.0);
        base.old_position = base.position;
        base.head_yaw = 160.0;
        body.client_tick(&mut base, &mut living);
        assert_eq!(living.body_yaw, 85.0);
        for _ in 0..10 {
            body.client_tick(&mut base, &mut living);
        }
        assert_eq!(living.body_yaw, 85.0);
        for _ in 0..10 {
            body.client_tick(&mut base, &mut living);
        }
        assert_eq!(living.body_yaw, 160.0);
    }

    fn ghast() -> (EntityBase, LivingState) {
        let mut base = EntityBase::new(EntityId(1), Uuid::from_u128(1), EntityKind::Ghast);
        base.position = Vec3::new(0.5, 80.0, 0.5);
        base.old_position = base.position;
        let living = LivingState::new(&mut base, 0.0);
        (base, living)
    }

    struct Air;

    impl BlockView for Air {
        fn state(&self, _pos: BlockPos) -> Option<StateId> {
            Some(StateId::AIR)
        }
    }

    struct Wall(BlockPos);

    impl BlockView for Wall {
        fn state(&self, pos: BlockPos) -> Option<StateId> {
            Some(if pos.x == self.0.x {
                BlockKind::Stone.default_state()
            } else {
                StateId::AIR
            })
        }
    }

    #[test]
    fn the_ghast_control_is_idle_without_a_wanted_position() {
        let (mut base, mut living) = ghast();
        let mut movement = MoveControl::default();
        movement.set_style(MoveStyle::Ghast);
        let mut random = MobRandom::new(1);
        movement.tick_ghast(&mut base, &mut living, &Air, &mut random);
        assert_eq!(base.velocity, Vec3::ZERO);
        assert_eq!(movement.float_duration(), 0);
    }

    #[test]
    fn the_ghast_control_pushes_a_third_of_five_flying_speeds_along_the_way_every_few_ticks() {
        let (mut base, mut living) = ghast();
        let mut movement = MoveControl::default();
        movement.set_style(MoveStyle::Ghast);
        movement.set_wanted_position(Vec3::new(10.5, 80.0, 0.5), 1.0);
        let mut random = MobRandom::new(7);
        let mut mirror = random.clone();
        movement.tick_ghast(&mut base, &mut living, &Air, &mut random);
        let thrust = 0.06 * 5.0 / 3.0;
        assert_eq!(base.velocity, Vec3::new(thrust, 0.0, 0.0));
        let duration = mirror.next_int(5) + 2;
        assert_eq!(movement.float_duration(), -1 + duration);
        assert_eq!(movement.operation(), Operation::MoveTo);
        for _ in 0..duration - 1 {
            movement.tick_ghast(&mut base, &mut living, &Air, &mut random);
            assert_eq!(base.velocity, Vec3::new(thrust, 0.0, 0.0));
        }
        assert_eq!(movement.float_duration(), 0);
        movement.tick_ghast(&mut base, &mut living, &Air, &mut random);
        assert_eq!(base.velocity.x, thrust * 2.0);
        assert!(movement.float_duration() >= 1);
    }

    #[test]
    fn the_ghast_control_gives_up_when_a_wall_is_in_the_way() {
        let (mut base, mut living) = ghast();
        let mut movement = MoveControl::default();
        movement.set_style(MoveStyle::Ghast);
        movement.set_wanted_position(Vec3::new(10.5, 80.0, 0.5), 1.0);
        let mut random = MobRandom::new(7);
        movement.tick_ghast(
            &mut base,
            &mut living,
            &Wall(BlockPos::new(5, 80, 0)),
            &mut random,
        );
        assert_eq!(base.velocity, Vec3::ZERO);
        assert_eq!(movement.operation(), Operation::Wait);
        assert!(!movement.has_wanted());
    }

    #[test]
    fn a_ghast_can_reach_through_the_blocks_it_already_overlaps_but_not_through_a_wall() {
        let (base, _) = ghast();
        assert!(can_reach(&base, &Air, Vec3::new(8.0, 0.0, 0.0)));
        assert!(can_reach(&base, &Air, Vec3::ZERO));
        assert!(!can_reach(
            &base,
            &Wall(BlockPos::new(5, 80, 0)),
            Vec3::new(8.0, 0.0, 0.0)
        ));
        assert!(can_reach(
            &base,
            &Wall(BlockPos::new(0, 80, 0)),
            Vec3::new(0.0, 0.0, 0.5)
        ));
        assert!(can_reach(
            &base,
            &Wall(BlockPos::new(-5, 80, 0)),
            Vec3::new(8.0, 0.0, 0.0)
        ));
    }

    fn parrot() -> (EntityBase, LivingState) {
        let mut base = EntityBase::new(EntityId(1), Uuid::from_u128(1), EntityKind::Parrot);
        base.position = Vec3::new(0.5, 80.0, 0.5);
        base.old_position = base.position;
        let living = LivingState::new(&mut base, 0.0);
        (base, living)
    }

    #[test]
    fn the_flying_control_removes_gravity_turns_and_climbs_at_the_flying_speed() {
        let (mut base, mut living) = parrot();
        let mut movement = MoveControl::default();
        movement.set_flying(10, false);
        assert_eq!(movement.style(), MoveStyle::Flying);
        movement.set_wanted_position(Vec3::new(5.5, 85.0, 0.5), 1.0);
        movement.tick_flying(&mut base, &mut living);
        assert!(base.is_no_gravity());
        assert_eq!(base.yaw, 270.0);
        let speed = living.attribute(FLYING_SPEED) as f32;
        assert_eq!(living.speed, speed);
        assert_eq!(living.zza, speed);
        assert_eq!(living.yya, speed);
        assert_eq!(base.pitch, 90.0);
        assert_eq!(movement.operation(), Operation::Wait);
    }

    #[test]
    fn the_flying_control_sinks_when_the_target_is_below_and_uses_the_ground_speed_on_the_ground() {
        let (mut base, mut living) = parrot();
        let mut movement = MoveControl::default();
        movement.set_flying(10, false);
        movement.set_wanted_position(Vec3::new(0.5, 70.0, 5.5), 0.5);
        movement.tick_flying(&mut base, &mut living);
        let speed = (0.5 * living.attribute(FLYING_SPEED)) as f32;
        assert_eq!(living.yya, -speed);
        assert_eq!(base.pitch, 10.0);
        base.on_ground = true;
        movement.set_wanted_position(Vec3::new(5.5, 80.0, 0.5), 1.0);
        movement.tick_flying(&mut base, &mut living);
        let ground = living.attribute(MOVEMENT_SPEED) as f32;
        assert_eq!(living.speed, ground);
    }

    #[test]
    fn the_flying_control_stops_when_it_arrived_and_restores_gravity_unless_it_hovers() {
        let (mut base, mut living) = parrot();
        let mut movement = MoveControl::default();
        movement.set_flying(10, false);
        movement.set_wanted_position(base.position, 1.0);
        living.zza = 1.0;
        living.yya = 1.0;
        movement.tick_flying(&mut base, &mut living);
        assert_eq!((living.zza, living.yya), (0.0, 0.0));
        assert!(base.is_no_gravity());
        movement.tick_flying(&mut base, &mut living);
        assert!(!base.is_no_gravity());
        let mut hover = MoveControl::default();
        hover.set_flying(10, true);
        base.set_no_gravity(true);
        hover.tick_flying(&mut base, &mut living);
        assert!(base.is_no_gravity());
    }

    fn dolphin() -> (EntityBase, LivingState) {
        let mut base = EntityBase::new(EntityId(1), Uuid::from_u128(1), EntityKind::Dolphin);
        base.position = Vec3::new(0.5, 64.0, 0.5);
        base.old_position = base.position;
        let living = LivingState::new(&mut base, 0.0);
        (base, living)
    }

    fn dolphin_control() -> MoveControl {
        let mut movement = MoveControl::default();
        movement.set_smooth_swimming(SmoothSwimming {
            max_turn_x: 85,
            max_turn_y: 10,
            in_water_speed_modifier: 0.02,
            outside_water_speed_modifier: 0.1,
            apply_gravity: true,
        });
        movement
    }

    #[test]
    fn the_smooth_swimming_control_turns_the_body_at_the_y_limit_and_pitches_toward_the_target() {
        let (mut base, mut living) = dolphin();
        base.fluid.in_water = true;
        let mut movement = dolphin_control();
        movement.set_wanted_position(Vec3::new(5.5, 69.0, 0.5), 1.0);
        movement.tick_smooth_swimming(&mut base, &mut living, false);
        assert_eq!(base.yaw, 350.0);
        assert_eq!(living.body_yaw, base.yaw);
        assert_eq!(base.head_yaw, base.yaw);
        assert_eq!(base.pitch, -5.0);
        let speed = (1.0 * living.attribute(MOVEMENT_SPEED)) as f32;
        assert_eq!(living.speed, speed * 0.02);
        let radians = f64::from(base.pitch * mth::DEG_TO_RAD);
        assert_eq!(living.zza, mth::cos(radians) * speed);
        assert_eq!(living.yya, -mth::sin(radians) * speed);
        assert!(living.yya > 0.0);
        assert_eq!(base.velocity, Vec3::new(0.0, 0.005, 0.0));
        assert_eq!(movement.operation(), Operation::MoveTo);
    }

    #[test]
    fn the_smooth_swimming_pitch_is_clamped_by_the_x_limit_and_moves_five_degrees_per_tick() {
        let (mut base, mut living) = dolphin();
        base.fluid.in_water = true;
        let mut movement = MoveControl::default();
        movement.set_smooth_swimming(SmoothSwimming {
            max_turn_x: 2,
            max_turn_y: 10,
            in_water_speed_modifier: 0.02,
            outside_water_speed_modifier: 0.1,
            apply_gravity: false,
        });
        movement.set_wanted_position(Vec3::new(0.5, 80.0, 1.5), 1.0);
        movement.tick_smooth_swimming(&mut base, &mut living, false);
        assert_eq!(base.pitch, -2.0);
        assert_eq!(base.velocity, Vec3::ZERO);
    }

    #[test]
    fn the_smooth_swimming_control_slows_by_the_turning_angle_out_of_the_water() {
        let (mut base, mut living) = dolphin();
        base.yaw = 270.0;
        let mut movement = dolphin_control();
        movement.set_wanted_position(Vec3::new(5.5, 64.0, 0.5), 1.0);
        movement.tick_smooth_swimming(&mut base, &mut living, false);
        let speed = (1.0 * living.attribute(MOVEMENT_SPEED)) as f32;
        assert_eq!(base.yaw, 270.0);
        assert_eq!(living.speed, speed * 0.1 * 1.0);
        let (mut base, mut living) = dolphin();
        base.yaw = 300.0;
        let mut movement = dolphin_control();
        movement.set_wanted_position(Vec3::new(5.5, 64.0, 0.5), 1.0);
        movement.tick_smooth_swimming(&mut base, &mut living, false);
        assert_eq!(base.yaw, 290.0);
        let factor = 1.0 - mth::clamp((20.0 - 10.0) / 50.0, 0.0, 1.0);
        assert_eq!(living.speed, speed * 0.1 * factor);
        let (mut base, mut living) = dolphin();
        base.yaw = 130.0;
        let mut movement = dolphin_control();
        movement.set_wanted_position(Vec3::new(5.5, 64.0, 0.5), 1.0);
        movement.tick_smooth_swimming(&mut base, &mut living, false);
        assert_eq!(living.speed, 0.0);
    }

    #[test]
    fn the_smooth_swimming_control_stops_without_a_path_or_a_wanted_move() {
        let (mut base, mut living) = dolphin();
        base.fluid.in_water = true;
        living.speed = 0.5;
        living.zza = 0.5;
        living.yya = 0.5;
        living.xxa = 0.5;
        let mut movement = dolphin_control();
        movement.set_wanted_position(Vec3::new(5.5, 64.0, 0.5), 1.0);
        movement.tick_smooth_swimming(&mut base, &mut living, true);
        assert_eq!(
            (living.speed, living.zza, living.yya, living.xxa),
            (0.0, 0.0, 0.0, 0.0)
        );
        assert_eq!(base.velocity.y, 0.005);
        let mut idle = dolphin_control();
        living.zza = 0.5;
        idle.tick_smooth_swimming(&mut base, &mut living, false);
        assert_eq!(living.zza, 0.0);
    }

    #[test]
    fn the_smooth_swimming_control_only_zeroes_the_forward_input_when_already_there() {
        let (mut base, mut living) = dolphin();
        let mut movement = dolphin_control();
        movement.set_wanted_position(base.position, 1.0);
        living.zza = 1.0;
        movement.tick_smooth_swimming(&mut base, &mut living, false);
        assert_eq!(living.zza, 0.0);
        assert_eq!(base.yaw, 0.0);
    }

    fn cod() -> (EntityBase, LivingState) {
        let mut base = EntityBase::new(EntityId(1), Uuid::from_u128(1), EntityKind::Cod);
        base.position = Vec3::new(0.5, 64.0, 0.5);
        base.old_position = base.position;
        let living = LivingState::new(&mut base, 0.0);
        (base, living)
    }

    #[test]
    fn the_fish_control_lerps_the_speed_pushes_vertically_and_turns_the_body() {
        let (mut base, mut living) = cod();
        base.eye_in_water = true;
        let mut movement = MoveControl::default();
        movement.set_fish();
        movement.set_wanted_position(Vec3::new(5.5, 69.0, 0.5), 1.0);
        movement.tick_fish(&mut base, &mut living, false);
        let target = (1.0 * living.attribute(MOVEMENT_SPEED)) as f32;
        let lerped = 0.0 + 0.125 * (target - 0.0);
        assert_eq!(living.speed, lerped);
        assert_eq!(living.zza, lerped);
        let length = (25.0f64 + 25.0).sqrt();
        let push = f64::from(lerped) * (5.0 / length) * 0.1;
        assert_eq!(base.velocity, Vec3::new(0.0, 0.005 + push, 0.0));
        assert_eq!(base.yaw, 270.0);
        assert_eq!(living.body_yaw, 270.0);
    }

    #[test]
    fn the_fish_control_stops_when_the_navigation_is_done_and_keeps_its_operation() {
        let (mut base, mut living) = cod();
        living.speed = 0.3;
        let mut movement = MoveControl::default();
        movement.set_fish();
        movement.set_wanted_position(Vec3::new(5.5, 64.0, 0.5), 1.0);
        movement.tick_fish(&mut base, &mut living, true);
        assert_eq!(living.speed, 0.0);
        assert_eq!(base.velocity, Vec3::ZERO);
        assert_eq!(movement.operation(), Operation::MoveTo);
        let mut level = MoveControl::default();
        level.set_fish();
        level.set_wanted_position(Vec3::new(0.5, 64.0, 5.5), 1.0);
        level.tick_fish(&mut base, &mut living, false);
        assert_eq!(base.velocity.y, 0.0);
    }

    #[test]
    fn the_smooth_swimming_look_turns_the_head_with_offsets_and_drags_the_body_beyond_the_limit() {
        let (mut base, mut living) = dolphin();
        let mut look = LookControl::default();
        look.set_smooth_swimming(10);
        look.set_look_at(Vec3::new(10.5, 74.0, 0.5), 40.0, 30.0);
        look.tick_smooth(&mut base, &mut living, false);
        assert_eq!(base.head_yaw, -40.0);
        assert_eq!(living.body_yaw, -4.0);
        let pitch = base.pitch;
        assert!((-30.0..0.0).contains(&pitch), "{pitch}");
        look.tick_smooth(&mut base, &mut living, false);
        assert_eq!(base.head_yaw, -70.0);
        assert_eq!(living.body_yaw, -8.0);
    }

    #[test]
    fn the_smooth_swimming_look_levels_the_pitch_only_when_the_navigation_is_done() {
        let (mut base, mut living) = dolphin();
        let mut look = LookControl::default();
        look.set_smooth_swimming(10);
        base.set_pitch(30.0);
        look.tick_smooth(&mut base, &mut living, false);
        assert_eq!(base.pitch, 30.0);
        look.tick_smooth(&mut base, &mut living, true);
        assert_eq!(base.pitch, 25.0);
        base.head_yaw = 40.0;
        living.body_yaw = 0.0;
        look.tick_smooth(&mut base, &mut living, false);
        assert_eq!(base.head_yaw, 40.0);
        assert_eq!(living.body_yaw, 4.0);
        look.set_look_at(Vec3::new(0.5, 64.0, 5.5), 10.0, 5.0);
        look.tick_smooth(&mut base, &mut living, false);
        assert_eq!(base.head_yaw, 30.0);
        assert_eq!(living.body_yaw, 8.0);
        look.tick_smooth(&mut base, &mut living, false);
        assert_eq!(base.head_yaw, 20.0);
        assert_eq!(living.body_yaw, 12.0);
        assert!(!look.is_looking_at_target());
        look.tick_smooth(&mut base, &mut living, false);
        assert_eq!(base.head_yaw, 12.0);
    }

    #[test]
    fn the_style_defaults_to_the_ground_control() {
        assert_eq!(MoveControl::default().style(), MoveStyle::Ground);
    }

    #[test]
    fn rotlerp_wraps_into_a_full_turn() {
        assert_eq!(rotlerp(10.0, -170.0, 90.0), 280.0);
        assert_eq!(rotlerp(350.0, 20.0, 90.0), 20.0);
        assert_eq!(rotate_towards(0.0, 200.0, 10.0), -10.0);
    }

    fn turtle() -> (EntityBase, LivingState) {
        let mut base = EntityBase::new(EntityId(1), Uuid::from_u128(1), EntityKind::Turtle);
        base.position = Vec3::new(0.5, 64.0, 0.5);
        base.old_position = base.position;
        let living = LivingState::new(&mut base, 0.0);
        (base, living)
    }

    fn turtle_control() -> MoveControl {
        let mut movement = MoveControl::default();
        movement.set_turtle();
        movement
    }

    #[test]
    fn a_turtle_in_water_floats_up_and_halves_its_speed_far_from_home_down_to_eight_hundredths() {
        let (mut base, mut living) = turtle();
        base.fluid.in_water = true;
        living.speed = 0.5;
        let mut movement = turtle_control();
        movement.tick_turtle(
            &mut base,
            &mut living,
            true,
            BlockPos::new(100, 64, 0),
            false,
        );
        assert_eq!(base.velocity, Vec3::new(0.0, 0.005, 0.0));
        assert_eq!(living.speed, 0.0);
        living.speed = 0.5;
        let mut movement = turtle_control();
        movement.set_wanted_position(Vec3::new(0.5, 64.0, 5.5), 1.0);
        movement.tick_turtle(
            &mut base,
            &mut living,
            false,
            BlockPos::new(100, 64, 0),
            false,
        );
        let lerped = 0.25 + 0.125 * ((0.25_f64 * 1.0) as f32 - 0.25);
        assert_eq!(living.speed, lerped);
        living.speed = 0.01;
        let mut movement = turtle_control();
        movement.tick_turtle(
            &mut base,
            &mut living,
            true,
            BlockPos::new(100, 64, 0),
            false,
        );
        assert_eq!(living.speed, 0.0);
    }

    #[test]
    fn a_turtle_near_home_keeps_its_swimming_speed_and_a_baby_cuts_it_to_a_third() {
        let (mut base, mut living) = turtle();
        base.fluid.in_water = true;
        living.speed = 0.3;
        let mut movement = turtle_control();
        movement.set_wanted_position(Vec3::new(0.5, 64.0, 5.5), 1.0);
        movement.tick_turtle(
            &mut base,
            &mut living,
            false,
            BlockPos::new(0, 64, 0),
            false,
        );
        let lerped = 0.3 + 0.125 * (0.25 - 0.3);
        assert!((f64::from(living.speed) - lerped).abs() < 1.0e-6);
        let (mut base, mut living) = turtle();
        base.fluid.in_water = true;
        living.speed = 0.3;
        let mut movement = turtle_control();
        movement.set_wanted_position(Vec3::new(0.5, 64.0, 5.5), 1.0);
        movement.tick_turtle(&mut base, &mut living, false, BlockPos::new(0, 64, 0), true);
        let thirded = (0.3_f32 / 3.0).max(0.06);
        let lerped = thirded + 0.125 * (0.25 - thirded);
        assert!((living.speed - lerped).abs() < 1.0e-6);
    }

    #[test]
    fn a_turtle_on_the_ground_halves_its_speed_down_to_six_hundredths_before_steering() {
        let (mut base, mut living) = turtle();
        base.on_ground = true;
        living.speed = 0.4;
        let mut movement = turtle_control();
        movement.set_wanted_position(Vec3::new(5.5, 64.0, 0.5), 1.0);
        movement.tick_turtle(
            &mut base,
            &mut living,
            false,
            BlockPos::new(0, 64, 0),
            false,
        );
        let halved = 0.2_f32;
        let lerped = halved + 0.125 * (0.25 - halved);
        assert!((living.speed - lerped).abs() < 1.0e-6);
        assert_eq!(base.yaw, 270.0);
        assert_eq!(living.body_yaw, 270.0);
        assert_eq!(base.velocity, Vec3::new(0.0, 0.0, 0.0));
    }

    #[test]
    fn a_turtle_steers_vertically_by_a_tenth_of_its_speed_and_stops_when_arrived() {
        let (mut base, mut living) = turtle();
        base.fluid.in_water = true;
        let mut movement = turtle_control();
        movement.set_wanted_position(Vec3::new(0.5, 70.0, 0.5), 1.0);
        movement.tick_turtle(
            &mut base,
            &mut living,
            false,
            BlockPos::new(0, 64, 0),
            false,
        );
        assert!(base.velocity.y > 0.005);
        let expected = 0.005 + f64::from(living.speed) * 1.0 * 0.1;
        assert!((base.velocity.y - expected).abs() < 1.0e-9);
        let (mut base, mut living) = turtle();
        let mut movement = turtle_control();
        movement.set_wanted_position(base.position, 1.0);
        living.speed = 0.2;
        movement.tick_turtle(
            &mut base,
            &mut living,
            false,
            BlockPos::new(0, 64, 0),
            false,
        );
        assert_eq!(living.speed, 0.0);
    }

    fn guardian() -> (EntityBase, LivingState) {
        let mut base = EntityBase::new(EntityId(3), Uuid::from_u128(3), EntityKind::Guardian);
        base.position = Vec3::new(0.5, 64.0, 0.5);
        base.old_position = base.position;
        base.tick_count = 10;
        let living = LivingState::new(&mut base, 0.0);
        (base, living)
    }

    #[test]
    fn a_guardian_swims_toward_its_goal_swaying_and_looks_ahead_with_a_blended_target() {
        let (mut base, mut living) = guardian();
        let mut movement = MoveControl::default();
        movement.set_guardian();
        let mut look = LookControl::default();
        movement.set_wanted_position(Vec3::new(0.5, 70.0, 6.5), 1.0);
        movement.tick_guardian(&mut base, &mut living, false, &mut look);
        assert!(rspigot_domain::entity::species::guardian::is_moving(&base));
        assert!(base.yaw == 0.0 || base.yaw == 360.0);
        assert_eq!(living.body_yaw, base.yaw);
        let speed = living.speed;
        assert_eq!(speed, 0.125 * (living.attribute(MOVEMENT_SPEED) as f32));
        let phase = f64::from(10 + 3);
        let sway = (phase * 0.5).sin() * 0.05;
        let bob = (phase * 0.75).sin() * 0.05;
        let direction = Vec3::new(0.0, 6.0, 6.0);
        let length = direction.length();
        let radians = f64::from(base.yaw * 0.017_453_292_f32);
        let (cos, sin) = (radians.cos(), radians.sin());
        let expected = Vec3::new(
            sway * cos,
            bob * (sin + cos) * 0.25 + f64::from(speed) * (direction.y / length) * 0.1,
            sway * sin,
        );
        assert!((base.velocity.x - expected.x).abs() < 1.0e-9);
        assert!((base.velocity.y - expected.y).abs() < 1.0e-9);
        assert!((base.velocity.z - expected.z).abs() < 1.0e-9);
        assert!(!look.is_looking_at_target() || look.wanted().z > 0.5);
        movement.tick_guardian(&mut base, &mut living, true, &mut look);
        assert_eq!(living.speed, 0.0);
        assert!(!rspigot_domain::entity::species::guardian::is_moving(&base));
    }

    fn drowned() -> (EntityBase, LivingState) {
        let mut base = EntityBase::new(EntityId(1), Uuid::from_u128(1), EntityKind::Drowned);
        base.position = Vec3::new(0.5, 64.0, 0.5);
        base.old_position = base.position;
        let living = LivingState::new(&mut base, 0.0);
        (base, living)
    }

    #[test]
    fn a_drowned_that_does_not_want_to_swim_sinks_when_airborne_and_defers_to_the_ground_control() {
        let (mut base, mut living) = drowned();
        let mut movement = MoveControl::default();
        movement.set_drowned();
        let calm = DrownedSwim::default();
        base.on_ground = false;
        assert!(!movement.tick_drowned(&mut base, &mut living, false, calm));
        assert_eq!(base.velocity, Vec3::new(0.0, -0.008, 0.0));
        base.on_ground = true;
        base.velocity = Vec3::ZERO;
        assert!(!movement.tick_drowned(&mut base, &mut living, false, calm));
        assert_eq!(base.velocity, Vec3::ZERO);
        base.fluid.in_water = false;
        let wants = DrownedSwim {
            wants_to_swim: true,
            ..calm
        };
        assert!(!movement.tick_drowned(&mut base, &mut living, false, wants));
    }

    #[test]
    fn a_swimming_drowned_floats_up_toward_a_higher_target_and_pushes_along_the_path() {
        let (mut base, mut living) = drowned();
        base.fluid.in_water = true;
        let mut movement = MoveControl::default();
        movement.set_drowned();
        movement.set_wanted_position(Vec3::new(4.5, 64.0, 0.5), 1.0);
        let swim = DrownedSwim {
            wants_to_swim: true,
            searching_for_land: false,
            target_above: true,
        };
        assert!(movement.tick_drowned(&mut base, &mut living, false, swim));
        let speed = living.speed;
        assert_eq!(speed, 0.125 * (living.attribute(MOVEMENT_SPEED) as f32));
        let expected = Vec3::new(f64::from(speed) * 4.0 * 0.005, 0.002, 0.0);
        assert!((base.velocity.x - expected.x).abs() < 1.0e-9);
        assert!((base.velocity.y - expected.y).abs() < 1.0e-9);
        assert_eq!(living.body_yaw, base.yaw);
        let mut idle = MoveControl::default();
        idle.set_drowned();
        let (mut other, mut living) = drowned();
        other.fluid.in_water = true;
        living.speed = 0.3;
        let searching = DrownedSwim {
            searching_for_land: true,
            target_above: false,
            ..swim
        };
        assert!(idle.tick_drowned(&mut other, &mut living, true, searching));
        assert_eq!(other.velocity, Vec3::new(0.0, 0.002, 0.0));
        assert_eq!(living.speed, 0.0);
    }
}
