use rspigot_domain::Vec3;
use rspigot_domain::mth::{self, view_vector};

use crate::action::MobAction;
use crate::brain::AiContext;
use crate::goal::{Flag, Flags, Goal};
use crate::random::RandomSource;
use crate::world::{AiWorld, Living};

const MAX_ATTEMPTS: i32 = 64;
const FLOAT_RADIUS: f32 = 16.0;
const FLOAT_SPEED: f64 = 1.0;
const ARRIVED_SQR: f64 = 1.0;
const TOO_FAR_SQR: f64 = 3600.0;
const SHOOT_RANGE_SQR: f64 = 4096.0;
const WARN_AT: i32 = 10;
const FIRE_AT: i32 = 20;
const COOLDOWN: i32 = -40;
const FIREBALL_REACH: f64 = 4.0;
const WARN_EVENT: i32 = 1015;
const SHOOT_EVENT: i32 = 1016;
const RAD_TO_DEG_F: f32 = 57.295_776;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GhastAction {
    SetCharging(bool),
    Fireball { direction: Vec3, view: Vec3 },
}

fn target_of<W: AiWorld>(context: &AiContext<'_, W>) -> Option<Living> {
    context.brain.target.and_then(|id| context.world.living(id))
}

pub fn face_movement_direction<W: AiWorld>(context: &mut AiContext<'_, W>) {
    let yaw = match target_of(context) {
        None => {
            let movement = context.body.velocity;
            Some(-(mth::atan2(movement.x, movement.z) as f32) * RAD_TO_DEG_F)
        }
        Some(target) => {
            (context.mob.distance_to_sqr(target.position) < SHOOT_RANGE_SQR).then(|| {
                let dx = target.position.x - context.mob.position.x;
                let dz = target.position.z - context.mob.position.z;
                -(mth::atan2(dx, dz) as f32) * RAD_TO_DEG_F
            })
        }
    };
    if let Some(yaw) = yaw {
        context.body.yaw = yaw;
        context.body.body_yaw = yaw;
    }
}

fn choose_random_position<W: AiWorld>(context: &mut AiContext<'_, W>, from: Vec3) -> Vec3 {
    let mut offset = |origin: f64| {
        let roll = context.brain.random.next_float() * 2.0 - 1.0;
        origin + f64::from(roll * FLOAT_RADIUS)
    };
    let x = offset(from.x);
    let y = offset(from.y);
    let z = offset(from.z);
    Vec3::new(x, y, z)
}

fn is_good_target<W: AiWorld>(world: &W, pos: Vec3, distance: i32) -> bool {
    if distance <= 0 {
        return true;
    }
    let block = pos.block();
    if !world
        .state(block)
        .is_none_or(rspigot_blocks::StateId::is_air)
    {
        return false;
    }
    for (dx, dy, dz) in [
        (0, -1, 0),
        (0, 1, 0),
        (0, 0, -1),
        (0, 0, 1),
        (-1, 0, 0),
        (1, 0, 0),
    ] {
        for step in 1..distance {
            let at = block.offset(dx * step, dy * step, dz * step);
            if !world.state(at).is_none_or(rspigot_blocks::StateId::is_air) {
                return true;
            }
        }
    }
    false
}

