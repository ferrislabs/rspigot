use rspigot_blocks::StateFlags;
use rspigot_domain::entity::physics::has_tag;
use rspigot_domain::{BlockPos, EntityId, Vec3, mth};
use rspigot_entities::EntityKind;

use crate::brain::AiContext;
use crate::classify::is_pathfindable;
use crate::goal::{Flag, Flags, Goal, adjusted_tick_delay, reduced_tick_delay};
use crate::goals::RandomStrollGoal;
use crate::random::{MobRandom, RandomSource};
use crate::scan::closer_to_center_than;
use crate::targeting::TargetingConditions;
use crate::world::{AiWorld, Living, TargetKind, inflate};
use crate::{MobAction, random_pos};

pub const FISH_SWIM_SPEED: f64 = 1.0;
pub const FISH_SWIM_INTERVAL: i32 = 40;
pub const DOLPHIN_SWIM_INTERVAL: i32 = 10;
pub const SWIM_HORIZONTAL: i32 = 10;
pub const SWIM_VERTICAL: i32 = 7;
pub const SCHOOL_AREA: f64 = 8.0;
pub const SCHOOL_START_BASE: i32 = 200;
pub const SCHOOL_START_SPREAD: i32 = 200;
pub const SCHOOL_START_MODULO: i32 = 20;
pub const SCHOOL_RANGE_SQR: f64 = 121.0;
pub const SCHOOL_PATH_INTERVAL: i32 = 10;
pub const SCHOOL_PATH_SPEED: f64 = 1.0;
pub const PUFF_ALARM_RANGE: f64 = 2.0;
pub const SQUID_IDLE_TICKS: i32 = 100;
pub const SQUID_MOVE_ODDS: i32 = 50;
pub const SQUID_HORIZONTAL_SPEED: f32 = 0.2;
pub const SQUID_VERTICAL_BASE: f32 = -0.1;
pub const SQUID_FLEE_RANGE_SQR: f64 = 100.0;
pub const SQUID_FLEE_PUSH: f64 = 3.0;
pub const SQUID_FLEE_FALLOFF_START: f64 = 5.0;
pub const SQUID_FLEE_DIVISOR: f64 = 20.0;
pub const AIR_THRESHOLD: i32 = 140;
pub const AIR_SEARCH_HEIGHT: i32 = 8;
pub const AIR_ASSIST_SPEED: f32 = 0.02;
pub const LIQUID_SEARCH: f64 = 2.0;
pub const TREASURE_AIR: i32 = 100;
pub const TREASURE_CLOSE: f64 = 4.0;
pub const TREASURE_NEAR: f64 = 12.0;
pub const TREASURE_SPEED: f64 = 1.3;
pub const TREASURE_EVENT_ODDS: i32 = 80;
pub const TREASURE_EVENT: i8 = 38;
pub const TREASURE_RADIUS: i32 = 50;
pub const PLAYER_RANGE_SQR: f64 = 256.0;
pub const PLAYER_CLOSE_SQR: f64 = 6.25;
pub const PLAYER_TARGET_RANGE: f64 = 10.0;
pub const GRACE_ODDS: i32 = 6;
pub const GRACE_TICKS: i32 = 100;
pub const HEAD_YAW_OFFSET: f32 = 20.0;
pub const DOLPHIN_MAX_HEAD_ROT: f32 = 1.0;
pub const JUMP_STEPS: [i32; 6] = [0, 1, 4, 5, 6, 7];
pub const JUMP_PUSH: f64 = 0.6;
pub const JUMP_LIFT: f64 = 0.7;
pub const JUMP_FLAT_SPEED_SQR: f64 = 0.03_f32 as f64;
pub const JUMP_LEVEL_ANGLE: f32 = 10.0;
pub const JUMP_BLOCKS: &str = "minecraft:blocks_dolphin_jump";
pub const ITEM_AREA: f64 = 8.0;
pub const ITEM_SPEED: f64 = 1.2_f32 as f64;
pub const ITEM_COOLDOWN_ODDS: i32 = 100;
pub const ITEM_PICKUP_DELAY: i32 = 40;
pub const ITEM_EYE_DROP: f64 = 0.3_f32 as f64;
pub const ITEM_THROW: f32 = 0.3;
pub const ITEM_LIFT: f32 = 1.5;
pub const ITEM_SPREAD: f32 = 0.02;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FishFacts {
    pub leader: Option<EntityId>,
    pub school_size: i32,
    pub inflate_counter: i32,
    pub deflate_timer: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SquidFacts {
    pub movement: Vec3,
}

impl SquidFacts {
    pub fn has_movement_vector(&self) -> bool {
        self.movement.length_sqr() > f64::from(1.0e-5_f32)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DolphinFacts {
    pub got_fish: bool,
    pub treasure_pos: Option<BlockPos>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaterAction {
    Recruit {
        leader: EntityId,
        follower: EntityId,
    },
    StopFollowing,
    BreathAssist,
    DropHeldItem,
    GiveDolphinsGrace(EntityId),
}

fn act<W: AiWorld>(context: &mut AiContext<'_, W>, action: WaterAction) {
    context.act(MobAction::Water(action));
}

fn has_water(world: &impl AiWorld, pos: BlockPos) -> bool {
    world
        .state(pos)
        .is_some_and(|state| state.flags().contains(StateFlags::WATER))
}

fn is_follower(world: &impl AiWorld, leader: Option<EntityId>) -> bool {
    leader.is_some_and(|id| world.living(id).is_some_and(|living| living.alive))
}

fn school_class(kind: EntityKind) -> &'static str {
    kind.ancestors().first().copied().unwrap_or("Mob")
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RandomSwimmingGoal {
    stroll: RandomStrollGoal,
    fish: bool,
}

impl RandomSwimmingGoal {
    pub fn fish() -> Self {
        Self {
            stroll: RandomStrollGoal::with_interval(FISH_SWIM_SPEED, FISH_SWIM_INTERVAL),
            fish: true,
        }
    }

    pub fn dolphin() -> Self {
        Self {
            stroll: RandomStrollGoal::with_interval(FISH_SWIM_SPEED, DOLPHIN_SWIM_INTERVAL),
            fish: false,
        }
    }
}

impl<W: AiWorld> Goal<AiContext<'_, W>> for RandomSwimmingGoal {
    fn flags(&self) -> Flags {
        Flags::of(Flag::Move)
    }

    fn can_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        if self.fish
            && context.mob.kind.extends("AbstractSchoolingFish")
            && is_follower(context.world, context.mob.species.fish.leader)
        {
            return false;
        }
        self.stroll.can_use_with(context, |context| {
            random_pos::swimmable_pos(context, SWIM_HORIZONTAL, SWIM_VERTICAL)
        })
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FollowFlockLeaderGoal {
    next_start_tick: i32,
    time_to_recalc_path: i32,
}

fn next_start_tick(random: &mut MobRandom) -> i32 {
    reduced_tick_delay(
        random.next_int(SCHOOL_START_SPREAD) % SCHOOL_START_MODULO + SCHOOL_START_BASE,
    )
}

impl FollowFlockLeaderGoal {
    pub fn new(random: &mut MobRandom) -> Self {
        Self {
            next_start_tick: next_start_tick(random),
            time_to_recalc_path: 0,
        }
    }
}

fn school_follows<W: AiWorld>(world: &W, fish: &Living) -> bool {
    is_follower(world, fish.facts.school_leader)
}

fn school_can_be_followed<W: AiWorld>(world: &W, fish: &Living) -> bool {
    let size = fish.facts.school_size;
    let max = rspigot_domain::entity::species::fish::max_school_size(fish.kind);
    let _ = world;
    size > 1 && size < max
}

impl<W: AiWorld> Goal<AiContext<'_, W>> for FollowFlockLeaderGoal {
    fn flags(&self) -> Flags {
        Flags::NONE
    }

    fn can_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        let facts = context.mob.species.fish;
        if facts.school_size > 1 {
            return false;
        }
        if is_follower(context.world, facts.leader) {
            return true;
        }
        if self.next_start_tick > 0 {
            self.next_start_tick -= 1;
            return false;
        }
        self.next_start_tick = next_start_tick(&mut context.brain.random);
        let kind = context.mob.kind;
        let area = inflate(&context.mob.bounds(), SCHOOL_AREA, SCHOOL_AREA, SCHOOL_AREA);
        let world = context.world;
        let nearby: Vec<Living> = world
            .living_of(TargetKind::Class(school_class(kind)), &area)
            .into_iter()
            .filter(|fish| school_can_be_followed(world, fish) || !school_follows(world, fish))
            .collect();
        let free: Vec<&Living> = nearby
            .iter()
            .filter(|fish| !school_follows(world, fish))
            .collect();
        let (leader, leader_size) = match free.first() {
            Some(first) => (first.id, first.facts.school_size),
            None => (context.mob.id, facts.school_size),
        };
        let room = (rspigot_domain::entity::species::fish::max_school_size(kind) - leader_size)
            .max(0) as usize;
        let mut joins = false;
        for fish in free.iter().take(room).filter(|fish| fish.id != leader) {
            joins |= fish.id == context.mob.id;
            act(
                context,
                WaterAction::Recruit {
                    leader,
                    follower: fish.id,
                },
            );
        }
        joins
    }

    fn can_continue_to_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        let leader = context.mob.species.fish.leader;
        is_follower(context.world, leader)
            && leader
                .and_then(|id| context.world.living(id))
                .is_some_and(|leader| {
                    context.mob.position.distance_squared(leader.position) <= SCHOOL_RANGE_SQR
                })
    }

    fn start(&mut self, _context: &mut AiContext<'_, W>) {
        self.time_to_recalc_path = 0;
    }

    fn stop(&mut self, context: &mut AiContext<'_, W>) {
        act(context, WaterAction::StopFollowing);
    }

    fn tick(&mut self, context: &mut AiContext<'_, W>) {
        self.time_to_recalc_path -= 1;
        if self.time_to_recalc_path >= 1 {
            return;
        }
        self.time_to_recalc_path = adjusted_tick_delay(false, SCHOOL_PATH_INTERVAL);
        let leader = context.mob.species.fish.leader;
        if !is_follower(context.world, leader) {
            return;
        }
        if let Some(leader) = leader.and_then(|id| context.world.living(id)) {
            context.move_to_entity(&leader, SCHOOL_PATH_SPEED);
        }
    }
}

pub fn is_scary_kind_to_pufferfish(kind: EntityKind) -> bool {
    !matches!(
        kind,
        EntityKind::Turtle
            | EntityKind::Guardian
            | EntityKind::ElderGuardian
            | EntityKind::Cod
            | EntityKind::Pufferfish
            | EntityKind::Salmon
            | EntityKind::TropicalFish
            | EntityKind::Dolphin
            | EntityKind::Squid
            | EntityKind::GlowSquid
            | EntityKind::Tadpole
            | EntityKind::Nautilus
            | EntityKind::ZombieNautilus
            | EntityKind::SulfurCube
    )
}

pub fn is_scary_to_pufferfish(target: &Living) -> bool {
    if target.player && target.creative {
        return false;
    }
    is_scary_kind_to_pufferfish(target.kind)
}

pub fn pufferfish_conditions() -> TargetingConditions {
    TargetingConditions::for_non_combat()
        .ignore_invisibility_testing()
        .ignore_line_of_sight()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PufferfishPuffGoal;

impl<W: AiWorld> Goal<AiContext<'_, W>> for PufferfishPuffGoal {
    fn flags(&self) -> Flags {
        Flags::NONE
    }

    fn can_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        let area = inflate(
            &context.mob.bounds(),
            PUFF_ALARM_RANGE,
            PUFF_ALARM_RANGE,
            PUFF_ALARM_RANGE,
        );
        let world = context.world;
        let candidates = world.living_of(TargetKind::Class("LivingEntity"), &area);
        candidates
            .iter()
            .any(|target| pufferfish_conditions().test(context, target, is_scary_to_pufferfish))
    }

    fn start(&mut self, context: &mut AiContext<'_, W>) {
        context.body.fish.inflate_counter = 1;
        context.body.fish.deflate_timer = 0;
    }

    fn stop(&mut self, context: &mut AiContext<'_, W>) {
        context.body.fish.inflate_counter = 0;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SquidRandomMovementGoal;

impl<W: AiWorld> Goal<AiContext<'_, W>> for SquidRandomMovementGoal {
    fn flags(&self) -> Flags {
        Flags::NONE
    }

    fn can_use(&mut self, _context: &mut AiContext<'_, W>) -> bool {
        true
    }

    fn tick(&mut self, context: &mut AiContext<'_, W>) {
        if context.mob.no_action_time > SQUID_IDLE_TICKS {
            context.body.squid.movement = Vec3::ZERO;
            return;
        }
        let roll = context
            .brain
            .random
            .next_int(reduced_tick_delay(SQUID_MOVE_ODDS));
        if roll != 0 && context.mob.fluid.in_water && context.body.squid.has_movement_vector() {
            return;
        }
        let angle = context.brain.random.next_float() * std::f32::consts::TAU;
        let x = mth::cos(f64::from(angle)) * SQUID_HORIZONTAL_SPEED;
        let y = SQUID_VERTICAL_BASE + context.brain.random.next_float() * SQUID_HORIZONTAL_SPEED;
        let z = mth::sin(f64::from(angle)) * SQUID_HORIZONTAL_SPEED;
        context.body.squid.movement = Vec3::new(f64::from(x), f64::from(y), f64::from(z));
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SquidFleeGoal {
    flee_ticks: i32,
}

impl<W: AiWorld> Goal<AiContext<'_, W>> for SquidFleeGoal {
    fn flags(&self) -> Flags {
        Flags::NONE
    }

    fn can_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        let Some(attacker) = context
            .mob
            .last_hurt_by_mob
            .and_then(|id| context.world.living(id))
        else {
            return false;
        };
        context.mob.fluid.in_water
            && context.mob.position.distance_squared(attacker.position) < SQUID_FLEE_RANGE_SQR
    }

    fn start(&mut self, _context: &mut AiContext<'_, W>) {
        self.flee_ticks = 0;
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, context: &mut AiContext<'_, W>) {
        self.flee_ticks += 1;
        let Some(attacker) = context
            .mob
            .last_hurt_by_mob
            .and_then(|id| context.world.living(id))
        else {
            return;
        };
        let mut flee = context.mob.position - attacker.position;
        let target = (context.mob.position + flee).block();
        let state = context.world.state(target);
        let water = state.is_some_and(|state| state.flags().contains(StateFlags::WATER));
        let air = state.is_some_and(rspigot_blocks::StateId::is_air);
        if !water && !air {
            return;
        }
        let length = flee.length_sqr().sqrt();
        if length > 0.0 {
            let mut power = SQUID_FLEE_PUSH;
            if length > SQUID_FLEE_FALLOFF_START {
                power -= (length - SQUID_FLEE_FALLOFF_START) / SQUID_FLEE_FALLOFF_START;
            }
            if power > 0.0 {
                flee = flee.scale(power);
            }
        }
        if air {
            flee = Vec3::new(flee.x, 0.0, flee.z);
        }
        context.body.squid.movement = Vec3::new(
            flee.x / SQUID_FLEE_DIVISOR,
            flee.y / SQUID_FLEE_DIVISOR,
            flee.z / SQUID_FLEE_DIVISOR,
        );
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BreathAirGoal;

fn gives_air(world: &impl AiWorld, pos: BlockPos) -> bool {
    let Some(state) = world.state(pos) else {
        return false;
    };
    let fluid_free = !state.flags().has_fluid();
    let bubbles = state.kind() == Some(rspigot_blocks::BlockKind::BubbleColumn);
    (fluid_free || bubbles) && is_pathfindable(state)
}

fn neighbor_column(origin: BlockPos, max_y: i32) -> impl Iterator<Item = BlockPos> {
    const SIDES: [(i32, i32); 5] = [(0, 0), (0, -1), (1, 0), (0, 1), (-1, 0)];
    let direction = if max_y > origin.y { 1 } else { -1 };
    let height = (max_y - origin.y).abs() + 1;
    (0..SIDES.len() as i32 * height).map(move |index| {
        let (dx, dz) = SIDES[(index / height) as usize];
        BlockPos::new(
            origin.x + dx,
            origin.y + (index % height) * direction,
            origin.z + dz,
        )
    })
}

fn find_air_position<W: AiWorld>(context: &mut AiContext<'_, W>) {
    let origin = context.mob.block_pos();
    let world = context.world;
    let found = neighbor_column(origin, origin.y + AIR_SEARCH_HEIGHT)
        .find(|pos| gives_air(world, *pos))
        .unwrap_or_else(|| {
            Vec3::new(
                context.mob.position.x,
                context.mob.position.y + f64::from(AIR_SEARCH_HEIGHT),
                context.mob.position.z,
            )
            .block()
        });
    let target = Vec3::new(
        f64::from(found.x),
        f64::from(found.y + 1),
        f64::from(found.z),
    );
    context.move_to_pos(target, 1.0);
}

impl<W: AiWorld> Goal<AiContext<'_, W>> for BreathAirGoal {
    fn flags(&self) -> Flags {
        Flags::of(Flag::Move).with(Flag::Look)
    }

    fn can_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        context.mob.air_supply < AIR_THRESHOLD
    }

    fn is_interruptable(&self) -> bool {
        false
    }

    fn start(&mut self, context: &mut AiContext<'_, W>) {
        context.stop_navigation();
        find_air_position(context);
    }

    fn tick(&mut self, context: &mut AiContext<'_, W>) {
        find_air_position(context);
        act(context, WaterAction::BreathAssist);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TryFindWaterGoal;

impl<W: AiWorld> Goal<AiContext<'_, W>> for TryFindWaterGoal {
    fn flags(&self) -> Flags {
        Flags::NONE
    }

    fn can_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        context.mob.on_ground && !has_water(context.world, context.mob.block_pos())
    }

    fn start(&mut self, context: &mut AiContext<'_, W>) {
        let position = context.mob.position;
        let (x0, y0, z0) = (
            (position.x - LIQUID_SEARCH).floor() as i32,
            (position.y - LIQUID_SEARCH).floor() as i32,
            (position.z - LIQUID_SEARCH).floor() as i32,
        );
        let (x1, y1, z1) = (
            (position.x + LIQUID_SEARCH).floor() as i32,
            context.mob.block_pos().y,
            (position.z + LIQUID_SEARCH).floor() as i32,
        );
        let world = context.world;
        let found = (z0..=z1)
            .flat_map(|z| (y0..=y1).flat_map(move |y| (x0..=x1).map(move |x| (x, y, z))))
            .find(|(x, y, z)| has_water(world, BlockPos::new(*x, *y, *z)));
        if let Some((x, y, z)) = found {
            context
                .brain
                .movement
                .set_wanted_position(Vec3::new(f64::from(x), f64::from(y), f64::from(z)), 1.0);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DolphinSwimToTreasureGoal {
    stuck: bool,
}

fn treasure_reached<W: AiWorld>(context: &AiContext<'_, W>, treasure: BlockPos) -> bool {
    let level = BlockPos::new(
        treasure.x,
        context.mob.position.y.floor() as i32,
        treasure.z,
    );
    closer_to_center_than(level, context.mob.position, TREASURE_CLOSE)
}

fn close_to_next_pos<W: AiWorld>(context: &AiContext<'_, W>) -> bool {
    context
        .brain
        .navigation
        .target_pos()
        .is_some_and(|pos| closer_to_center_than(pos, context.mob.position, TREASURE_NEAR))
}

impl<W: AiWorld> Goal<AiContext<'_, W>> for DolphinSwimToTreasureGoal {
    fn flags(&self) -> Flags {
        Flags::of(Flag::Move).with(Flag::Look)
    }

    fn is_interruptable(&self) -> bool {
        false
    }

    fn can_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        context.body.dolphin.got_fish && context.mob.air_supply >= TREASURE_AIR
    }

    fn can_continue_to_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        let Some(treasure) = context.body.dolphin.treasure_pos else {
            return false;
        };
        !treasure_reached(context, treasure)
            && !self.stuck
            && context.mob.air_supply >= TREASURE_AIR
    }

    fn start(&mut self, context: &mut AiContext<'_, W>) {
        self.stuck = false;
        context.stop_navigation();
        match context
            .world
            .nearest_dolphin_structure(context.mob.block_pos(), TREASURE_RADIUS)
        {
            None => self.stuck = true,
            Some(pos) => {
                context.body.dolphin.treasure_pos = Some(pos);
                context.act(MobAction::EntityEvent(TREASURE_EVENT));
            }
        }
    }

    fn stop(&mut self, context: &mut AiContext<'_, W>) {
        if let Some(treasure) = context.body.dolphin.treasure_pos
            && !treasure_reached(context, treasure)
            && !self.stuck
        {
            return;
        }
        context.body.dolphin.got_fish = false;
    }

    fn tick(&mut self, context: &mut AiContext<'_, W>) {
        let Some(treasure) = context.body.dolphin.treasure_pos else {
            return;
        };
        if !close_to_next_pos(context) && !context.brain.navigation.is_done() {
            return;
        }
        let center = treasure.center();
        let mut position =
            random_pos::pos_towards(context, 16, 1, center, std::f64::consts::FRAC_PI_8);
        if position.is_none() {
            position = random_pos::pos_towards(context, 8, 4, center, random_pos::HALF_PI);
        }
        if let Some(found) = position {
            let block = found.block();
            let water = has_water(context.world, block)
                && crate::swim::is_water_pathfindable(context.world.state(block));
            if !water {
                position = random_pos::pos_towards(context, 8, 5, center, random_pos::HALF_PI);
            }
        }
        let Some(position) = position else {
            self.stuck = true;
            return;
        };
        context.brain.look.set_look_at(
            position,
            DOLPHIN_MAX_HEAD_ROT + HEAD_YAW_OFFSET,
            DOLPHIN_MAX_HEAD_ROT,
        );
        context.move_to_pos(position, TREASURE_SPEED);
        let bound = adjusted_tick_delay(false, TREASURE_EVENT_ODDS);
        if context.brain.random.next_int(bound) == 0 {
            context.act(MobAction::EntityEvent(TREASURE_EVENT));
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DolphinSwimWithPlayerGoal {
    speed_modifier: f64,
    player: Option<EntityId>,
}

impl DolphinSwimWithPlayerGoal {
    pub const fn new(speed_modifier: f64) -> Self {
        Self {
            speed_modifier,
            player: None,
        }
    }
}

fn swimming_player<W: AiWorld>(context: &AiContext<'_, W>, id: EntityId) -> Option<Living> {
    context
        .world
        .living(id)
        .filter(|player| player.facts.swimming)
}

impl<W: AiWorld> Goal<AiContext<'_, W>> for DolphinSwimWithPlayerGoal {
    fn flags(&self) -> Flags {
        Flags::of(Flag::Move).with(Flag::Look)
    }

    fn can_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        let conditions = TargetingConditions::for_non_combat()
            .range(PLAYER_TARGET_RANGE)
            .ignore_line_of_sight();
        let origin = context.mob.eye_position();
        self.player =
            crate::targeting::nearest_player(context, conditions, |_| true, origin).map(|p| p.id);
        let Some(player) = self.player else {
            return false;
        };
        swimming_player(context, player).is_some() && context.brain.target != Some(player)
    }

    fn can_continue_to_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        self.player
            .and_then(|id| swimming_player(context, id))
            .is_some_and(|player| {
                context.mob.position.distance_squared(player.position) < PLAYER_RANGE_SQR
            })
    }

    fn start(&mut self, context: &mut AiContext<'_, W>) {
        if let Some(player) = self.player {
            act(context, WaterAction::GiveDolphinsGrace(player));
        }
    }

    fn stop(&mut self, context: &mut AiContext<'_, W>) {
        self.player = None;
        context.stop_navigation();
    }

    fn tick(&mut self, context: &mut AiContext<'_, W>) {
        let Some(player) = self.player.and_then(|id| context.world.living(id)) else {
            return;
        };
        context.brain.look.set_look_at(
            Vec3::new(player.position.x, player.eye_y(), player.position.z),
            DOLPHIN_MAX_HEAD_ROT + HEAD_YAW_OFFSET,
            DOLPHIN_MAX_HEAD_ROT,
        );
        if context.mob.position.distance_squared(player.position) >= PLAYER_CLOSE_SQR {
            context.move_to_entity(&player, self.speed_modifier);
        } else {
            context.stop_navigation();
        }
        if player.facts.swimming && context.brain.random.next_int(GRACE_ODDS) == 0 {
            act(context, WaterAction::GiveDolphinsGrace(player.id));
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DolphinJumpGoal {
    interval: i32,
    breached: bool,
}

impl DolphinJumpGoal {
    pub const fn new(interval: i32) -> Self {
        Self {
            interval: reduced_tick_delay(interval),
            breached: false,
        }
    }
}

pub fn motion_direction(yaw: f32) -> (i32, i32) {
    match ((yaw / 90.0 + 0.5).floor() as i32) & 3 {
        0 => (0, 1),
        1 => (-1, 0),
        2 => (0, -1),
        _ => (1, 0),
    }
}

fn jump_water_clear(world: &impl AiWorld, pos: BlockPos, step: (i32, i32), at: i32) -> bool {
    let target = pos.offset(step.0 * at, 0, step.1 * at);
    has_water(world, target)
        && !world
            .state(target)
            .is_some_and(|state| has_tag(state, JUMP_BLOCKS))
}

fn jump_surface_clear(world: &impl AiWorld, pos: BlockPos, step: (i32, i32), at: i32) -> bool {
    let air = |up: i32| {
        world
            .state(pos.offset(step.0 * at, up, step.1 * at))
            .is_some_and(rspigot_blocks::StateId::is_air)
    };
    air(1) && air(2)
}

impl<W: AiWorld> Goal<AiContext<'_, W>> for DolphinJumpGoal {
    fn flags(&self) -> Flags {
        Flags::NONE
    }

    fn can_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        if context.brain.random.next_int(self.interval) != 0 {
            return false;
        }
        let step = motion_direction(context.mob.yaw);
        let pos = context.mob.block_pos();
        let world = context.world;
        JUMP_STEPS.iter().all(|at| {
            jump_water_clear(world, pos, step, *at) && jump_surface_clear(world, pos, step, *at)
        })
    }

    fn can_continue_to_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        let vertical = context.body.velocity.y;
        let pitch = context.body.pitch;
        let level = vertical * vertical < JUMP_FLAT_SPEED_SQR
            && pitch != 0.0
            && pitch.abs() < JUMP_LEVEL_ANGLE
            && context.mob.fluid.in_water;
        !level && !context.mob.on_ground
    }

    fn is_interruptable(&self) -> bool {
        false
    }

    fn start(&mut self, context: &mut AiContext<'_, W>) {
        let step = motion_direction(context.mob.yaw);
        context.body.velocity = context.body.velocity
            + Vec3::new(
                f64::from(step.0) * JUMP_PUSH,
                JUMP_LIFT,
                f64::from(step.1) * JUMP_PUSH,
            );
        context.stop_navigation();
    }

    fn stop(&mut self, context: &mut AiContext<'_, W>) {
        context.body.pitch = 0.0;
    }

    fn tick(&mut self, context: &mut AiContext<'_, W>) {
        let was_breached = self.breached;
        if !self.breached {
            self.breached = has_water(context.world, context.mob.block_pos());
        }
        if self.breached && !was_breached {
            context.act(MobAction::Sound {
                sound: "minecraft:entity.dolphin.jump",
                volume: 1.0,
                pitch: 1.0,
            });
        }
        let velocity = context.body.velocity;
        let pitch = context.body.pitch;
        if velocity.y * velocity.y < JUMP_FLAT_SPEED_SQR && pitch != 0.0 {
            context.body.pitch = pitch + 0.2 * mth::degrees_difference(pitch, 0.0);
        } else if velocity.length_sqr().sqrt() > f64::from(1.0e-5_f32) {
            let horizontal = velocity.horizontal_distance_sqr().sqrt();
            let angle = mth::atan2(-velocity.y, horizontal) * mth::RAD_TO_DEG;
            context.body.pitch = angle as f32;
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DolphinMoveToItemGoal {
    cooldown: i32,
}

fn dolphin_items<W: AiWorld>(context: &AiContext<'_, W>) -> Vec<crate::DroppedItem> {
    let area = inflate(&context.mob.bounds(), ITEM_AREA, ITEM_AREA, ITEM_AREA);
    context
        .world
        .items_in(&area)
        .into_iter()
        .filter(|item| {
            !item.has_pickup_delay && item.alive && has_water(context.world, item.position.block())
        })
        .collect()
}

fn move_to_first_item<W: AiWorld>(context: &mut AiContext<'_, W>) -> bool {
    let items = dolphin_items(context);
    let Some(first) = items.first() else {
        return false;
    };
    context.move_to_entity_at(first.position, ITEM_SPEED);
    true
}

fn drop_held_item<W: AiWorld>(context: &mut AiContext<'_, W>) -> bool {
    if context.mob.main_hand.is_none() {
        return false;
    }
    act(context, WaterAction::DropHeldItem);
    true
}

impl<W: AiWorld> Goal<AiContext<'_, W>> for DolphinMoveToItemGoal {
    fn flags(&self) -> Flags {
        Flags::of(Flag::Move)
    }

    fn can_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        if context.mob.tick_count < self.cooldown {
            return false;
        }
        !dolphin_items(context).is_empty()
    }

    fn start(&mut self, context: &mut AiContext<'_, W>) {
        if move_to_first_item(context) {
            context.act(MobAction::Sound {
                sound: "minecraft:entity.dolphin.play",
                volume: 1.0,
                pitch: 1.0,
            });
        }
        self.cooldown = 0;
    }

    fn stop(&mut self, context: &mut AiContext<'_, W>) {
        if drop_held_item(context) {
            self.cooldown =
                context.mob.tick_count + context.brain.random.next_int(ITEM_COOLDOWN_ODDS);
        }
    }

    fn tick(&mut self, context: &mut AiContext<'_, W>) {
        move_to_first_item(context);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DolphinPlayWithItemsGoal;

impl<W: AiWorld> Goal<AiContext<'_, W>> for DolphinPlayWithItemsGoal {
    fn flags(&self) -> Flags {
        Flags::NONE
    }

    fn can_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        context.mob.main_hand.is_some()
    }

    fn stop(&mut self, context: &mut AiContext<'_, W>) {
        drop_held_item(context);
    }

    fn tick(&mut self, context: &mut AiContext<'_, W>) {
        drop_held_item(context);
    }
}

pub fn thrown_item_velocity(yaw: f32, pitch: f32, angle: f32, spread: f32) -> Vec3 {
    let yaw_radians = f64::from(yaw * mth::DEG_TO_RAD);
    let pitch_radians = f64::from(pitch * mth::DEG_TO_RAD);
    let spread = spread * ITEM_SPREAD;
    let x = -mth::sin(yaw_radians) * ITEM_THROW * mth::cos(pitch_radians)
        + mth::cos(f64::from(angle)) * spread;
    let y = mth::sin(pitch_radians) * ITEM_THROW * ITEM_LIFT;
    let z = mth::cos(yaw_radians) * ITEM_THROW * mth::cos(pitch_radians)
        + mth::sin(f64::from(angle)) * spread;
    Vec3::new(f64::from(x), f64::from(y), f64::from(z))
}

macro_rules! water_goals {
    ($($variant:ident($ty:ty)),+ $(,)?) => {
        goal_enum!(WaterGoal { $($variant($ty)),+ });
    };
}

water_goals!(
    RandomSwimming(RandomSwimmingGoal),
    FollowFlockLeader(FollowFlockLeaderGoal),
    PufferfishPuff(PufferfishPuffGoal),
    SquidRandomMovement(SquidRandomMovementGoal),
    SquidFlee(SquidFleeGoal),
    BreathAir(BreathAirGoal),
    TryFindWater(TryFindWaterGoal),
    DolphinSwimToTreasure(DolphinSwimToTreasureGoal),
    DolphinSwimWithPlayer(DolphinSwimWithPlayerGoal),
    DolphinJump(DolphinJumpGoal),
    DolphinMoveToItem(DolphinMoveToItemGoal),
    DolphinPlayWithItems(DolphinPlayWithItemsGoal),
);
