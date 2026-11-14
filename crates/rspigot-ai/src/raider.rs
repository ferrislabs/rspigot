use rspigot_domain::entity::species::raider::{
    PATROL_COMPANION_RANGE, PATROL_FAILURE_COOLDOWN, PATROL_LEADER_SPEED, PATROL_RETARGET_RANGE,
    PATROL_SIDE_DEGREES, PATROL_SIDE_STEP, PATROL_SPEED, PATROL_STEP, PATROL_TARGET_SPAN,
    PATROL_WANDER_SPAN,
};
use rspigot_domain::explosion::shuffle;
use rspigot_domain::poi::{Occupancy, PoiType};
use rspigot_domain::{BlockPos, EntityId, Vec3, mth};

use crate::action::MobAction;
use crate::blocks::{BreakDoorGoal, OpenDoorGoal};
use crate::brain::AiContext;
use crate::goal::{Flag, Flags, Goal, reduced_tick_delay};
use crate::goals::{MobGoal, NearestAttackableTargetGoal};
use crate::random::RandomSource;
use crate::registry::Goals;
use crate::scan::closer_to_center_than;
use crate::villager::RaidFacts;
use crate::world::{AiWorld, TargetKind, inflate};

const RAID_BANNER_PRIORITY: i32 = 1;
const PATHFIND_TO_RAID_PRIORITY: i32 = 3;
const PATROL_PRIORITY: i32 = 4;
const MOVE_THROUGH_VILLAGE_PRIORITY: i32 = 4;
const CELEBRATION_PRIORITY: i32 = 5;
const MOVE_THROUGH_VILLAGE_SPEED: f64 = 1.05;
const MOVE_THROUGH_VILLAGE_DISTANCE: i32 = 1;
const HEAL_RAIDERS_INTERVAL: i32 = 500;
const HEAL_RAIDERS_COOLDOWN: i32 = 200;
const ATTACK_PLAYERS_INTERVAL: i32 = 10;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct RaiderFacts {
    pub patrolling: bool,
    pub patrol_leader: bool,
    pub patrol_target: Option<BlockPos>,
    pub raid: Option<RaidFacts>,
    pub can_be_leader: bool,
    pub wearing_banner: bool,
    pub leader_alive: bool,
    pub johnny: bool,
    pub blind: bool,
}

impl RaiderFacts {
    pub fn has_active_raid(&self) -> bool {
        self.raid.is_some_and(|raid| raid.active)
    }

    fn raid_in_progress(&self) -> Option<RaidFacts> {
        self.raid.filter(|raid| raid.active && !raid.over)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RaiderAction {
    StopPatrolling,
    SetPatrolTarget {
        mob: EntityId,
        target: BlockPos,
    },
    ShoutTarget {
        mob: EntityId,
        target: EntityId,
        aggressive: bool,
    },
    ResetNoActionTime,
    SetCelebrating(bool),
    CelebrateSound,
    Recruit,
    PickUpBanner(EntityId),
}

fn y_rot(vector: Vec3, angle: f32) -> Vec3 {
    let cos = f64::from(mth::cos(f64::from(angle)));
    let sin = f64::from(mth::sin(f64::from(angle)));
    Vec3::new(
        vector.x * cos + vector.z * sin,
        vector.y,
        vector.z * cos - vector.x * sin,
    )
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LongDistancePatrolGoal {
    speed_modifier: f64,
    leader_speed_modifier: f64,
    cooldown_until: i64,
}

impl Default for LongDistancePatrolGoal {
    fn default() -> Self {
        Self::new(PATROL_SPEED, PATROL_LEADER_SPEED)
    }
}

impl LongDistancePatrolGoal {
    pub const fn new(speed_modifier: f64, leader_speed_modifier: f64) -> Self {
        Self {
            speed_modifier,
            leader_speed_modifier,
            cooldown_until: -1,
        }
    }

    pub fn cooldown_until(&self) -> i64 {
        self.cooldown_until
    }

    fn can_use<W: AiWorld>(&self, context: &mut AiContext<'_, W>) -> bool {
        let facts = context.mob.species.raider;
        context.world.game_time() >= self.cooldown_until
            && facts.patrolling
            && context.target().is_none()
            && facts.patrol_target.is_some()
    }

    fn companions<W: AiWorld>(context: &AiContext<'_, W>) -> Vec<EntityId> {
        let area = inflate(
            &context.mob.bounds(),
            PATROL_COMPANION_RANGE,
            PATROL_COMPANION_RANGE,
            PATROL_COMPANION_RANGE,
        );
        context
            .world
            .living_of(TargetKind::Class("PatrollingMonster"), &area)
            .into_iter()
            .filter(|other| other.id != context.mob.id)
            .map(|other| other.id)
            .collect()
    }

    fn move_randomly<W: AiWorld>(&self, context: &mut AiContext<'_, W>) -> bool {
        let dx = context.brain.random.next_int(PATROL_WANDER_SPAN);
        let dz = context.brain.random.next_int(PATROL_WANDER_SPAN);
        let half = PATROL_WANDER_SPAN / 2;
        let target = context
            .world
            .motion_blocking_no_leaves_pos(context.mob.block_pos().offset(dx - half, 0, dz - half));
        context.move_to_pos(
            Vec3::new(
                f64::from(target.x),
                f64::from(target.y),
                f64::from(target.z),
            ),
            self.speed_modifier,
        )
    }

    fn tick<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        let facts = context.mob.species.raider;
        let leader = facts.patrol_leader;
        if !context.brain.navigation.is_done() {
            return;
        }
        let companions = Self::companions(context);
        if facts.patrolling && companions.is_empty() {
            context.act(MobAction::Raider(RaiderAction::StopPatrolling));
            return;
        }
        let Some(patrol_target) = facts.patrol_target else {
            return;
        };
        let position = context.mob.position;
        if leader {
            let center = Vec3::new(
                f64::from(patrol_target.x) + 0.5,
                f64::from(patrol_target.y) + 0.5,
                f64::from(patrol_target.z) + 0.5,
            );
            if center.distance_squared(position) < PATROL_RETARGET_RANGE * PATROL_RETARGET_RANGE {
                let span = PATROL_TARGET_SPAN;
                let dx = context.brain.random.next_int(span) - span / 2;
                let dz = context.brain.random.next_int(span) - span / 2;
                let target = context.mob.block_pos().offset(dx, 0, dz);
                context.act(MobAction::Raider(RaiderAction::SetPatrolTarget {
                    mob: context.mob.id,
                    target,
                }));
                return;
            }
        }
        let anchor = patrol_target.center();
        let delta = Vec3::new(
            position.x - anchor.x,
            position.y - anchor.y,
            position.z - anchor.z,
        );
        let turned = y_rot(delta, PATROL_SIDE_DEGREES as f32).scale(PATROL_SIDE_STEP);
        let side = Vec3::new(
            turned.x + anchor.x,
            turned.y + anchor.y,
            turned.z + anchor.z,
        );
        let heading = Vec3::new(
            side.x - position.x,
            side.y - position.y,
            side.z - position.z,
        )
        .normalize()
        .scale(PATROL_STEP);
        let aimed = Vec3::new(
            heading.x + position.x,
            heading.y + position.y,
            heading.z + position.z,
        );
        let goal = context.world.motion_blocking_no_leaves_pos(aimed.block());
        let speed = if leader {
            self.leader_speed_modifier
        } else {
            self.speed_modifier
        };
        let target = Vec3::new(f64::from(goal.x), f64::from(goal.y), f64::from(goal.z));
        if context.move_to_pos(target, speed) {
            if leader {
                for companion in companions {
                    context.act(MobAction::Raider(RaiderAction::SetPatrolTarget {
                        mob: companion,
                        target: goal,
                    }));
                }
            }
        } else {
            self.move_randomly(context);
            self.cooldown_until = context.world.game_time() + PATROL_FAILURE_COOLDOWN;
        }
    }
}

impl<W: AiWorld> Goal<AiContext<'_, W>> for LongDistancePatrolGoal {
    fn flags(&self) -> Flags {
        Flags::of(Flag::Move)
    }

    fn can_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        Self::can_use(self, context)
    }

    fn tick(&mut self, context: &mut AiContext<'_, W>) {
        Self::tick(self, context);
    }
}

const BANNER_SPEED: f64 = 1.15;
const BANNER_PICKUP_DISTANCE: f64 = 1.414;
const BANNER_RETRY_TICKS: i64 = 600;
const BANNER_SEARCH_HEIGHT: f64 = 8.0;
const RECRUIT_INTERVAL: i32 = 20;
const RAID_APPROACH_HORIZONTAL: i32 = 15;
const RAID_APPROACH_VERTICAL: i32 = 4;
const POI_SEARCH_RADIUS: i32 = 48;
const VISITED_LIMIT: usize = 2;
const VILLAGE_NEAR_HORIZONTAL: i32 = 16;
const VILLAGE_NEAR_VERTICAL: i32 = 7;
const VILLAGE_NEAR_RADIANS: f32 = std::f32::consts::PI / 10.0;
const VILLAGE_FAR_HORIZONTAL: i32 = 8;
const CELEBRATE_SOUND_INTERVAL: i32 = 100;
const CELEBRATE_JUMP_INTERVAL: i32 = 50;
const BREAK_DOOR_RETRY_TICKS: i32 = 10;
const JOHNNY_INTERVAL: i32 = 0;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ObtainRaidLeaderBannerGoal {
    unreachable: Vec<(EntityId, i64)>,
    path: Option<crate::Path>,
    pursued: Option<EntityId>,
    following: bool,
}

