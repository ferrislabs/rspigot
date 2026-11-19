use std::collections::BTreeMap;

use super::{PlayerId, TeamId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Team {
    id: TeamId,
    lives: BTreeMap<PlayerId, u32>,
}

impl Team {
    pub fn new(id: TeamId) -> Self {
        Self {
            id,
            lives: BTreeMap::new(),
        }
    }

    pub fn id(&self) -> TeamId {
        self.id
    }

    pub fn len(&self) -> usize {
        self.lives.len()
    }

    pub fn is_empty(&self) -> bool {
        self.lives.is_empty()
    }

    pub fn contains(&self, player: PlayerId) -> bool {
        self.lives.contains_key(&player)
    }

    pub fn members(&self) -> impl Iterator<Item = PlayerId> + '_ {
        self.lives.keys().copied()
    }

    pub fn standing(&self) -> usize {
        self.lives.values().filter(|lives| **lives > 0).count()
    }

    pub fn lives_of(&self, player: PlayerId) -> Option<u32> {
        self.lives.get(&player).copied()
    }

    pub(super) fn add(&mut self, player: PlayerId, lives: u32) {
        self.lives.insert(player, lives);
    }

    pub(super) fn remove(&mut self, player: PlayerId) -> bool {
        self.lives.remove(&player).is_some()
    }

    pub(super) fn lose_life(&mut self, player: PlayerId) -> Option<u32> {
        let lives = self.lives.get_mut(&player)?;
        *lives = lives.saturating_sub(1);
        Some(*lives)
    }
}
