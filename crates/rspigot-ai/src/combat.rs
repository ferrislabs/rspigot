use rspigot_domain::{EntityId, Vec3, mth};
use rspigot_entities::EntityKind;
use rspigot_items::ItemKind;

use crate::Path;
use crate::PolarBearAction;
use crate::action::{MobAction, TargetReason};
use crate::brain::AiContext;
use crate::goal::{adjusted_tick_delay, reduced_tick_delay};
use crate::random::RandomSource;
use crate::targeting::{self, TargetingConditions};
use crate::world::{AiWorld, Hand, Living, MobBody, MobView, TargetKind};

pub(crate) const FOX_BITE: &str = "minecraft:entity.fox.bite";
const CAN_USE_COOLDOWN: i64 = 20;
const ATTACK_INTERVAL: i32 = 20;
const MELEE_LOOK_SPEED: f32 = 30.0;
const REPATH_CHANCE: f32 = 0.05;
const FAR_DISTANCE_SQR: f64 = 1024.0;
const MID_DISTANCE_SQR: f64 = 256.0;
const RAISE_ARM_TICKS: i32 = 5;
const SPIDER_FORGET_CHANCE: i32 = 100;
const SPIDER_LIGHT_LIMIT: f32 = 0.5;
const BOW_USE_TICKS: i32 = 20;
const SEE_TIME_LIMIT: i32 = -60;
const STRAFE_FLIP_CHANCE: f64 = 0.3;
const STRAFE_TICKS: i32 = 20;
const SWELL_START_SQR: f64 = 9.0;
const SWELL_STOP_SQR: f64 = 49.0;
const LEAP_MIN_SQR: f64 = 4.0;
const LEAP_MAX_SQR: f64 = 16.0;
const LEAP_CHANCE: i32 = 5;
const LEAP_HORIZONTAL: f64 = 0.4;
const LEAP_CARRY: f64 = 0.2;
const ALERT_RANGE_Y: f64 = 10.0;
const HURT_BY_MEMORY: i32 = 300;
const UNSEEN_MEMORY: i32 = 60;
const POLAR_BEAR_SPEED: f64 = 1.25;
const POLAR_BEAR_WARNING_MARGIN: f32 = 3.0;
const POLAR_BEAR_STAND_TICKS: i32 = 10;

pub fn default_attack_reach() -> f64 {
    f64::from(2.04f32).sqrt() - f64::from(0.6f32)
}

pub fn within_melee_range(mob: &MobView, target: &Living) -> bool {
    let reach = default_attack_reach();
    let bounds = crate::world::inflate(&mob.bounds(), reach, 0.0, reach);
    bounds.intersects(&target.bounds)
}

pub fn eye_of(target: &Living) -> Vec3 {
    Vec3::new(target.position.x, target.eye_y(), target.position.z)
}

fn rotlerp(from: f32, to: f32, max: f32) -> f32 {
    from + mth::wrap_degrees(to - from).clamp(-max, max)
}

