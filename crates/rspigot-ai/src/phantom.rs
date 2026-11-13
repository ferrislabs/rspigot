use rspigot_domain::entity::EntityBase;
use rspigot_domain::entity::living::LivingState;
use rspigot_domain::{BlockPos, EntityId, Vec3, mth};

use crate::action::{MobAction, TargetReason};
use crate::brain::AiContext;
use crate::goal::{Flag, Flags, Goal, adjusted_tick_delay, reduced_tick_delay};
use crate::random::RandomSource;
use crate::targeting::TargetingConditions;
use crate::world::{AiWorld, Living, TargetKind, inflate};

pub const SWOOP_SOUND: &str = "minecraft:entity.phantom.swoop";
pub const SWOOP_EVENT: i32 = 1039;
const FIRST_SWEEP_DELAY: i32 = 10;
const SWEEP_BASE_SECONDS: i32 = 8;
const SWEEP_RANDOM_SECONDS: i32 = 4;
const TICKS_PER_SECOND: i32 = 20;
const ANCHOR_ABOVE_TARGET: i32 = 20;
const ANCHOR_ABOVE_RANDOM: i32 = 20;
const ANCHOR_REST_ABOVE: i32 = 10;
const ANCHOR_REST_RANDOM: i32 = 20;
const SWOOP_VOLUME: f32 = 10.0;
const SWOOP_PITCH_BASE: f32 = 0.95;
const SWOOP_PITCH_SPREAD: f32 = 0.1;
const CAT_RADIUS: f64 = 16.0;
const CAT_SEARCH_INTERVAL: i32 = 20;
const TOUCH_DISTANCE_SQR: f64 = 4.0;
const SCAN_RANGE: f64 = 64.0;
const SCAN_HORIZONTAL: f64 = 16.0;
const SCAN_VERTICAL: f64 = 64.0;
const FIRST_SCAN_DELAY: i32 = 20;
const SCAN_DELAY: i32 = 60;
const CIRCLE_MIN_DISTANCE: f32 = 5.0;
const CIRCLE_DISTANCE_SPREAD: f32 = 10.0;
const CIRCLE_MAX_DISTANCE: f32 = 15.0;
const HEIGHT_BASE: f32 = -4.0;
const HEIGHT_SPREAD: f32 = 9.0;
const HEIGHT_ODDS: i32 = 350;
const DISTANCE_ODDS: i32 = 250;
const ANGLE_ODDS: i32 = 450;
const ANGLE_STEP: f32 = 15.0;
const DEG_TO_RAD_F: f32 = 0.017_453_292;
const PI_F: f32 = std::f32::consts::PI;
pub const INITIAL_SPEED: f32 = 0.1;
const MOVE_MIN_SPEED: f32 = 0.2;
const MOVE_MAX_SPEED: f32 = 1.8;
const MOVE_SLOWDOWN: f32 = 0.025;
const MOVE_SPEEDUP: f32 = 0.005;
const MOVE_TURN: f32 = 4.0;
const MOVE_STRAIGHT: f32 = 3.0;
const MOVE_BLEND: f64 = 0.2;
const MOVE_VERTICAL_SHRINK: f64 = 0.7;
const MOVE_EPSILON: f64 = 9.999_999_747_378_752e-6;
const RAD_TO_DEG_F: f32 = 57.295_776;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AttackPhase {
    #[default]
    Circle,
    Swoop,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PhantomBrain {
    pub phase: AttackPhase,
    pub move_target: Vec3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PhantomFacts {
    pub anchor: Option<BlockPos>,
    pub hurt_recently: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhantomAction {
    Hiss(EntityId),
}

pub fn tick_move_control(
    base: &mut EntityBase,
    living: &mut LivingState,
    speed: &mut f32,
    move_target: Vec3,
) {
    if base.horizontal_collision {
        base.set_yaw(base.yaw + 180.0);
        *speed = INITIAL_SPEED;
    }
    let mut dx = move_target.x - base.position.x;
    let dy = move_target.y - base.position.y;
    let mut dz = move_target.z - base.position.z;
    let mut horizontal = (dx * dx + dz * dz).sqrt();
    if horizontal.abs() <= MOVE_EPSILON {
        return;
    }
    let shrink = 1.0 - (dy * MOVE_VERTICAL_SHRINK).abs() / horizontal;
    dx *= shrink;
    dz *= shrink;
    horizontal = (dx * dx + dz * dz).sqrt();
    let distance = (dx * dx + dz * dz + dy * dy).sqrt();
    let old_yaw = base.yaw;
    let target_yaw = mth::wrap_degrees(mth::atan2(dz, dx) as f32 * RAD_TO_DEG_F);
    let current = mth::wrap_degrees(base.yaw + 90.0);
    base.set_yaw(mth::approach_degrees(current, target_yaw, MOVE_TURN) - 90.0);
    living.body_yaw = base.yaw;
    if mth::degrees_difference_abs(old_yaw, base.yaw) < MOVE_STRAIGHT {
        *speed = mth::approach(
            *speed,
            MOVE_MAX_SPEED,
            (MOVE_MAX_SPEED / *speed) * MOVE_SPEEDUP,
        );
    } else {
        *speed = mth::approach(*speed, MOVE_MIN_SPEED, MOVE_SLOWDOWN);
    }
    let pitch = -(mth::atan2(-dy, horizontal) as f32 * RAD_TO_DEG_F);
    base.set_pitch(pitch);
    let heading = (base.yaw + 90.0) * DEG_TO_RAD_F;
    let x = f64::from(*speed * mth::cos(f64::from(heading))) * (dx / distance).abs();
    let z = f64::from(*speed * mth::sin(f64::from(heading))) * (dz / distance).abs();
    let y = f64::from(*speed * mth::sin(f64::from(pitch * DEG_TO_RAD_F))) * (dy / distance).abs();
    let velocity = base.velocity;
    base.velocity = velocity + (Vec3::new(x, y, z) - velocity).scale(MOVE_BLEND);
}

fn target_of<W: AiWorld>(context: &AiContext<'_, W>) -> Option<Living> {
    context.brain.target.and_then(|id| context.world.living(id))
}

fn can_attack<W: AiWorld>(context: &mut AiContext<'_, W>, target: &Living) -> bool {
    TargetingConditions::for_combat().test(context, target, |_| true)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AttackStrategyGoal {
    next_sweep_tick: i32,
}

impl AttackStrategyGoal {
    pub fn next_sweep_tick(&self) -> i32 {
        self.next_sweep_tick
    }

    fn set_anchor_above_target<W: AiWorld>(context: &mut AiContext<'_, W>) {
        if context.body.phantom.anchor.is_none() {
            return;
        }
        let Some(target) = target_of(context) else {
            return;
        };
        let above = ANCHOR_ABOVE_TARGET + context.brain.random.next_int(ANCHOR_ABOVE_RANDOM);
        let pos = target.position.block().offset(0, above, 0);
        let sea = context.world.sea_level();
        context.body.phantom.anchor = Some(if pos.y < sea {
            BlockPos::new(pos.x, sea + 1, pos.z)
        } else {
            pos
        });
    }
}

impl<W: AiWorld> Goal<AiContext<'_, W>> for AttackStrategyGoal {
    fn flags(&self) -> Flags {
        Flags::NONE
    }

    fn can_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        target_of(context).is_some_and(|target| can_attack(context, &target))
    }

    fn start(&mut self, context: &mut AiContext<'_, W>) {
        self.next_sweep_tick = adjusted_tick_delay(false, FIRST_SWEEP_DELAY);
        context.brain.phantom.phase = AttackPhase::Circle;
        Self::set_anchor_above_target(context);
    }

    fn stop(&mut self, context: &mut AiContext<'_, W>) {
        if let Some(anchor) = context.body.phantom.anchor {
            let height = context.world.motion_blocking_height(anchor.x, anchor.z);
            let above = ANCHOR_REST_ABOVE + context.brain.random.next_int(ANCHOR_REST_RANDOM);
            context.body.phantom.anchor = Some(BlockPos::new(anchor.x, height + above, anchor.z));
        }
    }

    fn tick(&mut self, context: &mut AiContext<'_, W>) {
        if context.brain.phantom.phase != AttackPhase::Circle {
            return;
        }
        self.next_sweep_tick -= 1;
        if self.next_sweep_tick >= 1 {
            return;
        }
        context.brain.phantom.phase = AttackPhase::Swoop;
        Self::set_anchor_above_target(context);
        let seconds = SWEEP_BASE_SECONDS + context.brain.random.next_int(SWEEP_RANDOM_SECONDS);
        self.next_sweep_tick = adjusted_tick_delay(false, seconds * TICKS_PER_SECOND);
        let pitch = context.brain.random.next_float() * SWOOP_PITCH_SPREAD + SWOOP_PITCH_BASE;
        context.act(MobAction::Sound {
            sound: SWOOP_SOUND,
            volume: SWOOP_VOLUME,
            pitch,
        });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SweepAttackGoal {
    cat_search_tick: i32,
    scared_of_cat: bool,
}

impl SweepAttackGoal {
    pub fn is_scared_of_cat(&self) -> bool {
        self.scared_of_cat
    }
}

impl<W: AiWorld> Goal<AiContext<'_, W>> for SweepAttackGoal {
    fn flags(&self) -> Flags {
        Flags::of(Flag::Move)
    }

    fn can_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        context.brain.target.is_some() && context.brain.phantom.phase == AttackPhase::Swoop
    }

    fn can_continue_to_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        let Some(target) = target_of(context) else {
            return false;
        };
        if !target.alive || (target.player && (target.spectator || target.creative)) {
            return false;
        }
        if !self.can_use(context) {
            return false;
        }
        if self.cat_search_tick < context.mob.tick_count {
            self.cat_search_tick = context.mob.tick_count + CAT_SEARCH_INTERVAL;
            let area = inflate(&context.mob.bounds(), CAT_RADIUS, CAT_RADIUS, CAT_RADIUS);
            let cats: Vec<Living> = context
                .world
                .living_of(TargetKind::Class("Cat"), &area)
                .into_iter()
                .filter(|cat| cat.alive)
                .collect();
            for cat in &cats {
                context.act(MobAction::Air(crate::AirAction::Phantom(
                    PhantomAction::Hiss(cat.id),
                )));
            }
            self.scared_of_cat = !cats.is_empty();
        }
        !self.scared_of_cat
    }

    fn stop(&mut self, context: &mut AiContext<'_, W>) {
        context.set_target(None, TargetReason::ForgotTarget);
        context.brain.phantom.phase = AttackPhase::Circle;
    }

    fn tick(&mut self, context: &mut AiContext<'_, W>) {
        let Some(target) = target_of(context) else {
            return;
        };
        let height = target.bounds.max_y - target.bounds.min_y;
        context.brain.phantom.move_target = Vec3::new(
            target.position.x,
            target.position.y + height * 0.5,
            target.position.z,
        );
        if context.mob.bounds().intersects(&target.bounds) {
            context.act(MobAction::Attack(target.id));
            context.brain.phantom.phase = AttackPhase::Circle;
            if !context.mob.silent {
                context.act(MobAction::LevelEvent {
                    event: SWOOP_EVENT,
                    pos: context.mob.block_pos(),
                    data: 0,
                });
            }
        } else if context.mob.horizontal_collision || context.mob.species.phantom.hurt_recently {
            context.brain.phantom.phase = AttackPhase::Circle;
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CircleAroundAnchorGoal {
    angle: f32,
    distance: f32,
    height: f32,
    clockwise: f32,
}

impl CircleAroundAnchorGoal {
    pub const fn new() -> Self {
        Self {
            angle: 0.0,
            distance: 0.0,
            height: 0.0,
            clockwise: 0.0,
        }
    }

    pub fn distance(&self) -> f32 {
        self.distance
    }

    pub fn height(&self) -> f32 {
        self.height
    }

    pub fn clockwise(&self) -> f32 {
        self.clockwise
    }

    pub fn angle(&self) -> f32 {
        self.angle
    }

    fn select_next<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        let anchor = *context
            .body
            .phantom
            .anchor
            .get_or_insert_with(|| context.mob.block_pos());
        self.angle += self.clockwise * ANGLE_STEP * DEG_TO_RAD_F;
        let cos = f64::from(self.distance * mth::cos(f64::from(self.angle)));
        let sin = f64::from(self.distance * mth::sin(f64::from(self.angle)));
        context.brain.phantom.move_target = Vec3::new(
            f64::from(anchor.x) + cos,
            f64::from(anchor.y) + f64::from(HEIGHT_BASE + self.height),
            f64::from(anchor.z) + sin,
        );
    }

    fn touching_target<W: AiWorld>(context: &AiContext<'_, W>) -> bool {
        context
            .brain
            .phantom
            .move_target
            .distance_squared(context.mob.position)
            < TOUCH_DISTANCE_SQR
    }
}

impl Default for CircleAroundAnchorGoal {
    fn default() -> Self {
        Self::new()
    }
}

impl<W: AiWorld> Goal<AiContext<'_, W>> for CircleAroundAnchorGoal {
    fn flags(&self) -> Flags {
        Flags::of(Flag::Move)
    }

    fn can_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        context.brain.target.is_none() || context.brain.phantom.phase == AttackPhase::Circle
    }

    fn start(&mut self, context: &mut AiContext<'_, W>) {
        let random = &mut context.brain.random;
        self.distance = CIRCLE_MIN_DISTANCE + random.next_float() * CIRCLE_DISTANCE_SPREAD;
        self.height = HEIGHT_BASE + random.next_float() * HEIGHT_SPREAD;
        self.clockwise = if random.next_boolean() { 1.0 } else { -1.0 };
        self.select_next(context);
    }

    fn tick(&mut self, context: &mut AiContext<'_, W>) {
        let random = &mut context.brain.random;
        if random.next_int(adjusted_tick_delay(false, HEIGHT_ODDS)) == 0 {
            self.height = HEIGHT_BASE + random.next_float() * HEIGHT_SPREAD;
        }
        if random.next_int(adjusted_tick_delay(false, DISTANCE_ODDS)) == 0 {
            self.distance += 1.0;
            if self.distance > CIRCLE_MAX_DISTANCE {
                self.distance = CIRCLE_MIN_DISTANCE;
                self.clockwise = -self.clockwise;
            }
        }
        if random.next_int(adjusted_tick_delay(false, ANGLE_ODDS)) == 0 {
            self.angle = random.next_float() * 2.0 * PI_F;
            self.select_next(context);
        }
        if Self::touching_target(context) {
            self.select_next(context);
        }
        let position = context.mob.position;
        let pos = context.mob.block_pos();
        let empty = |context: &AiContext<'_, W>, at: BlockPos| {
            context
                .world
                .state(at)
                .is_none_or(rspigot_blocks::StateId::is_air)
        };
        if context.brain.phantom.move_target.y < position.y && !empty(context, pos.offset(0, -1, 0))
        {
            self.height = self.height.max(1.0);
            self.select_next(context);
        }
        if context.brain.phantom.move_target.y > position.y && !empty(context, pos.offset(0, 1, 0))
        {
            self.height = self.height.min(-1.0);
            self.select_next(context);
        }
    }

    fn can_continue_to_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        self.can_use(context)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AttackPlayerTargetGoal {
    next_scan_tick: i32,
}

impl AttackPlayerTargetGoal {
    pub const fn new() -> Self {
        Self {
            next_scan_tick: reduced_tick_delay(FIRST_SCAN_DELAY),
        }
    }

    pub fn next_scan_tick(&self) -> i32 {
        self.next_scan_tick
    }
}

impl Default for AttackPlayerTargetGoal {
    fn default() -> Self {
        Self::new()
    }
}

impl<W: AiWorld> Goal<AiContext<'_, W>> for AttackPlayerTargetGoal {
    fn flags(&self) -> Flags {
        Flags::NONE
    }

    fn can_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        if self.next_scan_tick > 0 {
            self.next_scan_tick -= 1;
            return false;
        }
        self.next_scan_tick = reduced_tick_delay(SCAN_DELAY);
        let area = inflate(
            &context.mob.bounds(),
            SCAN_HORIZONTAL,
            SCAN_VERTICAL,
            SCAN_HORIZONTAL,
        );
        let conditions = TargetingConditions::for_combat().range(SCAN_RANGE);
        let mut players: Vec<Living> = context
            .world
            .players()
            .iter()
            .filter(|player| player.bounds.intersects(&area))
            .copied()
            .collect();
        players.retain(|player| conditions.test(context, player, |_| true));
        players.sort_by(|left, right| right.position.y.total_cmp(&left.position.y));
        for player in players {
            if can_attack(context, &player) {
                context.set_target(Some(player), TargetReason::ClosestPlayer);
                return true;
            }
        }
        false
    }

    fn can_continue_to_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        target_of(context).is_some_and(|target| can_attack(context, &target))
    }
}

