mod block_entity_index;
mod manager;
mod queue;
mod workers;

use rspigot_domain::world::Difficulty;

pub use manager::{ChunkEvents, ChunkManager, ChunkManagerConfig, ChunkManagerStats};
pub use queue::{JobQueue, Priority};
pub use workers::{ChunkJob, ChunkResult, ChunkWorkers, QueryAnswer, WorldQuery};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MobGeneration {
    pub seed: i64,
    pub zoom_seed: i64,
    pub difficulty: Difficulty,
}
