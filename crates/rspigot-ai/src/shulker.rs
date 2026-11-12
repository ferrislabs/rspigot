use rspigot_domain::entity::Axis;
use rspigot_domain::entity::bullet::axis_of;
use rspigot_domain::entity::species::shulker;
use rspigot_domain::{EntityId, Vec3};

use crate::action::{MobAction, TargetReason};
use crate::brain::AiContext;
use crate::goal::{Flag, Flags, Goal, adjusted_tick_delay, reduced_tick_delay};
use crate::random::RandomSource;
use crate::world::{AiWorld, Living, MobView, inflate};

pub use crate::world::ShulkerFacts;

const SHOOT_RANGE_SQR: f64 = shulker::SHOOT_RANGE_SQR;
const SEARCH_HEIGHT: f64 = 4.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ShulkerAction {
    SetRawPeek(i32),
    Shoot {
        target: EntityId,
        axis: Axis,
        pitch: f32,
    },
}

pub(crate) fn target_search_area(mob: &MobView, follow: f64) -> rspigot_domain::entity::Aabb {
    let area = mob.bounds();
    match axis_of(mob.species.shulker.attach_face) {
        Axis::X => inflate(&area, SEARCH_HEIGHT, follow, follow),
        Axis::Z => inflate(&area, follow, follow, SEARCH_HEIGHT),
        Axis::Y => inflate(&area, follow, SEARCH_HEIGHT, follow),
    }
}

fn target_of<W: AiWorld>(context: &AiContext<'_, W>) -> Option<Living> {
    context.brain.target.and_then(|id| context.world.living(id))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ShulkerAttackGoal {
    attack_time: i32,
}

impl ShulkerAttackGoal {
    pub fn attack_time(&self) -> i32 {
        self.attack_time
    }
}

impl<W: AiWorld> Goal<AiContext<'_, W>> for ShulkerAttackGoal {
    fn flags(&self) -> Flags {
        Flags::of(Flag::Move).with(Flag::Look)
    }

    fn can_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        target_of(context).is_some_and(|target| target.alive) && !context.world.peaceful()
    }

    fn start(&mut self, context: &mut AiContext<'_, W>) {
        self.attack_time = shulker::SHOOT_COOLDOWN;
        context.act(MobAction::Shulker(ShulkerAction::SetRawPeek(
            shulker::ATTACK_PEEK,
        )));
    }

    fn stop(&mut self, context: &mut AiContext<'_, W>) {
        context.act(MobAction::Shulker(ShulkerAction::SetRawPeek(0)));
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, context: &mut AiContext<'_, W>) {
        if context.world.peaceful() {
            return;
        }
        self.attack_time -= 1;
        let Some(target) = target_of(context) else {
            return;
        };
        context.brain.look.set_look_at(
            Vec3::new(target.position.x, target.eye_y(), target.position.z),
            shulker::MAX_HEAD_ROTATION,
            shulker::MAX_HEAD_ROTATION,
        );
        if context.mob.distance_to_sqr(target.position) >= SHOOT_RANGE_SQR {
            context.set_target(None, TargetReason::Unknown);
        } else if self.attack_time < 1 {
            let roll = context
                .brain
                .random
                .next_int(shulker::SHOOT_COOLDOWN_RANDOM);
            self.attack_time = roll * shulker::SHOOT_COOLDOWN / 2 + shulker::SHOOT_COOLDOWN;
            let axis = axis_of(context.mob.species.shulker.attach_face);
            let first = context.brain.random.next_float();
            let second = context.brain.random.next_float();
            context.act(MobAction::Shulker(ShulkerAction::Shoot {
                target: target.id,
                axis,
                pitch: (first - second) * shulker::SHOOT_SOUND_PITCH_SPREAD + 1.0,
            }));
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ShulkerPeekGoal {
    peek_time: i32,
}

impl ShulkerPeekGoal {
    pub fn peek_time(&self) -> i32 {
        self.peek_time
    }
}

impl<W: AiWorld> Goal<AiContext<'_, W>> for ShulkerPeekGoal {
    fn flags(&self) -> Flags {
        Flags::NONE
    }

    fn can_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        context.brain.target.is_none()
            && context
                .brain
                .random
                .next_int(reduced_tick_delay(shulker::PEEK_ODDS))
                == 0
            && context.mob.species.shulker.can_stay
    }

    fn can_continue_to_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        context.brain.target.is_none() && self.peek_time > 0
    }

    fn start(&mut self, context: &mut AiContext<'_, W>) {
        let units = context.brain.random.next_int(shulker::PEEK_TIME_RANDOM) + 1;
        self.peek_time = adjusted_tick_delay(false, units * shulker::PEEK_TIME_UNIT);
        context.act(MobAction::Shulker(ShulkerAction::SetRawPeek(
            shulker::IDLE_PEEK,
        )));
    }

    fn stop(&mut self, context: &mut AiContext<'_, W>) {
        if context.brain.target.is_none() {
            context.act(MobAction::Shulker(ShulkerAction::SetRawPeek(0)));
        }
    }

    fn tick(&mut self, _context: &mut AiContext<'_, W>) {
        self.peek_time -= 1;
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ShulkerGoal {
    Attack(ShulkerAttackGoal),
    Peek(ShulkerPeekGoal),
}

impl<W: AiWorld> Goal<AiContext<'_, W>> for ShulkerGoal {
    fn flags(&self) -> Flags {
        match self {
            Self::Attack(goal) => Goal::<AiContext<'_, W>>::flags(goal),
            Self::Peek(goal) => Goal::<AiContext<'_, W>>::flags(goal),
        }
    }

    fn can_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        match self {
            Self::Attack(goal) => Goal::can_use(goal, context),
            Self::Peek(goal) => Goal::can_use(goal, context),
        }
    }

    fn can_continue_to_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        match self {
            Self::Attack(goal) => Goal::can_continue_to_use(goal, context),
            Self::Peek(goal) => Goal::can_continue_to_use(goal, context),
        }
    }

    fn start(&mut self, context: &mut AiContext<'_, W>) {
        match self {
            Self::Attack(goal) => Goal::start(goal, context),
            Self::Peek(goal) => Goal::start(goal, context),
        }
    }

    fn stop(&mut self, context: &mut AiContext<'_, W>) {
        match self {
            Self::Attack(goal) => Goal::stop(goal, context),
            Self::Peek(goal) => Goal::stop(goal, context),
        }
    }

    fn requires_update_every_tick(&self) -> bool {
        match self {
            Self::Attack(goal) => Goal::<AiContext<'_, W>>::requires_update_every_tick(goal),
            Self::Peek(goal) => Goal::<AiContext<'_, W>>::requires_update_every_tick(goal),
        }
    }

    fn tick(&mut self, context: &mut AiContext<'_, W>) {
        match self {
            Self::Attack(goal) => Goal::tick(goal, context),
            Self::Peek(goal) => Goal::tick(goal, context),
        }
    }
}