#[cfg(test)]
mod tests {
    use rspigot_blocks::BlockKind;
    use rspigot_entities::EntityKind;
    use uuid::Uuid;

    use super::*;
    use crate::mock::{Harness, Mock, living};
    use crate::random::MobRandom;

    fn harness(players: &[Vec3], seed: i64) -> Harness {
        let world = Mock {
            players: players
                .iter()
                .enumerate()
                .map(|(index, at)| living(2 + index as i32, EntityKind::Player, *at))
                .collect(),
            ..Mock::default()
        };
        let mut harness = Harness::new(EntityKind::Phantom, Vec3::new(0.5, 90.0, 0.5), world, seed);
        harness.body.phantom.anchor = Some(BlockPos::new(0, 95, 0));
        harness
    }

    fn targeted(at: Vec3, seed: i64) -> Harness {
        let mut harness = harness(&[at], seed);
        harness.brain.target = Some(EntityId(2));
        harness
    }

    fn phantom_base() -> (EntityBase, LivingState) {
        let mut base = EntityBase::new(EntityId(1), Uuid::from_u128(1), EntityKind::Phantom);
        base.position = Vec3::new(0.5, 90.0, 0.5);
        let living = LivingState::new(&mut base, 0.0);
        (base, living)
    }

    #[test]
    fn the_control_turns_four_degrees_a_tick_and_blends_the_velocity_a_fifth_of_the_way() {
        let (mut base, mut living) = phantom_base();
        let mut speed = INITIAL_SPEED;
        tick_move_control(
            &mut base,
            &mut living,
            &mut speed,
            Vec3::new(20.5, 90.0, 0.5),
        );
        assert_eq!(base.yaw, -4.0);
        assert_eq!(living.body_yaw, base.yaw);
        assert!(speed > INITIAL_SPEED && speed < 0.2);
        assert!(base.velocity.x > 0.0 && base.velocity.y == 0.0);
        assert_eq!(base.pitch, 0.0);
    }

