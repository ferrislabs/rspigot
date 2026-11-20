use rspigot_blocks::BlockKind;
use rspigot_domain::chunk::SavedTick;
use rspigot_domain::{BlockPos, ChunkPos, Identifier};
use rustc_hash::FxHashMap;

use crate::fluids::{is_fluid_id, saved_fluid_id};
use crate::ports::ClientLink;
use crate::server::Server;

#[derive(Default)]
struct Gathered {
    blocks: Vec<SavedTick>,
    fluids: Vec<SavedTick>,
}

fn saved(id: &str, pos: BlockPos, priority: i8, due: u64, now: u64) -> Option<SavedTick> {
    Some(SavedTick {
        id: Identifier::parse(id).ok()?,
        pos,
        priority: i32::from(priority),
        delay: i32::try_from(due.saturating_sub(now)).unwrap_or(i32::MAX),
    })
}

impl<L: ClientLink> Server<L> {
    fn gather_scheduled_ticks(&self) -> FxHashMap<ChunkPos, Gathered> {
        let now = self.current_tick();
        let mut found: FxHashMap<ChunkPos, Gathered> = FxHashMap::default();
        for (due, priority, pos, kind) in self.falls.entries() {
            if let Some(tick) = saved(kind.name(), pos, priority, due, now) {
                found.entry(pos.chunk()).or_default().blocks.push(tick);
            }
        }
        for (due, pos) in self.fluids.entries() {
            let id = self.world.chunks.block(pos).and_then(saved_fluid_id);
            if let Some(tick) = id.and_then(|id| saved(id, pos, 0, due, now)) {
                found.entry(pos.chunk()).or_default().fluids.push(tick);
            }
        }
        found
    }

    fn write_scheduled_ticks(&mut self, pos: ChunkPos, gathered: Gathered) {
        let Some(column) = self.world.chunks.get(pos) else {
            return;
        };
        if column.block_ticks == gathered.blocks && column.fluid_ticks == gathered.fluids {
            return;
        }
        if let Some(column) = self.world.chunks.chunk_mut(pos) {
            column.block_ticks = gathered.blocks;
            column.fluid_ticks = gathered.fluids;
        }
    }

    pub(crate) fn store_scheduled_ticks(&mut self) {
        let mut found = self.gather_scheduled_ticks();
        let loaded: Vec<ChunkPos> = self.world.chunks.loaded_positions().collect();
        for pos in loaded {
            if !self.world.chunks.load_unhandled(pos) {
                let gathered = found.remove(&pos).unwrap_or_default();
                self.write_scheduled_ticks(pos, gathered);
            }
        }
    }

    pub(crate) fn unload_scheduled_ticks(&mut self, pos: ChunkPos) {
        let gathered = self.gather_scheduled_ticks().remove(&pos);
        self.falls.remove_chunk(pos);
        self.fluids.remove_chunk(pos);
        self.write_scheduled_ticks(pos, gathered.unwrap_or_default());
    }

