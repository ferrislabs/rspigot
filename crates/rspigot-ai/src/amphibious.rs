use rspigot_blocks::{BlockKind, StateFlags, StateId};
use rspigot_domain::BlockPos;

use crate::action::MobAction;
use crate::behavior::{
    BehaviorCx, Condition, MemoryModuleType, MemoryStatus, MemoryValue, PositionTracker, Trigger,
    WalkTarget,
};
use crate::frog::FrogAction;
use crate::scan::manhattan_ordered;
use crate::world::AiWorld;

pub const FLUID_TAGS: &str = "minecraft:fluid";
pub const FROG_TRIES_TO_FIND_LAND_NEAR: &str = "minecraft:frog_tries_to_find_land_near";
pub const AXOLOTL_TRIES_TO_FIND: &str = "minecraft:axolotl_tries_to_find";
pub const SUPPORTS_FROGSPAWN: &str = "minecraft:supports_frogspawn";
pub const FIND_LIQUID_RETRY: i64 = 22;
pub const FIND_RETRY: i64 = 40;
pub const CLOSE_ENOUGH: f64 = 1.5;

const HORIZONTAL: [(i32, i32); 4] = [(0, -1), (1, 0), (0, 1), (-1, 0)];
const FLUID_EMPTY: u32 = 0;
const FLUID_FLOWING_WATER: u32 = 1;
const FLUID_WATER: u32 = 2;
const FLUID_FLOWING_LAVA: u32 = 3;
const FLUID_LAVA: u32 = 4;

pub fn fluid_id(state: Option<StateId>) -> u32 {
    let Some(state) = state else {
        return FLUID_EMPTY;
    };
    let flags = state.flags();
    let level = || {
        state
            .property("level")
            .and_then(|value| value.parse::<i32>().ok())
            .unwrap_or(0)
    };
    if flags.contains(StateFlags::WATER) {
        return if state.kind() == Some(BlockKind::Water) && level() != 0 {
            FLUID_FLOWING_WATER
        } else {
            FLUID_WATER
        };
    }
    if flags.contains(StateFlags::LAVA) {
        return if state.kind() == Some(BlockKind::Lava) && level() != 0 {
            FLUID_FLOWING_LAVA
        } else {
            FLUID_LAVA
        };
    }
    FLUID_EMPTY
}

pub fn fluid_in_tag(state: Option<StateId>, tag: &str) -> bool {
    let id = fluid_id(state);
    id != FLUID_EMPTY
        && rspigot_data::tags::tag(FLUID_TAGS, tag)
            .is_some_and(|ids| ids.binary_search(&id).is_ok())
}

pub fn block_in_tag(state: Option<StateId>, tag: &str) -> bool {
    crate::animal::block_tag(state, tag)
}

pub fn is_liquid_block(state: Option<StateId>) -> bool {
    state.is_some_and(|state| matches!(state.kind(), Some(BlockKind::Water | BlockKind::Lava)))
}

pub fn has_up_face(state: Option<StateId>) -> bool {
    state.is_some_and(|state| {
        state
            .collision_shape()
            .iter()
            .any(|shape| shape.max_y >= 1.0 - 1.0e-7)
    })
}

fn is_empty_shape(state: Option<StateId>) -> bool {
    state.is_some_and(|state| state.collision_shape().is_empty())
}

fn differs_horizontally(a: BlockPos, b: BlockPos) -> bool {
    a.x != b.x || a.z != b.z
}