impl ObtainRaidLeaderBannerGoal {
    pub fn pursued(&self) -> Option<EntityId> {
        self.pursued
    }

    fn cannot_pick_up(facts: &RaiderFacts) -> bool {
        !facts.has_active_raid()
            || facts.raid.is_some_and(|raid| raid.over)
            || !facts.can_be_leader
            || facts.wearing_banner
            || facts.leader_alive
    }

    fn can_use<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) -> bool {
        let facts = context.mob.species.raider;
        if Self::cannot_pick_up(&facts) {
            return false;
        }
        let follow = context.mob.follow_range;
        let area = inflate(&context.mob.bounds(), follow, BANNER_SEARCH_HEIGHT, follow);
        let now = context.world.game_time();
        let mut cache = Vec::new();
        for item in context.world.items_in(&area) {
            if item.has_pickup_delay || !item.alive || !context.world.is_ominous_banner(item.id) {
                continue;
            }
            let until = self
                .unreachable
                .iter()
                .find(|(id, _)| *id == item.id)
                .map_or(i64::MIN, |(_, until)| *until);
            if until > now {
                cache.push((item.id, until));
                continue;
            }
            let path = context.create_path_to(item.position.block(), 1);
            if let Some(path) = path.filter(crate::Path::can_reach) {
                self.path = Some(path);
                self.pursued = Some(item.id);
                return true;
            }
            cache.push((item.id, now + BANNER_RETRY_TICKS));
        }
        self.unreachable = cache;
        false
    }

    fn can_continue_to_use<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) -> bool {
        let Some(pursued) = self.pursued else {
            return false;
        };
        if !self.following {
            return false;
        }
        let Some(item) = context.world.dropped_item(pursued) else {
            return false;
        };
        item.alive
            && !context.brain.navigation.is_done()
            && !Self::cannot_pick_up(&context.mob.species.raider)
    }

    fn start<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        let path = self.path.take();
        self.following = context.move_along(path, BANNER_SPEED);
    }

    fn stop(&mut self) {
        self.path = None;
        self.pursued = None;
        self.following = false;
    }

    fn tick<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        let Some(pursued) = self.pursued else {
            return;
        };
        let Some(item) = context.world.dropped_item(pursued) else {
            return;
        };
        let delta = item.position - context.mob.position;
        if delta.length_sqr() < BANNER_PICKUP_DISTANCE * BANNER_PICKUP_DISTANCE {
            context.act(MobAction::Raider(RaiderAction::PickUpBanner(pursued)));
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PathfindToRaidGoal {
    recruitment_tick: i32,
}

impl PathfindToRaidGoal {
    fn can_use<W: AiWorld>(&self, context: &mut AiContext<'_, W>) -> bool {
        context.target().is_none()
            && context.mob.species.raider.raid_in_progress().is_some()
            && !context.world.is_village(context.mob.block_pos())
    }

    fn can_continue_to_use<W: AiWorld>(&self, context: &mut AiContext<'_, W>) -> bool {
        context.mob.species.raider.raid_in_progress().is_some()
            && !context.world.is_village(context.mob.block_pos())
    }

    fn tick<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        let Some(raid) = context.mob.species.raider.raid.filter(|raid| raid.active) else {
            return;
        };
        if self.recruitment_tick < context.mob.tick_count {
            self.recruitment_tick = context.mob.tick_count + RECRUIT_INTERVAL;
            context.act(MobAction::Raider(RaiderAction::Recruit));
        }
        if context.brain.navigation.is_done() {
            let center = bottom_center(raid.center);
            if let Some(position) = crate::random_pos::pos_towards(
                context,
                RAID_APPROACH_HORIZONTAL,
                RAID_APPROACH_VERTICAL,
                center,
                f64::from(std::f32::consts::FRAC_PI_2),
            ) {
                context.move_to_pos(position, 1.0);
            }
        }
    }
}

fn bottom_center(pos: BlockPos) -> Vec3 {
    Vec3::new(
        f64::from(pos.x) + 0.5,
        f64::from(pos.y),
        f64::from(pos.z) + 0.5,
    )
}

#[derive(Debug, Clone, PartialEq)]
pub struct RaiderMoveThroughVillageGoal {
    speed_modifier: f64,
    distance: i32,
    visited: Vec<BlockPos>,
    poi_pos: BlockPos,
    stuck: bool,
}

impl RaiderMoveThroughVillageGoal {
    pub const fn new(speed_modifier: f64, distance: i32) -> Self {
        Self {
            speed_modifier,
            distance,
            visited: Vec::new(),
            poi_pos: BlockPos::new(0, 0, 0),
            stuck: false,
        }
    }

    pub const fn speed_modifier(&self) -> f64 {
        self.speed_modifier
    }

    pub const fn distance(&self) -> i32 {
        self.distance
    }

    pub fn visited(&self) -> &[BlockPos] {
        &self.visited
    }

