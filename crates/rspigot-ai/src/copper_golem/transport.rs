use rspigot_blocks::{BlockKind, StateId};
use rspigot_domain::entity::Aabb;
use rspigot_domain::entity::species::copper_golem::GolemState;
use rspigot_domain::item::Slot;
use rspigot_domain::{BlockPos, ChunkPos, Vec3};

use crate::action::MobAction;
use crate::animal::block_tag;
use crate::behavior::{
    Behavior, BehaviorCx, Condition, GlobalPos, MemoryModuleType, MemoryStatus, MemoryValue,
};
use crate::copper_golem::container::{
    self, ChestSide, chest_side, connected_pos, is_chest_blocked_at,
};
use crate::copper_golem::{CopperGolemAction, ITEM_DROP, ITEM_GET, ITEM_NO_DROP, ITEM_NO_GET};
use crate::creaking::DomainView;
use crate::villager::support::set_walk_and_look_targets;
use crate::world::AiWorld;

pub const SPEED_MODIFIER: f32 = 1.0;
pub const HORIZONTAL_SEARCH_DISTANCE: i32 = 32;
pub const VERTICAL_SEARCH_DISTANCE: i32 = 8;
pub const INTERACTION_TICKS: i32 = 60;
pub const COOLDOWN_TICKS: i32 = 140;
pub const MEMORY_TICKS: i64 = 6000;
const MAX_VISITED: usize = 10;
const MAX_UNREACHABLE: usize = 50;
const QUEUE_DISTANCE: f64 = 3.0;
const REACHED_DISTANCE: f64 = 2.0;
const PATH_FINISHED_RANGE: f64 = 1.0;
const PATH_RANGE: f64 = 0.5;
const VERTICAL_REACH: f64 = 0.5;
const OPEN_TICK: i32 = 1;
const SOUND_TICK: i32 = 9;
const COPPER_CHESTS: &str = "minecraft:copper_chests";
const CHUNK_SIZE: i32 = 16;
const FACE_OFFSETS: [(i32, i32, i32); 6] = [
    (0, -1, 0),
    (0, 1, 0),
    (0, 0, -1),
    (0, 0, 1),
    (-1, 0, 0),
    (1, 0, 0),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportState {
    Travelling,
    Queuing,
    Interacting,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InteractionState {
    PickupItem,
    PickupNoItem,
    PlaceItem,
    PlaceNoItem,
}

impl InteractionState {
    const fn golem(self) -> (GolemState, &'static str) {
        match self {
            Self::PickupItem => (GolemState::GettingItem, ITEM_GET),
            Self::PickupNoItem => (GolemState::GettingNoItem, ITEM_NO_GET),
            Self::PlaceItem => (GolemState::DroppingItem, ITEM_DROP),
            Self::PlaceNoItem => (GolemState::DroppingNoItem, ITEM_NO_DROP),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetVerdict {
    Cancel(BlockPos),
    Retarget { from: BlockPos, to: BlockPos },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransportTarget {
    pub pos: BlockPos,
    pub state: StateId,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TransportItemsBetweenContainers {
    target: Option<TransportTarget>,
    state: TransportState,
    interaction: Option<InteractionState>,
    ticks_since_reaching_target: i32,
    verdict: Option<TargetVerdict>,
}

impl Default for TransportItemsBetweenContainers {
    fn default() -> Self {
        Self::new()
    }
}

fn is_source(state: StateId) -> bool {
    block_tag(Some(state), COPPER_CHESTS)
}

fn is_destination(state: StateId) -> bool {
    matches!(
        state.kind(),
        Some(BlockKind::Chest | BlockKind::TrappedChest)
    )
}

fn global<W: AiWorld>(cx: &BehaviorCx<'_, '_, W>, pos: BlockPos) -> GlobalPos {
    GlobalPos {
        dimension: cx.ai.world.dimension().to_owned(),
        pos,
    }
}

fn positions<W: AiWorld>(cx: &BehaviorCx<'_, '_, W>, memory: MemoryModuleType) -> Vec<GlobalPos> {
    match cx.brain.memories.get(memory) {
        Some(MemoryValue::GlobalPosList(list)) => list.clone(),
        _ => Vec::new(),
    }
}

fn bounds_of(state: StateId) -> Option<Aabb> {
    let mut shapes = state.collision_shape().iter();
    let first = shapes.next()?;
    let mut bounds = Aabb::new(
        first.min_x,
        first.min_y,
        first.min_z,
        first.max_x,
        first.max_y,
        first.max_z,
    );
    for shape in shapes {
        bounds = Aabb::new(
            bounds.min_x.min(shape.min_x),
            bounds.min_y.min(shape.min_y),
            bounds.min_z.min(shape.min_z),
            bounds.max_x.max(shape.max_x),
            bounds.max_y.max(shape.max_y),
            bounds.max_z.max(shape.max_z),
        );
    }
    Some(bounds)
}

fn is_picking_up<W: AiWorld>(cx: &BehaviorCx<'_, '_, W>) -> bool {
    cx.ai.mob.main_hand.is_none()
}

fn is_panicking<W: AiWorld>(cx: &BehaviorCx<'_, '_, W>) -> bool {
    cx.brain.memories.has_value(MemoryModuleType::IsPanicking)
}

fn has_finished_path<W: AiWorld>(cx: &BehaviorCx<'_, '_, W>) -> bool {
    cx.ai
        .brain
        .navigation
        .path()
        .is_some_and(crate::Path::is_done)
}

fn interaction_range<W: AiWorld>(cx: &BehaviorCx<'_, '_, W>) -> f64 {
    if has_finished_path(cx) {
        PATH_FINISHED_RANGE
    } else {
        PATH_RANGE
    }
}

fn middle_y<W: AiWorld>(cx: &BehaviorCx<'_, '_, W>, pos: Vec3) -> Vec3 {
    Vec3::new(pos.x, pos.y + f64::from(cx.ai.mob.height) / 2.0, pos.z)
}

fn center_pos<W: AiWorld>(cx: &BehaviorCx<'_, '_, W>) -> Vec3 {
    middle_y(cx, cx.ai.mob.position)
}

fn within_target_distance<W: AiWorld>(
    cx: &BehaviorCx<'_, '_, W>,
    distance: f64,
    target: &TransportTarget,
    from: Vec3,
) -> bool {
    let (width, height) = (f64::from(cx.ai.mob.width), f64::from(cx.ai.mob.height));
    let body = Aabb::new(
        from.x - width / 2.0,
        from.y - height / 2.0,
        from.z - width / 2.0,
        from.x + width / 2.0,
        from.y + height / 2.0,
        from.z + width / 2.0,
    );
    let Some(shape) = bounds_of(target.state) else {
        return false;
    };
    Aabb::new(
        shape.min_x - distance,
        shape.min_y - VERTICAL_REACH,
        shape.min_z - distance,
        shape.max_x + distance,
        shape.max_y + VERTICAL_REACH,
        shape.max_z + distance,
    )
    .move_by(Vec3::new(
        f64::from(target.pos.x),
        f64::from(target.pos.y),
        f64::from(target.pos.z),
    ))
    .intersects(&body)
}

fn can_see_any_side<W: AiWorld>(
    cx: &BehaviorCx<'_, '_, W>,
    target: &TransportTarget,
    eye: Vec3,
) -> bool {
    let center = target.pos.center();
    FACE_OFFSETS.iter().any(|(dx, dy, dz)| {
        let to = Vec3::new(
            center.x + f64::from(*dx) * 0.5,
            center.y + f64::from(*dy) * 0.5,
            center.z + f64::from(*dz) * 0.5,
        );
        rspigot_domain::entity::raycast::clip_blocks_with(
            &DomainView(cx.ai.world),
            eye,
            to,
            StateId::collision_shape,
        )
        .is_some_and(|hit| hit.pos == target.pos)
    })
}

fn try_create<W: AiWorld>(world: &W, pos: BlockPos) -> Option<TransportTarget> {
    let state = world.state(pos)?;
    if !is_source(state) && !is_destination(state) {
        return None;
    }
    world.chest_container(pos, false)?;
    Some(TransportTarget { pos, state })
}

fn connected_targets<W: AiWorld>(world: &W, target: &TransportTarget) -> Vec<TransportTarget> {
    let mut all = vec![*target];
    if chest_side(target.state) != ChestSide::Single
        && let Some(other) = try_create(world, connected_pos(target.pos, target.state))
    {
        all.push(other);
    }
    all
}

impl TransportItemsBetweenContainers {
    pub const fn new() -> Self {
        Self {
            target: None,
            state: TransportState::Travelling,
            interaction: None,
            ticks_since_reaching_target: 0,
            verdict: None,
        }
    }

    pub fn target(&self) -> Option<TransportTarget> {
        self.target
    }

    pub fn state(&self) -> TransportState {
        self.state
    }

    pub fn interaction(&self) -> Option<InteractionState> {
        self.interaction
    }

    pub fn ticks_since_reaching_target(&self) -> i32 {
        self.ticks_since_reaching_target
    }

    pub fn set_verdict(&mut self, verdict: TargetVerdict) {
        self.verdict = Some(verdict);
    }

    pub fn conditions(&self) -> Vec<Condition> {
        vec![
            (
                MemoryModuleType::VisitedBlockPositions,
                MemoryStatus::Registered,
            ),
            (
                MemoryModuleType::UnreachableTransportBlockPositions,
                MemoryStatus::Registered,
            ),
            (
                MemoryModuleType::TransportItemsCooldownTicks,
                MemoryStatus::ValueAbsent,
            ),
            (MemoryModuleType::IsPanicking, MemoryStatus::ValueAbsent),
        ]
    }

    fn set_state(&mut self, state: TransportState) {
        self.state = state;
    }

    fn stop_in_place<W: AiWorld>(cx: &mut BehaviorCx<'_, '_, W>) {
        cx.ai.stop_navigation();
        cx.ai.brain.movement.stop_in_place();
        let y = cx.ai.body.velocity.y;
        cx.ai.body.velocity = Vec3::new(0.0, y, 0.0);
    }

    fn stop_targeting_current_target<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>) {
        self.ticks_since_reaching_target = 0;
        self.target = None;
        cx.ai.stop_navigation();
        cx.brain.memories.erase(MemoryModuleType::WalkTarget);
    }

    fn clear_memories_after_matching_target_found<W: AiWorld>(
        &mut self,
        cx: &mut BehaviorCx<'_, '_, W>,
    ) {
        self.stop_targeting_current_target(cx);
        cx.brain
            .memories
            .erase(MemoryModuleType::VisitedBlockPositions);
        cx.brain
            .memories
            .erase(MemoryModuleType::UnreachableTransportBlockPositions);
    }

    fn enter_cooldown_after_no_matching_target_found<W: AiWorld>(
        &mut self,
        cx: &mut BehaviorCx<'_, '_, W>,
    ) {
        self.stop_targeting_current_target(cx);
        cx.brain.memories.set(
            MemoryModuleType::TransportItemsCooldownTicks,
            MemoryValue::Int(COOLDOWN_TICKS),
        );
        cx.brain
            .memories
            .erase(MemoryModuleType::VisitedBlockPositions);
        cx.brain
            .memories
            .erase(MemoryModuleType::UnreachableTransportBlockPositions);
    }

    fn on_start_travelling<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>) {
        cx.ai
            .act(MobAction::CopperGolem(CopperGolemAction::ClearOpenedChest));
        cx.ai
            .act(MobAction::CopperGolem(CopperGolemAction::SetState(
                GolemState::Idle,
            )));
        self.set_state(TransportState::Travelling);
        self.interaction = None;
        self.ticks_since_reaching_target = 0;
    }

    fn set_visited_block_pos<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, pos: BlockPos) {
        let mut visited = positions(cx, MemoryModuleType::VisitedBlockPositions);
        let entry = global(cx, pos);
        if !visited.contains(&entry) {
            visited.push(entry);
        }
        if visited.len() <= MAX_VISITED {
            cx.brain.memories.set_with_expiry(
                MemoryModuleType::VisitedBlockPositions,
                MemoryValue::GlobalPosList(visited),
                MEMORY_TICKS,
            );
        } else {
            self.enter_cooldown_after_no_matching_target_found(cx);
        }
    }

    fn mark_visited_block_pos_as_unreachable<W: AiWorld>(
        &mut self,
        cx: &mut BehaviorCx<'_, '_, W>,
        pos: BlockPos,
    ) {
        let entry = global(cx, pos);
        let mut visited = positions(cx, MemoryModuleType::VisitedBlockPositions);
        visited.retain(|known| *known != entry);
        let mut unreachable = positions(cx, MemoryModuleType::UnreachableTransportBlockPositions);
        if !unreachable.contains(&entry) {
            unreachable.push(entry);
        }
        if unreachable.len() <= MAX_UNREACHABLE {
            cx.brain.memories.set_with_expiry(
                MemoryModuleType::VisitedBlockPositions,
                MemoryValue::GlobalPosList(visited),
                MEMORY_TICKS,
            );
            cx.brain.memories.set_with_expiry(
                MemoryModuleType::UnreachableTransportBlockPositions,
                MemoryValue::GlobalPosList(unreachable),
                MEMORY_TICKS,
            );
        } else {
            self.enter_cooldown_after_no_matching_target_found(cx);
        }
    }

    fn is_wanted_block<W: AiWorld>(cx: &BehaviorCx<'_, '_, W>, state: StateId) -> bool {
        if is_picking_up(cx) {
            is_source(state)
        } else {
            is_destination(state)
        }
    }

    fn is_position_already_visited<W: AiWorld>(
        cx: &BehaviorCx<'_, '_, W>,
        visited: &[GlobalPos],
        unreachable: &[GlobalPos],
        target: &TransportTarget,
    ) -> bool {
        connected_targets(cx.ai.world, target).iter().any(|part| {
            let entry = global(cx, part.pos);
            visited.contains(&entry) || unreachable.contains(&entry)
        })
    }

    fn is_another_mob_interacting_with_target<W: AiWorld>(
        cx: &BehaviorCx<'_, '_, W>,
        target: &TransportTarget,
    ) -> bool {
        connected_targets(cx.ai.world, target)
            .iter()
            .any(|part| !cx.ai.world.container_openers(part.pos).is_empty())
    }

    fn search_distances<W: AiWorld>(cx: &BehaviorCx<'_, '_, W>) -> (i32, i32) {
        if cx.ai.mob.passenger {
            (1, 1)
        } else {
            (HORIZONTAL_SEARCH_DISTANCE, VERTICAL_SEARCH_DISTANCE)
        }
    }

    fn is_target_valid_to_pick<W: AiWorld>(
        cx: &BehaviorCx<'_, '_, W>,
        pos: BlockPos,
        visited: &[GlobalPos],
        unreachable: &[GlobalPos],
    ) -> Option<TransportTarget> {
        let (horizontal, vertical) = Self::search_distances(cx);
        let origin = cx.ai.mob.block_pos();
        if (pos.x - origin.x).abs() > horizontal
            || (pos.z - origin.z).abs() > horizontal
            || (pos.y - origin.y).abs() > vertical
        {
            return None;
        }
        let target = try_create(cx.ai.world, pos)?;
        let wanted = Self::is_wanted_block(cx, target.state);
        let seen = Self::is_position_already_visited(cx, visited, unreachable, &target);
        let locked = cx.ai.world.container_locked(target.pos);
        (wanted && !seen && !locked).then_some(target)
    }

    fn transport_target<W: AiWorld>(&self, cx: &BehaviorCx<'_, '_, W>) -> Option<TransportTarget> {
        let visited = positions(cx, MemoryModuleType::VisitedBlockPositions);
        let unreachable = positions(cx, MemoryModuleType::UnreachableTransportBlockPositions);
        let (horizontal, _) = Self::search_distances(cx);
        let center = cx.ai.mob.block_pos().chunk();
        let radius = horizontal.div_euclid(CHUNK_SIZE) + 1;
        let position = cx.ai.mob.position;
        let mut best: Option<TransportTarget> = None;
        let mut best_distance = f64::from(f32::MAX);
        for z in center.z - radius..=center.z + radius {
            for x in center.x - radius..=center.x + radius {
                let Some(chests) = cx.ai.world.chests_in_chunk(ChunkPos::new(x, z)) else {
                    continue;
                };
                for pos in chests {
                    let distance = pos.center().distance_squared(position);
                    if distance < best_distance
                        && let Some(target) =
                            Self::is_target_valid_to_pick(cx, pos, &visited, &unreachable)
                    {
                        best_distance = distance;
                        best = Some(target);
                    }
                }
            }
        }
        best
    }

    fn position_to_reach_target_from<W: AiWorld>(
        cx: &BehaviorCx<'_, '_, W>,
        path: Option<&crate::Path>,
    ) -> Vec3 {
        let base = match path.and_then(crate::Path::end_node) {
            Some(node) => Vec3::new(
                f64::from(node.pos.x) + 0.5,
                f64::from(node.pos.y),
                f64::from(node.pos.z) + 0.5,
            ),
            None => cx.ai.mob.position,
        };
        middle_y(cx, base)
    }

    fn has_valid_travelling_path<W: AiWorld>(
        cx: &mut BehaviorCx<'_, '_, W>,
        target: &TransportTarget,
    ) -> bool {
        let path = match cx.ai.brain.navigation.path().cloned() {
            Some(path) => Some(path),
            None => cx.ai.create_path_to(target.pos, 0),
        };
        let from = Self::position_to_reach_target_from(cx, path.as_ref());
        let within = within_target_distance(cx, interaction_range(cx), target, from);
        (path.is_none() && !within) || (within && can_see_any_side(cx, target, from))
    }

    fn has_valid_target<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>) -> bool {
        let Some(target) = self.target else {
            return false;
        };
        let unchanged = cx
            .ai
            .world
            .state(target.pos)
            .is_some_and(|state| state.kind() == target.state.kind());
        if !Self::is_wanted_block(cx, target.state) || !unchanged {
            return false;
        }
        if is_chest_blocked_at(cx.ai.world, target.pos) {
            return false;
        }
        if self.state != TransportState::Travelling {
            return true;
        }
        if Self::has_valid_travelling_path(cx, &target) {
            return true;
        }
        self.mark_visited_block_pos_as_unreachable(cx, target.pos);
        false
    }

    fn update_invalid_target<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>) -> bool {
        if let Some(verdict) = self.verdict.take()
            && let Some(current) = self.target
        {
            match verdict {
                TargetVerdict::Cancel(pos) if pos == current.pos => {
                    self.enter_cooldown_after_no_matching_target_found(cx);
                    return true;
                }
                TargetVerdict::Retarget { from, to } if from == current.pos => {
                    self.stop_targeting_current_target(cx);
                    match try_create(cx.ai.world, to) {
                        Some(target) => {
                            self.target = Some(target);
                            self.on_start_travelling(cx);
                            self.set_visited_block_pos(cx, to);
                        }
                        None => self.enter_cooldown_after_no_matching_target_found(cx),
                    }
                    return true;
                }
                _ => {}
            }
        }
        if self.has_valid_target(cx) {
            return false;
        }
        self.stop_targeting_current_target(cx);
        match self.transport_target(cx) {
            Some(target) => {
                self.target = Some(target);
                cx.ai
                    .act(MobAction::CopperGolem(CopperGolemAction::TargetBlock(
                        target.pos,
                    )));
                self.on_start_travelling(cx);
                self.set_visited_block_pos(cx, target.pos);
                true
            }
            None => {
                self.enter_cooldown_after_no_matching_target_found(cx);
                true
            }
        }
    }

    fn walk_towards_target<W: AiWorld>(&self, cx: &mut BehaviorCx<'_, '_, W>) {
        if let Some(target) = self.target {
            set_walk_and_look_targets(cx, target.pos, SPEED_MODIFIER, 0);
        }
    }

    fn start_queuing<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>) {
        Self::stop_in_place(cx);
        self.set_state(TransportState::Queuing);
    }

    fn resume_travelling<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>) {
        self.set_state(TransportState::Travelling);
        self.walk_towards_target(cx);
    }

    fn on_queuing_for_target<W: AiWorld>(
        &mut self,
        cx: &mut BehaviorCx<'_, '_, W>,
        target: &TransportTarget,
    ) {
        if !Self::is_another_mob_interacting_with_target(cx, target) {
            self.resume_travelling(cx);
        }
    }

    fn on_travel_to_target<W: AiWorld>(
        &mut self,
        cx: &mut BehaviorCx<'_, '_, W>,
        target: &TransportTarget,
    ) {
        let center = center_pos(cx);
        if within_target_distance(cx, QUEUE_DISTANCE, target, center)
            && Self::is_another_mob_interacting_with_target(cx, target)
        {
            self.start_queuing(cx);
        } else if within_target_distance(cx, interaction_range(cx), target, center_pos(cx)) {
            self.start_on_reached_target_interaction(cx, target);
        } else {
            self.walk_towards_target(cx);
        }
    }

    fn container_slots<W: AiWorld>(
        cx: &BehaviorCx<'_, '_, W>,
        target: &TransportTarget,
    ) -> Option<Vec<Slot>> {
        cx.ai
            .world
            .chest_container(target.pos, true)
            .map(|view| view.slots)
    }

    fn interaction_for<W: AiWorld>(cx: &BehaviorCx<'_, '_, W>, slots: &[Slot]) -> InteractionState {
        if is_picking_up(cx) {
            if container::is_empty(slots) {
                InteractionState::PickupNoItem
            } else {
                InteractionState::PickupItem
            }
        } else {
            let leaves = container::is_empty(slots)
                || cx
                    .ai
                    .world
                    .main_hand_stack(cx.ai.mob.id)
                    .is_some_and(|hand| container::has_item_matching(slots, &hand));
            if leaves {
                InteractionState::PlaceItem
            } else {
                InteractionState::PlaceNoItem
            }
        }
    }

    fn start_on_reached_target_interaction<W: AiWorld>(
        &mut self,
        cx: &mut BehaviorCx<'_, '_, W>,
        target: &TransportTarget,
    ) {
        if let Some(slots) = Self::container_slots(cx, target) {
            self.interaction = Some(Self::interaction_for(cx, &slots));
        }
        self.set_state(TransportState::Interacting);
    }

    fn pick_up_items<W: AiWorld>(
        &mut self,
        cx: &mut BehaviorCx<'_, '_, W>,
        target: &TransportTarget,
        mut slots: Vec<Slot>,
    ) {
        let before = slots.clone();
        let taken = container::pick_up(&mut slots);
        cx.ai
            .act(MobAction::CopperGolem(CopperGolemAction::SetContainer {
                pos: target.pos,
                slots: container::changed(&before, &slots),
            }));
        cx.ai
            .act(MobAction::CopperGolem(CopperGolemAction::SetMainHand {
                stack: taken,
                guaranteed: true,
            }));
        self.clear_memories_after_matching_target_found(cx);
    }

    fn put_down_item<W: AiWorld>(
        &mut self,
        cx: &mut BehaviorCx<'_, '_, W>,
        target: &TransportTarget,
        mut slots: Vec<Slot>,
    ) {
        let before = slots.clone();
        let held = cx.ai.world.main_hand_stack(cx.ai.mob.id);
        let rest = container::add_items(&mut slots, held);
        cx.ai
            .act(MobAction::CopperGolem(CopperGolemAction::SetContainer {
                pos: target.pos,
                slots: container::changed(&before, &slots),
            }));
        let emptied = rest.is_none();
        cx.ai
            .act(MobAction::CopperGolem(CopperGolemAction::SetMainHand {
                stack: rest,
                guaranteed: false,
            }));
        if emptied {
            self.clear_memories_after_matching_target_found(cx);
        } else {
            self.stop_targeting_current_target(cx);
        }
    }

    fn do_reached_target_interaction<W: AiWorld>(
        &mut self,
        cx: &mut BehaviorCx<'_, '_, W>,
        target: &TransportTarget,
    ) {
        let Some(slots) = Self::container_slots(cx, target) else {
            return;
        };
        match Self::interaction_for(cx, &slots) {
            InteractionState::PickupItem => self.pick_up_items(cx, target, slots),
            InteractionState::PlaceItem => self.put_down_item(cx, target, slots),
            InteractionState::PickupNoItem | InteractionState::PlaceNoItem => {
                self.stop_targeting_current_target(cx);
            }
        }
    }

    fn on_target_interaction<W: AiWorld>(
        &mut self,
        cx: &mut BehaviorCx<'_, '_, W>,
        target: &TransportTarget,
    ) {
        cx.brain.memories.set(
            MemoryModuleType::LookTarget,
            MemoryValue::Tracker(crate::behavior::PositionTracker::of_block(target.pos)),
        );
        Self::stop_in_place(cx);
        let Some(interaction) = self.interaction else {
            return;
        };
        let (golem, sound) = interaction.golem();
        let ticks = self.ticks_since_reaching_target;
        if ticks == OPEN_TICK {
            cx.ai
                .act(MobAction::CopperGolem(CopperGolemAction::OpenContainer(
                    target.pos,
                )));
            cx.ai
                .act(MobAction::CopperGolem(CopperGolemAction::SetState(golem)));
        }
        if ticks == SOUND_TICK {
            cx.ai.act(MobAction::Sound {
                sound,
                volume: 1.0,
                pitch: 1.0,
            });
        }
        if ticks == INTERACTION_TICKS {
            let own_container = cx
                .ai
                .world
                .chest_container(target.pos, true)
                .is_some_and(|view| !view.compound);
            if own_container
                && cx
                    .ai
                    .world
                    .container_openers(target.pos)
                    .contains(&cx.ai.mob.id)
            {
                cx.ai
                    .act(MobAction::CopperGolem(CopperGolemAction::CloseContainer(
                        target.pos,
                    )));
            }
            cx.ai
                .act(MobAction::CopperGolem(CopperGolemAction::ClearOpenedChest));
        }
    }

    fn on_reached_target<W: AiWorld>(
        &mut self,
        cx: &mut BehaviorCx<'_, '_, W>,
        target: &TransportTarget,
    ) {
        if within_target_distance(cx, REACHED_DISTANCE, target, center_pos(cx)) {
            self.ticks_since_reaching_target += 1;
            self.on_target_interaction(cx, target);
            if self.ticks_since_reaching_target >= INTERACTION_TICKS {
                self.do_reached_target_interaction(cx, target);
                self.on_start_travelling(cx);
            }
        } else {
            self.on_start_travelling(cx);
        }
    }
}

