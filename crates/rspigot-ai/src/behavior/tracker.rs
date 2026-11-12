use rspigot_domain::{BlockPos, EntityId, Vec3};

use crate::behavior::memory::Memories;
use crate::brain::AiContext;
use crate::world::AiWorld;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PositionTracker {
    Block {
        pos: BlockPos,
        center: Vec3,
    },
    Entity {
        id: EntityId,
        track_eye_height: bool,
        target_eye_height: bool,
    },
}

const ENTITY_EYE_HEIGHT_FACTOR: f64 = 0.85;

fn item_eye_height(item: &crate::world::DroppedItem) -> f64 {
    (item.bounds.max_y - item.bounds.min_y) * ENTITY_EYE_HEIGHT_FACTOR
}

impl PositionTracker {
    pub fn of_block(pos: BlockPos) -> Self {
        Self::Block {
            pos,
            center: Vec3::new(
                f64::from(pos.x) + 0.5,
                f64::from(pos.y) + 0.5,
                f64::from(pos.z) + 0.5,
            ),
        }
    }

    pub fn of_position(position: Vec3) -> Self {
        Self::Block {
            pos: position.block(),
            center: position,
        }
    }

    pub fn of_entity(id: EntityId, track_eye_height: bool) -> Self {
        Self::Entity {
            id,
            track_eye_height,
            target_eye_height: false,
        }
    }

    pub fn of_entity_targeting_eyes(id: EntityId, track_eye_height: bool) -> Self {
        Self::Entity {
            id,
            track_eye_height,
            target_eye_height: true,
        }
    }

    pub fn current_position(&self, world: &impl AiWorld) -> Option<Vec3> {
        match *self {
            Self::Block { center, .. } => Some(center),
            Self::Entity {
                id,
                track_eye_height,
                ..
            } => {
                if let Some(living) = world.living(id) {
                    return Some(if track_eye_height {
                        Vec3::new(
                            living.position.x,
                            living.position.y + living.eye_height,
                            living.position.z,
                        )
                    } else {
                        living.position
                    });
                }
                let item = world.dropped_item(id)?;
                Some(if track_eye_height {
                    Vec3::new(
                        item.position.x,
                        item.position.y + item_eye_height(&item),
                        item.position.z,
                    )
                } else {
                    item.position
                })
            }
        }
    }

    pub fn current_block_position(&self, world: &impl AiWorld) -> Option<BlockPos> {
        match *self {
            Self::Block { pos, .. } => Some(pos),
            Self::Entity {
                id,
                target_eye_height,
                ..
            } => {
                if let Some(living) = world.living(id) {
                    return Some(if target_eye_height {
                        Vec3::new(living.position.x, living.eye_y(), living.position.z).block()
                    } else {
                        living.position.block()
                    });
                }
                let item = world.dropped_item(id)?;
                Some(if target_eye_height {
                    Vec3::new(
                        item.position.x,
                        item.position.y + item_eye_height(&item),
                        item.position.z,
                    )
                    .block()
                } else {
                    item.position.block()
                })
            }
        }
    }

    pub fn is_visible_by<W: AiWorld>(
        &self,
        cx: &mut AiContext<'_, W>,
        memories: &Memories,
    ) -> bool {
        match *self {
            Self::Block { .. } => true,
            Self::Entity { id, .. } => {
                let Some(living) = cx.world.living(id) else {
                    return cx.world.dropped_item(id).is_some();
                };
                if !living.alive {
                    return false;
                }
                memories
                    .get(crate::behavior::MemoryModuleType::NearestVisibleLivingEntities)
                    .and_then(crate::behavior::MemoryValue::as_visible)
                    .is_some_and(|visible| visible.contains_entity(cx, memories, id))
            }
        }
    }
}
