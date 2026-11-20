use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rspigot_blocks::{BlockKind, StateId};
use rspigot_core::chunks::{ChunkManager, ChunkManagerConfig, ChunkWorkers, Priority};
use rspigot_core::ports::{ChunkGenerator, ChunkStore, EntityStore, StoreError};
use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::poi::{PoiChunkData, PoiRecord, PoiSectionData, PoiType};
use rspigot_domain::world::Dimension;
use rspigot_domain::{BiomeId, BlockPos, ChunkPos, Identifier, Vec3};
use rspigot_nbt::Compound;

const HOME: ChunkPos = ChunkPos::new(0, 0);

fn bed() -> BlockPos {
    BlockPos::new(3, 64, 3)
}

fn bed_state() -> StateId {
    BlockKind::RedBed
        .state_with(&[("part", "head")])
        .expect("bed")
}

fn furnished(pos: ChunkPos) -> ChunkColumn {
    let at = |x: i32, z: i32| BlockPos::new(pos.min_block_x() + x, 64, pos.min_block_z() + z);
    let mut chunk = ChunkColumn::empty(pos, Dimension::overworld().height, BiomeId(0));
    chunk.set_block(at(3, 3), bed_state());
    chunk.set_block(at(5, 5), BlockKind::Bell.default_state());
    chunk.set_block(at(7, 7), BlockKind::BlastFurnace.default_state());
    chunk.set_block(at(9, 9), BlockKind::NetherPortal.default_state());
    chunk
}

#[derive(Default)]
struct Disk {
    chunks: Mutex<HashMap<ChunkPos, ChunkColumn>>,
    poi: Mutex<HashMap<ChunkPos, PoiChunkData>>,
    poi_saves: Mutex<usize>,
}

impl Disk {
    fn with_chunk(chunk: ChunkColumn) -> Arc<Self> {
        let disk = Self::default();
        disk.chunks.lock().unwrap().insert(chunk.pos(), chunk);
        Arc::new(disk)
    }

    fn saved_poi(&self, pos: ChunkPos) -> Option<PoiChunkData> {
        self.poi.lock().unwrap().get(&pos).cloned()
    }
}

impl ChunkStore for Disk {
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

impl EntityStore for Disk {
    fn load_entities(
        &self,
        _: &Identifier,
        _: ChunkPos,
    ) -> Result<Option<Vec<Compound>>, StoreError> {
        Ok(None)
    }

    fn save_entities(&self, _: &Identifier, _: ChunkPos, _: &[Compound]) -> Result<(), StoreError> {
        Ok(())
    }

    fn load_poi(&self, _: &Identifier, pos: ChunkPos) -> Result<Option<PoiChunkData>, StoreError> {
        Ok(self.poi.lock().unwrap().get(&pos).cloned())
    }

    fn save_poi(
        &self,
        _: &Identifier,
        pos: ChunkPos,
        data: &PoiChunkData,
    ) -> Result<(), StoreError> {
        *self.poi_saves.lock().unwrap() += 1;
        let mut stored = self.poi.lock().unwrap();
        if data.is_empty() {
            stored.remove(&pos);
        } else {
            stored.insert(pos, data.clone());
        }
        Ok(())
    }
}

struct Generator;

impl ChunkGenerator for Generator {
    fn generate(&self, _: &Dimension, pos: ChunkPos) -> ChunkColumn {
        furnished(pos)
    }

    fn spawn_point(&self, _: &Dimension) -> Vec3 {
        Vec3::ZERO
    }
}

struct Server {
    manager: ChunkManager,
    workers: ChunkWorkers,
}

fn start(disk: &Arc<Disk>) -> Server {
    Server {
        manager: ChunkManager::new(
            Arc::new(Dimension::overworld()),
            ChunkManagerConfig {
                unload_delay_ticks: 2,
                autosave_interval_ticks: 0,
            },
        ),
        workers: ChunkWorkers::spawn(1, Arc::clone(disk), Arc::new(Generator)),
    }
}

impl Server {
    fn load(&mut self, pos: ChunkPos) {
        self.manager.add_ticket(pos, Priority(1), &self.workers);
        let deadline = Instant::now() + Duration::from_secs(10);
        while !self.manager.is_loaded(pos) && Instant::now() < deadline {
            if let Ok(result) = self
                .workers
                .results()
                .recv_timeout(Duration::from_millis(20))
            {
                self.manager.accept(result);
            }
        }
        assert!(self.manager.is_loaded(pos));
    }

    fn unload(&mut self, pos: ChunkPos) {
        self.manager.remove_ticket(pos);
        for _ in 0..4 {
            self.manager.tick();
        }
        assert!(self.manager.unload(pos, true, &self.workers));
        assert!(self.workers.flush_blocking(Duration::from_secs(10)));
    }

    fn autosave(&mut self) {
        self.manager.save_dirty(&self.workers);
        assert!(self.workers.flush_blocking(Duration::from_secs(10)));
    }

