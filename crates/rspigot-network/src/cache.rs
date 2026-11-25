use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use bytes::Bytes;
use rspigot_core::events::ClientEvent;
use rspigot_domain::{ChunkPos, Identifier};

pub type ChunkKey = (Identifier, ChunkPos);

const FRAME_OVERHEAD: u64 = 64;
const SWEEP_INTERVAL: Duration = Duration::from_secs(1);

struct Entry {
    revision: u64,
    frame: Bytes,
    last_used: Instant,
}

impl Entry {
    fn weight(&self) -> u64 {
        self.frame.len() as u64 + FRAME_OVERHEAD
    }
}

struct State {
    entries: HashMap<ChunkKey, Entry>,
    bytes: u64,
    last_sweep: Option<Instant>,
}

impl State {
    fn sweep(&mut self, now: Instant, idle: Duration) {
        let due = self
            .last_sweep
            .is_none_or(|last| now.saturating_duration_since(last) >= SWEEP_INTERVAL);
        if !due {
            return;
        }
        self.last_sweep = Some(now);
        let mut freed = 0;
        self.entries.retain(|_, entry| {
            let keep = now.saturating_duration_since(entry.last_used) < idle;
            if !keep {
                freed += entry.weight();
            }
            keep
        });
        self.bytes -= freed;
    }

    fn evict_oldest(&mut self) -> bool {
        let victim = self
            .entries
            .iter()
            .min_by_key(|(_, entry)| entry.last_used)
            .map(|(key, _)| key.clone());
        match victim.and_then(|key| self.entries.remove(&key)) {
            Some(entry) => {
                self.bytes -= entry.weight();
                true
            }
            None => false,
        }
    }

    fn insert(&mut self, key: ChunkKey, entry: Entry, max_bytes: u64) {
        if let Some(stale) = self.entries.remove(&key) {
            self.bytes -= stale.weight();
        }
        let incoming = entry.weight();
        if incoming > max_bytes {
            return;
        }
        while self.bytes + incoming > max_bytes && self.evict_oldest() {}
        self.bytes += incoming;
        self.entries.insert(key, entry);
    }
}

pub struct ChunkPacketCache {
    state: Mutex<State>,
    max_bytes: u64,
    idle: Duration,
}