    #[test]
    fn the_control_climbs_towards_a_higher_target_and_pitches_up() {
        let (mut base, mut living) = phantom_base();
        let mut speed = 1.0;
        tick_move_control(
            &mut base,
            &mut living,
            &mut speed,
            Vec3::new(0.5, 110.0, 20.5),
        );
        assert!(base.pitch > 0.0);
        assert!(base.velocity.y > 0.0);
    }

    #[test]
    fn a_big_turn_slows_it_towards_the_minimum_and_a_straight_flight_speeds_it_up() {
        let (mut base, mut living) = phantom_base();
        base.yaw = 90.0;
        let mut speed = 1.0;
        tick_move_control(
            &mut base,
            &mut living,
            &mut speed,
            Vec3::new(20.5, 90.0, 0.5),
        );
        assert_eq!(speed, 1.0 - 0.025);
        let (mut aligned, mut aligned_living) = phantom_base();
        aligned.yaw = -90.0;
        let mut quick = 1.0;
        tick_move_control(
            &mut aligned,
            &mut aligned_living,
            &mut quick,
            Vec3::new(20.5, 90.0, 0.5),
        );
        assert_eq!(quick, 1.0 + (1.8 / 1.0) * 0.005);
    }

    #[test]
    fn a_wall_turns_it_around_and_resets_its_speed() {
        let (mut base, mut living) = phantom_base();
        base.horizontal_collision = true;
        base.yaw = 10.0;
        let mut speed = 1.5;
        let here = base.position;
        tick_move_control(&mut base, &mut living, &mut speed, here);
        assert_eq!(base.yaw, 190.0);
        assert_eq!(speed, INITIAL_SPEED);
    }

