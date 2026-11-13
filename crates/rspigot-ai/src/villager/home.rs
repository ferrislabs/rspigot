use std::collections::HashMap;

use rspigot_domain::poi::{Occupancy, PoiType};
use rspigot_domain::{BlockPos, EntityId, SectionPos};

use crate::action::MobAction;
use crate::behavior::{
    Activity, Behavior, BehaviorCx, Condition, MemoryModuleType, MemoryStatus, MemoryValue,
    PositionTracker, Trigger, WalkTarget,
};
use crate::random::RandomSource;
use crate::random_pos;
use crate::scan::closer_to_center_than;
use crate::villager::VillagerAction;
use crate::villager::support::tag_contains_block;
use crate::world::AiWorld;

const HOME_SEARCH_RADIUS: i32 = 48;
const HOME_RETRY_DELAY: i64 = 40;
const HOME_MAX_TRIES: i32 = 5;
const HOME_SCAN_INTERVAL: i64 = 20;
const HOME_FAR_DISTANCE_SQR: f64 = 4.0;
const BED_REACH_DISTANCE: f64 = 2.0;
const BED_STAY_DISTANCE: f64 = 1.14;
const BED_STAY_HEIGHT: f64 = 0.4;
const WAKE_GRACE_TICKS: i64 = 100;
const SLEEP_RETRY_DELAY: i64 = 40;
const VILLAGE_STROLL_HORIZONTAL: i32 = 10;
const VILLAGE_STROLL_VERTICAL: i32 = 7;
const NEAR_VILLAGE_SECTIONS: i32 = 2;
const LAND_HORIZONTAL: i32 = 15;
const LAND_VERTICAL: i32 = 7;
const GO_TO_VILLAGE_TRIES: i32 = 5;
const JUMP_MAX_TIME_TO_REACH_BED: i32 = 100;
const JUMP_BASE_COUNT: i32 = 3;
const JUMP_COUNT_BOUND: i32 = 4;
const JUMP_COOLDOWN: i32 = 5;
const BABY_JUMP_TAG: &str = "minecraft:villager_babies_can_jump_on_bed";
const BED_TAG: &str = "minecraft:villagers_can_sleep_on_bed";
const HALF_PI: f64 = std::f64::consts::FRAC_PI_2;
const TAG_WALK_SPEED: f32 = 0.6;
const TAG_RANDOM_BOUND: i32 = 10;
const TAG_ATTEMPTS: i32 = 10;
const TAG_HORIZONTAL: i32 = 20;
const TAG_VERTICAL: i32 = 8;
const TAG_MAX_CHASERS: i32 = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WakeUp;

impl WakeUp {
    pub fn conditions(&self) -> Vec<Condition> {
        Vec::new()
    }
}

impl Trigger for WakeUp {
    fn trigger<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        if !cx.brain.is_active(Activity::Rest) && cx.ai.mob.species.villager.sleeping {
            cx.ai.act(MobAction::Villager(VillagerAction::StopSleeping));
        }
        true
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SleepInBed {
    next_ok_start_time: i64,
}

impl SleepInBed {
    const CONDITION: [Condition; 5] = [
        (MemoryModuleType::Home, MemoryStatus::ValuePresent),
        (MemoryModuleType::LastWoken, MemoryStatus::Registered),
        (MemoryModuleType::LastSlept, MemoryStatus::Registered),
        (MemoryModuleType::WalkTarget, MemoryStatus::Registered),
        (
            MemoryModuleType::CantReachWalkTargetSince,
            MemoryStatus::Registered,
        ),
    ];

    pub fn new() -> Self {
        Self::default()
    }

    fn home<W: AiWorld>(cx: &BehaviorCx<'_, '_, W>) -> Option<BlockPos> {
        cx.brain
            .memories
            .get(MemoryModuleType::Home)
            .and_then(MemoryValue::as_global_pos)
            .map(|global| global.pos)
    }
}

impl Behavior for SleepInBed {
    fn entry_condition(&self) -> &[Condition] {
        &Self::CONDITION
    }

