use rspigot_domain::Vec3;
use rspigot_domain::spear;
use rspigot_items::ItemKind;

use crate::brain::AiContext;
use crate::combat::{eye_of, look_at_entity};
use crate::goal::{Flag, Flags, Goal, reduced_tick_delay};
use crate::random_pos::land_pos_away_between;
use crate::world::{AiWorld, Hand};

pub const SPEAR_LOOK_SPEED: f32 = 30.0;
pub const PASSENGER_EXTRA_DISTANCE: i32 = 2;
const MAX_FLEEING_TICKS: i32 = reduced_tick_delay(100);
const FLEE_MIN_BASE: i32 = 9;
const FLEE_MAX_BASE: i32 = 11;
const REPOSITION_MIN_BASE: i32 = 6;
const REPOSITION_MAX_BASE: i32 = 7;
const AWAY_VERTICAL: i32 = 7;

pub fn is_spear_item(item: rspigot_domain::ItemId) -> bool {
    item.kind()
        .is_some_and(|kind: ItemKind| spear::of(kind).is_some())
}

pub fn kinetic_use_duration(item: rspigot_domain::ItemId) -> i32 {
    item.kind()
        .and_then(spear::of)
        .map_or(0, |spear| spear.use_duration())
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpearUseState {
    engage_time: i32,
    fleeing_time: i32,
    away_pos: Option<Vec3>,
    done: bool,
}

impl Default for SpearUseState {
    fn default() -> Self {
        Self::new()
    }
}

impl SpearUseState {
    pub fn new() -> Self {
        Self {
            engage_time: -1,
            fleeing_time: -1,
            away_pos: None,
            done: false,
        }
    }

    pub fn not_engaged_yet(&self) -> bool {
        self.engage_time < 0
    }

    pub fn start_engagement(&mut self, spear_down_time: i32) {
        self.engage_time = spear_down_time;
    }

    pub fn tick_and_check_engagement(&mut self) -> bool {
        if self.engage_time > 0 {
            self.engage_time -= 1;
            if self.engage_time == 0 {
                return true;
            }
        }
        false
    }

    pub fn tick_and_check_fleeing(&mut self) -> bool {
        if self.fleeing_time > 0 {
            self.fleeing_time += 1;
            if self.fleeing_time > MAX_FLEEING_TICKS {
                self.done = true;
                return true;
            }
        }
        false
    }

    pub fn engage_time(&self) -> i32 {
        self.engage_time
    }

    pub fn fleeing_time(&self) -> i32 {
        self.fleeing_time
    }

    pub fn away_pos(&self) -> Option<Vec3> {
        self.away_pos
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpearUseGoal {
    speed_when_charging: f64,
    speed_when_repositioning: f64,
    approach_distance_sq: f32,
    target_in_range_radius_sq: f32,
    state: Option<SpearUseState>,
}

impl SpearUseGoal {
    pub fn new(
        speed_when_charging: f64,
        speed_when_repositioning: f64,
        approach_distance: f32,
        target_in_range_radius: f32,
    ) -> Self {
        Self {
            speed_when_charging,
            speed_when_repositioning,
            approach_distance_sq: approach_distance * approach_distance,
            target_in_range_radius_sq: target_in_range_radius * target_in_range_radius,
            state: None,
        }
    }

    pub fn state(&self) -> Option<&SpearUseState> {
        self.state.as_ref()
    }

    #[cfg(test)]
    fn state_mut(&mut self) -> Option<&mut SpearUseState> {
        self.state.as_mut()
    }

    fn able_to_attack<W: AiWorld>(context: &AiContext<'_, W>) -> bool {
        context.target().is_some() && context.mob.main_hand.is_some_and(is_spear_item)
    }

    fn use_duration<W: AiWorld>(context: &AiContext<'_, W>) -> i32 {
        reduced_tick_delay(context.mob.main_hand.map_or(0, kinetic_use_duration))
    }

    fn away<W: AiWorld>(
        context: &mut AiContext<'_, W>,
        min_base: i32,
        max_base: i32,
        passenger_extra: i32,
        distance: f64,
        from: Vec3,
        floor_max: f64,
    ) -> Option<Vec3> {
        let min = (f64::from(min_base + passenger_extra) - distance).max(0.0);
        let max = (f64::from(max_base + passenger_extra) - distance).max(floor_max);
        land_pos_away_between(context, min, max, AWAY_VERTICAL, from)
    }
}

impl<W: AiWorld> Goal<AiContext<'_, W>> for SpearUseGoal {
    fn flags(&self) -> Flags {
        Flags::of(Flag::Move).with(Flag::Look)
    }

    fn can_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        Self::able_to_attack(context) && context.body.using_item.is_none()
    }

    fn can_continue_to_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        self.state.is_some_and(|state| !state.done) && Self::able_to_attack(context)
    }

    fn start(&mut self, context: &mut AiContext<'_, W>) {
        context.body.aggressive = true;
        self.state = Some(SpearUseState::new());
    }

    fn stop(&mut self, context: &mut AiContext<'_, W>) {
        context.stop_navigation();
        context.body.aggressive = false;
        self.state = None;
        context.body.using_item = None;
    }

    fn tick(&mut self, context: &mut AiContext<'_, W>) {
        if self.state.is_none() {
            return;
        }
        let Some(target) = context.target() else {
            return;
        };
        let distance_sqr = context.mob.distance_to_sqr(target.position);
        let extra = if context.mob.passenger {
            PASSENGER_EXTRA_DISTANCE
        } else {
            0
        };
        look_at_entity(
            context.mob,
            context.body,
            &target,
            SPEAR_LOOK_SPEED,
            SPEAR_LOOK_SPEED,
        );
        context
            .brain
            .look
            .set_look_at(eye_of(&target), SPEAR_LOOK_SPEED, SPEAR_LOOK_SPEED);
        let duration = Self::use_duration(context);
        let Some(mut state) = self.state else {
            return;
        };
        if state.not_engaged_yet() {
            if f64::from(self.approach_distance_sq) < distance_sqr {
                context.move_to_entity(&target, self.speed_when_repositioning);
                return;
            }
            state.start_engagement(duration);
            context.body.using_item = Some(Hand::Main);
        }
        if state.tick_and_check_engagement() {
            context.body.using_item = None;
            let distance = distance_sqr.sqrt();
            state.away_pos = Self::away(
                context,
                FLEE_MIN_BASE,
                FLEE_MAX_BASE,
                extra,
                distance,
                target.position,
                1.0,
            );
            state.fleeing_time = 1;
        }
        if state.tick_and_check_fleeing() {
            self.state = Some(state);
            return;
        }
        match state.away_pos {
            None => {
                context.move_to_entity(&target, self.speed_when_charging);
                if f64::from(self.target_in_range_radius_sq) <= distance_sqr
                    && !context.brain.navigation.is_done()
                {
                    self.state = Some(state);
                    return;
                }
                let distance = distance_sqr.sqrt();
                let min = f64::from(REPOSITION_MIN_BASE + extra) - distance;
                let max = f64::from(REPOSITION_MAX_BASE + extra) - distance;
                state.away_pos =
                    land_pos_away_between(context, min, max, AWAY_VERTICAL, target.position);
            }
            Some(away) => {
                context.move_to_pos(away, self.speed_when_repositioning);
                if context.brain.navigation.is_done() {
                    if state.fleeing_time > 0 {
                        state.done = true;
                    } else {
                        state.away_pos = None;
                    }
                }
            }
        }
        self.state = Some(state);
    }
}

