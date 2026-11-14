use rspigot_domain::Identifier;

use crate::entities::EntityWorld;
use crate::falling::ScheduledFalls;
use crate::fluids::ScheduledFluids;
use crate::game_events::GameEvents;
use crate::ports::ClientLink;
use crate::raids::RaidRuntime;
use crate::server::{Server, World};
use crate::spawning::Spawning;

pub(crate) type DimensionKey = Identifier;

#[cfg_attr(not(test), expect(dead_code))]
pub(crate) struct WorldState {
    pub(crate) world: World,
    pub(crate) entities: EntityWorld,
    pub(crate) falls: ScheduledFalls,
    pub(crate) game_events: GameEvents,
    pub(crate) fluids: ScheduledFluids,
    pub(crate) spawning: Spawning,
    pub(crate) raids: RaidRuntime,
}

impl<L: ClientLink> Server<L> {
    #[cfg_attr(not(test), expect(dead_code))]
    pub(crate) fn swap_world(&mut self, other: &mut WorldState) {
        let WorldState {
            world,
            entities,
            falls,
            game_events,
            fluids,
            spawning,
            raids,
        } = other;
        std::mem::swap(&mut self.world, world);
        std::mem::swap(&mut self.entities, entities);
        std::mem::swap(&mut self.falls, falls);
        std::mem::swap(&mut self.game_events, game_events);
        std::mem::swap(&mut self.fluids, fluids);
        std::mem::swap(&mut self.spawning, spawning);
        std::mem::swap(&mut self.raids, raids);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use rspigot_blocks::{Block, Stone};
    use rspigot_domain::chunk::ChunkColumn;
    use rspigot_domain::text::Component;
    use rspigot_domain::world::Dimension;
    use rspigot_domain::{BlockPos, ChunkPos, Vec3};
    use rspigot_entities::EntityKind as Kind;
    use rspigot_nbt::Compound;

    use super::*;
    use crate::chunks::ChunkManager;
    use crate::config::{GameConfig, LevelData};
    use crate::events::ClientEvent;
    use crate::mob_actions::new_mob;
    use crate::ports::{ChunkGenerator, ChunkStore, EntityStore, StoreError};
    use crate::status::StatusBoard;

    struct Quiet;

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
                    let at = BlockPos::new(pos.min_block_x() + x, 62, pos.min_block_z() + z);
                    chunk.set_block(at, Stone::DEFAULT_STATE);
                }
            }
            chunk
        }

        fn spawn_point(&self, _: &Dimension) -> Vec3 {
            Vec3::new(0.5, 64.0, 0.5)
        }
    }

    fn server() -> Server<Quiet> {
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
        .with_rng_seed(5);
        server.wait_for_spawn(Duration::from_secs(10));
        server
    }

    fn nether_state(server: &Server<Quiet>) -> WorldState {
        let dimension = Arc::new(Dimension::nether());
        let level = LevelData::default();
        WorldState {
            spawning: Spawning::new(level.seed, 0, &dimension),
            world: World {
                chunks: ChunkManager::new(Arc::clone(&dimension), server.config.chunks),
                dimension,
                spawn: Vec3::new(0.5, 64.0, 0.5),
                difficulty: server.world.difficulty,
                level,
            },
            entities: EntityWorld::default(),
            falls: ScheduledFalls::default(),
            game_events: GameEvents::default(),
            fluids: ScheduledFluids::default(),
            raids: RaidRuntime::default(),
        }
    }

    fn pig(server: &mut Server<Quiet>) -> rspigot_domain::EntityId {
        let mut base = server.new_entity_base(Kind::Pig);
        base.position = Vec3::new(0.5, 64.0, 0.5);
        server
            .place_entity(new_mob(base))
            .expect("the pig is placed")
    }

    #[test]
    fn swapping_twice_restores_the_original_state() {
        let mut server = server();
        let id = pig(&mut server);
        let mut other = nether_state(&server);
        server.swap_world(&mut other);
        server.swap_world(&mut other);
        assert_eq!(
            server.world.dimension.key,
            Identifier::minecraft("overworld")
        );
        assert_eq!(
            other.world.dimension.key,
            Identifier::minecraft("the_nether")
        );
        assert!(server.entities.store.get(id).is_some());
        assert!(other.entities.store.get(id).is_none());
    }

    #[test]
    fn a_dimension_does_not_see_the_entities_of_the_other() {
        let mut server = server();
        let id = pig(&mut server);
        let mut other = nether_state(&server);
        server.swap_world(&mut other);
        assert_eq!(
            server.world.dimension.key,
            Identifier::minecraft("the_nether")
        );
        assert!(server.entities.store.get(id).is_none());
        assert!(other.entities.store.get(id).is_some());
        server.swap_world(&mut other);
        assert!(server.entities.store.get(id).is_some());
        assert!(other.entities.store.get(id).is_none());
    }
}
