use rspigot_blocks::BlockKind;

use crate::bed::SLEEPING_SIZE;
use crate::entity::physics::{BlockView, block_colliders, collide};
use crate::entity::{Aabb, Pose};
use crate::{BlockPos, Vec3};

pub const WALKING_LIMIT: f64 = 100.0;
pub const ELYTRA_LIMIT: f64 = 300.0;
pub const WRONG_LIMIT: f64 = 0.0625;
pub const FLOATING_DY: f64 = -0.03125;
pub const FLOATING_TICKS: i32 = 80;
pub const PLAYER_STEP: f32 = 0.6;
pub const PLAYER_WIDTH: f32 = 0.6;
pub const TELEPORT_RESEND_TICKS: u64 = 20;
const FIT_EPSILON: f64 = 1.0e-7;
const COLLISION_EPSILON: f64 = 1.0e-5;
const FLOOR_INFLATE: f64 = 0.0625;
const FLOOR_PROBE: f64 = 0.55;
const PISTON_MARGIN: f64 = 0.5;
const FREE_PACKETS: i32 = 5;
const IDLE_ALLOWANCE: i32 = 20;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pace {
    pub distance_sq: f64,
    pub velocity_sq: f64,
    pub fall_flying: bool,
    pub packets: i32,
    pub speed: f32,
    pub multiplier: f64,
}