impl ChunkPacketCache {
    pub fn new(max_bytes: u64, idle: Duration) -> Self {
        Self {
            state: Mutex::new(State {
                entries: HashMap::new(),
                bytes: 0,
                last_sweep: None,
            }),
            max_bytes,
            idle,
        }
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn get_or_build<E>(
        &self,
        key: ChunkKey,
        revision: u64,
        now: Instant,
        build: impl FnOnce() -> Result<Bytes, E>,
    ) -> Result<Bytes, E> {
        let hit = {
            let mut state = self.lock();
            let hit = match state.entries.get_mut(&key) {
                Some(entry) if entry.revision == revision => {
                    entry.last_used = now;
                    Some(entry.frame.clone())
                }
                _ => None,
            };
            state.sweep(now, self.idle);
            hit
        };
        if let Some(frame) = hit {
            return Ok(frame);
        }
        let frame = build()?;
        let entry = Entry {
            revision,
            frame: frame.clone(),
            last_used: now,
        };
        self.lock().insert(key, entry, self.max_bytes);
        Ok(frame)
    }

    pub fn len(&self) -> usize {
        self.lock().entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.lock().entries.is_empty()
    }

    pub fn bytes(&self) -> u64 {
        self.lock().bytes
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::time::{Duration, Instant};

    use super::*;

    const IDLE: Duration = Duration::from_secs(10);

    fn key(x: i32) -> ChunkKey {
        (Identifier::minecraft("overworld"), ChunkPos::new(x, 0))
    }

    fn frame(len: usize) -> Bytes {
        Bytes::from(vec![7u8; len])
    }

    fn fetch(
        cache: &ChunkPacketCache,
        x: i32,
        revision: u64,
        now: Instant,
        len: usize,
    ) -> (Bytes, bool) {
        let built = Cell::new(false);
        let result: Result<Bytes, ()> = cache.get_or_build(key(x), revision, now, || {
            built.set(true);
            Ok(frame(len))
        });
        (result.unwrap(), built.get())
    }

    #[test]
    fn builds_once_per_revision() {
        let cache = ChunkPacketCache::new(1 << 20, IDLE);
        let now = Instant::now();
        let builds = Cell::new(0);
        for _ in 0..3 {
            let frame: Result<Bytes, ()> = cache.get_or_build(key(1), 3, now, || {
                builds.set(builds.get() + 1);
                Ok(Bytes::from_static(b"frame"))
            });
            assert_eq!(frame.as_deref(), Ok(&b"frame"[..]));
        }
        assert_eq!(builds.get(), 1);
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn new_revision_replaces_old_frame() {
        let cache = ChunkPacketCache::new(1 << 20, IDLE);
        let now = Instant::now();
        let builds = Cell::new(0);
        let first: Result<Bytes, ()> = cache.get_or_build(key(1), 3, now, || {
            builds.set(builds.get() + 1);
            Ok(Bytes::from_static(b"frame"))
        });
        assert!(first.is_ok());
        assert_eq!(cache.bytes(), 5 + 64);
        let next: Result<Bytes, ()> = cache.get_or_build(key(1), 4, now, || {
            builds.set(builds.get() + 1);
            Ok(Bytes::from_static(b"frame-two"))
        });
        assert_eq!(next.as_deref(), Ok(&b"frame-two"[..]));
        assert_eq!(builds.get(), 2);
        assert_eq!(cache.len(), 1);
        assert_eq!(cache.bytes(), 9 + 64);
    }

    #[test]
    fn build_error_is_returned_and_nothing_is_cached() {
        let cache = ChunkPacketCache::new(1 << 20, IDLE);
        let result: Result<Bytes, &str> =
            cache.get_or_build(key(1), 1, Instant::now(), || Err("boom"));
        assert_eq!(result, Err("boom"));
        assert!(cache.is_empty());
        assert_eq!(cache.bytes(), 0);
    }

    #[test]
    fn idle_entry_is_evicted_on_a_later_call() {
        let cache = ChunkPacketCache::new(1 << 20, IDLE);
        let t0 = Instant::now();
        fetch(&cache, 1, 1, t0, 10);
        fetch(&cache, 2, 1, t0, 10);
        assert_eq!(cache.len(), 2);
        fetch(&cache, 3, 1, t0 + Duration::from_secs(11), 10);
        assert_eq!(cache.len(), 1);
        assert_eq!(cache.bytes(), 10 + 64);
        let (_, rebuilt) = fetch(&cache, 1, 1, t0 + Duration::from_secs(11), 10);
        assert!(rebuilt);
    }

    #[test]
    fn entry_used_within_idle_survives() {
        let cache = ChunkPacketCache::new(1 << 20, IDLE);
        let t0 = Instant::now();
        fetch(&cache, 1, 1, t0, 10);
        fetch(&cache, 2, 1, t0, 10);
        let (_, rebuilt) = fetch(&cache, 1, 1, t0 + Duration::from_secs(8), 10);
        assert!(!rebuilt);
        fetch(&cache, 3, 1, t0 + Duration::from_secs(12), 10);
        assert_eq!(cache.len(), 2);
        let (_, rebuilt) = fetch(&cache, 1, 1, t0 + Duration::from_secs(12), 10);
        assert!(!rebuilt);
        let (_, rebuilt) = fetch(&cache, 2, 1, t0 + Duration::from_secs(12), 10);
        assert!(rebuilt);
    }

    #[test]
    fn sweeps_at_most_once_per_second() {
        let cache = ChunkPacketCache::new(1 << 20, Duration::from_millis(100));
        let t0 = Instant::now();
        fetch(&cache, 1, 1, t0, 10);
        fetch(&cache, 2, 1, t0 + Duration::from_millis(500), 10);
        assert_eq!(cache.len(), 2);
        fetch(&cache, 3, 1, t0 + Duration::from_millis(1000), 10);
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn least_recently_used_entry_is_evicted_first() {
        let weight = 10 + 64;
        let cache = ChunkPacketCache::new(3 * weight, Duration::from_secs(3600));
        let t0 = Instant::now();
        fetch(&cache, 1, 1, t0, 10);
        fetch(&cache, 2, 1, t0 + Duration::from_secs(1), 10);
        fetch(&cache, 3, 1, t0 + Duration::from_secs(2), 10);
        fetch(&cache, 1, 1, t0 + Duration::from_secs(3), 10);
        assert_eq!(cache.bytes(), 3 * weight);
        fetch(&cache, 4, 1, t0 + Duration::from_secs(4), 10);
        assert_eq!(cache.len(), 3);
        assert_eq!(cache.bytes(), 3 * weight);
        let at = t0 + Duration::from_secs(5);
        assert!(!fetch(&cache, 1, 1, at, 10).1);
        assert!(!fetch(&cache, 3, 1, at, 10).1);
        assert!(!fetch(&cache, 4, 1, at, 10).1);
        assert!(fetch(&cache, 2, 1, at, 10).1);
    }

    #[test]
    fn frame_heavier_than_the_cap_is_returned_but_not_cached() {
        let cache = ChunkPacketCache::new(100, IDLE);
        let t0 = Instant::now();
        let (bytes, built) = fetch(&cache, 1, 1, t0, 100);
        assert!(built);
        assert_eq!(bytes.len(), 100);
        assert!(cache.is_empty());
        assert_eq!(cache.bytes(), 0);
        let (_, built) = fetch(&cache, 1, 1, t0, 100);
        assert!(built);
    }

    #[test]
    fn oversized_new_revision_drops_the_stale_frame() {
        let cache = ChunkPacketCache::new(100, IDLE);
        let t0 = Instant::now();
        fetch(&cache, 1, 1, t0, 10);
        assert_eq!(cache.len(), 1);
        let (bytes, _) = fetch(&cache, 1, 2, t0, 100);
        assert_eq!(bytes.len(), 100);
        assert!(cache.is_empty());
        assert_eq!(cache.bytes(), 0);
    }
}

struct SharedFrame {
    frame: Bytes,
    keep: Arc<ClientEvent>,
    last_used: Instant,
}

struct SharedState {
    entries: HashMap<usize, SharedFrame>,
    order: VecDeque<usize>,
    bytes: u64,
    last_sweep: Option<Instant>,
}

pub struct SharedFrameCache {
    state: Mutex<SharedState>,
    max_bytes: u64,
    idle: Duration,
}

impl SharedFrameCache {
    pub fn new(max_bytes: u64, idle: Duration) -> Self {
        Self {
            state: Mutex::new(SharedState {
                entries: HashMap::new(),
                order: VecDeque::new(),
                bytes: 0,
                last_sweep: None,
            }),
            max_bytes,
            idle,
        }
    }

    fn lock(&self) -> MutexGuard<'_, SharedState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn get_or_build<E>(
        &self,
        event: &Arc<ClientEvent>,
        now: Instant,
        build: impl FnOnce() -> Result<Bytes, E>,
    ) -> Result<Bytes, E> {
        let key = Arc::as_ptr(event) as usize;
        let hit = {
            let mut state = self.lock();
            let hit = state.entries.get_mut(&key).map(|entry| {
                entry.last_used = now;
                entry.frame.clone()
            });
            state.sweep(now, self.idle);
            hit
        };
        if let Some(frame) = hit {
            return Ok(frame);
        }
        let frame = build()?;
        self.lock().insert(
            key,
            SharedFrame {
                frame: frame.clone(),
                keep: Arc::clone(event),
                last_used: now,
            },
            self.max_bytes,
        );
        Ok(frame)
    }

    pub fn len(&self) -> usize {
        self.lock().entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn bytes(&self) -> u64 {
        self.lock().bytes
    }
}

impl SharedState {
    fn insert(&mut self, key: usize, frame: SharedFrame, max_bytes: u64) {
        let weight = frame.frame.len() as u64 + FRAME_OVERHEAD;
        if let Some(previous) = self.entries.insert(key, frame) {
            self.bytes -= previous.frame.len() as u64 + FRAME_OVERHEAD;
        } else {
            self.order.push_back(key);
        }
        self.bytes += weight;
        while self.bytes > max_bytes {
            let Some(oldest) = self.order.pop_front() else {
                break;
            };
            if let Some(removed) = self.entries.remove(&oldest) {
                self.bytes -= removed.frame.len() as u64 + FRAME_OVERHEAD;
                drop(removed.keep);
            }
        }
    }

    fn sweep(&mut self, now: Instant, idle: Duration) {
        if self
            .last_sweep
            .is_some_and(|last| now.duration_since(last) < SWEEP_INTERVAL)
        {
            return;
        }
        self.last_sweep = Some(now);
        let mut kept = VecDeque::with_capacity(self.order.len());
        for key in self.order.drain(..) {
            let expired = self
                .entries
                .get(&key)
                .is_some_and(|entry| now.duration_since(entry.last_used) > idle);
            if expired {
                if let Some(removed) = self.entries.remove(&key) {
                    self.bytes -= removed.frame.len() as u64 + FRAME_OVERHEAD;
                }
            } else if self.entries.contains_key(&key) {
                kept.push_back(key);
            }
        }
        self.order = kept;
    }
}

#[cfg(test)]
mod shared_tests {
    use super::*;

    fn frame(size: usize) -> Bytes {
        Bytes::from(vec![1u8; size])
    }

    #[test]
    fn the_same_event_is_built_once_for_every_session() {
        let cache = SharedFrameCache::new(1024, Duration::from_secs(1));
        let event = Arc::new(ClientEvent::TickingStep(1));
        let now = Instant::now();
        let mut builds = 0;
        for _ in 0..3 {
            let got: Result<Bytes, ()> = cache.get_or_build(&event, now, || {
                builds += 1;
                Ok(frame(10))
            });
            assert_eq!(got.expect("built").len(), 10);
        }
        assert_eq!(builds, 1);
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn the_oldest_frames_leave_when_the_cache_is_full() {
        let cache = SharedFrameCache::new(300, Duration::from_secs(60));
        let now = Instant::now();
        let events: Vec<Arc<ClientEvent>> = (0..4)
            .map(|step| Arc::new(ClientEvent::TickingStep(step)))
            .collect();
        for event in &events {
            let _: Result<Bytes, ()> = cache.get_or_build(event, now, || Ok(frame(50)));
        }
        assert!(cache.bytes() <= 300);
        assert_eq!(cache.len(), 2);
        let mut rebuilt = false;
        let _: Result<Bytes, ()> = cache.get_or_build(&events[0], now, || {
            rebuilt = true;
            Ok(frame(50))
        });
        assert!(rebuilt);
    }

    #[test]
    fn idle_frames_are_swept() {
        let cache = SharedFrameCache::new(1 << 20, Duration::from_millis(10));
        let start = Instant::now();
        let event = Arc::new(ClientEvent::TickingStep(1));
        let _: Result<Bytes, ()> = cache.get_or_build(&event, start, || Ok(frame(8)));
        let later = start + SWEEP_INTERVAL + Duration::from_millis(50);
        let other = Arc::new(ClientEvent::TickingStep(2));
        let _: Result<Bytes, ()> = cache.get_or_build(&other, later, || Ok(frame(8)));
        assert_eq!(cache.len(), 1);
    }
}
