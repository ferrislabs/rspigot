use rspigot_domain::{BlockPos, EntityId, Vec3};
use rspigot_entities::EntityKind;

use crate::action::{MobAction, TargetReason};
use crate::behavior::control::{Behavior, BehaviorCx};
use crate::behavior::memory::{Condition, MemoryModuleType, MemoryStatus, MemoryValue, WalkTarget};
use crate::behavior::support::{UniformInt, dist_sqr, manhattan_distance};
use crate::behavior::tracker::PositionTracker;
use crate::goat::GoatAction;
use crate::random_pos::is_stable_destination;
use crate::targeting::TargetingConditions;
use crate::world::{AiWorld, Living, TargetKind};

pub const RAM_TIME_OUT: i32 = 200;
pub const PREPARE_RAM_TIME_OUT: i32 = 160;
pub const START_RAM_EVENT: i8 = 58;
pub const STOP_RAM_EVENT: i8 = 59;
const RAM_ARRIVED_DISTANCE_SQR: f64 = 0.25 * 0.25;
const MIN_KNOCKBACK_SPEED: f32 = 0.2;
const MAX_KNOCKBACK_SPEED: f32 = 3.0;
const KNOCKBACK_SPEED_SCALE: f32 = 1.65;
const EFFECT_SPEED_STEP: f32 = 0.25;
const SNAPS_GOAT_HORN: &str = "minecraft:snaps_goat_horn";
const IMPACT: [&str; 2] = [
    "minecraft:entity.goat.ram_impact",
    "minecraft:entity.goat.screaming.ram_impact",
];
const PREPARE: [&str; 2] = [
    "minecraft:entity.goat.prepare_ram",
    "minecraft:entity.goat.screaming.prepare_ram",
];