    #[test]
    fn the_strategy_needs_a_target_it_may_attack() {
        let mut goal = AttackStrategyGoal::default();
        assert!(!harness(&[], 1).run(|context| goal.can_use(context)));
        assert!(targeted(Vec3::new(5.5, 90.0, 0.5), 1).run(|context| goal.can_use(context)));
        let mut creative = targeted(Vec3::new(5.5, 90.0, 0.5), 1);
        creative.world.players[0].invulnerable = true;
        assert!(!creative.run(|context| goal.can_use(context)));
    }

    #[test]
    fn starting_the_strategy_circles_and_anchors_twenty_to_forty_blocks_above_the_target() {
        let mut goal = AttackStrategyGoal::default();
        let mut harness = targeted(Vec3::new(5.5, 70.0, 7.5), 4);
        let mut mirror = MobRandom::new(4);
        let above = 20 + mirror.next_int(20);
        harness.brain.phantom.phase = AttackPhase::Swoop;
        harness.run(|context| goal.start(context));
        assert_eq!(goal.next_sweep_tick(), reduced_tick_delay(10));
        assert_eq!(harness.brain.phantom.phase, AttackPhase::Circle);
        assert_eq!(
            harness.body.phantom.anchor,
            Some(BlockPos::new(5, 70 + above, 7))
        );
    }

