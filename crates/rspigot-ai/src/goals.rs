use crate::action::{TargetReason, TargetRequest};
use crate::random::RandomSource;
use rspigot_blocks::BlockKind;
use rspigot_domain::{EntityId, ItemId, Vec3};
use rspigot_entities::EntityKind;
use rspigot_items::ItemKind;

use crate::animal::{BreedGoal, EatBlockGoal, FollowParentGoal, PanicGoal};
use crate::blocks::{BreakDoorGoal, MoveThroughVillageGoal, RemoveTurtleEggGoal};
use crate::brain::AiContext;
use crate::combat::{
    HurtByTargetGoal, LeapAtTargetGoal, MeleeAttackGoal, OcelotAttackGoal, RangedBowAttackGoal,
    SwellGoal,
};
use crate::control::MAX_HEAD_Y_ROT;
use crate::evade::{AvoidEntityGoal, FleeSunGoal, RestrictSunGoal};
use crate::goal::{Flag, Flags, Goal, adjusted_tick_delay, reduced_tick_delay};
use crate::neutral::ResetUniversalAngerGoal;
use crate::rabbit::RaidGardenGoal;
use crate::random_pos::{self, mob_light_magic_value};
use crate::ranged::RangedAttackGoal;
use crate::restriction::MoveTowardsRestrictionGoal;
use crate::tame::{FollowOwnerGoal, OwnerTargetGoal, SitWhenOrderedToGoal};
use crate::targeting::{self, TargetingConditions};
use crate::world::{AiWorld, Living, MobView, TargetKind, inflate};

const ITEM_TAGS: &str = "minecraft:item";
const ENTITY_TAGS: &str = "minecraft:entity_type";
const POWDER_SNOW_WALKABLE_MOBS: &str = "minecraft:powder_snow_walkable_mobs";
const STROLL_INTERVAL: i32 = 120;
const NO_ACTION_LIMIT: i32 = 100;
const WATER_AVOIDING_PROBABILITY: f32 = 0.001;
const LOOK_PROBABILITY: f32 = 0.02;
const LOOK_AT_MOB_HEIGHT: f64 = 3.0;
const TEMPT_STOP_DISTANCE: f64 = 2.5;
const TEMPT_CALM_DOWN: i32 = 100;
const TEMPT_SCARE_DISTANCE_SQR: f64 = 36.0;
const TEMPT_SCARE_MOVE_SQR: f64 = 0.010_000_000_000_000_002;
const TEMPT_SCARE_ROTATION: f64 = 5.0;
const TARGET_INTERVAL: i32 = 10;
const UNSEEN_MEMORY_TICKS: i32 = 60;
const SPIDER_LIGHT_LIMIT: f32 = 0.5;
const FLOAT_CHANCE: f32 = 0.8;
const GHAST_TARGET_HEIGHT: f64 = 4.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FloatGoal;

impl FloatGoal {
    pub(crate) fn can_use<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) -> bool {
        let mob = context.mob;
        let threshold = if f64::from(mob.eye_height) < 0.4 {
            0.0
        } else {
            0.4
        };
        mob.fluid.water_height > threshold || mob.fluid.in_lava()
    }

    pub(crate) fn tick<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
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

