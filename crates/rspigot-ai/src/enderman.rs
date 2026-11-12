use rspigot_blocks::{StateFlags, StateId};
use rspigot_domain::entity::Aabb;
use rspigot_domain::entity::physics::has_tag;
use rspigot_domain::mth::view_vector;
use rspigot_domain::{BlockPos, EntityId, Vec3, mth};
use rspigot_entities::EntityKind;

use crate::action::{MobAction, TargetReason};
use crate::brain::AiContext;
use crate::combat::{TargetMemory, look_at_entity};
use crate::goal::{Flag, Flags, Goal, adjusted_tick_delay, reduced_tick_delay};
use crate::goals::{MobGoal, NearestAttackableTargetGoal};
use crate::holdable;
use crate::path_type::PathType;
use crate::random::RandomSource;
use crate::registry::Goals;
use crate::targeting::{self, TargetingConditions};
use crate::view::{collides, is_air, is_water};
use crate::world::{AiWorld, Living, MobView, TargetKind};
use crate::{BlockView, MeleeAttackGoal, ResetUniversalAngerGoal};

pub const STARE_CONE: f64 = 0.025;
pub const FREEZE_RANGE_SQR: f64 = 256.0;
pub const TELEPORT_RADIUS: f64 = 64.0;
pub const TELEPORT_VERTICAL: i32 = 64;
pub const TELEPORT_TOWARDS_SPREAD: f64 = 8.0;
pub const TELEPORT_TOWARDS_DISTANCE: f64 = 16.0;
pub const TELEPORT_TOWARDS_VERTICAL: i32 = 16;
pub const TELEPORT_EVENT: i8 = 46;
const SIGHT_LIMIT: f64 = 128.0;
const WORLD_BORDER: f64 = 29_999_984.0;
const TELEPORT_TO_TAG: &str = "minecraft:entities_can_teleport_to";
const NO_TELEPORT_TAG: &str = "minecraft:enderman_does_not_teleport_to";
const HOLDABLE_TAG: &str = "minecraft:enderman_holdable";
const AGGRO_DELAY: i32 = 5;
const TELEPORT_DELAY: i32 = 30;
const LOOK_SPEED: f32 = 10.0;
const NEAR_TELEPORT_SQR: f64 = 16.0;
const LEAVE_INTERVAL: i32 = 2000;
const TAKE_INTERVAL: i32 = 20;

pub fn is_looking_at_me(
    mob: &MobView,
    world: &impl AiWorld,
    target: &Living,
    cone: f64,
    gaze_height: f64,
) -> bool {
    let view = view_vector(target.pitch, target.yaw).normalize();
    let eye = target.eye_y();
    let delta = Vec3::new(
        mob.position.x - target.position.x,
        gaze_height - eye,
        mob.position.z - target.position.z,
    );
    let distance = delta.length();
    let delta = delta.normalize();
    let dot = view.x * delta.x + view.y * delta.y + view.z * delta.z;
    if dot <= 1.0 - cone / distance {
        return false;
    }
    let from = Vec3::new(target.position.x, eye, target.position.z);
    let to = Vec3::new(mob.position.x, gaze_height, mob.position.z);
    to.distance_squared(from).sqrt() <= SIGHT_LIMIT && crate::clip_misses(world, from, to)
}

pub fn is_being_stared_by(mob: &MobView, world: &impl AiWorld, player: &Living) -> bool {
    !player.facts.gaze_disguise && is_looking_at_me(mob, world, player, STARE_CONE, mob.eye_y())
}

pub fn teleport_roll(mob: &MobView, random: &mut impl RandomSource) -> Vec3 {
    let x = mob.position.x + (random.next_double() - 0.5) * TELEPORT_RADIUS;
    let y = mob.position.y + f64::from(random.next_int(TELEPORT_VERTICAL) - TELEPORT_VERTICAL / 2);
    let z = mob.position.z + (random.next_double() - 0.5) * TELEPORT_RADIUS;
    Vec3::new(x, y, z)
}

