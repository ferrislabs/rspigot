use rspigot_blocks::{BlockKind, StateFlags, StateId};
use rspigot_domain::{BlockPos, Vec3, mth};

use crate::animal::{BreedGoal, kind_in_block_tag};
use crate::brain::AiContext;
use crate::goal::{Flag, Flags, Goal, adjusted_tick_delay, reduced_tick_delay};
use crate::goals::RandomStrollGoal;
use crate::random::RandomSource;
use crate::random_pos::{self, HALF_PI};
use crate::scan::closer_to_center_than;
use crate::world::AiWorld;
use crate::{MobAction, PanicGoal};

pub const SAND_TAG: &str = "minecraft:sand";
pub const HOME_REACHED: f64 = 7.0;
pub const HOME_TRY_RADIUS: f64 = 16.0;
pub const LAY_RADIUS: f64 = 9.0;
pub const LAY_DURATION: i32 = 200;
pub const TRAVEL_RANGE: i32 = 1025;
pub const TRAVEL_OFFSET: i32 = 512;
pub const TRAVEL_HEIGHT: i32 = 9;
pub const TRAVEL_HEIGHT_OFFSET: i32 = 4;
pub const TRAVEL_CHUNK_MARGIN: i32 = 34;
pub const NARROW_ANGLE: f64 = 0.314_159_274_101_257_3;
const GIVE_UP_TICKS: i32 = 1200;
pub const HOME_FAR: f64 = 64.0;
const WATER_SEARCH: i32 = 24;
const LAY_SEARCH: i32 = 16;
const FOUND_DELAY_BASE: i32 = 200;
const FOUND_DELAY_SPREAD: i32 = 200;
const RECALCULATE_PERIOD: i32 = 40;
const WATER_RECALCULATE_PERIOD: i32 = 160;
const HOME_CHANCE: i32 = 700;
const HOME_GIVE_UP: i32 = 600;
const STROLL_INTERVAL: i32 = 100;
const STROLL_SPEED: f64 = 1.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TurtleFacts {
    pub home_pos: BlockPos,
    pub has_egg: bool,
    pub laying_egg: bool,
    pub lay_egg_counter: i32,
    pub going_home: bool,
    pub travel_pos: Option<BlockPos>,
}

