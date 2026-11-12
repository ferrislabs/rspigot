use rspigot_domain::entity::species::raider::{
    PATROL_COMPANION_RANGE, PATROL_FAILURE_COOLDOWN, PATROL_LEADER_SPEED, PATROL_RETARGET_RANGE,
    PATROL_SIDE_DEGREES, PATROL_SIDE_STEP, PATROL_SPEED, PATROL_STEP, PATROL_TARGET_SPAN,
    PATROL_WANDER_SPAN,
};
use rspigot_domain::{BlockPos, EntityId, Vec3, mth};

use crate::action::MobAction;
use crate::brain::AiContext;
use crate::goal::{Flag, Flags, Goal, reduced_tick_delay};
use crate::goals::{MobGoal, NearestAttackableTargetGoal};
use crate::random::RandomSource;
use crate::registry::Goals;
use crate::world::{AiWorld, TargetKind, inflate};

const RAID_BANNER_PRIORITY: i32 = 1;
const PATHFIND_TO_RAID_PRIORITY: i32 = 3;
const PATROL_PRIORITY: i32 = 4;
const MOVE_THROUGH_VILLAGE_PRIORITY: i32 = 4;
const CELEBRATION_PRIORITY: i32 = 5;
const MOVE_THROUGH_VILLAGE_SPEED: f64 = 1.05;
const MOVE_THROUGH_VILLAGE_DISTANCE: i32 = 1;
const HEAL_RAIDERS_INTERVAL: i32 = 500;
const HEAL_RAIDERS_COOLDOWN: i32 = 200;
const ATTACK_PLAYERS_INTERVAL: i32 = 10;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct RaiderFacts {
    pub patrolling: bool,
    pub patrol_leader: bool,
    pub patrol_target: Option<BlockPos>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RaiderAction {
    StopPatrolling,
    SetPatrolTarget {
        mob: EntityId,
        target: BlockPos,
    },
    ShoutTarget {
        mob: EntityId,
        target: EntityId,
        aggressive: bool,
    },
}

fn y_rot(vector: Vec3, angle: f32) -> Vec3 {
    let cos = f64::from(mth::cos(f64::from(angle)));
    let sin = f64::from(mth::sin(f64::from(angle)));
    Vec3::new(
        vector.x * cos + vector.z * sin,
        vector.y,
        vector.z * cos - vector.x * sin,
    )
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LongDistancePatrolGoal {
    speed_modifier: f64,
    leader_speed_modifier: f64,
    cooldown_until: i64,
}

impl Default for LongDistancePatrolGoal {
    fn default() -> Self {
        Self::new(PATROL_SPEED, PATROL_LEADER_SPEED)
    }
}

impl LongDistancePatrolGoal {
    pub const fn new(speed_modifier: f64, leader_speed_modifier: f64) -> Self {
        Self {
            speed_modifier,
            leader_speed_modifier,
            cooldown_until: -1,
        }
    }

    pub fn cooldown_until(&self) -> i64 {
        self.cooldown_until
    }

    fn can_use<W: AiWorld>(&self, context: &mut AiContext<'_, W>) -> bool {
        let facts = context.mob.species.raider;
        context.world.game_time() >= self.cooldown_until
            && facts.patrolling
            && context.target().is_none()
            && facts.patrol_target.is_some()
    }

    fn companions<W: AiWorld>(context: &AiContext<'_, W>) -> Vec<EntityId> {
        let area = inflate(
            &context.mob.bounds(),
            PATROL_COMPANION_RANGE,
            PATROL_COMPANION_RANGE,
            PATROL_COMPANION_RANGE,
        );
        context
            .world
            .living_of(TargetKind::Class("PatrollingMonster"), &area)
            .into_iter()
            .filter(|other| other.id != context.mob.id)
            .map(|other| other.id)
            .collect()
    }

    fn move_randomly<W: AiWorld>(&self, context: &mut AiContext<'_, W>) -> bool {
        let dx = context.brain.random.next_int(PATROL_WANDER_SPAN);
        let dz = context.brain.random.next_int(PATROL_WANDER_SPAN);
        let half = PATROL_WANDER_SPAN / 2;
        let target = context
            .world
            .motion_blocking_no_leaves_pos(context.mob.block_pos().offset(dx - half, 0, dz - half));
        context.move_to_pos(
            Vec3::new(
                f64::from(target.x),
                f64::from(target.y),
                f64::from(target.z),
            ),
            self.speed_modifier,
        )
    }

    fn tick<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        let facts = context.mob.species.raider;
        let leader = facts.patrol_leader;
        if !context.brain.navigation.is_done() {
            return;
        }
        let companions = Self::companions(context);
        if facts.patrolling && companions.is_empty() {
            context.act(MobAction::Raider(RaiderAction::StopPatrolling));
            return;
        }
        let Some(patrol_target) = facts.patrol_target else {
            return;
        };
        let position = context.mob.position;
        if leader {
            let center = Vec3::new(
                f64::from(patrol_target.x) + 0.5,
                f64::from(patrol_target.y) + 0.5,
                f64::from(patrol_target.z) + 0.5,
            );
            if center.distance_squared(position) < PATROL_RETARGET_RANGE * PATROL_RETARGET_RANGE {
                let span = PATROL_TARGET_SPAN;
                let dx = context.brain.random.next_int(span) - span / 2;
                let dz = context.brain.random.next_int(span) - span / 2;
                let target = context.mob.block_pos().offset(dx, 0, dz);
                context.act(MobAction::Raider(RaiderAction::SetPatrolTarget {
                    mob: context.mob.id,
                    target,
                }));
                return;
            }
        }
        let anchor = patrol_target.center();
        let delta = Vec3::new(
            position.x - anchor.x,
            position.y - anchor.y,
            position.z - anchor.z,
        );
        let turned = y_rot(delta, PATROL_SIDE_DEGREES as f32).scale(PATROL_SIDE_STEP);
        let side = Vec3::new(
            turned.x + anchor.x,
            turned.y + anchor.y,
            turned.z + anchor.z,
        );
        let heading = Vec3::new(
            side.x - position.x,
            side.y - position.y,
            side.z - position.z,
        )
        .normalize()
        .scale(PATROL_STEP);
        let aimed = Vec3::new(
            heading.x + position.x,
            heading.y + position.y,
            heading.z + position.z,
        );
        let goal = context.world.motion_blocking_no_leaves_pos(aimed.block());
        let speed = if leader {
            self.leader_speed_modifier
        } else {
            self.speed_modifier
        };
        let target = Vec3::new(f64::from(goal.x), f64::from(goal.y), f64::from(goal.z));
        if context.move_to_pos(target, speed) {
            if leader {
                for companion in companions {
                    context.act(MobAction::Raider(RaiderAction::SetPatrolTarget {
                        mob: companion,
                        target: goal,
                    }));
                }
            }
        } else {
            self.move_randomly(context);
            self.cooldown_until = context.world.game_time() + PATROL_FAILURE_COOLDOWN;
        }
    }
}

impl<W: AiWorld> Goal<AiContext<'_, W>> for LongDistancePatrolGoal {
    fn flags(&self) -> Flags {
        Flags::of(Flag::Move)
    }

    fn can_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        Self::can_use(self, context)
    }

    fn tick(&mut self, context: &mut AiContext<'_, W>) {
        Self::tick(self, context);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ObtainRaidLeaderBannerGoal;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PathfindToRaidGoal;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RaiderMoveThroughVillageGoal {
    speed_modifier: f64,
    distance: i32,
}

impl RaiderMoveThroughVillageGoal {
    pub const fn new(speed_modifier: f64, distance: i32) -> Self {
        Self {
            speed_modifier,
            distance,
        }
    }

    pub const fn speed_modifier(&self) -> f64 {
        self.speed_modifier
    }

    pub const fn distance(&self) -> i32 {
        self.distance
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RaiderCelebrationGoal;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NearestHealableRaiderTargetGoal {
    random_interval: i32,
    cooldown: i32,
}

impl Default for NearestHealableRaiderTargetGoal {
    fn default() -> Self {
        Self {
            random_interval: reduced_tick_delay(HEAL_RAIDERS_INTERVAL),
            cooldown: 0,
        }
    }
}

impl NearestHealableRaiderTargetGoal {
    pub fn cooldown(&self) -> i32 {
        self.cooldown
    }

    pub fn random_interval(&self) -> i32 {
        self.random_interval
    }

    pub fn decrement_cooldown(&mut self) {
        self.cooldown = self.cooldown.wrapping_sub(1);
    }

    fn can_use<W: AiWorld>(&self, context: &mut AiContext<'_, W>) -> bool {
        if self.cooldown < 1 {
            let _ = context.brain.random.next_boolean();
        }
        false
    }

    fn start(&mut self) {
        self.cooldown = reduced_tick_delay(HEAL_RAIDERS_COOLDOWN);
    }
}

impl<W: AiWorld> Goal<AiContext<'_, W>> for NearestHealableRaiderTargetGoal {
    fn flags(&self) -> Flags {
        Flags::of(Flag::Target)
    }

    fn can_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        Self::can_use(self, context)
    }

    fn can_continue_to_use(&mut self, _context: &mut AiContext<'_, W>) -> bool {
        false
    }

    fn start(&mut self, _context: &mut AiContext<'_, W>) {
        Self::start(self);
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NearestAttackableWitchTargetGoal {
    target: NearestAttackableTargetGoal,
    can_attack: bool,
}

impl Default for NearestAttackableWitchTargetGoal {
    fn default() -> Self {
        Self {
            target: NearestAttackableTargetGoal::with(
                TargetKind::Player,
                ATTACK_PLAYERS_INTERVAL,
                true,
                crate::goals::TargetFilter::Any,
            ),
            can_attack: true,
        }
    }
}

impl NearestAttackableWitchTargetGoal {
    pub fn can_attack(&self) -> bool {
        self.can_attack
    }

    pub fn set_can_attack(&mut self, can_attack: bool) {
        self.can_attack = can_attack;
    }
}

impl<W: AiWorld> Goal<AiContext<'_, W>> for NearestAttackableWitchTargetGoal {
    fn flags(&self) -> Flags {
        Flags::of(Flag::Target)
    }

    fn can_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        self.can_attack && self.target.can_use(context)
    }

    fn can_continue_to_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        self.target.can_continue_to_use(context)
    }

    fn start(&mut self, context: &mut AiContext<'_, W>) {
        self.target.start(context);
    }

    fn stop(&mut self, context: &mut AiContext<'_, W>) {
        self.target.stop(context);
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RaiderGoal {
    LongDistancePatrol(LongDistancePatrolGoal),
    ObtainRaidLeaderBanner(ObtainRaidLeaderBannerGoal),
    PathfindToRaid(PathfindToRaidGoal),
    MoveThroughVillage(RaiderMoveThroughVillageGoal),
    Celebration(RaiderCelebrationGoal),
    HealRaiders(NearestHealableRaiderTargetGoal),
}

impl<W: AiWorld> Goal<AiContext<'_, W>> for RaiderGoal {
    fn flags(&self) -> Flags {
        match self {
            Self::LongDistancePatrol(goal) => Goal::<AiContext<'_, W>>::flags(goal),
            Self::HealRaiders(goal) => Goal::<AiContext<'_, W>>::flags(goal),
            Self::ObtainRaidLeaderBanner(_)
            | Self::PathfindToRaid(_)
            | Self::MoveThroughVillage(_)
            | Self::Celebration(_) => Flags::of(Flag::Move),
        }
    }

    fn can_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        match self {
            Self::LongDistancePatrol(goal) => Goal::can_use(goal, context),
            Self::HealRaiders(goal) => Goal::can_use(goal, context),
            Self::ObtainRaidLeaderBanner(_)
            | Self::PathfindToRaid(_)
            | Self::MoveThroughVillage(_)
            | Self::Celebration(_) => false,
        }
    }

    fn can_continue_to_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        match self {
            Self::LongDistancePatrol(goal) => Goal::can_continue_to_use(goal, context),
            Self::HealRaiders(goal) => Goal::can_continue_to_use(goal, context),
            Self::ObtainRaidLeaderBanner(_)
            | Self::PathfindToRaid(_)
            | Self::MoveThroughVillage(_)
            | Self::Celebration(_) => false,
        }
    }

    fn start(&mut self, context: &mut AiContext<'_, W>) {
        match self {
            Self::HealRaiders(goal) => Goal::start(goal, context),
            Self::LongDistancePatrol(_)
            | Self::ObtainRaidLeaderBanner(_)
            | Self::PathfindToRaid(_)
            | Self::MoveThroughVillage(_)
            | Self::Celebration(_) => {}
        }
    }

    fn tick(&mut self, context: &mut AiContext<'_, W>) {
        if let Self::LongDistancePatrol(goal) = self {
            Goal::tick(goal, context);
        }
    }
}

