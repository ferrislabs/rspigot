use rspigot_blocks::StateFlags;
use rspigot_domain::{BlockPos, Vec3, mth};

use crate::brain::AiContext;
use crate::goal::{Flags, Goal};
use crate::random::RandomSource;
use crate::scan::closer_to_center_than;
use crate::targeting::{self, TargetingConditions};
use crate::world::AiWorld;

const RESTING_RANGE: f64 = 4.0;
const HEAD_TURN_ODDS: i32 = 200;
const HEAD_TURNS: i32 = 360;
const RETARGET_ODDS: i32 = 30;
const REACHED_DISTANCE: f64 = 2.0;
const SIDE_RANGE: i32 = 7;
const UP_RANGE: i32 = 6;
const DOWN_OFFSET: f64 = 2.0;
const HORIZONTAL_SPEED: f64 = 0.5;
const VERTICAL_SPEED: f64 = 0.7;
const VERTICAL_AIM: f64 = 0.1;
const ACCELERATION: f64 = 0.1;
const SLEEP_ODDS: i32 = 100;
const RAD_TO_DEG_F: f32 = 57.295_776;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BatFacts {
    pub resting: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatAction {
    ToggleSleep { awake: bool },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BatGoal {
    target: Option<BlockPos>,
}

impl BatGoal {
    pub const fn new() -> Self {
        Self { target: None }
    }

    pub fn target(&self) -> Option<BlockPos> {
        self.target
    }
}

fn conducts<W: AiWorld>(world: &W, pos: BlockPos) -> bool {
    world
        .state(pos)
        .is_some_and(|state| state.flags().contains(StateFlags::REDSTONE_CONDUCTOR))
}

fn toggle<W: AiWorld>(context: &mut AiContext<'_, W>, awake: bool) {
    context.act(crate::MobAction::Air(crate::AirAction::Bat(
        BatAction::ToggleSleep { awake },
    )));
}

impl<W: AiWorld> Goal<AiContext<'_, W>> for BatGoal {
    fn flags(&self) -> Flags {
        Flags::NONE
    }

    fn can_use(&mut self, _context: &mut AiContext<'_, W>) -> bool {
        true
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, context: &mut AiContext<'_, W>) {
        let pos = context.mob.block_pos();
        let above = pos.offset(0, 1, 0);
        if context.mob.species.bat.resting {
            if conducts(context.world, above) {
                if context.brain.random.next_int(HEAD_TURN_ODDS) == 0 {
                    let turn = context.brain.random.next_int(HEAD_TURNS);
                    context.body.head_yaw = turn as f32;
                }
                let origin = context.mob.position;
                let conditions = TargetingConditions::for_non_combat().range(RESTING_RANGE);
                if targeting::nearest_player(context, conditions, |_| true, origin).is_some() {
                    toggle(context, true);
                }
            } else {
                toggle(context, true);
            }
            return;
        }
        self.fly(context, above);
    }
}

impl BatGoal {
    fn fly<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>, above: BlockPos) {
        let min_y = context.world.min_y();
        if let Some(target) = self.target
            && (!context
                .world
                .state(target)
                .is_none_or(rspigot_blocks::StateId::is_air)
                || target.y <= min_y)
        {
            self.target = None;
        }
        let position = context.mob.position;
        let retarget = match self.target {
            None => true,
            Some(target) => {
                context.brain.random.next_int(RETARGET_ODDS) == 0
                    || closer_to_center_than(target, position, REACHED_DISTANCE)
            }
        };
        if retarget {
            let random = &mut context.brain.random;
            let x = position.x + f64::from(random.next_int(SIDE_RANGE))
                - f64::from(random.next_int(SIDE_RANGE));
            let y = position.y + f64::from(random.next_int(UP_RANGE)) - DOWN_OFFSET;
            let z = position.z + f64::from(random.next_int(SIDE_RANGE))
                - f64::from(random.next_int(SIDE_RANGE));
            self.target = Some(Vec3::new(x, y, z).block());
        }
        let Some(target) = self.target else {
            return;
        };
        let dx = f64::from(target.x) + 0.5 - position.x;
        let dy = f64::from(target.y) + VERTICAL_AIM - position.y;
        let dz = f64::from(target.z) + 0.5 - position.z;
        let velocity = context.body.velocity;
        let next = Vec3::new(
            velocity.x + (signum(dx) * HORIZONTAL_SPEED - velocity.x) * ACCELERATION,
            velocity.y + (signum(dy) * VERTICAL_SPEED - velocity.y) * ACCELERATION,
            velocity.z + (signum(dz) * HORIZONTAL_SPEED - velocity.z) * ACCELERATION,
        );
        context.body.velocity = next;
        let heading = (mth::atan2(next.z, next.x) as f32 * RAD_TO_DEG_F) - 90.0;
        let turn = mth::wrap_degrees(heading - context.body.yaw);
        context.body.yaw += turn;
        if context.brain.random.next_int(SLEEP_ODDS) == 0 && conducts(context.world, above) {
            toggle(context, false);
        }
    }
}

