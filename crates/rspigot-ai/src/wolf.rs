use rspigot_domain::Vec3;
use rspigot_entities::EntityKind;
use rspigot_items::ItemKind;

use crate::action::{MobAction, TargetReason};
use crate::brain::AiContext;
use crate::combat::{HurtByTargetGoal, LeapAtTargetGoal, MeleeAttackGoal};
use crate::evade::AvoidEntityGoal;
use crate::goal::{Flag, Flags, Goal, adjusted_tick_delay};
use crate::goals::{MobGoal, NearestAttackableTargetGoal, TargetFilter, TemptItems};
use crate::random::RandomSource;
use crate::registry::Goals;
use crate::tame::{FollowOwnerGoal, OwnerTargetGoal, SitWhenOrderedToGoal};
use crate::targeting::{self, TargetingConditions};
use crate::world::{AiWorld, TargetKind};
use crate::{BreedGoal, PanicGoal, PathType, ResetUniversalAngerGoal};

const BEG_LOOK_SPEED: f32 = 10.0;
const BEG_MIN_TICKS: i32 = 40;
const BEG_EXTRA_TICKS: i32 = 40;
const LLAMA_AVOID_DISTANCE: f32 = 24.0;
const LLAMA_STRENGTH_ROLL: i32 = 5;
const WOLF_FOOD: &str = "minecraft:wolf_food";

#[derive(Debug, Clone, PartialEq)]
pub struct BegGoal {
    look_distance: f32,
    player: Option<rspigot_domain::EntityId>,
    look_time: i32,
}

impl BegGoal {
    pub const fn new(look_distance: f32) -> Self {
        Self {
            look_distance,
            player: None,
            look_time: 0,
        }
    }

    fn holding_interesting(
        main: Option<rspigot_domain::ItemId>,
        off: Option<rspigot_domain::ItemId>,
    ) -> bool {
        [main, off].into_iter().any(|item| {
            TemptItems::Item(ItemKind::Bone).matches(item)
                || TemptItems::Tag(WOLF_FOOD).matches(item)
        })
    }

    fn can_use<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) -> bool {
        let conditions = TargetingConditions::for_non_combat().range(f64::from(self.look_distance));
        let origin = context.mob.position;
        self.player =
            targeting::nearest_player(context, conditions, |_| true, origin).map(|p| p.id);
        let Some(player) = self.player.and_then(|id| context.world.living(id)) else {
            return false;
        };
        Self::holding_interesting(player.main_hand, player.off_hand)
    }

    fn can_continue_to_use<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) -> bool {
        let Some(player) = self.player.and_then(|id| context.world.living(id)) else {
            return false;
        };
        if !player.alive {
            return false;
        }
        let reach = f64::from(self.look_distance * self.look_distance);
        if context.mob.distance_to_sqr(player.position) > reach {
            return false;
        }
        self.look_time > 0 && Self::holding_interesting(player.main_hand, player.off_hand)
    }

    fn start<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        context.act(MobAction::Wolf(WolfAction::SetInterested(true)));
        let ticks = BEG_MIN_TICKS + context.brain.random.next_int(BEG_EXTRA_TICKS);
        self.look_time = adjusted_tick_delay(false, ticks);
    }

    fn stop<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        context.act(MobAction::Wolf(WolfAction::SetInterested(false)));
        self.player = None;
    }

    fn tick<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        let Some(player) = self.player.and_then(|id| context.world.living(id)) else {
            return;
        };
        let at = Vec3::new(player.position.x, player.eye_y(), player.position.z);
        let max = context.mob.max_head_x_rot();
        context.brain.look.set_look_at(at, BEG_LOOK_SPEED, max);
        self.look_time -= 1;
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct WolfAvoidEntityGoal {
    inner: AvoidEntityGoal,
}

impl WolfAvoidEntityGoal {
    pub fn llama(walk: f64, sprint: f64) -> Self {
        Self {
            inner: AvoidEntityGoal::new(
                TargetKind::Class("Llama"),
                LLAMA_AVOID_DISTANCE,
                walk,
                sprint,
            ),
        }
    }