pub(crate) fn register(add: &mut Goals<'_>) {
    add.goal(
        PATROL_PRIORITY,
        MobGoal::Raider(RaiderGoal::LongDistancePatrol(
            LongDistancePatrolGoal::default(),
        )),
    );
    add.goal(
        RAID_BANNER_PRIORITY,
        MobGoal::Raider(RaiderGoal::ObtainRaidLeaderBanner(
            ObtainRaidLeaderBannerGoal,
        )),
    );
    add.goal(
        PATHFIND_TO_RAID_PRIORITY,
        MobGoal::Raider(RaiderGoal::PathfindToRaid(PathfindToRaidGoal)),
    );
    add.goal(
        MOVE_THROUGH_VILLAGE_PRIORITY,
        MobGoal::Raider(RaiderGoal::MoveThroughVillage(
            RaiderMoveThroughVillageGoal::new(
                MOVE_THROUGH_VILLAGE_SPEED,
                MOVE_THROUGH_VILLAGE_DISTANCE,
            ),
        )),
    );
    add.goal(
        CELEBRATION_PRIORITY,
        MobGoal::Raider(RaiderGoal::Celebration(RaiderCelebrationGoal)),
    );
}

#[cfg(test)]
mod tests {
    use rspigot_entities::EntityKind;

