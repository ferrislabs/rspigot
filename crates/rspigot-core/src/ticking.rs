use rspigot_domain::ChunkPos;
use rspigot_domain::EntityId;
use rustc_hash::FxHashSet;

use crate::ports::ClientLink;
use crate::server::Server;

#[derive(Debug, Default)]
pub struct Ticking {
    centers: Vec<ChunkPos>,
    near: FxHashSet<ChunkPos>,
    entities: Vec<EntityId>,
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn refresh_ticking(&mut self) {
        let mut centers: Vec<ChunkPos> = self
            .players
            .values()
            .map(|player| player.position.chunk())
            .collect();
        centers.sort_unstable();
        centers.dedup();
        if centers != self.ticking.centers {
            let reach = i32::from(self.config.simulation_distance);
            let mut near = FxHashSet::default();
            for center in &centers {
                for dx in -reach..=reach {
                    for dz in -reach..=reach {
                        near.insert(ChunkPos::new(center.x + dx, center.z + dz));
                    }
                }
            }
            self.ticking.near = near;
            self.ticking.centers = centers;
        }
        let mut ids: Vec<EntityId> = Vec::new();
        let simulated = self.world.chunks.simulated_chunks();
        for chunk in self.ticking.near.iter().chain(simulated.iter()) {
            if self.entity_ticking(*chunk) {
                ids.extend_from_slice(self.entities.store.in_chunk(*chunk));
            }
        }
        ids.sort_unstable();
        ids.dedup();
        self.ticking.entities = ids;
    }

    pub(crate) fn entity_ticking(&self, chunk: ChunkPos) -> bool {
        self.world.chunks.simulates(chunk)
            || (self.ticking.near.contains(&chunk) && self.world.chunks.neighbours_loaded(chunk))
    }

    pub(crate) fn chunk_ticking(&self, chunk: ChunkPos) -> bool {
        self.world.chunks.neighbours_loaded(chunk)
    }

    pub(crate) fn ticking_entities(&self) -> Vec<EntityId> {
        self.ticking.entities.clone()
    }
}
