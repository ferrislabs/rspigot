use std::sync::Arc;

use rspigot_domain::ChunkPos;
use rustc_hash::FxHashSet;

use crate::chunks::{ChunkManager, ChunkWorkers, Priority};
use crate::events::ClientEvent;
use crate::ports::ClientLink;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StreamConfig {
    pub initial_chunks_per_tick: f32,
    pub max_chunks_per_tick: f32,
    pub max_unacknowledged_batches: u32,
    pub prefetch_margin: i32,
}

impl Default for StreamConfig {
    fn default() -> Self {
        Self {
            initial_chunks_per_tick: 32.0,
            max_chunks_per_tick: 64.0,
            max_unacknowledged_batches: 4,
            prefetch_margin: 1,
        }
    }
}

pub fn is_within_view(
    center: ChunkPos,
    view_distance: i32,
    pos: ChunkPos,
    include_outer: bool,
) -> bool {
    let dx = i64::from(((pos.x - center.x).abs() - 1).max(0));
    let dz = i64::from(((pos.z - center.z).abs() - 1).max(0));
    let far = (dx.max(dz) - i64::from(include_outer)).max(0);
    let near = dx.min(dz);
    near * near + far * far < i64::from(view_distance) * i64::from(view_distance)
}

fn priority(center: ChunkPos, pos: ChunkPos, view_distance: i32) -> Priority {
    let distance = center.squared_distance(pos) as u32;
    if is_within_view(center, view_distance, pos, true) {
        Priority(distance)
    } else {
        Priority(distance + (1 << 20))
    }
}

pub struct ChunkStreamer {
    config: StreamConfig,
    center: ChunkPos,
    view_distance: i32,
    tickets: FxHashSet<ChunkPos>,
    sent: FxHashSet<ChunkPos>,
    chunks_per_tick: f32,
    batch_quota: f32,
    unacknowledged_batches: u32,
    skipped: bool,
}

impl ChunkStreamer {
    pub fn new(config: StreamConfig, center: ChunkPos, view_distance: u8) -> Self {
        Self {
            config,
            center,
            view_distance: i32::from(view_distance.max(2)),
            tickets: FxHashSet::default(),
            sent: FxHashSet::default(),
            chunks_per_tick: config.initial_chunks_per_tick,
            batch_quota: 0.0,
            unacknowledged_batches: 0,
            skipped: false,
        }
    }

    pub fn is_skipped(&self) -> bool {
        self.skipped
    }

    pub fn set_skipped(
        &mut self,
        skipped: bool,
        manager: &mut ChunkManager,
        workers: &ChunkWorkers,
        link: &impl ClientLink,
    ) {
        if self.skipped == skipped {
            return;
        }
        self.skipped = skipped;
        self.refresh(manager, workers, link);
    }

    pub fn center(&self) -> ChunkPos {
        self.center
    }

    pub fn view_distance(&self) -> i32 {
        self.view_distance
    }

    pub fn has_sent(&self, pos: ChunkPos) -> bool {
        self.sent.contains(&pos)
    }

    pub fn sent_count(&self) -> usize {
        self.sent.len()
    }

    fn ticket_radius(&self) -> i32 {
        self.view_distance + 1 + self.config.prefetch_margin
    }

