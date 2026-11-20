use rspigot_nbt::Compound;

use super::block_meta::{BlockMeta, ChunkMeta};
use super::heightmap::Heightmaps;
use super::light::ChunkLight;
use super::section::ChunkSection;
use crate::ident::Identifier;
use crate::ids::{BiomeId, BlockStateId};
use crate::meta::{MetaError, MetaValue};
use crate::pos::{BlockPos, ChunkPos};
use crate::world::WorldHeight;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ChunkStatus {
    Empty,
    StructureStarts,
    StructureReferences,
    Biomes,
    Noise,
    Surface,
    Carvers,
    Features,
    InitializeLight,
    Light,
    Spawn,
    Full,
}

impl ChunkStatus {
    pub const fn serialized_name(self) -> &'static str {
        match self {
            Self::Empty => "minecraft:empty",
            Self::StructureStarts => "minecraft:structure_starts",
            Self::StructureReferences => "minecraft:structure_references",
            Self::Biomes => "minecraft:biomes",
            Self::Noise => "minecraft:noise",
            Self::Surface => "minecraft:surface",
            Self::Carvers => "minecraft:carvers",
            Self::Features => "minecraft:features",
            Self::InitializeLight => "minecraft:initialize_light",
            Self::Light => "minecraft:light",
            Self::Spawn => "minecraft:spawn",
            Self::Full => "minecraft:full",
        }
    }

    pub fn from_serialized_name(name: &str) -> Option<Self> {
        let path = name.strip_prefix("minecraft:").unwrap_or(name);
        Some(match path {
            "empty" => Self::Empty,
            "structure_starts" => Self::StructureStarts,
            "structure_references" => Self::StructureReferences,
            "biomes" => Self::Biomes,
            "noise" => Self::Noise,
            "surface" => Self::Surface,
            "carvers" => Self::Carvers,
            "features" => Self::Features,
            "initialize_light" => Self::InitializeLight,
            "light" => Self::Light,
            "spawn" => Self::Spawn,
            "full" => Self::Full,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct BlockEntity {
    pub pos: BlockPos,
    pub kind: Identifier,
    pub data: Compound,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedTick {
    pub id: Identifier,
    pub pos: BlockPos,
    pub priority: i32,
    pub delay: i32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChunkColumn {
    pos: ChunkPos,
    height: WorldHeight,
    sections: Box<[ChunkSection]>,
    pub heightmaps: Heightmaps,
    pub light: ChunkLight,
    pub block_entities: Vec<BlockEntity>,
    pub block_ticks: Vec<SavedTick>,
    pub fluid_ticks: Vec<SavedTick>,
    pub status: ChunkStatus,
    pub inhabited_time: i64,
    pub extra: Compound,
    pub block_meta: ChunkMeta,
    revision: u64,
    dirty: bool,
}

impl ChunkColumn {
    pub fn empty(pos: ChunkPos, height: WorldHeight, biome: BiomeId) -> Self {
        let count = height.section_count();
        Self {
            pos,
            height,
            sections: (0..count).map(|_| ChunkSection::empty(biome)).collect(),
            heightmaps: Heightmaps::default(),
            light: ChunkLight::new(count),
            block_entities: Vec::new(),
            block_ticks: Vec::new(),
            fluid_ticks: Vec::new(),
            status: ChunkStatus::Full,
            inhabited_time: 0,
            extra: Compound::new(),
            block_meta: ChunkMeta::default(),
            revision: 0,
            dirty: false,
        }
    }

    pub fn from_sections(
        pos: ChunkPos,
        height: WorldHeight,
        sections: Vec<ChunkSection>,
    ) -> Option<Self> {
        if sections.len() != height.section_count() {
            return None;
        }
        let count = sections.len();
        Some(Self {
            pos,
            height,
            sections: sections.into_boxed_slice(),
            heightmaps: Heightmaps::default(),
            light: ChunkLight::new(count),
            block_entities: Vec::new(),
            block_ticks: Vec::new(),
            fluid_ticks: Vec::new(),
            status: ChunkStatus::Full,
            inhabited_time: 0,
            extra: Compound::new(),
            block_meta: ChunkMeta::default(),
            revision: 0,
            dirty: false,
        })
    }

    pub fn pos(&self) -> ChunkPos {
        self.pos
    }

    pub fn height(&self) -> WorldHeight {
        self.height
    }

    pub fn sections(&self) -> &[ChunkSection] {
        &self.sections
    }

    pub fn sections_mut(&mut self) -> &mut [ChunkSection] {
        self.touch();
        &mut self.sections
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn mark_saved(&mut self) {
        self.dirty = false;
    }

    pub fn touch(&mut self) {
        self.revision += 1;
        self.dirty = true;
    }

    fn locate(&self, pos: BlockPos) -> Option<(usize, usize, usize, usize)> {
        if pos.chunk() != self.pos || !self.height.contains(pos.y) {
            return None;
        }
        let rel = (pos.y - self.height.min_y) as usize;
        Some((
            (pos.x & 15) as usize,
            rel >> 4,
            rel & 15,
            (pos.z & 15) as usize,
        ))
    }

    pub fn block(&self, pos: BlockPos) -> Option<BlockStateId> {
        let (x, section, y, z) = self.locate(pos)?;
        Some(self.sections[section].block(x, y, z))
    }

    pub fn set_block(&mut self, pos: BlockPos, state: BlockStateId) -> Option<BlockStateId> {
        let (x, section, y, z) = self.locate(pos)?;
        let old = self.sections[section].set_block(x, y, z, state);
        if old != state {
            if old.kind() != state.kind() && self.block_meta.has_any() {
                let lost = self.block_meta.take(pos);
                if !lost.persistent.is_empty() {
                    self.touch();
                }
            }
            self.touch();
        }
        Some(old)
    }

    pub fn set_block_meta(
        &mut self,
        pos: BlockPos,
        key: &str,
        value: MetaValue,
        volatile: bool,
    ) -> Result<(), MetaError> {
        self.block_meta.holder_mut(pos, volatile).set(key, value)?;
        self.block_meta.prune(pos);
        if !volatile {
            self.touch();
        }
        Ok(())
    }

    pub fn remove_block_meta(&mut self, pos: BlockPos, key: &str, volatile: bool) -> bool {
        let removed = self.block_meta.holder_mut(pos, volatile).remove(key);
        self.block_meta.prune(pos);
        if removed && !volatile {
            self.touch();
        }
        removed
    }

    pub fn clear_block_meta(&mut self, pos: BlockPos, volatile: bool) -> bool {
        let cleared = self.block_meta.holder_mut(pos, volatile).clear();
        self.block_meta.prune(pos);
        if cleared && !volatile {
            self.touch();
        }
        cleared
    }

    pub fn take_block_meta(&mut self, pos: BlockPos) -> BlockMeta {
        let taken = self.block_meta.take(pos);
        if !taken.persistent.is_empty() {
            self.touch();
        }
        taken
    }

    pub fn restore_block_meta(&mut self, pos: BlockPos, meta: BlockMeta) {
        if !meta.persistent.is_empty() {
            self.touch();
        }
        self.block_meta.put(pos, meta);
    }

    pub fn block_entity(&self, pos: BlockPos) -> Option<&BlockEntity> {
        self.block_entities.iter().find(|b| b.pos == pos)
    }

    pub fn set_block_entity(&mut self, entity: BlockEntity) {
        self.block_entities.retain(|b| b.pos != entity.pos);
        self.block_entities.push(entity);
        self.touch();
    }

    pub fn remove_block_entity(&mut self, pos: BlockPos) -> Option<BlockEntity> {
        let index = self.block_entities.iter().position(|b| b.pos == pos)?;
        self.touch();
        Some(self.block_entities.swap_remove(index))
    }

    pub fn highest_block(
        &self,
        x: usize,
        z: usize,
        mut include: impl FnMut(BlockStateId) -> bool,
    ) -> Option<i32> {
        for (index, section) in self.sections.iter().enumerate().rev() {
            if section.is_empty() {
                continue;
            }
            for y in (0..16).rev() {
                if include(section.block(x, y, z)) {
                    return Some(self.height.min_y + (index as i32) * 16 + y as i32);
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn overworld() -> WorldHeight {
        WorldHeight::new(-64, 384).unwrap()
    }

    #[test]
    fn block_meta_is_dropped_on_a_kind_change_kept_on_a_state_change_and_marks_dirty() {
        use rspigot_blocks::BlockKind;
        let pos = BlockPos::new(2, 10, 2);
        let mut column = ChunkColumn::empty(ChunkPos::new(0, 0), overworld(), BiomeId(0));
        column.set_block(pos, BlockKind::OakDoor.default_state());
        column.mark_saved();
        column
            .set_block_meta(pos, "marker:kind", MetaValue::Int(1), false)
            .unwrap();
        assert!(column.is_dirty());
        let open = BlockKind::OakDoor
            .default_state()
            .with_property("open", "true")
            .unwrap();
        column.set_block(pos, open);
        assert!(column.block_meta.persistent(pos).is_some());
        column.set_block(pos, BlockKind::Stone.default_state());
        assert!(column.block_meta.persistent(pos).is_none());

        column
            .set_block_meta(pos, "marker:v", MetaValue::Int(1), true)
            .unwrap();
        column.mark_saved();
        assert!(!column.is_dirty());
        column.set_block(pos, BlockStateId::AIR);
        assert!(column.block_meta.volatile(pos).is_none());
    }

    #[test]
    fn set_and_get_block_with_negative_coordinates() {
        let pos = ChunkPos::new(-1, -1);
        let mut column = ChunkColumn::empty(pos, overworld(), BiomeId(0));
        let block = BlockPos::new(-1, -64, -16);
        assert_eq!(
            column.set_block(block, BlockStateId(1)),
            Some(BlockStateId::AIR)
        );
        assert_eq!(column.block(block), Some(BlockStateId(1)));
        assert_eq!(column.sections()[0].non_air_count(), 1);
        assert!(column.is_dirty());
        assert_eq!(column.revision(), 1);
    }

    #[test]
    fn a_new_column_has_no_saved_ticks() {
        let column = ChunkColumn::empty(ChunkPos::new(0, 0), overworld(), BiomeId(0));
        assert!(column.block_ticks.is_empty());
        assert!(column.fluid_ticks.is_empty());
    }

    #[test]
    fn out_of_column_is_rejected() {
        let mut column = ChunkColumn::empty(ChunkPos::new(0, 0), overworld(), BiomeId(0));
        assert_eq!(
            column.set_block(BlockPos::new(16, 0, 0), BlockStateId(1)),
            None
        );
        assert_eq!(column.block(BlockPos::new(0, 320, 0)), None);
    }

    #[test]
    fn highest_block_finds_top() {
        let mut column = ChunkColumn::empty(ChunkPos::new(0, 0), overworld(), BiomeId(0));
        column.set_block(BlockPos::new(2, 70, 3), BlockStateId(1));
        assert_eq!(column.highest_block(2, 3, |s| !s.is_air()), Some(70));
        assert_eq!(column.highest_block(0, 0, |s| !s.is_air()), None);
    }
}