    fn has_suitable_poi<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) -> bool {
        let mut homes: Vec<BlockPos> = context
            .world
            .pois_in_range(context.mob.block_pos(), POI_SEARCH_RADIUS, Occupancy::Any)
            .into_iter()
            .filter(|record| record.kind() == PoiType::Home)
            .map(|record| record.pos())
            .collect();
        shuffle(&mut homes, &mut context.brain.random);
        match homes.into_iter().find(|pos| !self.visited.contains(pos)) {
            Some(pos) => {
                self.poi_pos = pos;
                true
            }
            None => false,
        }
    }

    fn can_use<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) -> bool {
        if self.visited.len() > VISITED_LIMIT {
            self.visited.remove(0);
        }
        context.mob.species.raider.raid_in_progress().is_some()
            && self.has_suitable_poi(context)
            && context.target().is_none()
    }

    fn can_continue_to_use<W: AiWorld>(&self, context: &mut AiContext<'_, W>) -> bool {
        !context.brain.navigation.is_done()
            && context.target().is_none()
            && !closer_to_center_than(
                self.poi_pos,
                context.mob.position,
                f64::from(context.mob.width + self.distance as f32),
            )
            && !self.stuck
    }

    fn start<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        context.act(MobAction::Raider(RaiderAction::ResetNoActionTime));
        context.move_to_pos(bottom_center(self.poi_pos), self.speed_modifier);
        self.stuck = false;
    }

    fn stop<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        if closer_to_center_than(self.poi_pos, context.mob.position, f64::from(self.distance)) {
            self.visited.push(self.poi_pos);
        }
    }

    fn tick<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        if !context.brain.navigation.is_done() {
            return;
        }
        let toward = bottom_center(self.poi_pos);
        let position = crate::random_pos::pos_towards(
            context,
            VILLAGE_NEAR_HORIZONTAL,
            VILLAGE_NEAR_VERTICAL,
            toward,
            f64::from(VILLAGE_NEAR_RADIANS),
        )
        .or_else(|| {
            crate::random_pos::pos_towards(
                context,
                VILLAGE_FAR_HORIZONTAL,
                VILLAGE_NEAR_VERTICAL,
                toward,
                f64::from(std::f32::consts::FRAC_PI_2),
            )
        });
        match position {
            Some(position) => {
                context.move_to_pos(position, self.speed_modifier);
            }
            None => self.stuck = true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RaiderCelebrationGoal;

impl RaiderCelebrationGoal {
    fn can_use<W: AiWorld>(context: &mut AiContext<'_, W>) -> bool {
        context.target().is_none()
            && context
                .mob
                .species
                .raider
                .raid
                .is_some_and(|raid| raid.loss)
    }

    fn tick<W: AiWorld>(context: &mut AiContext<'_, W>) {
        if !context.mob.silent {
            let delay = crate::goal::adjusted_tick_delay(false, CELEBRATE_SOUND_INTERVAL);
            if context.brain.random.next_int(delay) == 0 {
                context.act(MobAction::Raider(RaiderAction::CelebrateSound));
            }
        }
        if !context.mob.passenger {
            let delay = crate::goal::adjusted_tick_delay(false, CELEBRATE_JUMP_INTERVAL);
            if context.brain.random.next_int(delay) == 0 {
                context.brain.jump.jump();
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NearestHealableRaiderTargetGoal {
    random_interval: i32,
    cooldown: i32,
    target: NearestAttackableTargetGoal,
}

impl Default for NearestHealableRaiderTargetGoal {
    fn default() -> Self {
        Self {
            random_interval: reduced_tick_delay(HEAL_RAIDERS_INTERVAL),
            cooldown: 0,
            target: NearestAttackableTargetGoal::with(
                TargetKind::Class("Raider"),
                0,
                true,
                crate::goals::TargetFilter::HealableRaider,
            ),
        }
    }
}

impl NearestHealableRaiderTargetGoal {
    pub fn cooldown(&self) -> i32 {
        self.cooldown
    }

    pub fn random_interval(&self) -> i32 {
        self.random_interval
    }

    pub fn decrement_cooldown(&mut self) {
        self.cooldown = self.cooldown.wrapping_sub(1);
    }

    fn can_use<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) -> bool {
        if self.cooldown < 1 && context.brain.random.next_boolean() {
            return context.mob.species.raider.has_active_raid() && self.target.can_use(context);
        }
        false
    }

    fn start(&mut self) {
        self.cooldown = reduced_tick_delay(HEAL_RAIDERS_COOLDOWN);
    }
}

impl<W: AiWorld> Goal<AiContext<'_, W>> for NearestHealableRaiderTargetGoal {
    fn flags(&self) -> Flags {
        Flags::of(Flag::Target)
    }

    fn can_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        Self::can_use(self, context)
    }

    fn can_continue_to_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        self.target.can_continue_to_use(context)
    }

    fn start(&mut self, context: &mut AiContext<'_, W>) {
        Self::start(self);
        self.target.start(context);
    }

    fn stop(&mut self, context: &mut AiContext<'_, W>) {
        self.target.stop(context);
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NearestAttackableWitchTargetGoal {
    target: NearestAttackableTargetGoal,
    can_attack: bool,
}

impl Default for NearestAttackableWitchTargetGoal {
    fn default() -> Self {
        Self {
            target: NearestAttackableTargetGoal::with(
                TargetKind::Player,
                ATTACK_PLAYERS_INTERVAL,
                true,
                crate::goals::TargetFilter::Any,
            ),
            can_attack: true,
        }
    }
}

impl NearestAttackableWitchTargetGoal {
    pub fn can_attack(&self) -> bool {
        self.can_attack
    }

    pub fn set_can_attack(&mut self, can_attack: bool) {
        self.can_attack = can_attack;
    }
}

impl<W: AiWorld> Goal<AiContext<'_, W>> for NearestAttackableWitchTargetGoal {
    fn flags(&self) -> Flags {
        Flags::of(Flag::Target)
    }

    fn can_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        self.can_attack && self.target.can_use(context)
    }

    fn can_continue_to_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        self.target.can_continue_to_use(context)
    }

    fn start(&mut self, context: &mut AiContext<'_, W>) {
        self.target.start(context);
    }

    fn stop(&mut self, context: &mut AiContext<'_, W>) {
        self.target.stop(context);
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VindicatorBreakDoorGoal {
    door: BreakDoorGoal,
}

impl Default for VindicatorBreakDoorGoal {
    fn default() -> Self {
        Self {
            door: BreakDoorGoal::vindicator(),
        }
    }
}

impl VindicatorBreakDoorGoal {
    fn can_use<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) -> bool {
        if !context.mob.species.raider.has_active_raid() {
            return false;
        }
        let delay = reduced_tick_delay(BREAK_DOOR_RETRY_TICKS);
        context.brain.random.next_int(delay) == 0 && self.door.can_use(context)
    }

    fn can_continue_to_use<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) -> bool {
        context.mob.species.raider.has_active_raid()
            && !self.door.passed()
            && self.door.can_continue_to_use(context)
    }

    fn start<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        self.door.start(context);
        context.act(MobAction::Raider(RaiderAction::ResetNoActionTime));
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct JohnnyAttackGoal {
    target: NearestAttackableTargetGoal,
}

impl Default for JohnnyAttackGoal {
    fn default() -> Self {
        Self {
            target: NearestAttackableTargetGoal::with(
                TargetKind::Class("LivingEntity"),
                JOHNNY_INTERVAL,
                true,
                crate::goals::TargetFilter::Attackable,
            ),
        }
    }
}

impl JohnnyAttackGoal {
    fn can_use<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) -> bool {
        context.mob.species.raider.johnny && self.target.can_use(context)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct RaiderOpenDoorGoal {
    door: OpenDoorGoal,
}

impl RaiderOpenDoorGoal {
    fn can_use<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) -> bool {
        self.door.can_use(context) && context.mob.species.raider.has_active_raid()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum RaiderGoal {
    VindicatorBreakDoor(VindicatorBreakDoorGoal),
    JohnnyAttack(JohnnyAttackGoal),
    OpenDoor(RaiderOpenDoorGoal),
    LongDistancePatrol(LongDistancePatrolGoal),
    ObtainRaidLeaderBanner(ObtainRaidLeaderBannerGoal),
    PathfindToRaid(PathfindToRaidGoal),
    MoveThroughVillage(RaiderMoveThroughVillageGoal),
    Celebration(RaiderCelebrationGoal),
    HealRaiders(NearestHealableRaiderTargetGoal),
}

impl<W: AiWorld> Goal<AiContext<'_, W>> for RaiderGoal {
    fn flags(&self) -> Flags {
        match self {
            Self::LongDistancePatrol(goal) => Goal::<AiContext<'_, W>>::flags(goal),
            Self::HealRaiders(goal) => Goal::<AiContext<'_, W>>::flags(goal),
            Self::JohnnyAttack(_) => Flags::of(Flag::Target),
            Self::OpenDoor(_) => Flags::NONE,
            Self::VindicatorBreakDoor(_)
            | Self::ObtainRaidLeaderBanner(_)
            | Self::PathfindToRaid(_)
            | Self::MoveThroughVillage(_)
            | Self::Celebration(_) => Flags::of(Flag::Move),
        }
    }

    fn can_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        match self {
            Self::LongDistancePatrol(goal) => Goal::can_use(goal, context),
            Self::HealRaiders(goal) => Goal::can_use(goal, context),
            Self::VindicatorBreakDoor(goal) => goal.can_use(context),
            Self::JohnnyAttack(goal) => goal.can_use(context),
            Self::OpenDoor(goal) => goal.can_use(context),
            Self::ObtainRaidLeaderBanner(goal) => goal.can_use(context),
            Self::PathfindToRaid(goal) => goal.can_use(context),
            Self::MoveThroughVillage(goal) => goal.can_use(context),
            Self::Celebration(_) => RaiderCelebrationGoal::can_use(context),
        }
    }

    fn can_continue_to_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        match self {
            Self::LongDistancePatrol(goal) => Goal::can_continue_to_use(goal, context),
            Self::HealRaiders(goal) => Goal::can_continue_to_use(goal, context),
            Self::VindicatorBreakDoor(goal) => goal.can_continue_to_use(context),
            Self::JohnnyAttack(goal) => goal.target.can_continue_to_use(context),
            Self::OpenDoor(goal) => goal.door.can_continue_to_use(),
            Self::ObtainRaidLeaderBanner(goal) => goal.can_continue_to_use(context),
            Self::PathfindToRaid(goal) => goal.can_continue_to_use(context),
            Self::MoveThroughVillage(goal) => goal.can_continue_to_use(context),
            Self::Celebration(_) => RaiderCelebrationGoal::can_use(context),
        }
    }

    fn start(&mut self, context: &mut AiContext<'_, W>) {
        match self {
            Self::HealRaiders(goal) => Goal::start(goal, context),
            Self::VindicatorBreakDoor(goal) => goal.start(context),
            Self::JohnnyAttack(goal) => {
                goal.target.start(context);
                context.act(MobAction::Raider(RaiderAction::ResetNoActionTime));
            }
            Self::OpenDoor(goal) => goal.door.start(context),
            Self::ObtainRaidLeaderBanner(goal) => goal.start(context),
            Self::MoveThroughVillage(goal) => goal.start(context),
            Self::Celebration(_) => {
                context.act(MobAction::Raider(RaiderAction::SetCelebrating(true)));
            }
            Self::LongDistancePatrol(_) | Self::PathfindToRaid(_) => {}
        }
    }

    fn stop(&mut self, context: &mut AiContext<'_, W>) {
        match self {
            Self::HealRaiders(goal) => Goal::stop(goal, context),
            Self::VindicatorBreakDoor(goal) => goal.door.stop(context),
            Self::JohnnyAttack(goal) => goal.target.stop(context),
            Self::OpenDoor(goal) => goal.door.stop(context),
            Self::ObtainRaidLeaderBanner(goal) => goal.stop(),
            Self::MoveThroughVillage(goal) => goal.stop(context),
            Self::Celebration(_) => {
                context.act(MobAction::Raider(RaiderAction::SetCelebrating(false)));
            }
            Self::LongDistancePatrol(_) | Self::PathfindToRaid(_) => {}
        }
    }

    fn requires_update_every_tick(&self) -> bool {
        matches!(self, Self::OpenDoor(_))
    }

    fn tick(&mut self, context: &mut AiContext<'_, W>) {
        match self {
            Self::VindicatorBreakDoor(goal) => goal.door.tick(context),
            Self::OpenDoor(goal) => goal.door.tick(context),
            Self::JohnnyAttack(_) => {}
            Self::LongDistancePatrol(goal) => Goal::tick(goal, context),
            Self::ObtainRaidLeaderBanner(goal) => goal.tick(context),
            Self::PathfindToRaid(goal) => goal.tick(context),
            Self::MoveThroughVillage(goal) => goal.tick(context),
            Self::Celebration(_) => RaiderCelebrationGoal::tick(context),
            Self::HealRaiders(_) => {}
        }
    }
}

pub(crate) fn register(add: &mut Goals<'_>) {
    add.goal(
        PATROL_PRIORITY,
        MobGoal::Raider(RaiderGoal::LongDistancePatrol(
            LongDistancePatrolGoal::default(),
        )),
    );
    add.goal(
        RAID_BANNER_PRIORITY,
        MobGoal::Raider(RaiderGoal::ObtainRaidLeaderBanner(
            ObtainRaidLeaderBannerGoal::default(),
        )),
    );
    add.goal(
        PATHFIND_TO_RAID_PRIORITY,
        MobGoal::Raider(RaiderGoal::PathfindToRaid(PathfindToRaidGoal::default())),
    );
    add.goal(
        MOVE_THROUGH_VILLAGE_PRIORITY,
        MobGoal::Raider(RaiderGoal::MoveThroughVillage(
            RaiderMoveThroughVillageGoal::new(
                MOVE_THROUGH_VILLAGE_SPEED,
                MOVE_THROUGH_VILLAGE_DISTANCE,
            ),
        )),
    );
    add.goal(
        CELEBRATION_PRIORITY,
        MobGoal::Raider(RaiderGoal::Celebration(RaiderCelebrationGoal)),
    );
}

