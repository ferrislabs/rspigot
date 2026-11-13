use rspigot_domain::entity::species::villager::Profession;
use rspigot_domain::poi::{Occupancy, PoiType};
use rspigot_domain::{BlockPos, EntityId, Vec3};
use rspigot_entities::EntityKind;

use crate::action::MobAction;
use crate::behavior::{
    Activity, Behavior, BehaviorCx, Condition, GlobalPos, MemoryModuleType, MemoryStatus,
    MemoryValue, PositionTracker, Trigger, WalkTarget,
};
use crate::random::RandomSource;
use crate::scan::closer_to_center_than;
use crate::villager::VillagerAction;
use crate::villager::support::{dist_sqr_pos, lock_gaze_and_walk_to_each_other, target_is_valid};
use crate::world::AiWorld;

const PANIC_GOLEM_CHECK_INTERVAL: i64 = 100;
const PANIC_GOLEM_NEEDED: i32 = 3;
const CALM_DOWN_HOSTILE_DISTANCE_SQR: f64 = 36.0;
const BELL_RING_CHANCE: f32 = 0.95;
const BELL_RING_DISTANCE: f64 = 3.0;
const BELL_STAY_HIDDEN_TICKS: i64 = 300;
const RAID_CHECK_BOUND: i32 = 20;
const SOCIALIZE_BOUND: i32 = 100;
const SOCIALIZE_BELL_DISTANCE: f64 = 4.0;
const SOCIALIZE_VILLAGER_DISTANCE_SQR: f64 = 32.0;
const SOCIALIZE_WALK_SPEED: f32 = 0.3;
const GAZE_SPEED: f32 = 0.5;
const GAZE_CLOSE_ENOUGH: i32 = 2;
const MAKE_LOVE_DURATION: i32 = 350;
const MAKE_LOVE_DISTANCE_SQR: f64 = 5.0;
const MAKE_LOVE_BASE_TICKS: i64 = 275;
const MAKE_LOVE_BOUND: i32 = 50;
const MAKE_LOVE_PARTICLE_BOUND: i32 = 35;
const BED_SEARCH_RADIUS: i32 = 48;
const TRADE_MAX_DISTANCE_SQR: f64 = 5.0;
const SKY_SPOT_ATTEMPTS: i32 = 10;
const HEART_EVENT: i8 = 12;
const ANGRY_EVENT: i8 = 13;
const LOVE_EVENT: i8 = 18;
const HIDE_SECONDS_TICKS: i32 = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct VillagerPanicTrigger;

impl VillagerPanicTrigger {
    fn is_hurt<W: AiWorld>(cx: &BehaviorCx<'_, '_, W>) -> bool {
        cx.brain.memories.has_value(MemoryModuleType::HurtBy)
    }

    fn has_hostile<W: AiWorld>(cx: &BehaviorCx<'_, '_, W>) -> bool {
        cx.brain
            .memories
            .has_value(MemoryModuleType::NearestHostile)
    }
}

impl Behavior for VillagerPanicTrigger {
    fn entry_condition(&self) -> &[Condition] {
        &[]
    }

