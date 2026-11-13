use rspigot_domain::{BlockPos, Vec3};
use rspigot_entities::EntityKind;

use crate::behavior::control::{BehaviorCx, Trigger};
use crate::behavior::memory::{Condition, MemoryModuleType, MemoryStatus, MemoryValue, WalkTarget};
use crate::behavior::support::{
    EntityFilter, MobPredicate, UniformInt, dist_sqr, get_entity, get_walk_target, look_at_entity,
    set_walk_target,
};
use crate::behavior::tracker::PositionTracker;
use crate::random::RandomSource;
use crate::random_pos;
use crate::scan::closer_to_center_than;
use crate::world::AiWorld;

const AWAY_HORIZONTAL: i32 = 16;
const AWAY_VERTICAL: i32 = 7;
const AWAY_ATTEMPTS: i32 = 10;
const STROLL_HORIZONTAL: i32 = 10;
const STROLL_VERTICAL: i32 = 7;
const FLY_HEIGHT: i32 = -2;
const LOCATION_OFFSET_BOUND: i32 = 3;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SetWalkTargetFromLookTarget {
    speed_modifier: f32,
    close_enough_dist: i32,
}

impl SetWalkTargetFromLookTarget {
    pub fn new(speed_modifier: f32, close_enough_dist: i32) -> Self {
        Self {
            speed_modifier,
            close_enough_dist,
        }
    }

    pub fn conditions(&self) -> Vec<Condition> {
        vec![
            (MemoryModuleType::WalkTarget, MemoryStatus::ValueAbsent),
            (MemoryModuleType::LookTarget, MemoryStatus::ValuePresent),
        ]
    }
}

impl Trigger for SetWalkTargetFromLookTarget {
    fn trigger<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        let Some(look) = cx
            .brain
            .memories
            .get(MemoryModuleType::LookTarget)
            .and_then(MemoryValue::as_tracker)
            .copied()
        else {
            return false;
        };
        set_walk_target(
            cx,
            WalkTarget {
                target: look,
                speed_modifier: self.speed_modifier,
                close_enough_dist: self.close_enough_dist,
            },
        );
        true
    }
}

