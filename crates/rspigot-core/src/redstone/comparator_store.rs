use rspigot_domain::chunk::BlockEntity;
use rspigot_domain::{BlockPos, Identifier};
use rspigot_nbt::Compound;

use crate::ports::ClientLink;
use crate::server::Server;

const ENTITY: &str = "comparator";
const KEY: &str = "OutputSignal";

impl<L: ClientLink> Server<L> {
    pub(crate) fn comparator_stored(&self, pos: BlockPos) -> i32 {
        self.world
            .chunks
            .get(pos.chunk())
            .and_then(|chunk| chunk.block_entity(pos))
            .and_then(|entity| entity.data.get_i32(KEY))
            .unwrap_or(0)
    }

    pub(crate) fn store_comparator(&mut self, pos: BlockPos, output: i32) -> i32 {
        let known = self.comparator_stored(pos);
        let present = self
            .world
            .chunks
            .get(pos.chunk())
            .is_some_and(|chunk| chunk.block_entity(pos).is_some());
        if known == output && (present || output == 0) {
            return known;
        }
        if let Some(chunk) = self.world.chunks.chunk_mut(pos.chunk()) {
            chunk.set_block_entity(BlockEntity {
                pos,
                kind: Identifier::minecraft(ENTITY),
                data: Compound::new().with(KEY, output),
            });
        }
        known
    }

    pub(crate) fn forget_comparator(&mut self, pos: BlockPos) {
        if let Some(chunk) = self.world.chunks.chunk_mut(pos.chunk())
            && chunk
                .block_entity(pos)
                .is_some_and(|entity| entity.kind == Identifier::minecraft(ENTITY))
        {
            chunk.remove_block_entity(pos);
        }
    }
}
