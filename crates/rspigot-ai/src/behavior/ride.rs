use crate::action::{MobAction, RideAction};
use crate::behavior::control::{BehaviorCx, Trigger};
use crate::behavior::memory::{Condition, MemoryModuleType, MemoryStatus, MemoryValue, WalkTarget};
use crate::behavior::support::{get_entity, set_walk_target};
use crate::behavior::tracker::PositionTracker;
use crate::world::{AiWorld, Living};

const MOUNT_DISTANCE: f64 = 1.0;
const MOUNT_CLOSE_ENOUGH: i32 = 1;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mount {
    speed_modifier: f32,
}

impl Mount {
    pub fn new(speed_modifier: f32) -> Self {
        Self { speed_modifier }
    }

    pub fn conditions(&self) -> Vec<Condition> {
        vec![
            (MemoryModuleType::LookTarget, MemoryStatus::Registered),
            (MemoryModuleType::WalkTarget, MemoryStatus::ValueAbsent),
            (MemoryModuleType::RideTarget, MemoryStatus::ValuePresent),
        ]
    }
}

impl Trigger for Mount {
    fn trigger<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        if cx.ai.mob.passenger {
            return false;
        }
        let Some(target) =
            get_entity(cx, MemoryModuleType::RideTarget).and_then(|id| cx.ai.world.living(id))
        else {
            return false;
        };
        if cx.ai.mob.distance_to_sqr(target.position) < MOUNT_DISTANCE * MOUNT_DISTANCE {
            cx.ai.act(MobAction::Ride(RideAction::Mount(target.id)));
        } else {
            cx.brain.memories.set(
                MemoryModuleType::LookTarget,
                MemoryValue::Tracker(PositionTracker::of_entity(target.id, true)),
            );
            set_walk_target(
                cx,
                WalkTarget {
                    target: PositionTracker::of_entity(target.id, false),
                    speed_modifier: self.speed_modifier,
                    close_enough_dist: MOUNT_CLOSE_ENOUGH,
                },
            );
        }
        true
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DontRideIf {
    PiglinWantsToStopRiding,
}

impl DontRideIf {
    fn test<W: AiWorld>(self, cx: &BehaviorCx<'_, '_, W>, vehicle: &Living) -> bool {
        match self {
            Self::PiglinWantsToStopRiding => crate::piglin::wants_to_stop_riding(cx, vehicle),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DismountOrSkipMounting {
    max_walk_dist_to_ride_target: i32,
    dont_ride_if: DontRideIf,
}

impl DismountOrSkipMounting {
    pub fn new(max_walk_dist_to_ride_target: i32, dont_ride_if: DontRideIf) -> Self {
        Self {
            max_walk_dist_to_ride_target,
            dont_ride_if,
        }
    }

    pub fn conditions(&self) -> Vec<Condition> {
        vec![(MemoryModuleType::RideTarget, MemoryStatus::Registered)]
    }
}

impl Trigger for DismountOrSkipMounting {
    fn trigger<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        let vehicle = cx.ai.mob.vehicle;
        let ride_target = get_entity(cx, MemoryModuleType::RideTarget);
        let Some(entity) = vehicle.or(ride_target) else {
            return false;
        };
        let living = cx.ai.world.living(entity);
        let max_distance = f64::from(self.max_walk_dist_to_ride_target);
        let valid = living.as_ref().is_some_and(|vehicle| {
            vehicle.alive
                && cx.ai.mob.distance_to_sqr(vehicle.position) < max_distance * max_distance
        });
        if valid
            && let Some(vehicle) = &living
            && !self.dont_ride_if.test(cx, vehicle)
        {
            return false;
        }
        cx.ai.act(MobAction::Ride(RideAction::Dismount));
        cx.brain.memories.erase(MemoryModuleType::RideTarget);
        true
    }
}