const FLY_HORIZONTAL: i32 = 8;
const FLY_VERTICAL: i32 = 7;
const FLY_HOVER_MAX: i32 = 3;
const FLY_HOVER_MIN: i32 = 1;
const FLY_AIR_VERTICAL: i32 = 4;
const FLY_AIR_HEIGHT: i32 = -2;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RandomStrollGoal {
    speed_modifier: f64,
    interval: i32,
    force_trigger: bool,
    check_no_action_time: bool,
    water_avoiding: Option<f32>,
    flying: bool,
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
            flying: false,
            wanted: Vec3::ZERO,
        }
    }

    pub fn water_avoiding_flying(speed_modifier: f64) -> Self {
        Self {
            water_avoiding: Some(WATER_AVOIDING_PROBABILITY),
            flying: true,
            ..Self::new(speed_modifier)
        }
    }

    pub fn with_interval(speed_modifier: f64, interval: i32) -> Self {
        Self {
            interval,
            ..Self::new(speed_modifier)
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

    pub fn with_settings(speed_modifier: f64, interval: i32, check_no_action_time: bool) -> Self {
        Self {
            interval,
            check_no_action_time,
            ..Self::new(speed_modifier)
        }
    }

    pub fn trigger(&mut self) {
        self.force_trigger = true;
    }

    pub(crate) fn position<W: AiWorld>(&self, context: &mut AiContext<'_, W>) -> Option<Vec3> {
        if self.flying {
            let view = rspigot_domain::mth::view_vector(context.mob.pitch, context.mob.head_yaw);
            let direction = (view.x, view.z);
            return random_pos::hover_pos(
                context,
                FLY_HORIZONTAL,
                FLY_VERTICAL,
                direction,
                random_pos::HALF_PI,
                FLY_HOVER_MAX,
                FLY_HOVER_MIN,
            )
            .or_else(|| {
                random_pos::air_and_water_pos(
                    context,
                    FLY_HORIZONTAL,
                    FLY_AIR_VERTICAL,
                    FLY_AIR_HEIGHT,
                    direction,
                    random_pos::HALF_PI,
                )
            });
        }
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

    pub(crate) fn can_use<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) -> bool {
        let this = *self;
        self.can_use_with(context, |context| this.position(context))
    }

    pub(crate) fn can_use_with<W: AiWorld>(
        &mut self,
        context: &mut AiContext<'_, W>,
        position: impl FnOnce(&mut AiContext<'_, W>) -> Option<Vec3>,
    ) -> bool {
        if !self.force_trigger {
            if self.check_no_action_time && context.mob.no_action_time >= NO_ACTION_LIMIT {
                return false;
            }
            let bound = reduced_tick_delay(self.interval);
            if context.brain.random.next_int(bound) != 0 {
                return false;
            }
        }
        let Some(position) = position(context) else {
            return false;
        };
        self.wanted = position;
        self.force_trigger = false;
        true
    }

    pub(crate) fn can_continue_to_use<W: AiWorld>(
        &mut self,
        context: &mut AiContext<'_, W>,
    ) -> bool {
        !context.brain.navigation.is_done()
    }

    pub(crate) fn start<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        context.move_to_pos(self.wanted, self.speed_modifier);
    }

    pub(crate) fn stop<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        context.stop_navigation();
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LookAtPlayerGoal {
    interact: bool,
    look_distance: f32,
    probability: f32,
    only_horizontal: bool,
    at_mobs: bool,
    mob_class: &'static str,
    look_at: Option<EntityId>,
    look_time: i32,
}

impl LookAtPlayerGoal {
    pub fn new(look_distance: f32) -> Self {
        Self {
            interact: false,
            look_distance,
            probability: LOOK_PROBABILITY,
            only_horizontal: false,
            at_mobs: false,
            mob_class: "Mob",
            look_at: None,
            look_time: 0,
        }
    }

    pub fn at_class(look_distance: f32, probability: f32, class: &'static str) -> Self {
        Self {
            at_mobs: true,
            mob_class: class,
            ..Self::with_probability(look_distance, probability)
        }
    }

    pub fn with_probability(look_distance: f32, probability: f32) -> Self {
        Self {
            probability,
            ..Self::new(look_distance)
        }
    }

    pub fn horizontal(look_distance: f32, probability: f32) -> Self {
        Self {
            only_horizontal: true,
            ..Self::with_probability(look_distance, probability)
        }
    }

    pub fn interact(look_distance: f32, probability: f32) -> Self {
        Self {
            interact: true,
            ..Self::with_probability(look_distance, probability)
        }
    }

    pub fn at_mobs(look_distance: f32) -> Self {
        Self {
            at_mobs: true,
            ..Self::new(look_distance)
        }
    }

    pub fn is_interact(&self) -> bool {
        self.interact
    }

    pub(crate) fn can_use_trading(&mut self, trading_player: Option<EntityId>) -> bool {
        self.look_at = trading_player;
        self.look_at.is_some()
    }

    pub fn look_at(&self) -> Option<EntityId> {
        self.look_at
    }

    pub(crate) fn can_use<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) -> bool {
        if context.brain.random.next_float() >= self.probability {
            return false;
        }
        let conditions = TargetingConditions::for_non_combat().range(f64::from(self.look_distance));
        let origin = context.mob.eye_position();
        self.look_at = if self.at_mobs {
            let reach = f64::from(self.look_distance);
            let area = inflate(&context.mob.bounds(), reach, LOOK_AT_MOB_HEIGHT, reach);
            let world = context.world;
            let candidates = world.living_of(TargetKind::Class(self.mob_class), &area);
            targeting::nearest(context, candidates, conditions, |_| true, origin)
        } else {
            targeting::nearest_player(context, conditions, |_| true, origin)
        }
        .map(|found| found.id);
        self.look_at.is_some()
    }

    pub(crate) fn can_continue_to_use<W: AiWorld>(
        &mut self,
        context: &mut AiContext<'_, W>,
    ) -> bool {
        let Some(target) = self.look_at.and_then(|id| context.world.living(id)) else {
            return false;
        };
        if !target.alive {
            return false;
        }
        let distance = f64::from(self.look_distance * self.look_distance);
        context.mob.distance_to_sqr(target.position) <= distance && self.look_time > 0
    }

    pub(crate) fn start<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        let ticks = 40 + context.brain.random.next_int(40);
        self.look_time = adjusted_tick_delay(false, ticks);
    }

    pub(crate) fn stop(&mut self) {
        self.look_at = None;
    }

    pub(crate) fn tick<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
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
    HappyGhast,
}

impl TemptItems {
    pub fn matches(self, item: Option<ItemId>) -> bool {
        self.matches_for(false, item)
    }

    pub fn matches_for(self, food_only: bool, item: Option<ItemId>) -> bool {
        let Some(item) = item.filter(|item| *item != ItemId::AIR) else {
            return false;
        };
        match self {
            Self::Tag(name) => rspigot_data::tags::tag(ITEM_TAGS, name)
                .is_some_and(|ids| ids.binary_search(&u32::from(item.0)).is_ok()),
            Self::Item(kind) => kind.id() == item,
            Self::HappyGhast => {
                let tag = if food_only {
                    rspigot_domain::entity::species::happy_ghast::FOOD_TAG
                } else {
                    rspigot_domain::entity::species::happy_ghast::TEMPT_ITEMS_TAG
                };
                Self::Tag(tag).matches(Some(item))
            }
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
    non_tame: bool,
    selected: Option<EntityId>,
    last_position: Vec3,
    last_pitch: f64,
    last_yaw: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Scare {
    Never,
    UnlessTrusting,
    UnlessSelected,
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
            non_tame: false,
            selected: None,
            last_position: Vec3::ZERO,
            last_pitch: 0.0,
            last_yaw: 0.0,
        }
    }

    pub fn with_stop_distance(mut self, stop_distance: f64) -> Self {
        self.stop_distance = stop_distance;
        self
    }

    pub fn scared_by_movement(speed_modifier: f64, items: TemptItems) -> Self {
        Self {
            scare: Scare::UnlessTrusting,
            ..Self::new(speed_modifier, items)
        }
    }

    pub fn cat(speed_modifier: f64, items: TemptItems) -> Self {
        Self {
            scare: Scare::UnlessSelected,
            non_tame: true,
            ..Self::new(speed_modifier, items)
        }
    }

    pub fn selected(&self) -> Option<EntityId> {
        self.selected
    }

    pub fn player(&self) -> Option<EntityId> {
        self.player
    }

    pub fn is_running(&self) -> bool {
        self.running
    }

    fn can_use<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) -> bool {
        self.find_player(context) && !(self.non_tame && context.mob.tamed)
    }