    fn check_extra_start_conditions<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>) -> bool {
        if cx.ai.mob.passenger {
            return false;
        }
        let Some(global) = cx
            .brain
            .memories
            .get(MemoryModuleType::Home)
            .and_then(MemoryValue::as_global_pos)
        else {
            return false;
        };
        if global.dimension != cx.ai.world.dimension() {
            return false;
        }
        let pos = global.pos;
        if let Some(woken) = cx
            .brain
            .memories
            .get(MemoryModuleType::LastWoken)
            .and_then(MemoryValue::as_long)
        {
            let elapsed = cx.ai.world.game_time() - woken;
            if 0 < elapsed && elapsed < WAKE_GRACE_TICKS {
                return false;
            }
        }
        closer_to_center_than(pos, cx.ai.mob.position, BED_REACH_DISTANCE)
            && cx.ai.world.state(pos).is_some_and(|state| {
                state
                    .kind()
                    .is_some_and(|kind| tag_contains_block(kind, BED_TAG))
                    && state.property("occupied") != Some("true")
            })
    }

    fn start<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, now: i64) {
        if self.next_ok_start_time >= now {
            return;
        }
        let Some(pos) = Self::home(cx) else {
            return;
        };
        cx.ai
            .act(MobAction::Villager(VillagerAction::StartSleeping(pos)));
        cx.brain
            .memories
            .set(MemoryModuleType::LastSlept, MemoryValue::Long(now));
        cx.brain.memories.erase(MemoryModuleType::WalkTarget);
        cx.brain
            .memories
            .erase(MemoryModuleType::CantReachWalkTargetSince);
    }

    fn can_still_use<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        let Some(pos) = Self::home(cx) else {
            return false;
        };
        cx.brain.is_active(Activity::Rest)
            && cx.ai.mob.position.y > f64::from(pos.y) + BED_STAY_HEIGHT
            && closer_to_center_than(pos, cx.ai.mob.position, BED_STAY_DISTANCE)
    }

    fn stop<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, now: i64) {
        if cx.ai.mob.species.villager.sleeping {
            cx.ai.act(MobAction::Villager(VillagerAction::StopSleeping));
            self.next_ok_start_time = now + SLEEP_RETRY_DELAY;
        }
    }

    fn timed_out(&self, _end_timestamp: i64, _now: i64) -> bool {
        false
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SetClosestHomeAsWalkTarget {
    speed_modifier: f32,
    last_update: i64,
    batch_cache: HashMap<BlockPos, i64>,
}

impl SetClosestHomeAsWalkTarget {
    pub fn new(speed_modifier: f32) -> Self {
        Self {
            speed_modifier,
            last_update: 0,
            batch_cache: HashMap::new(),
        }
    }

    pub fn conditions(&self) -> Vec<Condition> {
        vec![
            (MemoryModuleType::WalkTarget, MemoryStatus::ValueAbsent),
            (MemoryModuleType::Home, MemoryStatus::ValueAbsent),
        ]
    }
}

impl Trigger for SetClosestHomeAsWalkTarget {
    fn trigger<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        let game_time = cx.ai.world.game_time();
        if game_time - self.last_update < HOME_SCAN_INTERVAL {
            return false;
        }
        let center = cx.ai.mob.block_pos();
        let homes: Vec<BlockPos> = cx
            .ai
            .world
            .pois_in_range(center, HOME_SEARCH_RADIUS, Occupancy::Any)
            .into_iter()
            .filter(|record| record.kind() == PoiType::Home)
            .map(|record| record.pos())
            .collect();
        let closest = homes.iter().copied().min_by(|a, b| {
            crate::villager::support::dist_sqr_pos(*a, center)
                .total_cmp(&crate::villager::support::dist_sqr_pos(*b, center))
        });
        let Some(closest) = closest else {
            return true;
        };
        if crate::villager::support::dist_sqr_pos(closest, center) <= HOME_FAR_DISTANCE_SQR {
            return true;
        }
        let mut tried = 0;
        self.last_update = game_time + i64::from(cx.ai.brain.random.next_int(20));
        let last_update = self.last_update;
        let candidates: Vec<BlockPos> = homes
            .into_iter()
            .filter(|pos| {
                if self.batch_cache.contains_key(pos) {
                    return false;
                }
                tried += 1;
                if tried < HOME_MAX_TRIES {
                    self.batch_cache
                        .insert(*pos, last_update + HOME_RETRY_DELAY);
                }
                true
            })
            .collect();
        let path = cx
            .ai
            .create_path_to_any(&candidates, PoiType::Home.valid_range());
        match path.filter(crate::Path::can_reach) {
            Some(path) => {
                let target = path.target();
                if cx.ai.world.poi_type_at(target).is_some() {
                    cx.brain.memories.set(
                        MemoryModuleType::WalkTarget,
                        MemoryValue::WalkTarget(WalkTarget::block(target, self.speed_modifier, 1)),
                    );
                }
            }
            None => {
                if tried < HOME_MAX_TRIES {
                    self.batch_cache.retain(|_, time| *time >= last_update);
                }
            }
        }
        true
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InsideBrownianWalk {
    speed_modifier: f32,
}

impl InsideBrownianWalk {
    pub fn new(speed_modifier: f32) -> Self {
        Self { speed_modifier }
    }

    pub fn conditions(&self) -> Vec<Condition> {
        vec![(MemoryModuleType::WalkTarget, MemoryStatus::ValueAbsent)]
    }
}

impl Trigger for InsideBrownianWalk {
    fn trigger<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        let origin = cx.ai.mob.block_pos();
        if cx.ai.world.can_see_sky(origin) {
            return false;
        }
        let mut cells = Vec::with_capacity(27);
        for x in -1..=1 {
            for y in -1..=1 {
                for z in -1..=1 {
                    cells.push(origin.offset(x, y, z));
                }
            }
        }
        let mut index = cells.len();
        while index > 1 {
            let swap = cx.ai.brain.random.next_int(index as i32) as usize;
            cells.swap(index - 1, swap);
            index -= 1;
        }
        let found = cells.into_iter().find(|pos| {
            !cx.ai.world.can_see_sky(*pos)
                && cx.ai.world.chunk_loaded(pos.chunk())
                && random_pos::is_stable_destination(cx.ai.world, *pos)
        });
        if let Some(pos) = found {
            cx.brain.memories.set(
                MemoryModuleType::WalkTarget,
                MemoryValue::WalkTarget(WalkTarget::block(pos, self.speed_modifier, 0)),
            );
        }
        true
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GoToClosestVillage {
    speed_modifier: f32,
    close_enough_distance: i32,
}

impl GoToClosestVillage {
    pub fn new(speed_modifier: f32, close_enough_distance: i32) -> Self {
        Self {
            speed_modifier,
            close_enough_distance,
        }
    }

    pub fn conditions(&self) -> Vec<Condition> {
        vec![(MemoryModuleType::WalkTarget, MemoryStatus::ValueAbsent)]
    }
}

impl Trigger for GoToClosestVillage {
    fn trigger<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        let pos = cx.ai.mob.block_pos();
        if cx.ai.world.is_village(pos) {
            return false;
        }
        let current = crate::village::sections_to_village(cx.ai.world, SectionPos::of(pos));
        let mut best = None;
        for _ in 0..GO_TO_VILLAGE_TRIES {
            let found = random_pos::land_pos_scored(
                cx.ai,
                LAND_HORIZONTAL,
                LAND_VERTICAL,
                |context, candidate| {
                    f64::from(-crate::village::sections_to_village(
                        context.world,
                        SectionPos::of(candidate),
                    ))
                },
            );
            if let Some(found) = found {
                let level =
                    crate::village::sections_to_village(cx.ai.world, SectionPos::of(found.block()));
                if level < current {
                    best = Some(found);
                    break;
                }
                if level == current {
                    best = Some(found);
                }
            }
        }
        if let Some(target) = best {
            cx.brain.memories.set(
                MemoryModuleType::WalkTarget,
                MemoryValue::WalkTarget(WalkTarget::position(
                    target,
                    self.speed_modifier,
                    self.close_enough_distance,
                )),
            );
        }
        true
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VillageBoundRandomStroll {
    speed_modifier: f32,
    max_xy_dist: i32,
    max_y_dist: i32,
}

impl VillageBoundRandomStroll {
    pub fn new(speed_modifier: f32) -> Self {
        Self::with_range(
            speed_modifier,
            VILLAGE_STROLL_HORIZONTAL,
            VILLAGE_STROLL_VERTICAL,
        )
    }

    pub fn with_range(speed_modifier: f32, max_xy_dist: i32, max_y_dist: i32) -> Self {
        Self {
            speed_modifier,
            max_xy_dist,
            max_y_dist,
        }
    }

    pub fn conditions(&self) -> Vec<Condition> {
        vec![(MemoryModuleType::WalkTarget, MemoryStatus::ValueAbsent)]
    }
}

impl Trigger for VillageBoundRandomStroll {
    fn trigger<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        let pos = cx.ai.mob.block_pos();
        let target = if cx.ai.world.is_village(pos) {
            random_pos::land_pos(cx.ai, self.max_xy_dist, self.max_y_dist)
        } else {
            let section = SectionPos::of(pos);
            let closest = crate::village::find_section_closest_to_village(
                cx.ai.world,
                section,
                NEAR_VILLAGE_SECTIONS,
            );
            if closest == section {
                random_pos::land_pos(cx.ai, self.max_xy_dist, self.max_y_dist)
            } else {
                random_pos::pos_towards(
                    cx.ai,
                    self.max_xy_dist,
                    self.max_y_dist,
                    closest.center().center(),
                    HALF_PI,
                )
            }
        };
        cx.brain.memories.set_option(
            MemoryModuleType::WalkTarget,
            target.map(|position| {
                MemoryValue::WalkTarget(WalkTarget::position(position, self.speed_modifier, 0))
            }),
        );
        true
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct JumpOnBed {
    speed_modifier: f32,
    target_bed: Option<BlockPos>,
    remaining_time_to_reach_bed: i32,
    remaining_jumps: i32,
    remaining_cooldown: i32,
}

impl JumpOnBed {
    const CONDITION: [Condition; 2] = [
        (MemoryModuleType::NearestBed, MemoryStatus::ValuePresent),
        (MemoryModuleType::WalkTarget, MemoryStatus::ValueAbsent),
    ];

    pub fn new(speed_modifier: f32) -> Self {
        Self {
            speed_modifier,
            target_bed: None,
            remaining_time_to_reach_bed: 0,
            remaining_jumps: 0,
            remaining_cooldown: 0,
        }
    }

    fn is_jumpable<W: AiWorld>(world: &W, pos: BlockPos) -> bool {
        world
            .state(pos)
            .and_then(|state| state.kind())
            .is_some_and(|kind| tag_contains_block(kind, BABY_JUMP_TAG))
    }

    fn on_or_over_bed<W: AiWorld>(cx: &BehaviorCx<'_, '_, W>) -> bool {
        let pos = cx.ai.mob.block_pos();
        Self::is_jumpable(cx.ai.world, pos) || Self::is_jumpable(cx.ai.world, pos.offset(0, -1, 0))
    }

    fn nearest_bed<W: AiWorld>(cx: &BehaviorCx<'_, '_, W>) -> Option<BlockPos> {
        cx.brain
            .memories
            .get(MemoryModuleType::NearestBed)
            .and_then(MemoryValue::as_block)
    }
}

impl Behavior for JumpOnBed {
    fn entry_condition(&self) -> &[Condition] {
        &Self::CONDITION
    }

    fn check_extra_start_conditions<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>) -> bool {
        cx.ai.mob.baby && (Self::on_or_over_bed(cx) || Self::nearest_bed(cx).is_some())
    }

    fn start<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        if let Some(bed) = Self::nearest_bed(cx) {
            self.target_bed = Some(bed);
            self.remaining_time_to_reach_bed = JUMP_MAX_TIME_TO_REACH_BED;
            self.remaining_jumps = JUMP_BASE_COUNT + cx.ai.brain.random.next_int(JUMP_COUNT_BOUND);
            self.remaining_cooldown = 0;
            cx.brain.memories.set(
                MemoryModuleType::WalkTarget,
                MemoryValue::WalkTarget(WalkTarget::block(bed, self.speed_modifier, 0)),
            );
        }
    }

    fn can_still_use<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        let Some(bed) = self.target_bed else {
            return false;
        };
        let on_or_over = Self::on_or_over_bed(cx);
        let tired_of_walking = !on_or_over && self.remaining_time_to_reach_bed <= 0;
        let tired_of_jumping = on_or_over && self.remaining_jumps < 1;
        cx.ai.mob.baby
            && Self::is_jumpable(cx.ai.world, bed)
            && !tired_of_walking
            && !tired_of_jumping
    }

    fn tick<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        if !Self::on_or_over_bed(cx) {
            self.remaining_time_to_reach_bed -= 1;
        }
        if 0 < self.remaining_cooldown {
            self.remaining_cooldown -= 1;
        }
        if Self::is_jumpable(cx.ai.world, cx.ai.mob.block_pos()) {
            cx.ai.brain.jump.jump();
            self.remaining_jumps -= 1;
            self.remaining_cooldown = JUMP_COOLDOWN;
        }
    }

    fn stop<W: AiWorld>(&mut self, _cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        self.target_bed = None;
        self.remaining_time_to_reach_bed = 0;
        self.remaining_jumps = 0;
        self.remaining_cooldown = 0;
    }

    fn timed_out(&self, _end_timestamp: i64, _now: i64) -> bool {
        false
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PlayTagWithOtherKids;

impl PlayTagWithOtherKids {
    pub fn conditions(&self) -> Vec<Condition> {
        vec![
            (
                MemoryModuleType::VisibleVillagerBabies,
                MemoryStatus::ValuePresent,
            ),
            (MemoryModuleType::WalkTarget, MemoryStatus::ValueAbsent),
            (MemoryModuleType::LookTarget, MemoryStatus::Registered),
            (
                MemoryModuleType::InteractionTarget,
                MemoryStatus::Registered,
            ),
        ]
    }

    fn chase_kid<W: AiWorld>(cx: &mut BehaviorCx<'_, '_, W>, kid: EntityId) {
        cx.brain.memories.set(
            MemoryModuleType::InteractionTarget,
            MemoryValue::Entity(kid),
        );
        cx.brain.memories.set(
            MemoryModuleType::LookTarget,
            MemoryValue::Tracker(PositionTracker::of_entity(kid, true)),
        );
        cx.brain.memories.set(
            MemoryModuleType::WalkTarget,
            MemoryValue::WalkTarget(WalkTarget::entity(kid, TAG_WALK_SPEED, 1)),
        );
    }

    fn find_someone_being_chased<W: AiWorld>(
        cx: &BehaviorCx<'_, '_, W>,
        friends: &[EntityId],
    ) -> Option<EntityId> {
        let mut counts: Vec<(EntityId, i32)> = Vec::new();
        for friend in friends {
            let Some(chased) = cx
                .ai
                .world
                .living(*friend)
                .and_then(|living| living.facts.villager.interaction_target)
            else {
                continue;
            };
            match counts.iter_mut().find(|(id, _)| *id == chased) {
                Some((_, count)) => *count += 1,
                None => counts.push((chased, 1)),
            }
        }
        counts.sort_by_key(|(_, count)| *count);
        counts
            .into_iter()
            .find(|(_, count)| 0 < *count && *count < TAG_MAX_CHASERS)
            .map(|(id, _)| id)
    }
}

impl Trigger for PlayTagWithOtherKids {
    fn trigger<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        if cx.ai.brain.random.next_int(TAG_RANDOM_BOUND) != 0 {
            return false;
        }
        let friends: Vec<EntityId> = cx
            .brain
            .memories
            .get(MemoryModuleType::VisibleVillagerBabies)
            .and_then(MemoryValue::as_entities)
            .map(<[EntityId]>::to_vec)
            .unwrap_or_default();
        let me = cx.ai.mob.id;
        let chased_by_someone = friends.iter().any(|friend| {
            cx.ai
                .world
                .living(*friend)
                .is_some_and(|living| living.facts.villager.interaction_target == Some(me))
        });
        if chased_by_someone {
            for _ in 0..TAG_ATTEMPTS {
                if let Some(position) = random_pos::land_pos(cx.ai, TAG_HORIZONTAL, TAG_VERTICAL)
                    && cx.ai.world.is_village(position.block())
                {
                    cx.brain.memories.set(
                        MemoryModuleType::WalkTarget,
                        MemoryValue::WalkTarget(WalkTarget::position(position, TAG_WALK_SPEED, 0)),
                    );
                    return true;
                }
            }
            return true;
        }
        if let Some(chased) = Self::find_someone_being_chased(cx, &friends) {
            Self::chase_kid(cx, chased);
            return true;
        }
        if let Some(first) = friends.first() {
            Self::chase_kid(cx, *first);
        }
        true
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct UpdateActivityFromSchedule;

impl UpdateActivityFromSchedule {
    pub fn conditions(&self) -> Vec<Condition> {
        Vec::new()
    }
}

impl Trigger for UpdateActivityFromSchedule {
    fn trigger<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        let clock = cx.ai.world.clock_time();
        let game_time = cx.ai.world.game_time();
        cx.brain.update_activity_from_schedule(clock, game_time);
        true
    }
}