#[cfg(test)]
mod tests {
    use rspigot_entities::EntityKind;

    use super::*;
    use crate::brain::MobAi;
    use crate::mock::{Harness, Mock, living, seed_where};
    use crate::random::MobRandom;
    use crate::registry::GoalSettings;
    use crate::{HurtByTargetGoal, TargetFilter};

    const SEED: i64 = 11;

    fn witch(world: Mock) -> Harness {
        Harness::new(EntityKind::Witch, Vec3::new(0.5, 64.0, 0.5), world, SEED)
    }

    fn patrolling(harness: &mut Harness, target: BlockPos, leader: bool) {
        harness.mob.species.raider = RaiderFacts {
            patrolling: true,
            patrol_leader: leader,
            patrol_target: Some(target),
            ..RaiderFacts::default()
        };
    }

    fn actions(harness: &mut Harness) -> Vec<MobAction> {
        std::mem::take(&mut harness.brain.actions)
    }

    fn companion_world() -> Mock {
        Mock {
            others: vec![living(7, EntityKind::Pillager, Vec3::new(4.5, 64.0, 0.5))],
            ..Mock::default()
        }
    }

    #[test]
    fn the_patrol_goal_needs_a_patrol_target_and_no_attack_target() {
        let mut harness = witch(Mock::default());
        let goal = LongDistancePatrolGoal::default();
        assert!(!harness.run(|context| goal.can_use(context)));
        patrolling(&mut harness, BlockPos::new(100, 64, 0), false);
        assert!(harness.run(|context| goal.can_use(context)));
        harness.mob.species.raider.patrol_target = None;
        assert!(!harness.run(|context| goal.can_use(context)));
        patrolling(&mut harness, BlockPos::new(100, 64, 0), false);
        harness.mob.species.raider.patrolling = false;
        assert!(!harness.run(|context| goal.can_use(context)));
        patrolling(&mut harness, BlockPos::new(100, 64, 0), false);
        harness.brain.target = Some(EntityId(2));
        harness
            .world
            .players
            .push(living(2, EntityKind::Player, Vec3::new(3.5, 64.0, 0.5)));
        assert!(!harness.run(|context| goal.can_use(context)));
    }

    #[test]
    fn the_patrol_goal_waits_out_its_cooldown() {
        let mut harness = witch(Mock::default());
        patrolling(&mut harness, BlockPos::new(100, 64, 0), false);
        let mut goal = LongDistancePatrolGoal::default();
        assert_eq!(goal.cooldown_until(), -1);
        goal.cooldown_until = 101;
        assert!(!harness.run(|context| goal.can_use(context)));
        goal.cooldown_until = 100;
        assert!(harness.run(|context| goal.can_use(context)));
    }

    #[test]
    fn a_lone_patrolling_raider_stops_patrolling() {
        let mut harness = witch(Mock::default());
        patrolling(&mut harness, BlockPos::new(100, 64, 0), false);
        let mut goal = LongDistancePatrolGoal::default();
        harness.run(|context| goal.tick(context));
        assert_eq!(
            actions(&mut harness),
            vec![MobAction::Raider(RaiderAction::StopPatrolling)]
        );
    }

    fn expected_goal(from: Vec3, target: BlockPos) -> BlockPos {
        let anchor = (
            f64::from(target.x) + 0.5,
            f64::from(target.y),
            f64::from(target.z) + 0.5,
        );
        let delta = (from.x - anchor.0, from.y - anchor.1, from.z - anchor.2);
        let angle: f64 = 90.0;
        let (cos, sin) = (angle.cos(), angle.sin());
        let turned = (
            (delta.0 * cos + delta.2 * sin) * 0.4 + anchor.0,
            delta.1 * 0.4 + anchor.1,
            (delta.2 * cos - delta.0 * sin) * 0.4 + anchor.2,
        );
        let heading = (turned.0 - from.x, turned.1 - from.y, turned.2 - from.z);
        let length = (heading.0 * heading.0 + heading.1 * heading.1 + heading.2 * heading.2).sqrt();
        BlockPos::new(
            (heading.0 / length * 10.0 + from.x).floor() as i32,
            64,
            (heading.2 / length * 10.0 + from.z).floor() as i32,
        )
    }

    #[test]
    fn a_follower_walks_ten_blocks_toward_a_point_beside_the_patrol_target() {
        let target = BlockPos::new(100, 64, 20);
        let mut harness = witch(companion_world());
        patrolling(&mut harness, target, false);
        let mut goal = LongDistancePatrolGoal::default();
        harness.run(|context| goal.tick(context));
        assert!(harness.brain.navigation.is_in_progress());
        assert!(actions(&mut harness).is_empty());
        let end = harness
            .brain
            .navigation
            .path()
            .map(|path| path.target())
            .expect("a path was made");
        let wanted = expected_goal(harness.mob.position, target);
        assert!((end.x - wanted.x).abs() <= 1 && (end.z - wanted.z).abs() <= 1);
        assert_eq!(goal.cooldown_until(), -1);
    }

    #[test]
    fn a_leader_hands_its_new_waypoint_to_every_companion() {
        let target = BlockPos::new(100, 64, 20);
        let mut harness = witch(companion_world());
        patrolling(&mut harness, target, true);
        let mut goal = LongDistancePatrolGoal::default();
        harness.run(|context| goal.tick(context));
        let wanted = expected_goal(harness.mob.position, target);
        let taken = actions(&mut harness);
        assert_eq!(taken.len(), 1);
        let MobAction::Raider(RaiderAction::SetPatrolTarget { mob, target }) = taken[0] else {
            panic!("a waypoint action, got {:?}", taken[0]);
        };
        assert_eq!(mob, EntityId(7));
        assert!((target.x - wanted.x).abs() <= 1 && (target.z - wanted.z).abs() <= 1);
        assert_eq!(target.y, 64);
    }

    #[test]
    fn a_leader_near_its_patrol_target_picks_a_new_one_within_five_hundred_blocks() {
        let mut harness = witch(companion_world());
        patrolling(&mut harness, BlockPos::new(5, 64, 0), true);
        let mut mirror = MobRandom::new(SEED);
        let dx = mirror.next_int(1000) - 500;
        let dz = mirror.next_int(1000) - 500;
        let mut goal = LongDistancePatrolGoal::default();
        harness.run(|context| goal.tick(context));
        assert_eq!(
            actions(&mut harness),
            vec![MobAction::Raider(RaiderAction::SetPatrolTarget {
                mob: EntityId(1),
                target: BlockPos::new(dx, 64, dz),
            })]
        );
    }

    #[test]
    fn a_busy_navigation_keeps_the_patrol_goal_still() {
        let mut harness = witch(companion_world());
        patrolling(&mut harness, BlockPos::new(100, 64, 20), false);
        let mut goal = LongDistancePatrolGoal::default();
        harness.run(|context| goal.tick(context));
        let before = harness.brain.navigation.path().map(|path| path.target());
        harness.mob.species.raider.patrol_target = Some(BlockPos::new(-100, 64, -20));
        harness.run(|context| goal.tick(context));
        assert_eq!(
            harness.brain.navigation.path().map(|path| path.target()),
            before
        );
    }

    #[test]
    fn the_raid_goals_never_start_without_a_raid() {
        let mut harness = witch(Mock::default());
        let mut goals = [
            RaiderGoal::ObtainRaidLeaderBanner(ObtainRaidLeaderBannerGoal::default()),
            RaiderGoal::PathfindToRaid(PathfindToRaidGoal::default()),
            RaiderGoal::MoveThroughVillage(RaiderMoveThroughVillageGoal::new(1.05, 1)),
            RaiderGoal::Celebration(RaiderCelebrationGoal),
        ];
        for goal in &mut goals {
            assert!(!harness.run(|context| Goal::can_use(goal, context)));
            assert!(!harness.run(|context| Goal::can_continue_to_use(goal, context)));
            assert_eq!(
                Goal::<AiContext<'_, Mock>>::flags(goal),
                Flags::of(Flag::Move)
            );
        }
    }

    #[test]
    fn healing_raiders_rolls_a_boolean_only_when_the_cooldown_has_run_out() {
        let mut harness = witch(Mock::default());
        let mut goal = NearestHealableRaiderTargetGoal::default();
        assert_eq!(goal.random_interval(), 250);
        assert_eq!(goal.cooldown(), 0);
        let mut mirror = MobRandom::new(SEED);
        assert!(!harness.run(|context| goal.can_use(context)));
        let _ = mirror.next_boolean();
        assert_eq!(harness.brain.random.next_int(1000), mirror.next_int(1000));
        goal.cooldown = 1;
        let mut untouched = MobRandom::new(SEED);
        let mut again = witch(Mock::default());
        assert!(!again.run(|context| goal.can_use(context)));
        assert_eq!(again.brain.random.next_int(1000), untouched.next_int(1000));
    }

    #[test]
    fn healing_raiders_counts_its_cooldown_down_and_restarts_it_at_a_hundred() {
        let mut goal = NearestHealableRaiderTargetGoal::default();
        goal.decrement_cooldown();
        goal.decrement_cooldown();
        assert_eq!(goal.cooldown(), -2);
        goal.start();
        assert_eq!(goal.cooldown(), 100);
        assert_eq!(
            Goal::<AiContext<'_, Mock>>::flags(&goal),
            Flags::of(Flag::Target)
        );
    }

