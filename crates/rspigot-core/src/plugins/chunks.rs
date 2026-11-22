use rspigot_domain::{ChunkPos, EntityId};

use crate::plugins::decisions::{Continuation, Lane};
use crate::plugins::events::{
    AsyncStructureGenerate, AsyncStructureGenerateCause, AsyncStructureSpawn, ChunkLoad,
    ChunkPopulate, ChunkUnload, ChunkUnloadPatch, EntitiesLoad, EntitiesUnload,
};
use crate::plugins::{BoundingBox, ChunkRef, EventKind, EventPayload};
use crate::ports::ClientLink;
use crate::server::Server;
use rspigot_domain::Vec3;
use rspigot_nbt::{Compound, Tag};

fn piece_bounds(piece: &Tag) -> Option<[i32; 6]> {
    match piece.as_compound()?.get("BB")? {
        Tag::IntArray(values) => values.as_slice().try_into().ok(),
        _ => None,
    }
}

fn structure_starts(extra: &Compound, pos: ChunkPos) -> Vec<(String, BoundingBox)> {
    let Some(starts) = extra
        .get_compound("structures")
        .and_then(|structures| structures.get_compound("starts"))
    else {
        return Vec::new();
    };
    starts
        .iter()
        .filter_map(|(name, tag)| {
            let start = tag.as_compound()?;
            if start.get_i32("ChunkX") != Some(pos.x) || start.get_i32("ChunkZ") != Some(pos.z) {
                return None;
            }
            let boxes: Vec<[i32; 6]> = start
                .get_list("Children")?
                .iter()
                .filter_map(piece_bounds)
                .collect();
            let (first, rest) = boxes.split_first()?;
            let all = rest.iter().fold(*first, |acc, next| {
                [
                    acc[0].min(next[0]),
                    acc[1].min(next[1]),
                    acc[2].min(next[2]),
                    acc[3].max(next[3]),
                    acc[4].max(next[4]),
                    acc[5].max(next[5]),
                ]
            });
            let id = if name.contains(':') {
                name.to_owned()
            } else {
                format!("minecraft:{name}")
            };
            Some((
                id,
                BoundingBox {
                    min: Vec3::new(f64::from(all[0]), f64::from(all[1]), f64::from(all[2])),
                    max: Vec3::new(
                        f64::from(all[3] + 1),
                        f64::from(all[4] + 1),
                        f64::from(all[5] + 1),
                    ),
                },
            ))
        })
        .collect()
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn world_name(&self) -> String {
        self.world.dimension.key.to_string()
    }

    fn chunk_ref(&self, pos: ChunkPos) -> ChunkRef {
        ChunkRef {
            world: self.world_name(),
            x: pos.x,
            z: pos.z,
        }
    }

    pub(crate) fn chunk_loaded(&mut self, pos: ChunkPos, new_chunk: bool) {
        self.fire(
            Lane::Chunk(pos),
            Continuation::Notice(EventKind::ChunkLoad),
            move |server| {
                Some(EventPayload::ChunkLoad(ChunkLoad {
                    chunk: server.chunk_ref(pos),
                    new_chunk,
                    world: server.world_name(),
                }))
            },
        );
        if new_chunk {
            self.announce_structures(pos);
            self.fire(
                Lane::Chunk(pos),
                Continuation::Notice(EventKind::ChunkPopulate),
                move |server| {
                    Some(EventPayload::ChunkPopulate(ChunkPopulate {
                        chunk: server.chunk_ref(pos),
                        world: server.world_name(),
                    }))
                },
            );
        }
    }

    fn announce_structures(&mut self, pos: ChunkPos) {
        let starts = self
            .world
            .chunks
            .get(pos)
            .map(|chunk| structure_starts(&chunk.extra, pos))
            .unwrap_or_default();
        for (structure, bounding_box) in starts {
            let spawned = AsyncStructureSpawn {
                bounding_box,
                chunk_x: pos.x,
                chunk_z: pos.z,
                structure: structure.clone(),
                world: self.world_name(),
            };
            self.fire(
                Lane::Chunk(pos),
                Continuation::Notice(EventKind::AsyncStructureSpawn),
                move |_| Some(EventPayload::AsyncStructureSpawn(spawned)),
            );
            let generated = AsyncStructureGenerate {
                bounding_box,
                cause: AsyncStructureGenerateCause::WorldGeneration,
                chunk_x: pos.x,
                chunk_z: pos.z,
                structure,
                world: self.world_name(),
            };
            self.fire(
                Lane::Chunk(pos),
                Continuation::Notice(EventKind::AsyncStructureGenerate),
                move |_| Some(EventPayload::AsyncStructureGenerate(generated)),
            );
        }
    }

    pub(crate) fn entities_loaded(&mut self, pos: ChunkPos) {
        self.fire(
            Lane::Chunk(pos),
            Continuation::Notice(EventKind::EntitiesLoad),
            move |server| {
                Some(EventPayload::EntitiesLoad(EntitiesLoad {
                    chunk: server.chunk_ref(pos),
                    entities: server
                        .entities
                        .store
                        .in_chunk(pos)
                        .iter()
                        .filter_map(|id| server.entity_view(*id))
                        .collect(),
                    world: server.world_name(),
                }))
            },
        );
    }

    pub(crate) fn entities_unloading(&mut self, pos: ChunkPos, saved: &[EntityId]) {
        self.fire(
            Lane::Chunk(pos),
            Continuation::Notice(EventKind::EntitiesUnload),
            move |server| {
                Some(EventPayload::EntitiesUnload(EntitiesUnload {
                    chunk: server.chunk_ref(pos),
                    entities: saved
                        .iter()
                        .filter_map(|id| server.entity_view(*id))
                        .collect(),
                    world: server.world_name(),
                }))
            },
        );
    }

    pub(crate) fn ask_chunk_unload(&mut self, pos: ChunkPos) {
        self.fire(
            Lane::Chunk(pos),
            Continuation::ChunkUnload(pos),
            move |server| {
                Some(EventPayload::ChunkUnload(ChunkUnload {
                    chunk: server.chunk_ref(pos),
                    save_chunk: true,
                    world: server.world_name(),
                }))
            },
        );
    }

    pub(crate) fn finish_chunk_unload(&mut self, pos: ChunkPos, patch: ChunkUnloadPatch) {
        let save = patch.save_chunk.unwrap_or(true);
        if self.world.chunks.unload_due(pos) {
            self.unload_scheduled_ticks(pos);
            let leaving = self.entities.store.in_chunk(pos).to_vec();
            self.settle_acting_on(&leaving);
        }
        self.world.chunks.unload(pos, save, &self.workers);
    }
}