pub fn teleport_towards_roll(
    mob: &MobView,
    random: &mut impl RandomSource,
    target: &Living,
) -> Vec3 {
    let away = Vec3::new(
        mob.position.x - target.position.x,
        mob.position.y + f64::from(mob.height) * 0.5 - target.eye_y(),
        mob.position.z - target.position.z,
    )
    .normalize();
    let x = mob.position.x + (random.next_double() - 0.5) * TELEPORT_TOWARDS_SPREAD
        - away.x * TELEPORT_TOWARDS_DISTANCE;
    let y = mob.position.y
        + f64::from(random.next_int(TELEPORT_TOWARDS_VERTICAL) - TELEPORT_TOWARDS_VERTICAL / 2)
        - away.y * TELEPORT_TOWARDS_DISTANCE;
    let z = mob.position.z + (random.next_double() - 0.5) * TELEPORT_TOWARDS_SPREAD
        - away.z * TELEPORT_TOWARDS_DISTANCE;
    Vec3::new(x, y, z)
}

fn cells_in(area: &Aabb) -> impl Iterator<Item = BlockPos> {
    let (x0, y0, z0) = (
        area.min_x.floor() as i32,
        area.min_y.floor() as i32,
        area.min_z.floor() as i32,
    );
    let (x1, y1, z1) = (
        area.max_x.floor() as i32,
        area.max_y.floor() as i32,
        area.max_z.floor() as i32,
    );
    (x0..=x1).flat_map(move |x| {
        (y0..=y1).flat_map(move |y| (z0..=z1).map(move |z| BlockPos::new(x, y, z)))
    })
}

fn state_at(world: &impl BlockView, pos: BlockPos) -> StateId {
    world.state(pos).unwrap_or(StateId::AIR)
}

pub fn random_teleport(world: &impl AiWorld, mob: &MobView, to: Vec3) -> Option<Vec3> {
    let x = to.x.clamp(-WORLD_BORDER, WORLD_BORDER);
    let z = to.z.clamp(-WORLD_BORDER, WORLD_BORDER);
    let mut pos = Vec3::new(x, to.y, z).block();
    if !world.chunk_loaded(pos.chunk()) {
        return None;
    }
    let mut y = to.y;
    while pos.y > world.min_y() {
        pos = pos.offset(0, -1, 0);
        let support = state_at(world, pos);
        if has_tag(support, TELEPORT_TO_TAG) {
            return fits(world, mob, Vec3::new(x, y, z), support);
        }
        y -= 1.0;
    }
    None
}

fn fits(world: &impl AiWorld, mob: &MobView, at: Vec3, support: StateId) -> Option<Vec3> {
    if has_tag(support, NO_TELEPORT_TAG) {
        return None;
    }
    let area = Aabb::of_size(at, mob.width, mob.height);
    let shape = rspigot_blocks::Aabb::new(
        area.min_x, area.min_y, area.min_z, area.max_x, area.max_y, area.max_z,
    );
    if collides(world, &shape) {
        return None;
    }
    let mut cells = cells_in(&area).map(|pos| state_at(world, pos));
    let blocked = cells.any(|state| state.flags().has_fluid() || has_tag(state, NO_TELEPORT_TAG));
    if blocked {
        return None;
    }
    let below = state_at(world, at.block().offset(0, -1, 0));
    if is_water(Some(below)) {
        return None;
    }
    Some(at)
}

pub fn find_teleport(
    world: &impl AiWorld,
    mob: &MobView,
    random: &mut impl RandomSource,
) -> Option<Vec3> {
    let roll = teleport_roll(mob, random);
    if mob.passenger {
        return None;
    }
    random_teleport(world, mob, roll)
}