fn variant_sound(sounds: [&'static str; 2], screaming: bool) -> &'static str {
    sounds[usize::from(screaming)]
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RamTimings {
    pub regular: UniformInt,
    pub screaming: UniformInt,
}

impl RamTimings {
    fn between_rams(self, screaming: bool) -> UniformInt {
        if screaming {
            self.screaming
        } else {
            self.regular
        }
    }
}

fn within_border<W: AiWorld>(world: &W, target: &Living) -> bool {
    world.within_world_border(BlockPos::new(
        target.bounds.min_x.floor() as i32,
        target.bounds.min_y.floor() as i32,
        target.bounds.min_z.floor() as i32,
    )) && world.within_world_border(BlockPos::new(
        target.bounds.max_x.floor() as i32,
        target.bounds.max_y.floor() as i32,
        target.bounds.max_z.floor() as i32,
    ))
}

pub fn ram_accepts<W: AiWorld>(cx: &mut BehaviorCx<'_, '_, W>, target: &Living) -> bool {
    let griefing = cx.ai.world.mob_griefing();
    let world = cx.ai.world;
    TargetingConditions::for_combat().test(cx.ai, target, |candidate| {
        candidate.kind != EntityKind::Goat
            && (griefing || candidate.kind != EntityKind::ArmorStand)
            && within_border(world, candidate)
    })
}

#[derive(Debug, Clone, PartialEq)]
pub struct RamTarget {
    timings: RamTimings,
    speed: f32,
    ram_direction: Vec3,
}

impl RamTarget {
    const CONDITION: [Condition; 2] = [
        (
            MemoryModuleType::RamCooldownTicks,
            MemoryStatus::ValueAbsent,
        ),
        (MemoryModuleType::RamTarget, MemoryStatus::ValuePresent),
    ];

    pub fn new(timings: RamTimings, speed: f32) -> Self {
        Self {
            timings,
            speed,
            ram_direction: Vec3::ZERO,
        }
    }

    fn knockback_force<W: AiWorld>(cx: &BehaviorCx<'_, '_, W>) -> f64 {
        if cx.ai.mob.baby { 1.0 } else { 2.5 }
    }

    fn nearby<W: AiWorld>(cx: &mut BehaviorCx<'_, '_, W>) -> Option<Living> {
        let area = cx.ai.mob.bounds();
        let mut candidates = cx.ai.world.living_of(TargetKind::Player, &area);
        candidates.extend(
            cx.ai
                .world
                .living_of(TargetKind::Class("LivingEntity"), &area),
        );
        candidates
            .into_iter()
            .find(|candidate| ram_accepts(cx, candidate))
    }

    fn has_rammed_horn_breaking_block<W: AiWorld>(cx: &BehaviorCx<'_, '_, W>) -> bool {
        let mob = cx.ai.mob;
        let ahead = mob.position + mob.velocity.multiply(1.0, 0.0, 1.0).normalize();
        let pos = ahead.block();
        crate::animal::block_tag(cx.ai.world.state(pos), SNAPS_GOAT_HORN)
            || crate::animal::block_tag(cx.ai.world.state(pos.offset(0, 1, 0)), SNAPS_GOAT_HORN)
    }

    fn finish_ram<W: AiWorld>(&self, cx: &mut BehaviorCx<'_, '_, W>) {
        cx.ai.act(MobAction::EntityEvent(STOP_RAM_EVENT));
        let screaming = cx.ai.mob.species.goat.screaming;
        let cooldown = self
            .timings
            .between_rams(screaming)
            .sample(&mut cx.ai.brain.random);
        cx.brain.memories.set(
            MemoryModuleType::RamCooldownTicks,
            MemoryValue::Int(cooldown),
        );
        cx.brain.memories.erase(MemoryModuleType::RamTarget);
    }

    fn knockback_speed<W: AiWorld>(cx: &BehaviorCx<'_, '_, W>) -> f32 {
        let mob = cx.ai.mob;
        let bonus = (mob.speed_effect_level - mob.slowness_effect_level) as f32 * EFFECT_SPEED_STEP;
        (mob.speed * KNOCKBACK_SPEED_SCALE).clamp(MIN_KNOCKBACK_SPEED, MAX_KNOCKBACK_SPEED) + bonus
    }
}

impl Behavior for RamTarget {
    fn entry_condition(&self) -> &[Condition] {
        &Self::CONDITION
    }

    fn check_extra_start_conditions<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>) -> bool {
        cx.brain.memories.has_value(MemoryModuleType::RamTarget)
    }

    fn can_still_use<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        cx.brain.memories.has_value(MemoryModuleType::RamTarget)
    }

    fn start<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        let here = cx.ai.mob.block_pos();
        let Some(target) = cx
            .brain
            .memories
            .get(MemoryModuleType::RamTarget)
            .and_then(MemoryValue::as_position)
        else {
            return;
        };
        self.ram_direction = Vec3::new(
            f64::from(here.x) - target.x,
            0.0,
            f64::from(here.z) - target.z,
        )
        .normalize();
        cx.brain.memories.set(
            MemoryModuleType::WalkTarget,
            MemoryValue::WalkTarget(WalkTarget::position(target, self.speed, 0)),
        );
    }

    fn tick<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        let screaming = cx.ai.mob.species.goat.screaming;
        if let Some(target) = Self::nearby(cx) {
            let speed = Self::knockback_speed(cx);
            cx.ai.act(MobAction::Goat(GoatAction::Ram {
                target: target.id,
                speed,
                force: Self::knockback_force(cx),
                direction: self.ram_direction,
            }));
            self.finish_ram(cx);
            cx.ai.act(MobAction::Sound {
                sound: variant_sound(IMPACT, screaming),
                volume: 1.0,
                pitch: 1.0,
            });
            return;
        }
        if Self::has_rammed_horn_breaking_block(cx) {
            cx.ai.act(MobAction::Sound {
                sound: variant_sound(IMPACT, screaming),
                volume: 1.0,
                pitch: 1.0,
            });
            cx.ai.act(MobAction::Goat(GoatAction::DropHorn));
            self.finish_ram(cx);
            return;
        }
        let walk = cx
            .brain
            .memories
            .get(MemoryModuleType::WalkTarget)
            .and_then(MemoryValue::as_walk_target)
            .copied();
        let ram = cx
            .brain
            .memories
            .get(MemoryModuleType::RamTarget)
            .and_then(MemoryValue::as_position);
        let arrived = match (walk, ram) {
            (Some(walk), Some(ram)) => walk
                .target
                .current_position(cx.ai.world)
                .is_some_and(|position| position.distance_squared(ram) < RAM_ARRIVED_DISTANCE_SQR),
            _ => true,
        };
        if arrived {
            self.finish_ram(cx);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RamCandidate {
    start_position: BlockPos,
    target_position: BlockPos,
    target: EntityId,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PrepareRamNearestTarget {
    timings: RamTimings,
    min_ram_distance: i32,
    max_ram_distance: i32,
    walk_speed: f32,
    ram_prepare_time: i64,
    reached_ram_position_timestamp: Option<i64>,
    ram_candidate: Option<RamCandidate>,
}

const HORIZONTAL: [(i32, i32); 4] = [(0, -1), (1, 0), (0, 1), (-1, 0)];

impl PrepareRamNearestTarget {
    const CONDITION: [Condition; 4] = [
        (MemoryModuleType::LookTarget, MemoryStatus::Registered),
        (
            MemoryModuleType::RamCooldownTicks,
            MemoryStatus::ValueAbsent,
        ),
        (
            MemoryModuleType::NearestVisibleLivingEntities,
            MemoryStatus::ValuePresent,
        ),
        (MemoryModuleType::RamTarget, MemoryStatus::ValueAbsent),
    ];

    pub fn new(
        timings: RamTimings,
        min_ram_distance: i32,
        max_ram_distance: i32,
        walk_speed: f32,
        ram_prepare_time: i32,
    ) -> Self {
        Self {
            timings,
            min_ram_distance,
            max_ram_distance,
            walk_speed,
            ram_prepare_time: i64::from(ram_prepare_time),
            reached_ram_position_timestamp: None,
            ram_candidate: None,
        }
    }

    fn is_walkable_block<W: AiWorld>(cx: &BehaviorCx<'_, '_, W>, pos: BlockPos) -> bool {
        if !is_stable_destination(cx.ai.world, pos) {
            return false;
        }
        let path_type = crate::walk::mob_path_type(
            cx.ai.world,
            &cx.ai.brain.profile,
            cx.ai.tick_state().state,
            pos,
        );
        cx.ai.brain.profile.malus(path_type) == 0.0
    }

    fn calculate_ramming_start_position<W: AiWorld>(
        cx: &mut BehaviorCx<'_, '_, W>,
        target: &Living,
        min: i32,
        max: i32,
    ) -> Option<BlockPos> {
        let target_pos = target.position.block();
        if !Self::is_walkable_block(cx, target_pos) {
            return None;
        }
        let mut candidates = Vec::new();
        for (dx, dz) in HORIZONTAL {
            let mut cursor = target_pos;
            for _ in 0..max {
                cursor = cursor.offset(dx, 0, dz);
                if !Self::is_walkable_block(cx, cursor) {
                    cursor = cursor.offset(-dx, 0, -dz);
                    break;
                }
            }
            if manhattan_distance(cursor, target_pos) >= min {
                candidates.push(cursor);
            }
        }
        let here = cx.ai.mob.block_pos();
        candidates.sort_by(|a, b| dist_sqr(here, *a).total_cmp(&dist_sqr(here, *b)));
        candidates.into_iter().find(|candidate| {
            cx.ai
                .create_path_to(*candidate, 0)
                .is_some_and(|path| path.can_reach())
        })
    }

    fn choose_ram_position<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, target: &Living) {
        self.reached_ram_position_timestamp = None;
        self.ram_candidate = Self::calculate_ramming_start_position(
            cx,
            target,
            self.min_ram_distance,
            self.max_ram_distance,
        )
        .map(|start| RamCandidate {
            start_position: start,
            target_position: target.position.block(),
            target: target.id,
        });
    }

    fn edge_of_block(start: BlockPos, target: BlockPos) -> Vec3 {
        let sign = |value: i32| f64::from(value.signum());
        let bottom = Vec3::new(
            f64::from(target.x) + 0.5,
            f64::from(target.y),
            f64::from(target.z) + 0.5,
        );
        Vec3::new(
            bottom.x + sign(target.x - start.x) * 0.5,
            bottom.y,
            bottom.z + sign(target.z - start.z) * 0.5,
        )
    }

    fn cooldown_on_fail<W: AiWorld>(&self, cx: &BehaviorCx<'_, '_, W>) -> i32 {
        self.timings
            .between_rams(cx.ai.mob.species.goat.screaming)
            .min_inclusive
    }
}

impl Behavior for PrepareRamNearestTarget {
    fn entry_condition(&self) -> &[Condition] {
        &Self::CONDITION
    }

    fn start<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        let found = cx
            .brain
            .memories
            .get(MemoryModuleType::NearestVisibleLivingEntities)
            .and_then(MemoryValue::as_visible)
            .cloned()
            .and_then(|visible| {
                let mut chosen = None;
                for id in visible.nearby_entities() {
                    let Some(living) = cx.ai.world.living(*id) else {
                        continue;
                    };
                    if ram_accepts(cx, &living)
                        && visible.contains_entity(cx.ai, &cx.brain.memories, living.id)
                    {
                        chosen = Some(living);
                        break;
                    }
                }
                chosen
            });
        let Some(target) = found else {
            return;
        };
        let reason = if target.player {
            TargetReason::ClosestPlayer
        } else {
            TargetReason::ClosestEntity
        };
        cx.ai.request_brain_target(None, Some(target.id), reason);
        self.choose_ram_position(cx, &target);
    }

    fn stop<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        if !cx.brain.memories.has_value(MemoryModuleType::RamTarget) {
            cx.ai.act(MobAction::EntityEvent(STOP_RAM_EVENT));
            let cooldown = self.cooldown_on_fail(cx);
            cx.brain.memories.set(
                MemoryModuleType::RamCooldownTicks,
                MemoryValue::Int(cooldown),
            );
        }
    }

    fn can_still_use<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        self.ram_candidate.is_some_and(|candidate| {
            cx.ai
                .world
                .living(candidate.target)
                .is_some_and(|l| l.alive)
        })
    }

    fn tick<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, now: i64) {
        let Some(candidate) = self.ram_candidate else {
            return;
        };
        cx.brain.memories.set(
            MemoryModuleType::WalkTarget,
            MemoryValue::WalkTarget(WalkTarget::block(
                candidate.start_position,
                self.walk_speed,
                0,
            )),
        );
        cx.brain.memories.set(
            MemoryModuleType::LookTarget,
            MemoryValue::Tracker(PositionTracker::of_entity(candidate.target, true)),
        );
        let Some(target) = cx.ai.world.living(candidate.target) else {
            return;
        };
        if target.position.block() == candidate.target_position {
            let here = cx.ai.mob.block_pos();
            if here == candidate.start_position {
                cx.ai.act(MobAction::EntityEvent(START_RAM_EVENT));
                let reached = *self.reached_ram_position_timestamp.get_or_insert(now);
                if self.ram_prepare_time <= now - reached {
                    cx.brain.memories.set(
                        MemoryModuleType::RamTarget,
                        MemoryValue::Position(Self::edge_of_block(here, candidate.target_position)),
                    );
                    let screaming = cx.ai.mob.species.goat.screaming;
                    cx.ai
                        .act(MobAction::Voice(variant_sound(PREPARE, screaming)));
                    self.ram_candidate = None;
                }
            }
        } else {
            cx.ai.act(MobAction::EntityEvent(STOP_RAM_EVENT));
            cx.ai.stop_navigation();
            self.choose_ram_position(cx, &target);
        }
    }
}
