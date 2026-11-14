use rspigot_domain::Identifier;

use std::sync::Arc;

use rspigot_domain::ChunkPos;
use rspigot_domain::world::Dimension;
use rspigot_nbt::Compound;

use crate::chunks::{ChunkJob, ChunkManager, ChunkWorkers, MobGeneration, Priority};
use crate::config::hashed_seed;
use crate::entities::EntityWorld;
use crate::falling::ScheduledFalls;
use crate::fluids::ScheduledFluids;
use crate::game_events::GameEvents;
use crate::ports::{ChunkGenerator, ChunkStore, ClientLink, EntityStore};
use crate::raids::RaidRuntime;
use crate::server::{Server, World};
use crate::spawning::Spawning;

pub(crate) type DimensionKey = Identifier;

const RAIDS_DATA: &str = "raids";
const DRAGON_FIGHT_DATA: &str = "ender_dragon_fight";

fn stored_data<S: EntityStore>(store: &S, key: &DimensionKey, name: &str) -> Option<Compound> {
    match store.load_world_data(key, name) {
        Ok(data) => data,
        Err(error) => {
            tracing::warn!(%key, name, %error, "ignoring unreadable world data");
            None
        }
    }
}

pub(crate) struct WorldState {
    pub(crate) workers: ChunkWorkers,
    pub(crate) world: World,
    pub(crate) entities: EntityWorld,
    pub(crate) falls: ScheduledFalls,
    pub(crate) game_events: GameEvents,
    pub(crate) fluids: ScheduledFluids,
    pub(crate) spawning: Spawning,
    pub(crate) raids: RaidRuntime,
}

impl<L: ClientLink> Server<L> {
    pub fn load_dimension<S: ChunkStore + EntityStore, G: ChunkGenerator>(
        &mut self,
        key: DimensionKey,
        dimension: Dimension,
        generator: Arc<G>,
        store: Arc<S>,
    ) {
        let dimension = Arc::new(dimension);
        let spawn = generator.spawn_point(&dimension);
        let mut level = self.world.level.clone();
        level.spawn = Some(spawn.block());
        level.raids = stored_data(&*store, &key, RAIDS_DATA);
        level.dragon_fight = stored_data(&*store, &key, DRAGON_FIGHT_DATA);
        level.raining = false;
        level.thundering = false;
        if dimension.has_fixed_time {
            level.day_time = 0;
        }
        let workers = ChunkWorkers::spawn(self.config.chunk_workers, store, generator);
        let mut chunks = ChunkManager::new(Arc::clone(&dimension), self.config.chunks);
        chunks.set_mob_generation(level.spawn_mobs.then_some(MobGeneration {
            seed: level.seed,
            zoom_seed: hashed_seed(level.seed),
            difficulty: self.config.difficulty,
        }));
        let center = spawn.chunk();
        let radius = self.config.spawn_chunk_radius;
        for dx in -radius..=radius {
            for dz in -radius..=radius {
                chunks.add_ticket(
                    ChunkPos::new(center.x + dx, center.z + dz),
                    Priority::URGENT,
                    &workers,
                );
            }
        }
        let state = WorldState {
            spawning: Spawning::new(level.seed, self.config.spawning.slime_seed, &dimension),
            workers,
            world: World {
                chunks,
                dimension,
                spawn,
                difficulty: self.config.difficulty,
                level,
            },
            entities: EntityWorld::default(),
            falls: ScheduledFalls::default(),
            game_events: GameEvents::default(),
            fluids: ScheduledFluids::default(),
            raids: RaidRuntime::default(),
        };
        self.dormant.insert(key.clone(), state);
        self.in_dimension(&key, |server| {
            server.load_raids();
            server.load_dragon_fight();
            if server.entities.fight.is_none() && server.world.dimension.has_dragon_fight() {
                server.ensure_dragon_fight(rspigot_domain::BlockPos::new(0, 0, 0), true);
            }
        });
    }

    pub(crate) fn save_world_data(&mut self) {
        if self.world.dimension.key == Identifier::minecraft("overworld") {
            return;
        }
        self.save_raids();
        self.save_dragon_fight();
        let saved = [
            (RAIDS_DATA, self.world.level.raids.as_ref()),
            (DRAGON_FIGHT_DATA, self.world.level.dragon_fight.as_ref()),
        ];
        for (name, data) in saved {
            if let Some(data) = data {
                self.workers.submit(
                    Priority::BACKGROUND,
                    ChunkJob::SaveData {
                        dimension: Arc::clone(&self.world.dimension),
                        name,
                        data: Arc::new(data.clone()),
                    },
                );
            }
        }
    }