fn target_found<W: AiWorld>(cx: &mut BehaviorCx<'_, '_, W>, pos: BlockPos, speed_modifier: f32) {
    cx.brain.memories.set(
        MemoryModuleType::LookTarget,
        MemoryValue::Tracker(PositionTracker::of_block(pos)),
    );
    cx.brain.memories.set(
        MemoryModuleType::WalkTarget,
        MemoryValue::WalkTarget(WalkTarget::block(pos, speed_modifier, 0)),
    );
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TryFindLiquid {
    range: i32,
    speed_modifier: f32,
    fluid_tag: &'static str,
    next_ok_start: i64,
}

impl TryFindLiquid {
    pub fn new(range: i32, speed_modifier: f32, fluid_tag: &'static str) -> Self {
        Self {
            range,
            speed_modifier,
            fluid_tag,
            next_ok_start: 0,
        }
    }

    pub fn conditions(&self) -> Vec<Condition> {
        vec![
            (MemoryModuleType::AttackTarget, MemoryStatus::ValueAbsent),
            (MemoryModuleType::WalkTarget, MemoryStatus::ValueAbsent),
            (MemoryModuleType::LookTarget, MemoryStatus::Registered),
        ]
    }

    fn search<W: AiWorld>(&self, cx: &BehaviorCx<'_, '_, W>, origin: BlockPos) -> Option<BlockPos> {
        let world = cx.ai.world;
        let mut found: Option<BlockPos> = None;
        for pos in manhattan_ordered(origin, self.range, self.range, self.range) {
            if !differs_horizontally(pos, origin) {
                continue;
            }
            let state = world.state(pos);
            if !is_liquid_block(state) || !fluid_in_tag(state, self.fluid_tag) {
                continue;
            }
            if world
                .state(pos.offset(0, 1, 0))
                .is_some_and(StateId::is_air)
            {
                return Some(pos);
            }
            if found.is_none()
                && !crate::scan::closer_to_center_than(pos, cx.ai.mob.position, CLOSE_ENOUGH)
            {
                found = Some(pos);
            }
        }
        found
    }
}

impl Trigger for TryFindLiquid {
    fn trigger<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, now: i64) -> bool {
        let origin = cx.ai.mob.block_pos();
        if fluid_in_tag(cx.ai.world.state(origin), self.fluid_tag) {
            return false;
        }
        if now < self.next_ok_start {
            self.next_ok_start = now + FIND_LIQUID_RETRY;
            return true;
        }
        if let Some(found) = self.search(cx, origin) {
            target_found(cx, found, self.speed_modifier);
        }
        self.next_ok_start = now + FIND_RETRY;
        true
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TryFindLandNearLiquid {
    range: i32,
    speed_modifier: f32,
    fluid_tag: &'static str,
    next_ok_start: i64,
}

impl TryFindLandNearLiquid {
    pub fn new(range: i32, speed_modifier: f32, fluid_tag: &'static str) -> Self {
        Self {
            range,
            speed_modifier,
            fluid_tag,
            next_ok_start: 0,
        }
    }

    pub fn conditions(&self) -> Vec<Condition> {
        vec![
            (MemoryModuleType::AttackTarget, MemoryStatus::ValueAbsent),
            (MemoryModuleType::WalkTarget, MemoryStatus::ValueAbsent),
            (MemoryModuleType::LookTarget, MemoryStatus::Registered),
        ]
    }

    fn is_target<W: AiWorld>(&self, world: &W, pos: BlockPos) -> bool {
        if !is_empty_shape(world.state(pos)) || is_empty_shape(world.state(pos.offset(0, -1, 0))) {
            return false;
        }
        HORIZONTAL.iter().any(|(dx, dz)| {
            let side = pos.offset(*dx, 0, *dz);
            world.state(side).is_some_and(StateId::is_air)
                && fluid_in_tag(world.state(side.offset(0, -1, 0)), self.fluid_tag)
        })
    }
}

impl Trigger for TryFindLandNearLiquid {
    fn trigger<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, now: i64) -> bool {
        let origin = cx.ai.mob.block_pos();
        if fluid_in_tag(cx.ai.world.state(origin), self.fluid_tag) {
            return false;
        }
        if now < self.next_ok_start {
            self.next_ok_start = now + FIND_RETRY;
            return true;
        }
        let found = manhattan_ordered(origin, self.range, self.range, self.range)
            .find(|pos| differs_horizontally(*pos, origin) && self.is_target(cx.ai.world, *pos));
        if let Some(found) = found {
            target_found(cx, found, self.speed_modifier);
        }
        self.next_ok_start = now + FIND_RETRY;
        true
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TryLaySpawnOnFluidNearLand;

impl TryLaySpawnOnFluidNearLand {
    pub fn conditions(&self) -> Vec<Condition> {
        vec![
            (MemoryModuleType::AttackTarget, MemoryStatus::ValueAbsent),
            (MemoryModuleType::WalkTarget, MemoryStatus::ValuePresent),
            (MemoryModuleType::IsPregnant, MemoryStatus::ValuePresent),
        ]
    }
}

pub fn supports_frogspawn(state: Option<StateId>) -> bool {
    fluid_in_tag(state, SUPPORTS_FROGSPAWN) || crate::animal::block_tag(state, SUPPORTS_FROGSPAWN)
}

impl Trigger for TryLaySpawnOnFluidNearLand {
    fn trigger<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        if cx.ai.mob.fluid.in_water || !cx.ai.mob.on_ground {
            return false;
        }
        let below = cx.ai.mob.block_pos().offset(0, -1, 0);
        for (dx, dz) in HORIZONTAL {
            let relative = below.offset(dx, 0, dz);
            if has_up_face(cx.ai.world.state(relative)) {
                continue;
            }
            if !supports_frogspawn(cx.ai.world.state(relative)) {
                continue;
            }
            let above = relative.offset(0, 1, 0);
            if cx.ai.world.state(above).is_some_and(StateId::is_air) {
                cx.ai.act(MobAction::Frog(FrogAction::LaySpawn(above)));
                cx.brain.memories.erase(MemoryModuleType::IsPregnant);
                return true;
            }
        }
        true
    }
}
