use rspigot_blocks::{BlockKind, StateFlags};
use rspigot_domain::BlockPos;

use crate::brain::AiContext;
use crate::classify::is_pathfindable;
use crate::goal::{Flag, Flags, Goal};
use crate::turtle::{MoveToBlock, start_move_to_block};
use crate::world::AiWorld;

pub const GO_TO_LAVA_SPEED: f64 = 1.0;
pub const PANIC_SPEED: f64 = 1.65;
pub const TEMPT_SPEED: f64 = 1.4;
pub const BREED_PRIORITY: i32 = 2;
pub const STROLL_INTERVAL: i32 = 60;
pub const TEMPT_ITEMS: &str = "minecraft:strider_tempt_items";
const SEARCH_RANGE: i32 = 8;
const VERTICAL_SEARCH_RANGE: i32 = 2;
const RECALCULATE_PERIOD: i32 = 20;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StriderGoToLavaGoal {
    base: MoveToBlock,
}

impl StriderGoToLavaGoal {
    pub fn new(speed_modifier: f64) -> Self {
        Self {
            base: MoveToBlock::new(speed_modifier, SEARCH_RANGE, 0, VERTICAL_SEARCH_RANGE),
        }
    }

    fn valid<W: AiWorld>(world: &W, pos: BlockPos) -> bool {
        world
            .state(pos)
            .is_some_and(|state| state.kind() == Some(BlockKind::Lava))
            && world
                .state(pos.offset(0, 1, 0))
                .is_some_and(is_pathfindable)
    }
}

impl<W: AiWorld> Goal<AiContext<'_, W>> for StriderGoToLavaGoal {
    fn flags(&self) -> Flags {
        Flags::of(Flag::Move).with(Flag::Jump)
    }

    fn can_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        !context.mob.fluid.in_lava() && self.base.can_use(context, Self::valid)
    }

    fn can_continue_to_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        !context.mob.fluid.in_lava() && Self::valid(context.world, self.base.block_pos)
    }

    fn start(&mut self, context: &mut AiContext<'_, W>) {
        start_move_to_block(&mut self.base, context);
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, context: &mut AiContext<'_, W>) {
        self.base.tick(context, RECALCULATE_PERIOD);
    }
}

pub fn is_lava_state(state: Option<rspigot_blocks::StateId>) -> bool {
    state.is_some_and(|state| state.flags().contains(StateFlags::LAVA))
}
