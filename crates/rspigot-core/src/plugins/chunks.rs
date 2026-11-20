use rspigot_domain::{ChunkPos, EntityId};

use crate::plugins::decisions::{Continuation, Lane};
use crate::plugins::events::{
    ChunkLoad, ChunkPopulate, ChunkUnload, ChunkUnloadPatch, EntitiesLoad, EntitiesUnload,
};
use crate::plugins::{ChunkRef, EventKind, EventPayload};
use crate::ports::ClientLink;
use crate::server::Server;

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
