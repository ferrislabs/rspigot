use std::collections::HashMap;

use rspigot_blocks::StateId;
use rspigot_domain::entity::Aabb;
use rspigot_domain::{BlockPos, EntityId};

use crate::BlockView;
use crate::world::{AiWorld, Living, TargetKind};

#[derive(Default)]
pub(crate) struct Scene {
    pub blocks: HashMap<BlockPos, StateId>,
    pub undarkened: i32,
    pub players: Vec<Living>,
    pub others: Vec<Living>,
    pub busy: Vec<Aabb>,
}

impl Scene {
    pub fn put(&mut self, pos: BlockPos, state: StateId) {
        self.blocks.insert(pos, state);
    }
}

impl From<HashMap<BlockPos, StateId>> for Scene {
    fn from(blocks: HashMap<BlockPos, StateId>) -> Self {
        Self {
            blocks,
            ..Self::default()
        }
    }
}

impl BlockView for Scene {
    fn state(&self, pos: BlockPos) -> Option<StateId> {
        Some(self.blocks.get(&pos).copied().unwrap_or(StateId::AIR))
    }
}

impl AiWorld for Scene {
    fn players(&self) -> &[Living] {
        &self.players
    }

    fn living(&self, id: EntityId) -> Option<Living> {
        self.players
            .iter()
            .chain(&self.others)
            .find(|living| living.id == id)
            .copied()
    }

    fn living_of(&self, kind: TargetKind, area: &Aabb) -> Vec<Living> {
        self.others
            .iter()
            .filter(|living| kind.matches(living.kind) && living.bounds.intersects(area))
            .copied()
            .collect()
    }

    fn difficulty(&self) -> i32 {
        2
    }

    fn game_time(&self) -> i64 {
        0
    }

    fn raw_brightness(&self, _pos: BlockPos) -> i32 {
        15
    }

    fn raw_brightness_undarkened(&self, _pos: BlockPos) -> i32 {
        self.undarkened
    }

    fn any_entity_in(&self, area: &Aabb, except: EntityId) -> bool {
        self.busy.iter().any(|busy| busy.intersects(area))
            || self
                .others
                .iter()
                .any(|living| living.id != except && living.bounds.intersects(area))
    }
}