    fn claim(&mut self) {
        let taken =
            self.manager
                .poi_mut()
                .take(|kind| kind == PoiType::Home, |_, _| true, bed(), 4);
        assert_eq!(taken, Some(bed()));
    }

    fn tickets(&self) -> Option<i32> {
        self.manager
            .poi()
            .record_at(bed())
            .map(|r| r.free_tickets())
    }
}

fn kinds(server: &Server) -> Vec<PoiType> {
    [
        bed(),
        BlockPos::new(5, 64, 5),
        BlockPos::new(7, 64, 7),
        BlockPos::new(9, 64, 9),
    ]
    .iter()
    .filter_map(|pos| server.manager.poi().type_at(*pos))
    .collect()
}

#[test]
fn a_claimed_bed_stays_claimed_across_unload_and_reload() {
    let disk = Arc::new(Disk::default());
    let mut server = start(&disk);
    server.load(HOME);
    assert_eq!(server.tickets(), Some(1));
    server.claim();
    server.unload(HOME);
    assert_eq!(
        disk.saved_poi(HOME)
            .and_then(|d| d.section(4).map(|s| s.records.len())),
        Some(4)
    );
    server.load(HOME);
    assert_eq!(server.tickets(), Some(0));
}

#[test]
fn a_claimed_bed_survives_a_restart_through_autosave() {
    let disk = Arc::new(Disk::default());
    let mut server = start(&disk);
    server.load(HOME);
    server.autosave();
    let saves = *disk.poi_saves.lock().unwrap();
    server.claim();
    server.autosave();
    assert_eq!(*disk.poi_saves.lock().unwrap(), saves + 1);
    server.autosave();
    assert_eq!(*disk.poi_saves.lock().unwrap(), saves + 1);
    drop(server);
    let mut restarted = start(&disk);
    restarted.load(HOME);
    assert_eq!(restarted.tickets(), Some(0));
}

#[test]
fn a_chunk_without_a_poi_file_rebuilds_from_its_blocks() {
    let disk = Disk::with_chunk(furnished(HOME));
    let mut server = start(&disk);
    server.load(HOME);
    assert_eq!(
        kinds(&server),
        vec![
            PoiType::Home,
            PoiType::Meeting,
            PoiType::Armorer,
            PoiType::NetherPortal
        ]
    );
    assert_eq!(server.manager.poi().record_count(), 4);
    assert!(server.manager.poi().is_dirty(HOME));
}

#[test]
fn an_invalid_saved_section_is_rescanned() {
    let disk = Disk::with_chunk(furnished(HOME));
    disk.poi.lock().unwrap().insert(
        HOME,
        PoiChunkData {
            sections: vec![PoiSectionData {
                section_y: 4,
                valid: false,
                records: vec![
                    PoiRecord::with_free_tickets(bed(), PoiType::Home, 0),
                    PoiRecord::new(BlockPos::new(11, 64, 11), PoiType::Meeting),
                ],
            }],
        },
    );
    let mut server = start(&disk);
    server.load(HOME);
    assert_eq!(server.tickets(), Some(0));
    assert_eq!(server.manager.poi().record_count(), 4);
    assert_eq!(
        server.manager.poi().type_at(BlockPos::new(11, 64, 11)),
        None
    );
}

#[test]
fn a_valid_saved_section_is_trusted_over_the_blocks() {
    let disk = Disk::with_chunk(furnished(HOME));
    disk.poi.lock().unwrap().insert(
        HOME,
        PoiChunkData {
            sections: vec![PoiSectionData {
                section_y: 4,
                valid: true,
                records: vec![PoiRecord::with_free_tickets(bed(), PoiType::Home, 0)],
            }],
        },
    );
    let mut server = start(&disk);
    server.load(HOME);
    assert_eq!(server.manager.poi().record_count(), 1);
    assert_eq!(server.tickets(), Some(0));
    assert!(!server.manager.poi().is_dirty(HOME));
}

#[test]
fn poi_memory_shrinks_after_unload() {
    let disk = Arc::new(Disk::default());
    let mut server = start(&disk);
    let other = ChunkPos::new(1, 0);
    server.load(HOME);
    server.load(other);
    assert_eq!(server.manager.poi().record_count(), 8);
    server.unload(HOME);
    assert_eq!(server.manager.poi().record_count(), 4);
    server.unload(other);
    assert_eq!(server.manager.poi().record_count(), 0);
    assert_eq!(server.manager.poi().section_count(), 0);
    assert!(server.manager.poi().dirty_chunks().is_empty());
}

#[test]
fn removing_the_last_poi_clears_the_saved_entry() {
    let disk = Arc::new(Disk::default());
    let mut server = start(&disk);
    server.load(HOME);
    server.autosave();
    assert!(disk.saved_poi(HOME).is_some());
    for pos in [
        bed(),
        BlockPos::new(5, 64, 5),
        BlockPos::new(7, 64, 7),
        BlockPos::new(9, 64, 9),
    ] {
        server.manager.set_block(pos, StateId::AIR);
    }
    server.autosave();
    assert!(disk.saved_poi(HOME).is_none());
}