    use super::*;
    use crate::brain::MobAi;
    use crate::mock::{Harness, Mock, living, seed_where};
    use crate::random::MobRandom;
    use crate::registry::GoalSettings;
    use crate::{HurtByTargetGoal, TargetFilter};

    const SEED: i64 = 11;

    fn witch(world: Mock) -> Harness {
        Harness::new(EntityKind::Witch, Vec3::new(0.5, 64.0, 0.5), world, SEED)
    }

    fn patrolling(harness: &mut Harness, target: BlockPos, leader: bool) {
        harness.mob.species.raider = RaiderFacts {
            patrolling: true,
            patrol_leader: leader,
            patrol_target: Some(target),
        };
    }

    fn actions(harness: &mut Harness) -> Vec<MobAction> {
        std::mem::take(&mut harness.brain.actions)
    }

    fn companion_world() -> Mock {
        Mock {
            others: vec![living(7, EntityKind::Pillager, Vec3::new(4.5, 64.0, 0.5))],
            ..Mock::default()
        }
    }

    #[test]
    fn the_patrol_goal_needs_a_patrol_target_and_no_attack_target() {
        let mut harness = witch(Mock::default());
        let goal = LongDistancePatrolGoal::default();
        assert!(!harness.run(|context| goal.can_use(context)));
        patrolling(&mut harness, BlockPos::new(100, 64, 0), false);
        assert!(harness.run(|context| goal.can_use(context)));
        harness.mob.species.raider.patrol_target = None;
        assert!(!harness.run(|context| goal.can_use(context)));
        patrolling(&mut harness, BlockPos::new(100, 64, 0), false);
        harness.mob.species.raider.patrolling = false;
        assert!(!harness.run(|context| goal.can_use(context)));
        patrolling(&mut harness, BlockPos::new(100, 64, 0), false);
        harness.brain.target = Some(EntityId(2));
        harness
            .world
            .players
            .push(living(2, EntityKind::Player, Vec3::new(3.5, 64.0, 0.5)));
        assert!(!harness.run(|context| goal.can_use(context)));
    }

