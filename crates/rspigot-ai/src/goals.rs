use crate::action::{TargetReason, TargetRequest};
use crate::random::RandomSource;
use rspigot_blocks::BlockKind;
use rspigot_domain::{EntityId, ItemId, Vec3};
use rspigot_items::ItemKind;

use crate::animal::{BreedGoal, EatBlockGoal, FollowParentGoal, PanicGoal};
use crate::blocks::{BreakDoorGoal, MoveThroughVillageGoal, RemoveTurtleEggGoal};
use crate::brain::AiContext;
use crate::combat::{
    HurtByTargetGoal, LeapAtTargetGoal, MeleeAttackGoal, OcelotAttackGoal, RangedBowAttackGoal,
    SwellGoal,
};
use crate::control::{MAX_HEAD_X_ROT, MAX_HEAD_Y_ROT};
use crate::evade::{AvoidEntityGoal, FleeSunGoal, RestrictSunGoal};
use crate::goal::{Flag, Flags, Goal, adjusted_tick_delay, reduced_tick_delay};
use crate::rabbit::RaidGardenGoal;
use crate::random_pos::{self, mob_light_magic_value};
use crate::targeting::{self, TargetingConditions};
use crate::world::{AiWorld, Living, TargetKind, inflate};

const ITEM_TAGS: &str = "minecraft:item";
const ENTITY_TAGS: &str = "minecraft:entity_type";
const POWDER_SNOW_WALKABLE_MOBS: &str = "minecraft:powder_snow_walkable_mobs";
const STROLL_INTERVAL: i32 = 120;
const NO_ACTION_LIMIT: i32 = 100;
const WATER_AVOIDING_PROBABILITY: f32 = 0.001;
const LOOK_PROBABILITY: f32 = 0.02;
const TEMPT_STOP_DISTANCE: f64 = 2.5;
const TEMPT_CALM_DOWN: i32 = 100;
const TEMPT_SCARE_DISTANCE_SQR: f64 = 36.0;
const TEMPT_SCARE_MOVE_SQR: f64 = 0.010_000_000_000_000_002;
const TEMPT_SCARE_ROTATION: f64 = 5.0;
const TARGET_INTERVAL: i32 = 10;
const UNSEEN_MEMORY_TICKS: i32 = 60;
const SPIDER_LIGHT_LIMIT: f32 = 0.5;
const FLOAT_CHANCE: f32 = 0.8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FloatGoal;

