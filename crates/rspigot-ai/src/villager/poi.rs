use std::collections::HashMap;

use rspigot_domain::entity::species::villager::Profession;
use rspigot_domain::poi::{Occupancy, PoiType};
use rspigot_domain::{BlockPos, EntityId, Vec3};
use rspigot_entities::EntityKind;

use crate::action::MobAction;
use crate::behavior::{
    Behavior, BehaviorCx, Condition, GlobalPos, MemoryModuleType, MemoryStatus, MemoryValue,
    Trigger, WalkTarget,
};
use crate::random::RandomSource;
use crate::scan::closer_to_center_than;
use crate::villager::VillagerAction;
use crate::villager::support::{set_walk_and_look_targets, tag_contains_block};
use crate::world::AiWorld;

const ACQUIRE_RADIUS: i32 = 48;
const ACQUIRE_BATCH_LIMIT: usize = 5;
const ACQUIRE_FIRST_DELAY: i32 = 20;
const ACQUIRE_INTERVAL: i64 = 20;
const ACQUIRE_JITTER: i32 = 20;
const RETRY_MIN_INCREASE: i32 = 40;
const RETRY_RANDOM_INCREASE: i32 = 40;
const RETRY_MAX_INTERVAL: i32 = 400;
const RETRY_VALIDITY: i64 = 400;
const VALIDATE_DISTANCE: f64 = 16.0;
const PROFESSION_DISTANCE: f64 = 2.0;
const HAPPY_EVENT: i8 = 14;
const BED_TAG: &str = "minecraft:villagers_can_sleep_on_bed";
const POTENTIAL_JOB_TIMEOUT: i32 = 1200;
const WALK_TARGET_CLOSE_ENOUGH: i32 = 1;
const UNREACHABLE_RETRIES: i32 = 1000;
const TOWARDS_HORIZONTAL: i32 = 15;
const TOWARDS_VERTICAL: i32 = 7;
const HALF_PI: f64 = std::f64::consts::FRAC_PI_2;
const STROLL_RETRY_TICKS: i64 = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PoiRule {
    HeldJobSite,
    AcquirableJobSite,
    Home,
    Meeting,
}

impl PoiRule {
    pub fn matches(self, profession: Profession, poi: PoiType) -> bool {
        match self {
            Self::HeldJobSite => profession.holds_job_site(poi),
            Self::AcquirableJobSite => profession.acquires_job_site(poi),
            Self::Home => poi == PoiType::Home,
            Self::Meeting => poi == PoiType::Meeting,
        }
    }
}

pub fn is_free_bed<W: AiWorld>(world: &W, pos: BlockPos) -> bool {
    world.state(pos).is_some_and(|state| {
        state
            .kind()
            .is_some_and(|kind| tag_contains_block(kind, BED_TAG))
            && state.property("occupied") != Some("true")
    })
}