impl Behavior for TransportItemsBetweenContainers {
    fn entry_condition(&self) -> &[Condition] {
        const CONDITIONS: [Condition; 4] = [
            (
                MemoryModuleType::VisitedBlockPositions,
                MemoryStatus::Registered,
            ),
            (
                MemoryModuleType::UnreachableTransportBlockPositions,
                MemoryStatus::Registered,
            ),
            (
                MemoryModuleType::TransportItemsCooldownTicks,
                MemoryStatus::ValueAbsent,
            ),
            (MemoryModuleType::IsPanicking, MemoryStatus::ValueAbsent),
        ];
        &CONDITIONS
    }

    fn check_extra_start_conditions<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>) -> bool {
        !cx.ai.mob.leashed
    }

    fn start<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        cx.ai
            .brain
            .navigation
            .set_path_to_targets_below_surface(true);
    }

    fn can_still_use<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        !cx.brain
            .memories
            .has_value(MemoryModuleType::TransportItemsCooldownTicks)
            && !is_panicking(cx)
            && !cx.ai.mob.leashed
    }

    fn timed_out(&self, _end_timestamp: i64, _now: i64) -> bool {
        false
    }

    fn tick<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, now: i64) {
        let invalid = self.update_invalid_target(cx);
        let Some(target) = self.target else {
            self.stop(cx, now);
            return;
        };
        if invalid {
            return;
        }
        if self.state == TransportState::Queuing {
            self.on_queuing_for_target(cx, &target);
        }
        if self.state == TransportState::Travelling {
            self.on_travel_to_target(cx, &target);
        }
        if self.state == TransportState::Interacting {
            self.on_reached_target(cx, &target);
        }
    }

    fn stop<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        self.on_start_travelling(cx);
        cx.ai
            .brain
            .navigation
            .set_path_to_targets_below_surface(false);
    }
}
