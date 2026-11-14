use rspigot_blocks::{BlockKind, StateId};
use rspigot_domain::{BlockPos, EntityId, Vec3, mth};
use rspigot_entities::EntityKind;

use crate::action::MobAction;
use crate::animal::{PanicGoal, block_tag};
use crate::brain::AiContext;
use crate::combat::{LeapAtTargetGoal, OcelotAttackGoal};
use crate::evade::AvoidEntityGoal;
use crate::goal::{Flag, Flags, Goal, adjusted_tick_delay, reduced_tick_delay};
use crate::goals::{MobGoal, NearestAttackableTargetGoal, TargetFilter, TemptGoal, TemptItems};
use crate::random::RandomSource;
use crate::registry::Goals;
use crate::scan::closer_to_center_than;
use crate::tame::{FollowOwnerGoal, SitWhenOrderedToGoal};
use crate::world::{AiWorld, Living, TargetKind, inflate};
use rspigot_domain::entity::Aabb;

pub const CAT_FOOD: &str = "minecraft:cat_food";
pub const TEMPT_SPEED: f64 = 0.6;
pub const WALK_SPEED: f64 = 0.8;
pub const SPRINT_SPEED: f64 = 1.33;
pub const AVOID_DISTANCE: f32 = 16.0;
pub const STROLL_WATER_PROBABILITY: f32 = 1.000_000_1e-5;
const CATS_CAN_LIE_ON: &str = "minecraft:cats_can_lie_on";
const CATS_CAN_SIT_ON: &str = "minecraft:cats_can_sit_on";
const BEDS: &str = "minecraft:beds";
const GIVE_UP_TICKS: i32 = 1200;
const STAY_TICKS: i32 = 1200;
const INTERVAL_TICKS: i32 = 200;
const LIE_RESTART_TICKS: i32 = 40;
const LIE_SEARCH_RANGE: i32 = 8;
const LIE_VERTICAL_RANGE: i32 = 6;
const LIE_VERTICAL_START: i32 = -2;
const SIT_SEARCH_RANGE: i32 = 8;
const RELAX_OWNER_DISTANCE_SQR: f64 = 100.0;
const RELAX_NEAR_SQR: f64 = 2.5;
const RELAX_LIE_AFTER: i32 = 16;
const RELAX_SPACE_INFLATE: f64 = 2.0;
const GIFT_MIN_SLEEP: i32 = 100;
const LOOK_LIMIT: f32 = 45.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CatFacts {
    pub lying: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatAction {
    SetLying(bool),
    SetRelaxStateOne(bool),
    MorningGift,
}

fn relative_air<W: AiWorld>(world: &W, pos: BlockPos) -> bool {
    world.state(pos).is_none_or(StateId::is_air)
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct MoveToBlock {
    speed_modifier: f64,
    search_range: i32,
    vertical_search_range: i32,
    vertical_search_start: i32,
    next_start_tick: i32,
    try_ticks: i32,
    max_stay_ticks: i32,
    block_pos: BlockPos,
    reached_target: bool,
}

impl MoveToBlock {
    fn new(speed_modifier: f64, search_range: i32, vertical_search_range: i32) -> Self {
        Self {
            speed_modifier,
            search_range,
            vertical_search_range,
            vertical_search_start: 0,
            next_start_tick: 0,
            try_ticks: 0,
            max_stay_ticks: 0,
            block_pos: BlockPos::new(0, 0, 0),
            reached_target: false,
        }
    }

    fn can_use<W: AiWorld>(
        &mut self,
        context: &mut AiContext<'_, W>,
        fixed_delay: Option<i32>,
        valid: fn(&W, BlockPos) -> bool,
    ) -> bool {
        if self.next_start_tick > 0 {
            self.next_start_tick -= 1;
            return false;
        }
        self.next_start_tick = fixed_delay.unwrap_or_else(|| {
            reduced_tick_delay(INTERVAL_TICKS + context.brain.random.next_int(INTERVAL_TICKS))
        });
        self.find_nearest_block(context, valid)
    }

    fn can_continue_to_use<W: AiWorld>(
        &self,
        context: &AiContext<'_, W>,
        valid: fn(&W, BlockPos) -> bool,
    ) -> bool {
        self.try_ticks >= -self.max_stay_ticks
            && self.try_ticks <= GIVE_UP_TICKS
            && valid(context.world, self.block_pos)
    }

    fn start<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        let pos = self.block_pos;
        context.move_to_pos(
            Vec3::new(
                f64::from(pos.x) + 0.5,
                f64::from(pos.y + 1),
                f64::from(pos.z) + 0.5,
            ),
            self.speed_modifier,
        );
        self.try_ticks = 0;
        let inner = context.brain.random.next_int(STAY_TICKS) + STAY_TICKS;
        self.max_stay_ticks = context.brain.random.next_int(inner) + STAY_TICKS;
    }

    fn tick<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        let target = self.block_pos.offset(0, 1, 0);
        if closer_to_center_than(target, context.mob.position, 1.0) {
            self.reached_target = true;
            self.try_ticks -= 1;
        } else {
            self.reached_target = false;
            self.try_ticks += 1;
            if self.try_ticks % 40 == 0 {
                context.move_to_pos(
                    Vec3::new(
                        f64::from(target.x) + 0.5,
                        f64::from(target.y),
                        f64::from(target.z) + 0.5,
                    ),
                    self.speed_modifier,
                );
            }
        }
    }

    fn find_nearest_block<W: AiWorld>(
        &mut self,
        context: &AiContext<'_, W>,
        valid: fn(&W, BlockPos) -> bool,
    ) -> bool {
        let origin = context.mob.block_pos();
        let mut y = self.vertical_search_start;
        while y <= self.vertical_search_range {
            for radius in 0..self.search_range {
                let mut x = 0;
                while x <= radius {
                    let mut z = if x < radius && x > -radius { radius } else { 0 };
                    while z <= radius {
                        let pos = origin.offset(x, y - 1, z);
                        if context.mob.is_within_home(pos) && valid(context.world, pos) {
                            self.block_pos = pos;
                            return true;
                        }
                        z = if z > 0 { -z } else { 1 - z };
                    }
                    x = if x > 0 { -x } else { 1 - x };
                }
            }
            y = if y > 0 { -y } else { 1 - y };
        }
        false
    }
}