    #[test]
    fn the_patrol_goal_waits_out_its_cooldown() {
        let mut harness = witch(Mock::default());
        patrolling(&mut harness, BlockPos::new(100, 64, 0), false);
        let mut goal = LongDistancePatrolGoal::default();
        assert_eq!(goal.cooldown_until(), -1);
        goal.cooldown_until = 101;
        assert!(!harness.run(|context| goal.can_use(context)));
        goal.cooldown_until = 100;
        assert!(harness.run(|context| goal.can_use(context)));
    }

    #[test]
    fn a_lone_patrolling_raider_stops_patrolling() {
        let mut harness = witch(Mock::default());
        patrolling(&mut harness, BlockPos::new(100, 64, 0), false);
        let mut goal = LongDistancePatrolGoal::default();
        harness.run(|context| goal.tick(context));
        assert_eq!(
            actions(&mut harness),
            vec![MobAction::Raider(RaiderAction::StopPatrolling)]
        );
    }

    fn expected_goal(from: Vec3, target: BlockPos) -> BlockPos {
        let anchor = (
            f64::from(target.x) + 0.5,
            f64::from(target.y),
            f64::from(target.z) + 0.5,
        );
        let delta = (from.x - anchor.0, from.y - anchor.1, from.z - anchor.2);
        let angle: f64 = 90.0;
        let (cos, sin) = (angle.cos(), angle.sin());
        let turned = (
            (delta.0 * cos + delta.2 * sin) * 0.4 + anchor.0,
            delta.1 * 0.4 + anchor.1,
            (delta.2 * cos - delta.0 * sin) * 0.4 + anchor.2,
        );
        let heading = (turned.0 - from.x, turned.1 - from.y, turned.2 - from.z);
        let length = (heading.0 * heading.0 + heading.1 * heading.1 + heading.2 * heading.2).sqrt();
        BlockPos::new(
            (heading.0 / length * 10.0 + from.x).floor() as i32,
            64,
            (heading.2 / length * 10.0 + from.z).floor() as i32,
        )
    }