impl FloatGoal {
    fn can_use<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) -> bool {
        let mob = context.mob;
        let threshold = if f64::from(mob.eye_height) < 0.4 {
            0.0
        } else {
            0.4
        };
        mob.fluid.water_height > threshold || mob.fluid.in_lava()
    }

    fn tick<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        if context.brain.random.next_float() < FLOAT_CHANCE {
            context.brain.jump.jump();
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClimbOnTopOfPowderSnowGoal;

impl ClimbOnTopOfPowderSnowGoal {
    fn can_use<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) -> bool {
        let mob = context.mob;
        if !(mob.in_powder_snow || mob.was_in_powder_snow) {
            return false;
        }
        let walkable = rspigot_data::tags::tag(ENTITY_TAGS, POWDER_SNOW_WALKABLE_MOBS)
            .is_some_and(|ids| ids.binary_search(&u32::from(mob.kind.id().0)).is_ok());
        if !walkable {
            return false;
        }
        let state = context.world.state(mob.block_pos().offset(0, 1, 0));
        state.is_some_and(|state| {
            state.kind() == Some(BlockKind::PowderSnow) || state.collision_shape().is_empty()
        })
    }

    fn tick<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        context.brain.jump.jump();
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RandomStrollGoal {
    speed_modifier: f64,
    interval: i32,
    force_trigger: bool,
    check_no_action_time: bool,
    water_avoiding: Option<f32>,
    wanted: Vec3,
}

impl RandomStrollGoal {
    pub fn new(speed_modifier: f64) -> Self {
        Self {
            speed_modifier,
            interval: STROLL_INTERVAL,
            force_trigger: false,
            check_no_action_time: true,
            water_avoiding: None,
            wanted: Vec3::ZERO,
        }
    }

    pub fn water_avoiding(speed_modifier: f64) -> Self {
        Self {
            water_avoiding: Some(WATER_AVOIDING_PROBABILITY),
            ..Self::new(speed_modifier)
        }
    }

    pub fn water_avoiding_with(speed_modifier: f64, probability: f32) -> Self {
        Self {
            water_avoiding: Some(probability),
            ..Self::new(speed_modifier)
        }
    }

    pub fn trigger(&mut self) {
        self.force_trigger = true;
    }

    fn position<W: AiWorld>(&self, context: &mut AiContext<'_, W>) -> Option<Vec3> {
        let Some(probability) = self.water_avoiding else {
            return random_pos::default_pos(context, 10, 7);
        };
        if context.mob.fluid.in_water {
            let land = random_pos::land_pos(context, 15, 7);
            return land.or_else(|| random_pos::default_pos(context, 10, 7));
        }
        if context.brain.random.next_float() >= probability {
            random_pos::land_pos(context, 10, 7)
        } else {
            random_pos::default_pos(context, 10, 7)
        }
    }

    fn can_use<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) -> bool {
        if !self.force_trigger {
            if self.check_no_action_time && context.mob.no_action_time >= NO_ACTION_LIMIT {
                return false;
            }
            let bound = reduced_tick_delay(self.interval);
            if context.brain.random.next_int(bound) != 0 {
                return false;
            }
        }
        let Some(position) = self.position(context) else {
            return false;
        };
        self.wanted = position;
        self.force_trigger = false;
        true
    }

    fn can_continue_to_use<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) -> bool {
        !context.brain.navigation.is_done()
    }

    fn start<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        context.move_to_pos(self.wanted, self.speed_modifier);
    }

    fn stop<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        context.stop_navigation();
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LookAtPlayerGoal {
    look_distance: f32,
    probability: f32,
    only_horizontal: bool,
    look_at: Option<EntityId>,
    look_time: i32,
}

impl LookAtPlayerGoal {
    pub fn new(look_distance: f32) -> Self {
        Self {
            look_distance,
            probability: LOOK_PROBABILITY,
            only_horizontal: false,
            look_at: None,
            look_time: 0,
        }
    }

    pub fn look_at(&self) -> Option<EntityId> {
        self.look_at
    }

    fn can_use<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) -> bool {
        if context.brain.random.next_float() >= self.probability {
            return false;
        }
        let conditions = TargetingConditions::for_non_combat().range(f64::from(self.look_distance));
        let origin = context.mob.eye_position();
        self.look_at = targeting::nearest_player(context, conditions, |_| true, origin)
            .map(|player| player.id);
        self.look_at.is_some()
    }

    fn can_continue_to_use<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) -> bool {
        let Some(target) = self.look_at.and_then(|id| context.world.living(id)) else {
            return false;
        };
        if !target.alive {
            return false;
        }
        let distance = f64::from(self.look_distance * self.look_distance);
        context.mob.distance_to_sqr(target.position) <= distance && self.look_time > 0
    }

    fn start<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        let ticks = 40 + context.brain.random.next_int(40);
        self.look_time = adjusted_tick_delay(false, ticks);
    }

    fn stop(&mut self) {
        self.look_at = None;
    }

    fn tick<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        let Some(target) = self.look_at.and_then(|id| context.world.living(id)) else {
            return;
        };
        if !target.alive {
            return;
        }
        let y = if self.only_horizontal {
            context.mob.eye_y()
        } else {
            target.eye_y()
        };
        context
            .brain
            .look
            .look_at(Vec3::new(target.position.x, y, target.position.z));
        self.look_time -= 1;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct RandomLookAroundGoal {
    rel_x: f64,
    rel_z: f64,
    look_time: i32,
}