fn lie_target<W: AiWorld>(world: &W, pos: BlockPos) -> bool {
    relative_air(world, pos.offset(0, 1, 0)) && block_tag(world.state(pos), CATS_CAN_LIE_ON)
}

fn sit_target<W: AiWorld>(world: &W, pos: BlockPos) -> bool {
    if !relative_air(world, pos.offset(0, 1, 0)) {
        return false;
    }
    let state = world.state(pos);
    if !block_tag(state, CATS_CAN_SIT_ON) {
        return false;
    }
    let Some(state) = state else {
        return false;
    };
    if state.kind() == Some(BlockKind::Chest) {
        return world.chest_open_count(pos) < 1;
    }
    if state.kind() == Some(BlockKind::Furnace) {
        return state.property("lit") == Some("true");
    }
    if block_tag(Some(state), BEDS) {
        return state.property("part") != Some("head");
    }
    true
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CatLieOnBlockGoal {
    mover: MoveToBlock,
}

impl CatLieOnBlockGoal {
    pub fn new(speed_modifier: f64, search_range: i32) -> Self {
        let mut mover = MoveToBlock::new(speed_modifier, search_range, LIE_VERTICAL_RANGE);
        mover.vertical_search_start = LIE_VERTICAL_START;
        Self { mover }
    }

    fn can_use<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) -> bool {
        let mob = context.mob;
        mob.tamed
            && !mob.ordered_to_sit
            && !mob.species.cat.lying
            && self
                .mover
                .can_use(context, Some(LIE_RESTART_TICKS), lie_target::<W>)
    }

    fn can_continue_to_use<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) -> bool {
        self.mover.can_continue_to_use(context, lie_target::<W>)
    }

    fn start<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        self.mover.start(context);
        context.act(MobAction::SetSitting(false));
    }

    fn stop<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        context.act(MobAction::Cat(CatAction::SetLying(false)));
    }

    fn tick<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        self.mover.tick(context);
        context.act(MobAction::SetSitting(false));
        if !self.mover.reached_target {
            context.act(MobAction::Cat(CatAction::SetLying(false)));
        } else if !context.mob.species.cat.lying {
            context.act(MobAction::Cat(CatAction::SetLying(true)));
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CatSitOnBlockGoal {
    mover: MoveToBlock,
}

impl CatSitOnBlockGoal {
    pub fn new(speed_modifier: f64) -> Self {
        Self {
            mover: MoveToBlock::new(speed_modifier, SIT_SEARCH_RANGE, 1),
        }
    }

    fn can_use<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) -> bool {
        let mob = context.mob;
        mob.tamed && !mob.ordered_to_sit && self.mover.can_use(context, None, sit_target::<W>)
    }

    fn can_continue_to_use<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) -> bool {
        self.mover.can_continue_to_use(context, sit_target::<W>)
    }

    fn start<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        self.mover.start(context);
        context.act(MobAction::SetSitting(false));
    }

    fn stop<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        context.act(MobAction::SetSitting(false));
    }

    fn tick<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        self.mover.tick(context);
        context.act(MobAction::SetSitting(self.mover.reached_target));
    }
}