    #[test]
    fn attacking_players_is_gated_by_the_witch_and_otherwise_a_plain_player_search() {
        let seed = seed_where(|random| random.next_int(5) == 0);
        let world = Mock {
            players: vec![living(2, EntityKind::Player, Vec3::new(4.5, 64.0, 0.5))],
            ..Mock::default()
        };
        let mut harness = Harness::new(EntityKind::Witch, Vec3::new(0.5, 64.0, 0.5), world, seed);
        let mut goal = NearestAttackableWitchTargetGoal::default();
        assert!(goal.can_attack());
        goal.set_can_attack(false);
        let mut mirror = MobRandom::new(seed);
        assert!(!harness.run(|context| goal.can_use(context)));
        assert_eq!(harness.brain.random.next_int(1000), mirror.next_int(1000));
        goal.set_can_attack(true);
        let mut harness = Harness::new(
            EntityKind::Witch,
            Vec3::new(0.5, 64.0, 0.5),
            Mock {
                players: vec![living(2, EntityKind::Player, Vec3::new(4.5, 64.0, 0.5))],
                ..Mock::default()
            },
            seed,
        );
        assert!(harness.run(|context| goal.can_use(context)));
        let mut plain = crate::goals::NearestAttackableTargetGoal::with(
            TargetKind::Player,
            10,
            true,
            TargetFilter::Any,
        );
        let mut twin = Harness::new(
            EntityKind::Witch,
            Vec3::new(0.5, 64.0, 0.5),
            Mock {
                players: vec![living(2, EntityKind::Player, Vec3::new(4.5, 64.0, 0.5))],
                ..Mock::default()
            },
            seed,
        );
        assert!(twin.run(|context| plain.can_use(context)));
        assert_eq!(
            harness.brain.random.next_int(1000),
            twin.brain.random.next_int(1000)
        );
    }

    fn healable(ai: &MobAi) -> NearestHealableRaiderTargetGoal {
        ai.targets()
            .goals()
            .iter()
            .find_map(|wrapped| match wrapped.goal() {
                MobGoal::Raider(RaiderGoal::HealRaiders(goal)) => Some(*goal),
                _ => None,
            })
            .expect("a heal goal")
    }

    fn attacking(ai: &MobAi) -> NearestAttackableWitchTargetGoal {
        ai.targets()
            .goals()
            .iter()
            .find_map(|wrapped| match wrapped.goal() {
                MobGoal::WitchTarget(goal) => Some(*goal),
                _ => None,
            })
            .expect("an attack goal")
    }

    #[test]
    fn each_witch_step_counts_the_heal_cooldown_down_and_opens_the_player_attack_when_it_ends() {
        let mut ai = MobAi::new(EntityKind::Witch, 1, GoalSettings::default());
        for goal in ai.targets_mut().goals_mut() {
            if let MobGoal::Raider(RaiderGoal::HealRaiders(heal)) = goal {
                heal.cooldown = 3;
            }
        }
        ai.raider_ai_step();
        assert_eq!(healable(&ai).cooldown(), 2);
        assert!(!attacking(&ai).can_attack());
        ai.raider_ai_step();
        assert!(!attacking(&ai).can_attack());
        ai.raider_ai_step();
        assert_eq!(healable(&ai).cooldown(), 0);
        assert!(attacking(&ai).can_attack());
        ai.raider_ai_step();
        assert_eq!(healable(&ai).cooldown(), -1);
        assert!(attacking(&ai).can_attack());
    }

    fn hurt_by(attacker: EntityKind) -> bool {
        let world = Mock {
            others: vec![living(9, attacker, Vec3::new(2.5, 64.0, 0.5))],
            ..Mock::default()
        };
        let mut harness = witch(world);
        harness.mob.last_hurt_by_mob = Some(EntityId(9));
        harness.mob.last_hurt_by_mob_at = 5;
        let mut goal = HurtByTargetGoal::ignoring_damage_from(&["Raider"]);
        harness.run(|context| goal.can_use(context))
    }

    #[test]
    fn a_witch_does_not_retaliate_against_a_raider_but_does_against_anyone_else() {
        assert!(!hurt_by(EntityKind::Pillager));
        assert!(!hurt_by(EntityKind::Witch));
        assert!(hurt_by(EntityKind::Zombie));
    }

    #[test]
    fn the_witch_registers_the_raider_goals_then_its_own_with_the_vanilla_priorities() {
        let ai = MobAi::new(EntityKind::Witch, 1, GoalSettings::default());
        let goals: Vec<_> = ai
            .goals()
            .goals()
            .iter()
            .map(|wrapped| (wrapped.priority(), wrapped.goal().clone()))
            .collect();
        assert_eq!(
            goals,
            vec![
                (
                    4,
                    MobGoal::Raider(RaiderGoal::LongDistancePatrol(LongDistancePatrolGoal::new(
                        0.7, 0.595
                    )))
                ),
                (
                    1,
                    MobGoal::Raider(RaiderGoal::ObtainRaidLeaderBanner(
                        ObtainRaidLeaderBannerGoal::default()
                    ))
                ),
                (
                    3,
                    MobGoal::Raider(RaiderGoal::PathfindToRaid(PathfindToRaidGoal::default()))
                ),
                (
                    4,
                    MobGoal::Raider(RaiderGoal::MoveThroughVillage(
                        RaiderMoveThroughVillageGoal::new(1.05, 1)
                    ))
                ),
                (
                    5,
                    MobGoal::Raider(RaiderGoal::Celebration(RaiderCelebrationGoal))
                ),
                (1, MobGoal::Float(crate::goals::FloatGoal)),
                (
                    2,
                    MobGoal::RangedAttack(crate::RangedAttackGoal::new(1.0, 60, 10.0))
                ),
                (
                    2,
                    MobGoal::Stroll(crate::RandomStrollGoal::water_avoiding(1.0))
                ),
                (3, MobGoal::LookAtPlayer(crate::LookAtPlayerGoal::new(8.0))),
                (
                    3,
                    MobGoal::RandomLookAround(crate::RandomLookAroundGoal::default())
                ),
            ]
        );
        let targets: Vec<_> = ai
            .targets()
            .goals()
            .iter()
            .map(|wrapped| (wrapped.priority(), wrapped.goal().clone()))
            .collect();
        assert_eq!(
            targets,
            vec![
                (
                    1,
                    MobGoal::HurtByTarget(HurtByTargetGoal::ignoring_damage_from(&["Raider"]))
                ),
                (
                    2,
                    MobGoal::Raider(RaiderGoal::HealRaiders(
                        NearestHealableRaiderTargetGoal::default()
                    ))
                ),
                (
                    3,
                    MobGoal::WitchTarget(NearestAttackableWitchTargetGoal::default())
                ),
            ]
        );
    }

    #[test]
    fn the_witch_floats_so_it_swims_like_other_monsters() {
        let ai = MobAi::new(EntityKind::Witch, 1, GoalSettings::default());
        assert!(ai.can_float());
    }

    fn raid(active: bool, over: bool, loss: bool) -> RaidFacts {
        RaidFacts {
            center: BlockPos::new(100, 64, 0),
            active,
            over,
            loss,
            victory: over && !loss,
            ..RaidFacts::default()
        }
    }

    fn raider(kind: EntityKind, raid: Option<RaidFacts>) -> Harness {
        let mut harness = Harness::new(kind, Vec3::new(0.5, 64.0, 0.5), Mock::default(), SEED);
        harness.mob.species.raider.raid = raid;
        harness
    }

    fn goal_summary(ai: &MobAi, targets: bool) -> Vec<(i32, &'static str)> {
        let selector = if targets { ai.targets() } else { ai.goals() };
        selector
            .goals()
            .iter()
            .map(|wrapped| {
                let name = match wrapped.goal() {
                    MobGoal::Raider(RaiderGoal::LongDistancePatrol(_)) => "patrol",
                    MobGoal::Raider(RaiderGoal::ObtainRaidLeaderBanner(_)) => "banner",
                    MobGoal::Raider(RaiderGoal::PathfindToRaid(_)) => "pathfind_to_raid",
                    MobGoal::Raider(RaiderGoal::MoveThroughVillage(_)) => "move_through_village",
                    MobGoal::Raider(RaiderGoal::Celebration(_)) => "celebration",
                    MobGoal::Raider(RaiderGoal::HealRaiders(_)) => "heal",
                    MobGoal::Raider(RaiderGoal::VindicatorBreakDoor(_)) => "break_door",
                    MobGoal::Raider(RaiderGoal::OpenDoor(_)) => "open_door",
                    MobGoal::Raider(RaiderGoal::JohnnyAttack(_)) => "johnny",
                    MobGoal::Float(_) => "float",
                    MobGoal::AvoidEntity(_) => "avoid_creaking",
                    MobGoal::Crossbow(_) => "hold_ground",
                    MobGoal::MeleeAttack(_) => "melee",
                    MobGoal::Stroll(_) => "stroll",
                    MobGoal::LookAtPlayer(_) => "look",
                    MobGoal::HurtByTarget(_) => "hurt_by",
                    MobGoal::NearestAttackableTarget(_) => "nearest",
                    _ => "other",
                };
                (wrapped.priority(), name)
            })
            .collect()
    }

