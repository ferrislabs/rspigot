use rspigot_domain::entity::EntityBase;
use rspigot_domain::entity::living::LivingState;
use rspigot_domain::{BlockPos, EntityId, Vec3, mth};

use crate::action::{MobAction, TargetReason};
use crate::brain::AiContext;
use crate::combat::TargetMemory;
use crate::goal::{Flag, Flags, Goal, reduced_tick_delay};
use crate::random::RandomSource;
use crate::targeting::{self, TargetingConditions};
use crate::world::{AiWorld, Living};

pub const CHARGE_SOUND: &str = "minecraft:entity.vex.charge";
const RANDOM_MOVE_ATTEMPTS: i32 = 3;
const RANDOM_MOVE_SPEED: f64 = 0.25;
const RANDOM_MOVE_Y_ROT: f32 = 180.0;
const RANDOM_MOVE_X_ROT: f32 = 20.0;
const CHARGE_SPEED: f64 = 1.0;
const CHARGE_MIN_DISTANCE_SQR: f64 = 4.0;
const CHARGE_RETARGET_DISTANCE_SQR: f64 = 9.0;
const MOVE_ACCELERATION: f64 = 0.05;
const MOVE_BRAKE: f64 = 0.5;
const RAD_TO_DEG_F: f32 = 57.295_776;
const SPAWN_RANGE_SIDE: i32 = 15;
const SPAWN_RANGE_UP: i32 = 11;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct VexFacts {
    pub bound_origin: Option<BlockPos>,
    pub charging: bool,
    pub owner_target: Option<EntityId>,
    pub has_owner_target: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VexAction {
    SetCharging(bool),
}

fn target_of<W: AiWorld>(context: &AiContext<'_, W>) -> Option<Living> {
    context.brain.target.and_then(|id| context.world.living(id))
}

pub fn tick_move_control(
    base: &mut EntityBase,
    living: &mut LivingState,
    wanted: Vec3,
    speed_modifier: f64,
    target: Option<Vec3>,
) -> bool {
    let delta = wanted - base.position;
    let distance = delta.length();
    let dimensions = base.dimensions;
    let size = (f64::from(dimensions.width) * 2.0 + f64::from(dimensions.height)) / 3.0;
    if size <= distance {
        base.velocity = base.velocity + delta.scale(speed_modifier * MOVE_ACCELERATION / distance);
        let yaw = match target {
            None => -(mth::atan2(base.velocity.x, base.velocity.z) as f32) * RAD_TO_DEG_F,
            Some(target) => {
                let dx = target.x - base.position.x;
                let dz = target.z - base.position.z;
                -(mth::atan2(dx, dz) as f32) * RAD_TO_DEG_F
            }
        };
        base.set_yaw(yaw);
        living.body_yaw = base.yaw;
        true
    } else {
        base.velocity = base.velocity.scale(MOVE_BRAKE);
        false
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ChargeAttackGoal;

impl<W: AiWorld> Goal<AiContext<'_, W>> for ChargeAttackGoal {
    fn flags(&self) -> Flags {
        Flags::of(Flag::Move)
    }

    fn can_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        let Some(target) = target_of(context).filter(|target| target.alive) else {
            return false;
        };
        if context.brain.movement.has_wanted() {
            return false;
        }
        let bound = reduced_tick_delay(7);
        context.brain.random.next_int(bound) == 0
            && context.mob.distance_to_sqr(target.position) > CHARGE_MIN_DISTANCE_SQR
    }

    fn can_continue_to_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        context.brain.movement.has_wanted()
            && context.body.vex.charging
            && target_of(context).is_some_and(|target| target.alive)
    }

    fn start(&mut self, context: &mut AiContext<'_, W>) {
        if let Some(target) = target_of(context) {
            let eye = Vec3::new(target.position.x, target.eye_y(), target.position.z);
            context
                .brain
                .movement
                .set_wanted_position(eye, CHARGE_SPEED);
        }
        context.body.vex.charging = true;
        context.act(MobAction::Air(crate::AirAction::Vex(
            VexAction::SetCharging(true),
        )));
        context.act(MobAction::Sound {
            sound: CHARGE_SOUND,
            volume: 1.0,
            pitch: 1.0,
        });
    }

    fn stop(&mut self, context: &mut AiContext<'_, W>) {
        context.body.vex.charging = false;
        context.act(MobAction::Air(crate::AirAction::Vex(
            VexAction::SetCharging(false),
        )));
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, context: &mut AiContext<'_, W>) {
        let Some(target) = target_of(context) else {
            return;
        };
        if context.mob.bounds().intersects(&target.bounds) {
            context.act(MobAction::Attack(target.id));
            context.body.vex.charging = false;
            context.act(MobAction::Air(crate::AirAction::Vex(
                VexAction::SetCharging(false),
            )));
        } else if context.mob.distance_to_sqr(target.position) < CHARGE_RETARGET_DISTANCE_SQR {
            let eye = Vec3::new(target.position.x, target.eye_y(), target.position.z);
            context
                .brain
                .movement
                .set_wanted_position(eye, CHARGE_SPEED);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RandomMoveGoal;

impl<W: AiWorld> Goal<AiContext<'_, W>> for RandomMoveGoal {
    fn flags(&self) -> Flags {
        Flags::of(Flag::Move)
    }

    fn can_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        !context.brain.movement.has_wanted()
            && context.brain.random.next_int(reduced_tick_delay(7)) == 0
    }

    fn can_continue_to_use(&mut self, _context: &mut AiContext<'_, W>) -> bool {
        false
    }

    fn tick(&mut self, context: &mut AiContext<'_, W>) {
        let origin = context
            .mob
            .species
            .vex
            .bound_origin
            .unwrap_or_else(|| context.mob.block_pos());
        for _ in 0..RANDOM_MOVE_ATTEMPTS {
            let random = &mut context.brain.random;
            let dx = random.next_int(SPAWN_RANGE_SIDE) - 7;
            let dy = random.next_int(SPAWN_RANGE_UP) - 5;
            let dz = random.next_int(SPAWN_RANGE_SIDE) - 7;
            let pos = origin.offset(dx, dy, dz);
            if !context
                .world
                .state(pos)
                .is_none_or(rspigot_blocks::StateId::is_air)
            {
                continue;
            }
            let center = Vec3::new(
                f64::from(pos.x) + 0.5,
                f64::from(pos.y) + 0.5,
                f64::from(pos.z) + 0.5,
            );
            context
                .brain
                .movement
                .set_wanted_position(center, RANDOM_MOVE_SPEED);
            if context.brain.target.is_none() {
                context
                    .brain
                    .look
                    .set_look_at(center, RANDOM_MOVE_Y_ROT, RANDOM_MOVE_X_ROT);
            }
            break;
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CopyOwnerTargetGoal {
    memory: TargetMemory,
}

impl CopyOwnerTargetGoal {
    pub const fn new() -> Self {
        Self {
            memory: TargetMemory::new(false),
        }
    }

    fn owner_target<W: AiWorld>(context: &AiContext<'_, W>) -> Option<Living> {
        let facts = context.mob.species.vex;
        facts
            .owner_target
            .filter(|_| facts.has_owner_target)
            .and_then(|id| context.world.living(id))
    }
}

impl Default for CopyOwnerTargetGoal {
    fn default() -> Self {
        Self::new()
    }
}

impl<W: AiWorld> Goal<AiContext<'_, W>> for CopyOwnerTargetGoal {
    fn flags(&self) -> Flags {
        Flags::of(Flag::Target)
    }

    fn can_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        Self::owner_target(context).is_some_and(|target| {
            let conditions = TargetingConditions::for_non_combat()
                .ignore_line_of_sight()
                .ignore_invisibility_testing();
            targeting::goal_can_attack(context, &target, conditions)
        })
    }

    fn can_continue_to_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        self.memory.can_continue_to_use(context)
    }

    fn start(&mut self, context: &mut AiContext<'_, W>) {
        let target = Self::owner_target(context);
        context.set_target(target, TargetReason::OwnerAttackedTarget);
        self.memory.start();
    }

    fn stop(&mut self, context: &mut AiContext<'_, W>) {
        self.memory.stop(context);
    }
}

#[cfg(test)]
mod tests {
    use rspigot_blocks::BlockKind;
    use rspigot_entities::EntityKind;

    use super::*;
    use crate::mock::{Harness, Mock, first_draw_zero, living, seed_where};
    use crate::random::MobRandom;

    fn harness(target: Option<Vec3>, seed: i64) -> Harness {
        let world = Mock {
            players: target
                .map(|at| vec![living(2, EntityKind::Player, at)])
                .unwrap_or_default(),
            ..Mock::default()
        };
        let mut harness = Harness::new(EntityKind::Vex, Vec3::new(0.5, 70.0, 0.5), world, seed);
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
                MobAction::Air(crate::AirAction::Vex(VexAction::SetCharging(value))) => {
                    Some(*value)
                }
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_charge_needs_a_living_target_no_wanted_position_a_roll_and_more_than_two_blocks() {
        let roll = seed_where(|random| random.next_int(reduced_tick_delay(7)) == 0);
        let miss = seed_where(|random| random.next_int(reduced_tick_delay(7)) != 0);
        let far = Some(Vec3::new(8.5, 70.0, 0.5));
        let mut goal = ChargeAttackGoal;
        assert!(harness(far, roll).run(|context| goal.can_use(context)));
        assert!(!harness(far, miss).run(|context| goal.can_use(context)));
        assert!(!harness(None, roll).run(|context| goal.can_use(context)));
        let near = Some(Vec3::new(2.0, 70.0, 0.5));
        assert!(!harness(near, roll).run(|context| goal.can_use(context)));
        let mut wanting = harness(far, roll);
        wanting
            .brain
            .movement
            .set_wanted_position(Vec3::new(3.0, 70.0, 3.0), 1.0);
        assert!(!wanting.run(|context| goal.can_use(context)));
        assert_eq!(
            Goal::<AiContext<'_, Mock>>::flags(&goal),
            Flags::of(Flag::Move)
        );
        assert!(Goal::<AiContext<'_, Mock>>::requires_update_every_tick(
            &goal
        ));
    }

    #[test]
    fn starting_a_charge_aims_at_the_eyes_at_full_speed_raises_the_flag_and_plays_the_sound() {
        let mut harness = harness(Some(Vec3::new(8.5, 70.0, 0.5)), 1);
        let mut goal = ChargeAttackGoal;
        harness.run(|context| goal.start(context));
        let eye = harness.world.players[0].eye_y();
        assert_eq!(harness.brain.movement.wanted(), Vec3::new(8.5, eye, 0.5));
        assert_eq!(harness.brain.movement.speed_modifier(), 1.0);
        assert!(harness.body.vex.charging);
        let actions = drain(&mut harness);
        assert_eq!(charging(&actions), vec![true]);
        assert!(actions.contains(&MobAction::Sound {
            sound: CHARGE_SOUND,
            volume: 1.0,
            pitch: 1.0
        }));
        harness.run(|context| goal.stop(context));
        assert!(!harness.body.vex.charging);
        assert_eq!(charging(&drain(&mut harness)), vec![false]);
    }

    #[test]
    fn a_charge_continues_while_moving_charging_and_the_target_lives() {
        let mut harness = harness(Some(Vec3::new(8.5, 70.0, 0.5)), 1);
        let mut goal = ChargeAttackGoal;
        assert!(!harness.run(|context| goal.can_continue_to_use(context)));
        harness
            .brain
            .movement
            .set_wanted_position(Vec3::new(8.5, 71.0, 0.5), 1.0);
        assert!(!harness.run(|context| goal.can_continue_to_use(context)));
        harness.body.vex.charging = true;
        assert!(harness.run(|context| goal.can_continue_to_use(context)));
        harness.world.players[0].alive = false;
        assert!(!harness.run(|context| goal.can_continue_to_use(context)));
    }

    #[test]
    fn touching_the_target_strikes_it_and_ends_the_charge_and_closing_in_retargets_the_eyes() {
        let mut goal = ChargeAttackGoal;
        let mut touching = harness(Some(Vec3::new(0.5, 70.0, 0.5)), 1);
        touching.body.vex.charging = true;
        touching.run(|context| goal.tick(context));
        let actions = drain(&mut touching);
        assert!(actions.contains(&MobAction::Attack(EntityId(2))));
        assert_eq!(charging(&actions), vec![false]);
        assert!(!touching.body.vex.charging);
        let mut close = harness(Some(Vec3::new(2.5, 70.0, 0.5)), 1);
        close.run(|context| goal.tick(context));
        assert!(drain(&mut close).is_empty());
        let eye = close.world.players[0].eye_y();
        assert_eq!(close.brain.movement.wanted(), Vec3::new(2.5, eye, 0.5));
        let mut far = harness(Some(Vec3::new(6.5, 70.0, 0.5)), 1);
        far.run(|context| goal.tick(context));
        assert!(!far.brain.movement.has_wanted());
    }

    #[test]
    fn the_random_move_waits_for_a_roll_and_never_continues() {
        let roll = seed_where(first_draw_zero(reduced_tick_delay(7)));
        let miss = seed_where(|random| random.next_int(reduced_tick_delay(7)) != 0);
        let mut goal = RandomMoveGoal;
        assert!(harness(None, roll).run(|context| goal.can_use(context)));
        assert!(!harness(None, miss).run(|context| goal.can_use(context)));
        let mut wanting = harness(None, roll);
        wanting.brain.movement.set_wanted_position(Vec3::ZERO, 1.0);
        assert!(!wanting.run(|context| goal.can_use(context)));
        assert!(!harness(None, roll).run(|context| goal.can_continue_to_use(context)));
    }

    #[test]
    fn the_random_move_picks_an_empty_block_around_the_bound_origin_slowly_and_looks_at_it() {
        let mut harness = harness(None, 11);
        harness.mob.species.vex.bound_origin = Some(BlockPos::new(100, 70, 100));
        let mut mirror = MobRandom::new(11);
        let dx = mirror.next_int(15) - 7;
        let dy = mirror.next_int(11) - 5;
        let dz = mirror.next_int(15) - 7;
        let mut goal = RandomMoveGoal;
        harness.run(|context| goal.tick(context));
        let center = Vec3::new(
            f64::from(100 + dx) + 0.5,
            f64::from(70 + dy) + 0.5,
            f64::from(100 + dz) + 0.5,
        );
        assert_eq!(harness.brain.movement.wanted(), center);
        assert_eq!(harness.brain.movement.speed_modifier(), 0.25);
        assert!(harness.brain.look.is_looking_at_target());
        assert_eq!(harness.brain.look.wanted(), center);
    }

    #[test]
    fn the_random_move_gives_up_after_three_blocked_attempts_and_keeps_the_target_in_sight() {
        let mut blocked = harness(None, 3);
        for x in 80..120 {
            for y in 60..80 {
                for z in 80..120 {
                    blocked
                        .world
                        .blocks
                        .insert(BlockPos::new(x, y, z), BlockKind::Stone.default_state());
                }
            }
        }
        blocked.mob.species.vex.bound_origin = Some(BlockPos::new(100, 70, 100));
        let mut goal = RandomMoveGoal;
        let before = blocked.brain.random.clone();
        blocked.run(|context| goal.tick(context));
        assert!(!blocked.brain.movement.has_wanted());
        assert_ne!(blocked.brain.random, before);
        let mut with_target = harness(Some(Vec3::new(9.5, 70.0, 0.5)), 3);
        with_target.run(|context| goal.tick(context));
        assert!(with_target.brain.movement.has_wanted());
        assert!(!with_target.brain.look.is_looking_at_target());
    }

    #[test]
    fn the_move_control_accelerates_along_the_way_by_a_twentieth_of_the_modifier() {
        let mut base = rspigot_domain::entity::EntityBase::new(
            EntityId(1),
            uuid::Uuid::from_u128(1),
            EntityKind::Vex,
        );
        base.position = Vec3::new(0.5, 70.0, 0.5);
        let mut living = rspigot_domain::entity::living::LivingState::new(&mut base, 0.0);
        let stays = tick_move_control(
            &mut base,
            &mut living,
            Vec3::new(10.5, 70.0, 0.5),
            1.0,
            None,
        );
        assert!(stays);
        assert!((base.velocity.x - 0.05).abs() < 1.0e-12);
        assert_eq!(base.yaw, -(mth::atan2(0.05, 0.0) as f32) * RAD_TO_DEG_F);
        assert_eq!(living.body_yaw, base.yaw);
        let target = Vec3::new(0.5, 70.0, 20.5);
        tick_move_control(
            &mut base,
            &mut living,
            Vec3::new(10.5, 70.0, 0.5),
            1.0,
            Some(target),
        );
        assert_eq!(base.yaw, 0.0);
    }

    #[test]
    fn the_move_control_brakes_by_half_and_stops_wanting_inside_its_own_size() {
        let mut base = rspigot_domain::entity::EntityBase::new(
            EntityId(1),
            uuid::Uuid::from_u128(1),
            EntityKind::Vex,
        );
        base.position = Vec3::new(0.5, 70.0, 0.5);
        base.velocity = Vec3::new(0.4, 0.2, 0.0);
        let mut living = rspigot_domain::entity::living::LivingState::new(&mut base, 0.0);
        let stays = tick_move_control(&mut base, &mut living, Vec3::new(0.6, 70.0, 0.5), 1.0, None);
        assert!(!stays);
        assert_eq!(base.velocity, Vec3::new(0.2, 0.1, 0.0));
    }

    #[test]
    fn copying_the_owner_target_needs_one_the_owner_has_and_is_attackable() {
        let mut goal = CopyOwnerTargetGoal::new();
        let mut harness = harness(Some(Vec3::new(8.5, 70.0, 0.5)), 1);
        assert!(!harness.run(|context| goal.can_use(context)));
        harness.mob.species.vex.owner_target = Some(EntityId(2));
        harness.mob.species.vex.has_owner_target = true;
        assert!(harness.run(|context| goal.can_use(context)));
        harness.world.players[0].spectator = true;
        assert!(!harness.run(|context| goal.can_use(context)));
        harness.world.players[0].spectator = false;
        harness.brain.target = None;
        harness.run(|context| goal.start(context));
        let requests = std::mem::take(&mut harness.brain.target_requests);
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].to, Some(EntityId(2)));
        assert_eq!(requests[0].reason, TargetReason::OwnerAttackedTarget);
        assert_eq!(
            Goal::<AiContext<'_, Mock>>::flags(&goal),
            Flags::of(Flag::Target)
        );
    }
}
