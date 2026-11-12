use rspigot_domain::{EntityId, Vec3};

use crate::action::MobAction;
use crate::brain::AiContext;
use crate::combat::eye_of;
use crate::goal::{Flag, Flags, Goal};
use crate::random::RandomSource;
use crate::world::{AiWorld, Living};

const MELEE_RANGE_SQR: f64 = 4.0;
const MELEE_INTERVAL: i32 = 20;
const CHARGE_TIME: i32 = 60;
const SHOT_INTERVAL: i32 = 6;
const COOLDOWN: i32 = 100;
const LAST_SHOT_STEP: i32 = 4;
const SPREAD_FACTOR: f64 = 2.297;
const SPREAD_SCALE: f64 = 0.5;
const SEEN_LIMIT: i32 = 5;
const LOOK_SPEED: f32 = 10.0;
pub const SHOOT_EVENT: i32 = 1018;

fn height_of(living: &Living) -> f64 {
    f64::from((living.bounds.max_y - living.bounds.min_y) as f32)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BlazeAction {
    SetCharged(bool),
    Fireball(Vec3),
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct BlazeAttackGoal {
    attack_step: i32,
    attack_time: i32,
    last_seen: i32,
}

impl BlazeAttackGoal {
    pub fn attack_step(&self) -> i32 {
        self.attack_step
    }

    pub fn attack_time(&self) -> i32 {
        self.attack_time
    }

    pub fn last_seen(&self) -> i32 {
        self.last_seen
    }

    fn target<W: AiWorld>(context: &AiContext<'_, W>) -> Option<Living> {
        context
            .brain
            .target
            .and_then(|id: EntityId| context.world.living(id))
    }

    fn move_to<W: AiWorld>(context: &mut AiContext<'_, W>, target: &Living) {
        context
            .brain
            .movement
            .set_wanted_position(target.position, 1.0);
    }

    fn shoot<W: AiWorld>(context: &mut AiContext<'_, W>, target: &Living, distance: f64) {
        let mob = context.mob;
        let dx = target.position.x - mob.position.x;
        let target_middle = target.position.y + height_of(target) * 0.5;
        let dy = target_middle - (mob.position.y + f64::from(mob.height) * 0.5);
        let dz = target.position.z - mob.position.z;
        let spread = distance.sqrt().sqrt() * SPREAD_SCALE;
        if !mob.silent {
            context.act(MobAction::LevelEvent {
                event: SHOOT_EVENT,
                pos: mob.block_pos(),
                data: 0,
            });
        }
        let x = context.brain.random.triangle(dx, SPREAD_FACTOR * spread);
        let z = context.brain.random.triangle(dz, SPREAD_FACTOR * spread);
        let direction = Vec3::new(x, dy, z).normalize();
        context.act(MobAction::Blaze(BlazeAction::Fireball(direction)));
    }
}

impl<W: AiWorld> Goal<AiContext<'_, W>> for BlazeAttackGoal {
    fn flags(&self) -> Flags {
        Flags::of(Flag::Move).with(Flag::Look)
    }

    fn can_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        context.target().is_some()
    }

    fn start(&mut self, _context: &mut AiContext<'_, W>) {
        self.attack_step = 0;
    }

    fn stop(&mut self, context: &mut AiContext<'_, W>) {
        context.act(MobAction::Blaze(BlazeAction::SetCharged(false)));
        self.last_seen = 0;
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, context: &mut AiContext<'_, W>) {
        self.attack_time -= 1;
        let Some(target) = Self::target(context) else {
            return;
        };
        let world = context.world;
        let sees = context
            .brain
            .sensing
            .has_line_of_sight(world, context.mob, &target);
        if sees {
            self.last_seen = 0;
        } else {
            self.last_seen += 1;
        }
        let distance = context.mob.distance_to_sqr(target.position);
        if distance < MELEE_RANGE_SQR {
            if !sees {
                return;
            }
            if self.attack_time <= 0 {
                self.attack_time = MELEE_INTERVAL;
                context.act(MobAction::Attack(target.id));
            }
            Self::move_to(context, &target);
            return;
        }
        let follow = context.mob.follow_range;
        if distance < follow * follow && sees {
            if self.attack_time <= 0 {
                self.attack_step += 1;
                if self.attack_step == 1 {
                    self.attack_time = CHARGE_TIME;
                    context.act(MobAction::Blaze(BlazeAction::SetCharged(true)));
                } else if self.attack_step <= LAST_SHOT_STEP {
                    self.attack_time = SHOT_INTERVAL;
                } else {
                    self.attack_time = COOLDOWN;
                    self.attack_step = 0;
                    context.act(MobAction::Blaze(BlazeAction::SetCharged(false)));
                }
                if self.attack_step > 1 {
                    Self::shoot(context, &target, distance);
                }
            }
            context
                .brain
                .look
                .set_look_at(eye_of(&target), LOOK_SPEED, LOOK_SPEED);
        } else if self.last_seen < SEEN_LIMIT {
            Self::move_to(context, &target);
        }
    }
}