pub fn moved_too_quickly(pace: &Pace) -> bool {
    let base = if pace.fall_flying {
        ELYTRA_LIMIT
    } else {
        WALKING_LIMIT
    };
    let scaled = (pace.multiplier * f64::from(pace.packets as f32) * f64::from(pace.speed)).powi(2);
    pace.distance_sq - pace.velocity_sq > base.max(scaled)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PacketRate {
    received: i32,
    known: i32,
    allowed: i32,
    last_tick: u64,
}

impl PacketRate {
    pub fn on_packet(&mut self, tick: u64, moved: bool) -> i32 {
        self.received = self.received.wrapping_add(1);
        let mut packets = self.received.wrapping_sub(self.known);
        let elapsed = i32::try_from(tick.saturating_sub(self.last_tick)).unwrap_or(i32::MAX);
        self.allowed = self.allowed.saturating_add(elapsed).max(1);
        self.last_tick = tick;
        if packets > self.allowed.max(FREE_PACKETS) {
            packets = 1;
        }
        if moved {
            self.allowed -= 1;
        } else {
            self.allowed = IDLE_ALLOWANCE;
        }
        packets
    }

    pub fn end_tick(&mut self) {
        self.known = self.received;
    }

    pub fn reset(&mut self) {
        self.allowed = IDLE_ALLOWANCE;
    }
}

pub fn dimensions(pose: Pose) -> (f64, f64) {
    match pose {
        Pose::Crouching => (1.5, 1.27),
        Pose::Sleeping => (SLEEPING_SIZE, SLEEPING_SIZE),
        Pose::FallFlying | Pose::Swimming | Pose::SpinAttack => (0.6, 0.4),
        _ => (1.8, 1.62),
    }
}

pub fn player_box(position: Vec3, pose: Pose) -> Aabb {
    Aabb::of_size(position, PLAYER_WIDTH, dimensions(pose).0 as f32)
}

pub fn fits<V: BlockView>(view: &V, position: Vec3, pose: Pose) -> bool {
    block_colliders(view, &player_box(position, pose).deflate(FIT_EPSILON)).is_empty()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PoseInput {
    pub fall_flying: bool,
    pub sleeping: bool,
    pub swimming: bool,
    pub spin_attack: bool,
    pub sneaking: bool,
    pub flying: bool,
    pub spectator: bool,
    pub passenger: bool,
}

pub fn wanted_pose(input: &PoseInput) -> Pose {
    if input.fall_flying {
        Pose::FallFlying
    } else if input.sleeping {
        Pose::Sleeping
    } else if input.swimming {
        Pose::Swimming
    } else if input.spin_attack {
        Pose::SpinAttack
    } else if input.sneaking && !input.flying {
        Pose::Crouching
    } else {
        Pose::Standing
    }
}

pub fn settle_pose<V: BlockView>(view: &V, position: Vec3, input: &PoseInput) -> Pose {
    let wanted = wanted_pose(input);
    if !fits(view, position, Pose::Swimming) {
        return wanted;
    }
    if input.spectator || input.passenger || fits(view, position, wanted) {
        wanted
    } else if fits(view, position, Pose::Crouching) {
        Pose::Crouching
    } else {
        Pose::Swimming
    }
}

pub fn swimming_after(
    swimming: bool,
    flying: bool,
    sprinting: bool,
    passenger: bool,
    in_water: bool,
    under_water: bool,
    feet_in_water: bool,
) -> bool {
    if flying || passenger {
        false
    } else if swimming {
        sprinting && in_water
    } else {
        sprinting && under_water && feet_in_water
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Checks {
    pub moved_wrongly: bool,
    pub collisions: bool,
}

pub fn rejects_move<V: BlockView>(
    view: &V,
    from: Vec3,
    to: Vec3,
    pose: Pose,
    on_ground: bool,
    checks: Checks,
) -> bool {
    rejects_boxes(
        view,
        &player_box(from, pose),
        &player_box(to, pose),
        PLAYER_STEP,
        on_ground,
        checks,
    )
}

pub fn rejects_boxes<V: BlockView>(
    view: &V,
    old: &Aabb,
    new: &Aabb,
    step: f32,
    on_ground: bool,
    checks: Checks,
) -> bool {
    if !checks.moved_wrongly && !checks.collisions {
        return false;
    }
    let from = Vec3::new(old.min_x, old.min_y, old.min_z);
    let to = Vec3::new(new.min_x, new.min_y, new.min_z);
    let delta = to - from;
    let near = block_colliders(view, &old.expand_towards(delta).inflate(PISTON_MARGIN));
    if near
        .iter()
        .any(|c| view.block(c.pos).and_then(|s| s.kind()) == Some(BlockKind::MovingPiston))
    {
        return false;
    }
    let old_colliders = block_colliders(view, &old.deflate(COLLISION_EPSILON));
    let new_colliders = block_colliders(view, &new.deflate(COLLISION_EPSILON));
    let wrongly = checks.moved_wrongly && {
        let end = from + collide(view, old, delta, step, on_ground);
        let (dx, dz) = (to.x - end.x, to.z - end.z);
        dx * dx + dz * dz > WRONG_LIMIT
    };
    let stuck = !old_colliders.is_empty();
    let entered = checks.collisions
        && new_colliders
            .iter()
            .any(|n| !old_colliders.iter().any(|o| o.pos == n.pos));
    (wrongly && !stuck) || entered
}

pub fn nothing_below<V: BlockView>(view: &V, position: Vec3) -> bool {
    let area = player_box(position, Pose::Standing)
        .inflate(FLOOR_INFLATE)
        .expand_towards(Vec3::new(0.0, -FLOOR_PROBE, 0.0));
    let floor = |value: f64| value.floor() as i32;
    for y in floor(area.min_y)..=floor(area.max_y) {
        for z in floor(area.min_z)..=floor(area.max_z) {
            for x in floor(area.min_x)..=floor(area.max_x) {
                if view
                    .block(BlockPos::new(x, y, z))
                    .is_none_or(|state| !state.is_air())
                {
                    return false;
                }
            }
        }
    }
    true
}

pub fn is_floating<V: BlockView>(
    view: &V,
    position: Vec3,
    dy: f64,
    on_ground: bool,
    exempt: bool,
) -> bool {
    !exempt && !on_ground && dy >= FLOATING_DY && nothing_below(view, position)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gait {
    Swim,
    WalkUnderwater,
    WalkOnWater,
    Sprint,
    Crouch,
    Walk,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HungerRates {
    pub jump_walk: f32,
    pub jump_sprint: f32,
    pub swim_multiplier: f32,
    pub sprint_multiplier: f32,
    pub other_multiplier: f32,
    pub regen: f32,
}

impl Default for HungerRates {
    fn default() -> Self {
        Self {
            jump_walk: 0.05,
            jump_sprint: 0.2,
            swim_multiplier: 0.01,
            sprint_multiplier: 0.1,
            other_multiplier: 0.0,
            regen: 6.0,
        }
    }
}

impl HungerRates {
    pub fn jump(&self, sprinting: bool) -> f32 {
        if sprinting {
            self.jump_sprint
        } else {
            self.jump_walk
        }
    }

    pub fn travelled(&self, gait: Gait, centimeters: i32) -> f32 {
        let multiplier = match gait {
            Gait::Swim | Gait::WalkUnderwater | Gait::WalkOnWater => self.swim_multiplier,
            Gait::Sprint => self.sprint_multiplier,
            Gait::Crouch | Gait::Walk => self.other_multiplier,
        };
        multiplier * centimeters as f32 * 0.01
    }
}

pub fn centimeters(distance: f64) -> i32 {
    (distance * 100.0).round() as i32
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::BlockStateId;

    struct World(HashMap<BlockPos, BlockStateId>);

    impl BlockView for World {
        fn block(&self, pos: BlockPos) -> Option<BlockStateId> {
            Some(self.0.get(&pos).copied().unwrap_or(BlockStateId::AIR))
        }
    }

    fn floor() -> World {
        let mut blocks = HashMap::new();
        for x in -8..=8 {
            for z in -8..=8 {
                blocks.insert(BlockPos::new(x, 63, z), BlockKind::Stone.default_state());
            }
        }
        World(blocks)
    }

    fn with(mut world: World, pos: BlockPos, kind: BlockKind) -> World {
        world.0.insert(pos, kind.default_state());
        world
    }

    fn pace(distance: f64) -> Pace {
        Pace {
            distance_sq: distance * distance,
            velocity_sq: 0.0,
            fall_flying: false,
            packets: 1,
            speed: 1.0,
            multiplier: 10.0,
        }
    }

    #[test]
    fn walking_threshold_is_ten_blocks() {
        assert!(!moved_too_quickly(&pace(9.9)));
        assert!(moved_too_quickly(&pace(10.1)));
    }

    #[test]
    fn elytra_threshold_is_larger() {
        let mut gliding = pace(15.0);
        gliding.fall_flying = true;
        assert!(!moved_too_quickly(&gliding));
        gliding.distance_sq = 18.0 * 18.0;
        assert!(moved_too_quickly(&gliding));
    }

    #[test]
    fn velocity_is_subtracted_and_packets_scale_the_limit() {
        let mut fast = pace(12.0);
        fast.velocity_sq = 60.0;
        assert!(!moved_too_quickly(&fast));
        let mut burst = pace(15.0);
        burst.packets = 2;
        assert!(!moved_too_quickly(&burst));
    }

    #[test]
    fn packet_rate_counts_since_last_tick_and_caps_spam() {
        let mut rate = PacketRate::default();
        assert_eq!(rate.on_packet(1, true), 1);
        assert_eq!(rate.on_packet(1, true), 2);
        rate.end_tick();
        assert_eq!(rate.on_packet(2, true), 1);
        let mut spam = PacketRate::default();
        let counted: Vec<i32> = (0..8).map(|_| spam.on_packet(0, true)).collect();
        assert_eq!(counted.last(), Some(&1));
    }

    #[test]
    fn walking_over_the_floor_is_accepted() {
        let world = floor();
        let checks = Checks {
            moved_wrongly: true,
            collisions: true,
        };
        assert!(!rejects_move(
            &world,
            Vec3::new(0.5, 64.0, 0.5),
            Vec3::new(0.8, 64.0, 0.5),
            Pose::Standing,
            true,
            checks
        ));
    }

    #[test]
    fn walking_through_a_wall_is_rejected() {
        let mut world = floor();
        for y in 64..=65 {
            world = with(world, BlockPos::new(1, y, 0), BlockKind::Stone);
        }
        let checks = Checks {
            moved_wrongly: true,
            collisions: true,
        };
        assert!(rejects_move(
            &world,
            Vec3::new(0.5, 64.0, 0.5),
            Vec3::new(2.5, 64.0, 0.5),
            Pose::Standing,
            true,
            checks
        ));
        let spectator = Checks {
            moved_wrongly: false,
            collisions: false,
        };
        assert!(!rejects_move(
            &world,
            Vec3::new(0.5, 64.0, 0.5),
            Vec3::new(2.5, 64.0, 0.5),
            Pose::Standing,
            true,
            spectator
        ));
    }

    #[test]
    fn stepping_up_one_slab_is_accepted() {
        let world = with(floor(), BlockPos::new(1, 64, 0), BlockKind::StoneSlab);
        let checks = Checks {
            moved_wrongly: true,
            collisions: true,
        };
        assert!(!rejects_move(
            &world,
            Vec3::new(0.5, 64.0, 0.5),
            Vec3::new(1.5, 64.5, 0.5),
            Pose::Standing,
            true,
            checks
        ));
    }

    #[test]
    fn a_hovering_player_is_floating_but_not_above_a_block() {
        let world = floor();
        assert!(is_floating(
            &world,
            Vec3::new(0.5, 70.0, 0.5),
            0.0,
            false,
            false
        ));
        assert!(!is_floating(
            &world,
            Vec3::new(0.5, 64.2, 0.5),
            0.0,
            false,
            false
        ));
        assert!(!is_floating(
            &world,
            Vec3::new(0.5, 70.0, 0.5),
            -0.5,
            false,
            false
        ));
        assert!(!is_floating(
            &world,
            Vec3::new(0.5, 70.0, 0.5),
            0.0,
            false,
            true
        ));
    }

    #[test]
    fn open_air_keeps_the_wanted_pose() {
        let world = floor();
        assert_eq!(
            settle_pose(&world, Vec3::new(0.5, 64.0, 0.5), &PoseInput::default()),
            Pose::Standing
        );
    }

    #[test]
    fn a_gap_of_one_and_a_half_blocks_forces_crouching() {
        let world = with(floor(), BlockPos::new(0, 66, 0), BlockKind::Stone);
        assert_eq!(
            settle_pose(&world, Vec3::new(0.5, 64.3, 0.5), &PoseInput::default()),
            Pose::Crouching
        );
    }

    #[test]
    fn a_gap_of_one_block_forces_crawling() {
        let world = with(floor(), BlockPos::new(0, 65, 0), BlockKind::Stone);
        assert_eq!(
            settle_pose(&world, Vec3::new(0.5, 64.0, 0.5), &PoseInput::default()),
            Pose::Swimming
        );
    }

    #[test]
    fn a_spectator_ignores_the_ceiling() {
        let world = with(floor(), BlockPos::new(0, 65, 0), BlockKind::Stone);
        let input = PoseInput {
            spectator: true,
            ..PoseInput::default()
        };
        assert_eq!(
            settle_pose(&world, Vec3::new(0.5, 64.0, 0.5), &input),
            Pose::Standing
        );
    }

    #[test]
    fn swimming_follows_sprint_and_water() {
        assert!(swimming_after(false, false, true, false, true, true, true));
        assert!(!swimming_after(
            false, false, true, false, true, false, true
        ));
        assert!(swimming_after(true, false, true, false, true, false, false));
        assert!(!swimming_after(true, true, true, false, true, true, true));
        assert!(!swimming_after(true, false, true, true, true, true, true));
    }

    #[test]
    fn pose_priority_matches_vanilla() {
        let mut input = PoseInput {
            sneaking: true,
            swimming: true,
            ..PoseInput::default()
        };
        assert_eq!(wanted_pose(&input), Pose::Swimming);
        input.fall_flying = true;
        assert_eq!(wanted_pose(&input), Pose::FallFlying);
        let sneaking = PoseInput {
            sneaking: true,
            flying: true,
            ..PoseInput::default()
        };
        assert_eq!(wanted_pose(&sneaking), Pose::Standing);
    }

    #[test]
    fn pose_dimensions() {
        assert_eq!(dimensions(Pose::Swimming), (0.6, 0.4));
        assert_eq!(dimensions(Pose::Crouching), (1.5, 1.27));
        assert_eq!(dimensions(Pose::Standing), (1.8, 1.62));
    }

    #[test]
    fn exhaustion_rates_follow_spigot_defaults() {
        let rates = HungerRates::default();
        assert!((rates.jump(true) - 0.2).abs() < 1e-6);
        assert!((rates.jump(false) - 0.05).abs() < 1e-6);
        assert!((rates.travelled(Gait::Sprint, 100) - 0.1).abs() < 1e-6);
        assert!((rates.travelled(Gait::Swim, 100) - 0.01).abs() < 1e-6);
        assert_eq!(rates.travelled(Gait::Walk, 100), 0.0);
        assert_eq!(centimeters(0.126), 13);
    }
}
