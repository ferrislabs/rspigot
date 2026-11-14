use rspigot_domain::EntityId;
use rspigot_domain::mth;

use crate::action::MobAction;
use crate::brain::AiContext;
use crate::combat::eye_of;
use crate::world::AiWorld;

const LOOK_SPEED: f32 = 30.0;
const SEE_TIME_TO_STOP: i32 = 4;
const MIN_POWER: f32 = 0.1;
const MAX_POWER: f32 = 1.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RangedAttackGoal {
    speed_modifier: f64,
    attack_interval_min: i32,
    attack_interval_max: i32,
    attack_radius: f32,
    attack_radius_sqr: f32,
    attack_time: i32,
    see_time: i32,
    target: Option<EntityId>,
}

impl RangedAttackGoal {
    pub fn new(speed_modifier: f64, attack_interval: i32, attack_radius: f32) -> Self {
        Self::between(
            speed_modifier,
            attack_interval,
            attack_interval,
            attack_radius,
        )
    }

    pub fn between(
        speed_modifier: f64,
        attack_interval_min: i32,
        attack_interval_max: i32,
        attack_radius: f32,
    ) -> Self {
        Self {
            speed_modifier,
            attack_interval_min,
            attack_interval_max,
            attack_radius,
            attack_radius_sqr: attack_radius * attack_radius,
            attack_time: -1,
            see_time: 0,
            target: None,
        }
    }

    pub fn target(&self) -> Option<EntityId> {
        self.target
    }

    pub fn attack_time(&self) -> i32 {
        self.attack_time
    }

    pub(crate) fn can_use<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) -> bool {
        match context.target() {
            Some(target) if target.alive => {
                self.target = Some(target.id);
                true
            }
            _ => false,
        }
    }

    pub(crate) fn can_continue_to_use<W: AiWorld>(
        &mut self,
        context: &mut AiContext<'_, W>,
    ) -> bool {
        if self.can_use(context) {
            return true;
        }
        let alive = self
            .target
            .and_then(|id| context.world.living(id))
            .is_some_and(|target| target.alive);
        alive && !context.brain.navigation.is_done()
    }

    pub(crate) fn stop<W: AiWorld>(&mut self, _context: &mut AiContext<'_, W>) {
        self.target = None;
        self.see_time = 0;
        self.attack_time = -1;
    }

    pub(crate) fn tick<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        let Some(target) = self.target.and_then(|id| context.world.living(id)) else {
            return;
        };
        let distance = context.mob.distance_to_sqr(target.position);
        let world = context.world;
        let sees = context
            .brain
            .sensing
            .has_line_of_sight(world, context.mob, &target);
        if sees {
            self.see_time += 1;
        } else {
            self.see_time = 0;
        }
        if distance <= f64::from(self.attack_radius_sqr) && self.see_time > SEE_TIME_TO_STOP {
            context.stop_navigation();
        } else {
            context.move_to_entity(&target, self.speed_modifier);
        }
        context
            .brain
            .look
            .set_look_at(eye_of(&target), LOOK_SPEED, LOOK_SPEED);
        self.attack_time -= 1;
        if self.attack_time == 0 {
            if !sees {
                return;
            }
            let fraction = distance.sqrt() as f32 / self.attack_radius;
            let power = mth::clamp(fraction, MIN_POWER, MAX_POWER);
            context.act(MobAction::RangedAttack {
                target: target.id,
                power,
            });
            let span = (self.attack_interval_max - self.attack_interval_min) as f32;
            self.attack_time =
                mth::floor(f64::from(fraction * span + self.attack_interval_min as f32));
        } else if self.attack_time < 0 {
            let fraction = distance.sqrt() / f64::from(self.attack_radius);
            let min = f64::from(self.attack_interval_min);
            let max = f64::from(self.attack_interval_max);
            self.attack_time = mth::floor(mth::lerp(fraction, min, max));
        }
    }
}

#[cfg(test)]
mod tests {
    use rspigot_blocks::BlockKind;
    use rspigot_domain::{BlockPos, Vec3};
    use rspigot_entities::EntityKind;

    use super::*;
    use crate::mock::{Harness, Mock, living};

    fn harness(distance: f64) -> Harness {
        let world = Mock {
            others: vec![living(
                2,
                EntityKind::Zombie,
                Vec3::new(0.5 + distance, 64.0, 0.5),
            )],
            ..Mock::default()
        };
        let mut harness = Harness::new(EntityKind::SnowGolem, Vec3::new(0.5, 64.0, 0.5), world, 3);
        harness.brain.target = Some(EntityId(2));
        harness
    }

    fn walled(distance: f64) -> Harness {
        let mut harness = harness(distance);
        for y in 64..70 {
            for z in -1..3 {
                harness
                    .world
                    .blocks
                    .insert(BlockPos::new(2, y, z), BlockKind::Stone.default_state());
            }
        }
        harness
    }

    fn goal() -> RangedAttackGoal {
        RangedAttackGoal::new(1.25, 20, 10.0)
    }