    fn wanted(&self, center: ChunkPos) -> impl Iterator<Item = ChunkPos> + '_ {
        let radius = if self.skipped {
            -1
        } else {
            self.ticket_radius()
        };
        let ticket_view = self.view_distance + self.config.prefetch_margin;
        (-radius..=radius).flat_map(move |dx| {
            (-radius..=radius).filter_map(move |dz| {
                let pos = ChunkPos::new(center.x + dx, center.z + dz);
                is_within_view(center, ticket_view, pos, true).then_some(pos)
            })
        })
    }

    pub fn start(
        &mut self,
        manager: &mut ChunkManager,
        workers: &ChunkWorkers,
        link: &impl ClientLink,
    ) {
        link.send(ClientEvent::CenterChunk(self.center));
        let wanted: Vec<ChunkPos> = self.wanted(self.center).collect();
        for pos in wanted {
            self.acquire(pos, manager, workers);
        }
    }

    fn acquire(&mut self, pos: ChunkPos, manager: &mut ChunkManager, workers: &ChunkWorkers) {
        if self.tickets.insert(pos) {
            manager.add_ticket(pos, priority(self.center, pos, self.view_distance), workers);
        }
    }

    pub fn move_to(
        &mut self,
        center: ChunkPos,
        manager: &mut ChunkManager,
        workers: &ChunkWorkers,
        link: &impl ClientLink,
    ) {
        if center == self.center {
            return;
        }
        self.center = center;
        link.send(ClientEvent::CenterChunk(center));
        self.refresh(manager, workers, link);
    }

    pub fn set_view_distance(
        &mut self,
        view_distance: u8,
        manager: &mut ChunkManager,
        workers: &ChunkWorkers,
        link: &impl ClientLink,
    ) {
        let view_distance = i32::from(view_distance.max(2));
        if view_distance == self.view_distance {
            return;
        }
        self.view_distance = view_distance;
        self.refresh(manager, workers, link);
    }

    fn refresh(
        &mut self,
        manager: &mut ChunkManager,
        workers: &ChunkWorkers,
        link: &impl ClientLink,
    ) {
        let wanted: FxHashSet<ChunkPos> = self.wanted(self.center).collect();
        let released: Vec<ChunkPos> = self
            .tickets
            .iter()
            .filter(|p| !wanted.contains(p))
            .copied()
            .collect();
        for pos in released {
            self.tickets.remove(&pos);
            manager.remove_ticket(pos);
        }
        let forgotten: Vec<ChunkPos> = self
            .sent
            .iter()
            .filter(|p| !is_within_view(self.center, self.view_distance, **p, true))
            .copied()
            .collect();
        for pos in forgotten {
            self.sent.remove(&pos);
            link.send(ClientEvent::ForgetChunk(pos));
        }
        for pos in wanted {
            if self.tickets.contains(&pos) {
                manager.reprioritize(pos, priority(self.center, pos, self.view_distance), workers);
            } else {
                self.acquire(pos, manager, workers);
            }
        }
    }

    pub fn on_batch_received(&mut self, chunks_per_tick: f32) {
        self.unacknowledged_batches = self.unacknowledged_batches.saturating_sub(1);
        self.chunks_per_tick = if chunks_per_tick.is_nan() {
            0.01
        } else {
            chunks_per_tick.clamp(0.01, self.config.max_chunks_per_tick)
        };
        if self.unacknowledged_batches == 0 {
            self.batch_quota = 1.0;
        }
    }

    pub fn tick(&mut self, manager: &ChunkManager, link: &impl ClientLink) -> usize {
        if self.unacknowledged_batches >= self.config.max_unacknowledged_batches {
            return 0;
        }
        self.batch_quota =
            (self.batch_quota + self.chunks_per_tick).min(self.chunks_per_tick.max(1.0));
        let budget = self.batch_quota.floor() as usize;
        if budget == 0 {
            return 0;
        }
        let mut ready: Vec<(i64, ChunkPos)> = self
            .tickets
            .iter()
            .filter(|p| {
                !self.sent.contains(p)
                    && is_within_view(self.center, self.view_distance, **p, true)
                    && manager.is_loaded(**p)
            })
            .map(|p| (self.center.squared_distance(*p), *p))
            .collect();
        if ready.is_empty() {
            return 0;
        }
        if ready.len() > budget {
            ready.select_nth_unstable(budget - 1);
            ready.truncate(budget);
        }
        ready.sort_unstable();
        link.send(ClientEvent::ChunkBatchStart);
        let dimension = manager.dimension().key.clone();
        let mut count = 0u32;
        for (_, pos) in &ready {
            if let Some(chunk) = manager.get(*pos) {
                link.send(ClientEvent::Chunk {
                    dimension: dimension.clone(),
                    chunk: Arc::clone(chunk),
                });
                self.sent.insert(*pos);
                count += 1;
            }
        }
        link.send(ClientEvent::ChunkBatchFinished { count });
        self.unacknowledged_batches += 1;
        self.batch_quota -= count as f32;
        count as usize
    }

    pub fn release_all(&mut self, manager: &mut ChunkManager) {
        for pos in self.tickets.drain() {
            manager.remove_ticket(pos);
        }
        self.sent.clear();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    use rspigot_domain::chunk::ChunkColumn;
    use rspigot_domain::text::Component;
    use rspigot_domain::world::Dimension;
    use rspigot_domain::{BiomeId, Identifier, Vec3};

    use super::*;
    use crate::chunks::ChunkManagerConfig;
    use crate::ports::{ChunkGenerator, ChunkStore, StoreError};

    #[derive(Default)]
    struct RecordingLink {
        events: Mutex<Vec<ClientEvent>>,
    }

    impl RecordingLink {
        fn take(&self) -> Vec<ClientEvent> {
            self.events
                .lock()
                .map(|mut e| std::mem::take(&mut *e))
                .unwrap_or_default()
        }
    }

    impl ClientLink for RecordingLink {
        fn send(&self, event: ClientEvent) {
            if let Ok(mut events) = self.events.lock() {
                events.push(event);
            }
        }

        fn disconnect(&self, _: Component) {}

        fn is_connected(&self) -> bool {
            true
        }
    }

    struct NoStore;

    impl ChunkStore for NoStore {
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

    impl crate::ports::EntityStore for NoStore {
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

    struct EmptyGenerator;

    impl ChunkGenerator for EmptyGenerator {
        fn generate(&self, dimension: &Dimension, pos: ChunkPos) -> ChunkColumn {
            ChunkColumn::empty(pos, dimension.height, BiomeId(0))
        }

        fn spawn_point(&self, _: &Dimension) -> Vec3 {
            Vec3::ZERO
        }
    }

    fn load_all(manager: &mut ChunkManager, workers: &ChunkWorkers) {
        let deadline = Instant::now() + Duration::from_secs(20);
        while manager.stats().pending > 0 && Instant::now() < deadline {
            if let Ok(result) = workers.results().recv_timeout(Duration::from_millis(20)) {
                manager.accept(result);
            }
        }
    }

    fn sent_chunks(events: &[ClientEvent]) -> Vec<ChunkPos> {
        events
            .iter()
            .filter_map(|e| match e {
                ClientEvent::Chunk { chunk, .. } => Some(chunk.pos()),
                _ => None,
            })
            .collect()
    }

    fn setup() -> (ChunkManager, ChunkWorkers) {
        let workers = ChunkWorkers::spawn(4, Arc::new(NoStore), Arc::new(EmptyGenerator));
        let manager = ChunkManager::new(
            Arc::new(Dimension::overworld()),
            ChunkManagerConfig::default(),
        );
        (manager, workers)
    }

    #[test]
    fn view_check_matches_vanilla_cylinder() {
        let center = ChunkPos::new(0, 0);
        assert!(is_within_view(center, 2, ChunkPos::new(2, 0), false));
        assert!(!is_within_view(center, 2, ChunkPos::new(3, 0), false));
        assert!(is_within_view(center, 2, ChunkPos::new(3, 0), true));
        assert!(!is_within_view(center, 2, ChunkPos::new(3, 3), true));
    }

    #[test]
    fn sends_nearest_first_in_batches_without_duplicates() {
        let (mut manager, workers) = setup();
        let link = RecordingLink::default();
        let mut streamer = ChunkStreamer::new(StreamConfig::default(), ChunkPos::new(0, 0), 4);
        streamer.start(&mut manager, &workers, &link);
        load_all(&mut manager, &workers);
        link.take();
        streamer.tick(&manager, &link);
        let first = link.take();
        assert!(matches!(first.first(), Some(ClientEvent::ChunkBatchStart)));
        assert!(matches!(
            first.last(),
            Some(ClientEvent::ChunkBatchFinished { count: 32 })
        ));
        let chunks = sent_chunks(&first);
        assert_eq!(chunks[0], ChunkPos::new(0, 0));
        let mut all = chunks.clone();
        for _ in 0..20 {
            streamer.on_batch_received(32.0);
            streamer.tick(&manager, &link);
            all.extend(sent_chunks(&link.take()));
        }
        let unique: FxHashSet<ChunkPos> = all.iter().copied().collect();
        assert_eq!(unique.len(), all.len());
        assert!(
            all.iter()
                .all(|p| is_within_view(ChunkPos::new(0, 0), 4, *p, true))
        );
        assert_eq!(all.len(), streamer.sent_count());
        workers.shutdown();
    }

    #[test]
    fn stops_when_too_many_batches_unacknowledged() {
        let (mut manager, workers) = setup();
        let link = RecordingLink::default();
        let config = StreamConfig {
            initial_chunks_per_tick: 2.0,
            max_unacknowledged_batches: 2,
            ..StreamConfig::default()
        };
        let mut streamer = ChunkStreamer::new(config, ChunkPos::new(0, 0), 4);
        streamer.start(&mut manager, &workers, &link);
        load_all(&mut manager, &workers);
        assert_eq!(streamer.tick(&manager, &link), 2);
        assert_eq!(streamer.tick(&manager, &link), 2);
        assert_eq!(streamer.tick(&manager, &link), 0);
        streamer.on_batch_received(2.0);
        assert_eq!(streamer.tick(&manager, &link), 2);
        assert_eq!(streamer.tick(&manager, &link), 0);
        workers.shutdown();
    }

    #[test]
    fn moving_forgets_far_chunks_and_releases_tickets() {
        let (mut manager, workers) = setup();
        let link = RecordingLink::default();
        let mut streamer = ChunkStreamer::new(StreamConfig::default(), ChunkPos::new(0, 0), 2);
        streamer.start(&mut manager, &workers, &link);
        load_all(&mut manager, &workers);
        for _ in 0..5 {
            streamer.tick(&manager, &link);
            streamer.on_batch_received(64.0);
        }
        link.take();
        streamer.move_to(ChunkPos::new(10, 0), &mut manager, &workers, &link);
        let events = link.take();
        assert!(
            matches!(events.first(), Some(ClientEvent::CenterChunk(p)) if *p == ChunkPos::new(10, 0))
        );
        let forgotten = events
            .iter()
            .filter(|e| matches!(e, ClientEvent::ForgetChunk(_)))
            .count();
        assert!(forgotten > 0);
        assert_eq!(streamer.sent_count(), 0);
        workers.shutdown();
    }
}