    #[test]
    fn a_follower_walks_ten_blocks_toward_a_point_beside_the_patrol_target() {
        let target = BlockPos::new(100, 64, 20);
        let mut harness = witch(companion_world());
        patrolling(&mut harness, target, false);
        let mut goal = LongDistancePatrolGoal::default();
        harness.run(|context| goal.tick(context));
        assert!(harness.brain.navigation.is_in_progress());
        assert!(actions(&mut harness).is_empty());
        let end = harness
            .brain
            .navigation
            .path()
            .map(|path| path.target())
            .expect("a path was made");
        let wanted = expected_goal(harness.mob.position, target);
        assert!((end.x - wanted.x).abs() <= 1 && (end.z - wanted.z).abs() <= 1);
        assert_eq!(goal.cooldown_until(), -1);
    }

    #[test]
    fn a_leader_hands_its_new_waypoint_to_every_companion() {
        let target = BlockPos::new(100, 64, 20);
        let mut harness = witch(companion_world());
        patrolling(&mut harness, target, true);
        let mut goal = LongDistancePatrolGoal::default();
        harness.run(|context| goal.tick(context));
        let wanted = expected_goal(harness.mob.position, target);
        let taken = actions(&mut harness);
        assert_eq!(taken.len(), 1);
        let MobAction::Raider(RaiderAction::SetPatrolTarget { mob, target }) = taken[0] else {
            panic!("a waypoint action, got {:?}", taken[0]);
        };
        assert_eq!(mob, EntityId(7));
        assert!((target.x - wanted.x).abs() <= 1 && (target.z - wanted.z).abs() <= 1);
        assert_eq!(target.y, 64);
    }

    #[test]
    fn a_leader_near_its_patrol_target_picks_a_new_one_within_five_hundred_blocks() {
        let mut harness = witch(companion_world());
        patrolling(&mut harness, BlockPos::new(5, 64, 0), true);
        let mut mirror = MobRandom::new(SEED);
        let dx = mirror.next_int(1000) - 500;
        let dz = mirror.next_int(1000) - 500;
        let mut goal = LongDistancePatrolGoal::default();
        harness.run(|context| goal.tick(context));
        assert_eq!(
            actions(&mut harness),
            vec![MobAction::Raider(RaiderAction::SetPatrolTarget {
                mob: EntityId(1),
                target: BlockPos::new(dx, 64, dz),
            })]
        );
    }

    #[test]
    fn a_busy_navigation_keeps_the_patrol_goal_still() {
        let mut harness = witch(companion_world());
        patrolling(&mut harness, BlockPos::new(100, 64, 20), false);
        let mut goal = LongDistancePatrolGoal::default();
        harness.run(|context| goal.tick(context));
        let before = harness.brain.navigation.path().map(|path| path.target());
        harness.mob.species.raider.patrol_target = Some(BlockPos::new(-100, 64, -20));
        harness.run(|context| goal.tick(context));
        assert_eq!(
            harness.brain.navigation.path().map(|path| path.target()),
            before
        );
    }

    #[test]
    fn the_raid_goals_never_start_without_a_raid() {
        let mut harness = witch(Mock::default());
        let mut goals = [
            RaiderGoal::ObtainRaidLeaderBanner(ObtainRaidLeaderBannerGoal),
            RaiderGoal::PathfindToRaid(PathfindToRaidGoal),
            RaiderGoal::MoveThroughVillage(RaiderMoveThroughVillageGoal::new(1.05, 1)),
            RaiderGoal::Celebration(RaiderCelebrationGoal),
        ];
        for goal in &mut goals {
            assert!(!harness.run(|context| Goal::can_use(goal, context)));
            assert!(!harness.run(|context| Goal::can_continue_to_use(goal, context)));
            assert_eq!(
                Goal::<AiContext<'_, Mock>>::flags(goal),
                Flags::of(Flag::Move)
            );
        }
    }