fn rot_lerp(from: f32, to: f32, max: f32) -> f32 {
    let delta = mth::wrap_degrees(to - from).clamp(-max, max);
    from + delta
}

fn look_at_entity<W: AiWorld>(
    context: &mut AiContext<'_, W>,
    target: &Living,
    y_max: f32,
    x_max: f32,
) {
    let mob = context.mob;
    let dx = target.position.x - mob.position.x;
    let dz = target.position.z - mob.position.z;
    let dy = target.eye_y() - mob.eye_y();
    let horizontal = (dx * dx + dz * dz).sqrt();
    let pi = f64::from(std::f32::consts::PI);
    let yaw = (mth::atan2(dz, dx) * 180.0 / pi) as f32 - 90.0;
    let pitch = (-(mth::atan2(dy, horizontal) * 180.0 / pi)) as f32;
    context.body.pitch = rot_lerp(context.body.pitch, pitch, x_max);
    context.body.yaw = rot_lerp(context.body.yaw, yaw, y_max);
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct CatRelaxOnOwnerGoal {
    owner: Option<EntityId>,
    goal_pos: Option<BlockPos>,
    on_bed_ticks: i32,
}

impl CatRelaxOnOwnerGoal {
    fn owner<W: AiWorld>(&self, context: &AiContext<'_, W>) -> Option<Living> {
        self.owner.and_then(|id| context.world.living(id))
    }

    fn space_is_occupied<W: AiWorld>(&self, context: &AiContext<'_, W>) -> bool {
        let Some(pos) = self.goal_pos else {
            return false;
        };
        let block = Aabb::new(
            f64::from(pos.x),
            f64::from(pos.y),
            f64::from(pos.z),
            f64::from(pos.x + 1),
            f64::from(pos.y + 1),
            f64::from(pos.z + 1),
        );
        let area = inflate(
            &block,
            RELAX_SPACE_INFLATE,
            RELAX_SPACE_INFLATE,
            RELAX_SPACE_INFLATE,
        );
        context
            .world
            .living_of(TargetKind::Class("Cat"), &area)
            .iter()
            .any(|cat| cat.id != context.mob.id && cat.facts.cat_relaxing)
    }

    fn can_use<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) -> bool {
        let mob = context.mob;
        if !mob.tamed || mob.ordered_to_sit {
            return false;
        }
        let Some(owner) = mob.owner.and_then(|id| context.world.living(id)) else {
            return false;
        };
        if !owner.player {
            return false;
        }
        self.owner = Some(owner.id);
        if !owner.facts.sleeping {
            return false;
        }
        if mob.distance_to_sqr(owner.position) > RELAX_OWNER_DISTANCE_SQR {
            return false;
        }
        let pos = owner.position.block();
        let state = context.world.state(pos);
        if !block_tag(state, CATS_CAN_LIE_ON) {
            return false;
        }
        let behind =
            state
                .and_then(|state| state.property("facing"))
                .and_then(|facing| match facing {
                    "north" => Some((0, 0, 1)),
                    "south" => Some((0, 0, -1)),
                    "west" => Some((1, 0, 0)),
                    "east" => Some((-1, 0, 0)),
                    _ => None,
                });
        self.goal_pos = Some(behind.map_or(pos, |(x, y, z)| pos.offset(x, y, z)));
        !self.space_is_occupied(context)
    }

    fn can_continue_to_use<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) -> bool {
        let mob = context.mob;
        mob.tamed
            && !mob.ordered_to_sit
            && self
                .owner(context)
                .is_some_and(|owner| owner.facts.sleeping)
            && self.goal_pos.is_some()
            && !self.space_is_occupied(context)
    }

    fn go_to_goal<W: AiWorld>(&self, context: &mut AiContext<'_, W>) {
        let Some(pos) = self.goal_pos else {
            return;
        };
        context.act(MobAction::SetSitting(false));
        context.move_to_pos(
            Vec3::new(f64::from(pos.x), f64::from(pos.y), f64::from(pos.z)),
            f64::from(1.1_f32),
        );
    }

    fn start<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        self.go_to_goal(context);
    }

    fn stop<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        context.act(MobAction::Cat(CatAction::SetLying(false)));
        if self
            .owner(context)
            .is_some_and(|owner| owner.facts.sleep_timer >= GIFT_MIN_SLEEP)
        {
            context.act(MobAction::Cat(CatAction::MorningGift));
        }
        self.on_bed_ticks = 0;
        context.act(MobAction::Cat(CatAction::SetRelaxStateOne(false)));
        context.stop_navigation();
    }

    fn tick<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        let (Some(owner), Some(_)) = (self.owner(context), self.goal_pos) else {
            return;
        };
        self.go_to_goal(context);
        if context.mob.distance_to_sqr(owner.position) < RELAX_NEAR_SQR {
            self.on_bed_ticks += 1;
            if self.on_bed_ticks > adjusted_tick_delay(false, RELAX_LIE_AFTER) {
                context.act(MobAction::Cat(CatAction::SetLying(true)));
                context.act(MobAction::Cat(CatAction::SetRelaxStateOne(false)));
            } else {
                look_at_entity(context, &owner, LOOK_LIMIT, LOOK_LIMIT);
                context.act(MobAction::Cat(CatAction::SetRelaxStateOne(true)));
            }
        } else {
            context.act(MobAction::Cat(CatAction::SetLying(false)));
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum CatGoal {
    LieOnBlock(CatLieOnBlockGoal),
    SitOnBlock(CatSitOnBlockGoal),
    RelaxOnOwner(CatRelaxOnOwnerGoal),
}

impl<W: AiWorld> Goal<AiContext<'_, W>> for CatGoal {
    fn flags(&self) -> Flags {
        match self {
            Self::LieOnBlock(_) | Self::SitOnBlock(_) => Flags::of(Flag::Move).with(Flag::Jump),
            Self::RelaxOnOwner(_) => Flags::NONE,
        }
    }

    fn can_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        match self {
            Self::LieOnBlock(goal) => goal.can_use(context),
            Self::SitOnBlock(goal) => goal.can_use(context),
            Self::RelaxOnOwner(goal) => goal.can_use(context),
        }
    }

    fn can_continue_to_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        match self {
            Self::LieOnBlock(goal) => goal.can_continue_to_use(context),
            Self::SitOnBlock(goal) => goal.can_continue_to_use(context),
            Self::RelaxOnOwner(goal) => goal.can_continue_to_use(context),
        }
    }

    fn start(&mut self, context: &mut AiContext<'_, W>) {
        match self {
            Self::LieOnBlock(goal) => goal.start(context),
            Self::SitOnBlock(goal) => goal.start(context),
            Self::RelaxOnOwner(goal) => goal.start(context),
        }
    }

    fn stop(&mut self, context: &mut AiContext<'_, W>) {
        match self {
            Self::LieOnBlock(goal) => goal.stop(context),
            Self::SitOnBlock(goal) => goal.stop(context),
            Self::RelaxOnOwner(goal) => goal.stop(context),
        }
    }

    fn requires_update_every_tick(&self) -> bool {
        matches!(self, Self::LieOnBlock(_) | Self::SitOnBlock(_))
    }

    fn tick(&mut self, context: &mut AiContext<'_, W>) {
        match self {
            Self::LieOnBlock(goal) => goal.tick(context),
            Self::SitOnBlock(goal) => goal.tick(context),
            Self::RelaxOnOwner(goal) => goal.tick(context),
        }
    }
}