pub fn suitable_fly_to_position<W: AiWorld>(
    context: &mut AiContext<'_, W>,
    distance_to_blocks: i32,
) -> Vec3 {
    let origin = context.mob.position;
    let mut chosen = None;
    for _ in 0..MAX_ATTEMPTS {
        let candidate = choose_random_position(context, origin);
        let allowed = !context.mob.has_home() || context.mob.is_within_home(candidate.block());
        chosen = allowed.then_some(candidate);
        if let Some(position) = chosen
            && is_good_target(context.world, position, distance_to_blocks)
        {
            return position;
        }
    }
    let position = chosen.unwrap_or_else(|| choose_random_position(context, origin));
    let block = position.block();
    let height = context.world.motion_blocking_height(block.x, block.z);
    if height < block.y && height > context.world.min_y() {
        return Vec3::new(
            position.x,
            origin.y - (origin.y - position.y).abs(),
            position.z,
        );
    }
    position
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RandomFloatAroundGoal {
    distance_to_blocks: i32,
}

impl RandomFloatAroundGoal {
    pub fn new() -> Self {
        Self::with_distance(0)
    }

    pub fn with_distance(distance_to_blocks: i32) -> Self {
        Self { distance_to_blocks }
    }
}

impl Default for RandomFloatAroundGoal {
    fn default() -> Self {
        Self::new()
    }
}

impl<W: AiWorld> Goal<AiContext<'_, W>> for RandomFloatAroundGoal {
    fn flags(&self) -> Flags {
        Flags::of(Flag::Move)
    }

    fn can_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        let movement = &context.brain.movement;
        if !movement.has_wanted() {
            return true;
        }
        let distance = (movement.wanted() - context.mob.position).length_sqr();
        !(ARRIVED_SQR..=TOO_FAR_SQR).contains(&distance)
    }

    fn can_continue_to_use(&mut self, _context: &mut AiContext<'_, W>) -> bool {
        false
    }

    fn start(&mut self, context: &mut AiContext<'_, W>) {
        let position = suitable_fly_to_position(context, self.distance_to_blocks);
        context
            .brain
            .movement
            .set_wanted_position(position, FLOAT_SPEED);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct GhastLookGoal;

impl<W: AiWorld> Goal<AiContext<'_, W>> for GhastLookGoal {
    fn flags(&self) -> Flags {
        Flags::of(Flag::Look)
    }

    fn can_use(&mut self, _context: &mut AiContext<'_, W>) -> bool {
        true
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, context: &mut AiContext<'_, W>) {
        face_movement_direction(context);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct GhastShootFireballGoal {
    charge_time: i32,
}

impl GhastShootFireballGoal {
    pub fn charge_time(&self) -> i32 {
        self.charge_time
    }

    pub fn set_charge_time(&mut self, charge_time: i32) {
        self.charge_time = charge_time;
    }
}

impl<W: AiWorld> Goal<AiContext<'_, W>> for GhastShootFireballGoal {
    fn flags(&self) -> Flags {
        Flags::NONE
    }

    fn can_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        target_of(context).is_some()
    }

    fn start(&mut self, _context: &mut AiContext<'_, W>) {
        self.charge_time = 0;
    }

    fn stop(&mut self, context: &mut AiContext<'_, W>) {
        context.act(MobAction::Ghast(GhastAction::SetCharging(false)));
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, context: &mut AiContext<'_, W>) {
        let Some(target) = target_of(context) else {
            return;
        };
        let world = context.world;
        let close = context.mob.distance_to_sqr(target.position) < SHOOT_RANGE_SQR;
        if close
            && context
                .brain
                .sensing
                .has_line_of_sight(world, context.mob, &target)
        {
            self.charge_time += 1;
            if self.charge_time == WARN_AT && !context.mob.silent {
                context.act(MobAction::LevelEvent {
                    event: WARN_EVENT,
                    pos: context.mob.block_pos(),
                    data: 0,
                });
            }
            if self.charge_time == FIRE_AT {
                let view = view_vector(context.mob.pitch, context.mob.yaw);
                let target_middle = target.position.y
                    + f64::from((target.bounds.max_y - target.bounds.min_y) as f32) * 0.5;
                let mob_middle = context.mob.position.y + f64::from(context.mob.height) * 0.5;
                let direction = Vec3::new(
                    target.position.x - (context.mob.position.x + view.x * FIREBALL_REACH),
                    target_middle - (0.5 + mob_middle),
                    target.position.z - (context.mob.position.z + view.z * FIREBALL_REACH),
                );
                if !context.mob.silent {
                    context.act(MobAction::LevelEvent {
                        event: SHOOT_EVENT,
                        pos: context.mob.block_pos(),
                        data: 0,
                    });
                }
                context.act(MobAction::Ghast(GhastAction::Fireball {
                    direction: direction.normalize(),
                    view,
                }));
                self.charge_time = COOLDOWN;
            }
        } else if self.charge_time > 0 {
            self.charge_time -= 1;
        }
        context.act(MobAction::Ghast(GhastAction::SetCharging(
            self.charge_time > WARN_AT,
        )));
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GhastGoal {
    RandomFloatAround(RandomFloatAroundGoal),
    Look(GhastLookGoal),
    ShootFireball(GhastShootFireballGoal),
}

impl<W: AiWorld> Goal<AiContext<'_, W>> for GhastGoal {
    fn flags(&self) -> Flags {
        match self {
            Self::RandomFloatAround(goal) => Goal::<AiContext<'_, W>>::flags(goal),
            Self::Look(goal) => Goal::<AiContext<'_, W>>::flags(goal),
            Self::ShootFireball(goal) => Goal::<AiContext<'_, W>>::flags(goal),
        }
    }

    fn can_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        match self {
            Self::RandomFloatAround(goal) => Goal::can_use(goal, context),
            Self::Look(goal) => Goal::can_use(goal, context),
            Self::ShootFireball(goal) => Goal::can_use(goal, context),
        }
    }

    fn can_continue_to_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        match self {
            Self::RandomFloatAround(goal) => Goal::can_continue_to_use(goal, context),
            Self::Look(goal) => Goal::can_continue_to_use(goal, context),
            Self::ShootFireball(goal) => Goal::can_continue_to_use(goal, context),
        }
    }

    fn start(&mut self, context: &mut AiContext<'_, W>) {
        match self {
            Self::RandomFloatAround(goal) => Goal::start(goal, context),
            Self::Look(goal) => Goal::start(goal, context),
            Self::ShootFireball(goal) => Goal::start(goal, context),
        }
    }

    fn stop(&mut self, context: &mut AiContext<'_, W>) {
        match self {
            Self::RandomFloatAround(goal) => Goal::stop(goal, context),
            Self::Look(goal) => Goal::stop(goal, context),
            Self::ShootFireball(goal) => Goal::stop(goal, context),
        }
    }

    fn requires_update_every_tick(&self) -> bool {
        match self {
            Self::RandomFloatAround(goal) => {
                Goal::<AiContext<'_, W>>::requires_update_every_tick(goal)
            }
            Self::Look(goal) => Goal::<AiContext<'_, W>>::requires_update_every_tick(goal),
            Self::ShootFireball(goal) => Goal::<AiContext<'_, W>>::requires_update_every_tick(goal),
        }
    }

    fn tick(&mut self, context: &mut AiContext<'_, W>) {
        match self {
            Self::RandomFloatAround(goal) => Goal::tick(goal, context),
            Self::Look(goal) => Goal::tick(goal, context),
            Self::ShootFireball(goal) => Goal::tick(goal, context),
        }
    }
}