    fn find_player<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) -> bool {
        if self.calm_down > 0 {
            self.calm_down -= 1;
            return false;
        }
        let items = self.items;
        let food_only = context.mob.species.happy_ghast.wearing_harness || context.mob.baby;
        let conditions = TargetingConditions::for_non_combat()
            .ignore_line_of_sight()
            .range(context.mob.tempt_range);
        let origin = context.mob.position;
        let found = targeting::nearest_player(
            context,
            conditions,
            |player| {
                items.matches_for(food_only, player.main_hand)
                    || items.matches_for(food_only, player.off_hand)
            },
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
            Scare::UnlessSelected => !(self.selected.is_some() && self.selected == self.player),
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
        self.follow_player(context);
        if self.scare == Scare::UnlessSelected {
            let random = &mut context.brain.random;
            if self.selected.is_none() && random.next_int(adjusted_tick_delay(false, 600)) == 0 {
                self.selected = self.player;
            } else if random.next_int(adjusted_tick_delay(false, 500)) == 0 {
                self.selected = None;
            }
        }
    }

    fn follow_player<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        let Some(player) = self.player.and_then(|id| context.world.living(id)) else {
            return;
        };
        let at = Vec3::new(player.position.x, player.eye_y(), player.position.z);
        context
            .brain
            .look
            .set_look_at(at, MAX_HEAD_Y_ROT + 20.0, context.mob.max_head_x_rot());
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
    AngryAt,
    ChickenOrRabbit,
    SchoolingFish,
    Enemy,
    EnemyNonCreeper,
    WithinFourBlocksVertically,
    Prey,
    MobIsAdult,
    WitherLiving,
    GuardianAttack,
    DrownedOk,
    HealableRaider,
    Attackable,
    NotBaby,
    Untamed,
    NotZombifiedPiglin,
}

