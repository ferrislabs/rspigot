#[cfg(test)]
pub(crate) mod support {
    use std::sync::Arc;
    use std::time::Duration;

    use rspigot_blocks::{Block, Stone};
    use rspigot_domain::chunk::ChunkColumn;
    use rspigot_domain::text::Component;
    use rspigot_domain::world::Dimension;
    use rspigot_domain::{BlockPos, ChunkPos, Identifier, Vec3};
    use rspigot_nbt::Compound;

    use crate::config::{GameConfig, LevelData};
    use crate::events::ClientEvent;
    use crate::ports::{ChunkGenerator, ChunkStore, ClientLink, EntityStore, StoreError};
    use crate::server::Server;
    use crate::status::StatusBoard;

    pub(crate) struct Quiet;

    impl ClientLink for Quiet {
        fn send(&self, _: ClientEvent) {}

        fn disconnect(&self, _: Component) {}

        fn is_connected(&self) -> bool {
            true
        }
    }

    impl ChunkStore for Quiet {
        fn load(&self, _: &Identifier, _: ChunkPos) -> Result<Option<ChunkColumn>, StoreError> {
            Ok(None)
        }

        fn save(&self, _: &Identifier, _: &ChunkColumn) -> Result<(), StoreError> {
            Ok(())
        }

        fn flush(&self) -> Result<(), StoreError> {
            Ok(())
        }
    }

    impl EntityStore for Quiet {
        fn load_entities(
            &self,
            _: &Identifier,
            _: ChunkPos,
        ) -> Result<Option<Vec<Compound>>, StoreError> {
            Ok(None)
        }

        fn save_entities(
            &self,
            _: &Identifier,
            _: ChunkPos,
            _: &[Compound],
        ) -> Result<(), StoreError> {
            Ok(())
        }
    }

    impl ChunkGenerator for Quiet {
        fn generate(&self, dimension: &Dimension, pos: ChunkPos) -> ChunkColumn {
            let mut chunk = ChunkColumn::empty(pos, dimension.height, rspigot_domain::BiomeId(0));
            for x in 0..16 {
                for z in 0..16 {
                    for y in 60..=63 {
                        let at = BlockPos::new(pos.min_block_x() + x, y, pos.min_block_z() + z);
                        chunk.set_block(at, Stone::DEFAULT_STATE);
                    }
                }
            }
            chunk
        }

        fn spawn_point(&self, _: &Dimension) -> Vec3 {
            Vec3::new(0.5, 64.0, 0.5)
        }
    }

    pub(crate) fn server() -> Server<Quiet> {
        let config = GameConfig {
            chunk_workers: 1,
            spawn_chunk_radius: 1,
            ..GameConfig::default()
        };
        let mut server = Server::new(
            config,
            LevelData::default(),
            Arc::new(Quiet),
            Arc::new(Quiet),
            StatusBoard::new(1),
        )
        .with_rng_seed(11);
        server.wait_for_spawn(Duration::from_secs(10));
        server
    }
}