pub(crate) fn avoid_players() -> MobGoal {
    MobGoal::AvoidEntity(AvoidEntityGoal::unless_tame(
        TargetKind::Player,
        AVOID_DISTANCE,
        WALK_SPEED,
        SPRINT_SPEED,
    ))
}

pub(crate) const AVOID_PRIORITY: i32 = 4;

pub(crate) fn register(add: &mut Goals<'_>, _kind: EntityKind) {
    add.float(1);
    add.goal(1, MobGoal::Panic(PanicGoal::tamable(1.5)));
    add.goal(2, MobGoal::SitWhenOrdered(SitWhenOrderedToGoal));
    add.goal(
        3,
        MobGoal::Cat(CatGoal::RelaxOnOwner(CatRelaxOnOwnerGoal::default())),
    );
    add.goal(
        4,
        MobGoal::Tempt(TemptGoal::cat(TEMPT_SPEED, TemptItems::Tag(CAT_FOOD))),
    );
    add.goal(
        5,
        MobGoal::Cat(CatGoal::LieOnBlock(CatLieOnBlockGoal::new(
            1.1,
            LIE_SEARCH_RANGE,
        ))),
    );
    add.goal(
        6,
        MobGoal::FollowOwner(FollowOwnerGoal::new(1.0, 10.0, 5.0)),
    );
    add.goal(
        7,
        MobGoal::Cat(CatGoal::SitOnBlock(CatSitOnBlockGoal::new(WALK_SPEED))),
    );
    add.goal(8, MobGoal::LeapAtTarget(LeapAtTargetGoal::new(0.3)));
    add.goal(9, MobGoal::OcelotAttack(OcelotAttackGoal::default()));
    add.goal(10, MobGoal::Breed(crate::animal::BreedGoal::new(0.8)));
    add.stroll_rarely_in_water(11, WALK_SPEED, STROLL_WATER_PROBABILITY);
    add.look(12, 10.0);
    add.target(
        1,
        NearestAttackableTargetGoal::non_tame(
            TargetKind::Class("Rabbit"),
            false,
            TargetFilter::Any,
        ),
    );
    add.target(
        1,
        NearestAttackableTargetGoal::non_tame(TargetKind::Turtle, false, TargetFilter::BabyOnLand),
    );
    add.goal(AVOID_PRIORITY, avoid_players());
}
