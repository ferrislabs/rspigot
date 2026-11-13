use crate::brain::AiContext;
use crate::goal::{Flags, Goal};
use crate::world::AiWorld;

macro_rules! goal_enum {
    ($name:ident { $($variant:ident($ty:ty)),+ $(,)? }) => {
        #[derive(Debug, Clone, PartialEq)]
        pub enum $name {
            $($variant($ty)),+
        }

        impl<W: AiWorld> Goal<AiContext<'_, W>> for $name {
            fn flags(&self) -> Flags {
                match self {
                    $(Self::$variant(goal) => Goal::<AiContext<'_, W>>::flags(goal)),+
                }
            }

            fn can_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
                match self {
                    $(Self::$variant(goal) => Goal::can_use(goal, context)),+
                }
            }

            fn can_continue_to_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
                match self {
                    $(Self::$variant(goal) => Goal::can_continue_to_use(goal, context)),+
                }
            }

            fn is_interruptable(&self) -> bool {
                match self {
                    $(Self::$variant(goal) => Goal::<AiContext<'_, W>>::is_interruptable(goal)),+
                }
            }

            fn start(&mut self, context: &mut AiContext<'_, W>) {
                match self {
                    $(Self::$variant(goal) => Goal::start(goal, context)),+
                }
            }

            fn stop(&mut self, context: &mut AiContext<'_, W>) {
                match self {
                    $(Self::$variant(goal) => Goal::stop(goal, context)),+
                }
            }

            fn requires_update_every_tick(&self) -> bool {
                match self {
                    $(Self::$variant(goal) => {
                        Goal::<AiContext<'_, W>>::requires_update_every_tick(goal)
                    }),+
                }
            }

            fn tick(&mut self, context: &mut AiContext<'_, W>) {
                match self {
                    $(Self::$variant(goal) => Goal::tick(goal, context)),+
                }
            }
        }
    };
}

goal_enum!(AirGoal {
    BeeAttack(crate::bee::AttackGoal),
    BeeEnterHive(crate::bee::EnterHiveGoal),
    BeeValidateHive(crate::bee::ValidateHiveGoal),
    BeeValidateFlower(crate::bee::ValidateFlowerGoal),
    BeePollinate(crate::bee::PollinateGoal),
    BeeLocateHive(crate::bee::LocateHiveGoal),
    BeeGoToHive(crate::bee::GoToHiveGoal),
    BeeGoToKnownFlower(crate::bee::GoToKnownFlowerGoal),
    BeeGrowCrop(crate::bee::GrowCropGoal),
    BeeWander(crate::bee::WanderGoal),
    BeeHurtBy(crate::bee::HurtByOtherGoal),
    BeeBecomeAngry(crate::bee::BecomeAngryTargetGoal),
    ParrotWander(crate::parrot::WanderGoal),
    FollowMob(crate::parrot::FollowMobGoal),
    PhantomAttackStrategy(crate::phantom::AttackStrategyGoal),
    PhantomSweepAttack(crate::phantom::SweepAttackGoal),
    PhantomCircle(crate::phantom::CircleAroundAnchorGoal),
    PhantomAttackPlayer(crate::phantom::AttackPlayerTargetGoal),
    Bat(crate::bat::BatGoal),
    VexCharge(crate::vex::ChargeAttackGoal),
    VexRandomMove(crate::vex::RandomMoveGoal),
    VexCopyOwnerTarget(crate::vex::CopyOwnerTargetGoal),
    WitherDoNothing(crate::wither::DoNothingGoal),
});

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AirAction {
    Bee(crate::bee::BeeAction),
    Phantom(crate::phantom::PhantomAction),
    Bat(crate::bat::BatAction),
    Vex(crate::vex::VexAction),
    Wither(crate::wither::WitherAction),
    Allay(crate::allay::AllayAction),
}
