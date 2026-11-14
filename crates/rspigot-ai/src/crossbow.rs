use rspigot_domain::EntityId;
use rspigot_items::ItemKind;

use crate::action::{MobAction, TargetReason};
use crate::brain::AiContext;
use crate::combat::{eye_of, weapon_hand};
use crate::goal::{Flag, Flags, Goal};
use crate::raider::RaiderAction;
use crate::random::RandomSource;
use crate::targeting::TargetingConditions;
use crate::world::{AiWorld, Hand, TargetKind, inflate};

pub const DEFAULT_CHARGE_TICKS: i32 = 25;
const LOOK_SPEED: f32 = 30.0;
const SEE_TIME_TO_STOP: i32 = 4;
const PATH_DELAY_MIN: i32 = 20;
const PATH_DELAY_SPAN: i32 = 21;
const CHARGED_DELAY_BASE: i32 = 20;
const CHARGED_DELAY_SPREAD: i32 = 20;
const CROSSBOW_POWER: f32 = 1.0;
const CHARGING_SPEED_FACTOR: f64 = 0.5;
const SHOUT_REACH: f64 = 8.0;
const SHOUT_SOUND_ODDS: i32 = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CrossbowFacts {
    pub charge_ticks: i32,
}

impl Default for CrossbowFacts {
    fn default() -> Self {
        Self {
            charge_ticks: DEFAULT_CHARGE_TICKS,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrossbowAction {
    SetCharging(bool),
    ClearCharged,
    Release { hand: Hand, ticks: i32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrossbowState {
    Uncharged,
    Charging,
    Charged,
    ReadyToAttack,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RangedCrossbowAttackGoal {
    speed_modifier: f64,
    attack_radius_sqr: f32,
    state: CrossbowState,
    attack_delay: i32,
    see_time: i32,
    update_path_delay: i32,
}

impl RangedCrossbowAttackGoal {
    pub fn new(speed_modifier: f64, attack_radius: f32) -> Self {
        Self {
            speed_modifier,
            attack_radius_sqr: attack_radius * attack_radius,
            state: CrossbowState::Uncharged,
            attack_delay: 0,
            see_time: 0,
            update_path_delay: 0,
        }
    }

    pub fn state(&self) -> CrossbowState {
        self.state
    }

    pub fn attack_delay(&self) -> i32 {
        self.attack_delay
    }

    pub fn see_time(&self) -> i32 {
        self.see_time
    }

    fn is_valid_target<W: AiWorld>(context: &AiContext<'_, W>) -> bool {
        context.target().is_some_and(|target| target.alive)
    }

    fn is_holding_crossbow<W: AiWorld>(context: &AiContext<'_, W>) -> bool {
        context.mob.is_holding(ItemKind::Crossbow.id())
    }

    fn can_use<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) -> bool {
        Self::is_valid_target(context) && Self::is_holding_crossbow(context)
    }

    fn can_continue_to_use<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) -> bool {
        if !Self::is_valid_target(context) {
            return false;
        }
        if !self.can_use(context) && context.brain.navigation.is_done() {
            return false;
        }
        Self::is_holding_crossbow(context)
    }

    fn stop<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        context.body.aggressive = false;
        context.set_target(None, TargetReason::Unknown);
        self.see_time = 0;
        if context.body.using_item.is_some() {
            context.body.using_item = None;
            context.act(MobAction::Crossbow(CrossbowAction::SetCharging(false)));
        }
    }

    fn can_run(&self) -> bool {
        self.state == CrossbowState::Uncharged
    }

    fn tick<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        let Some(target) = context.target() else {
            return;
        };
        let world = context.world;
        let sees = context
            .brain
            .sensing
            .has_line_of_sight(world, context.mob, &target);
        if sees != (self.see_time > 0) {
            self.see_time = 0;
        }
        if sees {
            self.see_time += 1;
        } else {
            self.see_time -= 1;
        }
        let distance = context.mob.distance_to_sqr(target.position);
        let in_position = (distance <= f64::from(self.attack_radius_sqr)
            && self.see_time > SEE_TIME_TO_STOP)
            || self.attack_delay != 0;
        if in_position {
            self.update_path_delay = 0;
            context.stop_navigation();
        } else {
            self.update_path_delay -= 1;
            if self.update_path_delay < 1 {
                let speed = if self.can_run() {
                    self.speed_modifier
                } else {
                    self.speed_modifier * CHARGING_SPEED_FACTOR
                };
                context.move_to_entity(&target, speed);
                self.update_path_delay =
                    PATH_DELAY_MIN + context.brain.random.next_int(PATH_DELAY_SPAN);
            }
        }
        context
            .brain
            .look
            .set_look_at(eye_of(&target), LOOK_SPEED, LOOK_SPEED);
        match self.state {
            CrossbowState::Uncharged => {
                if in_position {
                    if context.body.using_item.is_none() {
                        context.body.using_item =
                            Some(weapon_hand(context.mob, ItemKind::Crossbow));
                    }
                    self.state = CrossbowState::Charging;
                    context.act(MobAction::Crossbow(CrossbowAction::SetCharging(true)));
                }
            }
            CrossbowState::Charging => {
                let using = context.body.using_item;
                if using.is_none() {
                    self.state = CrossbowState::Uncharged;
                }
                let (ticks, duration) = match using {
                    Some(_) => (
                        context.mob.ticks_using_item,
                        context.mob.species.crossbow.charge_ticks,
                    ),
                    None => (0, DEFAULT_CHARGE_TICKS),
                };
                if duration <= ticks
                    && let Some(hand) = using
                {
                    context.body.using_item = None;
                    context.act(MobAction::Crossbow(CrossbowAction::Release { hand, ticks }));
                    self.state = CrossbowState::Charged;
                    self.attack_delay =
                        CHARGED_DELAY_BASE + context.brain.random.next_int(CHARGED_DELAY_SPREAD);
                    context.act(MobAction::Crossbow(CrossbowAction::SetCharging(false)));
                }
            }
            CrossbowState::Charged => {
                self.attack_delay -= 1;
                if self.attack_delay == 0 {
                    self.state = CrossbowState::ReadyToAttack;
                }
            }
            CrossbowState::ReadyToAttack => {
                if sees {
                    context.act(MobAction::RangedAttack {
                        target: target.id,
                        power: CROSSBOW_POWER,
                    });
                    self.state = CrossbowState::Uncharged;
                }
            }
        }
    }
}

impl<W: AiWorld> Goal<AiContext<'_, W>> for RangedCrossbowAttackGoal {
    fn flags(&self) -> Flags {
        Flags::of(Flag::Move).with(Flag::Look)
    }

    fn can_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        Self::can_use(self, context)
    }

