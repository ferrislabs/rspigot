use rspigot_entities::EntityKind;

use crate::animal::{FollowParentGoal, PanicGoal};
use crate::brain::AiContext;
use crate::combat::{HurtByTargetGoal, MeleeAttackGoal};
use crate::goal::{Flag, Flags, Goal};
use crate::goals::{MobGoal, NearestAttackableTargetGoal, RandomStrollGoal, TargetFilter};
use crate::neutral::ResetUniversalAngerGoal;
use crate::registry::Goals;
use crate::world::{AiWorld, TargetKind, inflate};

const PANIC_SPEED: f64 = 2.0;
const FOLLOW_PARENT_SPEED: f64 = 1.25;
const STROLL_SPEED: f64 = 1.0;
const LOOK_DISTANCE: f32 = 6.0;
const TARGET_INTERVAL: i32 = 10;
const ATTACK_PLAYERS_INTERVAL: i32 = 20;
const ATTACK_PLAYERS_FOLLOW_SCALE: f64 = 0.5;
const CUB_RANGE: (f64, f64, f64) = (8.0, 4.0, 8.0);

#[derive(Debug, Clone, PartialEq)]
pub enum PolarBearGoal {
    AttackPlayers(NearestAttackableTargetGoal),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PolarBearFacts;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolarBearAction {
    SetStanding(bool),
    Warn,
}

impl PolarBearGoal {
    fn attack_players() -> Self {
        Self::AttackPlayers(
            NearestAttackableTargetGoal::with(
                TargetKind::Player,
                ATTACK_PLAYERS_INTERVAL,
                true,
                TargetFilter::Any,
            )
            .with_follow_scale(ATTACK_PLAYERS_FOLLOW_SCALE),
        )
    }
}

impl<W: AiWorld> Goal<AiContext<'_, W>> for PolarBearGoal {
    fn flags(&self) -> Flags {
        match self {
            Self::AttackPlayers(_) => Flags::of(Flag::Target),
        }
    }

    fn can_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        match self {
            Self::AttackPlayers(inner) => {
                if context.mob.baby {
                    return false;
                }
                if !inner.can_use(context) {
                    return false;
                }
                let area = inflate(&context.mob.bounds(), CUB_RANGE.0, CUB_RANGE.1, CUB_RANGE.2);
                context
                    .world
                    .living_of(TargetKind::Class("PolarBear"), &area)
                    .iter()
                    .any(|bear| bear.baby)
            }
        }
    }

    fn can_continue_to_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        match self {
            Self::AttackPlayers(inner) => inner.can_continue_to_use(context),
        }
    }

    fn start(&mut self, context: &mut AiContext<'_, W>) {
        match self {
            Self::AttackPlayers(inner) => inner.start(context),
        }
    }

    fn stop(&mut self, context: &mut AiContext<'_, W>) {
        match self {
            Self::AttackPlayers(inner) => inner.stop(context),
        }
    }
}

pub(crate) fn register(add: &mut Goals<'_>, _kind: EntityKind) {
    add.float(0);
    add.goal(1, MobGoal::MeleeAttack(MeleeAttackGoal::polar_bear()));
    add.goal(1, MobGoal::Panic(PanicGoal::new(PANIC_SPEED)));
    add.goal(
        4,
        MobGoal::FollowParent(FollowParentGoal::new(FOLLOW_PARENT_SPEED)),
    );
    add.goal(5, MobGoal::Stroll(RandomStrollGoal::new(STROLL_SPEED)));
    add.look(6, LOOK_DISTANCE);
    add.look_around(7);
    add.target_goal(1, MobGoal::HurtByTarget(HurtByTargetGoal::polar_bear()));
    add.target_goal(2, MobGoal::PolarBear(PolarBearGoal::attack_players()));
    add.target(
        3,
        NearestAttackableTargetGoal::with(
            TargetKind::Player,
            TARGET_INTERVAL,
            true,
            TargetFilter::AngryAt,
        ),
    );
    add.target(
        4,
        NearestAttackableTargetGoal::with(
            TargetKind::Class("Fox"),
            TARGET_INTERVAL,
            true,
            TargetFilter::MobIsAdult,
        ),
    );
    add.target_goal(
        5,
        MobGoal::ResetUniversalAnger(ResetUniversalAngerGoal::new(false)),
    );
}