    #[test]
    fn a_vindicator_registers_the_raider_goals_then_its_own_with_the_vanilla_priorities() {
        let ai = MobAi::new(EntityKind::Vindicator, 1, GoalSettings::default());
        assert_eq!(
            goal_summary(&ai, false),
            vec![
                (4, "patrol"),
                (1, "banner"),
                (3, "pathfind_to_raid"),
                (4, "move_through_village"),
                (5, "celebration"),
                (0, "float"),
                (1, "avoid_creaking"),
                (2, "break_door"),
                (3, "open_door"),
                (4, "hold_ground"),
                (5, "melee"),
                (8, "stroll"),
                (9, "look"),
                (10, "look"),
            ]
        );
        assert_eq!(
            goal_summary(&ai, true),
            vec![
                (1, "hurt_by"),
                (2, "nearest"),
                (3, "nearest"),
                (3, "nearest"),
                (4, "johnny"),
            ]
        );
        let melee = ai
            .goals()
            .goals()
            .iter()
            .find_map(|wrapped| match wrapped.goal() {
                MobGoal::MeleeAttack(goal) => Some(goal.clone()),
                _ => None,
            });
        assert_eq!(melee, Some(crate::MeleeAttackGoal::new(1.0, false)));
    }

    #[test]
    fn a_ravager_registers_only_its_own_goals_with_the_vanilla_priorities() {
        let ai = MobAi::new(EntityKind::Ravager, 1, GoalSettings::default());
        assert_eq!(
            goal_summary(&ai, false),
            vec![
                (0, "float"),
                (4, "melee"),
                (5, "stroll"),
                (6, "look"),
                (10, "look"),
            ]
        );
        assert_eq!(
            goal_summary(&ai, true),
            vec![
                (2, "hurt_by"),
                (3, "nearest"),
                (4, "nearest"),
                (4, "nearest"),
            ]
        );
        let melee = ai
            .goals()
            .goals()
            .iter()
            .find_map(|wrapped| match wrapped.goal() {
                MobGoal::MeleeAttack(goal) => Some(goal.clone()),
                _ => None,
            });
        assert_eq!(melee, Some(crate::MeleeAttackGoal::new(1.0, true)));
        let stroll = ai
            .goals()
            .goals()
            .iter()
            .find_map(|wrapped| match wrapped.goal() {
                MobGoal::Stroll(goal) => Some(*goal),
                _ => None,
            });
        assert_eq!(stroll, Some(crate::RandomStrollGoal::water_avoiding(0.4)));
    }

    #[test]
    fn the_ravager_does_not_attack_baby_villagers_but_attacks_adults() {
        let ai = MobAi::new(EntityKind::Ravager, 1, GoalSettings::default());
        let mut villagers = ai
            .targets()
            .goals()
            .iter()
            .filter_map(|wrapped| match wrapped.goal() {
                MobGoal::NearestAttackableTarget(goal) => Some(*goal),
                _ => None,
            })
            .collect::<Vec<_>>();
        let mut villager = villagers.remove(1);
        let seed = seed_where(|random| random.next_int(10) == 0);
        let mut adult = living(5, EntityKind::Villager, Vec3::new(4.5, 64.0, 0.5));
        let mut baby = living(6, EntityKind::Villager, Vec3::new(3.5, 64.0, 0.5));
        baby.baby = true;
        adult.baby = false;
        let world = Mock {
            others: vec![baby, adult],
            ..Mock::default()
        };
        let mut harness = Harness::new(EntityKind::Ravager, Vec3::new(0.5, 64.0, 0.5), world, seed);
        assert!(harness.run(|context| villager.can_use(context)));
        harness.run(|context| villager.start(context));
        assert_eq!(
            harness.brain.target_requests.last().and_then(|r| r.to),
            Some(EntityId(5))
        );
    }

    #[test]
    fn an_illager_never_picks_a_baby_villager_as_an_attack_target() {
        let mob = crate::mock::mob_view(EntityKind::Vindicator, Vec3::new(0.5, 64.0, 0.5));
        let world = Mock::default();
        let mut baby = living(6, EntityKind::Villager, Vec3::new(3.5, 64.0, 0.5));
        baby.baby = true;
        assert!(!crate::targeting::mob_can_attack(&mob, &world, &baby));
        baby.baby = false;
        assert!(crate::targeting::mob_can_attack(&mob, &world, &baby));
        let zombie = crate::mock::mob_view(EntityKind::Zombie, Vec3::new(0.5, 64.0, 0.5));
        baby.baby = true;
        assert!(crate::targeting::mob_can_attack(&zombie, &world, &baby));
    }

    #[test]
    fn a_blind_ravager_sees_nobody_until_the_stun_and_roar_end() {
        let mut harness = raider(EntityKind::Ravager, None);
        let target = living(2, EntityKind::Player, Vec3::new(4.5, 64.0, 0.5));
        let visible = harness.run(|context| {
            context
                .brain
                .sensing
                .has_line_of_sight(context.world, context.mob, &target)
        });
        assert!(visible);
        let mut blind = raider(EntityKind::Ravager, None);
        blind.mob.species.raider.blind = true;
        let visible = blind.run(|context| {
            context
                .brain
                .sensing
                .has_line_of_sight(context.world, context.mob, &target)
        });
        assert!(!visible);
    }

    #[test]
    fn pathfinding_to_the_raid_needs_an_active_unfinished_raid_and_a_spot_outside_the_village() {
        let goal = PathfindToRaidGoal::default();
        let mut harness = raider(EntityKind::Pillager, Some(raid(true, false, false)));
        assert!(harness.run(|context| goal.can_use(context)));
        assert!(harness.run(|context| goal.can_continue_to_use(context)));
        for (raid, label) in [
            (None, "no raid"),
            (Some(raid(false, false, false)), "inactive"),
            (Some(raid(true, true, false)), "over"),
        ] {
            let mut other = raider(EntityKind::Pillager, raid);
            assert!(!other.run(|context| goal.can_use(context)), "{label}");
            assert!(
                !other.run(|context| goal.can_continue_to_use(context)),
                "{label}"
            );
        }
        let mut inside = raider(EntityKind::Pillager, Some(raid(true, false, false)));
        inside
            .world
            .levels
            .insert(rspigot_domain::SectionPos::of(BlockPos::new(0, 64, 0)), 0);
        assert!(!inside.run(|context| goal.can_use(context)));
        assert!(!inside.run(|context| goal.can_continue_to_use(context)));
        let mut targeting = raider(EntityKind::Pillager, Some(raid(true, false, false)));
        targeting.brain.target = Some(EntityId(2));
        targeting
            .world
            .players
            .push(living(2, EntityKind::Player, Vec3::new(3.5, 64.0, 0.5)));
        assert!(!targeting.run(|context| goal.can_use(context)));
    }

    #[test]
    fn pathfinding_to_the_raid_recruits_every_twenty_ticks_and_walks_toward_the_center() {
        let mut goal = PathfindToRaidGoal::default();
        let mut harness = raider(EntityKind::Pillager, Some(raid(true, false, false)));
        harness.mob.tick_count = 5;
        harness.run(|context| goal.tick(context));
        let taken = actions(&mut harness);
        assert!(taken.contains(&MobAction::Raider(RaiderAction::Recruit)));
        assert!(!harness.brain.navigation.is_done());
        harness.mob.tick_count = 24;
        harness.run(|context| goal.tick(context));
        assert!(!actions(&mut harness).contains(&MobAction::Raider(RaiderAction::Recruit)));
        harness.mob.tick_count = 26;
        harness.run(|context| goal.tick(context));
        assert!(actions(&mut harness).contains(&MobAction::Raider(RaiderAction::Recruit)));
    }

    #[test]
    fn an_inactive_raid_neither_recruits_nor_walks() {
        let mut goal = PathfindToRaidGoal::default();
        let mut harness = raider(EntityKind::Pillager, Some(raid(false, false, false)));
        harness.run(|context| goal.tick(context));
        assert!(actions(&mut harness).is_empty());
        assert!(harness.brain.navigation.is_done());
    }

    #[test]
    fn celebrating_starts_after_a_lost_raid_with_nothing_to_attack() {
        let goal = RaiderGoal::Celebration(RaiderCelebrationGoal);
        let mut goal = goal;
        let mut harness = raider(EntityKind::Pillager, Some(raid(true, true, true)));
        assert!(harness.run(|context| Goal::can_use(&mut goal, context)));
        assert!(harness.run(|context| Goal::can_continue_to_use(&mut goal, context)));
        harness.run(|context| Goal::start(&mut goal, context));
        assert_eq!(
            actions(&mut harness),
            vec![MobAction::Raider(RaiderAction::SetCelebrating(true))]
        );
        harness.run(|context| Goal::stop(&mut goal, context));
        assert_eq!(
            actions(&mut harness),
            vec![MobAction::Raider(RaiderAction::SetCelebrating(false))]
        );
        for facts in [
            Some(raid(true, true, false)),
            Some(raid(true, false, false)),
            None,
        ] {
            let mut other = raider(EntityKind::Pillager, facts);
            assert!(!other.run(|context| Goal::can_use(&mut goal, context)));
        }
        let mut busy = raider(EntityKind::Pillager, Some(raid(true, true, true)));
        busy.brain.target = Some(EntityId(2));
        busy.world
            .players
            .push(living(2, EntityKind::Player, Vec3::new(3.5, 64.0, 0.5)));
        assert!(!busy.run(|context| Goal::can_use(&mut goal, context)));
    }