    #[test]
    fn an_anchor_below_the_sea_is_lifted_to_one_block_above_it() {
        let mut goal = AttackStrategyGoal::default();
        let mut harness = targeted(Vec3::new(5.5, 10.0, 7.5), 4);
        harness.run(|context| goal.start(context));
        assert_eq!(harness.body.phantom.anchor, Some(BlockPos::new(5, 64, 7)));
        let mut without = targeted(Vec3::new(5.5, 70.0, 7.5), 4);
        without.body.phantom.anchor = None;
        without.run(|context| goal.start(context));
        assert_eq!(without.body.phantom.anchor, None);
    }

    #[test]
    fn the_strategy_counts_down_while_circling_then_swoops_with_a_sound_and_a_new_delay() {
        let mut goal = AttackStrategyGoal::default();
        let mut harness = targeted(Vec3::new(5.5, 70.0, 7.5), 4);
        harness.run(|context| goal.start(context));
        for _ in 0..4 {
            harness.run(|context| goal.tick(context));
        }
        assert_eq!(harness.brain.phantom.phase, AttackPhase::Circle);
        assert_eq!(goal.next_sweep_tick(), 1);
        let mut mirror = harness.brain.random.clone();
        let _ = mirror.next_int(20);
        let seconds = 8 + mirror.next_int(4);
        let pitch = mirror.next_float() * 0.1 + 0.95;
        harness.run(|context| goal.tick(context));
        assert_eq!(harness.brain.phantom.phase, AttackPhase::Swoop);
        assert_eq!(goal.next_sweep_tick(), reduced_tick_delay(seconds * 20));
        assert!(harness.brain.actions.contains(&MobAction::Sound {
            sound: SWOOP_SOUND,
            volume: 10.0,
            pitch
        }));
    }