    fn can_continue_to_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        Self::can_continue_to_use(self, context)
    }

    fn stop(&mut self, context: &mut AiContext<'_, W>) {
        Self::stop(self, context);
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, context: &mut AiContext<'_, W>) {
        Self::tick(self, context);
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HoldGroundAttackGoal {
    hostile_radius: f32,
    hostile_radius_sqr: f32,
}

impl HoldGroundAttackGoal {
    pub fn new(hostile_radius: f32) -> Self {
        Self {
            hostile_radius,
            hostile_radius_sqr: hostile_radius * hostile_radius,
        }
    }

    fn can_use<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) -> bool {
        let attacker = context
            .mob
            .last_hurt_by_mob
            .and_then(|id| context.world.living(id));
        context.mob.species.raider.patrolling
            && context.target().is_some()
            && !context.body.aggressive
            && attacker.is_none_or(|attacker| !attacker.player)
    }

    fn shout<W: AiWorld>(
        &self,
        context: &mut AiContext<'_, W>,
        target: EntityId,
        aggressive: bool,
    ) {
        let conditions = TargetingConditions::for_non_combat()
            .range(f64::from(self.hostile_radius))
            .ignore_line_of_sight()
            .ignore_invisibility_testing();
        let area = inflate(&context.mob.bounds(), SHOUT_REACH, SHOUT_REACH, SHOUT_REACH);
        let world = context.world;
        let raiders = world.living_of(TargetKind::Class("Raider"), &area);
        for raider in raiders {
            if conditions.test(context, &raider, |_| true) {
                context.act(MobAction::Raider(RaiderAction::ShoutTarget {
                    mob: raider.id,
                    target,
                    aggressive,
                }));
            }
        }
    }

    fn start<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        context.stop_navigation();
        if let Some(target) = context.target() {
            self.shout(context, target.id, false);
        }
    }

    fn stop<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        if let Some(target) = context.target() {
            self.shout(context, target.id, true);
            context.body.aggressive = true;
        }
    }

    fn tick<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        let Some(target) = context.target() else {
            return;
        };
        let distance = context.mob.distance_to_sqr(target.position);
        if distance <= f64::from(self.hostile_radius_sqr) {
            context.body.aggressive = true;
        } else {
            context
                .brain
                .look
                .set_look_at(eye_of(&target), LOOK_SPEED, LOOK_SPEED);
            if context.brain.random.next_int(SHOUT_SOUND_ODDS) == 0 {
                context.act(MobAction::AmbientSound);
            }
        }
    }
}