fn signum(value: f64) -> f64 {
    if value > 0.0 {
        1.0
    } else if value < 0.0 {
        -1.0
    } else {
        value
    }
}

#[cfg(test)]
mod tests {
    use rspigot_blocks::BlockKind;
    use rspigot_entities::EntityKind;

    use super::*;
    use crate::MobAction;
    use crate::mock::{Harness, Mock, living, seed_where};
    use crate::random::MobRandom;

    fn harness(resting: bool, player: Option<Vec3>, seed: i64) -> Harness {
        let world = Mock {
            players: player
                .map(|at| vec![living(2, EntityKind::Player, at)])
                .unwrap_or_default(),
            ..Mock::default()
        };
        let mut harness = Harness::new(EntityKind::Bat, Vec3::new(0.5, 70.0, 0.5), world, seed);
        harness.mob.species.bat.resting = resting;
        harness
    }

    fn roof(harness: &mut Harness) {
        harness
            .world
            .blocks
            .insert(BlockPos::new(0, 71, 0), BlockKind::Stone.default_state());
    }

    fn toggles(harness: &mut Harness) -> Vec<bool> {
        std::mem::take(&mut harness.brain.actions)
            .iter()
            .filter_map(|action| match action {
                MobAction::Air(crate::AirAction::Bat(BatAction::ToggleSleep { awake })) => {
                    Some(*awake)
                }
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_goal_always_runs_every_tick_without_locking_anything() {
        let mut goal = BatGoal::new();
        let mut harness = harness(false, None, 1);
        assert!(harness.run(|context| goal.can_use(context)));
        assert!(Goal::<AiContext<'_, Mock>>::requires_update_every_tick(
            &goal
        ));
        assert_eq!(Goal::<AiContext<'_, Mock>>::flags(&goal), Flags::NONE);
    }

    #[test]
    fn a_resting_bat_wakes_when_nothing_conducts_above_it() {
        let mut goal = BatGoal::new();
        let mut harness = harness(true, None, 1);
        harness.run(|context| goal.tick(context));
        assert_eq!(toggles(&mut harness), vec![true]);
    }

    #[test]
    fn a_resting_bat_under_a_roof_stays_until_a_player_comes_within_four_blocks() {
        let mut goal = BatGoal::new();
        let mut alone = harness(true, None, 1);
        roof(&mut alone);
        alone.run(|context| goal.tick(context));
        assert!(toggles(&mut alone).is_empty());
        let mut near = harness(true, Some(Vec3::new(3.5, 70.0, 0.5)), 1);
        roof(&mut near);
        near.run(|context| goal.tick(context));
        assert_eq!(toggles(&mut near), vec![true]);
        let mut far = harness(true, Some(Vec3::new(6.5, 70.0, 0.5)), 1);
        roof(&mut far);
        far.run(|context| goal.tick(context));
        assert!(toggles(&mut far).is_empty());
    }

    #[test]
    fn a_resting_bat_turns_its_head_once_in_two_hundred_ticks_by_a_random_angle() {
        let seed = seed_where(|random| random.next_int(200) == 0);
        let mut goal = BatGoal::new();
        let mut harness = harness(true, None, seed);
        roof(&mut harness);
        let mut mirror = MobRandom::new(seed);
        mirror.next_int(200);
        let angle = mirror.next_int(360);
        harness.run(|context| goal.tick(context));
        assert_eq!(harness.body.head_yaw, angle as f32);
        let miss = seed_where(|random| random.next_int(200) != 0);
        let mut still = self::harness(true, None, miss);
        roof(&mut still);
        let before = still.body.head_yaw;
        still.run(|context| goal.tick(context));
        assert_eq!(still.body.head_yaw, before);
    }

    #[test]
    fn a_flying_bat_picks_a_target_around_itself_and_blends_its_velocity_towards_it() {
        let mut goal = BatGoal::new();
        let mut harness = harness(false, None, 9);
        let mut mirror = MobRandom::new(9);
        let x = 0.5 + f64::from(mirror.next_int(7)) - f64::from(mirror.next_int(7));
        let y = 70.0 + f64::from(mirror.next_int(6)) - 2.0;
        let z = 0.5 + f64::from(mirror.next_int(7)) - f64::from(mirror.next_int(7));
        let target = Vec3::new(x, y, z).block();
        harness.run(|context| goal.tick(context));
        assert_eq!(goal.target(), Some(target));
        let dx = f64::from(target.x) + 0.5 - 0.5;
        let dy = f64::from(target.y) + 0.1 - 70.0;
        let dz = f64::from(target.z) + 0.5 - 0.5;
        let expected = Vec3::new(
            (signum(dx) * 0.5) * 0.1,
            (signum(dy) * 0.7) * 0.1,
            (signum(dz) * 0.5) * 0.1,
        );
        assert_eq!(harness.body.velocity, expected);
        let heading = (mth::atan2(expected.z, expected.x) as f32 * RAD_TO_DEG_F) - 90.0;
        assert_eq!(harness.body.yaw, mth::wrap_degrees(heading - 0.0));
    }

    #[test]
    fn a_blocked_or_reached_target_is_replaced_and_the_target_below_the_world_is_dropped() {
        let mut goal = BatGoal::new();
        let mut harness = harness(false, None, 9);
        goal.target = Some(BlockPos::new(8, 75, 8));
        harness
            .world
            .blocks
            .insert(BlockPos::new(8, 75, 8), BlockKind::Stone.default_state());
        let mut mirror = MobRandom::new(9);
        harness.run(|context| goal.tick(context));
        let x = 0.5 + f64::from(mirror.next_int(7)) - f64::from(mirror.next_int(7));
        assert_eq!(goal.target().map(|pos| pos.x), Some(x.floor() as i32));
        let mut reached = self::harness(false, None, 4);
        goal.target = Some(BlockPos::new(0, 70, 0));
        reached.run(|context| goal.tick(context));
        assert_ne!(goal.target(), Some(BlockPos::new(0, 70, 0)));
    }

    #[test]
    fn a_flying_bat_goes_to_sleep_one_tick_in_a_hundred_when_a_ceiling_conducts() {
        let mut goal = BatGoal::new();
        let sleeper = seed_where(|random| random.next_int(30) != 0 && random.next_int(100) == 0);
        let mut sleepy = self::harness(false, None, sleeper);
        roof(&mut sleepy);
        goal.target = Some(BlockPos::new(3, 72, 3));
        sleepy.run(|context| goal.tick(context));
        assert_eq!(toggles(&mut sleepy), vec![false]);
        let mut open = self::harness(false, None, sleeper);
        goal.target = Some(BlockPos::new(3, 72, 3));
        open.run(|context| goal.tick(context));
        assert!(toggles(&mut open).is_empty());
    }

    #[test]
    fn signum_keeps_zero() {
        assert_eq!(signum(0.0), 0.0);
        assert_eq!(signum(-3.0), -1.0);
        assert_eq!(signum(0.2), 1.0);
    }
}