    #[test]
    fn celebrating_makes_noise_and_jumps_on_the_vanilla_draws() {
        let seed = seed_where(|random| random.next_int(50) == 0 && random.next_int(25) == 0);
        let mut harness = Harness::new(
            EntityKind::Pillager,
            Vec3::new(0.5, 64.0, 0.5),
            Mock::default(),
            seed,
        );
        let mut goal = RaiderGoal::Celebration(RaiderCelebrationGoal);
        harness.run(|context| Goal::tick(&mut goal, context));
        assert_eq!(
            actions(&mut harness),
            vec![MobAction::Raider(RaiderAction::CelebrateSound)]
        );
        assert!(harness.brain.jump.is_jumping());
        let mut silent = Harness::new(
            EntityKind::Pillager,
            Vec3::new(0.5, 64.0, 0.5),
            Mock::default(),
            seed,
        );
        silent.mob.silent = true;
        silent.mob.passenger = true;
        silent.run(|context| Goal::tick(&mut goal, context));
        assert!(actions(&mut silent).is_empty());
        assert!(!silent.brain.jump.is_jumping());
    }

    fn home(x: i32) -> rspigot_domain::poi::PoiRecord {
        rspigot_domain::poi::PoiRecord::new(BlockPos::new(x, 64, 0), PoiType::Home)
    }

    #[test]
    fn moving_through_the_village_picks_an_unvisited_home_while_a_raid_is_on() {
        let mut harness = raider(EntityKind::Pillager, Some(raid(true, false, false)));
        harness.world.pois = vec![home(10), home(20)];
        let mut goal = RaiderMoveThroughVillageGoal::new(1.05, 1);
        assert!(harness.run(|context| goal.can_use(context)));
        assert!([BlockPos::new(10, 64, 0), BlockPos::new(20, 64, 0)].contains(&goal.poi_pos));
        goal.visited = vec![BlockPos::new(10, 64, 0), BlockPos::new(20, 64, 0)];
        assert!(!harness.run(|context| goal.can_use(context)));
        goal.visited = vec![BlockPos::new(10, 64, 0)];
        assert!(harness.run(|context| goal.can_use(context)));
        assert_eq!(goal.poi_pos, BlockPos::new(20, 64, 0));
    }

    #[test]
    fn moving_through_the_village_ignores_other_pois_and_far_homes_and_needs_a_live_raid() {
        let mut goal = RaiderMoveThroughVillageGoal::new(1.05, 1);
        let mut harness = raider(EntityKind::Pillager, Some(raid(true, false, false)));
        harness.world.pois = vec![home(100)];
        assert!(!harness.run(|context| goal.can_use(context)));
        harness.world.pois = vec![rspigot_domain::poi::PoiRecord::new(
            BlockPos::new(5, 64, 0),
            PoiType::Meeting,
        )];
        assert!(!harness.run(|context| goal.can_use(context)));
        let mut over = raider(EntityKind::Pillager, Some(raid(true, true, false)));
        over.world.pois = vec![home(5)];
        assert!(!over.run(|context| goal.can_use(context)));
        let mut none = raider(EntityKind::Pillager, None);
        none.world.pois = vec![home(5)];
        let mut mirror = MobRandom::new(SEED);
        assert!(!none.run(|context| goal.can_use(context)));
        assert_eq!(none.brain.random.next_int(1000), mirror.next_int(1000));
    }

    #[test]
    fn the_village_walk_forgets_the_oldest_visit_beyond_three() {
        let mut goal = RaiderMoveThroughVillageGoal::new(1.05, 1);
        goal.visited = (0..4).map(|x| BlockPos::new(x, 64, 0)).collect();
        let mut harness = raider(EntityKind::Pillager, None);
        harness.run(|context| goal.can_use(context));
        assert_eq!(goal.visited().len(), 3);
        assert_eq!(goal.visited()[0], BlockPos::new(1, 64, 0));
    }

    #[test]
    fn the_village_walk_starts_toward_the_home_resets_the_idle_clock_and_remembers_the_visit() {
        let mut harness = raider(EntityKind::Pillager, Some(raid(true, false, false)));
        harness.world.pois = vec![home(10)];
        let mut goal = RaiderMoveThroughVillageGoal::new(1.05, 1);
        assert!(harness.run(|context| goal.can_use(context)));
        harness.run(|context| goal.start(context));
        assert!(!harness.brain.navigation.is_done());
        assert!(
            actions(&mut harness).contains(&MobAction::Raider(RaiderAction::ResetNoActionTime))
        );
        assert!(harness.run(|context| goal.can_continue_to_use(context)));
        goal.stuck = true;
        assert!(!harness.run(|context| goal.can_continue_to_use(context)));
        goal.stuck = false;
        harness.mob.position = Vec3::new(10.5, 64.0, 0.5);
        assert!(!harness.run(|context| goal.can_continue_to_use(context)));
        harness.run(|context| goal.stop(context));
        assert_eq!(goal.visited(), &[BlockPos::new(10, 64, 0)]);
        let mut far = RaiderMoveThroughVillageGoal::new(1.05, 1);
        far.poi_pos = BlockPos::new(50, 64, 0);
        harness.run(|context| far.stop(context));
        assert!(far.visited().is_empty());
    }

    #[test]
    fn the_village_walk_nudges_toward_the_home_once_the_path_ends() {
        let mut harness = raider(EntityKind::Pillager, Some(raid(true, false, false)));
        let mut goal = RaiderMoveThroughVillageGoal::new(1.05, 1);
        goal.poi_pos = BlockPos::new(10, 64, 0);
        harness.run(|context| goal.tick(context));
        assert!(!harness.brain.navigation.is_done() || goal.stuck);
    }

    fn banner_world(x: f64, delayed: bool) -> Mock {
        let mut item = crate::world::DroppedItem {
            id: EntityId(30),
            position: Vec3::new(x, 64.0, 0.5),
            item: rspigot_items::ItemKind::WhiteBanner.id(),
            bounds: rspigot_domain::entity::Aabb::of_size(Vec3::new(x, 64.0, 0.5), 0.25, 0.25),
            has_pickup_delay: delayed,
            alive: true,
        };
        item.alive = true;
        Mock {
            items: vec![item],
            banners: vec![EntityId(30)],
            ..Mock::default()
        }
    }

    fn leader_candidate(world: Mock) -> Harness {
        let mut harness =
            Harness::new(EntityKind::Pillager, Vec3::new(0.5, 64.0, 0.5), world, SEED);
        harness.mob.species.raider.raid = Some(raid(true, false, false));
        harness.mob.species.raider.can_be_leader = true;
        harness
    }

    #[test]
    fn a_leaderless_raider_walks_to_a_dropped_ominous_banner_and_picks_it_up_when_close() {
        let mut harness = leader_candidate(banner_world(6.5, false));
        let mut goal = ObtainRaidLeaderBannerGoal::default();
        assert!(harness.run(|context| goal.can_use(context)));
        assert_eq!(goal.pursued(), Some(EntityId(30)));
        harness.run(|context| goal.start(context));
        assert!(!harness.brain.navigation.is_done());
        assert!(harness.run(|context| goal.can_continue_to_use(context)));
        harness.run(|context| goal.tick(context));
        assert!(actions(&mut harness).is_empty());
        let mut close = leader_candidate(banner_world(1.0, false));
        let mut near = ObtainRaidLeaderBannerGoal::default();
        assert!(close.run(|context| near.can_use(context)));
        close.run(|context| near.start(context));
        close.run(|context| near.tick(context));
        assert_eq!(
            actions(&mut close),
            vec![MobAction::Raider(RaiderAction::PickUpBanner(EntityId(30)))]
        );
        near.stop();
        assert_eq!(near.pursued(), None);
    }

    #[test]
    fn the_banner_goal_skips_items_that_are_not_ominous_banners_or_are_delayed_or_gone() {
        let mut world = banner_world(6.5, false);
        world.banners.clear();
        let mut harness = leader_candidate(world);
        let mut goal = ObtainRaidLeaderBannerGoal::default();
        assert!(!harness.run(|context| goal.can_use(context)));
        let mut harness = leader_candidate(banner_world(6.5, true));
        assert!(!harness.run(|context| goal.can_use(context)));
        let mut gone = banner_world(6.5, false);
        gone.items[0].alive = false;
        let mut harness = leader_candidate(gone);
        assert!(!harness.run(|context| goal.can_use(context)));
    }

    #[test]
    fn the_banner_goal_is_refused_without_a_raid_for_a_non_leader_or_when_a_leader_lives() {
        let mut goal = ObtainRaidLeaderBannerGoal::default();
        type Change = fn(&mut RaiderFacts);
        let cases: [(&str, Change); 6] = [
            ("no raid", |facts| facts.raid = None),
            ("inactive", |facts| {
                facts.raid = Some(raid(false, false, false))
            }),
            ("over", |facts| facts.raid = Some(raid(true, true, false))),
            ("cannot lead", |facts| facts.can_be_leader = false),
            ("wears one", |facts| facts.wearing_banner = true),
            ("leader alive", |facts| facts.leader_alive = true),
        ];
        for (label, change) in cases {
            let mut harness = leader_candidate(banner_world(6.5, false));
            change(&mut harness.mob.species.raider);
            assert!(!harness.run(|context| goal.can_use(context)), "{label}");
        }
    }