    pub(crate) fn restore_scheduled_ticks(&mut self, pos: ChunkPos) {
        let Some(column) = self.world.chunks.get(pos) else {
            return;
        };
        if column.block_ticks.is_empty() && column.fluid_ticks.is_empty() {
            return;
        }
        let Some(column) = self.world.chunks.chunk_mut(pos) else {
            return;
        };
        let blocks = std::mem::take(&mut column.block_ticks);
        let fluids = std::mem::take(&mut column.fluid_ticks);
        let now = self.current_tick();
        let due = |delay: i32| now + u64::try_from(delay).unwrap_or(0);
        for tick in &blocks {
            let priority = tick.priority.clamp(i32::from(i8::MIN), i32::from(i8::MAX)) as i8;
            match BlockKind::from_name(tick.id.as_str()) {
                Some(kind) => self
                    .falls
                    .restore(due(tick.delay), priority, tick.pos, kind),
                None => {
                    tracing::warn!(id = %tick.id, "skipping a scheduled tick of an unknown block")
                }
            }
        }
        for tick in &fluids {
            if is_fluid_id(tick.id.as_str()) {
                self.fluids.restore(due(tick.delay), tick.pos);
            } else {
                tracing::warn!(id = %tick.id, "skipping a scheduled tick of an unknown fluid");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use rspigot_blocks::BlockKind;
    use rspigot_domain::chunk::ChunkColumn;
    use rspigot_domain::{BlockPos, ChunkPos, Identifier};

    use crate::chunks::Priority;
    use crate::config::{GameConfig, LevelData};
    use crate::ports::{ChunkStore, EntityStore, StoreError};
    use crate::server::Server;
    use crate::status::StatusBoard;
    use crate::world_state::tests::Quiet;

    #[derive(Default)]
    struct Mem {
        chunks: Mutex<HashMap<ChunkPos, ChunkColumn>>,
    }

    impl ChunkStore for Mem {
        fn load(&self, _: &Identifier, pos: ChunkPos) -> Result<Option<ChunkColumn>, StoreError> {
            Ok(self.chunks.lock().unwrap().get(&pos).cloned())
        }

        fn save(&self, _: &Identifier, chunk: &ChunkColumn) -> Result<(), StoreError> {
            self.chunks
                .lock()
                .unwrap()
                .insert(chunk.pos(), chunk.clone());
            Ok(())
        }

        fn flush(&self) -> Result<(), StoreError> {
            Ok(())
        }
    }

    impl EntityStore for Mem {
        fn load_entities(
            &self,
            _: &Identifier,
            _: ChunkPos,
        ) -> Result<Option<Vec<rspigot_nbt::Compound>>, StoreError> {
            Ok(None)
        }

        fn save_entities(
            &self,
            _: &Identifier,
            _: ChunkPos,
            _: &[rspigot_nbt::Compound],
        ) -> Result<(), StoreError> {
            Ok(())
        }
    }

    const FAR: ChunkPos = ChunkPos::new(20, 20);

    fn server(store: &Arc<Mem>) -> Server<Quiet> {
        let mut config = GameConfig {
            chunk_workers: 1,
            spawn_chunk_radius: 1,
            ..GameConfig::default()
        };
        config.chunks.unload_delay_ticks = 1;
        let mut server = Server::new(
            config,
            LevelData::default(),
            Arc::clone(store),
            Arc::new(Quiet),
            StatusBoard::new(1),
        )
        .with_rng_seed(5);
        server.wait_for_spawn(Duration::from_secs(10));
        server
    }

    fn load(server: &mut Server<Quiet>) {
        server
            .world
            .chunks
            .add_ticket(FAR, Priority(1), &server.workers);
        server.wait_for_spawn(Duration::from_secs(10));
        assert!(server.world.chunks.is_loaded(FAR));
    }

    fn unload(server: &mut Server<Quiet>) {
        server.world.chunks.remove_ticket(FAR);
        for _ in 0..10 {
            if !server.world.chunks.is_loaded(FAR) {
                break;
            }
            server.tick();
        }
        assert!(!server.world.chunks.is_loaded(FAR));
        assert!(server.workers.flush_blocking(Duration::from_secs(10)));
    }

    fn sand() -> BlockPos {
        BlockPos::new(FAR.min_block_x() + 3, 100, FAR.min_block_z() + 4)
    }

    fn lava() -> BlockPos {
        BlockPos::new(FAR.min_block_x() + 5, 70, FAR.min_block_z() + 6)
    }

    fn prepare(server: &mut Server<Quiet>) {
        server
            .world
            .chunks
            .set_block(sand(), BlockKind::Sand.default_state());
        server
            .world
            .chunks
            .set_block(lava(), BlockKind::Lava.default_state());
        server.schedule(sand(), BlockKind::Sand, 40);
        server.fluid_changed(lava());
        assert!(server.has_scheduled(sand(), BlockKind::Sand));
    }

    fn fluid_due(server: &Server<Quiet>, pos: BlockPos) -> Option<u64> {
        server
            .fluids
            .entries()
            .find(|(_, at)| *at == pos)
            .map(|(due, _)| due)
    }

    fn block_due(server: &Server<Quiet>, pos: BlockPos) -> Option<u64> {
        server
            .falls
            .entries()
            .find(|(_, _, at, _)| *at == pos)
            .map(|(due, ..)| due)
    }

    #[test]
    fn scheduled_ticks_survive_an_unload_with_their_remaining_delay() {
        let store = Arc::new(Mem::default());
        let mut server = server(&store);
        load(&mut server);
        prepare(&mut server);
        let block_due_before = block_due(&server, sand()).unwrap();
        let fluid_due_before = fluid_due(&server, lava()).unwrap();
        unload(&mut server);
        let unloaded_at = server.current_tick();
        let saved = store.chunks.lock().unwrap().get(&FAR).cloned().unwrap();
        assert_eq!(saved.block_ticks.len(), 1);
        assert_eq!(saved.block_ticks[0].id.as_str(), "minecraft:sand");
        assert_eq!(saved.block_ticks[0].pos, sand());
        assert_eq!(saved.block_ticks[0].priority, 0);
        assert_eq!(saved.fluid_ticks.len(), 1);
        assert_eq!(saved.fluid_ticks[0].id.as_str(), "minecraft:lava");
        assert_eq!(saved.fluid_ticks[0].pos, lava());
        let block_delay = i64::from(saved.block_ticks[0].delay);
        let fluid_delay = i64::from(saved.fluid_ticks[0].delay);
        assert!((0..=40).contains(&block_delay));
        assert!((0..=30).contains(&fluid_delay));
        let drained_at = block_due_before as i64 - block_delay;
        assert_eq!(drained_at, fluid_due_before as i64 - fluid_delay);
        assert!(drained_at <= unloaded_at as i64);
        assert!(!server.has_scheduled(sand(), BlockKind::Sand));
        assert!(block_due(&server, sand()).is_none());
        assert!(fluid_due(&server, lava()).is_none());
        load(&mut server);
        let now = server.current_tick();
        assert!(server.has_scheduled(sand(), BlockKind::Sand));
        assert_eq!(block_due(&server, sand()), Some(now + block_delay as u64));
        assert_eq!(fluid_due(&server, lava()), Some(now + fluid_delay as u64));
        let again = server.world.chunks.get(FAR).unwrap();
        assert!(again.block_ticks.is_empty());
        assert!(again.fluid_ticks.is_empty());
    }

    #[test]
    fn a_tick_of_an_unloaded_chunk_does_not_run_and_fires_after_the_reload() {
        let store = Arc::new(Mem::default());
        let mut server = server(&store);
        load(&mut server);
        prepare(&mut server);
        unload(&mut server);
        for _ in 0..60 {
            server.tick();
        }
        assert!(block_due(&server, sand()).is_none());
        load(&mut server);
        let state = server.world.chunks.block(sand()).unwrap();
        assert_eq!(state.kind(), Some(BlockKind::Sand));
        let due = block_due(&server, sand()).unwrap();
        while server.current_tick() < due {
            assert_eq!(
                server.world.chunks.block(sand()).unwrap().kind(),
                Some(BlockKind::Sand)
            );
            server.tick();
        }
        server.tick();
        assert_ne!(
            server.world.chunks.block(sand()).and_then(|s| s.kind()),
            Some(BlockKind::Sand)
        );
    }

    #[test]
    fn saving_copies_the_ticks_without_removing_them() {
        let store = Arc::new(Mem::default());
        let mut server = server(&store);
        load(&mut server);
        prepare(&mut server);
        server.store_scheduled_ticks();
        let column = server.world.chunks.get(FAR).unwrap();
        assert_eq!(column.block_ticks.len(), 1);
        assert_eq!(column.fluid_ticks.len(), 1);
        assert!(server.has_scheduled(sand(), BlockKind::Sand));
        assert!(fluid_due(&server, lava()).is_some());
    }

    #[test]
    fn a_vanilla_shaped_column_loads_its_ticks_into_the_queues() {
        use rspigot_domain::chunk::SavedTick;
        let store = Arc::new(Mem::default());
        let mut column = ChunkColumn::empty(
            FAR,
            rspigot_domain::world::Dimension::overworld().height,
            rspigot_domain::BiomeId(0),
        );
        column.block_ticks = vec![
            SavedTick {
                id: Identifier::minecraft("sand"),
                pos: sand(),
                priority: -1,
                delay: 7,
            },
            SavedTick {
                id: Identifier::minecraft("no_such_block"),
                pos: sand().offset(1, 0, 0),
                priority: 0,
                delay: 7,
            },
        ];
        column.fluid_ticks = vec![SavedTick {
            id: Identifier::minecraft("flowing_water"),
            pos: lava(),
            priority: 0,
            delay: 3,
        }];
        store.chunks.lock().unwrap().insert(FAR, column);
        let mut server = server(&store);
        load(&mut server);
        let now = server.current_tick();
        assert_eq!(block_due(&server, sand()), Some(now + 7));
        assert_eq!(fluid_due(&server, lava()), Some(now + 3));
        assert!(!server.has_scheduled(sand().offset(1, 0, 0), BlockKind::Sand));
        assert_eq!(server.falls.entries().count(), 1);
    }
}