#[cfg(test)]
mod tests {
    use rspigot_domain::{EntityId, Vec3};
    use rspigot_entities::EntityKind;
    use rspigot_items::ItemKind;

    use super::*;
    use crate::goal::Goal;
    use crate::mock::{Harness, Mock, living};
    use crate::world::Hand;

    fn zombie(distance: f64) -> Harness {
        let world = Mock {
            others: vec![living(
                2,
                EntityKind::Player,
                Vec3::new(0.5 + distance, 64.0, 0.5),
            )],
            ..Mock::default()
        };
        let mut harness = Harness::new(EntityKind::Zombie, Vec3::new(0.5, 64.0, 0.5), world, 17);
        harness.mob.main_hand = Some(ItemKind::IronSpear.id());
        harness.brain.target = Some(EntityId(2));
        harness
    }

    fn goal() -> SpearUseGoal {
        SpearUseGoal::new(1.0, 1.0, 10.0, 2.0)
    }

    #[test]
    fn it_needs_a_target_and_a_spear_and_a_free_hand() {
        let mut goal = goal();
        let mut harness = zombie(5.0);
        assert!(harness.run(|context| goal.can_use(context)));
        harness.mob.main_hand = Some(ItemKind::IronSword.id());
        assert!(!harness.run(|context| goal.can_use(context)));
        let mut using = zombie(5.0);
        using.body.using_item = Some(Hand::Main);
        assert!(!using.run(|context| goal.can_use(context)));
        let mut idle = zombie(5.0);
        idle.brain.target = None;
        assert!(!idle.run(|context| goal.can_use(context)));
    }

    #[test]
    fn starting_makes_the_mob_aggressive_and_stopping_calms_it() {
        let mut goal = goal();
        let mut harness = zombie(5.0);
        harness.run(|context| goal.start(context));
        assert!(harness.body.aggressive);
        assert!(harness.run(|context| goal.can_continue_to_use(context)));
        harness.body.using_item = Some(Hand::Main);
        harness.run(|context| goal.stop(context));
        assert!(!harness.body.aggressive);
        assert!(harness.body.using_item.is_none());
        assert!(!harness.run(|context| goal.can_continue_to_use(context)));
    }

