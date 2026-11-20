use rspigot_domain::entity::EntityBase;
use rspigot_domain::meta::{MetaError, MetaValue};
use rspigot_domain::player::PlayerId;
use rspigot_domain::{BlockPos, EntityId};

use crate::plugins::{ActionError, ActionErrorKind, EntityView};
use crate::ports::ClientLink;
use crate::server::Server;

pub const MAX_FOUND: usize = 1000;

#[derive(Debug, Clone, PartialEq)]
pub enum MetaChange {
    Set { key: String, value: MetaValue },
    Remove(String),
    Clear,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MetaOrder {
    pub volatile: bool,
    pub change: MetaChange,
}

impl MetaOrder {
    pub const fn name(&self) -> &'static str {
        match self.change {
            MetaChange::Set { .. } => "meta.set",
            MetaChange::Remove(_) => "meta.remove",
            MetaChange::Clear => "meta.clear",
        }
    }
}

pub type MetaEntries = Vec<(String, MetaValue)>;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct MetaSnapshot(Option<Box<(MetaEntries, MetaEntries)>>);

impl MetaSnapshot {
    pub fn new(persistent: MetaEntries, volatile: MetaEntries) -> Self {
        if persistent.is_empty() && volatile.is_empty() {
            Self(None)
        } else {
            Self(Some(Box::new((persistent, volatile))))
        }
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_none()
    }

    pub fn persistent(&self) -> &[(String, MetaValue)] {
        self.0.as_ref().map_or(&[], |sets| &sets.0)
    }

    pub fn volatile(&self) -> &[(String, MetaValue)] {
        self.0.as_ref().map_or(&[], |sets| &sets.1)
    }