impl TurtleFacts {
    pub fn set_laying_egg(&mut self, laying: bool) {
        self.lay_egg_counter = i32::from(laying);
        self.laying_egg = laying;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurtleAction {
    LayEgg { target: BlockPos },
}

fn act<W: AiWorld>(context: &mut AiContext<'_, W>, action: TurtleAction) {
    context.act(MobAction::Turtle(action));
}

fn is_sand(state: Option<StateId>) -> bool {
    state
        .and_then(StateId::kind)
        .is_some_and(|kind| kind_in_block_tag(kind, SAND_TAG))
}

pub fn on_sand(world: &impl AiWorld, pos: BlockPos) -> bool {
    is_sand(world.state(pos.offset(0, -1, 0)))
}

pub(crate) fn is_water_block(state: Option<StateId>) -> bool {
    state.and_then(StateId::kind) == Some(BlockKind::Water)
}

fn has_water_fluid(state: Option<StateId>) -> bool {
    state.is_some_and(|state| state.flags().contains(StateFlags::WATER))
}

pub fn walk_target_value(world: &impl AiWorld, facts: &TurtleFacts, pos: BlockPos) -> f32 {
    if !facts.going_home && has_water_fluid(world.state(pos)) {
        return 10.0;
    }
    if on_sand(world, pos) {
        return 10.0;
    }
    random_pos::light_magic_value(world, pos) - 0.5
}

pub fn is_stable_destination(world: &impl AiWorld, facts: &TurtleFacts, pos: BlockPos) -> bool {
    if facts.travel_pos.is_some() {
        return is_water_block(world.state(pos));
    }
    world
        .state(pos.offset(0, -1, 0))
        .is_some_and(|state| !state.is_air())
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct MoveToBlock {
    speed_modifier: f64,
    search_range: i32,
    vertical_search_range: i32,
    vertical_search_start: i32,
    next_start_tick: i32,
    pub(crate) try_ticks: i32,
    max_stay_ticks: i32,
    pub(crate) block_pos: BlockPos,
    reached_target: bool,
}

impl MoveToBlock {
    pub(crate) fn new(
        speed_modifier: f64,
        search_range: i32,
        vertical_search_start: i32,
        vertical_search_range: i32,
    ) -> Self {
        Self {
            speed_modifier,
            search_range,
            vertical_search_range,
            vertical_search_start,
            next_start_tick: 0,
            try_ticks: 0,
            max_stay_ticks: 0,
            block_pos: BlockPos::new(0, 0, 0),
            reached_target: false,
        }
    }

    fn find_nearest_block<W: AiWorld>(
        &mut self,
        context: &AiContext<'_, W>,
        valid: fn(&W, BlockPos) -> bool,
    ) -> bool {
        let origin = context.mob.block_pos();
        let mut dy = self.vertical_search_start;
        while dy <= self.vertical_search_range {
            for radius in 0..self.search_range {
                let mut dx = 0;
                while dx <= radius {
                    let mut dz = if dx < radius && dx > -radius {
                        radius
                    } else {
                        0
                    };
                    while dz <= radius {
                        let pos = origin.offset(dx, dy - 1, dz);
                        if context.mob.is_within_home(pos) && valid(context.world, pos) {
                            self.block_pos = pos;
                            return true;
                        }
                        dz = if dz > 0 { -dz } else { 1 - dz };
                    }
                    dx = if dx > 0 { -dx } else { 1 - dx };
                }
            }
            dy = if dy > 0 { -dy } else { 1 - dy };
        }
        false
    }

    pub(crate) fn can_use<W: AiWorld>(
        &mut self,
        context: &mut AiContext<'_, W>,
        valid: fn(&W, BlockPos) -> bool,
    ) -> bool {
        if self.next_start_tick >= 1 {
            self.next_start_tick -= 1;
            return false;
        }
        self.next_start_tick = reduced_tick_delay(
            context.brain.random.next_int(FOUND_DELAY_SPREAD) + FOUND_DELAY_BASE,
        );
        self.find_nearest_block(context, valid)
    }

    fn move_to_block<W: AiWorld>(&self, context: &mut AiContext<'_, W>) {
        let pos = self.block_pos;
        let at = Vec3::new(
            f64::from(pos.x) + 0.5,
            f64::from(pos.y + 1),
            f64::from(pos.z) + 0.5,
        );
        context.move_to_pos(at, self.speed_modifier);
    }

    pub(crate) fn tick<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>, recalculate: i32) {
        let target = self.block_pos.offset(0, 1, 0);
        if closer_to_center_than(target, context.mob.position, 1.0) {
            self.reached_target = true;
            self.try_ticks -= 1;
        } else {
            self.reached_target = false;
            self.try_ticks += 1;
            if self.try_ticks % recalculate == 0 {
                let at = Vec3::new(
                    f64::from(target.x) + 0.5,
                    f64::from(target.y),
                    f64::from(target.z) + 0.5,
                );
                context.move_to_pos(at, self.speed_modifier);
            }
        }
    }
}

impl MoveToBlock {
    pub(crate) fn start<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        start_move_to_block(self, context);
    }

    pub(crate) fn can_continue<W: AiWorld>(
        &self,
        world: &W,
        valid: fn(&W, BlockPos) -> bool,
    ) -> bool {
        self.try_ticks >= -self.max_stay_ticks
            && self.try_ticks <= GIVE_UP_TICKS
            && valid(world, self.block_pos)
    }
}

pub(crate) fn start_move_to_block<W: AiWorld>(
    state: &mut MoveToBlock,
    context: &mut AiContext<'_, W>,
) {
    state.move_to_block(context);
    state.try_ticks = 0;
    let inner = context.brain.random.next_int(1200) + 1200;
    state.max_stay_ticks = context.brain.random.next_int(inner) + 1200;
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TurtleGoToWaterGoal {
    base: MoveToBlock,
}

impl TurtleGoToWaterGoal {
    pub fn new(speed_modifier: f64, baby: bool) -> Self {
        let speed = if baby { 2.0 } else { speed_modifier };
        Self {
            base: MoveToBlock::new(speed, WATER_SEARCH, -1, 1),
        }
    }

    fn valid<W: AiWorld>(world: &W, pos: BlockPos) -> bool {
        is_water_block(world.state(pos))
    }
}

impl<W: AiWorld> Goal<AiContext<'_, W>> for TurtleGoToWaterGoal {
    fn flags(&self) -> Flags {
        Flags::of(Flag::Move).with(Flag::Jump)
    }

    fn can_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        let facts = context.body.turtle;
        let in_water = context.mob.fluid.in_water;
        if context.mob.baby && !in_water {
            return self.base.can_use(context, Self::valid);
        }
        if !facts.going_home && !in_water && !facts.has_egg {
            return self.base.can_use(context, Self::valid);
        }
        false
    }