    #[test]
    fn stopping_the_strategy_lifts_the_anchor_to_ten_to_thirty_above_the_terrain() {
        let mut goal = AttackStrategyGoal::default();
        let mut harness = harness(&[], 6);
        harness
            .world
            .blocks
            .insert(BlockPos::new(0, 80, 0), BlockKind::Stone.default_state());
        let mut mirror = MobRandom::new(6);
        let above = 10 + mirror.next_int(20);
        harness.run(|context| goal.stop(context));
        assert_eq!(
            harness.body.phantom.anchor,
            Some(BlockPos::new(0, 81 + above, 0))
        );
    }

    #[test]
    fn the_sweep_runs_only_in_the_swoop_phase_with_a_target() {
        let mut goal = SweepAttackGoal::default();
        let mut harness = targeted(Vec3::new(5.5, 70.0, 0.5), 1);
        assert!(!harness.run(|context| goal.can_use(context)));
        harness.brain.phantom.phase = AttackPhase::Swoop;
        assert!(harness.run(|context| goal.can_use(context)));
        assert_eq!(
            Goal::<AiContext<'_, Mock>>::flags(&goal),
            Flags::of(Flag::Move)
        );
    }

    #[test]
    fn the_sweep_aims_at_the_middle_of_the_target_and_ends_with_a_bite_and_an_event_on_contact() {
        let mut goal = SweepAttackGoal::default();
        let mut harness = targeted(Vec3::new(20.5, 70.0, 0.5), 1);
        harness.brain.phantom.phase = AttackPhase::Swoop;
        harness.run(|context| goal.tick(context));
        let height = harness.world.players[0].bounds.max_y - harness.world.players[0].bounds.min_y;
        assert_eq!(
            harness.brain.phantom.move_target,
            Vec3::new(20.5, 70.0 + height * 0.5, 0.5)
        );
        assert_eq!(harness.brain.phantom.phase, AttackPhase::Swoop);
        let mut touching = targeted(Vec3::new(0.5, 90.0, 0.5), 1);
        touching.brain.phantom.phase = AttackPhase::Swoop;
        touching.run(|context| goal.tick(context));
        assert_eq!(touching.brain.phantom.phase, AttackPhase::Circle);
        assert!(
            touching
                .brain
                .actions
                .contains(&MobAction::Attack(EntityId(2)))
        );
        assert!(touching.brain.actions.contains(&MobAction::LevelEvent {
            event: 1039,
            pos: BlockPos::new(0, 90, 0),
            data: 0
        }));
    }

