use rspigot_blocks::{Face, StateId, Support};
use rspigot_domain::entity::species::drowned;
use rspigot_domain::{BlockPos, Vec3};
use rspigot_items::ItemKind;

use crate::brain::AiContext;
use crate::combat::MeleeAttackGoal;
use crate::goal::{Flag, Flags, Goal};
use crate::random::RandomSource;
use crate::random_pos::{self, HALF_PI};
use crate::ranged::RangedAttackGoal;
use crate::turtle::{MoveToBlock, is_water_block};
use crate::world::{AiWorld, Hand, Living};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DrownedFacts {
    pub searching_for_land: bool,
}

fn ok_target<W: AiWorld>(world: &W, target: Option<&Living>) -> bool {
    target.is_some_and(|target| !world.bright_outside() || target.in_water)
}

fn current_target<W: AiWorld>(context: &AiContext<'_, W>) -> Option<Living> {
    context.target()
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DrownedGoToWaterGoal {
    speed_modifier: f64,
    wanted: Vec3,
}

impl DrownedGoToWaterGoal {
    pub fn new(speed_modifier: f64) -> Self {
        Self {
            speed_modifier,
            wanted: Vec3::ZERO,
        }
    }

    fn water_pos<W: AiWorld>(context: &mut AiContext<'_, W>) -> Option<Vec3> {
        let origin = context.mob.block_pos();
        for _ in 0..drowned::WATER_SEARCH_TRIES {
            let random = &mut context.brain.random;
            let dx = random.next_int(drowned::WATER_SEARCH_WIDTH) - drowned::WATER_SEARCH_OFFSET;
            let dy = drowned::WATER_SEARCH_TOP - random.next_int(drowned::WATER_SEARCH_HEIGHT);
            let dz = random.next_int(drowned::WATER_SEARCH_WIDTH) - drowned::WATER_SEARCH_OFFSET;
            let pos = origin.offset(dx, dy, dz);
            if is_water_block(context.world.state(pos)) {
                return Some(Vec3::new(
                    f64::from(pos.x) + 0.5,
                    f64::from(pos.y),
                    f64::from(pos.z) + 0.5,
                ));
            }
        }
        None
    }
}

impl<W: AiWorld> Goal<AiContext<'_, W>> for DrownedGoToWaterGoal {
    fn flags(&self) -> Flags {
        Flags::of(Flag::Move)
    }

    fn can_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        if !context.world.bright_outside() || context.mob.fluid.in_water {
            return false;
        }
        let Some(position) = Self::water_pos(context) else {
            return false;
        };
        self.wanted = position;
        true
    }

    fn can_continue_to_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        !context.brain.navigation.is_done()
    }

    fn start(&mut self, context: &mut AiContext<'_, W>) {
        context.move_to_pos(self.wanted, self.speed_modifier);
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DrownedGoToBeachGoal {
    base: MoveToBlock,
}

impl DrownedGoToBeachGoal {
    pub fn new(speed_modifier: f64) -> Self {
        Self {
            base: MoveToBlock::new(
                speed_modifier,
                drowned::BEACH_SEARCH,
                0,
                drowned::BEACH_VERTICAL,
            ),
        }
    }

    fn valid<W: AiWorld>(world: &W, pos: BlockPos) -> bool {
        world
            .state(pos.offset(0, 1, 0))
            .is_some_and(StateId::is_air)
            && world
                .state(pos.offset(0, 2, 0))
                .is_some_and(StateId::is_air)
            && world
                .state(pos)
                .is_some_and(|state| state.is_face_sturdy(Face::Up, Support::Full))
    }
}

impl<W: AiWorld> Goal<AiContext<'_, W>> for DrownedGoToBeachGoal {
    fn flags(&self) -> Flags {
        Flags::of(Flag::Move).with(Flag::Jump)
    }

    fn can_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        self.base.can_use(context, Self::valid)
            && !context.world.bright_outside()
            && context.mob.fluid.in_water
            && context.mob.position.y >= f64::from(context.world.sea_level() - drowned::BEACH_DEPTH)
    }

    fn can_continue_to_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        self.base.can_continue(context.world, Self::valid)
    }

    fn start(&mut self, context: &mut AiContext<'_, W>) {
        context.body.drowned.searching_for_land = false;
        self.base.start(context);
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, context: &mut AiContext<'_, W>) {
        self.base.tick(context, drowned::BEACH_RECALCULATE);
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DrownedSwimUpGoal {
    speed_modifier: f64,
    stuck: bool,
}

impl DrownedSwimUpGoal {
    pub fn new(speed_modifier: f64) -> Self {
        Self {
            speed_modifier,
            stuck: false,
        }
    }

    fn close_to_next_pos<W: AiWorld>(context: &AiContext<'_, W>) -> bool {
        context
            .brain
            .navigation
            .path()
            .map(|path| path.target())
            .is_some_and(|target| {
                context.mob.position.distance_squared(Vec3::new(
                    f64::from(target.x),
                    f64::from(target.y),
                    f64::from(target.z),
                )) < drowned::CLOSE_TO_NEXT_SQR
            })
    }
}

impl<W: AiWorld> Goal<AiContext<'_, W>> for DrownedSwimUpGoal {
    fn flags(&self) -> Flags {
        Flags::NONE
    }

    fn can_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        !context.world.bright_outside()
            && context.mob.fluid.in_water
            && context.mob.position.y
                < f64::from(context.world.sea_level() - drowned::SWIM_UP_START)
    }

    fn can_continue_to_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        self.can_use(context) && !self.stuck
    }

    fn start(&mut self, context: &mut AiContext<'_, W>) {
        context.body.drowned.searching_for_land = true;
        self.stuck = false;
    }

    fn stop(&mut self, context: &mut AiContext<'_, W>) {
        context.body.drowned.searching_for_land = false;
    }

    fn tick(&mut self, context: &mut AiContext<'_, W>) {
        let surface = f64::from(context.world.sea_level() - 1);
        if context.mob.position.y >= surface {
            return;
        }
        if !context.brain.navigation.is_done() && !Self::close_to_next_pos(context) {
            return;
        }
        let target = Vec3::new(context.mob.position.x, surface, context.mob.position.z);
        let Some(position) = random_pos::pos_towards(
            context,
            drowned::SWIM_UP_HORIZONTAL,
            drowned::SWIM_UP_VERTICAL,
            target,
            HALF_PI,
        ) else {
            self.stuck = true;
            return;
        };
        context.move_to_pos(position, self.speed_modifier);
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DrownedAttackGoal {
    melee: MeleeAttackGoal,
}

impl DrownedAttackGoal {
    pub fn new(speed_modifier: f64) -> Self {
        Self {
            melee: MeleeAttackGoal::zombie(speed_modifier),
        }
    }
}

impl<W: AiWorld> Goal<AiContext<'_, W>> for DrownedAttackGoal {
    fn flags(&self) -> Flags {
        Flags::of(Flag::Move).with(Flag::Look)
    }

    fn can_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        self.melee.can_use(context) && ok_target(context.world, current_target(context).as_ref())
    }

    fn can_continue_to_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        self.melee.can_continue_to_use(context)
            && ok_target(context.world, current_target(context).as_ref())
    }

    fn start(&mut self, context: &mut AiContext<'_, W>) {
        self.melee.start(context);
    }

    fn stop(&mut self, context: &mut AiContext<'_, W>) {
        self.melee.stop(context);
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, context: &mut AiContext<'_, W>) {
        self.melee.tick(context);
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DrownedTridentAttackGoal {
    ranged: RangedAttackGoal,
}

impl DrownedTridentAttackGoal {
    pub fn new(speed_modifier: f64, interval: i32, radius: f32) -> Self {
        Self {
            ranged: RangedAttackGoal::new(speed_modifier, interval, radius),
        }
    }
}

impl<W: AiWorld> Goal<AiContext<'_, W>> for DrownedTridentAttackGoal {
    fn flags(&self) -> Flags {
        Flags::of(Flag::Move).with(Flag::Look)
    }

    fn can_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        self.ranged.can_use(context) && context.mob.main_hand == Some(ItemKind::Trident.id())
    }

    fn can_continue_to_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        self.ranged.can_continue_to_use(context)
    }

    fn start(&mut self, context: &mut AiContext<'_, W>) {
        context.body.aggressive = true;
        context.body.using_item = Some(Hand::Main);
    }

    fn stop(&mut self, context: &mut AiContext<'_, W>) {
        self.ranged.stop(context);
        context.body.using_item = None;
        context.body.aggressive = false;
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, context: &mut AiContext<'_, W>) {
        self.ranged.tick(context);
    }
}

pub fn drowned_ok_target(world: &impl AiWorld, target: &Living) -> bool {
    ok_target(world, Some(target))
}

goal_enum!(DrownedGoal {
    GoToWater(DrownedGoToWaterGoal),
    TridentAttack(DrownedTridentAttackGoal),
    Attack(DrownedAttackGoal),
    GoToBeach(DrownedGoToBeachGoal),
    SwimUp(DrownedSwimUpGoal),
});