    fn can_continue_to_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        !context.mob.fluid.in_water
            && self.base.try_ticks <= GIVE_UP_TICKS
            && Self::valid(context.world, self.base.block_pos)
    }

    fn start(&mut self, context: &mut AiContext<'_, W>) {
        start_move_to_block(&mut self.base, context);
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, context: &mut AiContext<'_, W>) {
        self.base.tick(context, WATER_RECALCULATE_PERIOD);
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TurtleLayEggGoal {
    base: MoveToBlock,
}

impl TurtleLayEggGoal {
    pub fn new(speed_modifier: f64) -> Self {
        Self {
            base: MoveToBlock::new(speed_modifier, LAY_SEARCH, 0, 1),
        }
    }

    fn valid<W: AiWorld>(world: &W, pos: BlockPos) -> bool {
        world
            .state(pos.offset(0, 1, 0))
            .is_some_and(StateId::is_air)
            && is_sand(world.state(pos))
    }

    fn home_is_close<W: AiWorld>(context: &AiContext<'_, W>) -> bool {
        closer_to_center_than(
            context.body.turtle.home_pos,
            context.mob.position,
            LAY_RADIUS,
        )
    }
}

impl<W: AiWorld> Goal<AiContext<'_, W>> for TurtleLayEggGoal {
    fn flags(&self) -> Flags {
        Flags::of(Flag::Move).with(Flag::Jump)
    }

    fn can_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        context.body.turtle.has_egg
            && Self::home_is_close(context)
            && self.base.can_use(context, Self::valid)
    }

    fn can_continue_to_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        let base = &self.base;
        base.try_ticks >= -base.max_stay_ticks
            && base.try_ticks <= GIVE_UP_TICKS
            && Self::valid(context.world, base.block_pos)
            && context.body.turtle.has_egg
            && Self::home_is_close(context)
    }

    fn start(&mut self, context: &mut AiContext<'_, W>) {
        start_move_to_block(&mut self.base, context);
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, context: &mut AiContext<'_, W>) {
        self.base.tick(context, RECALCULATE_PERIOD);
        if context.mob.fluid.in_water || !self.base.reached_target {
            return;
        }
        if context.body.turtle.lay_egg_counter < 1 {
            context.body.turtle.set_laying_egg(true);
        } else if context.body.turtle.lay_egg_counter > adjusted_tick_delay(true, LAY_DURATION) {
            act(
                context,
                TurtleAction::LayEgg {
                    target: self.base.block_pos.offset(0, 1, 0),
                },
            );
            context.body.turtle.has_egg = false;
            context.body.turtle.set_laying_egg(false);
        }
        if context.body.turtle.laying_egg {
            context.body.turtle.lay_egg_counter += 1;
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TurtleGoHomeGoal {
    speed_modifier: f64,
    stuck: bool,
    close_to_home_try_ticks: i32,
}

impl TurtleGoHomeGoal {
    pub fn new(speed_modifier: f64) -> Self {
        Self {
            speed_modifier,
            stuck: false,
            close_to_home_try_ticks: 0,
        }
    }
}

fn target_towards<W: AiWorld>(context: &mut AiContext<'_, W>, target: Vec3) -> Option<Vec3> {
    random_pos::pos_towards(context, 16, 3, target, NARROW_ANGLE)
        .or_else(|| random_pos::pos_towards(context, 8, 7, target, HALF_PI))
}

fn bottom_center(pos: BlockPos) -> Vec3 {
    Vec3::new(
        f64::from(pos.x) + 0.5,
        f64::from(pos.y),
        f64::from(pos.z) + 0.5,
    )
}

impl<W: AiWorld> Goal<AiContext<'_, W>> for TurtleGoHomeGoal {
    fn flags(&self) -> Flags {
        Flags::NONE
    }

    fn can_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        if context.mob.baby {
            return false;
        }
        if context.body.turtle.has_egg {
            return true;
        }
        if context
            .brain
            .random
            .next_int(reduced_tick_delay(HOME_CHANCE))
            != 0
        {
            return false;
        }
        !closer_to_center_than(context.body.turtle.home_pos, context.mob.position, HOME_FAR)
    }

    fn can_continue_to_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        !(closer_to_center_than(
            context.body.turtle.home_pos,
            context.mob.position,
            HOME_REACHED,
        ) || self.stuck
            || self.close_to_home_try_ticks > adjusted_tick_delay(false, HOME_GIVE_UP))
    }

    fn start(&mut self, context: &mut AiContext<'_, W>) {
        context.body.turtle.going_home = true;
        self.stuck = false;
        self.close_to_home_try_ticks = 0;
    }

    fn stop(&mut self, context: &mut AiContext<'_, W>) {
        context.body.turtle.going_home = false;
    }

    fn tick(&mut self, context: &mut AiContext<'_, W>) {
        let home = context.body.turtle.home_pos;
        let close = closer_to_center_than(home, context.mob.position, HOME_TRY_RADIUS);
        if close {
            self.close_to_home_try_ticks += 1;
        }
        if !context.brain.navigation.is_done() {
            return;
        }
        let target = bottom_center(home);
        let mut found = target_towards(context, target);
        if let Some(position) = found
            && !close
            && !is_water_block(context.world.state(position.block()))
        {
            found = random_pos::pos_towards(context, 16, 5, target, HALF_PI);
        }
        let Some(position) = found else {
            self.stuck = true;
            return;
        };
        context.move_to_pos(position, self.speed_modifier);
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TurtleTravelGoal {
    speed_modifier: f64,
    stuck: bool,
}

impl TurtleTravelGoal {
    pub fn new(speed_modifier: f64) -> Self {
        Self {
            speed_modifier,
            stuck: false,
        }
    }
}

fn chunks_loaded_around<W: AiWorld>(world: &W, x: i32, z: i32) -> bool {
    let low_x = (x - TRAVEL_CHUNK_MARGIN) >> 4;
    let high_x = (x + TRAVEL_CHUNK_MARGIN) >> 4;
    let low_z = (z - TRAVEL_CHUNK_MARGIN) >> 4;
    let high_z = (z + TRAVEL_CHUNK_MARGIN) >> 4;
    (low_x..=high_x).all(|chunk_x| {
        (low_z..=high_z)
            .all(|chunk_z| world.chunk_loaded(rspigot_domain::ChunkPos::new(chunk_x, chunk_z)))
    })
}

impl<W: AiWorld> Goal<AiContext<'_, W>> for TurtleTravelGoal {
    fn flags(&self) -> Flags {
        Flags::NONE
    }

    fn can_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        let facts = context.body.turtle;
        !facts.going_home && !facts.has_egg && context.mob.fluid.in_water
    }

    fn can_continue_to_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        !(context.brain.navigation.is_done()
            || self.stuck
            || context.body.turtle.going_home
            || context.mob.in_love
            || context.body.turtle.has_egg)
    }

    fn start(&mut self, context: &mut AiContext<'_, W>) {
        let random = &mut context.brain.random;
        let dx = random.next_int(TRAVEL_RANGE) - TRAVEL_OFFSET;
        let mut dy = random.next_int(TRAVEL_HEIGHT) - TRAVEL_HEIGHT_OFFSET;
        let dz = random.next_int(TRAVEL_RANGE) - TRAVEL_OFFSET;
        let position = context.mob.position;
        if f64::from(context.world.sea_level() - 1) < f64::from(dy) + position.y {
            dy = 0;
        }
        context.body.turtle.travel_pos = Some(
            Vec3::new(
                f64::from(dx) + position.x,
                f64::from(dy) + position.y,
                f64::from(dz) + position.z,
            )
            .block(),
        );
        self.stuck = false;
    }

    fn stop(&mut self, context: &mut AiContext<'_, W>) {
        context.body.turtle.travel_pos = None;
    }

    fn tick(&mut self, context: &mut AiContext<'_, W>) {
        let Some(travel) = context.body.turtle.travel_pos else {
            self.stuck = true;
            return;
        };
        if !context.brain.navigation.is_done() {
            return;
        }
        let target = bottom_center(travel);
        let mut found = target_towards(context, target);
        if let Some(position) = found
            && !chunks_loaded_around(
                context.world,
                mth::floor(position.x),
                mth::floor(position.z),
            )
        {
            found = None;
        }
        let Some(position) = found else {
            self.stuck = true;
            return;
        };
        context.move_to_pos(position, self.speed_modifier);
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TurtleRandomStrollGoal {
    stroll: RandomStrollGoal,
}

impl Default for TurtleRandomStrollGoal {
    fn default() -> Self {
        Self {
            stroll: RandomStrollGoal::with_interval(STROLL_SPEED, STROLL_INTERVAL),
        }
    }
}

impl<W: AiWorld> Goal<AiContext<'_, W>> for TurtleRandomStrollGoal {
    fn flags(&self) -> Flags {
        Flags::of(Flag::Move)
    }

    fn can_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        if context.mob.fluid.in_water
            || context.body.turtle.going_home
            || context.body.turtle.has_egg
        {
            return false;
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
pub struct TurtleBreedGoal {
    breed: BreedGoal,
}

impl TurtleBreedGoal {
    pub fn new(speed_modifier: f64) -> Self {
        Self {
            breed: BreedGoal::new(speed_modifier),
        }
    }
}

impl<W: AiWorld> Goal<AiContext<'_, W>> for TurtleBreedGoal {
    fn flags(&self) -> Flags {
        Flags::of(Flag::Move).with(Flag::Look)
    }

    fn can_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        self.breed.can_use(context) && !context.body.turtle.has_egg
    }

    fn can_continue_to_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        self.breed.can_continue_to_use(context)
    }

    fn stop(&mut self, _context: &mut AiContext<'_, W>) {
        self.breed.stop();
    }

    fn tick(&mut self, context: &mut AiContext<'_, W>) {
        self.breed.tick(context);
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TurtlePanicGoal {
    panic: PanicGoal,
}

impl TurtlePanicGoal {
    pub fn new(speed_modifier: f64) -> Self {
        Self {
            panic: PanicGoal::turtle(speed_modifier),
        }
    }
}

impl<W: AiWorld> Goal<AiContext<'_, W>> for TurtlePanicGoal {
    fn flags(&self) -> Flags {
        Flags::of(Flag::Move)
    }

    fn can_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        self.panic.can_use(context)
    }

    fn can_continue_to_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        !context.brain.navigation.is_done()
    }

    fn start(&mut self, context: &mut AiContext<'_, W>) {
        self.panic.start(context);
    }

    fn stop(&mut self, _context: &mut AiContext<'_, W>) {
        self.panic.stop();
    }

    fn tick(&mut self, context: &mut AiContext<'_, W>) {
        self.panic.tick(context);
    }
}

goal_enum!(TurtleGoal {
    Panic(TurtlePanicGoal),
    Breed(TurtleBreedGoal),
    LayEgg(TurtleLayEggGoal),
    GoToWater(TurtleGoToWaterGoal),
    GoHome(TurtleGoHomeGoal),
    Travel(TurtleTravelGoal),
    RandomStroll(TurtleRandomStrollGoal),
});