impl<W: AiWorld> Goal<AiContext<'_, W>> for HoldGroundAttackGoal {
    fn flags(&self) -> Flags {
        Flags::of(Flag::Move).with(Flag::Look)
    }

    fn can_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        Self::can_use(self, context)
    }

    fn start(&mut self, context: &mut AiContext<'_, W>) {
        Self::start(self, context);
    }

    fn stop(&mut self, context: &mut AiContext<'_, W>) {
        Self::stop(self, context);
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, context: &mut AiContext<'_, W>) {
        Self::tick(self, context);
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CrossbowGoal {
    RangedAttack(RangedCrossbowAttackGoal),
    HoldGround(HoldGroundAttackGoal),
}

impl<W: AiWorld> Goal<AiContext<'_, W>> for CrossbowGoal {
    fn flags(&self) -> Flags {
        Flags::of(Flag::Move).with(Flag::Look)
    }

    fn can_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        match self {
            Self::RangedAttack(goal) => RangedCrossbowAttackGoal::can_use(goal, context),
            Self::HoldGround(goal) => HoldGroundAttackGoal::can_use(goal, context),
        }
    }

    fn can_continue_to_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        match self {
            Self::RangedAttack(goal) => {
                RangedCrossbowAttackGoal::can_continue_to_use(goal, context)
            }
            Self::HoldGround(goal) => HoldGroundAttackGoal::can_use(goal, context),
        }
    }

    fn start(&mut self, context: &mut AiContext<'_, W>) {
        if let Self::HoldGround(goal) = self {
            HoldGroundAttackGoal::start(goal, context);
        }
    }

    fn stop(&mut self, context: &mut AiContext<'_, W>) {
        match self {
            Self::RangedAttack(goal) => RangedCrossbowAttackGoal::stop(goal, context),
            Self::HoldGround(goal) => HoldGroundAttackGoal::stop(goal, context),
        }
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, context: &mut AiContext<'_, W>) {
        match self {
            Self::RangedAttack(goal) => RangedCrossbowAttackGoal::tick(goal, context),
            Self::HoldGround(goal) => HoldGroundAttackGoal::tick(goal, context),
        }
    }
}

#[cfg(test)]
mod tests {
    use rspigot_blocks::BlockKind;
    use rspigot_domain::{BlockPos, Vec3};
    use rspigot_entities::EntityKind;

    use super::*;
    use crate::brain::MobAi;
    use crate::mock::{Harness, Mock, living};
    use crate::random::MobRandom;
    use crate::registry::GoalSettings;

    const SEED: i64 = 17;

    fn pillager(distance: f64) -> Harness {
        let world = Mock {
            others: vec![living(
                2,
                EntityKind::Zombie,
                Vec3::new(0.5 + distance, 64.0, 0.5),
            )],
            ..Mock::default()
        };
        let mut harness =
            Harness::new(EntityKind::Pillager, Vec3::new(0.5, 64.0, 0.5), world, SEED);
        harness.mob.main_hand = Some(ItemKind::Crossbow.id());
        harness.brain.target = Some(EntityId(2));
        harness
    }