    #[test]
    fn an_unreachable_banner_is_not_retried_for_thirty_seconds() {
        let mut world = banner_world(6.5, false);
        world.blocks.insert(
            BlockPos::new(6, 64, 0),
            rspigot_blocks::BlockKind::Bedrock.default_state(),
        );
        for z in -10..=10 {
            for y in 64..70 {
                world.blocks.insert(
                    BlockPos::new(3, y, z),
                    rspigot_blocks::BlockKind::Bedrock.default_state(),
                );
            }
        }
        let mut harness = leader_candidate(world);
        let mut goal = ObtainRaidLeaderBannerGoal::default();
        assert!(!harness.run(|context| goal.can_use(context)));
        assert_eq!(goal.unreachable, vec![(EntityId(30), 100 + 600)]);
    }

    #[test]
    fn the_banner_goal_stops_when_the_item_vanishes_or_the_path_ends() {
        let mut harness = leader_candidate(banner_world(6.5, false));
        let mut goal = ObtainRaidLeaderBannerGoal::default();
        assert!(harness.run(|context| goal.can_use(context)));
        harness.run(|context| goal.start(context));
        harness.world.items.clear();
        assert!(!harness.run(|context| goal.can_continue_to_use(context)));
        let mut harness = leader_candidate(banner_world(6.5, false));
        let mut again = ObtainRaidLeaderBannerGoal::default();
        assert!(!harness.run(|context| again.can_continue_to_use(context)));
    }

    fn healable_world(witch_raider: bool) -> Mock {
        let mut raider = living(
            9,
            if witch_raider {
                EntityKind::Witch
            } else {
                EntityKind::Pillager
            },
            Vec3::new(4.5, 64.0, 0.5),
        );
        raider.alive = true;
        Mock {
            others: vec![raider],
            ..Mock::default()
        }
    }

    #[test]
    fn the_witch_heals_a_raider_of_another_kind_while_a_raid_is_active() {
        let seed = seed_where(|random| random.next_boolean());
        let mut harness = Harness::new(
            EntityKind::Witch,
            Vec3::new(0.5, 64.0, 0.5),
            healable_world(false),
            seed,
        );
        harness.mob.species.raider.raid = Some(raid(true, false, false));
        let mut goal = NearestHealableRaiderTargetGoal::default();
        assert!(harness.run(|context| goal.can_use(context)));
        let mut wrapped = RaiderGoal::HealRaiders(goal);
        harness.run(|context| Goal::start(&mut wrapped, context));
        let RaiderGoal::HealRaiders(started) = wrapped else {
            panic!("a heal goal");
        };
        assert_eq!(started.cooldown(), 100);
        assert_eq!(
            harness
                .brain
                .target_requests
                .last()
                .and_then(|request| request.to),
            Some(EntityId(9))
        );
    }

    #[test]
    fn the_witch_does_not_heal_other_witches_or_without_an_active_raid() {
        let seed = seed_where(|random| random.next_boolean());
        let mut goal = NearestHealableRaiderTargetGoal::default();
        let mut witches = Harness::new(
            EntityKind::Witch,
            Vec3::new(0.5, 64.0, 0.5),
            healable_world(true),
            seed,
        );
        witches.mob.species.raider.raid = Some(raid(true, false, false));
        assert!(!witches.run(|context| goal.can_use(context)));
        let mut quiet = Harness::new(
            EntityKind::Witch,
            Vec3::new(0.5, 64.0, 0.5),
            healable_world(false),
            seed,
        );
        assert!(!quiet.run(|context| goal.can_use(context)));
        quiet.mob.species.raider.raid = Some(raid(false, false, false));
        assert!(!quiet.run(|context| goal.can_use(context)));
    }

    #[test]
    fn a_vindicator_breaks_doors_only_during_a_raid_and_rolls_one_in_five() {
        let mut goal = VindicatorBreakDoorGoal::default();
        let mut quiet = raider(EntityKind::Vindicator, None);
        let mut mirror = MobRandom::new(SEED);
        assert!(!quiet.run(|context| goal.can_use(context)));
        assert_eq!(quiet.brain.random.next_int(1000), mirror.next_int(1000));
        let mut raiding = raider(EntityKind::Vindicator, Some(raid(true, false, false)));
        let mut mirror = MobRandom::new(SEED);
        let roll = mirror.next_int(5);
        let used = raiding.run(|context| goal.can_use(context));
        assert!(!used || roll == 0);
        assert_eq!(raiding.brain.random.next_int(1000), mirror.next_int(1000));
        let mut inactive = raider(EntityKind::Vindicator, Some(raid(false, false, false)));
        assert!(!inactive.run(|context| goal.can_continue_to_use(context)));
        assert_eq!(
            Goal::<AiContext<'_, Mock>>::flags(&RaiderGoal::VindicatorBreakDoor(goal)),
            Flags::of(Flag::Move)
        );
    }

    fn door_scene(raid: Option<RaidFacts>, seed: i64) -> Harness {
        let mut harness = Harness::new(
            EntityKind::Vindicator,
            Vec3::new(0.5, 64.0, 0.5),
            Mock::default(),
            seed,
        );
        harness.mob.species.raider.raid = raid;
        harness.mob.horizontal_collision = true;
        harness.run(|context| context.move_to_pos(Vec3::new(8.5, 64.0, 0.5), 1.0));
        let node = harness.brain.navigation.path().expect("a path").node(0).pos;
        harness.world.blocks.insert(
            node.offset(0, 1, 0),
            rspigot_blocks::BlockKind::OakDoor.default_state(),
        );
        harness
    }

    #[test]
    fn a_vindicator_breaks_a_closed_wooden_door_on_normal_difficulty_during_a_raid() {
        let seed = seed_where(|random| random.next_int(5) == 0);
        let mut harness = door_scene(Some(raid(true, false, false)), seed);
        let mut goal = VindicatorBreakDoorGoal::default();
        assert!(harness.run(|context| goal.can_use(context)));
        harness.run(|context| goal.start(context));
        assert!(
            actions(&mut harness).contains(&MobAction::Raider(RaiderAction::ResetNoActionTime))
        );
        assert!(harness.run(|context| goal.can_continue_to_use(context)));
        let mut zombie_rules = crate::blocks::BreakDoorGoal::default();
        assert!(!harness.run(|context| zombie_rules.can_use(context)));
        let mut vindicator_rules = crate::blocks::BreakDoorGoal::vindicator();
        assert!(harness.run(|context| vindicator_rules.can_use(context)));
    }

    #[test]
    fn the_raider_door_opener_opens_at_the_start_and_closes_when_the_goal_ends() {
        let mut goal = RaiderOpenDoorGoal::default();
        let mut harness = door_scene(Some(raid(true, false, false)), SEED);
        assert!(harness.run(|context| goal.can_use(context)));
        let mut wrapped = RaiderGoal::OpenDoor(goal);
        harness.run(|context| Goal::start(&mut wrapped, context));
        let door = harness
            .brain
            .navigation
            .path()
            .expect("a path")
            .node(0)
            .pos
            .offset(0, 1, 0);
        assert_eq!(
            actions(&mut harness),
            vec![MobAction::SetDoorOpen {
                pos: door,
                open: true,
                interact: false,
            }]
        );
        assert!(!harness.run(|context| Goal::can_continue_to_use(&mut wrapped, context)));
        harness.run(|context| Goal::stop(&mut wrapped, context));
        assert_eq!(
            actions(&mut harness),
            vec![MobAction::SetDoorOpen {
                pos: door,
                open: false,
                interact: false,
            }]
        );
        assert!(Goal::<AiContext<'_, Mock>>::requires_update_every_tick(
            &wrapped
        ));
        assert_eq!(Goal::<AiContext<'_, Mock>>::flags(&wrapped), Flags::NONE);
    }

    #[test]
    fn the_raider_door_opener_needs_an_active_raid() {
        let mut goal = RaiderOpenDoorGoal::default();
        let mut harness = door_scene(None, SEED);
        assert!(!harness.run(|context| goal.can_use(context)));
        let mut inactive = door_scene(Some(raid(false, false, false)), SEED);
        assert!(!inactive.run(|context| goal.can_use(context)));
    }

    #[test]
    fn johnny_attacks_anything_attackable_but_only_for_johnny() {
        let world = Mock {
            others: vec![
                living(8, EntityKind::Cow, Vec3::new(3.5, 64.0, 0.5)),
                living(9, EntityKind::Armadillo, Vec3::new(2.5, 64.0, 0.5)),
            ],
            ..Mock::default()
        };
        let mut harness = Harness::new(
            EntityKind::Vindicator,
            Vec3::new(0.5, 64.0, 0.5),
            world,
            SEED,
        );
        let mut goal = JohnnyAttackGoal::default();
        let mut mirror = MobRandom::new(SEED);
        assert!(!harness.run(|context| goal.can_use(context)));
        assert_eq!(harness.brain.random.next_int(1000), mirror.next_int(1000));
        harness.mob.species.raider.johnny = true;
        assert!(harness.run(|context| goal.can_use(context)));
        let mut wrapped = RaiderGoal::JohnnyAttack(goal);
        harness.run(|context| Goal::start(&mut wrapped, context));
        assert_eq!(
            harness
                .brain
                .target_requests
                .last()
                .and_then(|request| request.to),
            Some(EntityId(9))
        );
        assert!(
            actions(&mut harness).contains(&MobAction::Raider(RaiderAction::ResetNoActionTime))
        );
        assert_eq!(
            Goal::<AiContext<'_, Mock>>::flags(&wrapped),
            Flags::of(Flag::Target)
        );
    }
}