    #[test]
    fn healing_raiders_rolls_a_boolean_only_when_the_cooldown_has_run_out() {
        let mut harness = witch(Mock::default());
        let mut goal = NearestHealableRaiderTargetGoal::default();
        assert_eq!(goal.random_interval(), 250);
        assert_eq!(goal.cooldown(), 0);
        let mut mirror = MobRandom::new(SEED);
        assert!(!harness.run(|context| goal.can_use(context)));
        let _ = mirror.next_boolean();
        assert_eq!(harness.brain.random.next_int(1000), mirror.next_int(1000));
        goal.cooldown = 1;
        let mut untouched = MobRandom::new(SEED);
        let mut again = witch(Mock::default());
        assert!(!again.run(|context| goal.can_use(context)));
        assert_eq!(again.brain.random.next_int(1000), untouched.next_int(1000));
    }

    #[test]
    fn healing_raiders_counts_its_cooldown_down_and_restarts_it_at_a_hundred() {
        let mut goal = NearestHealableRaiderTargetGoal::default();
        goal.decrement_cooldown();
        goal.decrement_cooldown();
        assert_eq!(goal.cooldown(), -2);
        goal.start();
        assert_eq!(goal.cooldown(), 100);
        assert_eq!(
            Goal::<AiContext<'_, Mock>>::flags(&goal),
            Flags::of(Flag::Target)
        );
    }

    #[test]
    fn attacking_players_is_gated_by_the_witch_and_otherwise_a_plain_player_search() {
        let seed = seed_where(|random| random.next_int(5) == 0);
        let world = Mock {
            players: vec![living(2, EntityKind::Player, Vec3::new(4.5, 64.0, 0.5))],
            ..Mock::default()
        };
        let mut harness = Harness::new(EntityKind::Witch, Vec3::new(0.5, 64.0, 0.5), world, seed);
        let mut goal = NearestAttackableWitchTargetGoal::default();
        assert!(goal.can_attack());
        goal.set_can_attack(false);
        let mut mirror = MobRandom::new(seed);
        assert!(!harness.run(|context| goal.can_use(context)));
        assert_eq!(harness.brain.random.next_int(1000), mirror.next_int(1000));
        goal.set_can_attack(true);
        let mut harness = Harness::new(
            EntityKind::Witch,
            Vec3::new(0.5, 64.0, 0.5),
            Mock {
                players: vec![living(2, EntityKind::Player, Vec3::new(4.5, 64.0, 0.5))],
                ..Mock::default()
            },
            seed,
        );
        assert!(harness.run(|context| goal.can_use(context)));
        let mut plain = crate::goals::NearestAttackableTargetGoal::with(
            TargetKind::Player,
            10,
            true,
            TargetFilter::Any,
        );
        let mut twin = Harness::new(
            EntityKind::Witch,
            Vec3::new(0.5, 64.0, 0.5),
            Mock {
                players: vec![living(2, EntityKind::Player, Vec3::new(4.5, 64.0, 0.5))],
                ..Mock::default()
            },
            seed,
        );
        assert!(twin.run(|context| plain.can_use(context)));
        assert_eq!(
            harness.brain.random.next_int(1000),
            twin.brain.random.next_int(1000)
        );
    }

    fn healable(ai: &MobAi) -> NearestHealableRaiderTargetGoal {
        ai.targets()
            .goals()
            .iter()
            .find_map(|wrapped| match wrapped.goal() {
                MobGoal::Raider(RaiderGoal::HealRaiders(goal)) => Some(*goal),
                _ => None,
            })
            .expect("a heal goal")
    }

    fn attacking(ai: &MobAi) -> NearestAttackableWitchTargetGoal {
        ai.targets()
            .goals()
            .iter()
            .find_map(|wrapped| match wrapped.goal() {
                MobGoal::WitchTarget(goal) => Some(*goal),
                _ => None,
            })
            .expect("an attack goal")
    }