    #[test]
    fn a_collision_or_a_hurt_while_swooping_ends_the_dive_without_a_bite() {
        let mut goal = SweepAttackGoal::default();
        let mut walled = targeted(Vec3::new(20.5, 70.0, 0.5), 1);
        walled.brain.phantom.phase = AttackPhase::Swoop;
        walled.mob.horizontal_collision = true;
        walled.run(|context| goal.tick(context));
        assert_eq!(walled.brain.phantom.phase, AttackPhase::Circle);
        let mut hurt = targeted(Vec3::new(20.5, 70.0, 0.5), 1);
        hurt.brain.phantom.phase = AttackPhase::Swoop;
        hurt.mob.species.phantom.hurt_recently = true;
        hurt.run(|context| goal.tick(context));
        assert_eq!(hurt.brain.phantom.phase, AttackPhase::Circle);
        assert!(!hurt.brain.actions.contains(&MobAction::Attack(EntityId(2))));
    }

    #[test]
    fn the_sweep_gives_up_when_the_target_is_a_creative_player_or_dead() {
        let mut goal = SweepAttackGoal::default();
        let mut harness = targeted(Vec3::new(20.5, 70.0, 0.5), 1);
        harness.brain.phantom.phase = AttackPhase::Swoop;
        harness.mob.tick_count = 100;
        assert!(harness.run(|context| goal.can_continue_to_use(context)));
        harness.world.players[0].creative = true;
        assert!(!harness.run(|context| goal.can_continue_to_use(context)));
        harness.world.players[0].creative = false;
        harness.world.players[0].alive = false;
        assert!(!harness.run(|context| goal.can_continue_to_use(context)));
    }

    #[test]
    fn a_cat_nearby_makes_it_hiss_and_scares_the_phantom_off_for_twenty_ticks() {
        let mut goal = SweepAttackGoal::default();
        let mut harness = targeted(Vec3::new(20.5, 70.0, 0.5), 1);
        harness.brain.phantom.phase = AttackPhase::Swoop;
        harness.mob.tick_count = 100;
        harness
            .world
            .others
            .push(living(9, EntityKind::Cat, Vec3::new(4.5, 90.0, 0.5)));
        assert!(!harness.run(|context| goal.can_continue_to_use(context)));
        assert!(goal.is_scared_of_cat());
        assert!(
            harness
                .brain
                .actions
                .contains(&MobAction::Air(crate::AirAction::Phantom(
                    PhantomAction::Hiss(EntityId(9))
                )))
        );
        harness.brain.actions.clear();
        harness.world.others.clear();
        assert!(!harness.run(|context| goal.can_continue_to_use(context)));
        assert!(harness.brain.actions.is_empty());
        harness.mob.tick_count = 121;
        assert!(harness.run(|context| goal.can_continue_to_use(context)));
    }