impl TargetFilter {
    fn accepts<W: AiWorld>(self, mob: &MobView, world: &W, target: &Living) -> bool {
        match self {
            Self::Any => true,
            Self::BabyOnLand => target.baby && !target.in_water,
            Self::AngryAt => targeting::is_angry_at(mob, world, target),
            Self::ChickenOrRabbit => {
                target.kind == EntityKind::Chicken || target.kind == EntityKind::Rabbit
            }
            Self::SchoolingFish => target.kind.extends("AbstractSchoolingFish"),
            Self::WithinFourBlocksVertically => {
                (target.position.y - mob.position.y).abs() <= GHAST_TARGET_HEIGHT
            }
            Self::Enemy => crate::iron_golem::is_enemy(target.kind),
            Self::EnemyNonCreeper => {
                crate::iron_golem::is_enemy(target.kind) && target.kind != EntityKind::Creeper
            }
            Self::Prey => matches!(
                target.kind,
                EntityKind::Sheep | EntityKind::Rabbit | EntityKind::Fox
            ),
            Self::MobIsAdult => !mob.baby,
            Self::WitherLiving => crate::wither::living_selector(target),
            Self::GuardianAttack => crate::guardian::attack_selector(target, mob),
            Self::DrownedOk => crate::drowned::drowned_ok_target(world, target),
            Self::Attackable => target.kind != EntityKind::ArmorStand,
            Self::NotBaby => !target.baby,
            Self::Untamed => !target.facts.tamed,
            Self::NotZombifiedPiglin => target.kind != EntityKind::ZombifiedPiglin,
            Self::HealableRaider => {
                mob.species.raider.has_active_raid() && target.kind != EntityKind::Witch
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Special {
    Plain,
    ShulkerNearest,
    ShulkerDefense,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NearestAttackableTargetGoal {
    special: Special,
    kind: TargetKind,
    random_interval: i32,
    must_see: bool,
    filter: TargetFilter,
    needs_darkness: bool,
    non_tame: bool,
    follow_scale: f64,
    found: Option<Living>,
    unseen_ticks: i32,
    unseen_memory: i32,
}

impl NearestAttackableTargetGoal {
    pub fn new(kind: TargetKind, must_see: bool) -> Self {
        Self::with(kind, TARGET_INTERVAL, must_see, TargetFilter::Any)
    }

    pub fn with(kind: TargetKind, interval: i32, must_see: bool, filter: TargetFilter) -> Self {
        Self {
            special: Special::Plain,
            kind,
            random_interval: reduced_tick_delay(interval),
            must_see,
            filter,
            needs_darkness: false,
            non_tame: false,
            follow_scale: 1.0,
            found: None,
            unseen_ticks: 0,
            unseen_memory: UNSEEN_MEMORY_TICKS,
        }
    }

    pub fn with_unseen_memory(self, ticks: i32) -> Self {
        Self {
            unseen_memory: ticks,
            ..self
        }
    }

    pub fn shulker_nearest() -> Self {
        Self {
            special: Special::ShulkerNearest,
            ..Self::new(TargetKind::Player, true)
        }
    }

    pub fn shulker_defense() -> Self {
        Self {
            special: Special::ShulkerDefense,
            ..Self::with(
                TargetKind::Class("LivingEntity"),
                TARGET_INTERVAL,
                true,
                TargetFilter::Enemy,
            )
        }
    }

    pub fn with_follow_scale(self, follow_scale: f64) -> Self {
        Self {
            follow_scale,
            ..self
        }
    }

    fn follow_distance(&self, mob: &MobView) -> f64 {
        mob.follow_range * self.follow_scale
    }

    pub fn non_tame(kind: TargetKind, must_see: bool, filter: TargetFilter) -> Self {
        Self {
            non_tame: true,
            ..Self::with(kind, TARGET_INTERVAL, must_see, filter)
        }
    }

    pub fn spider(kind: TargetKind) -> Self {
        Self {
            needs_darkness: true,
            ..Self::new(kind, true)
        }
    }

    fn find_target<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        let follow = self.follow_distance(context.mob);
        let conditions = TargetingConditions::for_combat().range(follow);
        let origin = context.mob.eye_position();
        let filter = self.filter;
        let mob = context.mob;
        let world = context.world;
        self.found = if self.kind == TargetKind::Player {
            targeting::nearest_player(
                context,
                conditions,
                |t| filter.accepts(mob, world, t),
                origin,
            )
        } else {
            let area = match self.special {
                Special::Plain => inflate(&mob.bounds(), follow, follow, follow),
                Special::ShulkerNearest | Special::ShulkerDefense => {
                    crate::shulker::target_search_area(mob, follow)
                }
            };
            let candidates = world.living_of(self.kind, &area);
            targeting::nearest(
                context,
                candidates,
                conditions,
                |t| filter.accepts(mob, world, t),
                origin,
            )
        };
    }

    pub(crate) fn can_use<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) -> bool {
        if self.non_tame && context.mob.tamed {
            return false;
        }
        match self.special {
            Special::Plain => {}
            Special::ShulkerNearest if context.world.peaceful() => return false,
            Special::ShulkerDefense if !context.mob.team => return false,
            Special::ShulkerNearest | Special::ShulkerDefense => {}
        }
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

    pub(crate) fn can_continue_to_use<W: AiWorld>(
        &mut self,
        context: &mut AiContext<'_, W>,
    ) -> bool {
        if self.non_tame {
            let mob = context.mob;
            let world = context.world;
            let Some(found) = self.found.and_then(|found| world.living(found.id)) else {
                return false;
            };
            let conditions = TargetingConditions::for_combat().range(self.follow_distance(mob));
            let filter = self.filter;
            return conditions.test(context, &found, |t| filter.accepts(mob, world, t));
        }
        let Some(target) = context.target() else {
            return false;
        };
        if !targeting::mob_can_attack(context.mob, context.world, &target) {
            return false;
        }
        let follow = self.follow_distance(context.mob);
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
                if self.unseen_ticks > reduced_tick_delay(self.unseen_memory) {
                    return false;
                }
            }
        }
        context.set_target(Some(target), TargetReason::ClosestEntity);
        true
    }

    pub(crate) fn start<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        let reason = if self.found.is_some_and(|found| found.player) {
            TargetReason::ClosestPlayer
        } else {
            TargetReason::ClosestEntity
        };
        context.set_target(self.found, reason);
        self.unseen_ticks = 0;
    }

    pub(crate) fn stop<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
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
    RangedAttack(RangedAttackGoal),
    BlazeAttack(crate::blaze::BlazeAttackGoal),
    MoveTowardsRestriction(MoveTowardsRestrictionGoal),
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
    SitWhenOrdered(SitWhenOrderedToGoal),
    FollowOwner(FollowOwnerGoal),
    OwnerTarget(OwnerTargetGoal),
    ResetUniversalAnger(ResetUniversalAngerGoal),
    Wolf(crate::WolfGoal),
    Cat(crate::CatGoal),
    Fox(crate::FoxGoal),
    Panda(crate::PandaGoal),
    PolarBear(crate::PolarBearGoal),
    Enderman(crate::EndermanGoal),
    Silverfish(crate::SilverfishGoal),
    IronGolem(crate::IronGolemGoal),
    Ghast(crate::ghast::GhastGoal),
    Shulker(crate::shulker::ShulkerGoal),
    Raider(crate::raider::RaiderGoal),
    Crossbow(crate::crossbow::CrossbowGoal),
    WitchTarget(crate::raider::NearestAttackableWitchTargetGoal),
    Trader(crate::villager::TraderGoal),
    Air(crate::air::AirGoal),
    Spell(crate::spell::SpellGoal),
    Equine(crate::equine::EquineGoal),
    Llama(crate::llama::LlamaGoal),
    StriderGoToLava(crate::strider::StriderGoToLavaGoal),
    Water(crate::swimmers::WaterGoal),
    Turtle(crate::turtle::TurtleGoal),
    Guardian(crate::guardian::GuardianGoal),
    Drowned(crate::drowned::DrownedGoal),
    Cube(crate::cube::CubeGoal),
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
            | Self::MoveThroughVillage(_)
            | Self::MoveTowardsRestriction(_) => movement,
            Self::LookAtPlayer(goal) if goal.is_interact() => movement.with(Flag::Look),
            Self::LookAtPlayer(_) => Flags::of(Flag::Look),
            Self::RandomLookAround(_)
            | Self::Tempt(_)
            | Self::Breed(_)
            | Self::MeleeAttack(_)
            | Self::OcelotAttack(_)
            | Self::RangedBowAttack(_)
            | Self::RangedAttack(_)
            | Self::BlazeAttack(_) => movement.with(Flag::Look),
            Self::EatBlock(_) => movement.with(Flag::Look).with(Flag::Jump),
            Self::SitWhenOrdered(_) => movement.with(Flag::Jump),
            Self::FollowOwner(_) => movement.with(Flag::Look),
            Self::OwnerTarget(_) => Flags::of(Flag::Target),
            Self::LeapAtTarget(_) | Self::RemoveTurtleEgg(_) | Self::RaidGarden(_) => {
                movement.with(Flag::Jump)
            }
            Self::NearestAttackableTarget(_) | Self::HurtByTarget(_) => Flags::of(Flag::Target),
            Self::FollowParent(_)
            | Self::RestrictSun(_)
            | Self::BreakDoor(_)
            | Self::ResetUniversalAnger(_) => Flags::NONE,
            Self::Wolf(goal) => Goal::<AiContext<'_, W>>::flags(goal),
            Self::Cat(goal) => Goal::<AiContext<'_, W>>::flags(goal),
            Self::Fox(goal) => Goal::<AiContext<'_, W>>::flags(goal),
            Self::Panda(goal) => Goal::<AiContext<'_, W>>::flags(goal),
            Self::PolarBear(goal) => Goal::<AiContext<'_, W>>::flags(goal),
            Self::Enderman(goal) => Goal::<AiContext<'_, W>>::flags(goal),
            Self::Silverfish(goal) => Goal::<AiContext<'_, W>>::flags(goal),
            Self::IronGolem(goal) => Goal::<AiContext<'_, W>>::flags(goal),
            Self::Ghast(goal) => Goal::<AiContext<'_, W>>::flags(goal),
            Self::Shulker(goal) => Goal::<AiContext<'_, W>>::flags(goal),
            Self::Raider(goal) => Goal::<AiContext<'_, W>>::flags(goal),
            Self::Crossbow(goal) => Goal::<AiContext<'_, W>>::flags(goal),
            Self::WitchTarget(goal) => Goal::<AiContext<'_, W>>::flags(goal),
            Self::Trader(goal) => Goal::<AiContext<'_, W>>::flags(goal),
            Self::Air(goal) => Goal::<AiContext<'_, W>>::flags(goal),
            Self::Spell(goal) => Goal::<AiContext<'_, W>>::flags(goal),
            Self::Equine(goal) => Goal::<AiContext<'_, W>>::flags(goal),
            Self::Llama(goal) => Goal::<AiContext<'_, W>>::flags(goal),
            Self::StriderGoToLava(goal) => Goal::<AiContext<'_, W>>::flags(goal),
            Self::Water(goal) => Goal::<AiContext<'_, W>>::flags(goal),
            Self::Turtle(goal) => Goal::<AiContext<'_, W>>::flags(goal),
            Self::Guardian(goal) => Goal::<AiContext<'_, W>>::flags(goal),
            Self::Drowned(goal) => Goal::<AiContext<'_, W>>::flags(goal),
            Self::Cube(goal) => Goal::<AiContext<'_, W>>::flags(goal),
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
            Self::RangedAttack(goal) => goal.can_use(context),
            Self::BlazeAttack(goal) => Goal::can_use(goal, context),
            Self::MoveTowardsRestriction(goal) => goal.can_use(context),
            Self::Swell(goal) => goal.can_use(context),
            Self::LeapAtTarget(goal) => goal.can_use(context),
            Self::HurtByTarget(goal) => goal.can_use(context),
            Self::AvoidEntity(goal) => goal.can_use(context),
            Self::FleeSun(goal) => goal.can_use(context),
            Self::RestrictSun(goal) => goal.can_use(context),
            Self::RemoveTurtleEgg(goal) => goal.can_use(context),
            Self::BreakDoor(goal) => goal.can_use(context),
            Self::MoveThroughVillage(goal) => goal.can_use(context),
            Self::SitWhenOrdered(goal) => goal.can_use(context),
            Self::FollowOwner(goal) => goal.can_use(context),
            Self::OwnerTarget(goal) => goal.can_use(context),
            Self::ResetUniversalAnger(goal) => goal.can_use(context),
            Self::Wolf(goal) => Goal::can_use(goal, context),
            Self::Cat(goal) => Goal::can_use(goal, context),
            Self::Fox(goal) => Goal::can_use(goal, context),
            Self::Panda(goal) => Goal::can_use(goal, context),
            Self::PolarBear(goal) => Goal::can_use(goal, context),
            Self::Enderman(goal) => Goal::can_use(goal, context),
            Self::Silverfish(goal) => Goal::can_use(goal, context),
            Self::IronGolem(goal) => Goal::can_use(goal, context),
            Self::Ghast(goal) => Goal::can_use(goal, context),
            Self::Shulker(goal) => Goal::can_use(goal, context),
            Self::Raider(goal) => Goal::can_use(goal, context),
            Self::Crossbow(goal) => Goal::can_use(goal, context),
            Self::WitchTarget(goal) => Goal::can_use(goal, context),
            Self::Trader(goal) => Goal::can_use(goal, context),
            Self::Air(goal) => Goal::can_use(goal, context),
            Self::Spell(goal) => Goal::can_use(goal, context),
            Self::Equine(goal) => Goal::can_use(goal, context),
            Self::Llama(goal) => Goal::can_use(goal, context),
            Self::StriderGoToLava(goal) => Goal::can_use(goal, context),
            Self::Water(goal) => Goal::can_use(goal, context),
            Self::Turtle(goal) => Goal::can_use(goal, context),
            Self::Guardian(goal) => Goal::can_use(goal, context),
            Self::Drowned(goal) => Goal::can_use(goal, context),
            Self::Cube(goal) => Goal::can_use(goal, context),
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
            Self::Panic(_) | Self::FleeSun(_) => !context.brain.navigation.is_done(),
            Self::MoveThroughVillage(goal) => goal.can_continue_to_use(context),
            Self::Breed(goal) => goal.can_continue_to_use(context),
            Self::FollowParent(goal) => goal.can_continue_to_use(context),
            Self::EatBlock(goal) => goal.eat_animation_tick() > 0,
            Self::MeleeAttack(goal) => goal.can_continue_to_use(context),
            Self::RangedBowAttack(goal) => goal.can_continue_to_use(context),
            Self::RangedAttack(goal) => goal.can_continue_to_use(context),
            Self::BlazeAttack(goal) => Goal::can_continue_to_use(goal, context),
            Self::MoveTowardsRestriction(goal) => goal.can_continue_to_use(context),
            Self::Swell(goal) => goal.can_use(context),
            Self::LeapAtTarget(goal) => goal.can_continue_to_use(context),
            Self::HurtByTarget(goal) => goal.memory().can_continue_to_use(context),
            Self::RestrictSun(goal) => goal.can_use(context),
            Self::RemoveTurtleEgg(goal) => goal.can_continue_to_use(context),
            Self::BreakDoor(goal) => !goal.passed() && goal.can_continue_to_use(context),
            Self::SitWhenOrdered(goal) => goal.can_continue_to_use(context),
            Self::FollowOwner(goal) => goal.can_continue_to_use(context),
            Self::OwnerTarget(goal) => goal.memory().can_continue_to_use(context),
            Self::ResetUniversalAnger(goal) => goal.can_use(context),
            Self::Wolf(goal) => Goal::can_continue_to_use(goal, context),
            Self::Cat(goal) => Goal::can_continue_to_use(goal, context),
            Self::Fox(goal) => Goal::can_continue_to_use(goal, context),
            Self::Panda(goal) => Goal::can_continue_to_use(goal, context),
            Self::PolarBear(goal) => Goal::can_continue_to_use(goal, context),
            Self::Enderman(goal) => Goal::can_continue_to_use(goal, context),
            Self::Silverfish(goal) => Goal::can_continue_to_use(goal, context),
            Self::IronGolem(goal) => Goal::can_continue_to_use(goal, context),
            Self::Ghast(goal) => Goal::can_continue_to_use(goal, context),
            Self::Shulker(goal) => Goal::can_continue_to_use(goal, context),
            Self::Raider(goal) => Goal::can_continue_to_use(goal, context),
            Self::Crossbow(goal) => Goal::can_continue_to_use(goal, context),
            Self::WitchTarget(goal) => Goal::can_continue_to_use(goal, context),
            Self::Trader(goal) => Goal::can_continue_to_use(goal, context),
            Self::Air(goal) => Goal::can_continue_to_use(goal, context),
            Self::Spell(goal) => Goal::can_continue_to_use(goal, context),
            Self::Equine(goal) => Goal::can_continue_to_use(goal, context),
            Self::Llama(goal) => Goal::can_continue_to_use(goal, context),
            Self::StriderGoToLava(goal) => Goal::can_continue_to_use(goal, context),
            Self::Water(goal) => Goal::can_continue_to_use(goal, context),
            Self::Turtle(goal) => Goal::can_continue_to_use(goal, context),
            Self::Guardian(goal) => Goal::can_continue_to_use(goal, context),
            Self::Drowned(goal) => Goal::can_continue_to_use(goal, context),
            Self::Cube(goal) => Goal::can_continue_to_use(goal, context),
        }
    }

    fn is_interruptable(&self) -> bool {
        match self {
            Self::Wolf(goal) => Goal::<AiContext<'_, W>>::is_interruptable(goal),
            Self::Cat(goal) => Goal::<AiContext<'_, W>>::is_interruptable(goal),
            Self::Fox(goal) => Goal::<AiContext<'_, W>>::is_interruptable(goal),
            Self::Panda(goal) => Goal::<AiContext<'_, W>>::is_interruptable(goal),
            Self::PolarBear(goal) => Goal::<AiContext<'_, W>>::is_interruptable(goal),
            Self::Enderman(goal) => Goal::<AiContext<'_, W>>::is_interruptable(goal),
            Self::Silverfish(goal) => Goal::<AiContext<'_, W>>::is_interruptable(goal),
            Self::IronGolem(goal) => Goal::<AiContext<'_, W>>::is_interruptable(goal),
            Self::Ghast(goal) => Goal::<AiContext<'_, W>>::is_interruptable(goal),
            Self::Air(goal) => Goal::<AiContext<'_, W>>::is_interruptable(goal),
            Self::Spell(goal) => Goal::<AiContext<'_, W>>::is_interruptable(goal),
            Self::Equine(goal) => Goal::<AiContext<'_, W>>::is_interruptable(goal),
            Self::StriderGoToLava(goal) => Goal::<AiContext<'_, W>>::is_interruptable(goal),
            Self::Water(goal) => Goal::<AiContext<'_, W>>::is_interruptable(goal),
            Self::Turtle(goal) => Goal::<AiContext<'_, W>>::is_interruptable(goal),
            Self::Guardian(goal) => Goal::<AiContext<'_, W>>::is_interruptable(goal),
            Self::Drowned(goal) => Goal::<AiContext<'_, W>>::is_interruptable(goal),
            Self::Cube(goal) => Goal::<AiContext<'_, W>>::is_interruptable(goal),
            Self::Shulker(goal) => Goal::<AiContext<'_, W>>::is_interruptable(goal),
            Self::Raider(goal) => Goal::<AiContext<'_, W>>::is_interruptable(goal),
            Self::Crossbow(goal) => Goal::<AiContext<'_, W>>::is_interruptable(goal),
            Self::WitchTarget(goal) => Goal::<AiContext<'_, W>>::is_interruptable(goal),
            _ => true,
        }
    }

    fn start(&mut self, context: &mut AiContext<'_, W>) {
        match self {
            Self::Float(_)
            | Self::ClimbOnTopOfPowderSnow(_)
            | Self::OcelotAttack(_)
            | Self::RangedAttack(_)
            | Self::Breed(_) => {}
            Self::BlazeAttack(goal) => Goal::start(goal, context),
            Self::MoveTowardsRestriction(goal) => goal.start(context),
            Self::MoveThroughVillage(goal) => goal.start(context),
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
            Self::SitWhenOrdered(goal) => goal.start(context),
            Self::FollowOwner(goal) => goal.start(context),
            Self::OwnerTarget(goal) => goal.start(context),
            Self::ResetUniversalAnger(goal) => goal.start(context),
            Self::Wolf(goal) => Goal::start(goal, context),
            Self::Cat(goal) => Goal::start(goal, context),
            Self::Fox(goal) => Goal::start(goal, context),
            Self::Panda(goal) => Goal::start(goal, context),
            Self::PolarBear(goal) => Goal::start(goal, context),
            Self::Enderman(goal) => Goal::start(goal, context),
            Self::Silverfish(goal) => Goal::start(goal, context),
            Self::IronGolem(goal) => Goal::start(goal, context),
            Self::Ghast(goal) => Goal::start(goal, context),
            Self::Shulker(goal) => Goal::start(goal, context),
            Self::Raider(goal) => Goal::start(goal, context),
            Self::Crossbow(goal) => Goal::start(goal, context),
            Self::WitchTarget(goal) => Goal::start(goal, context),
            Self::Trader(goal) => Goal::start(goal, context),
            Self::Air(goal) => Goal::start(goal, context),
            Self::Spell(goal) => Goal::start(goal, context),
            Self::Equine(goal) => Goal::start(goal, context),
            Self::Llama(goal) => Goal::start(goal, context),
            Self::StriderGoToLava(goal) => Goal::start(goal, context),
            Self::Water(goal) => Goal::start(goal, context),
            Self::Turtle(goal) => Goal::start(goal, context),
            Self::Guardian(goal) => Goal::start(goal, context),
            Self::Drowned(goal) => Goal::start(goal, context),
            Self::Cube(goal) => Goal::start(goal, context),
        }
    }

    fn stop(&mut self, context: &mut AiContext<'_, W>) {
        match self {
            Self::Float(_)
            | Self::ClimbOnTopOfPowderSnow(_)
            | Self::RaidGarden(_)
            | Self::RandomLookAround(_)
            | Self::LeapAtTarget(_)
            | Self::MoveTowardsRestriction(_)
            | Self::FleeSun(_) => {}
            Self::BlazeAttack(goal) => Goal::stop(goal, context),
            Self::MoveThroughVillage(goal) => goal.stop(context),
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
            Self::RangedAttack(goal) => goal.stop(context),
            Self::Swell(goal) => goal.stop(),
            Self::HurtByTarget(goal) => goal.memory().stop(context),
            Self::AvoidEntity(goal) => goal.stop(),
            Self::RestrictSun(goal) => goal.stop(context),
            Self::RemoveTurtleEgg(goal) => goal.stop(context),
            Self::BreakDoor(goal) => goal.stop(context),
            Self::SitWhenOrdered(goal) => goal.stop(context),
            Self::FollowOwner(goal) => goal.stop(context),
            Self::OwnerTarget(goal) => goal.memory().stop(context),
            Self::ResetUniversalAnger(_) => {}
            Self::Wolf(goal) => Goal::stop(goal, context),
            Self::Cat(goal) => Goal::stop(goal, context),
            Self::Fox(goal) => Goal::stop(goal, context),
            Self::Panda(goal) => Goal::stop(goal, context),
            Self::PolarBear(goal) => Goal::stop(goal, context),
            Self::Enderman(goal) => Goal::stop(goal, context),
            Self::Silverfish(goal) => Goal::stop(goal, context),
            Self::IronGolem(goal) => Goal::stop(goal, context),
            Self::Ghast(goal) => Goal::stop(goal, context),
            Self::Shulker(goal) => Goal::stop(goal, context),
            Self::Raider(goal) => Goal::stop(goal, context),
            Self::Crossbow(goal) => Goal::stop(goal, context),
            Self::WitchTarget(goal) => Goal::stop(goal, context),
            Self::Trader(goal) => Goal::stop(goal, context),
            Self::Air(goal) => Goal::stop(goal, context),
            Self::Spell(goal) => Goal::stop(goal, context),
            Self::Equine(goal) => Goal::stop(goal, context),
            Self::Llama(goal) => Goal::stop(goal, context),
            Self::StriderGoToLava(goal) => Goal::stop(goal, context),
            Self::Water(goal) => Goal::stop(goal, context),
            Self::Turtle(goal) => Goal::stop(goal, context),
            Self::Guardian(goal) => Goal::stop(goal, context),
            Self::Drowned(goal) => Goal::stop(goal, context),
            Self::Cube(goal) => Goal::stop(goal, context),
        }
    }

    fn requires_update_every_tick(&self) -> bool {
        match self {
            Self::Wolf(goal) => Goal::<AiContext<'_, W>>::requires_update_every_tick(goal),
            Self::Cat(goal) => Goal::<AiContext<'_, W>>::requires_update_every_tick(goal),
            Self::Fox(goal) => Goal::<AiContext<'_, W>>::requires_update_every_tick(goal),
            Self::Panda(goal) => Goal::<AiContext<'_, W>>::requires_update_every_tick(goal),
            Self::PolarBear(goal) => Goal::<AiContext<'_, W>>::requires_update_every_tick(goal),
            Self::Enderman(goal) => Goal::<AiContext<'_, W>>::requires_update_every_tick(goal),
            Self::Silverfish(goal) => Goal::<AiContext<'_, W>>::requires_update_every_tick(goal),
            Self::IronGolem(goal) => Goal::<AiContext<'_, W>>::requires_update_every_tick(goal),
            Self::Ghast(goal) => Goal::<AiContext<'_, W>>::requires_update_every_tick(goal),
            Self::Air(goal) => Goal::<AiContext<'_, W>>::requires_update_every_tick(goal),
            Self::Spell(goal) => Goal::<AiContext<'_, W>>::requires_update_every_tick(goal),
            Self::Equine(goal) => Goal::<AiContext<'_, W>>::requires_update_every_tick(goal),
            Self::StriderGoToLava(goal) => {
                Goal::<AiContext<'_, W>>::requires_update_every_tick(goal)
            }
            Self::Water(goal) => Goal::<AiContext<'_, W>>::requires_update_every_tick(goal),
            Self::Turtle(goal) => Goal::<AiContext<'_, W>>::requires_update_every_tick(goal),
            Self::Guardian(goal) => Goal::<AiContext<'_, W>>::requires_update_every_tick(goal),
            Self::Drowned(goal) => Goal::<AiContext<'_, W>>::requires_update_every_tick(goal),
            Self::Cube(goal) => Goal::<AiContext<'_, W>>::requires_update_every_tick(goal),
            Self::Shulker(goal) => Goal::<AiContext<'_, W>>::requires_update_every_tick(goal),
            Self::Raider(goal) => Goal::<AiContext<'_, W>>::requires_update_every_tick(goal),
            Self::Crossbow(goal) => Goal::<AiContext<'_, W>>::requires_update_every_tick(goal),
            Self::WitchTarget(goal) => Goal::<AiContext<'_, W>>::requires_update_every_tick(goal),
            Self::BlazeAttack(goal) => Goal::<AiContext<'_, W>>::requires_update_every_tick(goal),
            _ => self.requires_update_every_tick_base(),
        }
    }

    fn tick(&mut self, context: &mut AiContext<'_, W>) {
        match self {
            Self::Wolf(goal) => Goal::tick(goal, context),
            Self::Cat(goal) => Goal::tick(goal, context),
            Self::Fox(goal) => Goal::tick(goal, context),
            Self::Panda(goal) => Goal::tick(goal, context),
            Self::PolarBear(goal) => Goal::tick(goal, context),
            Self::Enderman(goal) => Goal::tick(goal, context),
            Self::Silverfish(goal) => Goal::tick(goal, context),
            Self::IronGolem(goal) => Goal::tick(goal, context),
            Self::Ghast(goal) => Goal::tick(goal, context),
            Self::Shulker(goal) => Goal::tick(goal, context),
            Self::Raider(goal) => Goal::tick(goal, context),
            Self::Crossbow(goal) => Goal::tick(goal, context),
            Self::WitchTarget(goal) => Goal::tick(goal, context),
            Self::BlazeAttack(goal) => Goal::tick(goal, context),
            Self::Trader(goal) => Goal::tick(goal, context),
            Self::Air(goal) => Goal::tick(goal, context),
            Self::Spell(goal) => Goal::tick(goal, context),
            Self::Equine(goal) => Goal::tick(goal, context),
            Self::Llama(goal) => Goal::tick(goal, context),
            Self::StriderGoToLava(goal) => Goal::tick(goal, context),
            Self::Water(goal) => Goal::tick(goal, context),
            Self::Turtle(goal) => Goal::tick(goal, context),
            Self::Guardian(goal) => Goal::tick(goal, context),
            Self::Drowned(goal) => Goal::tick(goal, context),
            Self::Cube(goal) => Goal::tick(goal, context),
            _ => self.tick_base(context),
        }
    }
}

