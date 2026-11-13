use rspigot_domain::entity::species::guardian::{self, ATTACK_EVENT};
use rspigot_domain::{EntityId, Vec3};

use crate::brain::AiContext;
use crate::goal::{Flag, Flags, Goal};
use crate::goals::RandomStrollGoal;
use crate::restriction::MoveTowardsRestrictionGoal;
use crate::world::{AiWorld, Living};
use crate::{MobAction, TargetReason};

const LOOK_SPEED: f32 = 90.0;
const START_ATTACK_TIME: i32 = -10;
const CLOSE_RANGE_SQR: f64 = 9.0;
const HARD: i32 = 3;
const RESTRICTION_SPEED: f64 = 1.0;
const STROLL_SPEED: f64 = 1.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct GuardianFacts {
    pub attack_target: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct GuardianBrain {
    pub stroll_trigger: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GuardianAction {
    Beam { target: EntityId, damage: f32 },
}

fn eye_of(target: &Living) -> Vec3 {
    Vec3::new(target.position.x, target.eye_y(), target.position.z)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct GuardianAttackGoal {
    attack_time: i32,
}

impl GuardianAttackGoal {
    pub fn new() -> Self {
        Self::default()
    }

    fn target<W: AiWorld>(context: &AiContext<'_, W>) -> Option<Living> {
        context.brain.target.and_then(|id| context.world.living(id))
    }
}

impl<W: AiWorld> Goal<AiContext<'_, W>> for GuardianAttackGoal {
    fn flags(&self) -> Flags {
        Flags::of(Flag::Move).with(Flag::Look)
    }

    fn can_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        Self::target(context).is_some_and(|target| target.alive)
    }

    fn can_continue_to_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        if !self.can_use(context) {
            return false;
        }
        if guardian::is_elder(context.mob.kind) {
            return true;
        }
        Self::target(context)
            .is_some_and(|target| context.mob.distance_to_sqr(target.position) > CLOSE_RANGE_SQR)
    }

    fn start(&mut self, context: &mut AiContext<'_, W>) {
        self.attack_time = START_ATTACK_TIME;
        context.stop_navigation();
        if let Some(target) = Self::target(context) {
            context
                .brain
                .look
                .set_look_at(eye_of(&target), LOOK_SPEED, LOOK_SPEED);
        }
    }

    fn stop(&mut self, context: &mut AiContext<'_, W>) {
        context.body.guardian.attack_target = 0;
        context.set_target(None, TargetReason::Unknown);
        context.brain.guardian.stroll_trigger = true;
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, context: &mut AiContext<'_, W>) {
        let Some(target) = Self::target(context) else {
            return;
        };
        context.stop_navigation();
        context
            .brain
            .look
            .set_look_at(eye_of(&target), LOOK_SPEED, LOOK_SPEED);
        let world = context.world;
        let sees = context
            .brain
            .sensing
            .has_line_of_sight(world, context.mob, &target);
        if !sees {
            context.set_target(None, TargetReason::Unknown);
            return;
        }
        self.attack_time += 1;
        if self.attack_time == 0 {
            context.body.guardian.attack_target = target.id.0;
            if !context.mob.silent {
                context.act(MobAction::EntityEvent(ATTACK_EVENT));
            }
        } else if self.attack_time >= guardian::attack_duration(context.mob.kind) {
            let damage = guardian::beam_damage(
                world.difficulty() == HARD,
                guardian::is_elder(context.mob.kind),
            );
            context.act(MobAction::Guardian(GuardianAction::Beam {
                target: target.id,
                damage,
            }));
            context.act(MobAction::Attack(target.id));
            context.set_target(None, TargetReason::Unknown);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GuardianStrollGoal {
    stroll: RandomStrollGoal,
}

impl GuardianStrollGoal {
    pub fn new(interval: i32) -> Self {
        Self {
            stroll: RandomStrollGoal::with_interval(STROLL_SPEED, interval),
        }
    }
}

impl<W: AiWorld> Goal<AiContext<'_, W>> for GuardianStrollGoal {
    fn flags(&self) -> Flags {
        Flags::of(Flag::Move).with(Flag::Look)
    }

    fn can_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        if std::mem::take(&mut context.brain.guardian.stroll_trigger) {
            self.stroll.trigger();
        }
        self.stroll.can_use(context)
    }

    fn can_continue_to_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        self.stroll.can_continue_to_use(context)
    }

    fn start(&mut self, context: &mut AiContext<'_, W>) {
        self.stroll.start(context);
    }

    fn stop(&mut self, context: &mut AiContext<'_, W>) {
        self.stroll.stop(context);
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GuardianRestrictionGoal {
    inner: MoveTowardsRestrictionGoal,
}

impl Default for GuardianRestrictionGoal {
    fn default() -> Self {
        Self {
            inner: MoveTowardsRestrictionGoal::new(RESTRICTION_SPEED),
        }
    }
}

impl<W: AiWorld> Goal<AiContext<'_, W>> for GuardianRestrictionGoal {
    fn flags(&self) -> Flags {
        Flags::of(Flag::Move).with(Flag::Look)
    }

    fn can_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        self.inner.can_use(context)
    }

    fn can_continue_to_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        self.inner.can_continue_to_use(context)
    }

    fn start(&mut self, context: &mut AiContext<'_, W>) {
        self.inner.start(context);
    }
}

pub fn is_attack_target(kind: rspigot_entities::EntityKind) -> bool {
    kind == rspigot_entities::EntityKind::Player
        || kind == rspigot_entities::EntityKind::Squid
        || kind == rspigot_entities::EntityKind::GlowSquid
        || kind == rspigot_entities::EntityKind::Axolotl
}

pub fn attack_selector(target: &Living, guardian: &crate::world::MobView) -> bool {
    guardian::target_selector(
        is_attack_target(target.kind),
        guardian.distance_to_sqr(target.position),
    )
}

goal_enum!(GuardianGoal {
    Attack(GuardianAttackGoal),
    Restriction(GuardianRestrictionGoal),
    Stroll(GuardianStrollGoal),
});