    #[test]
    fn stopping_the_sweep_forgets_the_target_and_circles_again() {
        let mut goal = SweepAttackGoal::default();
        let mut harness = targeted(Vec3::new(20.5, 70.0, 0.5), 1);
        harness.brain.phantom.phase = AttackPhase::Swoop;
        harness.run(|context| goal.stop(context));
        assert_eq!(harness.brain.phantom.phase, AttackPhase::Circle);
        let requests = std::mem::take(&mut harness.brain.target_requests);
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].to, None);
    }

    #[test]
    fn the_circle_runs_without_a_target_or_while_circling() {
        let mut goal = CircleAroundAnchorGoal::new();
        let mut idle = harness(&[], 1);
        assert!(idle.run(|context| goal.can_use(context)));
        let mut swooping = targeted(Vec3::new(20.5, 70.0, 0.5), 1);
        swooping.brain.phantom.phase = AttackPhase::Swoop;
        assert!(!swooping.run(|context| goal.can_use(context)));
        swooping.brain.phantom.phase = AttackPhase::Circle;
        assert!(swooping.run(|context| goal.can_use(context)));
    }

    #[test]
    fn starting_the_circle_rolls_its_radius_height_and_direction_and_steps_to_the_first_point() {
        let mut goal = CircleAroundAnchorGoal::new();
        let mut harness = harness(&[], 8);
        let mut mirror = MobRandom::new(8);
        let distance = 5.0 + mirror.next_float() * 10.0;
        let height = -4.0 + mirror.next_float() * 9.0;
        let clockwise = if mirror.next_boolean() { 1.0 } else { -1.0 };
        harness.run(|context| goal.start(context));
        assert_eq!(goal.distance(), distance);
        assert_eq!(goal.height(), height);
        assert_eq!(goal.clockwise(), clockwise);
        let angle = clockwise * 15.0 * 0.017_453_292;
        assert_eq!(goal.angle(), angle);
        let expected = Vec3::new(
            f64::from(distance * mth::cos(f64::from(angle))),
            95.0 + f64::from(-4.0 + height),
            f64::from(distance * mth::sin(f64::from(angle))),
        );
        assert_eq!(harness.brain.phantom.move_target, expected);
    }

    #[test]
    fn the_circle_picks_the_next_point_once_it_touches_the_current_one() {
        let mut goal = CircleAroundAnchorGoal::new();
        let mut harness = harness(&[], 8);
        harness.run(|context| goal.start(context));
        let before = goal.angle();
        harness.brain.phantom.move_target = harness.mob.position;
        harness.run(|context| goal.tick(context));
        assert_ne!(goal.angle(), before);
    }

    #[test]
    fn the_circle_climbs_over_terrain_below_and_dips_under_a_ceiling() {
        let mut goal = CircleAroundAnchorGoal::new();
        let mut floor = harness(&[], 8);
        floor.run(|context| goal.start(context));
        floor
            .world
            .blocks
            .insert(BlockPos::new(0, 89, 0), BlockKind::Stone.default_state());
        floor.brain.phantom.move_target = Vec3::new(10.0, 80.0, 10.0);
        floor.run(|context| goal.tick(context));
        assert!(goal.height() >= 1.0);
        let mut ceiling = harness(&[], 8);
        let mut other = CircleAroundAnchorGoal::new();
        ceiling.run(|context| other.start(context));
        ceiling
            .world
            .blocks
            .insert(BlockPos::new(0, 91, 0), BlockKind::Stone.default_state());
        ceiling.brain.phantom.move_target = Vec3::new(10.0, 100.0, 10.0);
        ceiling.run(|context| other.tick(context));
        assert!(other.height() <= -1.0);
    }

    #[test]
    fn a_circle_without_an_anchor_anchors_where_it_is() {
        let mut goal = CircleAroundAnchorGoal::new();
        let mut harness = harness(&[], 8);
        harness.body.phantom.anchor = None;
        harness.run(|context| goal.start(context));
        assert_eq!(harness.body.phantom.anchor, Some(BlockPos::new(0, 90, 0)));
    }

    #[test]
    fn the_player_scan_waits_ten_ticks_then_every_thirty_and_takes_the_highest_player() {
        let mut goal = AttackPlayerTargetGoal::new();
        assert_eq!(goal.next_scan_tick(), 10);
        let mut harness = harness(&[Vec3::new(5.5, 70.0, 0.5), Vec3::new(3.5, 85.0, 0.5)], 1);
        for _ in 0..10 {
            assert!(!harness.run(|context| goal.can_use(context)));
        }
        assert!(harness.run(|context| goal.can_use(context)));
        assert_eq!(goal.next_scan_tick(), 30);
        let requests = std::mem::take(&mut harness.brain.target_requests);
        assert_eq!(requests[0].to, Some(EntityId(3)));
        assert_eq!(requests[0].reason, TargetReason::ClosestPlayer);
        assert!(!harness.run(|context| goal.can_use(context)));
    }

    #[test]
    fn the_player_scan_ignores_creative_players_and_those_out_of_reach() {
        let mut goal = AttackPlayerTargetGoal::new();
        let mut harness = harness(&[Vec3::new(5.5, 85.0, 0.5), Vec3::new(40.5, 85.0, 0.5)], 1);
        harness.world.players[0].invulnerable = true;
        for _ in 0..10 {
            goal.can_use_ticks(&mut harness);
        }
        assert!(!harness.run(|context| goal.can_use(context)));
    }

    #[test]
    fn the_scan_target_is_kept_while_it_can_be_attacked() {
        let mut goal = AttackPlayerTargetGoal::new();
        let mut harness = targeted(Vec3::new(5.5, 85.0, 0.5), 1);
        assert!(harness.run(|context| goal.can_continue_to_use(context)));
        harness.world.players[0].invulnerable = true;
        assert!(!harness.run(|context| goal.can_continue_to_use(context)));
        assert_eq!(Goal::<AiContext<'_, Mock>>::flags(&goal), Flags::NONE);
    }

    impl AttackPlayerTargetGoal {
        fn can_use_ticks(&mut self, harness: &mut Harness) {
            let _ = harness.run(|context| self.can_use(context));
        }
    }
}