impl MobGoal {
    fn requires_update_every_tick_base(&self) -> bool {
        matches!(
            self,
            Self::Float(_)
                | Self::ClimbOnTopOfPowderSnow(_)
                | Self::OcelotAttack(_)
                | Self::RaidGarden(_)
                | Self::RandomLookAround(_)
                | Self::MeleeAttack(_)
                | Self::RangedBowAttack(_)
                | Self::RangedAttack(_)
                | Self::Swell(_)
                | Self::RemoveTurtleEgg(_)
                | Self::BreakDoor(_)
        )
    }

    fn tick_base<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
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
            Self::RangedAttack(goal) => goal.tick(context),
            Self::Swell(goal) => goal.tick(context),
            Self::AvoidEntity(goal) => goal.tick(context),
            Self::RemoveTurtleEgg(goal) => goal.tick(context),
            Self::BreakDoor(goal) => goal.tick(context),
            Self::FollowOwner(goal) => goal.tick(context),
            Self::Stroll(_)
            | Self::NearestAttackableTarget(_)
            | Self::LeapAtTarget(_)
            | Self::HurtByTarget(_)
            | Self::FleeSun(_)
            | Self::RestrictSun(_)
            | Self::MoveThroughVillage(_)
            | Self::SitWhenOrdered(_)
            | Self::OwnerTarget(_)
            | Self::ResetUniversalAnger(_)
            | Self::BlazeAttack(_)
            | Self::MoveTowardsRestriction(_)
            | Self::Ghast(_)
            | Self::Shulker(_)
            | Self::Raider(_)
            | Self::Crossbow(_)
            | Self::WitchTarget(_)
            | Self::Trader(_)
            | Self::Air(_)
            | Self::Spell(_)
            | Self::Equine(_)
            | Self::Llama(_)
            | Self::StriderGoToLava(_)
            | Self::Water(_)
            | Self::Turtle(_)
            | Self::Guardian(_)
            | Self::Drowned(_)
            | Self::Cube(_)
            | Self::Wolf(_)
            | Self::Cat(_)
            | Self::Fox(_)
            | Self::Panda(_)
            | Self::PolarBear(_)
            | Self::Enderman(_)
            | Self::Silverfish(_)
            | Self::IronGolem(_) => {}
        }
    }
}