#[cfg(test)]
mod tests {
    use rspigot_blocks::BlockKind;
    use rspigot_domain::{BlockPos, EntityId};
    use rspigot_entities::EntityKind;

    use super::*;
    use crate::mock::{Harness, Mock, living};

    const SEED: i64 = 5;

    fn harness_at(position: Vec3, target: Option<Vec3>) -> Harness {
        let world = Mock {
            players: target
                .map(|at| vec![living(2, EntityKind::Player, at)])
                .unwrap_or_default(),
            ..Mock::default()
        };
        let mut harness = Harness::new(EntityKind::Ghast, position, world, SEED);
        harness.brain.target = target.map(|_| EntityId(2));
        harness
    }

    fn drain(harness: &mut Harness) -> Vec<MobAction> {
        std::mem::take(&mut harness.brain.actions)
    }

    fn charging(actions: &[MobAction]) -> Vec<bool> {
        actions
            .iter()
            .filter_map(|action| match action {
                MobAction::Ghast(GhastAction::SetCharging(charging)) => Some(*charging),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn floating_wanders_only_when_it_has_nowhere_to_go_or_is_there_or_too_far() {
        let mut goal = RandomFloatAroundGoal::new();
        let mut harness = harness_at(Vec3::new(0.5, 80.0, 0.5), None);
        assert!(harness.run(|context| goal.can_use(context)));
        harness
            .brain
            .movement
            .set_wanted_position(Vec3::new(10.5, 80.0, 0.5), 1.0);
        assert!(!harness.run(|context| goal.can_use(context)));
        harness
            .brain
            .movement
            .set_wanted_position(Vec3::new(0.5, 80.5, 0.5), 1.0);
        assert!(harness.run(|context| goal.can_use(context)));
        harness
            .brain
            .movement
            .set_wanted_position(Vec3::new(60.5, 80.0, 0.5), 1.0);
        assert!(!harness.run(|context| goal.can_use(context)));
        harness
            .brain
            .movement
            .set_wanted_position(Vec3::new(61.5, 80.0, 0.5), 1.0);
        assert!(harness.run(|context| goal.can_use(context)));
        assert!(!harness.run(|context| goal.can_continue_to_use(context)));
        assert_eq!(
            Goal::<AiContext<'_, Mock>>::flags(&goal),
            Flags::of(Flag::Move)
        );
    }

    #[test]
    fn floating_picks_a_spot_within_sixteen_blocks_from_three_floats_at_full_speed() {
        let mut goal = RandomFloatAroundGoal::new();
        let mut harness = harness_at(Vec3::new(0.5, 90.0, 0.5), None);
        let mut mirror = harness.brain.random.clone();
        harness.run(|context| goal.start(context));
        let mut axis = |origin: f64| origin + f64::from((mirror.next_float() * 2.0 - 1.0) * 16.0);
        let expected = Vec3::new(axis(0.5), axis(90.0), axis(0.5));
        assert!(harness.brain.movement.has_wanted());
        assert_eq!(harness.brain.movement.wanted(), expected);
        assert_eq!(harness.brain.movement.speed_modifier(), 1.0);
        let wanted = harness.brain.movement.wanted();
        assert!((wanted.x - 0.5).abs() <= 16.0);
        assert!((wanted.y - 90.0).abs() <= 16.0);
        assert!((wanted.z - 0.5).abs() <= 16.0);
    }

    #[test]
    fn a_spot_above_the_terrain_is_mirrored_to_the_far_side_of_the_mob_height() {
        let mut harness = harness_at(Vec3::new(0.5, 100.0, 0.5), None);
        let mut mirror = harness.brain.random.clone();
        let mut axis = |origin: f64| origin + f64::from((mirror.next_float() * 2.0 - 1.0) * 16.0);
        let candidate = Vec3::new(axis(0.5), axis(100.0), axis(0.5));
        assert!(candidate.y.floor() as i32 > 64);
        assert_eq!(harness.world.motion_blocking_height(0, 0), 64);
        let spot = harness.run(|context| suitable_fly_to_position(context, 0));
        assert_eq!(spot.x, candidate.x);
        assert_eq!(spot.z, candidate.z);
        assert_eq!(spot.y, 100.0 - (100.0 - candidate.y).abs());
    }

    #[test]
    fn a_spot_below_a_high_ceiling_is_left_alone() {
        let mut harness = harness_at(Vec3::new(0.5, 70.0, 0.5), None);
        for x in -40..40 {
            for z in -40..40 {
                harness
                    .world
                    .blocks
                    .insert(BlockPos::new(x, 100, z), BlockKind::Stone.default_state());
            }
        }
        assert_eq!(harness.world.motion_blocking_height(3, 3), 101);
        let mut mirror = harness.brain.random.clone();
        let mut axis = |origin: f64| origin + f64::from((mirror.next_float() * 2.0 - 1.0) * 16.0);
        let candidate = Vec3::new(axis(0.5), axis(70.0), axis(0.5));
        let spot = harness.run(|context| suitable_fly_to_position(context, 0));
        assert_eq!(spot, candidate);
    }

    #[test]
    fn a_restricted_mob_rejects_spots_outside_its_home_and_falls_back_after_sixty_four_tries() {
        let mut harness = harness_at(Vec3::new(0.5, 90.0, 0.5), None);
        harness.mob.home_pos = BlockPos::new(500, 90, 500);
        harness.mob.home_radius = 4;
        let mut mirror = harness.brain.random.clone();
        let spot = harness.run(|context| suitable_fly_to_position(context, 0));
        let mut axis = |origin: f64| origin + f64::from((mirror.next_float() * 2.0 - 1.0) * 16.0);
        for _ in 0..64 {
            axis(0.5);
            axis(90.0);
            axis(0.5);
        }
        let fallback = Vec3::new(axis(0.5), axis(90.0), axis(0.5));
        assert_eq!(spot, fallback);
    }

    #[test]
    fn a_good_target_needs_air_and_a_block_in_reach_when_a_distance_is_asked() {
        let mut harness = harness_at(Vec3::new(0.5, 90.0, 0.5), None);
        let spot = Vec3::new(0.5, 90.5, 0.5);
        assert!(is_good_target(&harness.world, spot, 0));
        assert!(!is_good_target(&harness.world, spot, 5));
        harness
            .world
            .blocks
            .insert(BlockPos::new(0, 93, 0), BlockKind::Stone.default_state());
        assert!(is_good_target(&harness.world, spot, 5));
        assert!(!is_good_target(&harness.world, spot, 3));
        harness
            .world
            .blocks
            .insert(BlockPos::new(0, 90, 0), BlockKind::Stone.default_state());
        assert!(!is_good_target(&harness.world, spot, 5));
    }

    #[test]
    fn the_look_goal_always_runs_and_only_claims_the_look_flag() {
        let mut goal = GhastLookGoal;
        let mut harness = harness_at(Vec3::new(0.5, 80.0, 0.5), None);
        assert!(harness.run(|context| goal.can_use(context)));
        assert_eq!(
            Goal::<AiContext<'_, Mock>>::flags(&goal),
            Flags::of(Flag::Look)
        );
        assert!(Goal::<AiContext<'_, Mock>>::requires_update_every_tick(
            &goal
        ));
    }

    #[test]
    fn without_a_target_the_ghast_faces_where_it_flies() {
        let mut goal = GhastLookGoal;
        let mut harness = harness_at(Vec3::new(0.5, 80.0, 0.5), None);
        harness.body.velocity = Vec3::new(1.0, 0.0, 1.0);
        harness.run(|context| goal.tick(context));
        let expected = -(mth::atan2(1.0, 1.0) as f32) * 57.295776;
        assert_eq!(harness.body.yaw, expected);
        assert_eq!(harness.body.body_yaw, expected);
        assert!((harness.body.yaw + 45.0).abs() < 1.0e-4);
    }

    #[test]
    fn with_a_target_in_range_the_ghast_faces_it_and_beyond_sixty_four_blocks_it_does_not_turn() {
        let mut goal = GhastLookGoal;
        let mut near = harness_at(Vec3::new(0.5, 80.0, 0.5), Some(Vec3::new(10.5, 80.0, 0.5)));
        near.body.velocity = Vec3::new(0.0, 0.0, 1.0);
        near.run(|context| goal.tick(context));
        assert_eq!(near.body.yaw, -90.0);
        assert_eq!(near.body.body_yaw, -90.0);
        let mut far = harness_at(Vec3::new(0.5, 80.0, 0.5), Some(Vec3::new(70.5, 80.0, 0.5)));
        far.body.yaw = 33.0;
        far.body.body_yaw = 12.0;
        far.run(|context| goal.tick(context));
        assert_eq!((far.body.yaw, far.body.body_yaw), (33.0, 12.0));
    }

    #[test]
    fn the_shoot_goal_needs_a_target_and_discharges_when_it_stops() {
        let mut goal = GhastShootFireballGoal::default();
        let mut none = harness_at(Vec3::new(0.5, 80.0, 0.5), None);
        assert!(!none.run(|context| goal.can_use(context)));
        let mut some = harness_at(Vec3::new(0.5, 80.0, 0.5), Some(Vec3::new(10.5, 80.0, 0.5)));
        assert!(some.run(|context| goal.can_use(context)));
        goal.set_charge_time(15);
        some.run(|context| goal.start(context));
        assert_eq!(goal.charge_time(), 0);
        some.run(|context| goal.stop(context));
        assert_eq!(charging(&drain(&mut some)), vec![false]);
        assert_eq!(Goal::<AiContext<'_, Mock>>::flags(&goal), Flags::NONE);
        assert!(Goal::<AiContext<'_, Mock>>::requires_update_every_tick(
            &goal
        ));
    }

    #[test]
    fn it_warns_at_ten_ticks_fires_at_twenty_and_waits_sixty_more() {
        let mut goal = GhastShootFireballGoal::default();
        let mut harness = harness_at(Vec3::new(0.5, 80.0, 0.5), Some(Vec3::new(20.5, 80.0, 0.5)));
        harness.mob.yaw = 0.0;
        harness.run(|context| goal.start(context));
        for tick in 1..=9 {
            harness.run(|context| goal.tick(context));
            assert_eq!(goal.charge_time(), tick);
            let actions = drain(&mut harness);
            assert_eq!(charging(&actions), vec![false], "tick {tick}");
            assert!(
                !actions
                    .iter()
                    .any(|action| matches!(action, MobAction::LevelEvent { .. }))
            );
        }
        harness.run(|context| goal.tick(context));
        let warn = drain(&mut harness);
        assert!(warn.contains(&MobAction::LevelEvent {
            event: 1015,
            pos: BlockPos::new(0, 80, 0),
            data: 0,
        }));
        assert_eq!(charging(&warn), vec![false]);
        for tick in 11..=19 {
            harness.run(|context| goal.tick(context));
            assert_eq!(charging(&drain(&mut harness)), vec![true], "tick {tick}");
        }
        harness.run(|context| goal.tick(context));
        assert_eq!(goal.charge_time(), -40);
        let shot = drain(&mut harness);
        assert!(shot.contains(&MobAction::LevelEvent {
            event: 1016,
            pos: BlockPos::new(0, 80, 0),
            data: 0,
        }));
        assert_eq!(charging(&shot), vec![false]);
        let fireballs: Vec<_> = shot
            .iter()
            .filter_map(|action| match action {
                MobAction::Ghast(GhastAction::Fireball { direction, view }) => {
                    Some((*direction, *view))
                }
                _ => None,
            })
            .collect();
        assert_eq!(fireballs.len(), 1);
        let (direction, view) = fireballs[0];
        assert_eq!(view, Vec3::new(0.0, 0.0, 1.0));
        let eye = 80.0 + f64::from(EntityKind::Ghast.dimensions().height) * 0.5;
        let target_middle = 80.0 + f64::from(EntityKind::Player.dimensions().height) * 0.5;
        let raw = Vec3::new(20.0, target_middle - (0.5 + eye), -4.0);
        assert_eq!(direction, raw.normalize());
        for _ in 0..59 {
            harness.run(|context| goal.tick(context));
            assert!(
                drain(&mut harness).iter().all(|action| !matches!(
                    action,
                    MobAction::Ghast(GhastAction::Fireball { .. })
                ))
            );
        }
        assert_eq!(goal.charge_time(), 19);
        harness.run(|context| goal.tick(context));
        assert_eq!(goal.charge_time(), -40);
        assert!(
            drain(&mut harness)
                .iter()
                .any(|action| matches!(action, MobAction::Ghast(GhastAction::Fireball { .. })))
        );
    }

    #[test]
    fn a_hidden_or_far_target_lets_the_charge_drain_one_per_tick() {
        let mut goal = GhastShootFireballGoal::default();
        let mut far = harness_at(Vec3::new(0.5, 80.0, 0.5), Some(Vec3::new(70.5, 80.0, 0.5)));
        goal.set_charge_time(12);
        far.run(|context| goal.tick(context));
        assert_eq!(goal.charge_time(), 11);
        assert_eq!(charging(&drain(&mut far)), vec![true]);
        far.run(|context| goal.tick(context));
        assert_eq!(goal.charge_time(), 10);
        assert_eq!(charging(&drain(&mut far)), vec![false]);
        let mut hidden = harness_at(Vec3::new(0.5, 80.0, 0.5), Some(Vec3::new(10.5, 80.0, 0.5)));
        for y in 78..84 {
            for z in -1..3 {
                hidden
                    .world
                    .blocks
                    .insert(BlockPos::new(4, y, z), BlockKind::Stone.default_state());
            }
        }
        let mut goal = GhastShootFireballGoal::default();
        goal.set_charge_time(-5);
        hidden.run(|context| goal.tick(context));
        assert_eq!(goal.charge_time(), -5);
        goal.set_charge_time(1);
        hidden.run(|context| goal.tick(context));
        assert_eq!(goal.charge_time(), 0);
        hidden.run(|context| goal.tick(context));
        assert_eq!(goal.charge_time(), 0);
    }

    #[test]
    fn a_silent_ghast_keeps_the_warning_and_the_shot_quiet() {
        let mut goal = GhastShootFireballGoal::default();
        let mut harness = harness_at(Vec3::new(0.5, 80.0, 0.5), Some(Vec3::new(20.5, 80.0, 0.5)));
        harness.mob.silent = true;
        goal.set_charge_time(19);
        harness.run(|context| goal.tick(context));
        let actions = drain(&mut harness);
        assert!(
            !actions
                .iter()
                .any(|action| matches!(action, MobAction::LevelEvent { .. }))
        );
        assert!(
            actions
                .iter()
                .any(|action| matches!(action, MobAction::Ghast(GhastAction::Fireball { .. })))
        );
    }
}
