use rspigot_blocks::BlockKind;
use rspigot_domain::{BlockPos, EntityId};
use rspigot_entities::EntityKind;

use crate::behavior::{BehaviorCx, MemoryModuleType, MemoryValue, PositionTracker, WalkTarget};
use crate::villager::VillagerAction;
use crate::villager::poi::PoiRule;
use crate::world::AiWorld;

pub fn tag_contains_block(kind: BlockKind, tag: &str) -> bool {
    crate::animal::kind_in_block_tag(kind, tag)
}

pub fn dist_sqr_pos(a: BlockPos, b: BlockPos) -> f64 {
    let (dx, dy, dz) = (
        f64::from(a.x - b.x),
        f64::from(a.y - b.y),
        f64::from(a.z - b.z),
    );
    dx * dx + dy * dy + dz * dz
}

pub fn manhattan(a: BlockPos, b: BlockPos) -> i32 {
    (a.x - b.x).abs() + (a.y - b.y).abs() + (a.z - b.z).abs()
}

pub fn set_walk_and_look_targets<W: AiWorld>(
    cx: &mut BehaviorCx<'_, '_, W>,
    pos: BlockPos,
    speed_modifier: f32,
    close_enough_dist: i32,
) {
    cx.brain.memories.set(
        MemoryModuleType::LookTarget,
        MemoryValue::Tracker(PositionTracker::of_block(pos)),
    );
    cx.brain.memories.set(
        MemoryModuleType::WalkTarget,
        MemoryValue::WalkTarget(WalkTarget::block(pos, speed_modifier, close_enough_dist)),
    );
}

pub fn set_walk_and_look_targets_entity<W: AiWorld>(
    cx: &mut BehaviorCx<'_, '_, W>,
    target: EntityId,
    speed_modifier: f32,
    close_enough_dist: i32,
) {
    cx.brain.memories.set(
        MemoryModuleType::LookTarget,
        MemoryValue::Tracker(PositionTracker::of_entity(target, true)),
    );
    cx.brain.memories.set(
        MemoryModuleType::WalkTarget,
        MemoryValue::WalkTarget(WalkTarget::entity(
            target,
            speed_modifier,
            close_enough_dist,
        )),
    );
}

pub fn lock_gaze_and_walk_to_each_other<W: AiWorld>(
    cx: &mut BehaviorCx<'_, '_, W>,
    other: EntityId,
    speed_modifier: f32,
    close_enough_dist: i32,
) {
    let me = cx.ai.mob.id;
    let look = |target: EntityId| PositionTracker::of_entity(target, true);
    let walk = |target: EntityId| WalkTarget::entity(target, speed_modifier, close_enough_dist);
    cx.brain.memories.set(
        MemoryModuleType::LookTarget,
        MemoryValue::Tracker(look(other)),
    );
    cx.brain.memories.set(
        MemoryModuleType::WalkTarget,
        MemoryValue::WalkTarget(walk(other)),
    );
    cx.ai.act(crate::action::MobAction::SetMemory {
        mob: other,
        memory: MemoryModuleType::LookTarget,
        value: Some(MemoryValue::Tracker(look(me))),
    });
    cx.ai.act(crate::action::MobAction::SetMemory {
        mob: other,
        memory: MemoryModuleType::WalkTarget,
        value: Some(MemoryValue::WalkTarget(walk(me))),
    });
}

pub fn target_is_valid<W: AiWorld>(
    cx: &mut BehaviorCx<'_, '_, W>,
    memory: MemoryModuleType,
    kind: EntityKind,
) -> bool {
    let Some(id) = cx
        .brain
        .memories
        .get(memory)
        .and_then(MemoryValue::as_entity)
    else {
        return false;
    };
    let Some(target) = cx.ai.world.living(id) else {
        return false;
    };
    if target.kind != kind || !target.alive {
        return false;
    }
    let visible = cx
        .brain
        .memories
        .get(MemoryModuleType::NearestVisibleLivingEntities)
        .and_then(MemoryValue::as_visible);
    visible.is_some_and(|visible| visible.contains_entity(cx.ai, &cx.brain.memories, id))
}

pub fn memory_rule(memory: MemoryModuleType) -> Option<PoiRule> {
    match memory {
        MemoryModuleType::JobSite => Some(PoiRule::HeldJobSite),
        MemoryModuleType::PotentialJobSite => Some(PoiRule::AcquirableJobSite),
        MemoryModuleType::Home => Some(PoiRule::Home),
        MemoryModuleType::MeetingPoint => Some(PoiRule::Meeting),
        _ => None,
    }
}

pub fn release_poi<W: AiWorld>(cx: &mut BehaviorCx<'_, '_, W>, memory: MemoryModuleType) {
    let Some(rule) = memory_rule(memory) else {
        return;
    };
    let Some(global) = cx
        .brain
        .memories
        .get(memory)
        .and_then(MemoryValue::as_global_pos)
    else {
        return;
    };
    let pos = global.pos;
    let profession = cx.ai.mob.species.villager.profession;
    if cx
        .ai
        .world
        .poi_type_at(pos)
        .is_some_and(|kind| rule.matches(profession, kind))
    {
        cx.ai.act(crate::action::MobAction::Villager(
            VillagerAction::ReleasePoi(pos),
        ));
    }
}