    pub fn of(base: &EntityBase) -> Self {
        if !base.meta.has_visible() && !base.volatile_meta.has_visible() {
            return Self::default();
        }
        Self::new(base.meta.entries(), base.volatile_meta.entries())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct BlockMetaView {
    pub pos: BlockPos,
    pub meta: MetaSnapshot,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MetaHolder {
    Player(PlayerId),
    Entity(EntityId),
    Block(BlockPos),
}

fn failure(error: &MetaError) -> ActionError {
    let kind = if error.is_limit() {
        ActionErrorKind::LimitReached
    } else {
        ActionErrorKind::InvalidArgument
    };
    ActionError::new(kind, error.to_string())
}

fn apply(meta: &mut rspigot_domain::meta::Metadata, change: MetaChange) -> Result<(), ActionError> {
    match change {
        MetaChange::Set { key, value } => meta.set(&key, value).map_err(|e| failure(&e)),
        MetaChange::Remove(key) => {
            meta.remove(&key);
            Ok(())
        }
        MetaChange::Clear => {
            meta.clear();
            Ok(())
        }
    }
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn entity_meta_change(
        &mut self,
        id: EntityId,
        order: MetaOrder,
    ) -> Result<(), ActionError> {
        let entity =
            self.entities.store.get_mut(id).ok_or_else(|| {
                ActionError::new(ActionErrorKind::TargetGone, "the entity is gone")
            })?;
        let base = &mut entity.base;
        let meta = if order.volatile {
            &mut base.volatile_meta
        } else {
            &mut base.meta
        };
        apply(meta, order.change)
    }

    pub(crate) fn player_meta_change(
        &mut self,
        id: PlayerId,
        order: MetaOrder,
    ) -> Result<(), ActionError> {
        let player = self
            .players
            .get_mut(&id)
            .ok_or_else(ActionError::player_gone)?;
        let meta = if order.volatile {
            &mut player.volatile_meta
        } else {
            &mut player.meta
        };
        apply(meta, order.change)
    }

    pub(super) fn players_with_meta(
        &self,
        key: &str,
        volatile: bool,
    ) -> Vec<crate::plugins::PlayerSnapshot> {
        let mut found: Vec<&crate::player::Player<L>> = self
            .players
            .values()
            .filter(|player| {
                let meta = if volatile {
                    &player.volatile_meta
                } else {
                    &player.meta
                };
                meta.contains(key)
            })
            .collect();
        found.sort_by_key(|player| player.entity_id);
        found
            .into_iter()
            .take(MAX_FOUND)
            .map(|player| self.describe(player))
            .collect()
    }

    pub(crate) fn block_meta_change(
        &mut self,
        pos: BlockPos,
        order: MetaOrder,
    ) -> Result<(), ActionError> {
        let height = self.world.dimension.height;
        if pos.y < height.min_y || pos.y > height.max_y() {
            return Err(ActionError::new(
                ActionErrorKind::InvalidArgument,
                format!("y {} is outside the world", pos.y),
            ));
        }
        let chunks = &mut self.world.chunks;
        let gone = || ActionError::new(ActionErrorKind::NotFound, "the chunk is not loaded");
        let volatile = order.volatile;
        match order.change {
            MetaChange::Set { key, value } => chunks
                .set_block_meta(pos, &key, value, volatile)
                .ok_or_else(gone)?
                .map_err(|error| failure(&error)),
            MetaChange::Remove(key) => {
                if !chunks.is_loaded(pos.chunk()) {
                    return Err(gone());
                }
                chunks.remove_block_meta(pos, &key, volatile);
                Ok(())
            }
            MetaChange::Clear => {
                if !chunks.is_loaded(pos.chunk()) {
                    return Err(gone());
                }
                chunks.clear_block_meta(pos, volatile);
                Ok(())
            }
        }
    }

    pub(super) fn meta_entries(
        &self,
        holder: &MetaHolder,
        key: Option<&str>,
        volatile: bool,
    ) -> Option<Vec<(String, MetaValue)>> {
        let all = match holder {
            MetaHolder::Player(id) => {
                let player = self.players.get(id)?;
                if volatile {
                    player.volatile_meta.entries()
                } else {
                    player.meta.entries()
                }
            }
            MetaHolder::Entity(id) => {
                let base = &self.entities.store.get(*id)?.base;
                if volatile {
                    base.volatile_meta.entries()
                } else {
                    base.meta.entries()
                }
            }
            MetaHolder::Block(pos) => {
                if !self.world.chunks.is_loaded(pos.chunk()) {
                    return None;
                }
                self.world
                    .chunks
                    .block_meta(*pos)
                    .map(|held| {
                        if volatile {
                            held.volatile.entries()
                        } else {
                            held.persistent.entries()
                        }
                    })
                    .unwrap_or_default()
            }
        };
        Some(match key {
            Some(key) => all.into_iter().filter(|(held, _)| held == key).collect(),
            None => all,
        })
    }

    pub(super) fn blocks_with_meta(
        &self,
        key: &str,
        value: Option<&MetaValue>,
        volatile: bool,
    ) -> Vec<BlockPos> {
        self.world
            .chunks
            .blocks_with_meta(key, value, volatile, MAX_FOUND)
    }

    pub(super) fn entities_with_meta(&self, key: &str, volatile: bool) -> Vec<EntityView> {
        let mut views: Vec<EntityView> = self
            .entities
            .store
            .iter()
            .filter(|entity| {
                let meta = if volatile {
                    &entity.base.volatile_meta
                } else {
                    &entity.base.meta
                };
                meta.contains(key)
            })
            .map(crate::plugins::spawns::view_of_entity)
            .collect();
        views.sort_by_key(|view| view.id);
        views.truncate(MAX_FOUND);
        views
    }

    pub(crate) fn blocks_meta_of(
        &self,
        payload: &crate::plugins::EventPayload,
    ) -> Vec<BlockMetaView> {
        if !self.world.chunks.any_block_meta() {
            return Vec::new();
        }
        let mut positions = Vec::new();
        payload.block_positions(&mut positions);
        self.block_metas_of(&positions)
    }

    pub(crate) fn block_metas_of(&self, positions: &[BlockPos]) -> Vec<BlockMetaView> {
        let mut seen: Vec<BlockPos> = Vec::new();
        let mut views = Vec::new();
        for pos in positions {
            if seen.contains(pos) {
                continue;
            }
            seen.push(*pos);
            if let Some(held) = self.world.chunks.block_meta(*pos) {
                let meta = MetaSnapshot::new(held.persistent.entries(), held.volatile.entries());
                if !meta.is_empty() {
                    views.push(BlockMetaView { pos: *pos, meta });
                }
            }
        }
        views
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rspigot_domain::entity::EntityBase;
    use rspigot_entities::EntityKind;
    use uuid::Uuid;

    fn base() -> EntityBase {
        EntityBase::new(EntityId(1), Uuid::from_u128(1), EntityKind::Zombie)
    }

    #[test]
    fn an_untagged_entity_has_an_empty_snapshot() {
        assert!(MetaSnapshot::of(&base()).is_empty());
    }

    #[test]
    fn a_tagged_entity_snapshot_holds_both_kinds() {
        let mut tagged = base();
        tagged.meta.set("a:b", MetaValue::Int(1)).unwrap();
        tagged
            .volatile_meta
            .set("a:c", MetaValue::Bool(true))
            .unwrap();
        let snapshot = MetaSnapshot::of(&tagged);
        assert_eq!(
            snapshot.persistent(),
            [("a:b".to_owned(), MetaValue::Int(1))]
        );
        assert_eq!(
            snapshot.volatile(),
            [("a:c".to_owned(), MetaValue::Bool(true))]
        );
    }

    #[test]
    fn limits_map_to_limit_reached_and_bad_keys_to_invalid_argument() {
        assert_eq!(
            failure(&MetaError::TooManyKeys).kind,
            ActionErrorKind::LimitReached
        );
        assert_eq!(
            failure(&MetaError::InvalidKey("x".to_owned())).kind,
            ActionErrorKind::InvalidArgument
        );
    }
}
