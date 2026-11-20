use std::collections::BTreeMap;

use rspigot_nbt::{Compound, Tag};

use crate::meta::Metadata;
use crate::pos::BlockPos;

pub const BLOCKS_KEY: &str = "rspigot:blocks";

#[derive(Debug, Clone, PartialEq, Default)]
pub struct BlockMeta {
    pub persistent: Metadata,
    pub volatile: Metadata,
}

impl BlockMeta {
    pub fn is_empty(&self) -> bool {
        self.persistent.is_empty() && self.volatile.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ChunkMeta {
    persistent: BTreeMap<BlockPos, Metadata>,
    volatile: BTreeMap<BlockPos, Metadata>,
}

fn position_key(pos: BlockPos) -> String {
    format!("{},{},{}", pos.x, pos.y, pos.z)
}

fn parse_position(key: &str) -> Option<BlockPos> {
    let mut parts = key.split(',');
    let x = parts.next()?.parse().ok()?;
    let y = parts.next()?.parse().ok()?;
    let z = parts.next()?.parse().ok()?;
    parts.next().is_none().then(|| BlockPos::new(x, y, z))
}

impl ChunkMeta {
    pub fn is_empty(&self) -> bool {
        self.persistent.is_empty() && self.volatile.is_empty()
    }

    pub fn persistent(&self, pos: BlockPos) -> Option<&Metadata> {
        self.persistent.get(&pos)
    }

    pub fn volatile(&self, pos: BlockPos) -> Option<&Metadata> {
        self.volatile.get(&pos)
    }

    pub fn holder(&self, pos: BlockPos, volatile: bool) -> Option<&Metadata> {
        if volatile {
            self.volatile(pos)
        } else {
            self.persistent(pos)
        }
    }

    pub fn holder_mut(&mut self, pos: BlockPos, volatile: bool) -> &mut Metadata {
        let map = if volatile {
            &mut self.volatile
        } else {
            &mut self.persistent
        };
        map.entry(pos).or_default()
    }

    pub fn prune(&mut self, pos: BlockPos) {
        if self.persistent.get(&pos).is_some_and(Metadata::is_empty) {
            self.persistent.remove(&pos);
        }
        if self.volatile.get(&pos).is_some_and(Metadata::is_empty) {
            self.volatile.remove(&pos);
        }
    }

    pub fn drop_volatile(&mut self) {
        self.volatile.clear();
    }

    pub fn has_any(&self) -> bool {
        !self.persistent.is_empty() || !self.volatile.is_empty()
    }

    pub fn positions(&self) -> impl Iterator<Item = BlockPos> + '_ {
        self.persistent.keys().chain(self.volatile.keys()).copied()
    }

    pub fn take(&mut self, pos: BlockPos) -> BlockMeta {
        BlockMeta {
            persistent: self.persistent.remove(&pos).unwrap_or_default(),
            volatile: self.volatile.remove(&pos).unwrap_or_default(),
        }
    }

    pub fn put(&mut self, pos: BlockPos, meta: BlockMeta) {
        if !meta.persistent.is_empty() {
            self.persistent.insert(pos, meta.persistent);
        }
        if !meta.volatile.is_empty() {
            self.volatile.insert(pos, meta.volatile);
        }
    }

    pub fn from_blocks(blocks: &Compound) -> Self {
        let mut meta = Self::default();
        for (position, holder) in blocks.iter() {
            if let (Some(pos), Some(holder)) = (parse_position(position), holder.as_compound()) {
                meta.persistent.insert(pos, Metadata::from_nbt(holder));
            }
        }
        meta
    }

    pub fn to_blocks(&self) -> Option<Compound> {
        let blocks: Compound = self
            .persistent
            .iter()
            .filter(|(_, holder)| !holder.is_empty())
            .map(|(pos, holder)| (position_key(*pos), Tag::Compound(holder.to_nbt())))
            .collect();
        (!blocks.is_empty()).then_some(blocks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meta::MetaValue;

    #[test]
    fn block_values_round_trip_with_absolute_coordinates_and_foreign_holder_keys() {
        let mut meta = ChunkMeta::default();
        let pos = BlockPos::new(-3, 64, 17);
        meta.holder_mut(pos, false)
            .set("marker:kind", MetaValue::Int(4))
            .unwrap();
        meta.holder_mut(pos, true)
            .set("marker:tmp", MetaValue::Int(4))
            .unwrap();
        let blocks = meta.to_blocks().unwrap();
        assert!(blocks.get_compound("-3,64,17").is_some());
        assert_eq!(blocks.len(), 1);

        let mut spigot = blocks.clone();
        spigot
            .get_mut("-3,64,17")
            .and_then(|tag| match tag {
                Tag::Compound(holder) => Some(holder),
                _ => None,
            })
            .unwrap()
            .insert("plugin:flag", Tag::Byte(1));
        let loaded = ChunkMeta::from_blocks(&spigot);
        assert_eq!(
            loaded.persistent(pos).unwrap().get("marker:kind"),
            Some(MetaValue::Int(4))
        );
        assert!(loaded.volatile(pos).is_none());
        assert_eq!(loaded.to_blocks().unwrap(), spigot);
    }

    #[test]
    fn an_empty_chunk_meta_writes_nothing() {
        assert_eq!(ChunkMeta::default().to_blocks(), None);
    }
}
