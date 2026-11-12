use rspigot_blocks::{BlockKind, StateFlags};
use rspigot_domain::entity::EntityBase;
use rspigot_domain::entity::living::LivingState;
use rspigot_domain::entity::species::fox::{
    FLAG_CROUCHING, FLAG_DEFENDING, FLAG_FACEPLANTED, FLAG_INTERESTED, FLAG_POUNCING, FLAG_SITTING,
    FLAG_SLEEPING,
};
use rspigot_domain::entity::species::fox::{
    is_crouching, is_faceplanted, is_interested, is_pouncing, is_sleeping,
};
use rspigot_domain::{BlockPos, EntityId, Vec3};
use rspigot_entities::EntityKind;
use uuid::Uuid;

use crate::action::{MobAction, TargetReason};
use crate::animal::{BreedGoal, FollowParentGoal, PanicGoal};
use crate::brain::AiContext;
use crate::combat::{LeapAtTargetGoal, MeleeAttackGoal, eye_of};
use crate::control::{LookControl, MAX_HEAD_X_ROT, MAX_HEAD_Y_ROT};
use crate::evade::{AvoidEntityGoal, FleeSunGoal, FoxAvoid};
use crate::goal::{Flag, Flags, Goal, GoalSelector, adjusted_tick_delay, reduced_tick_delay};
use crate::goals::{
    ClimbOnTopOfPowderSnowGoal, LookAtPlayerGoal, MobGoal, NearestAttackableTargetGoal,
    TargetFilter,
};
use crate::random::RandomSource;
use crate::random_pos::{land_pos_scored, walk_target_value};
use crate::registry::Goals;
use crate::scan::closer_to_center_than;
use crate::targeting::TargetingConditions;
use crate::world::{AiWorld, DroppedItem, Living, MobView, TargetKind, inflate};

