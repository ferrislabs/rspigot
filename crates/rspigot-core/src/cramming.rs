use rand::Rng;
use rspigot_domain::damage::DamageType;
use rspigot_domain::entity::{Aabb, EntityState, living};
use rspigot_domain::player::{GameMode, PlayerId};
use rspigot_domain::{ChunkPos, EntityId};

use crate::health::DamageSource;
use crate::ports::ClientLink;
use crate::server::Server;

const CRAMMING_DAMAGE: f32 = 6.0;
const CRAMMING_ROLL: u32 = 4;
const PLAYER_WIDTH: f32 = 0.6;

#[derive(Debug, Clone, Copy)]
struct Crowd {
    total: usize,
    riding: usize,
}

impl Crowd {
    fn on_foot(self) -> usize {
        self.total - self.riding
    }
}

const fn crowded(limit: i32, total: usize) -> bool {
    limit > 0 && total as i64 > limit as i64 - 1
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn tick_cramming(&mut self) {
        let limit = self.config.rules.max_entity_cramming;
        if limit <= 0 {
            return;
        }
        for id in self.ticking_entities() {
            self.cram_mob(id, limit);
        }
        let ids: Vec<PlayerId> = self.players.keys().copied().collect();
        for id in ids {
            self.cram_player(id, limit);
        }
    }

    fn cram_mob(&mut self, id: EntityId, limit: i32) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        if !matches!(entity.state, EntityState::Living(_))
            || !entity.is_simulated()
            || !self.entity_ticking(entity.base.chunk())
            || !self.living_entity_alive(id)
        {
            return;
        }
        let area = entity.base.bounding_box();
        let crowd = self.crowd_around(id, None, &area);
        if self.cramming_rolls(limit, crowd) {
            self.hurt_entity(
                id,
                &DamageSource::new(DamageType::Cramming),
                CRAMMING_DAMAGE,
            );
        }
    }

    fn cram_player(&mut self, id: PlayerId, limit: i32) {
        let Some(player) = self.players.get(&id) else {
            return;
        };
        if player.vitals.is_dead()
            || player.game_mode == GameMode::Spectator
            || player.dim != self.world.dimension.key
        {
            return;
        }
        let area = Aabb::of_size(player.position, PLAYER_WIDTH, player.height() as f32);
        let body = player.entity_id;
        let crowd = self.crowd_around(body, Some(id), &area);
        if self.cramming_rolls(limit, crowd) {
            self.hurt(
                id,
                &DamageSource::new(DamageType::Cramming),
                CRAMMING_DAMAGE,
            );
        }
    }

    fn cramming_rolls(&mut self, limit: i32, crowd: Crowd) -> bool {
        crowded(limit, crowd.total)
            && self.rng.random_range(0..CRAMMING_ROLL) == 0
            && crowded(limit, crowd.on_foot())
    }

    fn crowd_around(&self, me: EntityId, me_player: Option<PlayerId>, area: &Aabb) -> Crowd {
        let low = ChunkPos::from_block(area.min_x, area.min_z);
        let high = ChunkPos::from_block(area.max_x, area.max_z);
        let mut crowd = Crowd {
            total: 0,
            riding: 0,
        };
        for x in low.x..=high.x {
            for z in low.z..=high.z {
                for other in self.entities.store.in_chunk(ChunkPos::new(x, z)) {
                    let Some(candidate) = self.entities.store.get(*other) else {
                        continue;
                    };
                    let pushable = match &candidate.state {
                        EntityState::Living(state) => {
                            !living::is_dead_or_dying(&candidate.base, state)
                        }
                        EntityState::Boat(_) | EntityState::Minecart(_) => true,
                        _ => false,
                    };
                    if *other != me
                        && pushable
                        && !candidate.base.is_removed()
                        && candidate.base.bounding_box().intersects(area)
                    {
                        crowd.total += 1;
                        if self.entities.rides.is_passenger(*other) {
                            crowd.riding += 1;
                        }
                    }
                }
            }
        }
        for (key, player) in &self.players {
            if Some(*key) == me_player || player.entity_id == me {
                continue;
            }
            let body = Aabb::of_size(player.position, PLAYER_WIDTH, player.height() as f32);
            if player.game_mode != GameMode::Spectator
                && !player.vitals.is_dead()
                && player.dim == self.world.dimension.key
                && body.intersects(area)
            {
                crowd.total += 1;
                if self.entities.rides.is_passenger(player.entity_id) {
                    crowd.riding += 1;
                }
            }
        }
        crowd
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_crowd_needs_to_exceed_the_limit_minus_one_neighbours() {
        assert!(!crowded(24, 23));
        assert!(crowded(24, 24));
    }

    #[test]
    fn a_zero_limit_disables_cramming() {
        assert!(!crowded(0, 100));
    }
}