pub fn look_at_entity(mob: &MobView, body: &mut MobBody, target: &Living, y_max: f32, x_max: f32) {
    let dx = target.position.x - mob.position.x;
    let dz = target.position.z - mob.position.z;
    let dy = target.eye_y() - mob.eye_y();
    let horizontal = (dx * dx + dz * dz).sqrt();
    let yaw = (mth::atan2(dz, dx) * mth::RAD_TO_DEG) as f32 - 90.0;
    let pitch = (-(mth::atan2(dy, horizontal) * mth::RAD_TO_DEG)) as f32;
    body.pitch = rotlerp(body.pitch, pitch, x_max);
    body.yaw = rotlerp(body.yaw, yaw, y_max);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeleeKind {
    Plain,
    Zombie { raise_arm_ticks: i32 },
    Skeleton,
    Spider,
    Fox,
    PolarBear,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MeleeAttackGoal {
    kind: MeleeKind,
    speed_modifier: f64,
    following_target_even_if_not_seen: bool,
    path: Option<Path>,
    pathed_target: Vec3,
    ticks_until_next_path_recalculation: i32,
    ticks_until_next_attack: i32,
    last_can_use_check: i64,
}

impl MeleeAttackGoal {
    pub fn new(speed_modifier: f64, following_target_even_if_not_seen: bool) -> Self {
        Self {
            kind: MeleeKind::Plain,
            speed_modifier,
            following_target_even_if_not_seen,
            path: None,
            pathed_target: Vec3::ZERO,
            ticks_until_next_path_recalculation: 0,
            ticks_until_next_attack: 0,
            last_can_use_check: 0,
        }
    }

    pub fn zombie(speed_modifier: f64) -> Self {
        Self {
            kind: MeleeKind::Zombie { raise_arm_ticks: 0 },
            ..Self::new(speed_modifier, false)
        }
    }

    pub fn skeleton(speed_modifier: f64) -> Self {
        Self {
            kind: MeleeKind::Skeleton,
            ..Self::new(speed_modifier, false)
        }
    }

    pub fn fox(speed_modifier: f64) -> Self {
        Self {
            kind: MeleeKind::Fox,
            ..Self::new(speed_modifier, true)
        }
    }

    pub fn spider() -> Self {
        Self {
            kind: MeleeKind::Spider,
            ..Self::new(1.0, true)
        }
    }

    pub fn polar_bear() -> Self {
        Self {
            kind: MeleeKind::PolarBear,
            ..Self::new(POLAR_BEAR_SPEED, true)
        }
    }

    pub fn kind(&self) -> MeleeKind {
        self.kind
    }

    pub(crate) fn can_use<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) -> bool {
        let game_time = context.world.game_time();
        if game_time - self.last_can_use_check < CAN_USE_COOLDOWN {
            return false;
        }
        self.last_can_use_check = game_time;
        let Some(target) = context.target() else {
            return false;
        };
        if !target.alive {
            return false;
        }
        self.path = context.create_path_to(target.position.block(), 0);
        if self.path.is_some() {
            return true;
        }
        within_melee_range(context.mob, &target)
    }

    pub(crate) fn can_continue_to_use<W: AiWorld>(
        &mut self,
        context: &mut AiContext<'_, W>,
    ) -> bool {
        if self.kind == MeleeKind::Spider
            && context.mob.light >= SPIDER_LIGHT_LIMIT
            && context.brain.random.next_int(SPIDER_FORGET_CHANCE) == 0
        {
            context.set_target(None, TargetReason::Unknown);
            return false;
        }
        let Some(target) = context.target() else {
            return false;
        };
        if !target.alive {
            return false;
        }
        if !self.following_target_even_if_not_seen {
            return !context.brain.navigation.is_done();
        }
        if !context.mob.is_within_home(target.position.block()) {
            return false;
        }
        !(target.player && (target.spectator || target.creative))
    }

    pub(crate) fn start<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        let path = self.path.take();
        context.move_along(path, self.speed_modifier);
        context.body.aggressive = true;
        self.ticks_until_next_path_recalculation = 0;
        self.ticks_until_next_attack = 0;
        if let MeleeKind::Zombie { raise_arm_ticks } = &mut self.kind {
            *raise_arm_ticks = 0;
        }
    }

    pub(crate) fn stop<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        if self.kind == MeleeKind::PolarBear {
            context.act(MobAction::PolarBear(PolarBearAction::SetStanding(false)));
        }
        if context
            .target()
            .is_some_and(|target| target.player && (target.creative || target.spectator))
        {
            context.set_target(None, TargetReason::Unknown);
        }
        context.body.aggressive = false;
        context.stop_navigation();
    }

    pub(crate) fn tick<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        self.melee_tick(context);
        if let MeleeKind::Zombie { raise_arm_ticks } = &mut self.kind {
            *raise_arm_ticks += 1;
            let half = adjusted_tick_delay(true, ATTACK_INTERVAL) / 2;
            context.body.aggressive =
                *raise_arm_ticks >= RAISE_ARM_TICKS && self.ticks_until_next_attack < half;
        }
    }

    fn melee_tick<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        let Some(target) = context.target() else {
            return;
        };
        context
            .brain
            .look
            .set_look_at(eye_of(&target), MELEE_LOOK_SPEED, MELEE_LOOK_SPEED);
        self.ticks_until_next_path_recalculation =
            (self.ticks_until_next_path_recalculation - 1).max(0);
        let world = context.world;
        let sees = self.following_target_even_if_not_seen
            || context
                .brain
                .sensing
                .has_line_of_sight(world, context.mob, &target);
        if sees
            && self.ticks_until_next_path_recalculation <= 0
            && (self.pathed_target == Vec3::ZERO
                || target.position.distance_squared(self.pathed_target) >= 1.0
                || context.brain.random.next_float() < REPATH_CHANCE)
        {
            self.pathed_target = target.position;
            self.ticks_until_next_path_recalculation = 4 + context.brain.random.next_int(7);
            let distance = context.mob.distance_to_sqr(target.position);
            if distance > FAR_DISTANCE_SQR {
                self.ticks_until_next_path_recalculation += 10;
            } else if distance > MID_DISTANCE_SQR {
                self.ticks_until_next_path_recalculation += 5;
            }
            if !context.move_to_entity_within(&target, 0, self.speed_modifier) {
                self.ticks_until_next_path_recalculation += 15;
            }
            self.ticks_until_next_path_recalculation =
                adjusted_tick_delay(true, self.ticks_until_next_path_recalculation);
        }
        self.ticks_until_next_attack = (self.ticks_until_next_attack - 1).max(0);
        self.check_and_perform_attack(context, &target);
    }

    fn reset_attack_cooldown(&mut self) {
        self.ticks_until_next_attack = adjusted_tick_delay(true, ATTACK_INTERVAL);
    }

    fn can_perform_attack<W: AiWorld>(
        &self,
        context: &mut AiContext<'_, W>,
        target: &Living,
    ) -> bool {
        let world = context.world;
        self.ticks_until_next_attack <= 0
            && within_melee_range(context.mob, target)
            && context
                .brain
                .sensing
                .has_line_of_sight(world, context.mob, target)
    }

    fn check_and_perform_attack<W: AiWorld>(
        &mut self,
        context: &mut AiContext<'_, W>,
        target: &Living,
    ) {
        if self.kind == MeleeKind::PolarBear {
            self.polar_bear_attack(context, target);
        } else if self.can_perform_attack(context, target) {
            self.reset_attack_cooldown();
            if self.kind == MeleeKind::Fox {
                context.act(MobAction::Attack(target.id));
                context.act(MobAction::Fox(crate::FoxAction::Sound(FOX_BITE)));
                return;
            }
            context.act(MobAction::Swing(Hand::Main));
            context.act(MobAction::Attack(target.id));
        }
    }

    fn polar_bear_attack<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>, target: &Living) {
        let standing = |context: &mut AiContext<'_, W>, value: bool| {
            context.act(MobAction::PolarBear(PolarBearAction::SetStanding(value)));
        };
        if self.can_perform_attack(context, target) {
            self.reset_attack_cooldown();
            context.act(MobAction::Attack(target.id));
            standing(context, false);
            return;
        }
        let width = (target.bounds.max_x - target.bounds.min_x) as f32;
        let reach = (width + POLAR_BEAR_WARNING_MARGIN) * (width + POLAR_BEAR_WARNING_MARGIN);
        if context.mob.distance_to_sqr(target.position) < f64::from(reach) {
            if self.ticks_until_next_attack <= 0 {
                standing(context, false);
                self.reset_attack_cooldown();
            }
            if self.ticks_until_next_attack <= POLAR_BEAR_STAND_TICKS {
                standing(context, true);
                context.act(MobAction::PolarBear(PolarBearAction::Warn));
            }
        } else {
            self.reset_attack_cooldown();
            standing(context, false);
        }
    }
}

