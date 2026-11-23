use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use rspigot_domain::chunk::{BlockMeta, ChunkColumn};
use rspigot_domain::meta::{MetaError, MetaValue};
use rspigot_domain::poi::PoiManager;
use rspigot_domain::world::Dimension;
use rspigot_domain::{BlockPos, BlockStateId, ChunkPos};
use rspigot_nbt::Compound;
use rustc_hash::{FxHashMap, FxHashSet};

use super::queue::Priority;
use super::workers::{ChunkJob, ChunkResult, ChunkSource, ChunkWorkers};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChunkManagerConfig {
    pub unload_delay_ticks: u64,
    pub autosave_interval_ticks: u64,
}

impl Default for ChunkManagerConfig {
    fn default() -> Self {
        Self {
            unload_delay_ticks: 300,
            autosave_interval_ticks: 6000,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ChunkManagerStats {
    pub loaded: usize,
    pub pending: usize,
    pub loaded_from_disk: u64,
    pub generated: u64,
    pub saved: u64,
    pub unloaded: u64,
    pub cancelled: u64,
    pub total_load_time: Duration,
}

impl ChunkManagerStats {
    pub fn average_load_time(&self) -> Duration {
        let count = self.loaded_from_disk + self.generated;
        if count == 0 {
            Duration::ZERO
        } else {
            self.total_load_time / count as u32
        }
    }
}

struct Loaded {
    chunk: Arc<ChunkColumn>,
    tickets: u32,
    unload_at: Option<u64>,
    dirty: bool,
}

struct Pending {
    tickets: u32,
    priority: Priority,
    cancel: Arc<AtomicBool>,
}

#[derive(Debug, Default)]
pub struct ChunkEvents {
    pub loaded: Vec<(ChunkPos, bool)>,
    pub entity_loads: Vec<(ChunkPos, Vec<Compound>)>,
    pub entities_saved: Vec<(ChunkPos, u64)>,
    pub unloaded: Vec<ChunkPos>,
    pub autosaved: bool,
}

pub struct ChunkManager {
    dimension: Arc<Dimension>,
    config: ChunkManagerConfig,
    loaded: FxHashMap<ChunkPos, Loaded>,
    pending: FxHashMap<ChunkPos, Pending>,
    now: u64,
    stats: ChunkManagerStats,
    events: ChunkEvents,
    relight: FxHashMap<ChunkPos, u8>,
    stitch: FxHashSet<ChunkPos>,
    mob_generation: Option<super::MobGeneration>,
    poi: PoiManager,
    forced: std::collections::BTreeSet<ChunkPos>,
    timed: std::collections::BTreeMap<ChunkPos, TimedTicket>,
}

#[derive(Debug, Clone, Copy)]
struct TimedTicket {
    expires: u64,
    simulates: bool,
}

impl ChunkManager {
    pub fn new(dimension: Arc<Dimension>, config: ChunkManagerConfig) -> Self {
        let poi = PoiManager::new(dimension.height);
        Self {
            dimension,
            config,
            loaded: FxHashMap::default(),
            pending: FxHashMap::default(),
            now: 0,
            stats: ChunkManagerStats::default(),
            events: ChunkEvents::default(),
            relight: FxHashMap::default(),
            stitch: FxHashSet::default(),
            mob_generation: None,
            poi,
            forced: std::collections::BTreeSet::new(),
            timed: std::collections::BTreeMap::new(),
        }
    }

    pub fn poi(&self) -> &PoiManager {
        &self.poi
    }

    pub fn poi_mut(&mut self) -> &mut PoiManager {
        &mut self.poi
    }

    pub fn set_mob_generation(&mut self, generation: Option<super::MobGeneration>) {
        self.mob_generation = generation;
    }

    pub fn take_events(&mut self) -> ChunkEvents {
        std::mem::take(&mut self.events)
    }

    pub fn dimension(&self) -> &Arc<Dimension> {
        &self.dimension
    }

    pub fn add_ticket(&mut self, pos: ChunkPos, priority: Priority, workers: &ChunkWorkers) {
        if let Some(loaded) = self.loaded.get_mut(&pos) {
            loaded.tickets += 1;
            loaded.unload_at = None;
            return;
        }
        if let Some(pending) = self.pending.get_mut(&pos) {
            pending.tickets += 1;
            self.reprioritize(pos, priority, workers);
            return;
        }
        let cancel = workers.submit(priority, self.load_job(pos));
        self.pending.insert(
            pos,
            Pending {
                tickets: 1,
                priority,
                cancel,
            },
        );
    }

    pub fn reprioritize(&mut self, pos: ChunkPos, priority: Priority, workers: &ChunkWorkers) {
        if let Some(pending) = self.pending.get_mut(&pos)
            && priority < pending.priority
        {
            pending.cancel.store(true, Ordering::Release);
            pending.cancel = workers.submit(
                priority,
                ChunkJob::Load {
                    dimension: Arc::clone(&self.dimension),
                    pos,
                    mobs: self.mob_generation,
                },
            );
            pending.priority = priority;
        }
    }

    pub fn regenerate(&mut self, pos: ChunkPos, workers: &ChunkWorkers) -> bool {
        if self.pending.contains_key(&pos) {
            return false;
        }
        let tickets = self.loaded.remove(&pos).map_or(0, |loaded| {
            self.poi.unload_chunk(pos);
            loaded.tickets
        });
        let cancel = workers.submit(
            Priority::URGENT,
            ChunkJob::Generate {
                dimension: Arc::clone(&self.dimension),
                pos,
                mobs: self.mob_generation,
            },
        );
        self.pending.insert(
            pos,
            Pending {
                tickets: tickets + 1,
                priority: Priority::URGENT,
                cancel,
            },
        );
        true
    }

    pub fn set_forced(&mut self, pos: ChunkPos, forced: bool, workers: &ChunkWorkers) -> bool {
        if forced == self.forced.contains(&pos) {
            return false;
        }
        if forced {
            self.forced.insert(pos);
            self.add_ticket(pos, Priority::URGENT, workers);
        } else {
            self.forced.remove(&pos);
            self.remove_ticket(pos);
        }
        true
    }

    pub fn simulates(&self, pos: ChunkPos) -> bool {
        self.loaded.contains_key(&pos)
            && (self.forced.contains(&pos) || self.timed.get(&pos).is_some_and(|t| t.simulates))
    }

    pub fn simulated_chunks(&self) -> Vec<ChunkPos> {
        let mut found: Vec<ChunkPos> = self
            .forced
            .iter()
            .chain(
                self.timed
                    .iter()
                    .filter(|(_, ticket)| ticket.simulates)
                    .map(|(pos, _)| pos),
            )
            .copied()
            .filter(|pos| self.loaded.contains_key(pos))
            .collect();
        found.sort_unstable();
        found.dedup();
        found
    }

    pub fn hold_timed(
        &mut self,
        pos: ChunkPos,
        ticks: u64,
        simulates: bool,
        workers: &ChunkWorkers,
    ) {
        let expires = self.now + ticks;
        if let Some(ticket) = self.timed.get_mut(&pos) {
            ticket.expires = ticket.expires.max(expires);
            ticket.simulates |= simulates;
            return;
        }
        self.timed.insert(pos, TimedTicket { expires, simulates });
        self.add_ticket(pos, Priority::URGENT, workers);
    }

    pub fn is_held(&self, pos: ChunkPos) -> bool {
        self.timed.contains_key(&pos)
    }

    pub fn is_forced(&self, pos: ChunkPos) -> bool {
        self.forced.contains(&pos)
    }

    pub fn forced_chunks(&self) -> Vec<ChunkPos> {
        let mut found: Vec<ChunkPos> = self.forced.iter().copied().collect();
        found.sort_by_key(|pos| (i64::from(pos.z) << 32) | i64::from(pos.x as u32));
        found
    }

    pub fn remove_ticket(&mut self, pos: ChunkPos) {
        if let Some(loaded) = self.loaded.get_mut(&pos) {
            loaded.tickets = loaded.tickets.saturating_sub(1);
            if loaded.tickets == 0 {
                loaded.unload_at = Some(self.now + self.config.unload_delay_ticks);
            }
            return;
        }
        if let Some(pending) = self.pending.get_mut(&pos) {
            pending.tickets = pending.tickets.saturating_sub(1);
            if pending.tickets == 0 {
                pending.cancel.store(true, Ordering::Release);
                self.pending.remove(&pos);
                self.stats.cancelled += 1;
            }
        }
    }

    fn load_job(&self, pos: ChunkPos) -> ChunkJob {
        ChunkJob::Load {
            dimension: Arc::clone(&self.dimension),
            pos,
            mobs: self.mob_generation,
        }
    }

    pub fn accept(&mut self, result: ChunkResult) -> Option<ChunkPos> {
        match result {
            ChunkResult::Loaded {
                pos,
                chunk,
                source,
                elapsed,
                entities,
                poi,
                relit,
                ..
            } => {
                let pending = self.pending.remove(&pos)?;
                self.events
                    .loaded
                    .push((pos, source == ChunkSource::Generated));
                self.events
                    .entity_loads
                    .push((pos, entities.unwrap_or_default()));
                match source {
                    ChunkSource::Disk => self.stats.loaded_from_disk += 1,
                    ChunkSource::Generated => self.stats.generated += 1,
                }
                self.stats.total_load_time += elapsed;
                self.poi.import_chunk(&chunk, poi.as_ref());
                let mut chunk = chunk;
                chunk.block_meta.drop_volatile();
                self.stitch.insert(pos);
                self.loaded.insert(
                    pos,
                    Loaded {
                        chunk: Arc::new(*chunk),
                        tickets: pending.tickets,
                        unload_at: None,
                        dirty: source == ChunkSource::Generated || relit,
                    },
                );
                Some(pos)
            }
            ChunkResult::Saved { .. } => {
                self.stats.saved += 1;
                None
            }
            ChunkResult::EntitiesSaved { pos, generation } => {
                self.events.entities_saved.push((pos, generation));
                None
            }
            ChunkResult::Failed { pos, error } => {
                tracing::error!(?pos, %error, "chunk job failed");
                None
            }
            ChunkResult::PoiSaved { .. } | ChunkResult::DataSaved | ChunkResult::Flushed => None,
        }
    }

    pub fn tick(&mut self) -> Vec<ChunkPos> {
        self.now += 1;
        let now = self.now;
        let lapsed: Vec<ChunkPos> = self
            .timed
            .iter()
            .filter(|(_, ticket)| ticket.expires <= now)
            .map(|(pos, _)| *pos)
            .collect();
        for pos in lapsed {
            self.timed.remove(&pos);
            self.remove_ticket(pos);
        }
        let mut expired: Vec<ChunkPos> = self
            .loaded
            .iter()
            .filter(|(_, l)| l.tickets == 0 && l.unload_at.is_some_and(|at| at <= now))
            .map(|(pos, _)| *pos)
            .collect();
        expired.sort_unstable();
        if self.config.autosave_interval_ticks > 0
            && now.is_multiple_of(self.config.autosave_interval_ticks)
        {
            self.events.autosaved = true;
        }
        expired
    }

    pub fn unload_due(&self, pos: ChunkPos) -> bool {
        self.loaded
            .get(&pos)
            .is_some_and(|l| l.tickets == 0 && l.unload_at.is_some_and(|at| at <= self.now))
    }

    pub fn unload(&mut self, pos: ChunkPos, save: bool, workers: &ChunkWorkers) -> bool {
        if !self.unload_due(pos) {
            return false;
        }
        let Some(loaded) = self.loaded.remove(&pos) else {
            return false;
        };
        if save && loaded.dirty {
            self.submit_save(loaded.chunk, workers);
        }
        if save && self.poi.is_dirty(pos) {
            self.submit_poi_save(pos, workers);
        }
        self.poi.unload_chunk(pos);
        self.events.unloaded.push(pos);
        self.stats.unloaded += 1;
        true
    }

    fn submit_save(&self, chunk: Arc<ChunkColumn>, workers: &ChunkWorkers) {
        workers.submit(
            Priority::BACKGROUND,
            ChunkJob::Save {
                dimension: Arc::clone(&self.dimension),
                chunk,
            },
        );
    }

    fn submit_poi_save(&self, pos: ChunkPos, workers: &ChunkWorkers) {
        workers.submit(
            Priority::BACKGROUND,
            ChunkJob::SavePoi {
                dimension: Arc::clone(&self.dimension),
                pos,
                data: Arc::new(self.poi.export_chunk(pos)),
            },
        );
    }

    pub fn save_dirty(&mut self, workers: &ChunkWorkers) -> usize {
        self.save_dirty_except(workers, |_| false)
    }

    pub fn save_dirty_except(
        &mut self,
        workers: &ChunkWorkers,
        skipped: impl Fn(ChunkPos) -> bool,
    ) -> usize {
        for pos in self.poi.dirty_chunks() {
            if self.loaded.contains_key(&pos) && !skipped(pos) {
                self.submit_poi_save(pos, workers);
                self.poi.clear_dirty(pos);
            }
        }
        let mut count = 0;
        for (pos, loaded) in &mut self.loaded {
            if loaded.dirty && !skipped(*pos) {
                loaded.dirty = false;
                workers.submit(
                    Priority::BACKGROUND,
                    ChunkJob::Save {
                        dimension: Arc::clone(&self.dimension),
                        chunk: Arc::clone(&loaded.chunk),
                    },
                );
                count += 1;
            }
        }
        count
    }

    pub fn get(&self, pos: ChunkPos) -> Option<&Arc<ChunkColumn>> {
        self.loaded.get(&pos).map(|l| &l.chunk)
    }

    pub fn is_loaded(&self, pos: ChunkPos) -> bool {
        self.loaded.contains_key(&pos)
    }

    pub fn add_inhabited_time(&mut self, pos: ChunkPos, ticks: i64) {
        if let Some(loaded) = self.loaded.get_mut(&pos) {
            Arc::make_mut(&mut loaded.chunk).inhabited_time += ticks;
        }
    }

    pub fn load_unhandled(&self, pos: ChunkPos) -> bool {
        self.events.loaded.iter().any(|(loaded, _)| *loaded == pos)
    }

    pub fn is_pending(&self, pos: ChunkPos) -> bool {
        self.pending.contains_key(&pos)
    }

    pub fn chunk_mut(&mut self, pos: ChunkPos) -> Option<&mut ChunkColumn> {
        let loaded = self.loaded.get_mut(&pos)?;
        loaded.dirty = true;
        Some(Arc::make_mut(&mut loaded.chunk))
    }

    pub fn block(&self, pos: BlockPos) -> Option<BlockStateId> {
        self.get(pos.chunk())?.block(pos)
    }

    pub fn any_block_meta(&self) -> bool {
        self.loaded
            .values()
            .any(|loaded| loaded.chunk.block_meta.has_any())
    }

    fn meta_chunk_mut(&mut self, pos: BlockPos, volatile: bool) -> Option<&mut ChunkColumn> {
        if volatile {
            let loaded = self.loaded.get_mut(&pos.chunk())?;
            Some(Arc::make_mut(&mut loaded.chunk))
        } else {
            self.chunk_mut(pos.chunk())
        }
    }

    pub fn block_meta(&self, pos: BlockPos) -> Option<BlockMeta> {
        let chunk = self.get(pos.chunk())?;
        let meta = &chunk.block_meta;
        if !meta.has_any() {
            return None;
        }
        let held = BlockMeta {
            persistent: meta.persistent(pos).cloned().unwrap_or_default(),
            volatile: meta.volatile(pos).cloned().unwrap_or_default(),
        };
        (!held.is_empty()).then_some(held)
    }

    pub fn set_block_meta(
        &mut self,
        pos: BlockPos,
        key: &str,
        value: MetaValue,
        volatile: bool,
    ) -> Option<Result<(), MetaError>> {
        let chunk = self.meta_chunk_mut(pos, volatile)?;
        Some(chunk.set_block_meta(pos, key, value, volatile))
    }

    pub fn remove_block_meta(&mut self, pos: BlockPos, key: &str, volatile: bool) -> bool {
        if self.block_meta(pos).is_none() {
            return false;
        }
        self.meta_chunk_mut(pos, volatile)
            .is_some_and(|chunk| chunk.remove_block_meta(pos, key, volatile))
    }

    pub fn clear_block_meta(&mut self, pos: BlockPos, volatile: bool) -> bool {
        if self.block_meta(pos).is_none() {
            return false;
        }
        self.meta_chunk_mut(pos, volatile)
            .is_some_and(|chunk| chunk.clear_block_meta(pos, volatile))
    }

    pub fn take_block_meta(&mut self, pos: BlockPos) -> BlockMeta {
        if self.block_meta(pos).is_none() {
            return BlockMeta::default();
        }
        self.chunk_mut(pos.chunk())
            .map(|chunk| chunk.take_block_meta(pos))
            .unwrap_or_default()
    }

    pub fn restore_block_meta(&mut self, pos: BlockPos, meta: BlockMeta) {
        if meta.is_empty() {
            return;
        }
        if let Some(chunk) = self.chunk_mut(pos.chunk()) {
            chunk.restore_block_meta(pos, meta);
        }
    }

    pub fn blocks_with_meta(
        &self,
        key: &str,
        value: Option<&MetaValue>,
        volatile: bool,
        limit: usize,
    ) -> Vec<BlockPos> {
        let mut found = Vec::new();
        let mut chunks: Vec<ChunkPos> = self.loaded.keys().copied().collect();
        chunks.sort();
        for chunk_pos in chunks {
            let Some(chunk) = self.get(chunk_pos) else {
                continue;
            };
            let mut positions: Vec<BlockPos> = chunk.block_meta.positions().collect();
            positions.sort();
            positions.dedup();
            for pos in positions {
                let holder = chunk.block_meta.holder(pos, volatile);
                let matched = holder
                    .and_then(|holder| holder.get(key))
                    .is_some_and(|held| value.is_none_or(|wanted| *wanted == held));
                if matched {
                    found.push(pos);
                    if found.len() >= limit {
                        return found;
                    }
                }
            }
        }
        found
    }

    pub fn set_block(&mut self, pos: BlockPos, state: BlockStateId) -> Option<BlockStateId> {
        let current = self.block(pos)?;
        if current == state {
            return Some(current);
        }
        let chunk = self.chunk_mut(pos.chunk())?;
        let old = chunk.set_block(pos, state);
        if state.is_air() {
            chunk.remove_block_entity(pos);
        }
        self.poi.on_block_change(pos, current, state);
        if crate::lighting::affects_light(current, state) {
            *self.relight.entry(pos.chunk()).or_default() |= reach_mask(pos);
        }
        old
    }

    pub fn relight(&mut self) -> usize {
        self.relight_chunks().len()
    }

    pub fn relight_chunks(&mut self) -> Vec<ChunkPos> {
        let has_skylight = self.dimension.has_skylight;
        let mut edited: Vec<ChunkPos> = Vec::new();
        let mut recompute: Vec<ChunkPos> = Vec::new();
        for (pos, mask) in self.relight.drain() {
            edited.push(pos);
            recompute.push(pos);
            for (bit, (dx, dz)) in NEIGHBOURS.iter().enumerate() {
                if mask & (1 << bit) != 0 {
                    recompute.push(ChunkPos::new(pos.x + dx, pos.z + dz));
                }
            }
        }
        recompute.sort_unstable();
        recompute.dedup();
        recompute.retain(|pos| self.loaded.contains_key(pos));
        let before: Vec<(ChunkPos, rspigot_domain::chunk::ChunkLight)> = recompute
            .iter()
            .filter(|pos| !edited.contains(pos))
            .filter_map(|pos| Some((*pos, self.loaded.get(pos)?.chunk.light.clone())))
            .collect();
        let mut done = Vec::new();
        for pos in &recompute {
            if let Some(chunk) = self.chunk_mut(*pos) {
                crate::lighting::compute(chunk, has_skylight);
                if edited.contains(pos) {
                    done.push(*pos);
                }
            }
        }
        let mut stitch: Vec<ChunkPos> = self.stitch.drain().collect();
        stitch.extend(recompute);
        stitch.sort_unstable();
        stitch.dedup();
        for pos in stitch {
            if !self.loaded.contains_key(&pos) {
                continue;
            }
            for changed in crate::lighting::stitch(self, pos, has_skylight) {
                if !done.contains(&changed) {
                    done.push(changed);
                }
            }
        }
        for (pos, light) in before {
            let differs = self
                .loaded
                .get(&pos)
                .is_some_and(|loaded| loaded.chunk.light != light);
            if differs && !done.contains(&pos) {
                done.push(pos);
            }
        }
        done
    }

    pub fn stats(&self) -> ChunkManagerStats {
        ChunkManagerStats {
            loaded: self.loaded.len(),
            pending: self.pending.len(),
            ..self.stats
        }
    }

    pub fn loaded_positions(&self) -> impl Iterator<Item = ChunkPos> + '_ {
        self.loaded.keys().copied()
    }
}

impl crate::lighting::LightWorld for ChunkManager {
    fn light_chunk(&self, pos: ChunkPos) -> Option<&ChunkColumn> {
        self.loaded.get(&pos).map(|loaded| &*loaded.chunk)
    }

    fn light_chunk_mut(&mut self, pos: ChunkPos) -> Option<&mut ChunkColumn> {
        self.chunk_mut(pos)
    }
}

const NEIGHBOURS: [(i32, i32); 8] = [
    (-1, 0),
    (1, 0),
    (0, -1),
    (0, 1),
    (-1, -1),
    (1, -1),
    (-1, 1),
    (1, 1),
];

const LIGHT_REACH: i32 = 14;

fn reach_mask(pos: BlockPos) -> u8 {
    let (x, z) = (pos.x & 15, pos.z & 15);
    let (west, east, north, south) = (x + 1, 16 - x, z + 1, 16 - z);
    let reaches = [
        west,
        east,
        north,
        south,
        west + north,
        east + north,
        west + south,
        east + south,
    ];
    reaches
        .iter()
        .enumerate()
        .filter(|(_, distance)| **distance <= LIGHT_REACH)
        .fold(0, |mask, (bit, _)| mask | (1 << bit))
}

impl rspigot_domain::entity::BlockView for ChunkManager {
    fn block(&self, pos: BlockPos) -> Option<BlockStateId> {
        ChunkManager::block(self, pos)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::time::Instant;

    use rspigot_blocks::Block;
    use rspigot_domain::chunk::ChunkColumn;
    use rspigot_domain::{BiomeId, Identifier, Vec3};

    use super::*;
    use crate::ports::{ChunkGenerator, ChunkStore, StoreError};

    #[derive(Default)]
    struct MemoryStore {
        saved: Mutex<Vec<ChunkPos>>,
    }

    impl ChunkStore for MemoryStore {
        fn load(&self, _: &Identifier, _: ChunkPos) -> Result<Option<ChunkColumn>, StoreError> {
            Ok(None)
        }

        fn save(&self, _: &Identifier, chunk: &ChunkColumn) -> Result<(), StoreError> {
            if let Ok(mut saved) = self.saved.lock() {
                saved.push(chunk.pos());
            }
            Ok(())
        }

        fn flush(&self) -> Result<(), StoreError> {
            Ok(())
        }
    }

    impl crate::ports::EntityStore for MemoryStore {
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

    struct EmptyGenerator;

    impl ChunkGenerator for EmptyGenerator {
        fn generate(&self, dimension: &Dimension, pos: ChunkPos) -> ChunkColumn {
            ChunkColumn::empty(pos, dimension.height, BiomeId(0))
        }

        fn spawn_point(&self, _: &Dimension) -> Vec3 {
            Vec3::ZERO
        }
    }

    fn drain(manager: &mut ChunkManager, workers: &ChunkWorkers, expected: usize) -> Vec<ChunkPos> {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut loaded = Vec::new();
        while loaded.len() < expected && Instant::now() < deadline {
            if let Ok(result) = workers.results().recv_timeout(Duration::from_millis(50))
                && let Some(pos) = manager.accept(result)
            {
                loaded.push(pos);
            }
        }
        loaded
    }

    fn setup() -> (ChunkManager, ChunkWorkers, Arc<MemoryStore>) {
        let store = Arc::new(MemoryStore::default());
        let workers = ChunkWorkers::spawn(2, Arc::clone(&store), Arc::new(EmptyGenerator));
        let manager = ChunkManager::new(
            Arc::new(Dimension::overworld()),
            ChunkManagerConfig {
                unload_delay_ticks: 3,
                autosave_interval_ticks: 0,
            },
        );
        (manager, workers, store)
    }

    struct StaleLightStore {
        daisy: BlockPos,
    }

    impl StaleLightStore {
        fn chunk(&self) -> ChunkColumn {
            let pos = self.daisy.chunk();
            let mut chunk = ChunkColumn::empty(pos, Dimension::overworld().height, BiomeId(0));
            for x in 0..16 {
                for z in 0..16 {
                    chunk.set_block(
                        BlockPos::new(pos.min_block_x() + x, 63, pos.min_block_z() + z),
                        BlockStateId(1),
                    );
                }
            }
            chunk.set_block(self.daisy, rspigot_blocks::OxeyeDaisy::DEFAULT_STATE);
            crate::lighting::compute(&mut chunk, true);
            let index = ((self.daisy.y >> 4) - chunk.height().min_section() + 1) as usize;
            if let Some(layer) = chunk.light.sky[index].as_mut() {
                layer.set(
                    (self.daisy.x & 15) as usize,
                    (self.daisy.y & 15) as usize,
                    (self.daisy.z & 15) as usize,
                    0,
                );
            }
            chunk
        }
    }

    impl ChunkStore for StaleLightStore {
        fn load(&self, _: &Identifier, pos: ChunkPos) -> Result<Option<ChunkColumn>, StoreError> {
            Ok((pos == self.daisy.chunk()).then(|| self.chunk()))
        }

        fn save(&self, _: &Identifier, _: &ChunkColumn) -> Result<(), StoreError> {
            Ok(())
        }

        fn flush(&self) -> Result<(), StoreError> {
            Ok(())
        }
    }

    impl crate::ports::EntityStore for StaleLightStore {
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

    #[test]
    fn a_stored_chunk_with_a_dark_flower_is_relit_and_saved_again() {
        let daisy = BlockPos::new(4, 64, 4);
        let workers = ChunkWorkers::spawn(
            1,
            Arc::new(StaleLightStore { daisy }),
            Arc::new(EmptyGenerator),
        );
        let mut manager = ChunkManager::new(
            Arc::new(Dimension::overworld()),
            ChunkManagerConfig {
                unload_delay_ticks: 3,
                autosave_interval_ticks: 0,
            },
        );
        manager.add_ticket(daisy.chunk(), Priority(1), &workers);
        assert_eq!(drain(&mut manager, &workers, 1), vec![daisy.chunk()]);

        let chunk = manager.get(daisy.chunk()).expect("loaded");
        assert_eq!(crate::lighting::light_level(chunk, daisy, true), 15);
        assert_eq!(manager.stats().loaded_from_disk, 1);
        assert_eq!(manager.save_dirty(&workers), 1);
        workers.shutdown();
    }

    #[test]
    fn ticket_loads_chunk_and_release_unloads_after_delay() {
        let (mut manager, workers, store) = setup();
        let pos = ChunkPos::new(1, 2);
        manager.add_ticket(pos, Priority(1), &workers);
        assert!(manager.is_pending(pos));
        assert_eq!(drain(&mut manager, &workers, 1), vec![pos]);
        assert!(manager.is_loaded(pos));
        manager.remove_ticket(pos);
        assert!(manager.tick().is_empty());
        assert!(manager.tick().is_empty());
        assert!(manager.is_loaded(pos));
        assert_eq!(manager.tick(), [pos]);
        assert!(manager.is_loaded(pos));
        assert!(manager.unload(pos, true, &workers));
        assert!(!manager.is_loaded(pos));
        assert!(!manager.unload(pos, true, &workers));
        let deadline = Instant::now() + Duration::from_secs(10);
        while store.saved.lock().map(|s| s.is_empty()).unwrap_or(true) && Instant::now() < deadline
        {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(
            store.saved.lock().map(|s| s.clone()).unwrap_or_default(),
            vec![pos]
        );
        workers.shutdown();
    }

    #[test]
    fn a_timed_ticket_simulates_then_lapses_unless_refreshed() {
        let (mut manager, workers, _store) = setup();
        let pos = ChunkPos::new(7, 7);
        manager.hold_timed(pos, 5, true, &workers);
        assert!(manager.is_held(pos));
        assert_eq!(drain(&mut manager, &workers, 1), vec![pos]);
        assert!(manager.simulates(pos));
        assert_eq!(manager.simulated_chunks(), vec![pos]);
        for _ in 0..3 {
            manager.tick();
        }
        manager.hold_timed(pos, 5, true, &workers);
        for _ in 0..3 {
            manager.tick();
        }
        assert!(manager.is_held(pos));
        for _ in 0..3 {
            manager.tick();
        }
        assert!(!manager.is_held(pos));
        assert!(!manager.simulates(pos));
        workers.shutdown();
    }

    #[test]
    fn a_forced_chunk_keeps_its_ticket_until_it_is_released() {
        let (mut manager, workers, _store) = setup();
        let pos = ChunkPos::new(-3, 4);
        assert!(manager.set_forced(pos, true, &workers));
        assert!(!manager.set_forced(pos, true, &workers));
        assert!(manager.is_forced(pos));
        assert_eq!(drain(&mut manager, &workers, 1), vec![pos]);
        for _ in 0..20 {
            assert!(manager.tick().is_empty());
        }
        assert!(manager.is_loaded(pos));
        assert!(manager.set_forced(pos, false, &workers));
        assert!(!manager.set_forced(pos, false, &workers));
        assert!(!manager.is_forced(pos));
        workers.shutdown();
    }

    #[test]
    fn forced_chunks_list_in_the_vanilla_long_order() {
        let (mut manager, workers, _store) = setup();
        for (x, z) in [(5, 1), (-1, 1), (0, 0), (2, -1)] {
            manager.set_forced(ChunkPos::new(x, z), true, &workers);
        }
        let listed: Vec<(i32, i32)> = manager
            .forced_chunks()
            .into_iter()
            .map(|pos| (pos.x, pos.z))
            .collect();
        assert_eq!(listed, [(2, -1), (0, 0), (5, 1), (-1, 1)]);
        workers.shutdown();
    }

    struct BellStore {
        bell: BlockPos,
    }

    impl ChunkStore for BellStore {
        fn load(&self, _: &Identifier, pos: ChunkPos) -> Result<Option<ChunkColumn>, StoreError> {
            Ok((pos == self.bell.chunk()).then(|| {
                let mut chunk = ChunkColumn::empty(pos, Dimension::overworld().height, BiomeId(0));
                chunk.set_block(self.bell, rspigot_blocks::BlockKind::Bell.default_state());
                chunk
            }))
        }

        fn save(&self, _: &Identifier, _: &ChunkColumn) -> Result<(), StoreError> {
            Ok(())
        }

        fn flush(&self) -> Result<(), StoreError> {
            Ok(())
        }
    }

    impl crate::ports::EntityStore for BellStore {
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

    #[test]
    fn points_of_interest_follow_the_loaded_chunks_and_their_block_changes() {
        let bell = BlockPos::new(4, 64, 4);
        let workers =
            ChunkWorkers::spawn(1, Arc::new(BellStore { bell }), Arc::new(EmptyGenerator));
        let mut manager = ChunkManager::new(
            Arc::new(Dimension::overworld()),
            ChunkManagerConfig {
                unload_delay_ticks: 3,
                autosave_interval_ticks: 0,
            },
        );
        assert_eq!(manager.poi().record_count(), 0);
        manager.add_ticket(bell.chunk(), Priority(1), &workers);
        drain(&mut manager, &workers, 1);
        assert_eq!(
            manager.poi().type_at(bell),
            Some(rspigot_domain::poi::PoiType::Meeting)
        );
        let lectern = BlockPos::new(5, 64, 4);
        manager.set_block(lectern, rspigot_blocks::BlockKind::Lectern.default_state());
        assert_eq!(
            manager.poi().type_at(lectern),
            Some(rspigot_domain::poi::PoiType::Librarian)
        );
        manager.set_block(lectern, BlockStateId::AIR);
        assert_eq!(manager.poi().type_at(lectern), None);
        manager.set_block(bell, BlockStateId::AIR);
        assert_eq!(manager.poi().record_count(), 0);
        manager.set_block(bell, rspigot_blocks::BlockKind::Bell.default_state());
        assert_eq!(manager.poi().record_count(), 1);
        manager.remove_ticket(bell.chunk());
        (0..3).for_each(|_| {
            manager.tick();
        });
        assert!(manager.unload(bell.chunk(), false, &workers));
        assert_eq!(manager.poi().record_count(), 0);
        workers.shutdown();
    }

    #[test]
    fn re_adding_ticket_before_delay_keeps_chunk() {
        let (mut manager, workers, _) = setup();
        let pos = ChunkPos::new(0, 0);
        manager.add_ticket(pos, Priority(1), &workers);
        drain(&mut manager, &workers, 1);
        manager.remove_ticket(pos);
        manager.tick();
        manager.add_ticket(pos, Priority(1), &workers);
        for _ in 0..10 {
            assert!(manager.tick().is_empty());
        }
        assert!(!manager.unload_due(pos));
        assert!(!manager.unload(pos, true, &workers));
        assert!(manager.is_loaded(pos));
        workers.shutdown();
    }

    #[test]
    fn an_unload_without_a_save_drops_the_changes_of_its_chunk() {
        let (mut manager, workers, store) = setup();
        let pos = ChunkPos::new(3, 4);
        manager.add_ticket(pos, Priority(1), &workers);
        drain(&mut manager, &workers, 1);
        manager.remove_ticket(pos);
        let due = (0..3).flat_map(|_| manager.tick()).collect::<Vec<_>>();
        assert_eq!(due, [pos]);
        assert!(manager.unload(pos, false, &workers));
        assert_eq!(manager.take_events().unloaded, [pos]);
        std::thread::sleep(Duration::from_millis(50));
        assert!(store.saved.lock().map(|s| s.is_empty()).unwrap_or(false));
        workers.shutdown();
    }

    #[test]
    fn released_pending_chunk_is_cancelled() {
        let (mut manager, workers, _) = setup();
        let pos = ChunkPos::new(5, 5);
        manager.add_ticket(pos, Priority(1), &workers);
        manager.remove_ticket(pos);
        assert!(!manager.is_pending(pos));
        assert_eq!(manager.stats().cancelled, 1);
        std::thread::sleep(Duration::from_millis(50));
        while let Ok(result) = workers.results().try_recv() {
            assert_eq!(manager.accept(result), None);
        }
        assert!(!manager.is_loaded(pos));
        workers.shutdown();
    }

    #[test]
    fn light_crosses_chunk_borders_when_a_neighbour_loads_and_when_a_source_goes() {
        let (mut manager, workers, _) = setup();
        let (west, east) = (ChunkPos::new(0, 0), ChunkPos::new(1, 0));
        manager.add_ticket(west, Priority(1), &workers);
        drain(&mut manager, &workers, 1);
        let torch = BlockPos::new(15, 64, 8);
        manager.set_block(torch, rspigot_blocks::Torch::DEFAULT_STATE);
        manager.relight();
        manager.add_ticket(east, Priority(1), &workers);
        drain(&mut manager, &workers, 1);
        let beyond = BlockPos::new(17, 64, 8);
        let level = |manager: &ChunkManager| {
            manager
                .get(east)
                .map(|chunk| crate::lighting::light_level(chunk, beyond, false))
        };
        assert_eq!(level(&manager), Some(0));
        let done = manager.relight_chunks();
        assert!(done.contains(&east));
        assert_eq!(level(&manager), Some(12));
        manager.set_block(torch, BlockStateId::AIR);
        let done = manager.relight_chunks();
        assert!(done.contains(&east));
        assert_eq!(level(&manager), Some(0));
        workers.shutdown();
    }

    #[test]
    fn block_changes_relight_their_chunk_once_per_call() {
        let (mut manager, workers, _) = setup();
        let pos = ChunkPos::new(0, 0);
        manager.add_ticket(pos, Priority(1), &workers);
        drain(&mut manager, &workers, 1);
        let lit = |manager: &ChunkManager, y| {
            manager
                .get(pos)
                .map(|chunk| crate::lighting::light_level(chunk, BlockPos::new(1, y, 1), true))
        };
        assert_eq!(lit(&manager, 9), Some(15));
        manager.set_block(BlockPos::new(1, 10, 1), BlockStateId(1));
        assert_eq!(manager.relight(), 1);
        assert_eq!(lit(&manager, 9), Some(14));
        assert_eq!(manager.relight(), 0);
        manager.set_block(BlockPos::new(1, 10, 1), BlockStateId(1));
        assert_eq!(manager.relight(), 0);
        workers.shutdown();
    }

    #[test]
    fn set_block_marks_chunk_dirty_and_copies_on_write() {
        let (mut manager, workers, _) = setup();
        let pos = ChunkPos::new(0, 0);
        manager.add_ticket(pos, Priority(1), &workers);
        drain(&mut manager, &workers, 1);
        let snapshot = manager.get(pos).map(Arc::clone);
        manager.set_block(BlockPos::new(1, 10, 1), BlockStateId(1));
        assert_eq!(
            manager.block(BlockPos::new(1, 10, 1)),
            Some(BlockStateId(1))
        );
        assert_eq!(
            snapshot.and_then(|s| s.block(BlockPos::new(1, 10, 1))),
            Some(BlockStateId::AIR)
        );
        workers.shutdown();
    }
}