#[cfg(test)]
mod tests {
    use rspigot_blocks::BlockKind;
    use rspigot_domain::BlockPos;
    use rspigot_entities::EntityKind;

    use super::*;
    use crate::mock::{Harness, Mock, living};

    const SEED: i64 = 11;

    fn harness(distance: f64) -> Harness {
        let world = Mock {
            players: vec![living(
                2,
                EntityKind::Player,
                Vec3::new(0.5 + distance, 64.0, 0.5),
            )],
            ..Mock::default()
        };
        let mut harness = Harness::new(EntityKind::Blaze, Vec3::new(0.5, 64.0, 0.5), world, SEED);
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

    fn drain(harness: &mut Harness) -> Vec<MobAction> {
        std::mem::take(&mut harness.brain.actions)
    }

    fn fireballs(actions: &[MobAction]) -> Vec<Vec3> {
        actions
            .iter()
            .filter_map(|action| match action {
                MobAction::Blaze(BlazeAction::Fireball(direction)) => Some(*direction),
                _ => None,
            })
            .collect()
    }

    fn charges(actions: &[MobAction]) -> Vec<bool> {
        actions
            .iter()
            .filter_map(|action| match action {
                MobAction::Blaze(BlazeAction::SetCharged(charged)) => Some(*charged),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn it_needs_a_living_attackable_target() {
        let mut goal = BlazeAttackGoal::default();
        let mut none = Harness::new(
            EntityKind::Blaze,
            Vec3::new(0.5, 64.0, 0.5),
            Mock::default(),
            SEED,
        );
        assert!(!none.run(|context| goal.can_use(context)));
        let mut near = harness(6.0);
        assert!(near.run(|context| goal.can_use(context)));
        let mut dead = harness(6.0);
        dead.world.players[0].alive = false;
        assert!(!dead.run(|context| goal.can_use(context)));
        let mut peaceful_ghast = Harness::new(
            EntityKind::Blaze,
            Vec3::new(0.5, 64.0, 0.5),
            Mock {
                others: vec![living(2, EntityKind::Ghast, Vec3::new(6.5, 64.0, 0.5))],
                ..Mock::default()
            },
            SEED,
        );
        peaceful_ghast.brain.target = Some(EntityId(2));
        assert!(!peaceful_ghast.run(|context| goal.can_use(context)));
    }

    #[test]
    fn it_claims_move_and_look_and_runs_every_tick() {
        let goal = BlazeAttackGoal::default();
        let flags = Goal::<AiContext<'_, Mock>>::flags(&goal);
        assert!(flags.contains(Flag::Move) && flags.contains(Flag::Look));
        assert!(!flags.contains(Flag::Jump));
        assert!(Goal::<AiContext<'_, Mock>>::requires_update_every_tick(
            &goal
        ));
    }

    #[test]
    fn starting_resets_the_step_and_stopping_discharges_and_forgets_the_sight() {
        let mut goal = BlazeAttackGoal {
            attack_step: 3,
            attack_time: 5,
            last_seen: 4,
        };
        let mut harness = harness(6.0);
        harness.run(|context| goal.start(context));
        assert_eq!(goal.attack_step(), 0);
        harness.run(|context| goal.stop(context));
        assert_eq!(goal.last_seen(), 0);
        assert_eq!(charges(&drain(&mut harness)), vec![false]);
    }

    #[test]
    fn up_close_it_bites_every_twenty_ticks_and_keeps_walking_into_the_target() {
        let mut goal = BlazeAttackGoal::default();
        let mut harness = harness(1.5);
        harness.run(|context| goal.tick(context));
        let actions = drain(&mut harness);
        assert_eq!(actions, vec![MobAction::Attack(EntityId(2))]);
        assert_eq!(goal.attack_time(), 20);
        assert!(harness.brain.movement.has_wanted());
        assert_eq!(harness.brain.movement.wanted(), Vec3::new(2.0, 64.0, 0.5));
        assert_eq!(harness.brain.movement.speed_modifier(), 1.0);
        for expected in (1..20).rev() {
            harness.run(|context| goal.tick(context));
            assert_eq!(goal.attack_time(), expected);
            assert!(drain(&mut harness).is_empty());
        }
        harness.run(|context| goal.tick(context));
        assert_eq!(goal.attack_time(), 20);
        assert_eq!(drain(&mut harness), vec![MobAction::Attack(EntityId(2))]);
    }

    #[test]
    fn up_close_but_unseen_it_does_nothing_at_all() {
        let mut goal = BlazeAttackGoal::default();
        let mut harness = harness(1.5);
        harness
            .world
            .blocks
            .insert(BlockPos::new(1, 65, 0), BlockKind::Stone.default_state());
        harness
            .world
            .blocks
            .insert(BlockPos::new(1, 66, 0), BlockKind::Stone.default_state());
        harness
            .world
            .blocks
            .insert(BlockPos::new(1, 65, 1), BlockKind::Stone.default_state());
        harness
            .world
            .blocks
            .insert(BlockPos::new(1, 66, 1), BlockKind::Stone.default_state());
        harness.run(|context| goal.tick(context));
        assert!(drain(&mut harness).is_empty());
        assert!(!harness.brain.movement.has_wanted());
        assert_eq!(goal.last_seen(), 1);
    }

    #[test]
    fn at_range_it_charges_for_sixty_ticks_then_fires_three_single_fireballs_and_cools_down() {
        let mut goal = BlazeAttackGoal::default();
        let mut harness = harness(10.0);
        harness.run(|context| goal.start(context));
        harness.run(|context| goal.tick(context));
        assert_eq!(goal.attack_step(), 1);
        assert_eq!(goal.attack_time(), 60);
        let first = drain(&mut harness);
        assert_eq!(charges(&first), vec![true]);
        assert!(fireballs(&first).is_empty());
        for expected in (1..60).rev() {
            harness.run(|context| goal.tick(context));
            assert_eq!(goal.attack_time(), expected);
            assert!(drain(&mut harness).is_empty());
        }
        let mut volleys = Vec::new();
        for step in 2..=4 {
            harness.run(|context| goal.tick(context));
            assert_eq!(goal.attack_step(), step);
            assert_eq!(goal.attack_time(), 6);
            let actions = drain(&mut harness);
            assert_eq!(fireballs(&actions).len(), 1, "step {step}");
            assert!(actions.contains(&MobAction::LevelEvent {
                event: 1018,
                pos: BlockPos::new(0, 64, 0),
                data: 0,
            }));
            volleys.push(actions);
            for _ in 0..5 {
                harness.run(|context| goal.tick(context));
                assert!(drain(&mut harness).is_empty());
            }
        }
        harness.run(|context| goal.tick(context));
        assert_eq!(goal.attack_step(), 0);
        assert_eq!(goal.attack_time(), 100);
        let last = drain(&mut harness);
        assert_eq!(charges(&last), vec![false]);
        assert!(fireballs(&last).is_empty());
    }

    #[test]
    fn the_fireball_aims_at_the_target_with_a_triangle_spread_widening_with_the_range() {
        let mut goal = BlazeAttackGoal::default();
        let mut harness = harness(16.0);
        goal.attack_step = 1;
        goal.attack_time = 1;
        let mut mirror = harness.brain.random.clone();
        harness.run(|context| goal.tick(context));
        let fired = fireballs(&drain(&mut harness));
        assert_eq!(fired.len(), 1);
        let spread = (16.0f64 * 16.0).sqrt().sqrt() * 0.5;
        let x = mirror.triangle(16.0, 2.297 * spread);
        let z = mirror.triangle(0.0, 2.297 * spread);
        let dy = (64.0 + f64::from(EntityKind::Player.dimensions().height) * 0.5)
            - (64.0 + f64::from(EntityKind::Blaze.dimensions().height) * 0.5);
        assert_eq!(fired[0], Vec3::new(x, dy, z).normalize());
    }

    #[test]
    fn a_silent_blaze_fires_without_the_shoot_sound() {
        let mut goal = BlazeAttackGoal::default();
        let mut harness = harness(10.0);
        harness.mob.silent = true;
        goal.attack_step = 1;
        goal.attack_time = 1;
        harness.run(|context| goal.tick(context));
        let actions = drain(&mut harness);
        assert_eq!(fireballs(&actions).len(), 1);
        assert!(
            !actions
                .iter()
                .any(|action| matches!(action, MobAction::LevelEvent { .. }))
        );
    }

    #[test]
    fn while_aiming_it_looks_at_the_target_eyes_at_ten_degrees() {
        let mut goal = BlazeAttackGoal::default();
        let mut harness = harness(10.0);
        harness.run(|context| goal.tick(context));
        assert!(harness.brain.look.is_looking_at_target());
        let eye = f64::from(EntityKind::Player.dimensions().eye_height);
        assert_eq!(
            harness.brain.look.wanted(),
            Vec3::new(10.5, 64.0 + eye, 0.5)
        );
        assert!(!harness.brain.movement.has_wanted());
    }

    #[test]
    fn beyond_its_follow_range_it_closes_in_while_the_target_was_seen_lately() {
        let mut goal = BlazeAttackGoal::default();
        let mut harness = harness(60.0);
        harness.run(|context| goal.tick(context));
        assert!(drain(&mut harness).is_empty());
        assert!(harness.brain.movement.has_wanted());
        assert_eq!(harness.brain.movement.wanted(), Vec3::new(60.5, 64.0, 0.5));
        assert_eq!(goal.attack_step(), 0);
    }

    #[test]
    fn a_target_hidden_for_five_ticks_stops_being_chased() {
        let mut goal = BlazeAttackGoal::default();
        let mut harness = walled(10.0);
        for tick in 1..=4 {
            harness.run(|context| goal.tick(context));
            assert_eq!(goal.last_seen(), tick);
            assert!(harness.brain.movement.has_wanted(), "tick {tick}");
            harness.brain.movement.set_wait();
        }
        harness.run(|context| goal.tick(context));
        assert_eq!(goal.last_seen(), 5);
        assert!(!harness.brain.movement.has_wanted());
        assert!(drain(&mut harness).is_empty());
    }
}
