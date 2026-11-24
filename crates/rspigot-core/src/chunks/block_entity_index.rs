use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::{ChunkPos, Identifier};
use rustc_hash::{FxHashMap, FxHashSet};

#[derive(Default)]
pub struct BlockEntityIndex {
    by_kind: FxHashMap<Identifier, FxHashSet<ChunkPos>>,
    kinds_of: FxHashMap<ChunkPos, Vec<Identifier>>,
    dirty: FxHashSet<ChunkPos>,
}

impl BlockEntityIndex {
    pub fn mark(&mut self, pos: ChunkPos) {
        self.dirty.insert(pos);
    }

    pub fn forget(&mut self, pos: ChunkPos) {
        self.dirty.remove(&pos);
        if let Some(kinds) = self.kinds_of.remove(&pos) {
            self.unindex(pos, &kinds);
        }
    }

    pub fn refresh(&mut self, pos: ChunkPos, chunk: &ChunkColumn) {
        self.dirty.remove(&pos);
        let mut kinds: Vec<Identifier> = Vec::new();
        for entity in &chunk.block_entities {
            if !kinds.contains(&entity.kind) {
                kinds.push(entity.kind.clone());
            }
        }
        let previous = self.kinds_of.remove(&pos).unwrap_or_default();
        let stale: Vec<Identifier> = previous
            .into_iter()
            .filter(|kind| !kinds.contains(kind))
            .collect();
        self.unindex(pos, &stale);
        for kind in &kinds {
            self.by_kind.entry(kind.clone()).or_default().insert(pos);
        }
        if !kinds.is_empty() {
            self.kinds_of.insert(pos, kinds);
        }
    }

    pub fn take_dirty(&mut self) -> Vec<ChunkPos> {
        self.dirty.drain().collect()
    }

    pub fn chunks_matching(&self, matches: impl Fn(&Identifier) -> bool) -> Vec<ChunkPos> {
        let mut found: Vec<ChunkPos> = self
            .by_kind
            .iter()
            .filter(|(kind, _)| matches(kind))
            .flat_map(|(_, chunks)| chunks.iter().copied())
            .collect();
        found.sort_unstable();
        found.dedup();
        found
    }

    fn unindex(&mut self, pos: ChunkPos, kinds: &[Identifier]) {
        for kind in kinds {
            if let Some(chunks) = self.by_kind.get_mut(kind) {
                chunks.remove(&pos);
                if chunks.is_empty() {
                    self.by_kind.remove(kind);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use rspigot_domain::chunk::BlockEntity;
    use rspigot_domain::{BiomeId, BlockPos, world::Dimension};
    use rspigot_nbt::Compound;

    use super::*;

    fn chunk_with(pos: ChunkPos, kinds: &[&str]) -> ChunkColumn {
        let mut chunk = ChunkColumn::empty(pos, Dimension::overworld().height, BiomeId(0));
        for (i, kind) in kinds.iter().enumerate() {
            chunk.set_block_entity(BlockEntity {
                pos: BlockPos::new(pos.min_block_x() + i as i32, 64, pos.min_block_z()),
                kind: Identifier::minecraft(kind),
                data: Compound::default(),
            });
        }
        chunk
    }

    fn furnaces(index: &BlockEntityIndex) -> Vec<ChunkPos> {
        index.chunks_matching(|kind| kind.as_str() == "minecraft:furnace")
    }

    #[test]
    fn refresh_indexes_the_distinct_kinds_of_a_chunk() {
        let mut index = BlockEntityIndex::default();
        let pos = ChunkPos::new(1, 2);
        index.refresh(pos, &chunk_with(pos, &["furnace", "furnace", "hopper"]));
        assert_eq!(furnaces(&index), vec![pos]);
        assert_eq!(index.kinds_of[&pos].len(), 2);
    }

    #[test]
    fn chunks_matching_is_sorted_and_deduplicated_across_kinds() {
        let mut index = BlockEntityIndex::default();
        for (x, kinds) in [
            (3, vec!["furnace", "smoker"]),
            (-1, vec!["smoker"]),
            (2, vec!["furnace"]),
        ] {
            let pos = ChunkPos::new(x, 0);
            index.refresh(pos, &chunk_with(pos, &kinds));
        }
        let found = index.chunks_matching(|kind| {
            matches!(kind.as_str(), "minecraft:furnace" | "minecraft:smoker")
        });
        assert_eq!(
            found,
            vec![
                ChunkPos::new(-1, 0),
                ChunkPos::new(2, 0),
                ChunkPos::new(3, 0)
            ]
        );
    }

    #[test]
    fn refresh_drops_stale_kinds_and_empty_sets() {
        let mut index = BlockEntityIndex::default();
        let pos = ChunkPos::new(0, 0);
        index.refresh(pos, &chunk_with(pos, &["furnace"]));
        index.refresh(pos, &chunk_with(pos, &["hopper"]));
        assert!(furnaces(&index).is_empty());
        assert_eq!(index.by_kind.len(), 1);
        index.refresh(pos, &chunk_with(pos, &[]));
        assert!(index.by_kind.is_empty());
    }

    #[test]
    fn forget_removes_the_chunk_everywhere() {
        let mut index = BlockEntityIndex::default();
        let pos = ChunkPos::new(0, 0);
        index.refresh(pos, &chunk_with(pos, &["furnace"]));
        index.mark(pos);
        index.forget(pos);
        assert!(furnaces(&index).is_empty());
        assert!(index.by_kind.is_empty());
        assert!(index.kinds_of.is_empty());
        assert!(index.take_dirty().is_empty());
    }

    #[test]
    fn mark_then_take_dirty_drains_and_refresh_clears_the_flag() {
        let mut index = BlockEntityIndex::default();
        let a = ChunkPos::new(0, 0);
        let b = ChunkPos::new(1, 0);
        index.mark(a);
        index.mark(b);
        index.refresh(b, &chunk_with(b, &[]));
        assert_eq!(index.take_dirty(), vec![a]);
        assert!(index.take_dirty().is_empty());
    }
}
