#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use rspigot_blocks::BlockKind;
use rspigot_data::registries::biome_id;
use rspigot_domain::chunk::{
    Biomes, BlockEntity, Blocks, ChunkColumn, ChunkLight, ChunkSection, ChunkStatus, Heightmap,
    HeightmapKind, LightArray, PalettedContainer, SavedTick,
};
use rspigot_domain::world::WorldHeight;
use rspigot_domain::{BiomeId, BlockPos, BlockStateId, ChunkPos, Identifier};
use rspigot_nbt::{Compound, Tag};

static COUNTER: AtomicU64 = AtomicU64::new(0);

pub struct TempDir(PathBuf);

impl TempDir {
    pub fn new(label: &str) -> Self {
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let path = std::env::temp_dir().join(format!(
            "rspigot-anvil-it-{label}-{}-{nanos}-{unique}",
            std::process::id()
        ));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }

    pub fn world(&self) -> PathBuf {
        self.0.join("world")
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    pub fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    pub fn below(&mut self, bound: usize) -> usize {
        (self.next() % bound as u64) as usize
    }
}

pub fn overworld() -> Identifier {
    Identifier::minecraft("overworld")
}

pub fn nether() -> Identifier {
    Identifier::minecraft("the_nether")
}

pub fn end() -> Identifier {
    Identifier::minecraft("the_end")
}

pub fn block(name: &str) -> BlockStateId {
    BlockKind::from_name(name).unwrap().default_state()
}

pub fn biome(name: &str) -> BiomeId {
    BiomeId(biome_id(name).unwrap())
}

pub fn empty_sections(count: usize) -> Vec<ChunkSection> {
    (0..count)
        .map(|_| ChunkSection::empty(biome("minecraft:plains")))
        .collect()
}

pub fn simple_column(pos: ChunkPos, variety: usize) -> ChunkColumn {
    simple_column_in(pos, WorldHeight::OVERWORLD, variety)
}

pub fn simple_column_in(pos: ChunkPos, height: WorldHeight, variety: usize) -> ChunkColumn {
    let names = [
        "minecraft:stone",
        "minecraft:dirt",
        "minecraft:granite",
        "minecraft:diorite",
        "minecraft:andesite",
        "minecraft:cobblestone",
        "minecraft:oak_planks",
        "minecraft:bedrock",
    ];
    let mut sections = empty_sections(height.section_count());
    for (index, section) in sections.iter_mut().enumerate().take(variety) {
        let values: Vec<BlockStateId> = (0..4096)
            .map(|i| block(names[(i / 7 + index + pos.x.unsigned_abs() as usize) % names.len()]))
            .collect();
        *section = ChunkSection::from_parts(
            PalettedContainer::<BlockStateId, Blocks>::from_values(&values),
            PalettedContainer::<BiomeId, Biomes>::single(biome("minecraft:plains")),
        );
    }
    ChunkColumn::from_sections(pos, height, sections).unwrap()
}

pub fn rich_column(pos: ChunkPos) -> ChunkColumn {
    let plains = biome("minecraft:plains");
    let river = biome("minecraft:river");
    let chunk = simple_column(pos, 5);
    let mut sections: Vec<ChunkSection> = chunk.sections().to_vec();
    let biomes: Vec<BiomeId> = (0..64)
        .map(|i| if i % 5 == 0 { river } else { plains })
        .collect();
    sections[2] = ChunkSection::from_parts(
        sections[2].blocks().clone(),
        PalettedContainer::<BiomeId, Biomes>::from_values(&biomes),
    );
    let stairs = BlockKind::OakStairs
        .state_with(&[
            ("facing", "east"),
            ("half", "top"),
            ("shape", "inner_left"),
            ("waterlogged", "true"),
        ])
        .unwrap();
    let mut blocks = PalettedContainer::<BlockStateId, Blocks>::single(BlockStateId::AIR);
    blocks.set(100, stairs);
    blocks.set(101, block("minecraft:stone"));
    sections[7] = ChunkSection::from_parts(blocks, PalettedContainer::single(river));
    let mut column = ChunkColumn::from_sections(pos, WorldHeight::OVERWORLD, sections).unwrap();
    column.status = ChunkStatus::Full;
    column.inhabited_time = 12_345;
    let mut light = ChunkLight::new(24);
    light.sky[0] = Some(LightArray::filled(15));
    light.sky[8] = Some(LightArray::filled(9));
    light.block[3] = Some(LightArray::filled(4));
    light.block[25] = Some(LightArray::filled(1));
    column.light = light;
    let mut surface = Heightmap::default();
    surface.set(1, 2, 90);
    surface.set(15, 0, 120);
    column
        .heightmaps
        .maps
        .push((HeightmapKind::WorldSurface, surface));
    let mut blocking = Heightmap::default();
    blocking.set(7, 7, 33);
    column
        .heightmaps
        .maps
        .push((HeightmapKind::MotionBlocking, blocking));
    column.block_entities.push(BlockEntity {
        pos: BlockPos::new(pos.x * 16 + 3, 70, pos.z * 16 + 4),
        kind: Identifier::minecraft("chest"),
        data: Compound::new()
            .with(
                "Items",
                Tag::List(vec![Tag::Compound(
                    Compound::new()
                        .with("Slot", 0i8)
                        .with("id", "minecraft:stick")
                        .with("count", 3),
                )]),
            )
            .with("Lock", ""),
    });
    column.block_ticks = vec![SavedTick {
        id: rspigot_domain::Identifier::minecraft("repeater"),
        pos: rspigot_domain::BlockPos::new(-110, 70, 641),
        priority: -3,
        delay: 2,
    }];
    column.fluid_ticks = vec![SavedTick {
        id: rspigot_domain::Identifier::minecraft("flowing_water"),
        pos: rspigot_domain::BlockPos::new(-111, 70, 641),
        priority: 0,
        delay: 5,
    }];
    column.extra = Compound::new()
        .with("LastUpdate", 777i64)
        .with(
            "structures",
            Compound::new()
                .with("starts", Compound::new())
                .with("References", Compound::new()),
        )
        .with(
            "PostProcessing",
            Tag::List(vec![Tag::List(vec![Tag::Short(5)]), Tag::List(Vec::new())]),
        )
        .with(
            "ChunkBukkitValues",
            Compound::new().with("plugin:marker", 7i64),
        )
        .with(
            "entities",
            Tag::List(vec![Tag::Compound(
                Compound::new().with("id", "minecraft:pig"),
            )]),
        );
    column
}

pub fn heavy_column(pos: ChunkPos, seed: u64, pool: &[BlockStateId]) -> ChunkColumn {
    let mut rng = Rng::new(seed);
    let mut sections = empty_sections(24);
    for section in &mut sections {
        let values: Vec<BlockStateId> = (0..4096).map(|_| pool[rng.below(pool.len())]).collect();
        *section = ChunkSection::from_parts(
            PalettedContainer::<BlockStateId, Blocks>::from_values(&values),
            PalettedContainer::<BiomeId, Biomes>::single(biome("minecraft:plains")),
        );
    }
    ChunkColumn::from_sections(pos, WorldHeight::OVERWORLD, sections).unwrap()
}

pub fn region_entries(path: &Path) -> Vec<(usize, u32, u32)> {
    let bytes = std::fs::read(path).unwrap();
    (0..1024)
        .filter_map(|index| {
            let entry = u32::from_be_bytes(bytes[index * 4..index * 4 + 4].try_into().unwrap());
            (entry != 0).then_some((index, entry >> 8, entry & 0xFF))
        })
        .collect()
}

pub fn assert_no_overlap(path: &Path) {
    let file_len = std::fs::metadata(path).unwrap().len();
    let mut ranges: Vec<(u32, u32)> = region_entries(path)
        .into_iter()
        .map(|(_, sector, count)| (sector, sector + count))
        .collect();
    ranges.sort_unstable();
    for pair in ranges.windows(2) {
        assert!(pair[0].1 <= pair[1].0, "{pair:?}");
    }
    for (start, end) in &ranges {
        assert!(*start >= 2);
        assert!(u64::from(*end) * 4096 <= file_len, "{start}..{end}");
    }
    assert_eq!(file_len % 4096, 0);
}