impl RandomLookAroundGoal {
    fn can_use<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) -> bool {
        context.brain.random.next_float() < LOOK_PROBABILITY
    }

    fn start<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        let angle = std::f64::consts::TAU * context.brain.random.next_double();
        self.rel_x = angle.cos();
        self.rel_z = angle.sin();
        self.look_time = 20 + context.brain.random.next_int(20);
    }

    fn tick<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        self.look_time -= 1;
        let mob = context.mob;
        let at = Vec3::new(
            mob.position.x + self.rel_x,
            mob.eye_y(),
            mob.position.z + self.rel_z,
        );
        context.brain.look.look_at(at);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemptItems {
    Tag(&'static str),
    Item(ItemKind),
}

impl TemptItems {
    pub fn matches(self, item: Option<ItemId>) -> bool {
        let Some(item) = item.filter(|item| *item != ItemId::AIR) else {
            return false;
        };
        match self {
            Self::Tag(name) => rspigot_data::tags::tag(ITEM_TAGS, name)
                .is_some_and(|ids| ids.binary_search(&u32::from(item.0)).is_ok()),
            Self::Item(kind) => kind.id() == item,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TemptGoal {
    speed_modifier: f64,
    items: TemptItems,
    stop_distance: f64,
    player: Option<EntityId>,
    calm_down: i32,
    running: bool,
    scare: Scare,
    last_position: Vec3,
    last_pitch: f64,
    last_yaw: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Scare {
    Never,
    UnlessTrusting,
}

impl TemptGoal {
    pub fn new(speed_modifier: f64, items: TemptItems) -> Self {
        Self {
            speed_modifier,
            items,
            stop_distance: TEMPT_STOP_DISTANCE,
            player: None,
            calm_down: 0,
            running: false,
            scare: Scare::Never,
            last_position: Vec3::ZERO,
            last_pitch: 0.0,
            last_yaw: 0.0,
        }
    }

    pub fn scared_by_movement(speed_modifier: f64, items: TemptItems) -> Self {
        Self {
            scare: Scare::UnlessTrusting,
            ..Self::new(speed_modifier, items)
        }
    }

    pub fn player(&self) -> Option<EntityId> {
        self.player
    }

    pub fn is_running(&self) -> bool {
        self.running
    }

    fn can_use<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) -> bool {
        if self.calm_down > 0 {
            self.calm_down -= 1;
            return false;
        }
        let items = self.items;
        let conditions = TargetingConditions::for_non_combat()
            .ignore_line_of_sight()
            .range(context.mob.tempt_range);
        let origin = context.mob.position;
        let found = targeting::nearest_player(
            context,
            conditions,
            |player| items.matches(player.main_hand) || items.matches(player.off_hand),
            origin,
        )
        .map(|player| player.id);
        let Some(player) = found else {
            self.player = None;
            return false;
        };
        let brain = &mut *context.brain;
        if brain.defer_targets
            && let Some(verdict) = brain.tempt_verdict.take()
        {
            self.player = verdict;
            return self.player.is_some();
        }
        brain.target_requests.push(TargetRequest {
            from: None,
            to: Some(player),
            reason: TargetReason::Tempt,
            tempt: true,
        });
        if brain.defer_targets {
            return self.running && self.player == Some(player);
        }
        self.player = Some(player);
        true
    }

    fn can_scare<W: AiWorld>(&self, context: &AiContext<'_, W>) -> bool {
        match self.scare {
            Scare::Never => false,
            Scare::UnlessTrusting => !context.mob.trusting,
        }
    }

    fn can_continue_to_use<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) -> bool {
        if self.can_scare(context) {
            let Some(player) = self.player.and_then(|id| context.world.living(id)) else {
                return false;
            };
            if context.mob.distance_to_sqr(player.position) < TEMPT_SCARE_DISTANCE_SQR {
                if player.position.distance_squared(self.last_position) > TEMPT_SCARE_MOVE_SQR {
                    return false;
                }
                if (f64::from(player.pitch) - self.last_pitch).abs() > TEMPT_SCARE_ROTATION
                    || (f64::from(player.yaw) - self.last_yaw).abs() > TEMPT_SCARE_ROTATION
                {
                    return false;
                }
            } else {
                self.last_position = player.position;
            }
            self.last_pitch = f64::from(player.pitch);
            self.last_yaw = f64::from(player.yaw);
        }
        self.can_use(context)
    }

    fn start<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        if let Some(player) = self.player.and_then(|id| context.world.living(id)) {
            self.last_position = player.position;
        }
        self.running = true;
    }

    fn stop<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        self.player = None;
        context.stop_navigation();
        self.calm_down = reduced_tick_delay(TEMPT_CALM_DOWN);
        self.running = false;
    }

    fn tick<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        let Some(player) = self.player.and_then(|id| context.world.living(id)) else {
            return;
        };
        let at = Vec3::new(player.position.x, player.eye_y(), player.position.z);
        context
            .brain
            .look
            .set_look_at(at, MAX_HEAD_Y_ROT + 20.0, MAX_HEAD_X_ROT);
        if context.mob.distance_to_sqr(player.position) < self.stop_distance * self.stop_distance {
            context.stop_navigation();
        } else {
            context.move_to_entity(&player, self.speed_modifier);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetFilter {
    Any,
    BabyOnLand,
}

impl TargetFilter {
    fn accepts(self, target: &Living) -> bool {
        match self {
            Self::Any => true,
            Self::BabyOnLand => target.baby && !target.in_water,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NearestAttackableTargetGoal {
    kind: TargetKind,
    random_interval: i32,
    must_see: bool,
    filter: TargetFilter,
    needs_darkness: bool,
    found: Option<Living>,
    unseen_ticks: i32,
}

impl NearestAttackableTargetGoal {
    pub fn new(kind: TargetKind, must_see: bool) -> Self {
        Self::with(kind, TARGET_INTERVAL, must_see, TargetFilter::Any)
    }

    pub fn with(kind: TargetKind, interval: i32, must_see: bool, filter: TargetFilter) -> Self {
        Self {
            kind,
            random_interval: reduced_tick_delay(interval),
            must_see,
            filter,
            needs_darkness: false,
            found: None,
            unseen_ticks: 0,
        }
    }

    pub fn spider(kind: TargetKind) -> Self {
        Self {
            needs_darkness: true,
            ..Self::new(kind, true)
        }
    }

    fn find_target<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        let conditions = TargetingConditions::for_combat().range(context.mob.follow_range);
        let origin = context.mob.eye_position();
        let filter = self.filter;
        self.found = if self.kind == TargetKind::Player {
            targeting::nearest_player(context, conditions, |t| filter.accepts(t), origin)
        } else {
            let follow = context.mob.follow_range;
            let area = inflate(&context.mob.bounds(), follow, follow, follow);
            let candidates = context.world.living_of(self.kind, &area);
            targeting::nearest(
                context,
                candidates,
                conditions,
                |t| filter.accepts(t),
                origin,
            )
        };
    }

    fn can_use<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) -> bool {
        if self.needs_darkness
            && mob_light_magic_value(context.world, context.mob) >= SPIDER_LIGHT_LIMIT
        {
            return false;
        }
        if self.random_interval > 0 && context.brain.random.next_int(self.random_interval) != 0 {
            return false;
        }
        self.find_target(context);
        self.found.is_some()
    }

    fn can_continue_to_use<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) -> bool {
        let Some(target) = context.target() else {
            return false;
        };
        if !targeting::can_attack(context.world, &target) {
            return false;
        }
        let follow = context.mob.follow_range;
        if context.mob.distance_to_sqr(target.position) > follow * follow {
            return false;
        }
        if self.must_see {
            let world = context.world;
            if context
                .brain
                .sensing
                .has_line_of_sight(world, context.mob, &target)
            {
                self.unseen_ticks = 0;
            } else {
                self.unseen_ticks += 1;
                if self.unseen_ticks > reduced_tick_delay(UNSEEN_MEMORY_TICKS) {
                    return false;
                }
            }
        }
        context.set_target(Some(target), TargetReason::ClosestEntity);
        true
    }

    fn start<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        let reason = if self.found.is_some_and(|found| found.player) {
            TargetReason::ClosestPlayer
        } else {
            TargetReason::ClosestEntity
        };
        context.set_target(self.found, reason);
        self.unseen_ticks = 0;
    }

    fn stop<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        context.set_target(None, TargetReason::ForgotTarget);
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum MobGoal {
    Float(FloatGoal),
    ClimbOnTopOfPowderSnow(ClimbOnTopOfPowderSnowGoal),
    Stroll(RandomStrollGoal),
    LookAtPlayer(LookAtPlayerGoal),
    RandomLookAround(RandomLookAroundGoal),
    Tempt(TemptGoal),
    NearestAttackableTarget(NearestAttackableTargetGoal),
    Panic(PanicGoal),
    Breed(BreedGoal),
    FollowParent(FollowParentGoal),
    EatBlock(EatBlockGoal),
    MeleeAttack(MeleeAttackGoal),
    RangedBowAttack(RangedBowAttackGoal),
    Swell(SwellGoal),
    LeapAtTarget(LeapAtTargetGoal),
    HurtByTarget(HurtByTargetGoal),
    AvoidEntity(AvoidEntityGoal),
    FleeSun(FleeSunGoal),
    RestrictSun(RestrictSunGoal),
    RemoveTurtleEgg(RemoveTurtleEggGoal),
    BreakDoor(BreakDoorGoal),
    MoveThroughVillage(MoveThroughVillageGoal),
    OcelotAttack(OcelotAttackGoal),
    RaidGarden(RaidGardenGoal),
}

impl<W: AiWorld> Goal<AiContext<'_, W>> for MobGoal {
    fn flags(&self) -> Flags {
        let movement = Flags::of(Flag::Move);
        match self {
            Self::Float(_) | Self::ClimbOnTopOfPowderSnow(_) => Flags::of(Flag::Jump),
            Self::Stroll(_)
            | Self::Panic(_)
            | Self::Swell(_)
            | Self::AvoidEntity(_)
            | Self::FleeSun(_)
            | Self::MoveThroughVillage(_) => movement,
            Self::LookAtPlayer(_) => Flags::of(Flag::Look),
            Self::RandomLookAround(_)
            | Self::Tempt(_)
            | Self::Breed(_)
            | Self::MeleeAttack(_)
            | Self::OcelotAttack(_)
            | Self::RangedBowAttack(_) => movement.with(Flag::Look),
            Self::EatBlock(_) => movement.with(Flag::Look).with(Flag::Jump),
            Self::LeapAtTarget(_) | Self::RemoveTurtleEgg(_) | Self::RaidGarden(_) => {
                movement.with(Flag::Jump)
            }
            Self::NearestAttackableTarget(_) | Self::HurtByTarget(_) => Flags::of(Flag::Target),
            Self::FollowParent(_) | Self::RestrictSun(_) | Self::BreakDoor(_) => Flags::NONE,
        }
    }

    fn can_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        match self {
            Self::Float(goal) => goal.can_use(context),
            Self::ClimbOnTopOfPowderSnow(goal) => goal.can_use(context),
            Self::OcelotAttack(goal) => goal.can_use(context),
            Self::RaidGarden(goal) => goal.can_use(context),
            Self::Stroll(goal) => goal.can_use(context),
            Self::LookAtPlayer(goal) => goal.can_use(context),
            Self::RandomLookAround(goal) => goal.can_use(context),
            Self::Tempt(goal) => goal.can_use(context),
            Self::NearestAttackableTarget(goal) => goal.can_use(context),
            Self::Panic(goal) => goal.can_use(context),
            Self::Breed(goal) => goal.can_use(context),
            Self::FollowParent(goal) => goal.can_use(context),
            Self::EatBlock(goal) => goal.can_use(context),
            Self::MeleeAttack(goal) => goal.can_use(context),
            Self::RangedBowAttack(goal) => goal.can_use(context),
            Self::Swell(goal) => goal.can_use(context),
            Self::LeapAtTarget(goal) => goal.can_use(context),
            Self::HurtByTarget(goal) => goal.can_use(context),
            Self::AvoidEntity(goal) => goal.can_use(context),
            Self::FleeSun(goal) => goal.can_use(context),
            Self::RestrictSun(goal) => goal.can_use(context),
            Self::RemoveTurtleEgg(goal) => goal.can_use(context),
            Self::BreakDoor(goal) => goal.can_use(context),
            Self::MoveThroughVillage(goal) => goal.can_use(context),
        }
    }

    fn can_continue_to_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        match self {
            Self::Float(goal) => goal.can_use(context),
            Self::ClimbOnTopOfPowderSnow(goal) => goal.can_use(context),
            Self::OcelotAttack(goal) => goal.can_continue_to_use(context),
            Self::RaidGarden(goal) => goal.can_continue_to_use(context),
            Self::Stroll(goal) => goal.can_continue_to_use(context),
            Self::LookAtPlayer(goal) => goal.can_continue_to_use(context),
            Self::RandomLookAround(goal) => goal.look_time >= 0,
            Self::Tempt(goal) => goal.can_continue_to_use(context),
            Self::NearestAttackableTarget(goal) => goal.can_continue_to_use(context),
            Self::AvoidEntity(goal) => goal.can_continue_to_use(context),
            Self::Panic(_) | Self::FleeSun(_) | Self::MoveThroughVillage(_) => {
                !context.brain.navigation.is_done()
            }
            Self::Breed(goal) => goal.can_continue_to_use(context),
            Self::FollowParent(goal) => goal.can_continue_to_use(context),
            Self::EatBlock(goal) => goal.eat_animation_tick() > 0,
            Self::MeleeAttack(goal) => goal.can_continue_to_use(context),
            Self::RangedBowAttack(goal) => goal.can_continue_to_use(context),
            Self::Swell(goal) => goal.can_use(context),
            Self::LeapAtTarget(goal) => goal.can_continue_to_use(context),
            Self::HurtByTarget(goal) => goal.memory().can_continue_to_use(context),
            Self::RestrictSun(goal) => goal.can_use(context),
            Self::RemoveTurtleEgg(goal) => goal.can_continue_to_use(context),
            Self::BreakDoor(goal) => !goal.passed() && goal.can_continue_to_use(context),
        }
    }

    fn start(&mut self, context: &mut AiContext<'_, W>) {
        match self {
            Self::Float(_)
            | Self::ClimbOnTopOfPowderSnow(_)
            | Self::OcelotAttack(_)
            | Self::Breed(_)
            | Self::MoveThroughVillage(_) => {}
            Self::RaidGarden(goal) => goal.start(context),
            Self::HurtByTarget(goal) => goal.start(context),
            Self::Stroll(goal) => goal.start(context),
            Self::LookAtPlayer(goal) => goal.start(context),
            Self::RandomLookAround(goal) => goal.start(context),
            Self::Tempt(goal) => goal.start(context),
            Self::NearestAttackableTarget(goal) => goal.start(context),
            Self::Panic(goal) => goal.start(context),
            Self::FollowParent(goal) => goal.start(),
            Self::EatBlock(goal) => goal.start(context),
            Self::MeleeAttack(goal) => goal.start(context),
            Self::RangedBowAttack(goal) => goal.start(context),
            Self::Swell(goal) => goal.start(context),
            Self::LeapAtTarget(goal) => goal.start(context),
            Self::AvoidEntity(goal) => goal.start(context),
            Self::FleeSun(goal) => goal.start(context),
            Self::RestrictSun(goal) => goal.start(context),
            Self::RemoveTurtleEgg(goal) => goal.start(context),
            Self::BreakDoor(goal) => goal.start(context),
        }
    }

    fn stop(&mut self, context: &mut AiContext<'_, W>) {
        match self {
            Self::Float(_)
            | Self::ClimbOnTopOfPowderSnow(_)
            | Self::RaidGarden(_)
            | Self::RandomLookAround(_)
            | Self::LeapAtTarget(_)
            | Self::FleeSun(_)
            | Self::MoveThroughVillage(_) => {}
            Self::Stroll(goal) => goal.stop(context),
            Self::LookAtPlayer(goal) => goal.stop(),
            Self::Tempt(goal) => goal.stop(context),
            Self::NearestAttackableTarget(goal) => goal.stop(context),
            Self::Panic(goal) => goal.stop(),
            Self::OcelotAttack(goal) => goal.stop(context),
            Self::Breed(goal) => goal.stop(),
            Self::FollowParent(goal) => goal.stop(),
            Self::EatBlock(goal) => goal.stop(),
            Self::MeleeAttack(goal) => goal.stop(context),
            Self::RangedBowAttack(goal) => goal.stop(context),
            Self::Swell(goal) => goal.stop(),
            Self::HurtByTarget(goal) => goal.memory().stop(context),
            Self::AvoidEntity(goal) => goal.stop(),
            Self::RestrictSun(goal) => goal.stop(context),
            Self::RemoveTurtleEgg(goal) => goal.stop(context),
            Self::BreakDoor(goal) => goal.stop(context),
        }
    }

    fn requires_update_every_tick(&self) -> bool {
        matches!(
            self,
            Self::Float(_)
                | Self::ClimbOnTopOfPowderSnow(_)
                | Self::OcelotAttack(_)
                | Self::RaidGarden(_)
                | Self::RandomLookAround(_)
                | Self::MeleeAttack(_)
                | Self::RangedBowAttack(_)
                | Self::Swell(_)
                | Self::RemoveTurtleEgg(_)
                | Self::BreakDoor(_)
        )
    }

    fn tick(&mut self, context: &mut AiContext<'_, W>) {
        match self {
            Self::Float(goal) => goal.tick(context),
            Self::ClimbOnTopOfPowderSnow(goal) => goal.tick(context),
            Self::OcelotAttack(goal) => goal.tick(context),
            Self::RaidGarden(goal) => goal.tick(context),
            Self::Panic(goal) => goal.tick(context),
            Self::LookAtPlayer(goal) => goal.tick(context),
            Self::RandomLookAround(goal) => goal.tick(context),
            Self::Tempt(goal) => goal.tick(context),
            Self::Breed(goal) => goal.tick(context),
            Self::FollowParent(goal) => goal.tick(context),
            Self::EatBlock(goal) => goal.tick(context),
            Self::MeleeAttack(goal) => goal.tick(context),
            Self::RangedBowAttack(goal) => goal.tick(context),
            Self::Swell(goal) => goal.tick(context),
            Self::AvoidEntity(goal) => goal.tick(context),
            Self::RemoveTurtleEgg(goal) => goal.tick(context),
            Self::BreakDoor(goal) => goal.tick(context),
            Self::Stroll(_)
            | Self::NearestAttackableTarget(_)
            | Self::LeapAtTarget(_)
            | Self::HurtByTarget(_)
            | Self::FleeSun(_)
            | Self::RestrictSun(_)
            | Self::MoveThroughVillage(_) => {}
        }
    }
}
