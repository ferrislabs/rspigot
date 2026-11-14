use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender, unbounded};
use rspigot_domain::ChunkPos;
use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::world::Dimension;

use super::queue::{JobQueue, Priority};
use crate::lighting;
use rspigot_nbt::Compound;

use crate::ports::{ChunkGenerator, ChunkStore, EntityStore};

pub enum ChunkJob {
    Load {
        dimension: Arc<Dimension>,
        pos: ChunkPos,
        mobs: Option<super::MobGeneration>,
    },
    Save {
        dimension: Arc<Dimension>,
        chunk: Arc<ChunkColumn>,
    },
    SaveEntities {
        dimension: Arc<Dimension>,
        pos: ChunkPos,
        entities: Arc<Vec<Compound>>,
        merge: bool,
        generation: u64,
    },
    SaveData {
        dimension: Arc<Dimension>,
        name: &'static str,
        data: Arc<Compound>,
    },
    Flush,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChunkSource {
    Disk,
    Generated,
}

pub enum ChunkResult {
    Loaded {
        dimension: Arc<Dimension>,
        pos: ChunkPos,
        chunk: Box<ChunkColumn>,
        source: ChunkSource,
        elapsed: Duration,
        entities: Option<Vec<Compound>>,
        relit: bool,
    },
    Saved {
        pos: ChunkPos,
    },
    EntitiesSaved {
        pos: ChunkPos,
        generation: u64,
    },
    Failed {
        pos: ChunkPos,
        error: String,
    },
    DataSaved,
    Flushed,
}

pub struct ChunkWorkers {
    queue: Arc<JobQueue<ChunkJob>>,
    results: Receiver<ChunkResult>,
    handles: Vec<JoinHandle<()>>,
}

impl ChunkWorkers {
    pub fn spawn<S: ChunkStore + EntityStore, G: ChunkGenerator>(
        threads: usize,
        store: Arc<S>,
        generator: Arc<G>,
    ) -> Self {
        let queue = Arc::new(JobQueue::default());
        let (sender, results) = unbounded();
        let handles = (0..threads.max(1))
            .map(|index| {
                let queue = Arc::clone(&queue);
                let store = Arc::clone(&store);
                let generator = Arc::clone(&generator);
                let sender = sender.clone();
                std::thread::Builder::new()
                    .name(format!("chunk-worker-{index}"))
                    .spawn(move || run_worker(&queue, &*store, &*generator, &sender))
                    .ok()
            })
            .collect::<Option<Vec<_>>>()
            .unwrap_or_default();
        Self {
            queue,
            results,
            handles,
        }
    }

    pub fn submit(&self, priority: Priority, job: ChunkJob) -> Arc<AtomicBool> {
        self.queue.push(priority, job)
    }

    pub fn results(&self) -> &Receiver<ChunkResult> {
        &self.results
    }

    pub fn backlog(&self) -> usize {
        self.queue.len()
    }

    pub fn thread_count(&self) -> usize {
        self.handles.len()
    }

    pub fn shutdown(self) {
        self.queue.close();
        for handle in self.handles {
            let _ = handle.join();
        }
    }
}

fn run_worker<S: ChunkStore + EntityStore, G: ChunkGenerator>(
    queue: &JobQueue<ChunkJob>,
    store: &S,
    generator: &G,
    results: &Sender<ChunkResult>,
) {
    while let Some(job) = queue.pop_blocking() {
        let result = execute(job, store, generator);
        if results.send(result).is_err() {
            return;
        }
    }
}

fn execute<S: ChunkStore + EntityStore, G: ChunkGenerator>(
    job: ChunkJob,
    store: &S,
    generator: &G,
) -> ChunkResult {
    match job {
        ChunkJob::Load {
            dimension,
            pos,
            mobs,
        } => load(dimension, pos, mobs, store, generator),
        ChunkJob::Save { dimension, chunk } => match store.save(&dimension.key, &chunk) {
            Ok(()) => ChunkResult::Saved { pos: chunk.pos() },
            Err(error) => ChunkResult::Failed {
                pos: chunk.pos(),
                error: error.to_string(),
            },
        },
        ChunkJob::SaveEntities {
            dimension,
            pos,
            entities,
            merge,
            generation,
        } => match save_entities(&dimension, pos, &entities, merge, store) {
            Ok(()) => ChunkResult::EntitiesSaved { pos, generation },
            Err(error) => ChunkResult::Failed {
                pos,
                error: error.to_string(),
            },
        },
        ChunkJob::SaveData {
            dimension,
            name,
            data,
        } => {
            if let Err(error) = store.save_world_data(&dimension.key, name, &data) {
                tracing::error!(%error, name, "world data save failed");
            }
            ChunkResult::DataSaved
        }
        ChunkJob::Flush => {
            if let Err(error) = store.flush() {
                tracing::error!(%error, "chunk store flush failed");
            }
            ChunkResult::Flushed
        }
    }
}

fn load<S: ChunkStore + EntityStore, G: ChunkGenerator>(
    dimension: Arc<Dimension>,
    pos: ChunkPos,
    mobs: Option<super::MobGeneration>,
    store: &S,
    generator: &G,
) -> ChunkResult {
    let started = Instant::now();
    let (mut chunk, source) = match store.load(&dimension.key, pos) {
        Ok(Some(chunk)) => (chunk, ChunkSource::Disk),
        Ok(None) => (generator.generate(&dimension, pos), ChunkSource::Generated),
        Err(error) => {
            tracing::warn!(?pos, %error, "chunk load failed, regenerating");
            (generator.generate(&dimension, pos), ChunkSource::Generated)
        }
    };
    let mut entities = match store.load_entities(&dimension.key, pos) {
        Ok(entities) => entities,
        Err(error) => {
            tracing::error!(?pos, %error, "Failed to load entities");
            None
        }
    };
    let stale = source == ChunkSource::Disk
        && dimension.has_skylight
        && lighting::has_light(&chunk)
        && !lighting::sky_is_consistent(&chunk);
    if stale || !lighting::has_light(&chunk) {
        lighting::compute(&mut chunk, dimension.has_skylight);
    }
    if source == ChunkSource::Generated
        && entities.is_none()
        && generator.spawns_original_mobs(&dimension)
        && let Some(mobs) = mobs
    {
        entities = Some(crate::spawning::original_mobs(&chunk, &dimension, mobs));
    }
    ChunkResult::Loaded {
        dimension,
        pos,
        chunk: Box::new(chunk),
        source,
        elapsed: started.elapsed(),
        entities,
        relit: stale,
    }
}

fn entity_uuid(entity: &Compound) -> Option<&[i32]> {
    entity.get_int_array("UUID")
}

fn save_entities<S: EntityStore>(
    dimension: &Dimension,
    pos: ChunkPos,
    entities: &[Compound],
    merge: bool,
    store: &S,
) -> Result<(), crate::ports::StoreError> {
    if !merge {
        return store.save_entities(&dimension.key, pos, entities);
    }
    let mut merged: Vec<Compound> = store
        .load_entities(&dimension.key, pos)?
        .unwrap_or_default()
        .into_iter()
        .filter(|existing| {
            entity_uuid(existing).is_none_or(|uuid| {
                !entities
                    .iter()
                    .any(|entity| entity_uuid(entity) == Some(uuid))
            })
        })
        .collect();
    merged.extend(entities.iter().cloned());
    store.save_entities(&dimension.key, pos, &merged)
}
