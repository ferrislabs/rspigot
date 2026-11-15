use rspigot_domain::player::PlayerId;
use rspigot_domain::world::Difficulty;

use crate::events::ClientEvent;
use crate::ports::ClientLink;
use crate::server::{Audience, Server};

impl<L: ClientLink> Server<L> {
    pub(crate) fn handle_change_difficulty(&mut self, id: PlayerId, difficulty: Difficulty) {
        if self.is_operator_player(id) {
            self.apply_difficulty(difficulty, false);
        }
    }

    pub(crate) fn handle_lock_difficulty(&mut self, id: PlayerId, locked: bool) {
        if !self.is_operator_player(id) {
            return;
        }
        self.difficulty_locked = locked;
        self.announce_difficulty();
    }

    pub(crate) fn apply_difficulty(&mut self, difficulty: Difficulty, forced: bool) {
        if !forced && self.difficulty_locked {
            return;
        }
        let difficulty = if self.config.hardcore {
            Difficulty::Hard
        } else {
            difficulty
        };
        self.config.difficulty = difficulty;
        self.world.difficulty = difficulty;
        for state in self.dormant.values_mut() {
            state.world.difficulty = difficulty;
        }
        self.announce_difficulty();
    }

    fn announce_difficulty(&mut self) {
        let event = ClientEvent::Difficulty {
            difficulty: self.world.difficulty,
            locked: self.difficulty_locked,
        };
        self.push(Audience::All, event);
    }

    fn is_operator_player(&self, id: PlayerId) -> bool {
        self.players.get(&id).is_some_and(|player| player.operator)
    }
}