    #[test]
    fn a_far_target_is_approached_without_raising_the_spear() {
        let mut goal = goal();
        let mut harness = zombie(14.0);
        harness.run(|context| goal.start(context));
        harness.run(|context| goal.tick(context));
        assert!(!harness.brain.navigation.is_done());
        assert!(harness.body.using_item.is_none());
        assert!(goal.state().unwrap().not_engaged_yet());
    }

    #[test]
    fn inside_the_approach_distance_the_spear_is_raised_for_the_halved_use_duration() {
        let mut goal = goal();
        let mut harness = zombie(8.0);
        harness.run(|context| goal.start(context));
        harness.run(|context| goal.tick(context));
        assert_eq!(harness.body.using_item, Some(Hand::Main));
        assert_eq!(goal.state().unwrap().engage_time(), 118);
    }

    #[test]
    fn the_charge_runs_at_the_target_until_the_engage_time_is_spent() {
        let mut goal = goal();
        let mut harness = zombie(8.0);
        harness.run(|context| goal.start(context));
        for _ in 0..118 {
            harness.run(|context| goal.tick(context));
        }
        assert_eq!(goal.state().unwrap().engage_time(), 1);
        assert_eq!(harness.body.using_item, Some(Hand::Main));
        assert!(goal.state().unwrap().away_pos().is_none());
    }

    fn seeded(distance: f64, seed: i64) -> Harness {
        let mut harness = zombie(distance);
        harness.brain.random = crate::random::MobRandom::new(seed);
        harness
    }

    #[test]
    fn when_the_charge_ends_the_spear_is_lowered_and_the_mob_flees() {
        let mut found = 0;
        for seed in 0..30 {
            let mut goal = goal();
            let mut harness = seeded(8.0, seed);
            harness.run(|context| goal.start(context));
            for _ in 0..119 {
                harness.run(|context| goal.tick(context));
            }
            assert!(harness.body.using_item.is_none());
            assert!(goal.state().unwrap().fleeing_time() >= 1);
            if let Some(away) = goal.state().unwrap().away_pos() {
                found += 1;
                assert!(away.x < 0.5 + 8.0);
            }
        }
        assert!(found > 0);
    }

    #[test]
    fn fleeing_for_too_long_ends_the_goal() {
        let mut goal = goal();
        let mut harness = zombie(8.0);
        harness.run(|context| goal.start(context));
        for _ in 0..119 {
            harness.run(|context| goal.tick(context));
        }
        for _ in 0..60 {
            harness.run(|context| goal.tick(context));
        }
        assert!(!harness.run(|context| goal.can_continue_to_use(context)));
    }

    #[test]
    fn a_passenger_keeps_two_extra_blocks_of_distance() {
        let mut flights = Vec::new();
        for seed in 0..40 {
            let mut goal = goal();
            let mut harness = zombie(3.0);
            harness.brain.random = crate::random::MobRandom::new(seed);
            harness.mob.passenger = true;
            harness.run(|context| goal.start(context));
            goal.state_mut().unwrap().start_engagement(1);
            harness.body.using_item = Some(Hand::Main);
            harness.run(|context| goal.tick(context));
            if let Some(away) = goal.state().unwrap().away_pos() {
                flights.push(
                    (away - Vec3::new(0.5, 64.0, 0.5))
                        .horizontal_distance_sqr()
                        .sqrt(),
                );
            }
        }
        assert!(!flights.is_empty());
        assert!(flights.iter().all(|distance| *distance >= 7.0));
    }

    #[test]
    fn a_target_inside_the_contact_radius_makes_the_mob_pick_a_pass_through_point() {
        let mut found = 0;
        for seed in 0..30 {
            let mut goal = goal();
            let mut harness = seeded(1.5, seed);
            harness.run(|context| goal.start(context));
            harness.run(|context| goal.tick(context));
            assert_eq!(goal.state().unwrap().fleeing_time(), -1);
            if goal.state().unwrap().away_pos().is_some() {
                found += 1;
            }
        }
        assert!(found > 0);
    }

    #[test]
    fn a_target_outside_the_contact_radius_is_simply_charged_at() {
        let mut goal = goal();
        let mut harness = zombie(5.0);
        harness.run(|context| goal.start(context));
        harness.run(|context| goal.tick(context));
        assert!(goal.state().unwrap().away_pos().is_none());
        assert!(!harness.brain.navigation.is_done());
    }

    #[test]
    fn the_goal_claims_move_and_look() {
        let goal = goal();
        let flags = Goal::<crate::brain::AiContext<'_, Mock>>::flags(&goal);
        assert!(flags.contains(crate::goal::Flag::Move));
        assert!(flags.contains(crate::goal::Flag::Look));
    }
}