    fn walled(distance: f64) -> Harness {
        let mut harness = pillager(distance);
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

    fn goal() -> RangedCrossbowAttackGoal {
        RangedCrossbowAttackGoal::new(1.0, 8.0)
    }

    fn crossbow_actions(harness: &mut Harness) -> Vec<CrossbowAction> {
        std::mem::take(&mut harness.brain.actions)
            .into_iter()
            .filter_map(|action| match action {
                MobAction::Crossbow(action) => Some(action),
                _ => None,
            })
            .collect()
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

    fn hold_position(harness: &mut Harness, goal: &mut RangedCrossbowAttackGoal) {
        for _ in 0..5 {
            harness.run(|context| goal.tick(context));
        }
    }

    #[test]
    fn it_needs_a_living_target_and_a_crossbow_in_either_hand() {
        let mut goal = goal();
        let mut harness = pillager(5.0);
        assert!(harness.run(|context| goal.can_use(context)));
        harness.mob.main_hand = None;
        assert!(!harness.run(|context| goal.can_use(context)));
        harness.mob.off_hand = Some(ItemKind::Crossbow.id());
        assert!(harness.run(|context| goal.can_use(context)));
        harness.mob.main_hand = Some(ItemKind::Bow.id());
        assert!(harness.run(|context| goal.can_use(context)));
        harness.mob.off_hand = None;
        assert!(!harness.run(|context| goal.can_use(context)));
        let mut dead = pillager(5.0);
        dead.world.others[0].alive = false;
        assert!(!dead.run(|context| goal.can_use(context)));
        let mut none = pillager(5.0);
        none.brain.target = None;
        assert!(!none.run(|context| goal.can_use(context)));
    }

    #[test]
    fn it_continues_while_it_can_use_or_a_path_is_still_walked_and_the_crossbow_is_held() {
        let mut goal = goal();
        let mut harness = pillager(5.0);
        assert!(harness.run(|context| goal.can_continue_to_use(context)));
        harness.brain.target = None;
        assert!(!harness.run(|context| goal.can_continue_to_use(context)));
        harness.brain.target = Some(EntityId(2));
        harness.mob.main_hand = None;
        assert!(!harness.run(|context| goal.can_continue_to_use(context)));
        harness.mob.main_hand = Some(ItemKind::Crossbow.id());
        harness.world.others[0].alive = false;
        assert!(!harness.run(|context| goal.can_continue_to_use(context)));
    }

    #[test]
    fn it_walks_to_a_far_target_with_a_one_to_two_second_path_delay() {
        let mut goal = goal();
        let mut harness = pillager(30.0);
        let mut mirror = MobRandom::new(SEED);
        harness.run(|context| goal.tick(context));
        assert!(!harness.brain.navigation.is_done());
        assert_eq!(goal.update_path_delay, 20 + mirror.next_int(21));
        assert_eq!(goal.state(), CrossbowState::Uncharged);
        assert!(crossbow_actions(&mut harness).is_empty());
    }

    #[test]
    fn the_path_is_recomputed_only_when_the_delay_runs_out() {
        let mut goal = goal();
        let mut harness = pillager(30.0);
        harness.run(|context| goal.tick(context));
        let delay = goal.update_path_delay;
        harness.run(|context| goal.tick(context));
        assert_eq!(goal.update_path_delay, delay - 1);
    }

    #[test]
    fn it_halts_and_starts_charging_once_close_and_seen_for_five_ticks() {
        let mut goal = goal();
        let mut harness = pillager(5.0);
        for _ in 0..4 {
            harness.run(|context| goal.tick(context));
            assert_eq!(goal.state(), CrossbowState::Uncharged);
            assert!(harness.body.using_item.is_none());
        }
        harness.run(|context| goal.tick(context));
        assert_eq!(goal.see_time(), 5);
        assert!(harness.brain.navigation.is_done());
        assert_eq!(goal.update_path_delay, 0);
        assert_eq!(goal.state(), CrossbowState::Charging);
        assert_eq!(harness.body.using_item, Some(Hand::Main));
        assert_eq!(
            crossbow_actions(&mut harness),
            vec![CrossbowAction::SetCharging(true)]
        );
    }

    #[test]
    fn it_charges_the_crossbow_held_in_the_off_hand_when_the_main_hand_is_empty() {
        let mut goal = goal();
        let mut harness = pillager(5.0);
        harness.mob.main_hand = None;
        harness.mob.off_hand = Some(ItemKind::Crossbow.id());
        hold_position(&mut harness, &mut goal);
        assert_eq!(harness.body.using_item, Some(Hand::Off));
    }

    #[test]
    fn a_hidden_target_in_range_is_still_approached() {
        let mut goal = goal();
        let mut hidden = walled(5.0);
        for _ in 0..10 {
            hidden.run(|context| goal.tick(context));
        }
        assert!(goal.see_time() < 0);
        assert_eq!(goal.state(), CrossbowState::Uncharged);
        assert!(!hidden.brain.navigation.is_done());
    }

    #[test]
    fn a_far_target_is_never_charged_at_even_when_seen() {
        let mut goal = goal();
        let mut harness = pillager(12.0);
        for _ in 0..8 {
            harness.run(|context| goal.tick(context));
        }
        assert!(goal.see_time() > 4);
        assert_eq!(goal.state(), CrossbowState::Uncharged);
        assert!(harness.body.using_item.is_none());
    }

    #[test]
    fn the_sight_streak_resets_when_the_sight_changes() {
        let mut goal = goal();
        let mut harness = pillager(5.0);
        goal.see_time = -3;
        harness.run(|context| goal.tick(context));
        assert_eq!(goal.see_time(), 1);
        goal.see_time = 3;
        let mut hidden = walled(5.0);
        hidden.run(|context| goal.tick(context));
        assert_eq!(goal.see_time(), -1);
    }

    #[test]
    fn it_releases_the_crossbow_when_the_charge_time_is_reached() {
        let mut goal = goal();
        let mut harness = pillager(5.0);
        hold_position(&mut harness, &mut goal);
        harness.brain.actions.clear();
        harness.mob.using_item = Some(Hand::Main);
        harness.mob.ticks_using_item = 24;
        harness.run(|context| goal.tick(context));
        assert_eq!(goal.state(), CrossbowState::Charging);
        assert!(crossbow_actions(&mut harness).is_empty());
        harness.mob.ticks_using_item = 25;
        let mut probe = harness.brain.random.clone();
        let expected = 20 + probe.next_int(20);
        harness.run(|context| goal.tick(context));
        assert_eq!(goal.state(), CrossbowState::Charged);
        assert_eq!(goal.attack_delay(), expected);
        assert!((20..40).contains(&goal.attack_delay()));
        assert!(harness.body.using_item.is_none());
        assert_eq!(
            crossbow_actions(&mut harness),
            vec![
                CrossbowAction::Release {
                    hand: Hand::Main,
                    ticks: 25
                },
                CrossbowAction::SetCharging(false),
            ]
        );
    }

    #[test]
    fn a_quick_charge_crossbow_is_released_earlier() {
        let mut goal = goal();
        let mut harness = pillager(5.0);
        hold_position(&mut harness, &mut goal);
        harness.brain.actions.clear();
        harness.mob.using_item = Some(Hand::Main);
        harness.mob.species.crossbow.charge_ticks = 10;
        harness.mob.ticks_using_item = 10;
        harness.run(|context| goal.tick(context));
        assert_eq!(goal.state(), CrossbowState::Charged);
    }

    #[test]
    fn losing_the_item_mid_charge_falls_back_to_uncharged() {
        let mut goal = goal();
        let mut harness = pillager(5.0);
        hold_position(&mut harness, &mut goal);
        harness.brain.actions.clear();
        harness.body.using_item = None;
        harness.mob.ticks_using_item = 0;
        harness.run(|context| goal.tick(context));
        assert_eq!(goal.state(), CrossbowState::Uncharged);
        assert!(crossbow_actions(&mut harness).is_empty());
    }

    #[test]
    fn a_charged_crossbow_waits_out_its_delay_then_fires_only_when_the_target_is_seen() {
        let mut goal = goal();
        let mut harness = pillager(5.0);
        goal.state = CrossbowState::Charged;
        goal.attack_delay = 3;
        for expected in [2, 1] {
            harness.run(|context| goal.tick(context));
            assert_eq!(goal.attack_delay(), expected);
            assert_eq!(goal.state(), CrossbowState::Charged);
        }
        harness.run(|context| goal.tick(context));
        assert_eq!(goal.state(), CrossbowState::ReadyToAttack);
        assert!(shots(&mut harness).is_empty());
        harness.run(|context| goal.tick(context));
        assert_eq!(shots(&mut harness), vec![(EntityId(2), 1.0)]);
        assert_eq!(goal.state(), CrossbowState::Uncharged);
    }

    #[test]
    fn a_ready_crossbow_does_not_fire_at_a_hidden_target() {
        let mut goal = goal();
        let mut hidden = walled(5.0);
        goal.state = CrossbowState::ReadyToAttack;
        hidden.run(|context| goal.tick(context));
        assert!(shots(&mut hidden).is_empty());
        assert_eq!(goal.state(), CrossbowState::ReadyToAttack);
    }

    #[test]
    fn the_target_is_approached_at_half_speed_while_the_crossbow_is_not_uncharged() {
        let mut slow = goal();
        slow.state = CrossbowState::Charged;
        slow.attack_delay = 0;
        assert!(!slow.can_run());
        let mut fast = goal();
        assert!(fast.can_run());
        fast.state = CrossbowState::Uncharged;
        let mut harness = pillager(30.0);
        harness.run(|context| slow.tick(context));
        let slow_speed = harness.brain.navigation.speed_modifier();
        let mut other = pillager(30.0);
        other.run(|context| fast.tick(context));
        assert_eq!(slow_speed, 0.5 * other.brain.navigation.speed_modifier());
    }

    #[test]
    fn it_does_not_walk_while_the_charged_delay_is_pending() {
        let mut goal = goal();
        goal.state = CrossbowState::Charged;
        goal.attack_delay = 10;
        let mut harness = pillager(30.0);
        harness.run(|context| goal.tick(context));
        assert!(harness.brain.navigation.is_done());
    }

    #[test]
    fn it_looks_at_the_target_eyes_with_a_thirty_degree_budget() {
        let mut goal = goal();
        let mut harness = pillager(5.0);
        harness.run(|context| goal.tick(context));
        assert!(harness.brain.look.is_looking_at_target());
        let eye = f64::from(EntityKind::Zombie.dimensions().eye_height);
        assert_eq!(harness.brain.look.wanted(), Vec3::new(5.5, 64.0 + eye, 0.5));
    }

    #[test]
    fn stopping_calms_the_mob_forgets_the_target_and_cancels_a_charge_in_progress() {
        let mut goal = goal();
        let mut harness = pillager(5.0);
        hold_position(&mut harness, &mut goal);
        harness.brain.actions.clear();
        harness.body.aggressive = true;
        harness.run(|context| goal.stop(context));
        assert!(!harness.body.aggressive);
        assert_eq!(goal.see_time(), 0);
        assert!(harness.body.using_item.is_none());
        assert_eq!(harness.brain.target_requests.len(), 1);
        assert_eq!(harness.brain.target_requests[0].to, None);
        assert_eq!(
            crossbow_actions(&mut harness),
            vec![CrossbowAction::SetCharging(false)]
        );
        assert_eq!(goal.state(), CrossbowState::Charging);
    }

    #[test]
    fn stopping_without_using_an_item_sends_no_crossbow_action() {
        let mut goal = goal();
        let mut harness = pillager(5.0);
        goal.see_time = 9;
        harness.run(|context| goal.stop(context));
        assert!(crossbow_actions(&mut harness).is_empty());
        assert_eq!(goal.see_time(), 0);
    }

    #[test]
    fn the_goal_claims_move_and_look_and_updates_every_tick() {
        let goal = MobGoal::Crossbow(CrossbowGoal::RangedAttack(goal()));
        let flags = Goal::<AiContext<'_, Mock>>::flags(&goal);
        assert!(flags.contains(Flag::Move) && flags.contains(Flag::Look));
        assert!(!flags.contains(Flag::Jump));
        assert!(Goal::<AiContext<'_, Mock>>::requires_update_every_tick(
            &goal
        ));
    }

    use crate::goals::MobGoal;

    fn patrolling() -> Harness {
        let mut harness = pillager(15.0);
        harness.mob.species.raider.patrolling = true;
        harness
    }

    fn hold() -> HoldGroundAttackGoal {
        HoldGroundAttackGoal::new(10.0)
    }

    #[test]
    fn holding_the_ground_needs_a_patrol_a_target_and_a_calm_mob() {
        let mut goal = hold();
        let mut harness = patrolling();
        assert!(harness.run(|context| goal.can_use(context)));
        harness.mob.species.raider.patrolling = false;
        assert!(!harness.run(|context| goal.can_use(context)));
        harness.mob.species.raider.patrolling = true;
        harness.brain.target = None;
        assert!(!harness.run(|context| goal.can_use(context)));
        harness.brain.target = Some(EntityId(2));
        harness.body.aggressive = true;
        assert!(!harness.run(|context| goal.can_use(context)));
    }

    #[test]
    fn holding_the_ground_is_refused_after_a_player_hurt_the_mob_but_not_another_mob() {
        let mut goal = hold();
        let mut harness = patrolling();
        harness
            .world
            .players
            .push(living(5, EntityKind::Player, Vec3::new(3.5, 64.0, 0.5)));
        harness.mob.last_hurt_by_mob = Some(EntityId(5));
        assert!(!harness.run(|context| goal.can_use(context)));
        harness.mob.last_hurt_by_mob = Some(EntityId(2));
        assert!(harness.run(|context| goal.can_use(context)));
        harness.mob.last_hurt_by_mob = Some(EntityId(99));
        assert!(harness.run(|context| goal.can_use(context)));
    }

    fn raiders_around() -> Harness {
        let mut harness = patrolling();
        harness
            .world
            .others
            .push(living(7, EntityKind::Pillager, Vec3::new(3.5, 64.0, 0.5)));
        harness
            .world
            .others
            .push(living(8, EntityKind::Witch, Vec3::new(-6.5, 64.0, 0.5)));
        harness
            .world
            .others
            .push(living(9, EntityKind::Pillager, Vec3::new(14.5, 64.0, 0.5)));
        harness
            .world
            .others
            .push(living(10, EntityKind::Zombie, Vec3::new(1.5, 64.0, 0.5)));
        harness
    }

    fn shouts(harness: &mut Harness) -> Vec<(EntityId, EntityId, bool)> {
        std::mem::take(&mut harness.brain.actions)
            .into_iter()
            .filter_map(|action| match action {
                MobAction::Raider(RaiderAction::ShoutTarget {
                    mob,
                    target,
                    aggressive,
                }) => Some((mob, target, aggressive)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn starting_to_hold_the_ground_stops_walking_and_points_nearby_raiders_at_the_target() {
        let mut goal = hold();
        let mut harness = raiders_around();
        harness.run(|context| {
            context.move_to_entity(
                &living(2, EntityKind::Zombie, Vec3::new(15.5, 64.0, 0.5)),
                1.0,
            )
        });
        assert!(!harness.brain.navigation.is_done());
        harness.run(|context| goal.start(context));
        assert!(harness.brain.navigation.is_done());
        assert_eq!(
            shouts(&mut harness),
            vec![
                (EntityId(7), EntityId(2), false),
                (EntityId(8), EntityId(2), false)
            ]
        );
    }

    #[test]
    fn stopping_the_hold_makes_the_raiders_and_the_mob_aggressive() {
        let mut goal = hold();
        let mut harness = raiders_around();
        harness.run(|context| goal.stop(context));
        assert!(harness.body.aggressive);
        assert_eq!(
            shouts(&mut harness),
            vec![
                (EntityId(7), EntityId(2), true),
                (EntityId(8), EntityId(2), true)
            ]
        );
        let mut idle = raiders_around();
        idle.brain.target = None;
        idle.run(|context| goal.stop(context));
        assert!(!idle.body.aggressive);
        assert!(shouts(&mut idle).is_empty());
    }

    #[test]
    fn holding_the_ground_turns_aggressive_inside_the_hostile_radius() {
        let mut goal = hold();
        let mut near = pillager(9.0);
        near.mob.species.raider.patrolling = true;
        near.run(|context| goal.tick(context));
        assert!(near.body.aggressive);
        assert!(!near.brain.look.is_looking_at_target());
    }

    #[test]
    fn holding_the_ground_watches_a_distant_target_and_sometimes_calls_out() {
        let mut goal = hold();
        let mut far = patrolling();
        far.run(|context| goal.tick(context));
        assert!(!far.body.aggressive);
        assert!(far.brain.look.is_looking_at_target());
        let mut mirror = MobRandom::new(SEED);
        let called = mirror.next_int(50) == 0;
        let sounds = far
            .brain
            .actions
            .iter()
            .filter(|action| matches!(action, MobAction::AmbientSound))
            .count();
        assert_eq!(sounds, usize::from(called));
    }

    #[test]
    fn the_hold_goal_claims_move_and_look_and_ticks_every_tick() {
        let goal = MobGoal::Crossbow(CrossbowGoal::HoldGround(hold()));
        let flags = Goal::<AiContext<'_, Mock>>::flags(&goal);
        assert!(flags.contains(Flag::Move) && flags.contains(Flag::Look));
        assert!(Goal::<AiContext<'_, Mock>>::requires_update_every_tick(
            &goal
        ));
    }

    #[test]
    fn the_pillager_registers_the_raider_goals_then_its_own_with_the_vanilla_priorities() {
        use crate::goals::{LookAtPlayerGoal, RandomStrollGoal};
        use crate::raider::{
            LongDistancePatrolGoal, ObtainRaidLeaderBannerGoal, PathfindToRaidGoal,
            RaiderCelebrationGoal, RaiderGoal, RaiderMoveThroughVillageGoal,
        };
        let ai = MobAi::new(EntityKind::Pillager, 1, GoalSettings::default());
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
                        ObtainRaidLeaderBannerGoal::default()
                    ))
                ),
                (
                    3,
                    MobGoal::Raider(RaiderGoal::PathfindToRaid(PathfindToRaidGoal::default()))
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
                (0, MobGoal::Float(crate::goals::FloatGoal)),
                (
                    1,
                    MobGoal::AvoidEntity(crate::AvoidEntityGoal::new(
                        TargetKind::Class("Creaking"),
                        8.0,
                        1.0,
                        1.2
                    ))
                ),
                (
                    2,
                    MobGoal::Crossbow(CrossbowGoal::HoldGround(HoldGroundAttackGoal::new(10.0)))
                ),
                (
                    3,
                    MobGoal::Crossbow(CrossbowGoal::RangedAttack(RangedCrossbowAttackGoal::new(
                        1.0, 8.0
                    )))
                ),
                (8, MobGoal::Stroll(RandomStrollGoal::new(0.6))),
                (
                    9,
                    MobGoal::LookAtPlayer(LookAtPlayerGoal::with_probability(15.0, 1.0))
                ),
                (10, MobGoal::LookAtPlayer(LookAtPlayerGoal::at_mobs(15.0))),
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
                    MobGoal::HurtByTarget(crate::HurtByTargetGoal::ignoring_damage_from_alerting(
                        &["Raider"]
                    ))
                ),
                (
                    2,
                    MobGoal::NearestAttackableTarget(crate::NearestAttackableTargetGoal::new(
                        TargetKind::Player,
                        true
                    ))
                ),
                (
                    3,
                    MobGoal::NearestAttackableTarget(crate::NearestAttackableTargetGoal::new(
                        TargetKind::AbstractVillager,
                        false
                    ))
                ),
                (
                    3,
                    MobGoal::NearestAttackableTarget(crate::NearestAttackableTargetGoal::new(
                        TargetKind::IronGolem,
                        true
                    ))
                ),
            ]
        );
        assert!(ai.can_float());
    }

    #[test]
    fn a_pillager_neither_retaliates_against_a_raider_nor_alerts_a_non_pillager() {
        let world = Mock {
            others: vec![living(9, EntityKind::Witch, Vec3::new(2.5, 64.0, 0.5))],
            ..Mock::default()
        };
        let mut harness =
            Harness::new(EntityKind::Pillager, Vec3::new(0.5, 64.0, 0.5), world, SEED);
        harness.mob.last_hurt_by_mob = Some(EntityId(9));
        harness.mob.last_hurt_by_mob_at = 5;
        let mut goal = crate::HurtByTargetGoal::ignoring_damage_from_alerting(&["Raider"]);
        assert!(!harness.run(|context| goal.can_use(context)));
        let mut zombie = Harness::new(
            EntityKind::Pillager,
            Vec3::new(0.5, 64.0, 0.5),
            Mock {
                others: vec![living(9, EntityKind::Zombie, Vec3::new(2.5, 64.0, 0.5))],
                ..Mock::default()
            },
            SEED,
        );
        zombie.mob.last_hurt_by_mob = Some(EntityId(9));
        zombie.mob.last_hurt_by_mob_at = 5;
        assert!(zombie.run(|context| goal.can_use(context)));
    }
}