pub fn find_teleport_towards(
    world: &impl AiWorld,
    mob: &MobView,
    random: &mut impl RandomSource,
    target: &Living,
) -> Option<Vec3> {
    let roll = teleport_towards_roll(mob, random, target);
    if mob.passenger {
        return None;
    }
    random_teleport(world, mob, roll)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FreezeWhenLookedAt {
    target: Option<EntityId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LookForPlayer {
    memory: TargetMemory,
    target: Option<EntityId>,
    pending: Option<EntityId>,
    aggro_time: i32,
    teleport_time: i32,
}

impl LookForPlayer {
    fn new() -> Self {
        Self {
            memory: TargetMemory::new(false),
            target: None,
            pending: None,
            aggro_time: 0,
            teleport_time: 0,
        }
    }
}

fn anger_inducing(mob: &MobView, world: &impl AiWorld, target: &Living) -> bool {
    is_being_stared_by(mob, world, target) || targeting::is_angry_at(mob, world, target)
}

#[derive(Debug, Clone, PartialEq)]
pub enum EndermanGoal {
    FreezeWhenLookedAt(FreezeWhenLookedAt),
    LeaveBlock,
    TakeBlock,
    LookForPlayer(LookForPlayer),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EndermanFacts {
    pub carried: Option<StateId>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EndermanAction {
    Teleport(Vec3),
    BeingStaredAt,
    TakeBlock { pos: BlockPos, carried: StateId },
    PlaceBlock { pos: BlockPos, state: StateId },
}

fn teleport<W: AiWorld>(context: &mut AiContext<'_, W>) -> bool {
    let (mob, world) = (context.mob, context.world);
    match find_teleport(world, mob, &mut context.brain.random) {
        Some(to) => {
            context.act(MobAction::Enderman(EndermanAction::Teleport(to)));
            true
        }
        None => false,
    }
}

fn teleport_towards<W: AiWorld>(context: &mut AiContext<'_, W>, target: &Living) -> bool {
    let (mob, world) = (context.mob, context.world);
    match find_teleport_towards(world, mob, &mut context.brain.random, target) {
        Some(to) => {
            context.act(MobAction::Enderman(EndermanAction::Teleport(to)));
            true
        }
        None => false,
    }
}

fn can_place(
    context: &AiContext<'_, impl AiWorld>,
    pos: BlockPos,
    carried: StateId,
    target: StateId,
    below_state: StateId,
) -> bool {
    let world = context.world;
    let cell = Aabb::new(
        f64::from(pos.x),
        f64::from(pos.y),
        f64::from(pos.z),
        f64::from(pos.x) + 1.0,
        f64::from(pos.y) + 1.0,
        f64::from(pos.z) + 1.0,
    );
    is_air(Some(target))
        && !below_state.is_air()
        && below_state.kind() != Some(rspigot_blocks::BlockKind::Bedrock)
        && below_state
            .flags()
            .contains(StateFlags::FULL_COLLISION_CUBE)
        && holdable::can_survive(world, carried, pos)
        && !world.any_entity_in(&cell, context.mob.id)
}

impl FreezeWhenLookedAt {
    fn can_use<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) -> bool {
        let Some(target) = context.target() else {
            self.target = None;
            return false;
        };
        self.target = Some(target.id);
        if !target.player {
            return false;
        }
        if context.mob.distance_to_sqr(target.position) > FREEZE_RANGE_SQR {
            return false;
        }
        is_being_stared_by(context.mob, context.world, &target)
    }

    fn start<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        context.stop_navigation();
    }

    fn tick<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        let Some(target) = self.target.and_then(|id| context.world.living(id)) else {
            return;
        };
        context.brain.look.look_at(Vec3::new(
            target.position.x,
            target.eye_y(),
            target.position.z,
        ));
    }
}

fn leave_can_use<W: AiWorld>(context: &mut AiContext<'_, W>) -> bool {
    if context.mob.species.enderman.carried.is_none() || !context.world.mob_griefing() {
        return false;
    }
    context
        .brain
        .random
        .next_int(reduced_tick_delay(LEAVE_INTERVAL))
        == 0
}

fn leave_tick<W: AiWorld>(context: &mut AiContext<'_, W>) {
    let mob = context.mob;
    let random = &mut context.brain.random;
    let x = mth::floor(mob.position.x - 1.0 + random.next_double() * 2.0);
    let y = mth::floor(mob.position.y + random.next_double() * 2.0);
    let z = mth::floor(mob.position.z - 1.0 + random.next_double() * 2.0);
    let pos = BlockPos::new(x, y, z);
    let world = context.world;
    let target = state_at(world, pos);
    let below = state_at(world, pos.offset(0, -1, 0));
    let Some(carried) = mob.species.enderman.carried else {
        return;
    };
    let carried = holdable::update_from_neighbour_shapes(world, carried, pos);
    if can_place(context, pos, carried, target, below) {
        context.act(MobAction::Enderman(EndermanAction::PlaceBlock {
            pos,
            state: carried,
        }));
    }
}

fn take_can_use<W: AiWorld>(context: &mut AiContext<'_, W>) -> bool {
    if context.mob.species.enderman.carried.is_some() || !context.world.mob_griefing() {
        return false;
    }
    context
        .brain
        .random
        .next_int(reduced_tick_delay(TAKE_INTERVAL))
        == 0
}

fn take_tick<W: AiWorld>(context: &mut AiContext<'_, W>) {
    let mob = context.mob;
    let random = &mut context.brain.random;
    let x = mth::floor(mob.position.x - 2.0 + random.next_double() * 4.0);
    let y = mth::floor(mob.position.y + random.next_double() * 3.0);
    let z = mth::floor(mob.position.z + (-2.0) + random.next_double() * 4.0);
    let pos = BlockPos::new(x, y, z);
    let world = context.world;
    let state = state_at(world, pos);
    let from = Vec3::new(
        f64::from(mth::floor(mob.position.x)) + 0.5,
        f64::from(y) + 0.5,
        f64::from(mth::floor(mob.position.z)) + 0.5,
    );
    let to = Vec3::new(f64::from(x) + 0.5, f64::from(y) + 0.5, f64::from(z) + 0.5);
    let reaches = holdable::first_outline_hit(world, from, to).is_none_or(|hit| hit == pos);
    if has_tag(state, HOLDABLE_TAG)
        && reaches
        && let Some(kind) = state.kind()
    {
        context.act(MobAction::Enderman(EndermanAction::TakeBlock {
            pos,
            carried: kind.default_state(),
        }));
    }
}

impl LookForPlayer {
    fn can_use<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) -> bool {
        let (mob, world) = (context.mob, context.world);
        let conditions = TargetingConditions::for_combat().range(mob.follow_range);
        self.pending = targeting::nearest_player(
            context,
            conditions,
            |target| anger_inducing(mob, world, target),
            mob.position,
        )
        .map(|player| player.id);
        self.pending.is_some()
    }

    fn start<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        self.aggro_time = adjusted_tick_delay(false, AGGRO_DELAY);
        self.teleport_time = 0;
        context.act(MobAction::Enderman(EndermanAction::BeingStaredAt));
    }

    fn stop<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        self.pending = None;
        self.memory.stop(context);
    }

    fn can_continue_to_use<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) -> bool {
        let (mob, world) = (context.mob, context.world);
        if let Some(pending) = self.pending {
            let Some(player) = world.living(pending) else {
                return false;
            };
            if !anger_inducing(mob, world, &player) {
                return false;
            }
            look_at_entity(mob, context.body, &player, LOOK_SPEED, LOOK_SPEED);
            return true;
        }
        if let Some(target) = self.target.and_then(|id| world.living(id)) {
            let conditions = TargetingConditions::for_combat().ignore_line_of_sight();
            if conditions.test(context, &target, |_| true) {
                return true;
            }
        }
        self.memory.can_continue_to_use(context)
    }

    fn tick<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        let (mob, world) = (context.mob, context.world);
        if context.target().is_none() {
            self.target = None;
        }
        if let Some(pending) = self.pending {
            self.aggro_time -= 1;
            if self.aggro_time <= 0 {
                self.target = Some(pending);
                self.pending = None;
                let found = world.living(pending);
                let reason = if found.is_some_and(|found| found.player) {
                    TargetReason::ClosestPlayer
                } else {
                    TargetReason::ClosestEntity
                };
                context.set_target(found, reason);
                self.memory.start();
            }
            return;
        }
        let Some(target) = self.target.and_then(|id| world.living(id)) else {
            return;
        };
        if mob.passenger {
            return;
        }
        let distance = mob.distance_to_sqr(target.position);
        if is_being_stared_by(mob, world, &target) {
            if distance < NEAR_TELEPORT_SQR {
                teleport(context);
            }
            self.teleport_time = 0;
        } else if distance > FREEZE_RANGE_SQR
            && {
                let ready = self.teleport_time >= adjusted_tick_delay(false, TELEPORT_DELAY);
                self.teleport_time += 1;
                ready
            }
            && teleport_towards(context, &target)
        {
            self.teleport_time = 0;
        }
    }
}