#[cfg(test)]
mod look_tests {
    use rspigot_domain::Vec3;
    use rspigot_entities::EntityKind;

    use super::*;
    use crate::mock::{Harness, Mock, living};

    const SEED: i64 = 17;

    #[test]
    fn the_mob_look_goal_picks_the_nearest_mob_within_reach_and_a_player_goal_ignores_mobs() {
        let world = Mock {
            others: vec![
                living(2, EntityKind::Zombie, Vec3::new(9.5, 64.0, 0.5)),
                living(3, EntityKind::Cow, Vec3::new(4.5, 64.0, 0.5)),
                living(4, EntityKind::Zombie, Vec3::new(25.5, 64.0, 0.5)),
            ],
            ..Mock::default()
        };
        let mut harness =
            Harness::new(EntityKind::Pillager, Vec3::new(0.5, 64.0, 0.5), world, SEED);
        let mut mobs = LookAtPlayerGoal::at_mobs(15.0);
        mobs.probability = 1.0;
        assert!(harness.run(|context| mobs.can_use(context)));
        assert_eq!(mobs.look_at(), Some(EntityId(3)));
        let mut players = LookAtPlayerGoal::with_probability(15.0, 1.0);
        assert!(!harness.run(|context| players.can_use(context)));
        let mut empty = Harness::new(
            EntityKind::Pillager,
            Vec3::new(0.5, 64.0, 0.5),
            Mock::default(),
            SEED,
        );
        let mut lonely = LookAtPlayerGoal::at_mobs(15.0);
        lonely.probability = 1.0;
        assert!(!empty.run(|context| lonely.can_use(context)));
    }
}