pub fn bed_is_occupied<W: AiWorld>(world: &W, pos: BlockPos) -> bool {
    world.state(pos).is_some_and(|state| {
        state
            .kind()
            .is_some_and(|kind| tag_contains_block(kind, BED_TAG))
            && state.property("occupied") == Some("true")
    })
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct JitteredLinearRetry {
    previous_attempt: i64,
    next_attempt: i64,
    current_delay: i32,
}

impl JitteredLinearRetry {
    fn new(random: &mut impl RandomSource, first_attempt: i64) -> Self {
        let mut retry = Self {
            previous_attempt: first_attempt,
            next_attempt: first_attempt,
            current_delay: 0,
        };
        retry.mark_attempt(random, first_attempt);
        retry
    }

    fn mark_attempt(&mut self, random: &mut impl RandomSource, now: i64) {
        self.previous_attempt = now;
        let step = random.next_int(RETRY_RANDOM_INCREASE) + RETRY_MIN_INCREASE;
        self.current_delay = (self.current_delay + step).min(RETRY_MAX_INTERVAL);
        self.next_attempt = now + i64::from(self.current_delay);
    }

    fn is_still_valid(&self, now: i64) -> bool {
        now - self.previous_attempt < RETRY_VALIDITY
    }

    fn should_retry(&self, now: i64) -> bool {
        self.next_attempt <= now
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AcquirePoi {
    rule: PoiRule,
    memory_to_validate: MemoryModuleType,
    memory_to_acquire: MemoryModuleType,
    only_if_adult: bool,
    acquisition_event: Option<i8>,
    next_scheduled_start: i64,
    batch_cache: HashMap<BlockPos, JitteredLinearRetry>,
}

impl AcquirePoi {
    pub fn new(
        rule: PoiRule,
        memory_to_validate: MemoryModuleType,
        memory_to_acquire: MemoryModuleType,
        only_if_adult: bool,
        acquisition_event: Option<i8>,
    ) -> Self {
        Self {
            rule,
            memory_to_validate,
            memory_to_acquire,
            only_if_adult,
            acquisition_event,
            next_scheduled_start: 0,
            batch_cache: HashMap::new(),
        }
    }

    pub fn conditions(&self) -> Vec<Condition> {
        let mut conditions = vec![(self.memory_to_validate, MemoryStatus::ValueAbsent)];
        if self.memory_to_validate != self.memory_to_acquire {
            conditions.push((self.memory_to_acquire, MemoryStatus::ValueAbsent));
        }
        conditions
    }

    fn is_valid_poi<W: AiWorld>(&self, world: &W, pos: BlockPos) -> bool {
        match self.rule {
            PoiRule::Home => is_free_bed(world, pos),
            _ => true,
        }
    }
}

impl Trigger for AcquirePoi {
    fn trigger<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, now: i64) -> bool {
        if self.only_if_adult && cx.ai.mob.baby {
            return false;
        }
        if self.next_scheduled_start == 0 {
            let delay = i64::from(cx.ai.brain.random.next_int(ACQUIRE_FIRST_DELAY));
            self.next_scheduled_start = cx.ai.world.game_time() + delay;
        }
        if cx.ai.world.game_time() < self.next_scheduled_start {
            return false;
        }
        self.next_scheduled_start =
            now + ACQUIRE_INTERVAL + i64::from(cx.ai.brain.random.next_int(ACQUIRE_JITTER));
        self.batch_cache
            .retain(|_, retry| retry.is_still_valid(now));
        let profession = cx.ai.mob.species.villager.profession;
        let center = cx.ai.mob.block_pos();
        let rule = self.rule;
        let mut candidates: Vec<(BlockPos, rspigot_domain::poi::PoiType)> = Vec::new();
        for record in cx
            .ai
            .world
            .pois_in_range(center, ACQUIRE_RADIUS, Occupancy::HasSpace)
        {
            if !rule.matches(profession, record.kind()) {
                continue;
            }
            let pos = record.pos();
            let can_try = match self.batch_cache.get_mut(&pos) {
                None => true,
                Some(retry) if retry.should_retry(now) => {
                    retry.mark_attempt(&mut cx.ai.brain.random, now);
                    true
                }
                Some(_) => false,
            };
            if can_try {
                candidates.push((pos, record.kind()));
            }
        }
        candidates.sort_by(|a, b| {
            super::support::dist_sqr_pos(a.0, center)
                .total_cmp(&super::support::dist_sqr_pos(b.0, center))
        });
        candidates.truncate(ACQUIRE_BATCH_LIMIT);
        candidates.retain(|(pos, _)| self.is_valid_poi(cx.ai.world, *pos));
        let reach = candidates
            .iter()
            .map(|(_, kind)| kind.valid_range())
            .max()
            .unwrap_or(1);
        let positions: Vec<BlockPos> = candidates.iter().map(|(pos, _)| *pos).collect();
        let path = cx.ai.create_path_to_any(&positions, reach);
        match path.filter(crate::Path::can_reach) {
            Some(path) => {
                let target = path.target();
                if cx.ai.world.poi_type_at(target).is_some() {
                    cx.ai
                        .act(MobAction::Villager(VillagerAction::TakePoi(target)));
                    cx.brain.memories.set(
                        self.memory_to_acquire,
                        MemoryValue::GlobalPos(GlobalPos {
                            dimension: cx.ai.world.dimension().to_owned(),
                            pos: target,
                        }),
                    );
                    if let Some(event) = self.acquisition_event {
                        cx.ai.act(MobAction::EntityEvent(event));
                    }
                    self.batch_cache.clear();
                }
            }
            None => {
                for pos in positions {
                    if let std::collections::hash_map::Entry::Vacant(slot) =
                        self.batch_cache.entry(pos)
                    {
                        slot.insert(JitteredLinearRetry::new(&mut cx.ai.brain.random, now));
                    }
                }
            }
        }
        true
    }
}

pub const HOME_EVENT: i8 = HAPPY_EVENT;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ValidateNearbyPoi {
    rule: PoiRule,
    memory: MemoryModuleType,
}

impl ValidateNearbyPoi {
    pub fn new(rule: PoiRule, memory: MemoryModuleType) -> Self {
        Self { rule, memory }
    }

    pub fn conditions(&self) -> Vec<Condition> {
        vec![(self.memory, MemoryStatus::ValuePresent)]
    }
}

fn villager_in_bed_area<W: AiWorld>(world: &W, pos: BlockPos) -> bool {
    let area = rspigot_domain::entity::Aabb::new(
        f64::from(pos.x),
        f64::from(pos.y),
        f64::from(pos.z),
        f64::from(pos.x + 1),
        f64::from(pos.y + 1),
        f64::from(pos.z + 1),
    );
    world
        .living_of(crate::world::TargetKind::Class("Villager"), &area)
        .iter()
        .any(|villager| villager.facts.villager.sleeping)
}

impl Trigger for ValidateNearbyPoi {
    fn trigger<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        let Some(global) = cx
            .brain
            .memories
            .get(self.memory)
            .and_then(MemoryValue::as_global_pos)
            .cloned()
        else {
            return false;
        };
        if global.dimension != cx.ai.world.dimension()
            || !closer_to_center_than(global.pos, cx.ai.mob.position, VALIDATE_DISTANCE)
        {
            return false;
        }
        let profession = cx.ai.mob.species.villager.profession;
        let exists = cx
            .ai
            .world
            .poi_type_at(global.pos)
            .is_some_and(|kind| self.rule.matches(profession, kind));
        if exists {
            let occupied =
                bed_is_occupied(cx.ai.world, global.pos) && !cx.ai.mob.species.villager.sleeping;
            if occupied {
                cx.brain.memories.erase(self.memory);
                if !villager_in_bed_area(cx.ai.world, global.pos) {
                    cx.ai
                        .act(MobAction::Villager(VillagerAction::ReleasePoi(global.pos)));
                }
            }
        } else {
            cx.brain.memories.erase(self.memory);
        }
        true
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PoiCompetitorScan;

impl PoiCompetitorScan {
    pub fn conditions(&self) -> Vec<Condition> {
        vec![
            (MemoryModuleType::JobSite, MemoryStatus::ValuePresent),
            (
                MemoryModuleType::NearestLivingEntities,
                MemoryStatus::ValuePresent,
            ),
        ]
    }
}

impl Trigger for PoiCompetitorScan {
    fn trigger<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        let Some(job_site) = cx
            .brain
            .memories
            .get(MemoryModuleType::JobSite)
            .and_then(MemoryValue::as_global_pos)
            .cloned()
        else {
            return false;
        };
        let Some(poi_type) = cx.ai.world.poi_type_at(job_site.pos) else {
            return true;
        };
        let nearby: Vec<EntityId> = cx
            .brain
            .memories
            .get(MemoryModuleType::NearestLivingEntities)
            .and_then(MemoryValue::as_entities)
            .map(<[EntityId]>::to_vec)
            .unwrap_or_default();
        let me = cx.ai.mob.id;
        let mut winner_xp = cx.ai.mob.species.villager.xp;
        let mut winner = me;
        for id in nearby {
            let Some(other) = cx.ai.world.living(id) else {
                continue;
            };
            if other.kind != EntityKind::Villager || other.id == me {
                continue;
            }
            if !competes_for_same_job_site(&other, &job_site, poi_type) {
                continue;
            }
            let (new_winner, new_xp, loser) = if winner_xp > other.facts.villager.xp {
                (winner, winner_xp, other.id)
            } else {
                (other.id, other.facts.villager.xp, winner)
            };
            winner = new_winner;
            winner_xp = new_xp;
            if loser == me {
                cx.brain.memories.erase(MemoryModuleType::JobSite);
            } else {
                cx.ai.act(MobAction::SetMemory {
                    mob: loser,
                    memory: MemoryModuleType::JobSite,
                    value: None,
                });
            }
        }
        true
    }
}

fn competes_for_same_job_site(
    other: &crate::world::Living,
    job_site: &GlobalPos,
    poi_type: PoiType,
) -> bool {
    other.facts.villager.job_site == Some(job_site.pos)
        && other.facts.villager.profession.holds_job_site(poi_type)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GoToPotentialJobSite {
    speed_modifier: f32,
}

impl GoToPotentialJobSite {
    const CONDITION: [Condition; 1] = [(
        MemoryModuleType::PotentialJobSite,
        MemoryStatus::ValuePresent,
    )];

    pub fn new(speed_modifier: f32) -> Self {
        Self { speed_modifier }
    }

    pub const TIMEOUT: i32 = POTENTIAL_JOB_TIMEOUT;
}

impl Behavior for GoToPotentialJobSite {
    fn entry_condition(&self) -> &[Condition] {
        &Self::CONDITION
    }

    fn check_extra_start_conditions<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>) -> bool {
        cx.brain.active_non_core_activity().is_none_or(|activity| {
            matches!(
                activity,
                crate::behavior::Activity::Idle
                    | crate::behavior::Activity::Work
                    | crate::behavior::Activity::Play
            )
        })
    }

    fn can_still_use<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        cx.brain
            .memories
            .has_value(MemoryModuleType::PotentialJobSite)
    }

    fn tick<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        let Some(pos) = cx
            .brain
            .memories
            .get(MemoryModuleType::PotentialJobSite)
            .and_then(MemoryValue::as_global_pos)
            .map(|global| global.pos)
        else {
            return;
        };
        set_walk_and_look_targets(cx, pos, self.speed_modifier, WALK_TARGET_CLOSE_ENOUGH);
    }

    fn stop<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        let pos = cx
            .brain
            .memories
            .get(MemoryModuleType::PotentialJobSite)
            .and_then(MemoryValue::as_global_pos)
            .map(|global| global.pos);
        if let Some(pos) = pos
            && cx.ai.world.poi_type_at(pos).is_some()
        {
            cx.ai
                .act(MobAction::Villager(VillagerAction::ReleasePoi(pos)));
        }
        cx.brain.memories.erase(MemoryModuleType::PotentialJobSite);
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct YieldJobSite {
    speed_modifier: f32,
}

impl YieldJobSite {
    pub fn new(speed_modifier: f32) -> Self {
        Self { speed_modifier }
    }

    pub fn conditions(&self) -> Vec<Condition> {
        vec![
            (
                MemoryModuleType::PotentialJobSite,
                MemoryStatus::ValuePresent,
            ),
            (MemoryModuleType::JobSite, MemoryStatus::ValueAbsent),
            (
                MemoryModuleType::NearestLivingEntities,
                MemoryStatus::ValuePresent,
            ),
            (MemoryModuleType::WalkTarget, MemoryStatus::Registered),
            (MemoryModuleType::LookTarget, MemoryStatus::Registered),
        ]
    }
}

impl Trigger for YieldJobSite {
    fn trigger<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        if cx.ai.mob.baby || cx.ai.mob.species.villager.profession != Profession::None {
            return false;
        }
        let Some(pos) = cx
            .brain
            .memories
            .get(MemoryModuleType::PotentialJobSite)
            .and_then(MemoryValue::as_global_pos)
            .map(|global| global.pos)
        else {
            return false;
        };
        let Some(poi_type) = cx.ai.world.poi_type_at(pos) else {
            return false;
        };
        let nearby: Vec<EntityId> = cx
            .brain
            .memories
            .get(MemoryModuleType::NearestLivingEntities)
            .and_then(MemoryValue::as_entities)
            .map(<[EntityId]>::to_vec)
            .unwrap_or_default();
        let me = cx.ai.mob.id;
        let found = nearby
            .into_iter()
            .filter_map(|id| cx.ai.world.living(id))
            .filter(|other| other.kind == EntityKind::Villager && other.id != me)
            .find(|other| {
                let facts = &other.facts.villager;
                if facts.has_potential_job_site {
                    return false;
                }
                if !facts.profession.holds_job_site(poi_type) {
                    return false;
                }
                match facts.job_site {
                    None => cx
                        .ai
                        .world
                        .villager_can_reach(other.id, pos, poi_type.valid_range()),
                    Some(site) => site == pos,
                }
            });
        let Some(other) = found else {
            return true;
        };
        cx.brain.memories.erase(MemoryModuleType::WalkTarget);
        cx.brain.memories.erase(MemoryModuleType::LookTarget);
        cx.brain.memories.erase(MemoryModuleType::PotentialJobSite);
        if other.facts.villager.job_site.is_none() {
            let tracker = crate::behavior::PositionTracker::of_block(pos);
            cx.ai.act(MobAction::SetMemory {
                mob: other.id,
                memory: MemoryModuleType::LookTarget,
                value: Some(MemoryValue::Tracker(tracker)),
            });
            cx.ai.act(MobAction::SetMemory {
                mob: other.id,
                memory: MemoryModuleType::WalkTarget,
                value: Some(MemoryValue::WalkTarget(WalkTarget::block(
                    pos,
                    self.speed_modifier,
                    WALK_TARGET_CLOSE_ENOUGH,
                ))),
            });
            cx.ai.act(MobAction::SetMemory {
                mob: other.id,
                memory: MemoryModuleType::PotentialJobSite,
                value: Some(MemoryValue::GlobalPos(GlobalPos {
                    dimension: cx.ai.world.dimension().to_owned(),
                    pos,
                })),
            });
        }
        true
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AssignProfessionFromJobSite;

impl AssignProfessionFromJobSite {
    pub fn conditions(&self) -> Vec<Condition> {
        vec![
            (
                MemoryModuleType::PotentialJobSite,
                MemoryStatus::ValuePresent,
            ),
            (MemoryModuleType::JobSite, MemoryStatus::Registered),
        ]
    }
}

impl Trigger for AssignProfessionFromJobSite {
    fn trigger<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        let Some(global) = cx
            .brain
            .memories
            .get(MemoryModuleType::PotentialJobSite)
            .and_then(MemoryValue::as_global_pos)
            .cloned()
        else {
            return false;
        };
        if !closer_to_center_than(global.pos, cx.ai.mob.position, PROFESSION_DISTANCE) {
            return false;
        }
        cx.brain.memories.erase(MemoryModuleType::PotentialJobSite);
        let pos = global.pos;
        cx.brain
            .memories
            .set(MemoryModuleType::JobSite, MemoryValue::GlobalPos(global));
        cx.ai.act(MobAction::EntityEvent(HAPPY_EVENT));
        if cx.ai.mob.species.villager.profession == Profession::None
            && let Some(profession) = cx
                .ai
                .world
                .poi_type_at(pos)
                .and_then(Profession::from_job_site)
        {
            cx.ai.act(MobAction::Villager(VillagerAction::SetProfession(
                profession,
            )));
        }
        true
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ResetProfession;

impl ResetProfession {
    pub fn conditions(&self) -> Vec<Condition> {
        vec![(MemoryModuleType::JobSite, MemoryStatus::ValueAbsent)]
    }
}

impl Trigger for ResetProfession {
    fn trigger<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        let facts = cx.ai.mob.species.villager;
        let employed =
            facts.profession != Profession::None && facts.profession != Profession::Nitwit;
        if employed && facts.xp == 0 && facts.level < 2 {
            cx.ai.act(MobAction::Villager(VillagerAction::SetProfession(
                Profession::None,
            )));
        }
        true
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SetWalkTargetFromBlockMemory {
    memory: MemoryModuleType,
    speed_modifier: f32,
    close_enough_dist: i32,
    too_far_distance: i32,
    too_long_unreachable_duration: i32,
}

impl SetWalkTargetFromBlockMemory {
    pub fn new(
        memory: MemoryModuleType,
        speed_modifier: f32,
        close_enough_dist: i32,
        too_far_distance: i32,
        too_long_unreachable_duration: i32,
    ) -> Self {
        Self {
            memory,
            speed_modifier,
            close_enough_dist,
            too_far_distance,
            too_long_unreachable_duration,
        }
    }

    pub fn conditions(&self) -> Vec<Condition> {
        vec![
            (
                MemoryModuleType::CantReachWalkTargetSince,
                MemoryStatus::Registered,
            ),
            (MemoryModuleType::WalkTarget, MemoryStatus::ValueAbsent),
            (self.memory, MemoryStatus::ValuePresent),
        ]
    }

    fn give_up<W: AiWorld>(&self, cx: &mut BehaviorCx<'_, '_, W>, now: i64) {
        super::support::release_poi(cx, self.memory);
        cx.brain.memories.erase(self.memory);
        cx.brain.memories.set(
            MemoryModuleType::CantReachWalkTargetSince,
            MemoryValue::Long(now),
        );
    }
}

impl Trigger for SetWalkTargetFromBlockMemory {
    fn trigger<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, now: i64) -> bool {
        let Some(global) = cx
            .brain
            .memories
            .get(self.memory)
            .and_then(MemoryValue::as_global_pos)
            .cloned()
        else {
            return false;
        };
        let cant_reach_since = cx
            .brain
            .memories
            .get(MemoryModuleType::CantReachWalkTargetSince)
            .and_then(MemoryValue::as_long);
        let gave_up_too_long = cant_reach_since.is_some_and(|since| {
            cx.ai.world.game_time() - since > i64::from(self.too_long_unreachable_duration)
        });
        if global.dimension != cx.ai.world.dimension() || gave_up_too_long {
            self.give_up(cx, now);
            return true;
        }
        let here = cx.ai.mob.block_pos();
        let manhattan = super::support::manhattan(global.pos, here);
        if manhattan > self.too_far_distance {
            let towards = Vec3::new(
                f64::from(global.pos.x) + 0.5,
                f64::from(global.pos.y),
                f64::from(global.pos.z) + 0.5,
            );
            let mut tries = 0;
            while tries < UNREACHABLE_RETRIES {
                if let Some(target) = crate::random_pos::pos_towards(
                    cx.ai,
                    TOWARDS_HORIZONTAL,
                    TOWARDS_VERTICAL,
                    towards,
                    HALF_PI,
                ) && super::support::manhattan(target.block(), here) <= self.too_far_distance
                {
                    cx.brain.memories.set(
                        MemoryModuleType::WalkTarget,
                        MemoryValue::WalkTarget(WalkTarget::position(
                            target,
                            self.speed_modifier,
                            self.close_enough_dist,
                        )),
                    );
                    return true;
                }
                tries += 1;
            }
            self.give_up(cx, now);
        } else if manhattan > self.close_enough_dist {
            cx.brain.memories.set(
                MemoryModuleType::WalkTarget,
                MemoryValue::WalkTarget(WalkTarget::block(
                    global.pos,
                    self.speed_modifier,
                    self.close_enough_dist,
                )),
            );
        }
        true
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StrollToPoiList {
    stroll_to: MemoryModuleType,
    must_be_close_to: MemoryModuleType,
    speed_modifier: f32,
    close_enough_dist: i32,
    max_distance_from_poi: i32,
    next_ok_start_time: i64,
}

impl StrollToPoiList {
    pub fn new(
        stroll_to: MemoryModuleType,
        speed_modifier: f32,
        close_enough_dist: i32,
        max_distance_from_poi: i32,
        must_be_close_to: MemoryModuleType,
    ) -> Self {
        Self {
            stroll_to,
            must_be_close_to,
            speed_modifier,
            close_enough_dist,
            max_distance_from_poi,
            next_ok_start_time: 0,
        }
    }

    pub fn conditions(&self) -> Vec<Condition> {
        vec![
            (MemoryModuleType::WalkTarget, MemoryStatus::Registered),
            (self.stroll_to, MemoryStatus::ValuePresent),
            (self.must_be_close_to, MemoryStatus::ValuePresent),
        ]
    }
}

impl Trigger for StrollToPoiList {
    fn trigger<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, now: i64) -> bool {
        let Some(MemoryValue::GlobalPosList(list)) = cx.brain.memories.get(self.stroll_to) else {
            return false;
        };
        let list = list.clone();
        let Some(must_be_close) = cx
            .brain
            .memories
            .get(self.must_be_close_to)
            .and_then(MemoryValue::as_global_pos)
            .cloned()
        else {
            return false;
        };
        if list.is_empty() {
            return false;
        }
        let index = cx.ai.brain.random.next_int(list.len() as i32) as usize;
        let pos = &list[index];
        if pos.dimension == cx.ai.world.dimension()
            && closer_to_center_than(
                must_be_close.pos,
                cx.ai.mob.position,
                f64::from(self.max_distance_from_poi),
            )
        {
            if now > self.next_ok_start_time {
                cx.brain.memories.set(
                    MemoryModuleType::WalkTarget,
                    MemoryValue::WalkTarget(WalkTarget::block(
                        pos.pos,
                        self.speed_modifier,
                        self.close_enough_dist,
                    )),
                );
                self.next_ok_start_time = now + STROLL_RETRY_TICKS;
            }
            return true;
        }
        false
    }
}