impl<W: AiWorld> Goal<AiContext<'_, W>> for EndermanGoal {
    fn flags(&self) -> Flags {
        match self {
            Self::FreezeWhenLookedAt(_) => Flags::of(Flag::Jump).with(Flag::Move),
            Self::LeaveBlock | Self::TakeBlock => Flags::NONE,
            Self::LookForPlayer(_) => Flags::of(Flag::Target),
        }
    }

    fn can_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        match self {
            Self::FreezeWhenLookedAt(goal) => goal.can_use(context),
            Self::LeaveBlock => leave_can_use(context),
            Self::TakeBlock => take_can_use(context),
            Self::LookForPlayer(goal) => goal.can_use(context),
        }
    }

    fn can_continue_to_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        match self {
            Self::LookForPlayer(goal) => goal.can_continue_to_use(context),
            _ => self.can_use(context),
        }
    }

    fn start(&mut self, context: &mut AiContext<'_, W>) {
        match self {
            Self::FreezeWhenLookedAt(goal) => goal.start(context),
            Self::LookForPlayer(goal) => goal.start(context),
            Self::LeaveBlock | Self::TakeBlock => {}
        }
    }

    fn stop(&mut self, context: &mut AiContext<'_, W>) {
        if let Self::LookForPlayer(goal) = self {
            goal.stop(context);
        }
    }

    fn tick(&mut self, context: &mut AiContext<'_, W>) {
        match self {
            Self::FreezeWhenLookedAt(goal) => goal.tick(context),
            Self::LeaveBlock => leave_tick(context),
            Self::TakeBlock => take_tick(context),
            Self::LookForPlayer(goal) => goal.tick(context),
        }
    }
}

pub(crate) fn register(add: &mut Goals<'_>, _kind: EntityKind) {
    add.brain.profile.set_malus(PathType::Water, -1.0);
    add.float(0);
    add.goal(
        1,
        MobGoal::Enderman(EndermanGoal::FreezeWhenLookedAt(FreezeWhenLookedAt {
            target: None,
        })),
    );
    add.goal(2, MobGoal::MeleeAttack(MeleeAttackGoal::new(1.0, false)));
    add.stroll_rarely_in_water(7, 1.0, 0.0);
    add.look(8, 8.0);
    add.look_around(8);
    add.goal(10, MobGoal::Enderman(EndermanGoal::LeaveBlock));
    add.goal(11, MobGoal::Enderman(EndermanGoal::TakeBlock));
    add.target_goal(
        1,
        MobGoal::Enderman(EndermanGoal::LookForPlayer(LookForPlayer::new())),
    );
    add.target_goal(2, MobGoal::HurtByTarget(crate::HurtByTargetGoal::new()));
    add.target(
        3,
        NearestAttackableTargetGoal::new(TargetKind::Class("Endermite"), true),
    );
    add.target_goal(
        4,
        MobGoal::ResetUniversalAnger(ResetUniversalAngerGoal::new(false)),
    );
}