const OCELOT_ATTACK_COOLDOWN: i32 = 20;
const OCELOT_GIVE_UP_SQR: f64 = 225.0;
const OCELOT_POUNCE_SQR: f64 = 16.0;
const OCELOT_STALK_SPEED: f64 = 0.8;
const OCELOT_POUNCE_SPEED: f64 = 1.33;
const OCELOT_SNEAK_SPEED: f64 = 0.6;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct OcelotAttackGoal {
    target: Option<EntityId>,
    attack_time: i32,
}

impl OcelotAttackGoal {
    pub(crate) fn can_use<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) -> bool {
        let Some(target) = context.target() else {
            return false;
        };
        self.target = Some(target.id);
        true
    }

    pub(crate) fn can_continue_to_use<W: AiWorld>(
        &mut self,
        context: &mut AiContext<'_, W>,
    ) -> bool {
        let Some(target) = self.target.and_then(|id| context.world.living(id)) else {
            return false;
        };
        if !target.alive {
            return false;
        }
        if context.mob.distance_to_sqr(target.position) > OCELOT_GIVE_UP_SQR {
            return false;
        }
        !(context.brain.navigation.is_done() && !self.can_use(context))
    }

    pub(crate) fn stop<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        self.target = None;
        context.stop_navigation();
    }

    pub(crate) fn tick<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        let Some(target) = self.target.and_then(|id| context.world.living(id)) else {
            return;
        };
        context
            .brain
            .look
            .set_look_at(eye_of(&target), MELEE_LOOK_SPEED, MELEE_LOOK_SPEED);
        let width = context.mob.width;
        let reach = f64::from(width * 2.0 * width * 2.0);
        let distance = context.mob.distance_to_sqr(target.position);
        let speed = if reach < distance && distance < OCELOT_POUNCE_SQR {
            OCELOT_POUNCE_SPEED
        } else if distance < OCELOT_GIVE_UP_SQR {
            OCELOT_SNEAK_SPEED
        } else {
            OCELOT_STALK_SPEED
        };
        context.move_to_entity(&target, speed);
        self.attack_time = (self.attack_time - 1).max(0);
        if reach < distance {
            return;
        }
        if self.attack_time <= 0 {
            self.attack_time = OCELOT_ATTACK_COOLDOWN;
            context.act(MobAction::Attack(target.id));
        }
    }
}