#[cfg(test)]
mod tests {
    use rspigot_blocks::Face;
    use rspigot_entities::EntityKind;

    use super::*;
    use crate::mock::{Harness, Mock, living, seed_where};

    const SEED: i64 = 11;

    fn harness(target: Option<Vec3>, world: Mock) -> Harness {
        let world = Mock {
            players: target
                .map(|at| vec![living(2, EntityKind::Player, at)])
                .unwrap_or(world.players),
            ..world
        };
        let mut harness = Harness::new(EntityKind::Shulker, Vec3::new(0.5, 64.0, 0.5), world, SEED);
        harness.brain.target = target.map(|_| EntityId(2));
        harness
    }

    fn drain(harness: &mut Harness) -> Vec<MobAction> {
        std::mem::take(&mut harness.brain.actions)
    }

    fn peeks(actions: &[MobAction]) -> Vec<i32> {
        actions
            .iter()
            .filter_map(|action| match action {
                MobAction::Shulker(ShulkerAction::SetRawPeek(peek)) => Some(*peek),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_attack_goal_needs_a_live_target_and_a_non_peaceful_world_and_holds_move_and_look() {
        let mut goal = ShulkerAttackGoal::default();
        let mut harness = harness(Some(Vec3::new(5.5, 64.0, 0.5)), Mock::default());
        assert!(harness.run(|context| goal.can_use(context)));
        assert!(harness.run(|context| goal.can_continue_to_use(context)));
        assert_eq!(
            Goal::<AiContext<'_, Mock>>::flags(&goal),
            Flags::of(Flag::Move).with(Flag::Look)
        );
        assert!(Goal::<AiContext<'_, Mock>>::requires_update_every_tick(
            &goal
        ));
        harness.world.players[0].alive = false;
        assert!(!harness.run(|context| goal.can_use(context)));
        harness.world.players[0].alive = true;
        harness.world.peaceful = true;
        assert!(!harness.run(|context| goal.can_use(context)));
        harness.world.peaceful = false;
        harness.brain.target = None;
        assert!(!harness.run(|context| goal.can_use(context)));
    }

    #[test]
    fn starting_opens_the_lid_to_one_hundred_and_stopping_closes_it() {
        let mut goal = ShulkerAttackGoal::default();
        let mut harness = harness(Some(Vec3::new(5.5, 64.0, 0.5)), Mock::default());
        harness.run(|context| goal.start(context));
        assert_eq!(goal.attack_time(), 20);
        harness.run(|context| goal.stop(context));
        assert_eq!(peeks(&drain(&mut harness)), vec![100, 0]);
    }

    #[test]
    fn the_attack_goal_counts_down_and_shoots_when_the_cooldown_expires_then_waits_ten_to_one_hundred_ten_ticks()
     {
        let mut goal = ShulkerAttackGoal::default();
        let mut harness = harness(Some(Vec3::new(5.5, 64.0, 0.5)), Mock::default());
        harness.run(|context| goal.start(context));
        drain(&mut harness);
        for tick in 0..19 {
            harness.run(|context| goal.tick(context));
            assert!(drain(&mut harness).is_empty(), "tick {tick}");
        }
        let mut mirror = harness.brain.random.clone();
        harness.run(|context| goal.tick(context));
        let actions = drain(&mut harness);
        let roll = mirror.next_int(10);
        let pitch = (mirror.next_float() - mirror.next_float()) * 0.2 + 1.0;
        assert_eq!(goal.attack_time(), roll * 20 / 2 + 20);
        assert_eq!(
            actions,
            vec![MobAction::Shulker(ShulkerAction::Shoot {
                target: EntityId(2),
                axis: Axis::Y,
                pitch,
            })]
        );
        assert!((20..=110).contains(&goal.attack_time()));
    }

    #[test]
    fn the_attack_goal_shoots_along_the_axis_of_the_attach_face() {
        for (face, axis) in [
            (Face::Down, Axis::Y),
            (Face::Up, Axis::Y),
            (Face::North, Axis::Z),
            (Face::East, Axis::X),
        ] {
            let mut goal = ShulkerAttackGoal::default();
            let mut harness = harness(Some(Vec3::new(5.5, 64.0, 0.5)), Mock::default());
            harness.mob.species.shulker.attach_face = face;
            harness.run(|context| goal.start(context));
            drain(&mut harness);
            for _ in 0..20 {
                harness.run(|context| goal.tick(context));
            }
            let shot = drain(&mut harness);
            assert!(
                matches!(
                    shot[..],
                    [MobAction::Shulker(ShulkerAction::Shoot { axis: shot_axis, .. })] if shot_axis == axis
                ),
                "{face:?}"
            );
        }
    }

    #[test]
    fn a_target_twenty_blocks_away_is_dropped_instead_of_shot() {
        let mut goal = ShulkerAttackGoal::default();
        let mut harness = harness(Some(Vec3::new(20.5, 64.0, 0.5)), Mock::default());
        harness.run(|context| goal.start(context));
        drain(&mut harness);
        for _ in 0..25 {
            harness.run(|context| goal.tick(context));
        }
        assert!(
            drain(&mut harness)
                .iter()
                .all(|action| !matches!(action, MobAction::Shulker(ShulkerAction::Shoot { .. })))
        );
        assert_eq!(harness.brain.target_requests.len(), 1);
        assert_eq!(harness.brain.target_requests[0].to, None);
        let mut near = self::harness(Some(Vec3::new(19.9, 64.0, 0.5)), Mock::default());
        let mut goal = ShulkerAttackGoal::default();
        near.run(|context| goal.start(context));
        for _ in 0..20 {
            near.run(|context| goal.tick(context));
        }
        assert!(
            near.brain
                .actions
                .iter()
                .any(|action| matches!(action, MobAction::Shulker(ShulkerAction::Shoot { .. })))
        );
    }

    #[test]
    fn the_attack_goal_looks_at_the_target_eyes_with_full_speed() {
        let mut goal = ShulkerAttackGoal::default();
        let mut harness = harness(Some(Vec3::new(5.5, 64.0, 0.5)), Mock::default());
        harness.run(|context| goal.start(context));
        harness.run(|context| goal.tick(context));
        let eye = 64.0 + f64::from(EntityKind::Player.dimensions().eye_height);
        assert_eq!(harness.brain.look.wanted(), Vec3::new(5.5, eye, 0.5));
        assert!(harness.brain.look.is_looking_at_target());
    }

    #[test]
    fn a_peaceful_world_stops_the_attack_tick_from_counting_or_shooting() {
        let mut goal = ShulkerAttackGoal::default();
        let mut harness = harness(Some(Vec3::new(5.5, 64.0, 0.5)), Mock::default());
        harness.run(|context| goal.start(context));
        drain(&mut harness);
        harness.world.peaceful = true;
        for _ in 0..30 {
            harness.run(|context| goal.tick(context));
        }
        assert_eq!(goal.attack_time(), 20);
        assert!(drain(&mut harness).is_empty());
    }

    #[test]
    fn the_peek_goal_rolls_one_in_twenty_without_a_target_and_needs_a_spot_it_can_stay_at() {
        let seed = seed_where(|random| random.next_int(20) == 0);
        let mut goal = ShulkerPeekGoal::default();
        let mut harness = harness(None, Mock::default());
        harness.brain.random = crate::random::MobRandom::new(seed);
        assert!(harness.run(|context| goal.can_use(context)));
        harness.brain.random = crate::random::MobRandom::new(seed);
        harness.mob.species.shulker.can_stay = false;
        assert!(!harness.run(|context| goal.can_use(context)));
        harness.mob.species.shulker.can_stay = true;
        let miss = seed_where(|random| random.next_int(20) != 0);
        harness.brain.random = crate::random::MobRandom::new(miss);
        assert!(!harness.run(|context| goal.can_use(context)));
        let mut targeted = self::harness(Some(Vec3::new(5.5, 64.0, 0.5)), Mock::default());
        targeted.brain.random = crate::random::MobRandom::new(seed);
        assert!(!targeted.run(|context| goal.can_use(context)));
        assert_eq!(Goal::<AiContext<'_, Mock>>::flags(&goal), Flags::NONE);
        assert!(!Goal::<AiContext<'_, Mock>>::requires_update_every_tick(
            &goal
        ));
    }

    #[test]
    fn the_peek_goal_opens_to_thirty_for_one_to_three_seconds_in_half_rate_ticks() {
        let mut goal = ShulkerPeekGoal::default();
        let mut harness = harness(None, Mock::default());
        let mut mirror = harness.brain.random.clone();
        harness.run(|context| goal.start(context));
        let seconds = mirror.next_int(3) + 1;
        assert_eq!(goal.peek_time(), (seconds * 20 + 1) / 2);
        assert_eq!(peeks(&drain(&mut harness)), vec![30]);
        assert!(harness.run(|context| goal.can_continue_to_use(context)));
        for _ in 0..goal.peek_time() {
            harness.run(|context| goal.tick(context));
        }
        assert!(!harness.run(|context| goal.can_continue_to_use(context)));
        harness.run(|context| goal.stop(context));
        assert_eq!(peeks(&drain(&mut harness)), vec![0]);
    }

    #[test]
    fn the_peek_goal_keeps_the_lid_open_when_it_stops_with_a_target_and_never_continues_with_one() {
        let mut goal = ShulkerPeekGoal::default();
        let mut harness = harness(Some(Vec3::new(5.5, 64.0, 0.5)), Mock::default());
        harness.run(|context| goal.start(context));
        drain(&mut harness);
        assert!(!harness.run(|context| goal.can_continue_to_use(context)));
        harness.run(|context| goal.stop(context));
        assert!(drain(&mut harness).is_empty());
    }

    #[test]
    fn the_target_search_area_is_four_blocks_deep_along_the_attach_axis() {
        let mut harness = harness(None, Mock::default());
        let at = |harness: &Harness, face: Face| {
            let mut mob = harness.mob;
            mob.species.shulker.attach_face = face;
            target_search_area(&mob, 16.0)
        };
        let down = at(&harness, Face::Down);
        assert_eq!((down.min_x, down.max_x), (0.0 - 16.0, 1.0 + 16.0));
        assert_eq!((down.min_y, down.max_y), (64.0 - 4.0, 65.0 + 4.0));
        let north = at(&harness, Face::North);
        assert_eq!((north.min_z, north.max_z), (0.0 - 4.0, 1.0 + 4.0));
        assert_eq!((north.min_x, north.max_x), (0.0 - 16.0, 1.0 + 16.0));
        let west = at(&harness, Face::West);
        assert_eq!((west.min_x, west.max_x), (0.0 - 4.0, 1.0 + 4.0));
        assert_eq!((west.min_y, west.max_y), (64.0 - 16.0, 65.0 + 16.0));
        harness.mob.species.shulker.attach_face = Face::Up;
        assert_eq!(target_search_area(&harness.mob, 16.0), down);
    }

    #[test]
    fn the_nearest_target_goal_refuses_a_peaceful_world_and_finds_a_player_otherwise() {
        use crate::goals::NearestAttackableTargetGoal;
        let seed = seed_where(|random| random.next_int(5) == 0);
        let mut goal = NearestAttackableTargetGoal::shulker_nearest();
        let mut harness = harness(None, Mock::default());
        harness.world.players = vec![living(2, EntityKind::Player, Vec3::new(8.5, 64.0, 0.5))];
        harness.brain.random = crate::random::MobRandom::new(seed);
        assert!(harness.run(|context| goal.can_use(context)));
        harness.world.peaceful = true;
        harness.brain.random = crate::random::MobRandom::new(seed);
        assert!(!harness.run(|context| goal.can_use(context)));
    }

    #[test]
    fn the_defense_goal_needs_a_team_and_only_targets_enemies_in_the_axis_search_area() {
        use crate::goals::NearestAttackableTargetGoal;
        let seed = seed_where(|random| random.next_int(5) == 0);
        let mut goal = NearestAttackableTargetGoal::shulker_defense();
        let mut harness = harness(None, Mock::default());
        harness.world.others = vec![
            living(3, EntityKind::Zombie, Vec3::new(8.5, 64.0, 0.5)),
            living(4, EntityKind::Cow, Vec3::new(4.5, 64.0, 0.5)),
            living(5, EntityKind::Skeleton, Vec3::new(2.5, 70.0, 0.5)),
        ];
        harness.brain.random = crate::random::MobRandom::new(seed);
        assert!(!harness.run(|context| goal.can_use(context)));
        harness.mob.team = true;
        harness.brain.random = crate::random::MobRandom::new(seed);
        assert!(harness.run(|context| goal.can_use(context)));
        harness.run(|context| goal.start(context));
        assert_eq!(
            harness.brain.target_requests.last().map(|r| r.to),
            Some(Some(EntityId(3)))
        );
    }

    #[test]
    fn the_look_control_aims_the_head_round_the_attach_axis_and_never_pitches() {
        use crate::control::LookControl;
        use rspigot_domain::entity::EntityBase;
        use rspigot_domain::entity::living::LivingState;
        let mut base = EntityBase::new(EntityId(1), uuid::Uuid::from_u128(1), EntityKind::Shulker);
        base.position = Vec3::new(0.5, 64.0, 0.5);
        let mut living = LivingState::new(&mut base, 0.0);
        living.body_yaw = 0.0;
        let mut look = LookControl::default();
        look.set_shulker_face(Some(Face::Down));
        look.set_look_at(Vec3::new(0.5, 64.5, 10.5), 180.0, 180.0);
        look.tick_with(&mut base, &living, false, true);
        assert_eq!(base.head_yaw, 0.0);
        assert_eq!(base.pitch, 0.0);
        look.set_look_at(Vec3::new(-9.5, 64.5, 0.5), 180.0, 180.0);
        look.tick_with(&mut base, &living, true, true);
        assert!((base.head_yaw - 90.0).abs() < 1.0e-3, "{}", base.head_yaw);
        assert_eq!(base.pitch, 0.0);
        look.set_shulker_face(Some(Face::Up));
        base.head_yaw = 0.0;
        look.set_look_at(Vec3::new(0.5, 64.5, 10.5), 180.0, 180.0);
        look.tick_with(&mut base, &living, false, true);
        assert!(
            (base.head_yaw.abs() - 180.0).abs() < 1.0e-3,
            "{}",
            base.head_yaw
        );
        look.set_shulker_face(Some(Face::Down));
        base.head_yaw = 33.0;
        look.set_look_at(Vec3::new(0.5, base.eye_y(), 0.5), 180.0, 180.0);
        look.tick_with(&mut base, &living, false, true);
        assert_eq!(base.head_yaw, 33.0);
    }

    #[test]
    fn the_horizontal_look_goal_keeps_the_eye_height_of_the_shulker_and_looks_at_players_in_eight_blocks()
     {
        use crate::goals::LookAtPlayerGoal;
        let seed = seed_where(|random| random.next_float() < 0.02);
        let mut goal = LookAtPlayerGoal::horizontal(8.0, 0.02);
        let mut harness = harness(Some(Vec3::new(5.5, 70.0, 0.5)), Mock::default());
        harness.brain.random = crate::random::MobRandom::new(seed);
        assert!(harness.run(|context| goal.can_use(context)));
        harness.run(|context| goal.start(context));
        harness.run(|context| goal.tick(context));
        let eye = harness.mob.eye_y();
        assert_eq!(harness.brain.look.wanted(), Vec3::new(5.5, eye, 0.5));
        let mut far = self::harness(Some(Vec3::new(9.5, 64.0, 0.5)), Mock::default());
        far.brain.random = crate::random::MobRandom::new(seed);
        let mut goal = LookAtPlayerGoal::horizontal(8.0, 0.02);
        assert!(!far.run(|context| goal.can_use(context)));
    }
}
