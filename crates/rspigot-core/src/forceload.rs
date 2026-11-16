use std::sync::Arc;

use rspigot_domain::ChunkPos;
use rspigot_nbt::{Compound, Tag};

use crate::chunks::{ChunkJob, Priority};
use crate::ports::ClientLink;
use crate::server::Server;

pub(crate) const FORCED_DATA: &str = "chunks";
const FORCED_KEY: &str = "Forced";

fn key_of(pos: ChunkPos) -> i64 {
    i64::from(pos.x as u32) | (i64::from(pos.z as u32) << 32)
}

fn pos_of(key: i64) -> ChunkPos {
    ChunkPos::new(key as i32, (key >> 32) as i32)
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn load_forced_chunks(&mut self) {
        let Some(nbt) = self.world.level.forced_chunks.take() else {
            return;
        };
        for key in nbt.get_long_array(FORCED_KEY).unwrap_or_default() {
            self.world
                .chunks
                .set_forced(pos_of(*key), true, &self.workers);
        }
        self.world.level.forced_chunks = Some(nbt);
    }

    pub(crate) fn save_forced_chunks(&mut self) {
        let keys: Vec<i64> = self
            .world
            .chunks
            .forced_chunks()
            .into_iter()
            .map(key_of)
            .collect();
        self.world.level.forced_chunks =
            Some(Compound::new().with(FORCED_KEY, Tag::LongArray(keys)));
    }

    pub(crate) fn save_forced_chunks_data(&mut self) {
        self.save_forced_chunks();
        if let Some(data) = &self.world.level.forced_chunks {
            self.workers.submit(
                Priority::BACKGROUND,
                ChunkJob::SaveData {
                    dimension: Arc::clone(&self.world.dimension),
                    name: FORCED_DATA,
                    data: Arc::new(data.clone()),
                },
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_chunk_key_packs_x_low_and_z_high_and_round_trips_negatives() {
        for (x, z) in [(0, 0), (1, 2), (-1, -1), (-30_000, 29_999)] {
            let pos = ChunkPos::new(x, z);
            assert_eq!(pos_of(key_of(pos)), pos);
        }
        assert_eq!(key_of(ChunkPos::new(1, 1)), 4_294_967_297);
        assert_eq!(key_of(ChunkPos::new(-1, 0)), 4_294_967_295);
    }
}