    fn shots(harness: &mut Harness) -> Vec<(EntityId, f32)> {
        std::mem::take(&mut harness.brain.actions)
            .into_iter()
            .filter_map(|action| match action {
                MobAction::RangedAttack { target, power } => Some((target, power)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn it_needs_a_living_target() {
        let mut goal = goal();
        let mut none = Harness::new(
            EntityKind::SnowGolem,
            Vec3::new(0.5, 64.0, 0.5),
            Mock::default(),
            3,
        );
        assert!(!none.run(|context| goal.can_use(context)));
        let mut near = harness(6.0);
        assert!(near.run(|context| goal.can_use(context)));
        assert_eq!(goal.target(), Some(EntityId(2)));
        let mut dead = harness(6.0);
        dead.world.others[0].alive = false;
        let mut fresh = self::goal();
        assert!(!dead.run(|context| fresh.can_use(context)));
        assert_eq!(fresh.target(), None);
    }

    #[test]
    fn it_continues_while_it_can_use_or_a_living_target_is_still_being_walked_to() {
        let mut goal = goal();
        let mut harness = harness(6.0);
        assert!(harness.run(|context| goal.can_use(context)));
        assert!(harness.run(|context| goal.can_continue_to_use(context)));
        harness.brain.target = None;
        assert!(!harness.run(|context| goal.can_continue_to_use(context)));
        harness.run(|context| {
            context.move_to_entity(
                &living(2, EntityKind::Zombie, Vec3::new(8.5, 64.0, 0.5)),
                1.0,
            )
        });
        assert!(!harness.brain.navigation.is_done());
        assert!(harness.run(|context| goal.can_continue_to_use(context)));
        harness.world.others[0].alive = false;
        assert!(!harness.run(|context| goal.can_continue_to_use(context)));
    }

    #[test]
    fn stopping_forgets_the_target_and_resets_the_timers() {
        let mut goal = goal();
        let mut harness = harness(6.0);
        harness.run(|context| goal.can_use(context));
        harness.run(|context| goal.tick(context));
        assert_ne!(goal.attack_time(), -1);
        harness.run(|context| goal.stop(context));
        assert_eq!(goal.target(), None);
        assert_eq!(goal.attack_time(), -1);
        assert_eq!(goal.see_time, 0);
    }

    #[test]
    fn it_walks_to_a_far_target_and_halts_once_close_and_seen_for_five_ticks() {
        let mut goal = goal();
        let mut far = harness(30.0);
        far.run(|context| goal.can_use(context));
        for _ in 0..10 {
            far.run(|context| goal.tick(context));
        }
        assert!(!far.brain.navigation.is_done());
        let mut near = harness(6.0);
        let mut close = self::goal();
        near.run(|context| close.can_use(context));
        for _ in 0..4 {
            near.run(|context| close.tick(context));
            assert!(!near.brain.navigation.is_done());
        }
        near.run(|context| close.tick(context));
        assert!(near.brain.navigation.is_done());
    }

    #[test]
    fn it_keeps_walking_when_the_target_is_in_range_but_hidden() {
        let mut goal = goal();
        let mut hidden = walled(6.0);
        hidden.run(|context| goal.can_use(context));
        for _ in 0..10 {
            hidden.run(|context| goal.tick(context));
        }
        assert_eq!(goal.see_time, 0);
        assert!(!hidden.brain.navigation.is_done());
    }

    #[test]
    fn the_first_tick_arms_the_interval_then_the_shot_fires_when_it_reaches_zero() {
        let mut goal = goal();
        let mut harness = harness(5.0);
        harness.run(|context| goal.can_use(context));
        harness.run(|context| goal.tick(context));
        assert_eq!(goal.attack_time(), 20);
        for expected in (1..20).rev() {
            harness.run(|context| goal.tick(context));
            assert_eq!(goal.attack_time(), expected);
            assert!(shots(&mut harness).is_empty());
        }
        harness.run(|context| goal.tick(context));
        let fired = shots(&mut harness);
        assert_eq!(fired.len(), 1);
        assert_eq!(fired[0].0, EntityId(2));
        assert_eq!(goal.attack_time(), 20);
    }

    #[test]
    fn the_power_is_the_distance_over_the_radius_clamped_to_a_tenth_and_one() {
        for (distance, expected) in [(5.0, 0.5), (0.5, 0.1), (25.0, 1.0)] {
            let mut goal = goal();
            let mut harness = harness(distance);
            harness.run(|context| goal.can_use(context));
            goal.attack_time = 1;
            harness.run(|context| goal.tick(context));
            let fired = shots(&mut harness);
            assert_eq!(fired.len(), 1, "{distance}");
            assert!((fired[0].1 - expected).abs() < 1.0e-6, "{distance}");
        }
    }

    #[test]
    fn a_hidden_target_is_not_shot_at_and_the_timer_restarts_on_the_next_tick() {
        let mut goal = goal();
        let mut harness = walled(5.0);
        harness.run(|context| goal.can_use(context));
        goal.attack_time = 1;
        harness.run(|context| goal.tick(context));
        assert!(shots(&mut harness).is_empty());
        assert_eq!(goal.attack_time(), 0);
        harness.run(|context| goal.tick(context));
        assert_eq!(goal.attack_time(), 20);
    }

    #[test]
    fn the_interval_scales_between_the_minimum_and_the_maximum_with_the_distance() {
        let mut goal = RangedAttackGoal::between(1.0, 20, 60, 10.0);
        let mut harness = harness(5.0);
        harness.run(|context| goal.can_use(context));
        harness.run(|context| goal.tick(context));
        assert_eq!(goal.attack_time(), 40);
        goal.attack_time = 1;
        harness.run(|context| goal.tick(context));
        assert_eq!(shots(&mut harness).len(), 1);
        assert_eq!(goal.attack_time(), 40);
    }

    #[test]
    fn it_looks_at_the_target_eyes_with_a_thirty_degree_budget() {
        let mut goal = goal();
        let mut harness = harness(5.0);
        harness.run(|context| goal.can_use(context));
        harness.run(|context| goal.tick(context));
        assert!(harness.brain.look.is_looking_at_target());
        let eye = f64::from(EntityKind::Zombie.dimensions().eye_height);
        assert_eq!(harness.brain.look.wanted(), Vec3::new(5.5, 64.0 + eye, 0.5));
    }
}