pub const FOX_AGGRO: &str = "minecraft:entity.fox.aggro";
pub const FOX_SNIFF: &str = "minecraft:entity.fox.sniff";
const SHELTER_INTERVAL: i32 = 100;
const SLEEP_WAIT: i32 = 70;
const FLEE_SPEED: f64 = 1.25;
const MELEE_SPEED: f32 = 1.2;
const BERRY_SPEED: f32 = 1.2;
const ITEM_SPEED: f32 = 1.2;
const STALK_SPEED: f64 = 1.5;
const STALK_DISTANCE_SQR: f64 = 36.0;
const POUNCE_LOOK_Y: f32 = 60.0;
const POUNCE_LOOK_X: f32 = 30.0;
const BERRY_WAIT: i32 = 40;
const BERRY_SEARCH: i32 = 12;
const BERRY_VERTICAL: i32 = 1;
const BERRY_ACCEPTED: f64 = 2.0;
const BERRY_GIVE_UP: i32 = 1200;
const ALERT_RANGE: f64 = 12.0;
const ITEM_RANGE: f64 = 8.0;
const SWEET_BERRY_RIPE: u8 = 2;
const VILLAGE_CLOSE: i32 = 6;
const VILLAGE_DISTANCE: f64 = 10.0;
const FOLLOW_DEFEND_INTERVAL: i32 = 10;
const FISH_INTERVAL: i32 = 20;
const FOX_LOOK_DISTANCE: f32 = 24.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TrustedRef {
    pub uuid: Uuid,
    pub id: Option<EntityId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FoxFacts {
    pub flags: u8,
    pub fully_crouched: bool,
    pub jumping: bool,
    pub moving: bool,
    pub trusted: [Option<TrustedRef>; 2],
}

impl FoxFacts {
    fn flag(&self, mask: u8) -> bool {
        self.flags & mask != 0
    }

    fn set_flag(&mut self, mask: u8, value: bool) {
        if value {
            self.flags |= mask;
        } else {
            self.flags &= !mask;
        }
    }

    pub fn is_sitting(&self) -> bool {
        self.flag(FLAG_SITTING)
    }

    pub fn is_crouching(&self) -> bool {
        self.flag(FLAG_CROUCHING)
    }

    pub fn is_interested(&self) -> bool {
        self.flag(FLAG_INTERESTED)
    }

    pub fn is_pouncing(&self) -> bool {
        self.flag(FLAG_POUNCING)
    }

    pub fn is_sleeping(&self) -> bool {
        self.flag(FLAG_SLEEPING)
    }

    pub fn is_faceplanted(&self) -> bool {
        self.flag(FLAG_FACEPLANTED)
    }

    pub fn is_defending(&self) -> bool {
        self.flag(FLAG_DEFENDING)
    }

    pub fn set_sitting(&mut self, value: bool) {
        self.set_flag(FLAG_SITTING, value);
    }

    pub fn set_crouching(&mut self, value: bool) {
        self.set_flag(FLAG_CROUCHING, value);
    }

    pub fn set_interested(&mut self, value: bool) {
        self.set_flag(FLAG_INTERESTED, value);
    }

    pub fn set_pouncing(&mut self, value: bool) {
        self.set_flag(FLAG_POUNCING, value);
    }

    pub fn set_sleeping(&mut self, value: bool) {
        self.set_flag(FLAG_SLEEPING, value);
    }

    pub fn set_faceplanted(&mut self, value: bool) {
        self.set_flag(FLAG_FACEPLANTED, value);
    }

    pub fn set_defending(&mut self, value: bool) {
        self.set_flag(FLAG_DEFENDING, value);
    }

    pub fn clear_states(&mut self) {
        self.set_interested(false);
        self.set_crouching(false);
        self.set_sitting(false);
        self.set_sleeping(false);
        self.set_defending(false);
        self.set_faceplanted(false);
    }

    pub fn can_move(&self) -> bool {
        !self.is_sleeping() && !self.is_sitting() && !self.is_faceplanted()
    }

    pub fn trusts(&self, uuid: Uuid) -> bool {
        self.trusted
            .iter()
            .flatten()
            .any(|trusted| trusted.uuid == uuid)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FoxAction {
    SetFlags(u8),
    ClearStates(EntityId),
    Sound(&'static str),
    ResetCrouch,
    PickBerries(BlockPos),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SeekShelterGoal {
    flee: FleeSunGoal,
    interval: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SleepGoal {
    countdown: i32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StrollThroughVillageGoal {
    interval: i32,
    wanted: Option<BlockPos>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EatBerriesGoal {
    next_start_tick: i32,
    try_ticks: i32,
    max_stay_ticks: i32,
    block_pos: BlockPos,
    reached_target: bool,
    ticks_waited: i32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PerchAndSearchGoal {
    rel_x: f64,
    rel_z: f64,
    look_time: i32,
    looks_remaining: i32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DefendTrustedGoal {
    inner: NearestAttackableTargetGoal,
    interval: i32,
    trusted_hurt: Option<EntityId>,
    last_hurt_by: Option<Living>,
    timestamp: i32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FoxGoal {
    Float,
    Faceplant { countdown: i32 },
    Panic(PanicGoal),
    Breed(BreedGoal),
    StalkPrey,
    Pounce,
    SeekShelter(SeekShelterGoal),
    Melee(Box<MeleeAttackGoal>),
    Sleep(SleepGoal),
    FollowParent(FollowParentGoal),
    StrollThroughVillage(StrollThroughVillageGoal),
    EatBerries(EatBerriesGoal),
    SearchForItems,
    LookAtPlayer(LookAtPlayerGoal),
    PerchAndSearch(PerchAndSearchGoal),
    DefendTrusted(Box<DefendTrustedGoal>),
}

impl FoxGoal {
    pub fn faceplant() -> Self {
        Self::Faceplant { countdown: 0 }
    }

    pub fn seek_shelter(speed: f64) -> Self {
        Self::SeekShelter(SeekShelterGoal {
            flee: FleeSunGoal::new(speed),
            interval: reduced_tick_delay(SHELTER_INTERVAL),
        })
    }

    pub fn stroll_through_village(interval: i32) -> Self {
        Self::StrollThroughVillage(StrollThroughVillageGoal {
            interval: reduced_tick_delay(interval),
            wanted: None,
        })
    }

    pub fn eat_berries() -> Self {
        Self::EatBerries(EatBerriesGoal {
            next_start_tick: 0,
            try_ticks: 0,
            max_stay_ticks: 0,
            block_pos: BlockPos::new(0, 0, 0),
            reached_target: false,
            ticks_waited: 0,
        })
    }

    pub fn perch_and_search() -> Self {
        Self::PerchAndSearch(PerchAndSearchGoal {
            rel_x: 0.0,
            rel_z: 0.0,
            look_time: 0,
            looks_remaining: 0,
        })
    }

    pub fn defend_trusted() -> Self {
        Self::DefendTrusted(Box::new(DefendTrustedGoal {
            inner: NearestAttackableTargetGoal::with(
                TargetKind::Class("LivingEntity"),
                FOLLOW_DEFEND_INTERVAL,
                false,
                TargetFilter::Any,
            ),
            interval: reduced_tick_delay(FOLLOW_DEFEND_INTERVAL),
            trusted_hurt: None,
            last_hurt_by: None,
            timestamp: 0,
        }))
    }
}

fn alertable_target(fox: &FoxFacts, target: &Living) -> bool {
    if target.kind == EntityKind::Fox {
        false
    } else if target.kind == EntityKind::Chicken
        || target.kind == EntityKind::Rabbit
        || target.kind.extends("Monster")
    {
        true
    } else if target.kind.extends("TamableAnimal") {
        !target.facts.tamed
    } else if (target.player && (target.spectator || target.creative))
        || fox.trusts(target.facts.uuid)
    {
        false
    } else {
        !target.facts.sleeping && !target.discrete
    }
}

fn alertable<W: AiWorld>(context: &mut AiContext<'_, W>) -> bool {
    let area = inflate(&context.mob.bounds(), ALERT_RANGE, 6.0, ALERT_RANGE);
    let conditions = TargetingConditions::for_combat()
        .range(ALERT_RANGE)
        .ignore_line_of_sight();
    let mut candidates = context
        .world
        .living_of(TargetKind::Class("LivingEntity"), &area);
    candidates.extend(context.world.living_of(TargetKind::Player, &area));
    let fox = context.brain.fox;
    candidates
        .iter()
        .any(|candidate| conditions.test(context, candidate, |t| alertable_target(&fox, t)))
}

fn has_shelter<W: AiWorld>(context: &AiContext<'_, W>) -> bool {
    let mob = context.mob;
    let pos = Vec3::new(mob.position.x, mob.bounds().max_y, mob.position.z).block();
    !context.world.can_see_sky(pos) && walk_target_value(mob.kind, context.world, pos) >= 0.0
}

fn nearby_items<W: AiWorld>(context: &AiContext<'_, W>) -> Vec<DroppedItem> {
    let area = inflate(&context.mob.bounds(), ITEM_RANGE, ITEM_RANGE, ITEM_RANGE);
    context
        .world
        .items_in(&area)
        .into_iter()
        .filter(|item| !item.has_pickup_delay && item.alive)
        .collect()
}

fn replaceable<W: AiWorld>(world: &W, pos: BlockPos) -> bool {
    world
        .state(pos)
        .is_none_or(|state| state.flags().contains(StateFlags::REPLACEABLE))
}

pub fn is_path_clear<W: AiWorld>(world: &W, mob: &MobView, target: &Living) -> bool {
    let d0 = target.position.z - mob.position.z;
    let d1 = target.position.x - mob.position.x;
    let d2 = d0 / d1;
    for step in 0..6 {
        let fraction = f64::from(step as f32 / 6.0f32);
        let d3 = if d2 == 0.0 { 0.0 } else { d0 * fraction };
        let d4 = if d2 == 0.0 { d1 * fraction } else { d3 / d2 };
        for k in 1..4 {
            let at = Vec3::new(
                mob.position.x + d4,
                mob.position.y + f64::from(k),
                mob.position.z + d3,
            )
            .block();
            if !replaceable(world, at) {
                return false;
            }
        }
    }
    true
}

fn signum(value: f64) -> f64 {
    if value == 0.0 { 0.0 } else { value.signum() }
}

fn rot_lerp(delta: f32, from: f32, to: f32) -> f32 {
    from + delta * rspigot_domain::mth::wrap_degrees(to - from)
}

fn jump_fox_pitch(velocity: Vec3, jumping: bool) -> Option<f32> {
    let horizontal = velocity.horizontal_distance_sqr().sqrt();
    let factor = if jumping && velocity.y > 0.0 {
        6.5f32
    } else {
        1.0
    };
    let vertical = velocity.y * f64::from(factor);
    let total = (horizontal * horizontal + vertical * vertical).sqrt();
    (total > f64::from(1.0e-5f32)).then(|| {
        let degrees = signum(-vertical) * (horizontal / total).acos() * f64::from(180.0f32)
            / f64::from(std::f32::consts::PI);
        degrees as f32
    })
}

fn find_berry_block<W: AiWorld>(
    context: &AiContext<'_, W>,
    valid: impl Fn(BlockPos) -> bool,
) -> Option<BlockPos> {
    let origin = context.mob.block_pos();
    let mut dy = 0;
    while dy <= BERRY_VERTICAL {
        for radius in 0..BERRY_SEARCH {
            let mut dx = 0;
            while dx <= radius {
                let mut dz = if dx < radius && dx > -radius {
                    radius
                } else {
                    0
                };
                while dz <= radius {
                    let pos = origin.offset(dx, dy - 1, dz);
                    if context.mob.is_within_home(pos) && valid(pos) {
                        return Some(pos);
                    }
                    dz = if dz > 0 { -dz } else { 1 - dz };
                }
                dx = if dx > 0 { -dx } else { 1 - dx };
            }
        }
        dy = if dy > 0 { -dy } else { 1 - dy };
    }
    None
}

fn is_berry_target<W: AiWorld>(world: &W, pos: BlockPos) -> bool {
    let Some(state) = world.state(pos) else {
        return false;
    };
    let sweet = state.kind() == Some(BlockKind::SweetBerryBush)
        && state
            .property("age")
            .and_then(|age| age.parse::<u8>().ok())
            .is_some_and(|age| age >= SWEET_BERRY_RIPE);
    sweet || state.property("berries") == Some("true")
}

impl SeekShelterGoal {
    fn can_use<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) -> bool {
        if context.brain.fox.is_sleeping() || context.target().is_some() {
            return false;
        }
        let pos = context.mob.block_pos();
        if context.world.thundering() && context.world.can_see_sky(pos) {
            return self.flee.set_wanted_pos(context);
        }
        if self.interval > 0 {
            self.interval -= 1;
            return false;
        }
        self.interval = SHELTER_INTERVAL;
        context.world.bright_outside()
            && context.world.can_see_sky(pos)
            && !context.world.is_village(pos)
            && self.flee.set_wanted_pos(context)
    }
}

impl SleepGoal {
    fn can_sleep<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) -> bool {
        if self.countdown > 0 {
            self.countdown -= 1;
            return false;
        }
        context.world.bright_outside()
            && has_shelter(context)
            && !alertable(context)
            && !context.mob.in_powder_snow
    }
}

impl EatBerriesGoal {
    fn find<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) -> bool {
        let world = context.world;
        match find_berry_block(context, |pos| is_berry_target(world, pos)) {
            Some(pos) => {
                self.block_pos = pos;
                true
            }
            None => false,
        }
    }

    fn move_to_block<W: AiWorld>(&self, context: &mut AiContext<'_, W>) {
        let at = Vec3::new(
            f64::from(self.block_pos.x) + 0.5,
            f64::from(self.block_pos.y + 1),
            f64::from(self.block_pos.z) + 0.5,
        );
        context.move_to_pos(at, f64::from(BERRY_SPEED));
    }
}

impl DefendTrustedGoal {
    fn selector(fox: &FoxFacts, target: &Living) -> bool {
        target.facts.last_hurt_mob.is_some()
            && target.facts.last_hurt_mob_at < target.facts.tick_count + 600
            && !fox.trusts(target.facts.uuid)
    }

    fn can_attack<W: AiWorld>(context: &mut AiContext<'_, W>, target: Option<Living>) -> bool {
        let Some(target) = target else {
            return false;
        };
        let fox = context.brain.fox;
        let conditions = TargetingConditions::for_combat().range(context.mob.follow_range);
        conditions.test(context, &target, |t| Self::selector(&fox, t))
            && context.mob.is_within_home(target.position.block())
    }
}

impl<W: AiWorld> Goal<AiContext<'_, W>> for FoxGoal {
    fn flags(&self) -> Flags {
        let movement = Flags::of(Flag::Move);
        match self {
            Self::Float => Flags::of(Flag::Jump),
            Self::Faceplant { .. } => movement.with(Flag::Look).with(Flag::Jump),
            Self::Panic(_) | Self::SeekShelter(_) | Self::StrollThroughVillage(_) => movement,
            Self::SearchForItems => movement,
            Self::Breed(_) | Self::StalkPrey | Self::Melee(_) | Self::PerchAndSearch(_) => {
                movement.with(Flag::Look)
            }
            Self::Pounce | Self::EatBerries(_) => movement.with(Flag::Jump),
            Self::Sleep(_) => movement.with(Flag::Look).with(Flag::Jump),
            Self::FollowParent(_) => Flags::NONE,
            Self::LookAtPlayer(_) => Flags::of(Flag::Look),
            Self::DefendTrusted(_) => Flags::of(Flag::Target),
        }
    }

    fn can_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        match self {
            Self::Float => context.mob.fluid.water_height > 0.25 || context.mob.fluid.in_lava(),
            Self::Faceplant { .. } => context.brain.fox.is_faceplanted(),
            Self::Panic(goal) => !context.brain.fox.is_defending() && goal.can_use(context),
            Self::Breed(goal) => goal.can_use(context),
            Self::StalkPrey => {
                if context.brain.fox.is_sleeping() {
                    return false;
                }
                let Some(target) = context.target() else {
                    return false;
                };
                let fox = context.brain.fox;
                target.alive
                    && (target.kind == EntityKind::Chicken || target.kind == EntityKind::Rabbit)
                    && context.mob.distance_to_sqr(target.position) > STALK_DISTANCE_SQR
                    && !fox.is_crouching()
                    && !fox.is_interested()
                    && !fox.jumping
            }
            Self::Pounce => {
                if !context.brain.fox.fully_crouched {
                    return false;
                }
                let Some(target) = context.target().filter(|target| target.alive) else {
                    return false;
                };
                let clear = is_path_clear(context.world, context.mob, &target);
                if !clear {
                    let _ = context.create_path_to(target.position.block(), 0);
                    context.brain.fox.set_crouching(false);
                    context.brain.fox.set_interested(false);
                }
                clear
            }
            Self::SeekShelter(goal) => goal.can_use(context),
            Self::Melee(goal) => {
                let fox = context.brain.fox;
                !fox.is_sitting()
                    && !fox.is_sleeping()
                    && !fox.is_crouching()
                    && !fox.is_faceplanted()
                    && goal.can_use(context)
            }
            Self::Sleep(goal) => {
                let fox = context.brain.fox;
                if fox.moving {
                    return false;
                }
                goal.can_sleep(context) || fox.is_sleeping()
            }
            Self::FollowParent(goal) => !context.brain.fox.is_defending() && goal.can_use(context),
            Self::StrollThroughVillage(goal) => {
                if context.world.bright_outside() {
                    return false;
                }
                if context.brain.random.next_int(goal.interval) != 0 {
                    return false;
                }
                let pos = context.mob.block_pos();
                if !context.world.is_close_to_village(pos, VILLAGE_CLOSE) {
                    return false;
                }
                let found = land_pos_scored(context, 15, 7, |context, pos| {
                    -f64::from(context.world.sections_to_village(pos))
                });
                goal.wanted = found.map(Vec3::block);
                let fox = context.brain.fox;
                goal.wanted.is_some()
                    && !fox.is_sleeping()
                    && !fox.is_sitting()
                    && !fox.is_defending()
                    && context.target().is_none()
            }
            Self::EatBerries(goal) => {
                if context.brain.fox.is_sleeping() {
                    return false;
                }
                if goal.next_start_tick > 0 {
                    goal.next_start_tick -= 1;
                    return false;
                }
                let delay = 200 + context.brain.random.next_int(200);
                goal.next_start_tick = reduced_tick_delay(delay);
                goal.find(context)
            }
            Self::SearchForItems => {
                if context.mob.main_hand.is_some() {
                    return false;
                }
                if context.target().is_some() || context.mob.last_hurt_by_mob.is_some() {
                    return false;
                }
                if !context.brain.fox.can_move() {
                    return false;
                }
                if context.brain.random.next_int(reduced_tick_delay(10)) != 0 {
                    return false;
                }
                !nearby_items(context).is_empty() && context.mob.main_hand.is_none()
            }
            Self::LookAtPlayer(goal) => {
                goal.can_use(context)
                    && !context.brain.fox.is_faceplanted()
                    && !context.brain.fox.is_interested()
            }
            Self::PerchAndSearch(_) => {
                if context.mob.last_hurt_by_mob.is_some() {
                    return false;
                }
                if context.brain.random.next_float() >= 0.02 {
                    return false;
                }
                let fox = context.brain.fox;
                !fox.is_sleeping()
                    && context.target().is_none()
                    && context.brain.navigation.is_done()
                    && !alertable(context)
                    && !fox.is_pouncing()
                    && !fox.is_crouching()
            }
            Self::DefendTrusted(goal) => {
                if goal.interval > 0 && context.brain.random.next_int(goal.interval) != 0 {
                    return false;
                }
                let trusted = context.brain.fox.trusted;
                for reference in trusted.iter().flatten() {
                    let Some(entity) = reference.id.and_then(|id| context.world.living(id)) else {
                        continue;
                    };
                    goal.trusted_hurt = Some(entity.id);
                    let attacker = entity
                        .facts
                        .last_hurt_by_mob
                        .and_then(|id| context.world.living(id));
                    goal.last_hurt_by = attacker;
                    return entity.facts.last_hurt_by_mob_at != goal.timestamp
                        && Self::defend_can_attack(context, attacker);
                }
                false
            }
        }
    }

    fn can_continue_to_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        match self {
            Self::Faceplant { countdown } => context.brain.fox.is_faceplanted() && *countdown > 0,
            Self::Panic(_) | Self::SeekShelter(_) => !context.brain.navigation.is_done(),
            Self::Breed(goal) => goal.can_continue_to_use(context),
            Self::Pounce => {
                let Some(target) = context.target().filter(|target| target.alive) else {
                    return false;
                };
                let _ = target;
                let vertical = context.body.velocity.y;
                (vertical * vertical >= f64::from(0.05f32)
                    || context.body.pitch.abs() >= 15.0
                    || !context.mob.on_ground)
                    && !context.brain.fox.is_faceplanted()
            }
            Self::Melee(goal) => goal.can_continue_to_use(context),
            Self::Sleep(goal) => goal.can_sleep(context),
            Self::FollowParent(goal) => {
                !context.brain.fox.is_defending() && goal.can_continue_to_use(context)
            }
            Self::StrollThroughVillage(goal) => {
                let fox = context.brain.fox;
                let navigation = &context.brain.navigation;
                goal.wanted.is_some()
                    && !navigation.is_done()
                    && navigation.target_pos() == goal.wanted
                    && !fox.is_sleeping()
                    && !fox.is_sitting()
                    && !fox.is_defending()
                    && context.target().is_none()
            }
            Self::EatBerries(goal) => {
                goal.try_ticks >= -goal.max_stay_ticks
                    && goal.try_ticks <= BERRY_GIVE_UP
                    && is_berry_target(context.world, goal.block_pos)
            }
            Self::LookAtPlayer(goal) => {
                goal.can_continue_to_use(context)
                    && !context.brain.fox.is_faceplanted()
                    && !context.brain.fox.is_interested()
            }
            Self::PerchAndSearch(goal) => goal.looks_remaining > 0,
            Self::DefendTrusted(goal) => goal.inner.can_continue_to_use(context),
            Self::Float | Self::StalkPrey | Self::SearchForItems => self.can_use(context),
        }
    }

    fn is_interruptable(&self) -> bool {
        !matches!(self, Self::Pounce)
    }

    fn start(&mut self, context: &mut AiContext<'_, W>) {
        match self {
            Self::Float => context.brain.fox.clear_states(),
            Self::Faceplant { countdown } => *countdown = adjusted_tick_delay(false, 40),
            Self::Panic(goal) => goal.start(context),
            Self::Breed(goal) => {
                context.brain.fox.clear_states();
                if let Some(partner) = goal.partner() {
                    context.act(MobAction::Fox(FoxAction::ClearStates(partner)));
                }
            }
            Self::StalkPrey => {
                context.brain.fox.set_sitting(false);
                context.brain.fox.set_faceplanted(false);
            }
            Self::Pounce => {
                context.brain.fox.jumping = true;
                context.brain.fox.set_pouncing(true);
                context.brain.fox.set_interested(false);
                if let Some(target) = context.target() {
                    context
                        .brain
                        .look
                        .set_look_at(eye_of(&target), POUNCE_LOOK_Y, POUNCE_LOOK_X);
                    let delta = Vec3::new(
                        target.position.x - context.mob.position.x,
                        target.position.y - context.mob.position.y,
                        target.position.z - context.mob.position.z,
                    )
                    .normalize();
                    let velocity = context.body.velocity;
                    context.body.velocity = Vec3::new(
                        velocity.x + delta.x * 0.8,
                        velocity.y + 0.9,
                        velocity.z + delta.z * 0.8,
                    );
                }
                context.stop_navigation();
            }
            Self::SeekShelter(goal) => {
                context.brain.fox.clear_states();
                let wanted = goal.flee_wanted();
                context.move_to_pos(wanted, FLEE_SPEED);
            }
            Self::Melee(goal) => {
                context.brain.fox.set_interested(false);
                goal.start(context);
            }
            Self::Sleep(_) => {
                let fox = &mut context.brain.fox;
                fox.set_sitting(false);
                fox.set_crouching(false);
                fox.set_interested(false);
                fox.jumping = false;
                fox.set_sleeping(true);
                context.stop_navigation();
                let at = context.mob.position;
                context.brain.movement.set_wanted_position(at, 0.0);
            }
            Self::FollowParent(goal) => {
                context.brain.fox.clear_states();
                goal.start();
            }
            Self::StrollThroughVillage(_) => context.brain.fox.clear_states(),
            Self::EatBerries(goal) => {
                goal.ticks_waited = 0;
                context.brain.fox.set_sitting(false);
                goal.move_to_block(context);
                goal.try_ticks = 0;
                let inner = context.brain.random.next_int(1200) + 1200;
                goal.max_stay_ticks = context.brain.random.next_int(inner) + 1200;
            }
            Self::SearchForItems => {
                if let Some(item) = nearby_items(context).first() {
                    let position = item.position;
                    context.move_to_entity_at(position, f64::from(ITEM_SPEED));
                }
            }
            Self::LookAtPlayer(goal) => goal.start(context),
            Self::PerchAndSearch(goal) => {
                goal.reset_look(context);
                goal.looks_remaining = 2 + context.brain.random.next_int(3);
                context.brain.fox.set_sitting(true);
                context.stop_navigation();
            }
            Self::DefendTrusted(goal) => {
                if let Some(hurt) = goal.trusted_hurt.and_then(|id| context.world.living(id)) {
                    goal.timestamp = hurt.facts.last_hurt_by_mob_at;
                }
                context.act(MobAction::Fox(FoxAction::Sound(FOX_AGGRO)));
                context.brain.fox.set_defending(true);
                context.brain.fox.set_sleeping(false);
                let reason = if goal.last_hurt_by.is_some_and(|target| target.player) {
                    TargetReason::ClosestPlayer
                } else {
                    TargetReason::ClosestEntity
                };
                context.set_target(goal.last_hurt_by, reason);
            }
        }
    }

    fn stop(&mut self, context: &mut AiContext<'_, W>) {
        match self {
            Self::Faceplant { .. } => context.brain.fox.set_faceplanted(false),
            Self::Panic(goal) => goal.stop(),
            Self::Breed(goal) => goal.stop(),
            Self::StalkPrey => {
                let target = context.target();
                let clear = target
                    .as_ref()
                    .is_some_and(|target| is_path_clear(context.world, context.mob, target));
                if let (true, Some(target)) = (clear, target) {
                    context.brain.fox.set_interested(true);
                    context.brain.fox.set_crouching(true);
                    context.stop_navigation();
                    context
                        .brain
                        .look
                        .set_look_at(eye_of(&target), MAX_HEAD_Y_ROT, MAX_HEAD_X_ROT);
                } else {
                    context.brain.fox.set_interested(false);
                    context.brain.fox.set_crouching(false);
                }
            }
            Self::Pounce => {
                context.brain.fox.set_crouching(false);
                context.brain.fox.fully_crouched = false;
                context.act(MobAction::Fox(FoxAction::ResetCrouch));
                context.brain.fox.set_interested(false);
                context.brain.fox.set_pouncing(false);
            }
            Self::Melee(goal) => goal.stop(context),
            Self::Sleep(goal) => {
                goal.countdown = context.brain.random.next_int(SLEEP_WAIT);
                context.brain.fox.clear_states();
            }
            Self::FollowParent(goal) => goal.stop(),
            Self::LookAtPlayer(goal) => goal.stop(),
            Self::PerchAndSearch(_) => context.brain.fox.set_sitting(false),
            Self::DefendTrusted(goal) => {
                context.set_target(None, TargetReason::ForgotTarget);
                goal.last_hurt_by = None;
            }
            Self::Float
            | Self::SeekShelter(_)
            | Self::StrollThroughVillage(_)
            | Self::EatBerries(_)
            | Self::SearchForItems => {}
        }
    }

    fn requires_update_every_tick(&self) -> bool {
        matches!(self, Self::Float | Self::Melee(_) | Self::EatBerries(_))
    }

    fn tick(&mut self, context: &mut AiContext<'_, W>) {
        match self {
            Self::Float => {
                if context.brain.random.next_float() < 0.8 {
                    context.brain.jump.jump();
                }
            }
            Self::Faceplant { countdown } => *countdown -= 1,
            Self::Panic(goal) => goal.tick(context),
            Self::Breed(goal) => goal.tick(context),
            Self::StalkPrey => {
                let Some(target) = context.target() else {
                    return;
                };
                context
                    .brain
                    .look
                    .set_look_at(eye_of(&target), MAX_HEAD_Y_ROT, MAX_HEAD_X_ROT);
                if context.mob.distance_to_sqr(target.position) <= STALK_DISTANCE_SQR {
                    context.brain.fox.set_interested(true);
                    context.brain.fox.set_crouching(true);
                    context.stop_navigation();
                } else {
                    context.move_to_entity(&target, STALK_SPEED);
                }
            }
            Self::Pounce => {
                let target = context.target();
                if let Some(target) = &target {
                    context
                        .brain
                        .look
                        .set_look_at(eye_of(target), POUNCE_LOOK_Y, POUNCE_LOOK_X);
                }
                if !context.brain.fox.is_faceplanted() {
                    let velocity = context.body.velocity;
                    if velocity.y * velocity.y < f64::from(0.03f32) && context.body.pitch != 0.0 {
                        context.body.pitch = rot_lerp(0.2, context.body.pitch, 0.0);
                    } else if let Some(pitch) = jump_fox_pitch(velocity, context.brain.fox.jumping)
                    {
                        context.body.pitch = pitch;
                    }
                }
                if let Some(target) = target
                    && context
                        .mob
                        .position
                        .distance_squared(target.position)
                        .sqrt()
                        <= 2.0
                {
                    context.act(MobAction::Attack(target.id));
                } else if context.body.pitch > 0.0
                    && context.mob.on_ground
                    && context.body.velocity.y as f32 != 0.0
                    && context
                        .world
                        .state(context.mob.block_pos())
                        .is_some_and(|state| state.kind() == Some(BlockKind::Snow))
                {
                    context.body.pitch = 60.0;
                    context.set_target(None, TargetReason::Unknown);
                    context.brain.fox.set_faceplanted(true);
                }
            }
            Self::Melee(goal) => goal.tick(context),
            Self::FollowParent(goal) => goal.tick(context),
            Self::StrollThroughVillage(goal) => goal.tick(context),
            Self::EatBerries(goal) => goal.tick(context),
            Self::SearchForItems => {
                let items = nearby_items(context);
                if context.mob.main_hand.is_none()
                    && let Some(item) = items.first()
                {
                    context.move_to_entity_at(item.position, f64::from(ITEM_SPEED));
                }
            }
            Self::LookAtPlayer(goal) => goal.tick(context),
            Self::PerchAndSearch(goal) => goal.tick(context),
            Self::SeekShelter(_) | Self::Sleep(_) | Self::DefendTrusted(_) => {}
        }
    }
}

impl FoxGoal {
    fn defend_can_attack<W: AiWorld>(
        context: &mut AiContext<'_, W>,
        target: Option<Living>,
    ) -> bool {
        DefendTrustedGoal::can_attack(context, target)
    }
}

impl SeekShelterGoal {
    fn flee_wanted(&self) -> Vec3 {
        self.flee.wanted()
    }
}

impl StrollThroughVillageGoal {
    fn tick<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        let Some(wanted) = self.wanted else {
            return;
        };
        if !(context.brain.navigation.is_done()
            && !closer_to_center_than(wanted, context.mob.position, VILLAGE_DISTANCE))
        {
            return;
        }
        let long = Vec3::new(
            f64::from(wanted.x) + 0.5,
            f64::from(wanted.y),
            f64::from(wanted.z) + 0.5,
        );
        let me = context.mob.position;
        let distance = me - long;
        let long = distance.scale(0.4) + long;
        let move_target = (long - me).normalize().scale(10.0) + me;
        let path_target = context
            .world
            .motion_blocking_no_leaves_pos(move_target.block());
        let at = Vec3::new(
            f64::from(path_target.x),
            f64::from(path_target.y),
            f64::from(path_target.z),
        );
        if !context.move_to_pos(at, 1.0) {
            let x = -8 + context.brain.random.next_int(16);
            let z = -8 + context.brain.random.next_int(16);
            let random = context
                .world
                .motion_blocking_no_leaves_pos(context.mob.block_pos().offset(x, 0, z));
            let at = Vec3::new(
                f64::from(random.x),
                f64::from(random.y),
                f64::from(random.z),
            );
            context.move_to_pos(at, 1.0);
        }
    }
}

impl EatBerriesGoal {
    fn tick<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        if self.reached_target {
            if self.ticks_waited >= BERRY_WAIT {
                if context.world.mob_griefing() {
                    context.act(MobAction::Fox(FoxAction::PickBerries(self.block_pos)));
                }
            } else {
                self.ticks_waited += 1;
            }
        } else if context.brain.random.next_float() < 0.05 {
            context.act(MobAction::Fox(FoxAction::Sound(FOX_SNIFF)));
        }
        let target = self.block_pos.offset(0, 1, 0);
        if !closer_to_center_than(target, context.mob.position, BERRY_ACCEPTED) {
            self.reached_target = false;
            self.try_ticks += 1;
            if self.try_ticks % 100 == 0 {
                let at = Vec3::new(
                    f64::from(target.x) + 0.5,
                    f64::from(target.y),
                    f64::from(target.z) + 0.5,
                );
                context.move_to_pos(at, f64::from(BERRY_SPEED));
            }
        } else {
            self.reached_target = true;
            self.try_ticks -= 1;
        }
    }
}

impl PerchAndSearchGoal {
    fn reset_look<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        let angle = std::f64::consts::TAU * context.brain.random.next_double();
        self.rel_x = angle.cos();
        self.rel_z = angle.sin();
        let ticks = 80 + context.brain.random.next_int(20);
        self.look_time = adjusted_tick_delay(false, ticks);
    }

    fn tick<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        self.look_time -= 1;
        if self.look_time <= 0 {
            self.looks_remaining -= 1;
            self.reset_look(context);
        }
        let mob = context.mob;
        let at = Vec3::new(
            mob.position.x + self.rel_x,
            mob.eye_y(),
            mob.position.z + self.rel_z,
        );
        context
            .brain
            .look
            .set_look_at(at, MAX_HEAD_Y_ROT, MAX_HEAD_X_ROT);
    }
}

pub fn tick_look(
    look: &mut LookControl,
    base: &mut EntityBase,
    living: &LivingState,
    navigating: bool,
) {
    if is_sleeping(base) {
        return;
    }
    let reset =
        !is_pouncing(base) && !is_crouching(base) && !is_interested(base) && !is_faceplanted(base);
    look.tick_with(base, living, navigating, reset);
}

pub(crate) fn add_target_goals(targets: &mut GoalSelector<MobGoal>, red: bool) {
    let land = || {
        MobGoal::NearestAttackableTarget(NearestAttackableTargetGoal::with(
            TargetKind::Class("Animal"),
            10,
            false,
            TargetFilter::ChickenOrRabbit,
        ))
    };
    let turtle = || {
        MobGoal::NearestAttackableTarget(NearestAttackableTargetGoal::with(
            TargetKind::Turtle,
            10,
            false,
            TargetFilter::BabyOnLand,
        ))
    };
    let fish = || {
        MobGoal::NearestAttackableTarget(NearestAttackableTargetGoal::with(
            TargetKind::Class("AbstractFish"),
            FISH_INTERVAL,
            false,
            TargetFilter::SchoolingFish,
        ))
    };
    if red {
        targets.add_goal(4, land());
        targets.add_goal(4, turtle());
        targets.add_goal(6, fish());
    } else {
        targets.add_goal(4, fish());
        targets.add_goal(6, land());
        targets.add_goal(6, turtle());
    }
}

pub(crate) fn register(add: &mut Goals<'_>, kind: EntityKind) {
    use crate::path_type::PathType;
    add.brain
        .profile
        .set_malus(PathType::DamagingInNeighbor, 0.0);
    add.brain.profile.set_malus(PathType::Damaging, 0.0);
    let follow = kind
        .default_attribute("minecraft:follow_range")
        .unwrap_or(0.0);
    add.brain.navigation.set_required_path_length(32.0, follow);
    add.brain.profile.can_float = true;
    add.goal(0, MobGoal::Fox(FoxGoal::Float));
    add.goal(
        0,
        MobGoal::ClimbOnTopOfPowderSnow(ClimbOnTopOfPowderSnowGoal),
    );
    add.goal(1, MobGoal::Fox(FoxGoal::faceplant()));
    add.goal(2, MobGoal::Fox(FoxGoal::Panic(PanicGoal::new(2.2))));
    add.goal(3, MobGoal::Fox(FoxGoal::Breed(BreedGoal::new(1.0))));
    add.goal(
        4,
        MobGoal::AvoidEntity(AvoidEntityGoal::fox(
            TargetKind::Player,
            FoxAvoid::Player,
            16.0,
            1.6,
            1.4,
        )),
    );
    add.goal(
        4,
        MobGoal::AvoidEntity(AvoidEntityGoal::fox(
            TargetKind::Class("Wolf"),
            FoxAvoid::Wolf,
            8.0,
            1.6,
            1.4,
        )),
    );
    add.goal(
        4,
        MobGoal::AvoidEntity(AvoidEntityGoal::fox(
            TargetKind::Class("PolarBear"),
            FoxAvoid::PolarBear,
            8.0,
            1.6,
            1.4,
        )),
    );
    add.goal(5, MobGoal::Fox(FoxGoal::StalkPrey));
    add.goal(6, MobGoal::Fox(FoxGoal::Pounce));
    add.goal(6, MobGoal::Fox(FoxGoal::seek_shelter(FLEE_SPEED)));
    add.goal(
        7,
        MobGoal::Fox(FoxGoal::Melee(Box::new(MeleeAttackGoal::fox(f64::from(
            MELEE_SPEED,
        ))))),
    );
    let countdown = add.brain.random.next_int(SLEEP_WAIT);
    add.goal(7, MobGoal::Fox(FoxGoal::Sleep(SleepGoal { countdown })));
    add.goal(
        8,
        MobGoal::Fox(FoxGoal::FollowParent(FollowParentGoal::new(1.25))),
    );
    add.goal(9, MobGoal::Fox(FoxGoal::stroll_through_village(200)));
    add.goal(10, MobGoal::Fox(FoxGoal::eat_berries()));
    add.goal(10, MobGoal::LeapAtTarget(LeapAtTargetGoal::new(0.4)));
    add.stroll(11, 1.0);
    add.goal(11, MobGoal::Fox(FoxGoal::SearchForItems));
    add.goal(
        12,
        MobGoal::Fox(FoxGoal::LookAtPlayer(LookAtPlayerGoal::new(
            FOX_LOOK_DISTANCE,
        ))),
    );
    add.goal(13, MobGoal::Fox(FoxGoal::perch_and_search()));
    add.target_goal(3, MobGoal::Fox(FoxGoal::defend_trusted()));
}
