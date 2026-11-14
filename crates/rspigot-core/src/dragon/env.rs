use rspigot_ai::clip_misses;
use rspigot_domain::dragon::phase::{DragonEnv, Foe, Ground, Targeting};
use rspigot_domain::random::RandomSource;
use rspigot_domain::spawn::SpawnLevel;
use rspigot_domain::{BlockPos, EntityId, Vec3};
use rspigot_entities::SpawnHeightmap;

use crate::mobs::ServerRandom;
use crate::ports::ClientLink;
use crate::server::Server;

const VISIBILITY_FLOOR: f64 = 2.0;

pub(super) struct DragonWorld<'a, L: ClientLink> {
    pub(super) server: &'a mut Server<L>,
    pub(super) eye: Vec3,
    pub(super) position: Vec3,
}

impl<L: ClientLink> RandomSource for DragonWorld<'_, L> {
    fn next_float(&mut self) -> f32 {
        ServerRandom(&mut self.server.rng).next_float()
    }

    fn next_int(&mut self, bound: i32) -> i32 {
        ServerRandom(&mut self.server.rng).next_int(bound)
    }

    fn next_double(&mut self) -> f64 {
        ServerRandom(&mut self.server.rng).next_double()
    }

    fn next_boolean(&mut self) -> bool {
        ServerRandom(&mut self.server.rng).next_boolean()
    }

    fn next_gaussian(&mut self) -> f64 {
        ServerRandom(&mut self.server.rng).next_gaussian()
    }
}

fn foe_of(living: &rspigot_ai::Living) -> Foe {
    Foe {
        id: living.id,
        position: living.position,
        height: living.bounds.max_y - living.bounds.min_y,
    }
}

impl<L: ClientLink> DragonEnv for DragonWorld<'_, L> {
    fn ground(&self, ground: Ground, x: i32, z: i32) -> i32 {
        let heightmap = match ground {
            Ground::NoLeaves => SpawnHeightmap::MotionBlockingNoLeaves,
            Ground::Blocking => SpawnHeightmap::MotionBlocking,
        };
        self.server.top_height(heightmap, x, z)
    }

    fn nearest_player(&self, targeting: Targeting, near: Vec3) -> Option<Foe> {
        let range = targeting.range();
        let check_sight = matches!(targeting, Targeting::Scan | Targeting::Charge);
        self.server
            .entities
            .mobs
            .players
            .iter()
            .filter(|player| player.can_be_seen_as_enemy())
            .filter(|player| {
                targeting
                    .height_limit()
                    .is_none_or(|limit| (player.position.y - self.position.y).abs() <= limit)
            })
            .filter(|player| {
                if range <= 0.0 {
                    return true;
                }
                let reach = (range * player.visibility_percent()).max(VISIBILITY_FLOOR);
                player.position.distance_squared(self.position) <= reach * reach
            })
            .filter(|player| {
                !check_sight
                    || clip_misses(
                        &self.server.chunk_view(),
                        self.eye,
                        Vec3::new(player.position.x, player.eye_y(), player.position.z),
                    )
            })
            .min_by(|a, b| {
                a.position
                    .distance_squared(near)
                    .total_cmp(&b.position.distance_squared(near))
            })
            .map(foe_of)
    }

    fn foe(&self, id: EntityId) -> Option<Foe> {
        self.server
            .entities
            .mobs
            .players
            .iter()
            .find(|player| player.id == id)
            .map(foe_of)
    }

    fn has_line_of_sight(&self, id: EntityId) -> bool {
        self.server
            .entities
            .mobs
            .players
            .iter()
            .find(|player| player.id == id)
            .is_some_and(|player| {
                clip_misses(
                    &self.server.chunk_view(),
                    self.eye,
                    Vec3::new(player.position.x, player.eye_y(), player.position.z),
                )
            })
    }

    fn is_empty_block(&self, pos: BlockPos) -> bool {
        self.server
            .world
            .chunks
            .block(pos)
            .is_some_and(|state| state.is_air())
    }
}
