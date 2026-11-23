use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender, unbounded};
use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::poi::PoiChunkData;
use rspigot_domain::world::Dimension;
use rspigot_domain::{ChunkPos, Vec3};

use super::queue::{JobQueue, Priority};
use crate::lighting;
use rspigot_nbt::Compound;

use crate::ports::{
    ChunkGenerator, ChunkStore, EntityStore, PlaceFailure, PlaceRequest, PlacedBlock,
};

#[derive(Debug, Clone, PartialEq)]
pub enum WorldQuery {
    Structures(Vec<String>),
    Biomes(Vec<String>),
    Place(PlaceRequest),
    Templates,
    TemplateInfo(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum QueryAnswer {
    Located(Option<(String, Vec3)>),
    Placed(Result<Vec<PlacedBlock>, PlaceFailure>),
    Templates(Vec<String>),
    TemplateInfo(Option<crate::ports::TemplateInfo>),
}

pub enum ChunkJob {
    Load {
        dimension: Arc<Dimension>,
        pos: ChunkPos,
        mobs: Option<super::MobGeneration>,
    },
    Generate {
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
    SavePoi {
        dimension: Arc<Dimension>,
        pos: ChunkPos,
        data: Arc<PoiChunkData>,
    },
    SaveData {
        dimension: Arc<Dimension>,
        name: &'static str,
        data: Arc<Compound>,
    },
    Flush,
    Sync(Sender<()>),
    NearestStronghold {
        dimension: Arc<Dimension>,
        from: Vec3,
        reply: Sender<Option<Vec3>>,
    },
    Query {
        dimension: Arc<Dimension>,
        from: Vec3,
        query: WorldQuery,
        reply: Sender<QueryAnswer>,
    },
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
        poi: Option<PoiChunkData>,
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
    PoiSaved {
        pos: ChunkPos,
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

    pub fn flush_blocking(&self, timeout: Duration) -> bool {
        let (done, wait) = unbounded();
        self.submit(Priority::BACKGROUND, ChunkJob::Sync(done));
        wait.recv_timeout(timeout).is_ok()
    }

    pub fn request_stronghold(
        &self,
        dimension: &Arc<Dimension>,
        from: Vec3,
    ) -> Receiver<Option<Vec3>> {
        let (reply, answer) = unbounded();
        self.submit(
            Priority::URGENT,
            ChunkJob::NearestStronghold {
                dimension: Arc::clone(dimension),
                from,
                reply,
            },
        );
        answer
    }

    pub fn request_query(
        &self,
        dimension: &Arc<Dimension>,
        from: Vec3,
        query: WorldQuery,
    ) -> Receiver<QueryAnswer> {
        let (reply, answer) = unbounded();
        self.submit(
            Priority::URGENT,
            ChunkJob::Query {
                dimension: Arc::clone(dimension),
                from,
                query,
                reply,
            },
        );
        answer
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
        } => load(dimension, pos, mobs, false, store, generator),
        ChunkJob::Generate {
            dimension,
            pos,
            mobs,
        } => load(dimension, pos, mobs, true, store, generator),
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
        ChunkJob::SavePoi {
            dimension,
            pos,
            data,
        } => match store.save_poi(&dimension.key, pos, &data) {
            Ok(()) => ChunkResult::PoiSaved { pos },
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
        ChunkJob::Sync(done) => {
            if let Err(error) = store.flush() {
                tracing::error!(%error, "chunk store flush failed");
            }
            let _ = done.send(());
            ChunkResult::Flushed
        }
        ChunkJob::NearestStronghold {
            dimension,
            from,
            reply,
        } => {
            let _ = reply.send(generator.nearest_stronghold(&dimension, from));
            ChunkResult::Flushed
        }
        ChunkJob::Query {
            dimension,
            from,
            query,
            reply,
        } => {
            let answer = match query {
                WorldQuery::Structures(ids) => {
                    QueryAnswer::Located(generator.nearest_structures(&dimension, from, &ids))
                }
                WorldQuery::Biomes(ids) => {
                    QueryAnswer::Located(generator.nearest_biome(&dimension, from, &ids))
                }
                WorldQuery::Place(request) => {
                    QueryAnswer::Placed(generator.place(&dimension, &request))
                }
                WorldQuery::Templates => QueryAnswer::Templates(generator.template_names()),
                WorldQuery::TemplateInfo(id) => {
                    QueryAnswer::TemplateInfo(generator.template_info(&id))
                }
            };
            let _ = reply.send(answer);
            ChunkResult::Flushed
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
    fresh: bool,
    store: &S,
    generator: &G,
) -> ChunkResult {
    let started = Instant::now();
    let (mut chunk, source) = if fresh {
        (generator.generate(&dimension, pos), ChunkSource::Generated)
    } else {
        match store.load(&dimension.key, pos) {
            Ok(Some(chunk)) => (chunk, ChunkSource::Disk),
            Ok(None) => (generator.generate(&dimension, pos), ChunkSource::Generated),
            Err(error) => {
                tracing::warn!(?pos, %error, "chunk load failed, regenerating");
                (generator.generate(&dimension, pos), ChunkSource::Generated)
            }
        }
    };
    let mut entities = if fresh {
        None
    } else {
        match store.load_entities(&dimension.key, pos) {
            Ok(entities) => entities,
            Err(error) => {
                tracing::error!(?pos, %error, "Failed to load entities");
                None
            }
        }
    };
    let poi = match source {
        ChunkSource::Disk => store
            .load_poi(&dimension.key, pos)
            .map_err(|error| tracing::error!(?pos, %error, "Failed to load points of interest"))
            .unwrap_or(None),
        ChunkSource::Generated => None,
    };
    let stale = source == ChunkSource::Disk
        && dimension.has_skylight
        && lighting::has_light(&chunk)
        && !lighting::sky_is_consistent(&chunk);
    if stale || !lighting::has_light(&chunk) {
        lighting::compute(&mut chunk, dimension.has_skylight);
    }
    if source == ChunkSource::Generated && entities.is_none() {
        let mut generated = generator.structure_entities(&dimension, pos);
        let mobs_spawn = generator.spawns_original_mobs(&dimension);
        if mobs_spawn && let Some(mobs) = mobs {
            generated.extend(crate::spawning::original_mobs(&chunk, &dimension, mobs));
        }
        if mobs_spawn && mobs.is_some() || !generated.is_empty() {
            entities = Some(generated);
        }
    }
    ChunkResult::Loaded {
        dimension,
        pos,
        chunk: Box::new(chunk),
        source,
        elapsed: started.elapsed(),
        entities,
        poi,
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
