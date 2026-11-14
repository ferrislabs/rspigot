use rspigot_domain::entity::{Aabb, EntityState, RemovalReason, item};
use rspigot_domain::{ChunkPos, EntityId};

use crate::plugins::events::{EntityRemoveCause, ItemMerge};
use crate::plugins::steps::MergeStep;
use crate::plugins::{Continuation, EventPayload, Lane};
use crate::ports::ClientLink;
use crate::server::Server;

const MERGE_RADIUS: f64 = 0.5;
const MOVED_INTERVAL: i32 = 2;
const IDLE_INTERVAL: i32 = 40;
const MERGE_AGE_LIMIT: i32 = 6000;

impl<L: ClientLink> Server<L> {
    pub(crate) fn merge_due(&self, id: EntityId) -> bool {
        let Some(entity) = self.entities.store.get(id) else {
            return false;
        };
        let base = &entity.base;
        let moved = base.old_position.block() != base.position.block();
        let interval = if moved { MOVED_INTERVAL } else { IDLE_INTERVAL };
        base.tick_count % interval == 0
    }

    fn item_mergable(&self, id: EntityId) -> bool {
        let Some(entity) = self.entities.store.get(id) else {
            return false;
        };
        let EntityState::Item(state) = &entity.state else {
            return false;
        };
        let Some(stack) = item::stack(&entity.base) else {
            return false;
        };
        !entity.base.is_removed()
            && state.pickup_delay != item::INFINITE_PICKUP_DELAY
            && state.age != item::INFINITE_LIFETIME
            && state.age < MERGE_AGE_LIMIT
            && u32::from(stack.count.get()) < rspigot_domain::item::max_stack(stack)
    }

    fn stacks_fit(&self, to: EntityId, from: EntityId) -> bool {
        let (Some(to), Some(from)) = (
            self.entities
                .store
                .get(to)
                .and_then(|e| item::stack(&e.base)),
            self.entities
                .store
                .get(from)
                .and_then(|e| item::stack(&e.base)),
        ) else {
            return false;
        };
        u32::from(to.count.get()) + u32::from(from.count.get())
            <= rspigot_domain::item::max_stack(from)
            && rspigot_domain::item::same_item_same_components(to, from)
    }

    fn item_neighbours(&self, id: EntityId) -> Vec<EntityId> {
        let Some(entity) = self.entities.store.get(id) else {
            return Vec::new();
        };
        let area: Aabb =
            entity
                .base
                .bounding_box()
                .inflate_by(MERGE_RADIUS, MERGE_RADIUS - 0.5, MERGE_RADIUS);
        let low = ChunkPos::from_block(area.min_x, area.min_z);
        let high = ChunkPos::from_block(area.max_x, area.max_z);
        let mut found = Vec::new();
        for x in low.x..=high.x {
            for z in low.z..=high.z {
                for other in self.entities.store.in_chunk(ChunkPos::new(x, z)) {
                    let Some(candidate) = self.entities.store.get(*other) else {
                        continue;
                    };
                    if *other != id
                        && matches!(candidate.state, EntityState::Item(_))
                        && candidate.base.bounding_box().intersects(&area)
                    {
                        found.push(*other);
                    }
                }
            }
        }
        found.sort_by_key(|other| other.0);
        found
    }

    pub(crate) fn merge_with_neighbours(&mut self, id: EntityId) {
        if !self.item_mergable(id) {
            return;
        }
        for other in self.item_neighbours(id) {
            if !self.item_mergable(id) {
                return;
            }
            if !self.item_mergable(other) || !self.stacks_fit(id, other) {
                continue;
            }
            let lane = Lane::Entity(id);
            let then = Continuation::Merge(MergeStep {
                from: other,
                to: id,
            });
            let parked = self.parks(lane, &then);
            self.fire(lane, then, move |server| {
                let entity = server.entity_view(other)?;
                Some(EventPayload::ItemMerge(ItemMerge {
                    entity_type: entity.kind.clone(),
                    entity,
                    target: server.entity_view(id)?,
                }))
            });
            if parked {
                return;
            }
        }
    }

    pub(crate) fn finish_item_merge(&mut self, step: MergeStep, cancelled: bool) {
        let MergeStep { from, to } = step;
        if cancelled
            || !self.item_mergable(to)
            || !self.item_mergable(from)
            || !self.stacks_fit(to, from)
        {
            return;
        }
        let Some((count, delay, age)) = self.entities.store.get(from).and_then(|entity| {
            let EntityState::Item(state) = &entity.state else {
                return None;
            };
            Some((
                item::stack(&entity.base)?.count.get(),
                state.pickup_delay,
                state.age,
            ))
        }) else {
            return;
        };
        if let Some(entity) = self.entities.store.get_mut(to)
            && let EntityState::Item(state) = &mut entity.state
            && let Some(mut stack) = item::stack(&entity.base).cloned()
        {
            stack.count = stack.count.saturating_add(count);
            state.pickup_delay = state.pickup_delay.max(delay);
            state.age = state.age.min(age);
            item::set_stack(&mut entity.base, Some(stack));
            entity.base.needs_sync = true;
        }
        self.remove_entity(
            from,
            RemovalReason::Discarded,
            Some(EntityRemoveCause::Merge),
        );
    }
}