pub fn bow_power(ticks: i32) -> f32 {
    let time = ticks as f32 / 20.0;
    let power = (time * time + time * 2.0) / 3.0;
    if power > 1.0 { 1.0 } else { power }
}

pub fn weapon_hand(mob: &MobView, item: ItemKind) -> Hand {
    if mob.main_hand == Some(item.id()) {
        Hand::Main
    } else {
        Hand::Off
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RangedBowAttackGoal {
    speed_modifier: f64,
    attack_interval_min: i32,
    attack_radius_sqr: f32,
    attack_time: i32,
    see_time: i32,
    strafing_clockwise: bool,
    strafing_backwards: bool,
    strafing_time: i32,
}

impl RangedBowAttackGoal {
    pub fn new(speed_modifier: f64, attack_interval_min: i32, attack_radius: f32) -> Self {
        Self {
            speed_modifier,
            attack_interval_min,
            attack_radius_sqr: attack_radius * attack_radius,
            attack_time: -1,
            see_time: 0,
            strafing_clockwise: false,
            strafing_backwards: false,
            strafing_time: -1,
        }
    }

    fn holding_bow(mob: &MobView) -> bool {
        mob.is_holding(ItemKind::Bow.id())
    }

    pub(crate) fn can_use<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) -> bool {
        context.target().is_some() && Self::holding_bow(context.mob)
    }

    pub(crate) fn can_continue_to_use<W: AiWorld>(
        &mut self,
        context: &mut AiContext<'_, W>,
    ) -> bool {
        (self.can_use(context) || !context.brain.navigation.is_done())
            && Self::holding_bow(context.mob)
    }

    pub(crate) fn start<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        context.body.aggressive = true;
    }

    pub(crate) fn stop<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        context.body.aggressive = false;
        self.see_time = 0;
        self.attack_time = -1;
        context.body.using_item = None;
    }

    pub(crate) fn tick<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        let Some(target) = context.target() else {
            return;
        };
        let distance = context.mob.distance_to_sqr(target.position);
        let world = context.world;
        let sees = context
            .brain
            .sensing
            .has_line_of_sight(world, context.mob, &target);
        if sees != (self.see_time > 0) {
            self.see_time = 0;
        }
        if sees {
            self.see_time += 1;
        } else {
            self.see_time -= 1;
        }
        if distance <= f64::from(self.attack_radius_sqr) && self.see_time >= 20 {
            context.stop_navigation();
            self.strafing_time += 1;
        } else {
            context.move_to_entity(&target, self.speed_modifier);
            self.strafing_time = -1;
        }
        if self.strafing_time >= STRAFE_TICKS {
            if f64::from(context.brain.random.next_float()) < STRAFE_FLIP_CHANCE {
                self.strafing_clockwise = !self.strafing_clockwise;
            }
            if f64::from(context.brain.random.next_float()) < STRAFE_FLIP_CHANCE {
                self.strafing_backwards = !self.strafing_backwards;
            }
            self.strafing_time = 0;
        }
        if self.strafing_time > -1 {
            if distance > f64::from(self.attack_radius_sqr * 0.75) {
                self.strafing_backwards = false;
            } else if distance < f64::from(self.attack_radius_sqr * 0.25) {
                self.strafing_backwards = true;
            }
            let forwards = if self.strafing_backwards { -0.5 } else { 0.5 };
            let right = if self.strafing_clockwise { 0.5 } else { -0.5 };
            context.brain.movement.strafe(forwards, right);
            look_at_entity(
                context.mob,
                context.body,
                &target,
                MELEE_LOOK_SPEED,
                MELEE_LOOK_SPEED,
            );
        } else {
            context
                .brain
                .look
                .set_look_at(eye_of(&target), MELEE_LOOK_SPEED, MELEE_LOOK_SPEED);
        }
        if context.body.using_item.is_some() {
            if !sees && self.see_time < SEE_TIME_LIMIT {
                context.body.using_item = None;
            } else if sees {
                let ticks = context.mob.ticks_using_item;
                if ticks >= BOW_USE_TICKS {
                    context.body.using_item = None;
                    context.act(MobAction::RangedAttack {
                        target: target.id,
                        power: bow_power(ticks),
                    });
                    self.attack_time = self.attack_interval_min;
                }
            }
        } else {
            self.attack_time -= 1;
            if self.attack_time <= 0 && self.see_time >= SEE_TIME_LIMIT {
                context.body.using_item = Some(weapon_hand(context.mob, ItemKind::Bow));
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SwellGoal {
    target: Option<EntityId>,
}

impl SwellGoal {
    pub(crate) fn can_use<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) -> bool {
        if context.body.swell_dir > 0 {
            return true;
        }
        context.target().is_some_and(|target| {
            target.alive && context.mob.distance_to_sqr(target.position) < SWELL_START_SQR
        })
    }

    pub(crate) fn start<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        context.stop_navigation();
        self.target = context.target().map(|target| target.id);
    }

    pub(crate) fn stop(&mut self) {
        self.target = None;
    }

    pub(crate) fn tick<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        let target = self.target.and_then(|id| context.world.living(id));
        let world = context.world;
        context.body.swell_dir = match target {
            Some(target) if target.alive => {
                if context.mob.distance_to_sqr(target.position) > SWELL_STOP_SQR
                    || !context
                        .brain
                        .sensing
                        .has_line_of_sight(world, context.mob, &target)
                {
                    -1
                } else {
                    1
                }
            }
            _ => -1,
        };
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LeapAtTargetGoal {
    vertical: f32,
    target: Option<Living>,
}

impl LeapAtTargetGoal {
    pub fn new(vertical: f32) -> Self {
        Self {
            vertical,
            target: None,
        }
    }

    pub(crate) fn can_use<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) -> bool {
        self.target = context.target();
        let Some(target) = self.target else {
            return false;
        };
        let distance = context.mob.distance_to_sqr(target.position);
        if !(LEAP_MIN_SQR..=LEAP_MAX_SQR).contains(&distance) {
            return false;
        }
        if !context.mob.on_ground {
            return false;
        }
        context
            .brain
            .random
            .next_int(reduced_tick_delay(LEAP_CHANCE))
            == 0
    }

    pub(crate) fn can_continue_to_use<W: AiWorld>(
        &mut self,
        context: &mut AiContext<'_, W>,
    ) -> bool {
        !context.mob.on_ground
    }

    pub(crate) fn start<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        let Some(target) = self.target else {
            return;
        };
        let movement = context.body.velocity;
        let mob = context.mob;
        let mut leap = Vec3::new(
            target.position.x - mob.position.x,
            0.0,
            target.position.z - mob.position.z,
        );
        if leap.length_sqr() > 1.0e-7 {
            leap = leap.normalize().scale(LEAP_HORIZONTAL) + movement.scale(LEAP_CARRY);
        }
        context.body.velocity = Vec3::new(leap.x, f64::from(self.vertical), leap.z);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TargetMemory {
    must_see: bool,
    unseen_ticks: i32,
    unseen_memory_ticks: i32,
    target_mob: Option<EntityId>,
}

impl TargetMemory {
    pub const fn new(must_see: bool) -> Self {
        Self {
            must_see,
            unseen_ticks: 0,
            unseen_memory_ticks: UNSEEN_MEMORY,
            target_mob: None,
        }
    }

    pub(crate) fn can_continue_to_use<W: AiWorld>(
        &mut self,
        context: &mut AiContext<'_, W>,
    ) -> bool {
        let target = context
            .target()
            .or_else(|| self.target_mob.and_then(|id| context.world.living(id)));
        let Some(target) = target else {
            return false;
        };
        if !targeting::mob_can_attack(context.mob, context.world, &target) {
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
                if self.unseen_ticks > reduced_tick_delay(self.unseen_memory_ticks) {
                    return false;
                }
            }
        }
        context.set_target(Some(target), TargetReason::ClosestEntity);
        true
    }

    pub(crate) fn start(&mut self) {
        self.unseen_ticks = 0;
    }

    pub(crate) fn stop<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        context.set_target(None, TargetReason::ForgotTarget);
        self.target_mob = None;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HurtByTargetGoal {
    memory: TargetMemory,
    timestamp: i32,
    alert_others: Option<&'static [EntityKind]>,
    polar_bear: bool,
}

impl Default for HurtByTargetGoal {
    fn default() -> Self {
        Self::new()
    }
}

impl HurtByTargetGoal {
    pub const fn new() -> Self {
        Self {
            memory: TargetMemory::new(true),
            timestamp: 0,
            alert_others: None,
            polar_bear: false,
        }
    }

    pub const fn polar_bear() -> Self {
        Self {
            polar_bear: true,
            ..Self::new()
        }
    }

    pub const fn alerting(ignored: &'static [EntityKind]) -> Self {
        Self {
            alert_others: Some(ignored),
            ..Self::new()
        }
    }

    pub(crate) fn memory(&mut self) -> &mut TargetMemory {
        &mut self.memory
    }

    pub(crate) fn can_use<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) -> bool {
        let timestamp = context.mob.last_hurt_by_mob_at;
        let Some(attacker) = context
            .mob
            .last_hurt_by_mob
            .and_then(|id| context.world.living(id))
        else {
            return false;
        };
        if timestamp == self.timestamp {
            return false;
        }
        if attacker.player && context.world.universal_anger() {
            return false;
        }
        let conditions = TargetingConditions::for_combat()
            .ignore_line_of_sight()
            .ignore_invisibility_testing();
        conditions.test(context, &attacker, |_| true)
            && context.mob.is_within_home(attacker.position.block())
    }

    pub(crate) fn start<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        let attacker = context
            .mob
            .last_hurt_by_mob
            .and_then(|id| context.world.living(id));
        context.set_target(attacker, TargetReason::TargetAttackedEntity);
        self.memory.target_mob = context.target().map(|target| target.id);
        self.timestamp = context.mob.last_hurt_by_mob_at;
        self.memory.unseen_memory_ticks = HURT_BY_MEMORY;
        if let Some(ignored) = self.alert_others {
            Self::alert(context, ignored, false);
        }
        self.memory.start();
        if self.polar_bear && context.mob.baby {
            Self::alert(context, &[], true);
            self.memory.stop(context);
        }
    }

    fn alert<W: AiWorld>(
        context: &mut AiContext<'_, W>,
        ignored: &[EntityKind],
        adults_only: bool,
    ) {
        let Some(attacker) = context.mob.last_hurt_by_mob else {
            return;
        };
        let mob = context.mob;
        let follow = mob.follow_range;
        let corner = mob.position;
        let area = rspigot_domain::entity::Aabb::new(
            corner.x - follow,
            corner.y - ALERT_RANGE_Y,
            corner.z - follow,
            corner.x + 1.0 + follow,
            corner.y + 1.0 + ALERT_RANGE_Y,
            corner.z + 1.0 + follow,
        );
        let nearby = context
            .world
            .living_of(TargetKind::Class(mob.kind.class()), &area);
        for other in nearby {
            if other.id == mob.id
                || other.spectator
                || other.has_target
                || (adults_only && other.baby)
                || ignored.contains(&other.kind)
            {
                continue;
            }
            if mob.kind.extends("TamableAnimal")
                && (mob.owner != other.facts.owner
                    || (other.facts.tamed && other.facts.owner == Some(attacker)))
            {
                continue;
            }
            context.act(MobAction::Alert {
                mob: other.id,
                target: attacker,
            });
        }
    }
}