    fn can_use<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) -> bool {
        if !self.inner.can_use(context) {
            return false;
        }
        let Some(llama) = self
            .inner
            .to_avoid()
            .and_then(|id| context.world.living(id))
        else {
            return false;
        };
        if context.mob.tamed {
            return false;
        }
        llama.facts.llama_strength >= context.brain.random.next_int(LLAMA_STRENGTH_ROLL)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum WolfGoal {
    Beg(BegGoal),
    AvoidLlama(WolfAvoidEntityGoal),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WolfFacts;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WolfAction {
    SetInterested(bool),
}

impl<W: AiWorld> Goal<AiContext<'_, W>> for WolfGoal {
    fn flags(&self) -> Flags {
        match self {
            Self::Beg(_) => Flags::of(Flag::Look),
            Self::AvoidLlama(_) => Flags::of(Flag::Move),
        }
    }

    fn can_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        match self {
            Self::Beg(goal) => goal.can_use(context),
            Self::AvoidLlama(goal) => goal.can_use(context),
        }
    }

    fn can_continue_to_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        match self {
            Self::Beg(goal) => goal.can_continue_to_use(context),
            Self::AvoidLlama(goal) => goal.inner.can_continue_to_use(context),
        }
    }

    fn start(&mut self, context: &mut AiContext<'_, W>) {
        match self {
            Self::Beg(goal) => goal.start(context),
            Self::AvoidLlama(goal) => {
                context.set_target(None, TargetReason::Unknown);
                goal.inner.start(context);
            }
        }
    }

    fn stop(&mut self, context: &mut AiContext<'_, W>) {
        match self {
            Self::Beg(goal) => goal.stop(context),
            Self::AvoidLlama(goal) => goal.inner.stop(),
        }
    }

    fn tick(&mut self, context: &mut AiContext<'_, W>) {
        match self {
            Self::Beg(goal) => goal.tick(context),
            Self::AvoidLlama(goal) => {
                context.set_target(None, TargetReason::Unknown);
                goal.inner.tick(context);
            }
        }
    }
}

pub(crate) fn register(add: &mut Goals<'_>, _kind: EntityKind) {
    add.brain.profile.set_malus(PathType::PowderSnow, -1.0);
    add.brain
        .profile
        .set_malus(PathType::OnTopOfPowderSnow, -1.0);
    add.float(1);
    add.goal(1, MobGoal::Panic(PanicGoal::tamable_environmental(1.5)));
    add.goal(2, MobGoal::SitWhenOrdered(SitWhenOrderedToGoal));
    add.goal(
        3,
        MobGoal::Wolf(WolfGoal::AvoidLlama(WolfAvoidEntityGoal::llama(1.5, 1.5))),
    );
    add.goal(4, MobGoal::LeapAtTarget(LeapAtTargetGoal::new(0.4)));
    add.goal(5, MobGoal::MeleeAttack(MeleeAttackGoal::new(1.0, true)));
    add.goal(
        6,
        MobGoal::FollowOwner(FollowOwnerGoal::new(1.0, 10.0, 2.0)),
    );
    add.goal(7, MobGoal::Breed(BreedGoal::new(1.0)));
    add.stroll(8, 1.0);
    add.goal(9, MobGoal::Wolf(WolfGoal::Beg(BegGoal::new(8.0))));
    add.look(10, 8.0);
    add.look_around(10);
    add.target_goal(1, MobGoal::OwnerTarget(OwnerTargetGoal::hurt_by()));
    add.target_goal(2, MobGoal::OwnerTarget(OwnerTargetGoal::hurt_target()));
    add.target_goal(3, MobGoal::HurtByTarget(HurtByTargetGoal::alerting(&[])));
    add.target(
        4,
        NearestAttackableTargetGoal::with(TargetKind::Player, 10, true, TargetFilter::AngryAt),
    );
    add.target(
        5,
        NearestAttackableTargetGoal::non_tame(
            TargetKind::Class("Animal"),
            false,
            TargetFilter::Prey,
        ),
    );
    add.target(
        6,
        NearestAttackableTargetGoal::non_tame(TargetKind::Turtle, false, TargetFilter::BabyOnLand),
    );
    add.target(
        7,
        NearestAttackableTargetGoal::new(TargetKind::Class("AbstractSkeleton"), false),
    );
    add.target_goal(
        8,
        MobGoal::ResetUniversalAnger(ResetUniversalAngerGoal::new(true)),
    );
}
