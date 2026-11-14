use rspigot_blocks::StateId;
use rspigot_domain::{BlockPos, EntityId};

use crate::action::MobAction;
use crate::animal::kind_in_block_tag;
use crate::behavior::{
    BehaviorCx, Condition, GlobalPos, MemoryModuleType, MemoryStatus, MemoryValue, Trigger,
};
use crate::brain::AiContext;
use crate::scan::closer_to_center_than;
use crate::world::AiWorld;

const MOB_INTERACTABLE_DOORS: &str = "minecraft:mob_interactable_doors";
const CHECK_INTERVAL: i32 = 20;
const FORGET_DISTANCE: f64 = 3.0;
const TRAFFIC_DISTANCE: f64 = 2.0;

pub fn is_mob_interactable_door(state: Option<StateId>) -> bool {
    state
        .and_then(StateId::kind)
        .is_some_and(|kind| kind_in_block_tag(kind, MOB_INTERACTABLE_DOORS))
}

pub fn is_door_open(state: Option<StateId>) -> bool {
    state.and_then(|state| state.property("open")) == Some("true")
}

fn is_door_too_far_away<W: AiWorld>(ai: &AiContext<'_, W>, door: &GlobalPos) -> bool {
    door.dimension != ai.world.dimension()
        || !closer_to_center_than(door.pos, ai.mob.position, FORGET_DISTANCE)
}

fn is_mob_coming_through_door<W: AiWorld>(world: &W, other: EntityId, door: BlockPos) -> bool {
    world
        .path_door_nodes(other)
        .is_some_and(|(previous, next)| previous == door || next == door)
}

fn are_other_mobs_coming_through_door<W: AiWorld>(
    ai: &AiContext<'_, W>,
    door: BlockPos,
    nearest: Option<&[EntityId]>,
) -> bool {
    let Some(nearest) = nearest else {
        return false;
    };
    nearest
        .iter()
        .filter_map(|id| ai.world.living(*id))
        .filter(|other| other.kind == ai.mob.kind)
        .filter(|other| closer_to_center_than(door, other.position, TRAFFIC_DISTANCE))
        .any(|other| is_mob_coming_through_door(ai.world, other.id, door))
}

pub fn close_doors_that_i_have_opened_or_passed_through<W: AiWorld>(
    ai: &mut AiContext<'_, W>,
    previous: Option<BlockPos>,
    next: Option<BlockPos>,
    doors: &mut Vec<GlobalPos>,
    nearest: Option<&[EntityId]>,
) {
    doors.retain(|door| {
        if previous == Some(door.pos) || next == Some(door.pos) {
            return true;
        }
        if is_door_too_far_away(ai, door) {
            return false;
        }
        let state = ai.world.state(door.pos);
        if is_mob_interactable_door(state)
            && is_door_open(state)
            && !are_other_mobs_coming_through_door(ai, door.pos, nearest)
        {
            ai.act(MobAction::SetDoorOpen {
                pos: door.pos,
                open: false,
                interact: false,
            });
        }
        false
    });
}

fn remember_door_to_close<W: AiWorld>(
    ai: &AiContext<'_, W>,
    doors: &mut Option<Vec<GlobalPos>>,
    pos: BlockPos,
) {
    let door = GlobalPos {
        dimension: ai.world.dimension().to_owned(),
        pos,
    };
    let set = doors.get_or_insert_with(Vec::new);
    if !set.contains(&door) {
        set.push(door);
    }
}

pub fn doors_to_close<W: AiWorld>(cx: &BehaviorCx<'_, '_, W>) -> Option<Vec<GlobalPos>> {
    match cx.brain.memories.get(MemoryModuleType::DoorsToClose) {
        Some(MemoryValue::GlobalPosList(doors)) => Some(doors.clone()),
        _ => None,
    }
}

pub fn nearest_living_entities<W: AiWorld>(cx: &BehaviorCx<'_, '_, W>) -> Option<Vec<EntityId>> {
    cx.brain
        .memories
        .get(MemoryModuleType::NearestLivingEntities)
        .and_then(MemoryValue::as_entities)
        .map(<[EntityId]>::to_vec)
}

pub fn store_doors_to_close<W: AiWorld>(cx: &mut BehaviorCx<'_, '_, W>, doors: Vec<GlobalPos>) {
    cx.brain.memories.set_option(
        MemoryModuleType::DoorsToClose,
        Some(MemoryValue::GlobalPosList(doors)),
    );
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct InteractWithDoor {
    last_checked_node: Option<BlockPos>,
    remaining_cooldown: i32,
}

impl InteractWithDoor {
    pub fn conditions(&self) -> Vec<Condition> {
        vec![
            (MemoryModuleType::Path, MemoryStatus::ValuePresent),
            (MemoryModuleType::DoorsToClose, MemoryStatus::Registered),
            (
                MemoryModuleType::NearestLivingEntities,
                MemoryStatus::Registered,
            ),
        ]
    }

    fn open_door<W: AiWorld>(
        cx: &mut BehaviorCx<'_, '_, W>,
        doors: &mut Option<Vec<GlobalPos>>,
        pos: BlockPos,
        remember_always: bool,
    ) -> bool {
        let state = cx.ai.world.state(pos);
        if !is_mob_interactable_door(state) {
            return true;
        }
        if !is_door_open(state) {
            cx.ai.act(MobAction::SetDoorOpen {
                pos,
                open: true,
                interact: true,
            });
            remember_door_to_close(cx.ai, doors, pos);
        } else if remember_always {
            remember_door_to_close(cx.ai, doors, pos);
        }
        true
    }
}

impl Trigger for InteractWithDoor {
    fn trigger<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        let Some(path) = cx
            .brain
            .memories
            .get(MemoryModuleType::Path)
            .and_then(MemoryValue::as_path)
        else {
            return false;
        };
        if path.not_started() || path.is_done() {
            return false;
        }
        let next = path.next_node_pos();
        let Some(previous) = path.previous_node().map(|node| node.pos) else {
            return false;
        };
        if self.last_checked_node == Some(next) {
            self.remaining_cooldown = CHECK_INTERVAL;
        } else {
            self.remaining_cooldown -= 1;
            if self.remaining_cooldown > 0 {
                return false;
            }
        }
        self.last_checked_node = Some(next);
        let mut doors = doors_to_close(cx);
        Self::open_door(cx, &mut doors, previous, true);
        Self::open_door(cx, &mut doors, next, false);
        if let Some(mut set) = doors {
            let nearest = nearest_living_entities(cx);
            close_doors_that_i_have_opened_or_passed_through(
                cx.ai,
                Some(previous),
                Some(next),
                &mut set,
                nearest.as_deref(),
            );
            store_doors_to_close(cx, set);
        }
        true
    }
}
