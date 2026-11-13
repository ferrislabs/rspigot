use rspigot_blocks::Face;
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
        }
    }
}

impl LookControl {
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
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct MoveControl {
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

    pub fn set_style(&mut self, style: MoveStyle) {
        self.style = style;
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
        if can_reach(base, view, delta) {
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
        let dx = base.position.x - base.old_position.x;
        let dz = base.position.z - base.old_position.z;
        if dx * dx + dz * dz > MIN_SPEED_SQR {
            living.body_yaw = base.yaw;
            base.head_yaw =
                mth::rotate_if_necessary(base.head_yaw, living.body_yaw, MAX_HEAD_Y_ROT);
            self.last_stable_y_head_rot = base.head_yaw;
            self.head_stable_time = 0;
            return;
        }
        if (base.head_yaw - self.last_stable_y_head_rot).abs() > HEAD_STABLE_ANGLE {
            self.head_stable_time = 0;
            self.last_stable_y_head_rot = base.head_yaw;
            living.body_yaw =
                mth::rotate_if_necessary(living.body_yaw, base.head_yaw, MAX_HEAD_Y_ROT);
        } else {
            self.head_stable_time += 1;
            if self.head_stable_time > DELAY_UNTIL_FACING_FORWARD {
                let elapsed = self.head_stable_time - DELAY_UNTIL_FACING_FORWARD;
                let progress = mth::clamp(elapsed as f32 / TIME_TO_FACE_FORWARD, 0.0, 1.0);
                let max = MAX_HEAD_Y_ROT * (1.0 - progress);
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
}