fn distance_limit(max_dist: f32) -> f64 {
    f64::from(max_dist * max_dist)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SetEntityLookTarget {
    filter: EntityFilter,
    max_dist_sqr: f64,
}

fn look_conditions() -> Vec<Condition> {
    vec![
        (MemoryModuleType::LookTarget, MemoryStatus::ValueAbsent),
        (
            MemoryModuleType::NearestVisibleLivingEntities,
            MemoryStatus::ValuePresent,
        ),
    ]
}

impl SetEntityLookTarget {
    pub fn new(filter: EntityFilter, max_dist: f32) -> Self {
        Self {
            filter,
            max_dist_sqr: distance_limit(max_dist),
        }
    }

    pub fn conditions(&self) -> Vec<Condition> {
        look_conditions()
    }
}

fn closest_in_range<W: AiWorld>(
    cx: &mut BehaviorCx<'_, '_, W>,
    filter: EntityFilter,
    max_dist_sqr: f64,
) -> Option<crate::world::Living> {
    let mob = cx.ai.mob;
    cx.brain
        .memories
        .get(MemoryModuleType::NearestVisibleLivingEntities)
        .and_then(MemoryValue::as_visible)
        .and_then(|visible| {
            visible.find_closest(cx.ai, &cx.brain.memories, |entity| {
                filter.matches(entity) && mob.distance_to_sqr(entity.position) <= max_dist_sqr
            })
        })
}

impl Trigger for SetEntityLookTarget {
    fn trigger<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        let Some(found) = closest_in_range(cx, self.filter, self.max_dist_sqr) else {
            return false;
        };
        look_at_entity(cx, found.id);
        true
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SetEntityLookTargetSometimes {
    filter: EntityFilter,
    max_dist_sqr: f64,
    interval: UniformInt,
    ticks_until_next_start: i32,
}

impl SetEntityLookTargetSometimes {
    pub fn new(filter: EntityFilter, max_dist: f32, interval: UniformInt) -> Self {
        assert!(
            interval.min_inclusive > 1,
            "Interval min value must be greater than 1"
        );
        Self {
            filter,
            max_dist_sqr: distance_limit(max_dist),
            interval,
            ticks_until_next_start: 0,
        }
    }

    pub fn conditions(&self) -> Vec<Condition> {
        look_conditions()
    }

    fn tick_down_and_check<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>) -> bool {
        if self.ticks_until_next_start == 0 {
            self.ticks_until_next_start = self.interval.sample(&mut cx.ai.brain.random) - 1;
            return false;
        }
        self.ticks_until_next_start -= 1;
        self.ticks_until_next_start == 0
    }
}

impl Trigger for SetEntityLookTargetSometimes {
    fn trigger<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        let Some(found) = closest_in_range(cx, self.filter, self.max_dist_sqr) else {
            return false;
        };
        if self.tick_down_and_check(cx) {
            look_at_entity(cx, found.id);
            return true;
        }
        false
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RandomStroll {
    speed_modifier: f32,
    horizontal: i32,
    vertical: i32,
    may_stroll_from_water: bool,
    flying: bool,
}

impl RandomStroll {
    pub fn fly(speed_modifier: f32) -> Self {
        Self {
            flying: true,
            ..Self::stroll(speed_modifier)
        }
    }

    pub fn stroll(speed_modifier: f32) -> Self {
        Self::stroll_from_water(speed_modifier, true)
    }

    pub fn stroll_from_water(speed_modifier: f32, may_stroll_from_water: bool) -> Self {
        Self {
            speed_modifier,
            horizontal: STROLL_HORIZONTAL,
            vertical: STROLL_VERTICAL,
            may_stroll_from_water,
            flying: false,
        }
    }

    pub fn stroll_within(speed_modifier: f32, horizontal: i32, vertical: i32) -> Self {
        Self {
            speed_modifier,
            horizontal,
            vertical,
            may_stroll_from_water: true,
            flying: false,
        }
    }

    pub fn conditions(&self) -> Vec<Condition> {
        vec![(MemoryModuleType::WalkTarget, MemoryStatus::ValueAbsent)]
    }
}

impl Trigger for RandomStroll {
    fn trigger<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        if !self.may_stroll_from_water && cx.ai.mob.fluid.in_water {
            return false;
        }
        let found = if self.flying {
            let view = rspigot_domain::mth::view_vector(cx.ai.mob.pitch, cx.ai.mob.head_yaw);
            random_pos::air_and_water_pos(
                cx.ai,
                self.horizontal,
                self.vertical,
                FLY_HEIGHT,
                (view.x, view.z),
                random_pos::HALF_PI,
            )
        } else {
            random_pos::land_pos(cx.ai, self.horizontal, self.vertical)
        };
        let target = found.map(|position| WalkTarget::position(position, self.speed_modifier, 0));
        cx.brain.memories.set_option(
            MemoryModuleType::WalkTarget,
            target.map(MemoryValue::WalkTarget),
        );
        true
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BabyFollowAdult {
    follow_range: UniformInt,
    speed_modifier: f32,
    target_eye: bool,
    adult: MemoryModuleType,
}

impl BabyFollowAdult {
    pub fn new(follow_range: UniformInt, speed_modifier: f32) -> Self {
        Self::of(
            follow_range,
            speed_modifier,
            MemoryModuleType::NearestVisibleAdult,
            false,
        )
    }

    pub fn of(
        follow_range: UniformInt,
        speed_modifier: f32,
        adult: MemoryModuleType,
        target_eye: bool,
    ) -> Self {
        Self {
            follow_range,
            speed_modifier,
            target_eye,
            adult,
        }
    }

    pub fn conditions(&self) -> Vec<Condition> {
        vec![
            (MemoryModuleType::LookTarget, MemoryStatus::Registered),
            (MemoryModuleType::WalkTarget, MemoryStatus::ValueAbsent),
            (self.adult, MemoryStatus::ValuePresent),
        ]
    }
}

fn closer_than(a: Vec3, b: Vec3, distance: i32) -> bool {
    let distance = f64::from(distance);
    a.distance_squared(b) < distance * distance
}

impl Trigger for BabyFollowAdult {
    fn trigger<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        if !cx.ai.mob.baby {
            return false;
        }
        let Some(adult) = get_entity(cx, self.adult).and_then(|id| cx.ai.world.living(id)) else {
            return false;
        };
        let position = cx.ai.mob.position;
        if !closer_than(position, adult.position, self.follow_range.max_inclusive)
            || closer_than(position, adult.position, self.follow_range.min_inclusive)
        {
            return false;
        }
        let look = if self.target_eye {
            PositionTracker::Entity {
                id: adult.id,
                track_eye_height: true,
                target_eye_height: true,
            }
        } else {
            PositionTracker::of_entity(adult.id, true)
        };
        cx.brain
            .memories
            .set(MemoryModuleType::LookTarget, MemoryValue::Tracker(look));
        set_walk_target(
            cx,
            WalkTarget {
                target: PositionTracker::of_entity(adult.id, false),
                speed_modifier: self.speed_modifier,
                close_enough_dist: self.follow_range.min_inclusive - 1,
            },
        );
        true
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AwayFrom {
    Entity,
    Block,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SetWalkTargetAwayFrom {
    memory: MemoryModuleType,
    source: AwayFrom,
    speed_modifier: f32,
    desired_distance: i32,
    interrupt_current_walk: bool,
}

impl SetWalkTargetAwayFrom {
    pub fn pos(
        memory: MemoryModuleType,
        speed_modifier: f32,
        desired_distance: i32,
        interrupt_current_walk: bool,
    ) -> Self {
        Self {
            memory,
            source: AwayFrom::Block,
            speed_modifier,
            desired_distance,
            interrupt_current_walk,
        }
    }

    pub fn entity(
        memory: MemoryModuleType,
        speed_modifier: f32,
        desired_distance: i32,
        interrupt_current_walk: bool,
    ) -> Self {
        Self {
            memory,
            source: AwayFrom::Entity,
            speed_modifier,
            desired_distance,
            interrupt_current_walk,
        }
    }

    pub fn conditions(&self) -> Vec<Condition> {
        vec![
            (MemoryModuleType::WalkTarget, MemoryStatus::Registered),
            (self.memory, MemoryStatus::ValuePresent),
        ]
    }

    fn away_position<W: AiWorld>(&self, cx: &BehaviorCx<'_, '_, W>) -> Option<Vec3> {
        let value = cx.brain.memories.get(self.memory)?;
        match self.source {
            AwayFrom::Entity => cx
                .ai
                .world
                .living(value.as_entity()?)
                .map(|living| living.position),
            AwayFrom::Block => value.as_block().map(|pos| {
                Vec3::new(
                    f64::from(pos.x) + 0.5,
                    f64::from(pos.y),
                    f64::from(pos.z) + 0.5,
                )
            }),
        }
    }
}

impl Trigger for SetWalkTargetAwayFrom {
    fn trigger<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        let existing = get_walk_target(cx);
        if existing.is_some() && !self.interrupt_current_walk {
            return false;
        }
        let position = cx.ai.mob.position;
        let Some(away) = self.away_position(cx) else {
            return false;
        };
        if !closer_than(position, away, self.desired_distance) {
            return false;
        }
        if let Some(existing) = existing
            && existing.speed_modifier == self.speed_modifier
            && let Some(current) = existing.target.current_position(cx.ai.world)
        {
            let toward_walk = current - position;
            let toward_away = away - position;
            let dot = toward_walk.x * toward_away.x
                + toward_walk.y * toward_away.y
                + toward_walk.z * toward_away.z;
            if dot < 0.0 {
                return false;
            }
        }
        for _ in 0..AWAY_ATTEMPTS {
            if let Some(target) =
                random_pos::land_pos_away(cx.ai, AWAY_HORIZONTAL, AWAY_VERTICAL, away)
            {
                set_walk_target(cx, WalkTarget::position(target, self.speed_modifier, 0));
                break;
            }
        }
        true
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct GoToWantedItem {
    predicate: MobPredicate,
    speed_modifier: f32,
    interrupt_ongoing_walk: bool,
    max_dist_to_walk: i32,
}

impl GoToWantedItem {
    pub fn new(speed_modifier: f32, interrupt_ongoing_walk: bool, max_dist_to_walk: i32) -> Self {
        Self::when(
            MobPredicate::Always,
            speed_modifier,
            interrupt_ongoing_walk,
            max_dist_to_walk,
        )
    }

    pub fn when(
        predicate: MobPredicate,
        speed_modifier: f32,
        interrupt_ongoing_walk: bool,
        max_dist_to_walk: i32,
    ) -> Self {
        Self {
            predicate,
            speed_modifier,
            interrupt_ongoing_walk,
            max_dist_to_walk,
        }
    }

    pub fn conditions(&self) -> Vec<Condition> {
        vec![
            (MemoryModuleType::LookTarget, MemoryStatus::Registered),
            (
                MemoryModuleType::WalkTarget,
                if self.interrupt_ongoing_walk {
                    MemoryStatus::Registered
                } else {
                    MemoryStatus::ValueAbsent
                },
            ),
            (
                MemoryModuleType::NearestVisibleWantedItem,
                MemoryStatus::ValuePresent,
            ),
            (
                MemoryModuleType::ItemPickupCooldownTicks,
                MemoryStatus::Registered,
            ),
        ]
    }
}

impl Trigger for GoToWantedItem {
    fn trigger<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        if cx
            .brain
            .memories
            .has_value(MemoryModuleType::ItemPickupCooldownTicks)
            || !self.predicate.test(cx)
        {
            return false;
        }
        let Some(item) = get_entity(cx, MemoryModuleType::NearestVisibleWantedItem)
            .and_then(|id| cx.ai.world.dropped_item(id))
        else {
            return false;
        };
        if !closer_than(cx.ai.mob.position, item.position, self.max_dist_to_walk)
            || !cx.ai.world.within_world_border(item.position.block())
            || !cx.ai.mob.can_pick_up_loot
        {
            return false;
        }
        look_at_entity(cx, item.id);
        set_walk_target(
            cx,
            WalkTarget {
                target: PositionTracker::of_entity(item.id, false),
                speed_modifier: self.speed_modifier,
                close_enough_dist: 0,
            },
        );
        true
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GoToTargetLocation {
    location: MemoryModuleType,
    close_enough_dist: i32,
    speed_modifier: f32,
}

impl GoToTargetLocation {
    pub fn new(location: MemoryModuleType, close_enough_dist: i32, speed_modifier: f32) -> Self {
        Self {
            location,
            close_enough_dist,
            speed_modifier,
        }
    }

    pub fn conditions(&self) -> Vec<Condition> {
        vec![
            (self.location, MemoryStatus::ValuePresent),
            (MemoryModuleType::AttackTarget, MemoryStatus::ValueAbsent),
            (MemoryModuleType::WalkTarget, MemoryStatus::ValueAbsent),
            (MemoryModuleType::LookTarget, MemoryStatus::Registered),
        ]
    }

    fn nearby_pos<W: AiWorld>(cx: &mut BehaviorCx<'_, '_, W>, pos: BlockPos) -> BlockPos {
        let x = cx.ai.brain.random.next_int(LOCATION_OFFSET_BOUND) - 1;
        let z = cx.ai.brain.random.next_int(LOCATION_OFFSET_BOUND) - 1;
        pos.offset(x, 0, z)
    }
}

impl Trigger for GoToTargetLocation {
    fn trigger<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        let Some(location) = cx
            .brain
            .memories
            .get(self.location)
            .and_then(MemoryValue::as_block)
        else {
            return false;
        };
        let reach = f64::from(self.close_enough_dist);
        if dist_sqr(location, cx.ai.mob.block_pos()) >= reach * reach {
            let target = Self::nearby_pos(cx, location);
            let tracker = PositionTracker::of_block(target);
            cx.brain
                .memories
                .set(MemoryModuleType::LookTarget, MemoryValue::Tracker(tracker));
            set_walk_target(
                cx,
                WalkTarget {
                    target: tracker,
                    speed_modifier: self.speed_modifier,
                    close_enough_dist: self.close_enough_dist,
                },
            );
        }
        true
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SetLookAndInteract {
    kind: EntityKind,
    interaction_range_sqr: i32,
}

impl SetLookAndInteract {
    pub fn new(kind: EntityKind, interaction_range: i32) -> Self {
        Self {
            kind,
            interaction_range_sqr: interaction_range * interaction_range,
        }
    }

    pub fn conditions(&self) -> Vec<Condition> {
        vec![
            (MemoryModuleType::LookTarget, MemoryStatus::Registered),
            (
                MemoryModuleType::InteractionTarget,
                MemoryStatus::ValueAbsent,
            ),
            (
                MemoryModuleType::NearestVisibleLivingEntities,
                MemoryStatus::ValuePresent,
            ),
        ]
    }
}

impl Trigger for SetLookAndInteract {
    fn trigger<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        let limit = f64::from(self.interaction_range_sqr);
        let Some(target) = closest_in_range(cx, EntityFilter::Kind(self.kind), limit) else {
            return false;
        };
        cx.brain.memories.set(
            MemoryModuleType::InteractionTarget,
            MemoryValue::Entity(target.id),
        );
        look_at_entity(cx, target.id);
        true
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InteractWith {
    filter: EntityFilter,
    requires_breedable_self: bool,
    interaction_range_sqr: i32,
    interaction_target: MemoryModuleType,
    speed_modifier: f32,
    stop_distance: i32,
}

impl InteractWith {
    pub fn new(
        kind: EntityKind,
        interaction_range: i32,
        interaction_target: MemoryModuleType,
        speed_modifier: f32,
        stop_distance: i32,
    ) -> Self {
        Self {
            filter: EntityFilter::Kind(kind),
            requires_breedable_self: false,
            interaction_range_sqr: interaction_range * interaction_range,
            interaction_target,
            speed_modifier,
            stop_distance,
        }
    }

    pub fn breeding_villagers(
        interaction_range: i32,
        interaction_target: MemoryModuleType,
        speed_modifier: f32,
        stop_distance: i32,
    ) -> Self {
        Self {
            filter: EntityFilter::BreedableVillager,
            requires_breedable_self: true,
            interaction_range_sqr: interaction_range * interaction_range,
            interaction_target,
            speed_modifier,
            stop_distance,
        }
    }

    pub fn conditions(&self) -> Vec<Condition> {
        vec![
            (self.interaction_target, MemoryStatus::Registered),
            (MemoryModuleType::LookTarget, MemoryStatus::Registered),
            (MemoryModuleType::WalkTarget, MemoryStatus::ValueAbsent),
            (
                MemoryModuleType::NearestVisibleLivingEntities,
                MemoryStatus::ValuePresent,
            ),
        ]
    }
}

impl Trigger for InteractWith {
    fn trigger<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        if self.requires_breedable_self && !cx.ai.mob.species.villager.can_breed {
            return false;
        }
        let filter = self.filter;
        let any_valid = cx
            .brain
            .memories
            .get(MemoryModuleType::NearestVisibleLivingEntities)
            .and_then(MemoryValue::as_visible)
            .is_some_and(|visible| {
                visible.contains(cx.ai, &cx.brain.memories, |entity| filter.matches(entity))
            });
        if !any_valid {
            return false;
        }
        let limit = f64::from(self.interaction_range_sqr);
        if let Some(target) = closest_in_range(cx, filter, limit) {
            cx.brain
                .memories
                .set(self.interaction_target, MemoryValue::Entity(target.id));
            look_at_entity(cx, target.id);
            set_walk_target(
                cx,
                WalkTarget {
                    target: PositionTracker::of_entity(target.id, false),
                    speed_modifier: self.speed_modifier,
                    close_enough_dist: self.stop_distance,
                },
            );
        }
        true
    }
}

const POI_RETRY_TICKS: i64 = 80;
const POI_AROUND_RETRY_TICKS: i64 = 180;
const POI_STROLL_HORIZONTAL: i32 = 8;
const POI_STROLL_VERTICAL: i32 = 6;

fn near_poi<W: AiWorld>(
    cx: &BehaviorCx<'_, '_, W>,
    memory: MemoryModuleType,
    max_distance_from_poi: i32,
) -> Option<BlockPos> {
    let global = cx.brain.memories.get(memory)?.as_global_pos()?;
    (global.dimension == cx.ai.world.dimension()
        && closer_to_center_than(
            global.pos,
            cx.ai.mob.position,
            f64::from(max_distance_from_poi),
        ))
    .then_some(global.pos)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StrollToPoi {
    memory: MemoryModuleType,
    speed_modifier: f32,
    close_enough_dist: i32,
    max_distance_from_poi: i32,
    next_ok_start_time: i64,
}

impl StrollToPoi {
    pub fn new(
        memory: MemoryModuleType,
        speed_modifier: f32,
        close_enough_dist: i32,
        max_distance_from_poi: i32,
    ) -> Self {
        Self {
            memory,
            speed_modifier,
            close_enough_dist,
            max_distance_from_poi,
            next_ok_start_time: 0,
        }
    }

    pub fn conditions(&self) -> Vec<Condition> {
        vec![
            (MemoryModuleType::WalkTarget, MemoryStatus::Registered),
            (self.memory, MemoryStatus::ValuePresent),
        ]
    }
}

impl Trigger for StrollToPoi {
    fn trigger<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, now: i64) -> bool {
        let Some(pos) = near_poi(cx, self.memory, self.max_distance_from_poi) else {
            return false;
        };
        if now > self.next_ok_start_time {
            set_walk_target(
                cx,
                WalkTarget::block(pos, self.speed_modifier, self.close_enough_dist),
            );
            self.next_ok_start_time = now + POI_RETRY_TICKS;
        }
        true
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StrollAroundPoi {
    memory: MemoryModuleType,
    speed_modifier: f32,
    max_distance_from_poi: i32,
    next_ok_start_time: i64,
}

impl StrollAroundPoi {
    pub fn new(memory: MemoryModuleType, speed_modifier: f32, max_distance_from_poi: i32) -> Self {
        Self {
            memory,
            speed_modifier,
            max_distance_from_poi,
            next_ok_start_time: 0,
        }
    }

    pub fn conditions(&self) -> Vec<Condition> {
        vec![
            (MemoryModuleType::WalkTarget, MemoryStatus::Registered),
            (self.memory, MemoryStatus::ValuePresent),
        ]
    }
}

impl Trigger for StrollAroundPoi {
    fn trigger<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, now: i64) -> bool {
        if near_poi(cx, self.memory, self.max_distance_from_poi).is_none() {
            return false;
        }
        if now > self.next_ok_start_time {
            let target = random_pos::land_pos(cx.ai, POI_STROLL_HORIZONTAL, POI_STROLL_VERTICAL)
                .map(|position| WalkTarget::position(position, self.speed_modifier, 1));
            cx.brain.memories.set_option(
                MemoryModuleType::WalkTarget,
                target.map(MemoryValue::WalkTarget),
            );
            self.next_ok_start_time = now + POI_AROUND_RETRY_TICKS;
        }
        true
    }
}