    pub(crate) fn in_dimension<R>(
        &mut self,
        key: &DimensionKey,
        run: impl FnOnce(&mut Self) -> R,
    ) -> R {
        let Some(mut state) = self.dormant.remove(key) else {
            return run(self);
        };
        self.swap_world(&mut state);
        let result = run(self);
        self.deliver();
        self.swap_world(&mut state);
        self.dormant.insert(key.clone(), state);
        result
    }

    pub(crate) fn swap_world(&mut self, other: &mut WorldState) {
        let WorldState {
            workers,
            world,
            entities,
            falls,
            game_events,
            fluids,
            spawning,
            raids,
        } = other;
        std::mem::swap(&mut self.workers, workers);
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
pub(crate) mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use rspigot_blocks::{Block, Stone};
    use rspigot_domain::chunk::ChunkColumn;
    use rspigot_domain::player::{ClientSettings, GameProfile, PlayerId};
    use rspigot_domain::text::Component;
    use rspigot_domain::world::Dimension;
    use rspigot_domain::{BlockPos, ChunkPos, Vec3};
    use rspigot_entities::EntityKind as Kind;
    use rspigot_nbt::Compound;

    use super::*;
    use crate::chunks::ChunkManager;
    use crate::config::{GameConfig, LevelData};
    use crate::events::{ClientEvent, Inbound};
    use crate::mob_actions::new_mob;
    use crate::ports::{ChunkGenerator, ChunkStore, EntityStore, StoreError};
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
            workers: ChunkWorkers::spawn(1, Arc::new(Quiet), Arc::new(Quiet)),
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

    fn the_end() -> DimensionKey {
        Identifier::minecraft("the_end")
    }

    fn with_end(server: &mut Server<Quiet>) {
        server.load_dimension(
            the_end(),
            Dimension::end(),
            Arc::new(Quiet),
            Arc::new(Quiet),
        );
        server.in_dimension(&the_end(), |server| {
            server.wait_for_spawn(Duration::from_secs(10));
        });
    }

    fn drop_pig(server: &mut Server<Quiet>) -> (rspigot_domain::EntityId, f64) {
        server.in_dimension(&the_end(), |server| {
            let mut base = server.new_entity_base(Kind::Pig);
            base.position = Vec3::new(0.5, 80.0, 0.5);
            let id = server
                .place_entity(new_mob(base))
                .expect("the pig is placed");
            (id, 80.0)
        })
    }

    fn pig_height(server: &mut Server<Quiet>, id: rspigot_domain::EntityId) -> f64 {
        server.in_dimension(&the_end(), |server| {
            server
                .entities
                .store
                .get(id)
                .map_or(f64::NAN, |e| e.base.position.y)
        })
    }

    #[test]
    fn a_loaded_dimension_without_players_does_not_tick() {
        let mut server = server();
        with_end(&mut server);
        let (id, start) = drop_pig(&mut server);
        for _ in 0..10 {
            server.tick();
        }
        assert_eq!(pig_height(&mut server, id), start);
    }

    #[test]
    fn a_dimension_with_a_player_ticks() {
        let mut server = server();
        with_end(&mut server);
        let (id, start) = drop_pig(&mut server);
        server.handle(Inbound::Join {
            profile: GameProfile {
                id: PlayerId::offline("Alice"),
                name: "Alice".to_owned(),
                properties: Vec::new(),
            },
            settings: ClientSettings::default(),
            link: Quiet,
            address: [127, 0, 0, 1].into(),
            session: 1,
        });
        if let Some(player) = server.players.get_mut(&PlayerId::offline("Alice")) {
            player.dim = the_end();
        }
        for _ in 0..10 {
            server.tick();
        }
        assert!(pig_height(&mut server, id) < start);
        assert_eq!(
            server.world.dimension.key,
            Identifier::minecraft("overworld")
        );
    }

    #[derive(Default)]
    struct Mem {
        chunks: std::sync::Mutex<std::collections::HashMap<(Identifier, ChunkPos), ChunkColumn>>,
        entities:
            std::sync::Mutex<std::collections::HashMap<(Identifier, ChunkPos), Vec<Compound>>>,
        data: std::sync::Mutex<std::collections::HashMap<(Identifier, String), Compound>>,
    }

    impl ChunkStore for Mem {
        fn load(
            &self,
            dimension: &Identifier,
            pos: ChunkPos,
        ) -> Result<Option<ChunkColumn>, StoreError> {
            Ok(self
                .chunks
                .lock()
                .unwrap()
                .get(&(dimension.clone(), pos))
                .cloned())
        }

        fn save(&self, dimension: &Identifier, chunk: &ChunkColumn) -> Result<(), StoreError> {
            self.chunks
                .lock()
                .unwrap()
                .insert((dimension.clone(), chunk.pos()), chunk.clone());
            Ok(())
        }

        fn flush(&self) -> Result<(), StoreError> {
            Ok(())
        }
    }

    impl EntityStore for Mem {
        fn load_entities(
            &self,
            dimension: &Identifier,
            pos: ChunkPos,
        ) -> Result<Option<Vec<Compound>>, StoreError> {
            Ok(self
                .entities
                .lock()
                .unwrap()
                .get(&(dimension.clone(), pos))
                .cloned())
        }

        fn save_entities(
            &self,
            dimension: &Identifier,
            pos: ChunkPos,
            entities: &[Compound],
        ) -> Result<(), StoreError> {
            self.entities
                .lock()
                .unwrap()
                .insert((dimension.clone(), pos), entities.to_vec());
            Ok(())
        }

        fn load_world_data(
            &self,
            dimension: &Identifier,
            name: &str,
        ) -> Result<Option<Compound>, StoreError> {
            Ok(self
                .data
                .lock()
                .unwrap()
                .get(&(dimension.clone(), name.to_owned()))
                .cloned())
        }

        fn save_world_data(
            &self,
            dimension: &Identifier,
            name: &str,
            data: &Compound,
        ) -> Result<(), StoreError> {
            self.data
                .lock()
                .unwrap()
                .insert((dimension.clone(), name.to_owned()), data.clone());
            Ok(())
        }
    }

    fn server_with_end(store: &Arc<Mem>) -> Server<Quiet> {
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
        server.load_dimension(
            the_end(),
            Dimension::end(),
            Arc::new(Quiet),
            Arc::clone(store),
        );
        server.in_dimension(&the_end(), |server| {
            server.wait_for_spawn(Duration::from_secs(10));
        });
        server
    }

    fn stop(server: Server<Quiet>) {
        let (sender, inbound) = crossbeam_channel::unbounded::<Inbound<Quiet>>();
        sender.send(Inbound::Shutdown).expect("open");
        server.run(&inbound);
    }

    #[test]
    fn the_end_saves_and_restores_entities_and_the_dragon_fight() {
        let store = Arc::new(Mem::default());
        let mut server = server_with_end(&store);
        let (pig, dragon) = server.in_dimension(&the_end(), |server| {
            let mut base = server.new_entity_base(Kind::Pig);
            base.position = Vec3::new(0.5, 80.0, 0.5);
            let pig = server.place_entity(new_mob(base)).expect("placed");
            let uuid = server.entities.store.get(pig).expect("pig").base.uuid;
            server.ensure_dragon_fight(BlockPos::new(0, 0, 0), false);
            let dragon = uuid::Uuid::from_u128(0xd2a6);
            if let Some(fight) = server.entities.fight.as_mut() {
                fight.data.dragon_uuid = Some(dragon);
                fight.data.previously_killed = true;
            }
            (uuid, dragon)
        });
        stop(server);
        let end = the_end();
        assert!(
            store
                .entities
                .lock()
                .unwrap()
                .iter()
                .any(|((dimension, _), saved)| dimension == &end && !saved.is_empty())
        );
        let chunks = store.chunks.lock().unwrap();
        assert!(!chunks.is_empty());
        assert!(chunks.keys().all(|(dimension, _)| dimension == &end));
        drop(chunks);
        assert!(
            store
                .data
                .lock()
                .unwrap()
                .contains_key(&(end.clone(), "ender_dragon_fight".to_owned()))
        );
        assert!(
            store
                .entities
                .lock()
                .unwrap()
                .keys()
                .all(|(dimension, _)| dimension == &end)
        );
        let mut server = server_with_end(&store);
        let restored = server.in_dimension(&the_end(), |server| {
            let found = server.entities.store.by_uuid(pig).is_some();
            let fight = server.dragon_fight_data().expect("fight restored");
            (found, fight.dragon_uuid, fight.previously_killed)
        });
        assert_eq!(restored, (true, Some(dragon), true));
        assert!(server.dragon_fight_data().is_none());
        stop(server);
    }
}