    #[test]
    fn each_witch_step_counts_the_heal_cooldown_down_and_opens_the_player_attack_when_it_ends() {
        let mut ai = MobAi::new(EntityKind::Witch, 1, GoalSettings::default());
        for goal in ai.targets_mut().goals_mut() {
            if let MobGoal::Raider(RaiderGoal::HealRaiders(heal)) = goal {
                heal.cooldown = 3;
            }
        }
        ai.raider_ai_step();
        assert_eq!(healable(&ai).cooldown(), 2);
        assert!(!attacking(&ai).can_attack());
        ai.raider_ai_step();
        assert!(!attacking(&ai).can_attack());
        ai.raider_ai_step();
        assert_eq!(healable(&ai).cooldown(), 0);
        assert!(attacking(&ai).can_attack());
        ai.raider_ai_step();
        assert_eq!(healable(&ai).cooldown(), -1);
        assert!(attacking(&ai).can_attack());
    }

    fn hurt_by(attacker: EntityKind) -> bool {
        let world = Mock {
            others: vec![living(9, attacker, Vec3::new(2.5, 64.0, 0.5))],
            ..Mock::default()
        };
        let mut harness = witch(world);
        harness.mob.last_hurt_by_mob = Some(EntityId(9));
        harness.mob.last_hurt_by_mob_at = 5;
        let mut goal = HurtByTargetGoal::ignoring_damage_from(&["Raider"]);
        harness.run(|context| goal.can_use(context))
    }

    #[test]
    fn a_witch_does_not_retaliate_against_a_raider_but_does_against_anyone_else() {
        assert!(!hurt_by(EntityKind::Pillager));
        assert!(!hurt_by(EntityKind::Witch));
        assert!(hurt_by(EntityKind::Zombie));
    }

    #[test]
    fn the_witch_registers_the_raider_goals_then_its_own_with_the_vanilla_priorities() {
        let ai = MobAi::new(EntityKind::Witch, 1, GoalSettings::default());
        let goals: Vec<_> = ai
            .goals()
            .goals()
            .iter()
            .map(|wrapped| (wrapped.priority(), wrapped.goal().clone()))
            .collect();
        assert_eq!(
            goals,
            vec![
                (
                    4,
                    MobGoal::Raider(RaiderGoal::LongDistancePatrol(LongDistancePatrolGoal::new(
                        0.7, 0.595
                    )))
                ),
                (
                    1,
                    MobGoal::Raider(RaiderGoal::ObtainRaidLeaderBanner(
                        ObtainRaidLeaderBannerGoal
                    ))
                ),
                (
                    3,
                    MobGoal::Raider(RaiderGoal::PathfindToRaid(PathfindToRaidGoal))
                ),
                (
                    4,
                    MobGoal::Raider(RaiderGoal::MoveThroughVillage(
                        RaiderMoveThroughVillageGoal::new(1.05, 1)
                    ))
                ),
                (
                    5,
                    MobGoal::Raider(RaiderGoal::Celebration(RaiderCelebrationGoal))
                ),
                (1, MobGoal::Float(crate::goals::FloatGoal)),
                (
                    2,
                    MobGoal::RangedAttack(crate::RangedAttackGoal::new(1.0, 60, 10.0))
                ),
                (
                    2,
                    MobGoal::Stroll(crate::RandomStrollGoal::water_avoiding(1.0))
                ),
                (3, MobGoal::LookAtPlayer(crate::LookAtPlayerGoal::new(8.0))),
                (
                    3,
                    MobGoal::RandomLookAround(crate::RandomLookAroundGoal::default())
                ),
            ]
        );
        let targets: Vec<_> = ai
            .targets()
            .goals()
            .iter()
            .map(|wrapped| (wrapped.priority(), wrapped.goal().clone()))
            .collect();
        assert_eq!(
            targets,
            vec![
                (
                    1,
                    MobGoal::HurtByTarget(HurtByTargetGoal::ignoring_damage_from(&["Raider"]))
                ),
                (
                    2,
                    MobGoal::Raider(RaiderGoal::HealRaiders(
                        NearestHealableRaiderTargetGoal::default()
                    ))
                ),
                (
                    3,
                    MobGoal::WitchTarget(NearestAttackableWitchTargetGoal::default())
                ),
            ]
        );
    }

    #[test]
    fn the_witch_floats_so_it_swims_like_other_monsters() {
        let ai = MobAi::new(EntityKind::Witch, 1, GoalSettings::default());
        assert!(ai.can_float());
    }
}