    fn can_still_use<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        Self::is_hurt(cx) || Self::has_hostile(cx)
    }

    fn start<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        if !(Self::is_hurt(cx) || Self::has_hostile(cx)) {
            return;
        }
        if !cx.brain.is_active(Activity::Panic) {
            for memory in [
                MemoryModuleType::Path,
                MemoryModuleType::WalkTarget,
                MemoryModuleType::LookTarget,
                MemoryModuleType::BreedTarget,
                MemoryModuleType::InteractionTarget,
            ] {
                cx.brain.memories.erase(memory);
            }
        }
        cx.brain.set_active_activity_if_possible(Activity::Panic);
    }

    fn tick<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, now: i64) {
        if now % PANIC_GOLEM_CHECK_INTERVAL == 0 {
            cx.ai.act(MobAction::Villager(VillagerAction::SpawnGolem {
                timestamp: now,
                villagers_needed: PANIC_GOLEM_NEEDED,
            }));
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct VillagerCalmDown;

impl VillagerCalmDown {
    pub fn conditions(&self) -> Vec<Condition> {
        vec![
            (MemoryModuleType::HurtBy, MemoryStatus::Registered),
            (MemoryModuleType::HurtByEntity, MemoryStatus::Registered),
            (MemoryModuleType::NearestHostile, MemoryStatus::Registered),
        ]
    }
}

impl Trigger for VillagerCalmDown {
    fn trigger<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        let hurt_by = cx.brain.memories.has_value(MemoryModuleType::HurtBy);
        let hostile = cx
            .brain
            .memories
            .has_value(MemoryModuleType::NearestHostile);
        let near_attacker = cx
            .brain
            .memories
            .get(MemoryModuleType::HurtByEntity)
            .and_then(MemoryValue::as_entity)
            .and_then(|id| cx.ai.world.living(id))
            .is_some_and(|entity| {
                entity.position.distance_squared(cx.ai.mob.position)
                    <= CALM_DOWN_HOSTILE_DISTANCE_SQR
            });
        if !(hurt_by || hostile || near_attacker) {
            cx.brain.memories.erase(MemoryModuleType::HurtBy);
            cx.brain.memories.erase(MemoryModuleType::HurtByEntity);
            let clock = cx.ai.world.clock_time();
            let game_time = cx.ai.world.game_time();
            cx.brain.update_activity_from_schedule(clock, game_time);
        }
        true
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ReactToBell;

impl ReactToBell {
    pub fn conditions(&self) -> Vec<Condition> {
        vec![(MemoryModuleType::HeardBellTime, MemoryStatus::ValuePresent)]
    }
}

impl Trigger for ReactToBell {
    fn trigger<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        if cx.ai.world.raid_at(cx.ai.mob.block_pos()).is_none() {
            cx.brain.set_active_activity_if_possible(Activity::Hide);
        }
        true
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SetRaidStatus;

impl SetRaidStatus {
    pub fn conditions(&self) -> Vec<Condition> {
        Vec::new()
    }
}

impl Trigger for SetRaidStatus {
    fn trigger<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        if cx.ai.brain.random.next_int(RAID_CHECK_BOUND) != 0 {
            return false;
        }
        if let Some(raid) = cx.ai.world.raid_at(cx.ai.mob.block_pos()) {
            if raid.first_wave_spawned && !raid.between_waves {
                cx.brain.set_default_activity(Activity::Raid);
                cx.brain.set_active_activity_if_possible(Activity::Raid);
            } else {
                cx.brain.set_default_activity(Activity::PreRaid);
                cx.brain.set_active_activity_if_possible(Activity::PreRaid);
            }
        }
        true
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ResetRaidStatus;

impl ResetRaidStatus {
    pub fn conditions(&self) -> Vec<Condition> {
        Vec::new()
    }
}

impl Trigger for ResetRaidStatus {
    fn trigger<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        if cx.ai.brain.random.next_int(RAID_CHECK_BOUND) != 0 {
            return false;
        }
        let over = cx
            .ai
            .world
            .raid_at(cx.ai.mob.block_pos())
            .is_none_or(|raid| raid.stopped || raid.loss);
        if over {
            cx.brain.set_default_activity(Activity::Idle);
            let clock = cx.ai.world.clock_time();
            let game_time = cx.ai.world.game_time();
            cx.brain.update_activity_from_schedule(clock, game_time);
        }
        true
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RingBell;

impl RingBell {
    pub fn conditions(&self) -> Vec<Condition> {
        vec![(MemoryModuleType::MeetingPoint, MemoryStatus::ValuePresent)]
    }
}

impl Trigger for RingBell {
    fn trigger<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        if cx.ai.brain.random.next_float() <= BELL_RING_CHANCE {
            return true;
        }
        let Some(pos) = cx
            .brain
            .memories
            .get(MemoryModuleType::MeetingPoint)
            .and_then(MemoryValue::as_global_pos)
            .map(|global| global.pos)
        else {
            return false;
        };
        let here = cx.ai.mob.block_pos();
        let near = f64::from(pos.x - here.x).powi(2)
            + f64::from(pos.y - here.y).powi(2)
            + f64::from(pos.z - here.z).powi(2)
            < BELL_RING_DISTANCE * BELL_RING_DISTANCE;
        if near
            && cx
                .ai
                .world
                .state(pos)
                .is_some_and(|state| state.kind() == Some(rspigot_blocks::BlockKind::Bell))
        {
            cx.ai
                .act(MobAction::Villager(VillagerAction::RingBell(pos)));
        }
        true
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MoveToSkySeeingSpot {
    speed_modifier: f32,
}

impl MoveToSkySeeingSpot {
    pub fn new(speed_modifier: f32) -> Self {
        Self { speed_modifier }
    }

    pub fn conditions(&self) -> Vec<Condition> {
        vec![(MemoryModuleType::WalkTarget, MemoryStatus::ValueAbsent)]
    }

    pub fn has_no_blocks_above<W: AiWorld>(world: &W, body_y: f64, target: BlockPos) -> bool {
        world.can_see_sky(target)
            && f64::from(world.motion_blocking_height(target.x, target.z)) <= body_y.ceil()
    }

    fn outdoor_position<W: AiWorld>(&self, cx: &mut BehaviorCx<'_, '_, W>) -> Option<Vec3> {
        let origin = cx.ai.mob.block_pos();
        let mut pos = origin;
        for _ in 0..SKY_SPOT_ATTEMPTS {
            let dx = cx.ai.brain.random.next_int(20) - 10;
            let dy = cx.ai.brain.random.next_int(6) - 3;
            let dz = cx.ai.brain.random.next_int(20) - 10;
            pos = pos.offset(dx, dy, dz);
            if Self::has_no_blocks_above(cx.ai.world, cx.ai.mob.position.y, pos) {
                return Some(pos.center());
            }
        }
        None
    }
}

impl Trigger for MoveToSkySeeingSpot {
    fn trigger<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        if cx.ai.world.can_see_sky(cx.ai.mob.block_pos()) {
            return false;
        }
        if let Some(position) = self.outdoor_position(cx) {
            cx.brain.memories.set(
                MemoryModuleType::WalkTarget,
                MemoryValue::WalkTarget(WalkTarget::position(position, self.speed_modifier, 0)),
            );
        }
        true
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SetHiddenState {
    stay_hidden_ticks: i32,
    close_enough_dist: i32,
    ticks_hidden: i32,
}

impl SetHiddenState {
    pub fn new(seconds: i32, close_enough_dist: i32) -> Self {
        Self {
            stay_hidden_ticks: seconds * HIDE_SECONDS_TICKS,
            close_enough_dist,
            ticks_hidden: 0,
        }
    }

    pub fn conditions(&self) -> Vec<Condition> {
        vec![
            (MemoryModuleType::HidingPlace, MemoryStatus::ValuePresent),
            (MemoryModuleType::HeardBellTime, MemoryStatus::ValuePresent),
        ]
    }
}

impl Trigger for SetHiddenState {
    fn trigger<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, now: i64) -> bool {
        let heard = cx
            .brain
            .memories
            .get(MemoryModuleType::HeardBellTime)
            .and_then(MemoryValue::as_long)
            .unwrap_or(now);
        let bell_over = heard + BELL_STAY_HIDDEN_TICKS <= now;
        if self.stay_hidden_ticks < self.ticks_hidden || bell_over {
            cx.brain.memories.erase(MemoryModuleType::HeardBellTime);
            cx.brain.memories.erase(MemoryModuleType::HidingPlace);
            let clock = cx.ai.world.clock_time();
            let game_time = cx.ai.world.game_time();
            cx.brain.update_activity_from_schedule(clock, game_time);
            self.ticks_hidden = 0;
        }
        let Some(place) = cx
            .brain
            .memories
            .get(MemoryModuleType::HidingPlace)
            .and_then(MemoryValue::as_global_pos)
            .map(|global| global.pos)
        else {
            return true;
        };
        let here = cx.ai.mob.block_pos();
        let limit = f64::from(self.close_enough_dist);
        let distance = f64::from(
            (place.x - here.x).pow(2) + (place.y - here.y).pow(2) + (place.z - here.z).pow(2),
        );
        if distance < limit * limit {
            self.ticks_hidden += 1;
        }
        true
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LocateHidingPlace {
    radius: i32,
    speed_modifier: f32,
    close_enough_dist: i32,
}

impl LocateHidingPlace {
    pub fn new(radius: i32, speed_modifier: f32, close_enough_dist: i32) -> Self {
        Self {
            radius,
            speed_modifier,
            close_enough_dist,
        }
    }

    pub fn conditions(&self) -> Vec<Condition> {
        vec![
            (MemoryModuleType::WalkTarget, MemoryStatus::ValueAbsent),
            (MemoryModuleType::Home, MemoryStatus::Registered),
            (MemoryModuleType::HidingPlace, MemoryStatus::Registered),
            (MemoryModuleType::Path, MemoryStatus::Registered),
            (MemoryModuleType::LookTarget, MemoryStatus::Registered),
            (MemoryModuleType::BreedTarget, MemoryStatus::Registered),
            (
                MemoryModuleType::InteractionTarget,
                MemoryStatus::Registered,
            ),
        ]
    }
}

impl Trigger for LocateHidingPlace {
    fn trigger<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        let center = cx.ai.mob.block_pos();
        let position = cx.ai.mob.position;
        let close = f64::from(self.close_enough_dist);
        let mut homes: Vec<BlockPos> = cx
            .ai
            .world
            .pois_in_range(center, self.radius + 1, Occupancy::Any)
            .into_iter()
            .filter(|record| record.kind() == PoiType::Home)
            .map(|record| record.pos())
            .collect();
        homes.sort_by(|a, b| dist_sqr_pos(*a, center).total_cmp(&dist_sqr_pos(*b, center)));
        let mut chosen = homes
            .first()
            .copied()
            .filter(|pos| closer_to_center_than(*pos, position, close));
        if chosen.is_none() {
            let in_radius: Vec<BlockPos> = cx
                .ai
                .world
                .pois_in_range(center, self.radius, Occupancy::Any)
                .into_iter()
                .filter(|record| record.kind() == PoiType::Home)
                .map(|record| record.pos())
                .collect();
            if !in_radius.is_empty() {
                let index = cx.ai.brain.random.next_int(in_radius.len() as i32) as usize;
                chosen = Some(in_radius[index]);
            }
        }
        if chosen.is_none() {
            chosen = cx
                .brain
                .memories
                .get(MemoryModuleType::Home)
                .and_then(MemoryValue::as_global_pos)
                .map(|global| global.pos);
        }
        if let Some(pos) = chosen {
            for memory in [
                MemoryModuleType::Path,
                MemoryModuleType::LookTarget,
                MemoryModuleType::BreedTarget,
                MemoryModuleType::InteractionTarget,
            ] {
                cx.brain.memories.erase(memory);
            }
            cx.brain.memories.set(
                MemoryModuleType::HidingPlace,
                MemoryValue::GlobalPos(GlobalPos {
                    dimension: cx.ai.world.dimension().to_owned(),
                    pos,
                }),
            );
            if !closer_to_center_than(pos, position, close) {
                cx.brain.memories.set(
                    MemoryModuleType::WalkTarget,
                    MemoryValue::WalkTarget(WalkTarget::block(
                        pos,
                        self.speed_modifier,
                        self.close_enough_dist,
                    )),
                );
            }
        }
        true
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SocializeAtBell;

impl SocializeAtBell {
    pub fn conditions(&self) -> Vec<Condition> {
        vec![
            (MemoryModuleType::WalkTarget, MemoryStatus::Registered),
            (MemoryModuleType::LookTarget, MemoryStatus::Registered),
            (MemoryModuleType::MeetingPoint, MemoryStatus::ValuePresent),
            (
                MemoryModuleType::NearestVisibleLivingEntities,
                MemoryStatus::ValuePresent,
            ),
            (
                MemoryModuleType::InteractionTarget,
                MemoryStatus::ValueAbsent,
            ),
        ]
    }
}

impl Trigger for SocializeAtBell {
    fn trigger<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        if cx.ai.brain.random.next_int(SOCIALIZE_BOUND) != 0 {
            return false;
        }
        let Some(meeting) = cx
            .brain
            .memories
            .get(MemoryModuleType::MeetingPoint)
            .and_then(MemoryValue::as_global_pos)
            .cloned()
        else {
            return false;
        };
        if meeting.dimension != cx.ai.world.dimension()
            || !closer_to_center_than(meeting.pos, cx.ai.mob.position, SOCIALIZE_BELL_DISTANCE)
        {
            return true;
        }
        let body = cx.ai.mob.position;
        let has_villager = cx
            .brain
            .memories
            .get(MemoryModuleType::NearestVisibleLivingEntities)
            .and_then(MemoryValue::as_visible)
            .is_some_and(|visible| {
                visible.contains(cx.ai, &cx.brain.memories, |entity| {
                    entity.kind == EntityKind::Villager
                })
            });
        if !has_villager {
            return true;
        }
        let found = cx
            .brain
            .memories
            .get(MemoryModuleType::NearestVisibleLivingEntities)
            .and_then(MemoryValue::as_visible)
            .and_then(|visible| {
                visible.find_closest(cx.ai, &cx.brain.memories, |entity| {
                    entity.kind == EntityKind::Villager
                        && entity.position.distance_squared(body) <= SOCIALIZE_VILLAGER_DISTANCE_SQR
                })
            });
        if let Some(target) = found {
            cx.brain.memories.set(
                MemoryModuleType::InteractionTarget,
                MemoryValue::Entity(target.id),
            );
            cx.brain.memories.set(
                MemoryModuleType::LookTarget,
                MemoryValue::Tracker(PositionTracker::of_entity(target.id, true)),
            );
            cx.brain.memories.set(
                MemoryModuleType::WalkTarget,
                MemoryValue::WalkTarget(WalkTarget::entity(target.id, SOCIALIZE_WALK_SPEED, 1)),
            );
        }
        true
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LookAndFollowTradingPlayerSink {
    speed_modifier: f32,
}

impl LookAndFollowTradingPlayerSink {
    const CONDITION: [Condition; 2] = [
        (MemoryModuleType::WalkTarget, MemoryStatus::Registered),
        (MemoryModuleType::LookTarget, MemoryStatus::Registered),
    ];

    pub const TIMEOUT: i32 = i32::MAX;

    pub fn new(speed_modifier: f32) -> Self {
        Self { speed_modifier }
    }

    fn valid_player<W: AiWorld>(cx: &BehaviorCx<'_, '_, W>) -> Option<EntityId> {
        let player = cx.ai.mob.species.villager.trading_player?;
        let living = cx.ai.world.living(player)?;
        (!cx.ai.mob.fluid.in_water
            && !cx.ai.mob.species.villager.hurt_recently
            && living.position.distance_squared(cx.ai.mob.position) <= 16.0)
            .then_some(player)
    }

    fn follow_player<W: AiWorld>(&self, cx: &mut BehaviorCx<'_, '_, W>) {
        let Some(player) = cx.ai.mob.species.villager.trading_player else {
            return;
        };
        cx.brain.memories.set(
            MemoryModuleType::WalkTarget,
            MemoryValue::WalkTarget(WalkTarget {
                target: PositionTracker::of_entity(player, false),
                speed_modifier: self.speed_modifier,
                close_enough_dist: 2,
            }),
        );
        cx.brain.memories.set(
            MemoryModuleType::LookTarget,
            MemoryValue::Tracker(PositionTracker::of_entity(player, true)),
        );
    }
}

impl Behavior for LookAndFollowTradingPlayerSink {
    fn entry_condition(&self) -> &[Condition] {
        &Self::CONDITION
    }

    fn check_extra_start_conditions<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>) -> bool {
        Self::valid_player(cx).is_some()
    }

    fn can_still_use<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        Self::valid_player(cx).is_some()
    }

    fn start<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        self.follow_player(cx);
    }

    fn tick<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        self.follow_player(cx);
    }

    fn stop<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        cx.brain.memories.erase(MemoryModuleType::WalkTarget);
        cx.brain.memories.erase(MemoryModuleType::LookTarget);
    }

    fn timed_out(&self, _end_timestamp: i64, _now: i64) -> bool {
        false
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct VillagerMakeLove {
    birth_timestamp: i64,
}

impl VillagerMakeLove {
    const CONDITION: [Condition; 2] = [
        (MemoryModuleType::BreedTarget, MemoryStatus::ValuePresent),
        (
            MemoryModuleType::NearestVisibleLivingEntities,
            MemoryStatus::ValuePresent,
        ),
    ];

    pub const DURATION: i32 = MAKE_LOVE_DURATION;

    pub fn new() -> Self {
        Self::default()
    }

    fn partner<W: AiWorld>(cx: &BehaviorCx<'_, '_, W>) -> Option<crate::world::Living> {
        let id = cx
            .brain
            .memories
            .get(MemoryModuleType::BreedTarget)
            .and_then(MemoryValue::as_entity)?;
        cx.ai.world.living(id)
    }

    fn is_breeding_possible<W: AiWorld>(cx: &mut BehaviorCx<'_, '_, W>) -> bool {
        let Some(partner) = Self::partner(cx) else {
            return false;
        };
        partner.kind == EntityKind::Villager
            && target_is_valid(cx, MemoryModuleType::BreedTarget, EntityKind::Villager)
            && cx.ai.mob.species.villager.can_breed
            && partner.facts.villager.can_breed
    }

    fn take_vacant_bed<W: AiWorld>(cx: &mut BehaviorCx<'_, '_, W>) -> Option<BlockPos> {
        let center = cx.ai.mob.block_pos();
        let mut homes: Vec<BlockPos> = cx
            .ai
            .world
            .pois_in_range(center, BED_SEARCH_RADIUS, Occupancy::HasSpace)
            .into_iter()
            .filter(|record| record.kind() == PoiType::Home)
            .map(|record| record.pos())
            .collect();
        homes.sort_by(|a, b| dist_sqr_pos(*a, center).total_cmp(&dist_sqr_pos(*b, center)));
        let reach = PoiType::Home.valid_range();
        for pos in homes {
            let reachable = cx
                .ai
                .create_path_to_any(&[pos], reach)
                .is_some_and(|path| path.can_reach());
            if reachable {
                cx.ai.act(MobAction::Villager(VillagerAction::TakePoi(pos)));
                return Some(pos);
            }
        }
        None
    }
}

impl Behavior for VillagerMakeLove {
    fn entry_condition(&self) -> &[Condition] {
        &Self::CONDITION
    }

    fn check_extra_start_conditions<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>) -> bool {
        Self::is_breeding_possible(cx)
    }

    fn can_still_use<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, now: i64) -> bool {
        now <= self.birth_timestamp && Self::is_breeding_possible(cx)
    }

    fn start<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, now: i64) {
        let Some(partner) = Self::partner(cx) else {
            return;
        };
        lock_gaze_and_walk_to_each_other(cx, partner.id, GAZE_SPEED, GAZE_CLOSE_ENOUGH);
        cx.ai.act(MobAction::EntityEvent(LOVE_EVENT));
        cx.ai.act(MobAction::Villager(VillagerAction::PartnerEvent {
            partner: partner.id,
            event: LOVE_EVENT,
        }));
        self.birth_timestamp =
            now + i64::from(cx.ai.brain.random.next_int(MAKE_LOVE_BOUND)) + MAKE_LOVE_BASE_TICKS;
    }

    fn tick<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, now: i64) {
        let Some(partner) = Self::partner(cx) else {
            return;
        };
        if cx.ai.mob.position.distance_squared(partner.position) > MAKE_LOVE_DISTANCE_SQR {
            return;
        }
        lock_gaze_and_walk_to_each_other(cx, partner.id, GAZE_SPEED, GAZE_CLOSE_ENOUGH);
        if now < self.birth_timestamp {
            if cx.ai.brain.random.next_int(MAKE_LOVE_PARTICLE_BOUND) == 0 {
                cx.ai.act(MobAction::EntityEvent(HEART_EVENT));
                cx.ai.act(MobAction::Villager(VillagerAction::PartnerEvent {
                    partner: partner.id,
                    event: HEART_EVENT,
                }));
            }
            return;
        }
        cx.ai.act(MobAction::Villager(VillagerAction::EatAndDigest));
        cx.ai
            .act(MobAction::Villager(VillagerAction::EatAndDigestPartner(
                partner.id,
            )));
        match Self::take_vacant_bed(cx) {
            None => {
                cx.ai.act(MobAction::EntityEvent(ANGRY_EVENT));
                cx.ai.act(MobAction::Villager(VillagerAction::PartnerEvent {
                    partner: partner.id,
                    event: ANGRY_EVENT,
                }));
            }
            Some(bed) => {
                cx.ai.act(MobAction::Villager(VillagerAction::Breed {
                    partner: partner.id,
                    bed,
                }));
            }
        }
    }

    fn stop<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        cx.brain.memories.erase(MemoryModuleType::BreedTarget);
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct TradeWithVillager {
    trades: Vec<rspigot_domain::ItemId>,
}

impl TradeWithVillager {
    const CONDITION: [Condition; 2] = [
        (
            MemoryModuleType::InteractionTarget,
            MemoryStatus::ValuePresent,
        ),
        (
            MemoryModuleType::NearestVisibleLivingEntities,
            MemoryStatus::ValuePresent,
        ),
    ];

    pub fn new() -> Self {
        Self::default()
    }

    fn partner<W: AiWorld>(cx: &BehaviorCx<'_, '_, W>) -> Option<crate::world::Living> {
        let id = cx
            .brain
            .memories
            .get(MemoryModuleType::InteractionTarget)
            .and_then(MemoryValue::as_entity)?;
        cx.ai.world.living(id)
    }

    fn throw_half_stack<W: AiWorld>(
        cx: &mut BehaviorCx<'_, '_, W>,
        partner: &crate::world::Living,
        accept: impl Fn(&crate::villager::StackFacts) -> bool,
    ) {
        let inventory = cx.ai.mob.species.villager.inventory;
        for (slot, stack) in inventory.iter().enumerate() {
            let Some(stack) = stack else {
                continue;
            };
            if !accept(stack) {
                continue;
            }
            let count = i32::from(stack.count);
            let amount = if count > stack.max_stack() / 2 {
                count / 2
            } else if count > 24 {
                count - 24
            } else {
                continue;
            };
            cx.ai.act(MobAction::Villager(VillagerAction::ThrowSlot {
                slot,
                amount: amount as u8,
                target: partner.position,
            }));
            break;
        }
    }
}

impl Behavior for TradeWithVillager {
    fn entry_condition(&self) -> &[Condition] {
        &Self::CONDITION
    }

    fn check_extra_start_conditions<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>) -> bool {
        target_is_valid(
            cx,
            MemoryModuleType::InteractionTarget,
            EntityKind::Villager,
        )
    }

    fn can_still_use<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        target_is_valid(
            cx,
            MemoryModuleType::InteractionTarget,
            EntityKind::Villager,
        )
    }

    fn start<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        let Some(partner) = Self::partner(cx) else {
            return;
        };
        lock_gaze_and_walk_to_each_other(cx, partner.id, GAZE_SPEED, GAZE_CLOSE_ENOUGH);
        let mine = cx.ai.mob.species.villager.profession.requested_items();
        self.trades = partner
            .facts
            .villager
            .profession
            .requested_items()
            .iter()
            .filter(|item| !mine.contains(item))
            .map(|item| item.id())
            .collect();
    }

    fn tick<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        let Some(partner) = Self::partner(cx) else {
            return;
        };
        if cx.ai.mob.position.distance_squared(partner.position) > TRADE_MAX_DISTANCE_SQR {
            return;
        }
        lock_gaze_and_walk_to_each_other(cx, partner.id, GAZE_SPEED, GAZE_CLOSE_ENOUGH);
        cx.ai
            .act(MobAction::Villager(VillagerAction::Gossip(partner.id)));
        let facts = cx.ai.mob.species.villager;
        let farmer = facts.profession == Profession::Farmer;
        if facts.has_excess_food() && (farmer || partner.facts.villager.wants_more_food) {
            Self::throw_half_stack(cx, &partner, |stack| stack.nutrition() > 0);
        }
        if farmer && facts.wheat > i32::from(rspigot_items::ItemKind::Wheat.max_stack_size()) / 2 {
            Self::throw_half_stack(cx, &partner, |stack| {
                stack.kind() == Some(rspigot_items::ItemKind::Wheat)
            });
        }
        if !self.trades.is_empty() {
            let wanted = self.trades.clone();
            let has_any = facts
                .inventory
                .iter()
                .flatten()
                .any(|stack| wanted.contains(&stack.item));
            if has_any {
                Self::throw_half_stack(cx, &partner, |stack| wanted.contains(&stack.item));
            }
        }
    }

    fn stop<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        cx.brain.memories.erase(MemoryModuleType::InteractionTarget);
    }
}
