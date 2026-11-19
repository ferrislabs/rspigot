use std::sync::atomic::{AtomicBool, Ordering};

use rspigot_blocks::BlockKind;
use rspigot_data::registries::{biome_id, biome_name};
use rspigot_domain::chunk::{
    Biomes, BlockEntity, Blocks, ChunkColumn, ChunkLight, ChunkSection, ChunkStatus, Heightmap,
    HeightmapKind, LightArray, PaletteStrategy, PalettedContainer,
};
use rspigot_domain::world::WorldHeight;
use rspigot_domain::{BiomeId, BlockPos, BlockStateId, ChunkPos, Identifier};
use rspigot_nbt::{Compound, Tag, read_named, write_named};

use crate::error::AnvilError;

pub const DATA_VERSION: i32 = 5023;

const LIGHT_BYTES: usize = 2048;
const AIR_NAME: &str = "minecraft:air";
const DEFAULT_BIOME: &str = "minecraft:plains";

const MODELLED: [&str; 10] = [
    "DataVersion",
    "xPos",
    "yPos",
    "zPos",
    "Status",
    "InhabitedTime",
    "sections",
    "block_entities",
    "Heightmaps",
    "isLightOn",
];

const LEGACY_KEYS: [&str; 6] = [
    "HeightMap",
    "LightPopulated",
    "TerrainPopulated",
    "V",
    "hasLegacyStructureData",
    "CarvingMasks",
];

const BEFORE_SECTIONS: [&str; 4] = [
    "LastUpdate",
    "blending_data",
    "below_zero_retrogen",
    "UpgradeData",
];
const AFTER_BLOCK_ENTITIES: [&str; 4] =
    ["entities", "block_ticks", "fluid_ticks", "PostProcessing"];
const AFTER_HEIGHTMAPS: [&str; 2] = ["structures", "ChunkBukkitValues"];

static WARNED_OLD_VERSION: AtomicBool = AtomicBool::new(false);

type BlockContainer = PalettedContainer<BlockStateId, Blocks>;
type BiomeContainer = PalettedContainer<BiomeId, Biomes>;

pub fn decode(
    bytes: &[u8],
    pos: ChunkPos,
    height: Option<WorldHeight>,
) -> Result<Option<ChunkColumn>, AnvilError> {
    let mut input = bytes;
    let (_, root) = read_named(&mut input)?;
    from_nbt(root, pos, height)
}

pub fn encode(chunk: &ChunkColumn) -> Vec<u8> {
    let mut out = Vec::with_capacity(64 * 1024);
    write_named(&mut out, "", &to_nbt(chunk));
    out
}

pub fn from_nbt(
    mut root: Compound,
    pos: ChunkPos,
    height: Option<WorldHeight>,
) -> Result<Option<ChunkColumn>, AnvilError> {
    let version = root.get_i32("DataVersion").unwrap_or(0);
    if version > DATA_VERSION {
        return Err(AnvilError::NewerDataVersion {
            found: version,
            supported: DATA_VERSION,
        });
    }
    if root.contains_key("Level") && !root.contains_key("sections") {
        return Err(AnvilError::LegacyLayout);
    }
    let status_name = match root.get_str("Status") {
        Some(name) if !name.is_empty() => name,
        _ => return Ok(None),
    };
    if version < DATA_VERSION && !WARNED_OLD_VERSION.swap(true, Ordering::Relaxed) {
        tracing::warn!(
            data_version = version,
            supported = DATA_VERSION,
            "loading chunks written by an older version without upgrading them"
        );
    }
    let status = status_from_name(status_name);
    let height = match height {
        Some(height) => height,
        None => infer_height(&root)?,
    };
    if root.get_i32("xPos").is_some_and(|x| x != pos.x)
        || root.get_i32("zPos").is_some_and(|z| z != pos.z)
    {
        tracing::warn!(
            requested_x = pos.x,
            requested_z = pos.z,
            stored_x = root.get_i32("xPos"),
            stored_z = root.get_i32("zPos"),
            "chunk data is stored at the wrong location"
        );
    }
    let light_on = root.get_bool("isLightOn").unwrap_or(false);
    let plains = BiomeId(biome_id(DEFAULT_BIOME).unwrap_or(0));
    let section_count = height.section_count();
    let min_section = height.min_section();
    let mut sections: Vec<ChunkSection> = (0..section_count)
        .map(|_| ChunkSection::empty(plains))
        .collect();
    let mut light = ChunkLight::new(section_count);
    for tag in root.get_list("sections").unwrap_or(&[]) {
        let Some(section) = tag.as_compound() else {
            continue;
        };
        let Some(y) = section.get_i64("Y").and_then(|y| i32::try_from(y).ok()) else {
            continue;
        };
        let light_index = i64::from(y) - i64::from(min_section) + 1;
        let Ok(light_index) = usize::try_from(light_index) else {
            continue;
        };
        if light_index >= section_count + 2 {
            continue;
        }
        if (1..=section_count).contains(&light_index) {
            sections[light_index - 1] = decode_section(section, pos, y, plains);
        }
        if light_on {
            light.block[light_index] = light_array(section, "BlockLight", pos, y);
            light.sky[light_index] = light_array(section, "SkyLight", pos, y);
        }
    }
    let heightmaps = root
        .get_compound("Heightmaps")
        .map(|compound| decode_heightmaps(compound, height));
    let inhabited_time = root.get_i64("InhabitedTime").unwrap_or(0);
    let block_entities = match root.remove("block_entities") {
        Some(Tag::List(items)) => items.into_iter().filter_map(decode_block_entity).collect(),
        _ => Vec::new(),
    };
    let extra: Compound = root
        .into_iter()
        .filter(|(key, _)| {
            !MODELLED.contains(&key.as_str()) && !LEGACY_KEYS.contains(&key.as_str())
        })
        .collect();
    let mut chunk = ChunkColumn::from_sections(pos, height, sections)
        .ok_or_else(|| AnvilError::corrupt("section count does not match the world height"))?;
    chunk.status = status;
    chunk.inhabited_time = inhabited_time;
    chunk.light = light;
    if let Some(heightmaps) = heightmaps {
        chunk.heightmaps = heightmaps;
    }
    chunk.block_entities = block_entities;
    chunk.extra = extra;
    Ok(Some(chunk))
}

fn infer_height(root: &Compound) -> Result<WorldHeight, AnvilError> {
    let min_section = root
        .get_i32("yPos")
        .ok_or_else(|| AnvilError::corrupt("chunk has no yPos to infer the world height from"))?;
    let max_section = root
        .get_list("sections")
        .unwrap_or(&[])
        .iter()
        .filter_map(Tag::as_compound)
        .filter(|section| section.contains_key("block_states"))
        .filter_map(|section| section.get_i32("Y"))
        .max()
        .ok_or_else(|| {
            AnvilError::corrupt("chunk has no block sections to infer the world height from")
        })?;
    let count = i64::from(max_section) - i64::from(min_section) + 1;
    let count =
        u32::try_from(count).map_err(|_| AnvilError::corrupt("chunk sections are above yPos"))?;
    WorldHeight::new(min_section.saturating_mul(16), count.saturating_mul(16))
        .map_err(|error| AnvilError::corrupt(error.to_string()))
}

fn decode_section(section: &Compound, pos: ChunkPos, y: i32, plains: BiomeId) -> ChunkSection {
    let blocks = decode_container::<BlockStateId, Blocks>(
        section.get_compound("block_states"),
        block_state_from_tag,
    )
    .unwrap_or_else(|reason| {
        report_recoverable(pos, y, "block_states", &reason);
        BlockContainer::single(BlockStateId::AIR)
    });
    let biomes = decode_container::<BiomeId, Biomes>(section.get_compound("biomes"), |tag| {
        biome_from_tag(tag, plains)
    })
    .unwrap_or_else(|reason| {
        report_recoverable(pos, y, "biomes", &reason);
        BiomeContainer::single(plains)
    });
    ChunkSection::from_parts(blocks, biomes)
}

fn report_recoverable(pos: ChunkPos, y: i32, what: &str, reason: &str) {
    tracing::error!(
        x = pos.x,
        y,
        z = pos.z,
        what,
        reason,
        "recoverable error while loading a chunk section"
    );
}

fn light_array(section: &Compound, key: &str, pos: ChunkPos, y: i32) -> Option<LightArray> {
    let bytes = section.get_byte_array(key)?;
    let array = LightArray::from_bytes(bytes);
    if array.is_none() {
        tracing::warn!(
            x = pos.x,
            y,
            z = pos.z,
            key,
            len = bytes.len(),
            expected = LIGHT_BYTES,
            "ignoring a light array of the wrong size"
        );
    }
    array
}

fn decode_heightmaps(
    compound: &Compound,
    height: WorldHeight,
) -> rspigot_domain::chunk::Heightmaps {
    let mut heightmaps = rspigot_domain::chunk::Heightmaps::default();
    for (name, tag) in compound.iter() {
        let (Some(kind), Tag::LongArray(data)) = (HeightmapKind::from_serialized_name(name), tag)
        else {
            tracing::debug!(name, "dropping an unknown heightmap entry");
            continue;
        };
        let packed: Vec<u64> = data.iter().map(|value| *value as u64).collect();
        match Heightmap::unpack(height.height, &packed) {
            Some(map) => heightmaps.maps.push((kind, map)),
            None => tracing::debug!(name, "dropping a heightmap with an unexpected size"),
        }
    }
    heightmaps
}

fn decode_block_entity(tag: Tag) -> Option<BlockEntity> {
    let Tag::Compound(compound) = tag else {
        return None;
    };
    let kind = compound
        .get_str("id")
        .and_then(|id| Identifier::parse(id).ok());
    let position = match (
        compound.get_i32("x"),
        compound.get_i32("y"),
        compound.get_i32("z"),
    ) {
        (Some(x), Some(y), Some(z)) => Some(BlockPos::new(x, y, z)),
        _ => None,
    };
    let (Some(kind), Some(pos)) = (kind, position) else {
        tracing::warn!("skipping a block entity without an id or position");
        return None;
    };
    let data = compound
        .into_iter()
        .filter(|(key, _)| !matches!(key.as_str(), "id" | "x" | "y" | "z"))
        .collect();
    Some(BlockEntity { pos, kind, data })
}

fn ceil_log2(value: usize) -> u8 {
    let mut bits = 0u8;
    while (1usize << bits) < value {
        bits += 1;
    }
    bits
}

fn decode_container<T: Copy + Eq, S: PaletteStrategy>(
    compound: Option<&Compound>,
    entry: impl Fn(&Tag) -> T,
) -> Result<PalettedContainer<T, S>, String> {
    let compound = compound.ok_or("missing container")?;
    let palette: Vec<T> = compound
        .get_list("palette")
        .ok_or("missing palette")?
        .iter()
        .map(entry)
        .collect();
    let data: Vec<u64> = compound
        .get_long_array("data")
        .map(|data| data.iter().map(|value| *value as u64).collect())
        .unwrap_or_default();
    let bits = if palette.len() <= 1 {
        0
    } else {
        ceil_log2(palette.len()).max(S::MIN_BITS)
    };
    PalettedContainer::from_packed(palette, bits, data).map_err(|error| error.to_string())
}

fn normalized(name: &str) -> std::borrow::Cow<'_, str> {
    if name.contains(':') {
        std::borrow::Cow::Borrowed(name)
    } else {
        std::borrow::Cow::Owned(format!("minecraft:{name}"))
    }
}

fn block_state_from_tag(tag: &Tag) -> BlockStateId {
    let Some(compound) = tag.as_compound() else {
        return BlockStateId::AIR;
    };
    let name = normalized(
        compound
            .get_str("Name")
            .or_else(|| compound.get_str("id"))
            .unwrap_or(AIR_NAME),
    );
    let props: Vec<(&str, &str)> = compound
        .get_compound("Properties")
        .or_else(|| compound.get_compound("properties"))
        .map(|properties| {
            properties
                .iter()
                .filter_map(|(key, value)| value.as_str().map(|value| (key, value)))
                .collect()
        })
        .unwrap_or_default();
    let Some(kind) = BlockKind::from_name(&name) else {
        tracing::warn!(block = %name, "unknown block in a chunk palette, using air");
        return BlockStateId::AIR;
    };
    if let Some(state) = kind.state_with(&props) {
        return state;
    }
    tracing::warn!(block = %name, "invalid block properties in a chunk palette, keeping the valid ones");
    props
        .iter()
        .fold(kind.default_state(), |state, (key, value)| {
            state.with_property(key, value).unwrap_or(state)
        })
}

fn biome_from_tag(tag: &Tag, plains: BiomeId) -> BiomeId {
    let name = tag.as_str().map(normalized);
    match name.as_deref().and_then(biome_id) {
        Some(id) => BiomeId(id),
        None => {
            tracing::warn!(biome = ?name, "unknown biome in a chunk palette, using plains");
            plains
        }
    }
}

fn status_from_name(name: &str) -> ChunkStatus {
    match name.strip_prefix("minecraft:").unwrap_or(name) {
        "terrain" => ChunkStatus::Carvers,
        other => ChunkStatus::from_serialized_name(other).unwrap_or(ChunkStatus::Empty),
    }
}

fn status_name(status: ChunkStatus) -> &'static str {
    match status {
        ChunkStatus::Noise | ChunkStatus::Surface => ChunkStatus::Biomes.serialized_name(),
        ChunkStatus::Carvers => "minecraft:terrain",
        other => other.serialized_name(),
    }
}

pub fn to_nbt(chunk: &ChunkColumn) -> Compound {
    let height = chunk.height();
    let extra = &chunk.extra;
    let mut root = Compound::with_capacity(24);
    root.insert("DataVersion", DATA_VERSION);
    root.insert("xPos", chunk.pos().x);
    root.insert("yPos", height.min_section());
    root.insert("zPos", chunk.pos().z);
    copy_extra(&mut root, extra, &BEFORE_SECTIONS[..1]);
    root.insert("InhabitedTime", chunk.inhabited_time);
    root.insert("Status", status_name(chunk.status));
    copy_extra(&mut root, extra, &BEFORE_SECTIONS[1..]);
    root.insert("sections", Tag::List(encode_sections(chunk)));
    let light_on = chunk
        .light
        .sky
        .iter()
        .chain(chunk.light.block.iter())
        .any(Option::is_some);
    if light_on {
        root.insert("isLightOn", true);
    }
    root.insert(
        "block_entities",
        Tag::List(
            chunk
                .block_entities
                .iter()
                .map(encode_block_entity)
                .collect(),
        ),
    );
    copy_extra(&mut root, extra, &AFTER_BLOCK_ENTITIES);
    root.insert("Heightmaps", encode_heightmaps(chunk, height));
    copy_extra(&mut root, extra, &AFTER_HEIGHTMAPS);
    for (key, value) in extra.iter() {
        if !root.contains_key(key) && !MODELLED.contains(&key) {
            root.insert(key, value.clone());
        }
    }
    root
}

fn copy_extra(root: &mut Compound, extra: &Compound, keys: &[&str]) {
    for key in keys {
        if let Some(value) = extra.get(key) {
            root.insert(*key, value.clone());
        }
    }
}

fn encode_sections(chunk: &ChunkColumn) -> Vec<Tag> {
    let min_section = chunk.height().min_section();
    let sections = chunk.sections();
    let light = &chunk.light;
    let light_count = sections.len() + 2;
    let mut out = Vec::with_capacity(light_count);
    for index in 0..light_count {
        let block_light = light.block.get(index).and_then(Option::as_ref);
        let sky_light = light.sky.get(index).and_then(Option::as_ref);
        let section = index.checked_sub(1).and_then(|i| sections.get(i));
        if section.is_none() && block_light.is_none() && sky_light.is_none() {
            continue;
        }
        let mut compound = Compound::with_capacity(5);
        if let Some(section) = section {
            compound.insert(
                "block_states",
                encode_container(section.blocks(), block_state_to_tag),
            );
            compound.insert(
                "biomes",
                encode_container(section.biomes(), |biome| Tag::String(biome_to_name(*biome))),
            );
        }
        if let Some(array) = block_light {
            compound.insert("BlockLight", Tag::ByteArray(array.as_bytes().to_vec()));
        }
        if let Some(array) = sky_light {
            compound.insert("SkyLight", Tag::ByteArray(array.as_bytes().to_vec()));
        }
        let y = min_section - 1 + index as i32;
        compound.insert("Y", y as i8);
        out.push(Tag::Compound(compound));
    }
    out
}

fn biome_to_name(biome: BiomeId) -> String {
    biome_name(biome.0).unwrap_or(DEFAULT_BIOME).to_owned()
}

fn block_state_to_tag(state: &BlockStateId) -> Tag {
    let name = state.kind().map_or(AIR_NAME, BlockKind::name);
    let mut entry = Compound::with_capacity(2);
    entry.insert("Name", name);
    let props: Vec<_> = state.properties().collect();
    if !props.is_empty() {
        let mut compound = Compound::with_capacity(props.len());
        for (key, value) in props {
            compound.insert(key, value);
        }
        entry.insert("Properties", compound);
    }
    Tag::Compound(entry)
}

fn encode_container<T: Copy + Eq, S: PaletteStrategy>(
    container: &PalettedContainer<T, S>,
    entry: impl Fn(&T) -> Tag,
) -> Compound {
    let mut out = Compound::with_capacity(2);
    if let Some(value) = container.single_value() {
        out.insert("palette", Tag::List(vec![entry(&value)]));
        return out;
    }
    let source = container.palette();
    let mut remap = vec![u32::MAX; source.len()];
    let mut palette: Vec<T> = Vec::new();
    let mut indices: Vec<u32> = Vec::with_capacity(S::SIZE);
    for position in 0..S::SIZE {
        let slot = container.palette_index(position);
        if remap[slot] == u32::MAX {
            remap[slot] = palette.len() as u32;
            palette.push(source[slot]);
        }
        indices.push(remap[slot]);
    }
    out.insert("palette", Tag::List(palette.iter().map(&entry).collect()));
    if palette.len() > 1 {
        let bits = usize::from(ceil_log2(palette.len()).max(S::MIN_BITS));
        let per_long = 64 / bits;
        let mut longs = vec![0u64; S::SIZE.div_ceil(per_long)];
        for (position, index) in indices.iter().enumerate() {
            longs[position / per_long] |= u64::from(*index) << ((position % per_long) * bits);
        }
        out.insert(
            "data",
            Tag::LongArray(longs.into_iter().map(|value| value as i64).collect()),
        );
    }
    out
}

fn encode_heightmaps(chunk: &ChunkColumn, height: WorldHeight) -> Compound {
    let mut compound = Compound::with_capacity(chunk.heightmaps.maps.len());
    for (kind, map) in &chunk.heightmaps.maps {
        let packed: Vec<i64> = map
            .pack(height.height)
            .into_iter()
            .map(|value| value as i64)
            .collect();
        compound.insert(kind.serialized_name(), Tag::LongArray(packed));
    }
    compound
}

fn encode_block_entity(entity: &BlockEntity) -> Tag {
    let mut compound = Compound::with_capacity(4 + entity.data.len());
    compound.insert("id", entity.kind.as_str());
    compound.insert("x", entity.pos.x);
    compound.insert("y", entity.pos.y);
    compound.insert("z", entity.pos.z);
    for (key, value) in entity.data.iter() {
        if !matches!(key, "id" | "x" | "y" | "z") {
            compound.insert(key, value.clone());
        }
    }
    Tag::Compound(compound)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rspigot_data::registries::biome_id;
    use rspigot_domain::chunk::{
        Biomes, Blocks, ChunkLight, ChunkSection, ChunkStatus, Heightmap, HeightmapKind,
        LightArray, PalettedContainer,
    };
    use rspigot_domain::{BiomeId, BlockStateId};
    use rspigot_nbt::Tag;

    fn pack(indices: &[usize], bits: usize) -> Vec<i64> {
        let per_long = 64 / bits;
        let mut out = vec![0i64; indices.len().div_ceil(per_long)];
        for (i, value) in indices.iter().enumerate() {
            out[i / per_long] |= (*value as i64) << ((i % per_long) * bits);
        }
        out
    }

    fn state(name: &str) -> BlockStateId {
        BlockKind::from_name(name).unwrap().default_state()
    }

    fn stairs() -> BlockStateId {
        BlockKind::OakStairs
            .state_with(&[
                ("facing", "north"),
                ("half", "bottom"),
                ("shape", "straight"),
                ("waterlogged", "false"),
            ])
            .unwrap()
    }

    fn biome(name: &str) -> BiomeId {
        BiomeId(biome_id(name).unwrap())
    }

    fn container(palette: Vec<Tag>, data: Option<Vec<i64>>) -> Tag {
        let mut compound = Compound::new().with("palette", Tag::List(palette));
        if let Some(data) = data {
            compound.insert("data", data);
        }
        Tag::Compound(compound)
    }

    fn named(name: &str) -> Tag {
        Tag::Compound(Compound::new().with("Name", name))
    }

    fn stairs_tag() -> Tag {
        Tag::Compound(
            Compound::new().with("Name", "minecraft:oak_stairs").with(
                "Properties",
                Compound::new()
                    .with("facing", "north")
                    .with("half", "bottom")
                    .with("shape", "straight")
                    .with("waterlogged", "false"),
            ),
        )
    }

    fn strings(names: &[&str]) -> Vec<Tag> {
        names.iter().map(|n| Tag::String((*n).to_owned())).collect()
    }

    fn section_tag(
        y: i8,
        states: Option<Tag>,
        biomes: Option<Tag>,
        block_light: Option<u8>,
        sky_light: Option<u8>,
    ) -> Tag {
        let mut compound = Compound::new();
        if let Some(states) = states {
            compound.insert("block_states", states);
        }
        if let Some(biomes) = biomes {
            compound.insert("biomes", biomes);
        }
        if let Some(fill) = block_light {
            compound.insert("BlockLight", Tag::ByteArray(vec![fill; 2048]));
        }
        if let Some(fill) = sky_light {
            compound.insert("SkyLight", Tag::ByteArray(vec![fill; 2048]));
        }
        compound.insert("Y", y);
        Tag::Compound(compound)
    }

    const PLAINS: &str = "minecraft:plains";
    const RIVER: &str = "minecraft:river";

    const SEVENTEEN: [&str; 17] = [
        "minecraft:air",
        "minecraft:stone",
        "minecraft:granite",
        "minecraft:polished_granite",
        "minecraft:diorite",
        "minecraft:polished_diorite",
        "minecraft:andesite",
        "minecraft:polished_andesite",
        "minecraft:dirt",
        "minecraft:bedrock",
        "minecraft:cobblestone",
        "minecraft:oak_planks",
        "minecraft:spruce_planks",
        "minecraft:birch_planks",
        "minecraft:jungle_planks",
        "minecraft:acacia_planks",
        "minecraft:dark_oak_planks",
    ];

    fn hand_built_chunk() -> Compound {
        let mut sections = Vec::new();
        sections.push(section_tag(-5, None, None, None, Some(0xAB)));
        sections.push(section_tag(
            -4,
            Some(container(vec![named("minecraft:bedrock")], None)),
            Some(container(strings(&[PLAINS]), None)),
            Some(0x11),
            Some(0xFF),
        ));
        let stairs_indices: Vec<usize> = (0..4096).map(|i| usize::from(i < 16)).collect();
        let river_indices: Vec<usize> = (0..64).map(|i| i % 2).collect();
        sections.push(section_tag(
            -3,
            Some(container(
                vec![named("minecraft:stone"), stairs_tag()],
                Some(pack(&stairs_indices, 4)),
            )),
            Some(container(
                strings(&[PLAINS, RIVER]),
                Some(pack(&river_indices, 1)),
            )),
            None,
            None,
        ));
        for y in -2..=19 {
            let states = if y == 2 {
                let indices: Vec<usize> = (0..4096).map(|i| i % 17).collect();
                container(
                    SEVENTEEN.iter().map(|n| named(n)).collect(),
                    Some(pack(&indices, 5)),
                )
            } else {
                container(vec![named("minecraft:air")], None)
            };
            sections.push(section_tag(
                y,
                Some(states),
                Some(container(strings(&[PLAINS]), None)),
                None,
                None,
            ));
        }
        sections.push(section_tag(20, None, None, None, Some(0xFF)));
        let mut surface = vec![0usize; 256];
        surface[0] = 70;
        surface[255] = 384;
        let heightmaps = Compound::new()
            .with("WORLD_SURFACE", pack(&surface, 9)[..].to_vec())
            .with("MOTION_BLOCKING", pack(&vec![5usize; 256], 9));
        let chest = Compound::new()
            .with("id", "minecraft:chest")
            .with("x", -45)
            .with("y", -62)
            .with("z", 120)
            .with("keepPacked", false)
            .with("Items", Tag::List(Vec::new()));
        Compound::new()
            .with("DataVersion", DATA_VERSION)
            .with("xPos", -3)
            .with("yPos", -4)
            .with("zPos", 7)
            .with("LastUpdate", 123_456i64)
            .with("InhabitedTime", 777i64)
            .with("Status", "minecraft:full")
            .with("sections", Tag::List(sections))
            .with("isLightOn", true)
            .with("block_entities", Tag::List(vec![Tag::Compound(chest)]))
            .with(
                "entities",
                Tag::List(vec![Tag::Compound(
                    Compound::new().with("id", "minecraft:pig"),
                )]),
            )
            .with("block_ticks", Tag::List(Vec::new()))
            .with("fluid_ticks", Tag::List(Vec::new()))
            .with(
                "PostProcessing",
                Tag::List(vec![Tag::List(Vec::new()), Tag::List(Vec::new())]),
            )
            .with("Heightmaps", heightmaps)
            .with(
                "structures",
                Compound::new()
                    .with("starts", Compound::new())
                    .with("References", Compound::new()),
            )
            .with("ChunkBukkitValues", Compound::new().with("plugin:key", 1))
    }

    const POS: ChunkPos = ChunkPos::new(-3, 7);

    fn load(root: Compound) -> ChunkColumn {
        from_nbt(root, POS, Some(WorldHeight::OVERWORLD))
            .unwrap()
            .unwrap()
    }

    #[test]
    fn hand_built_26_3_chunk_loads() {
        let chunk = load(hand_built_chunk());
        assert_eq!(chunk.pos(), POS);
        assert_eq!(chunk.height(), WorldHeight::OVERWORLD);
        assert_eq!(chunk.sections().len(), 24);
        assert_eq!(chunk.status, ChunkStatus::Full);
        assert_eq!(chunk.inhabited_time, 777);
        assert!(!chunk.is_dirty());
        assert_eq!(chunk.revision(), 0);

        let bedrock = &chunk.sections()[0];
        assert_eq!(
            bedrock.blocks().single_value(),
            Some(state("minecraft:bedrock"))
        );
        assert_eq!(bedrock.biomes().single_value(), Some(biome(PLAINS)));

        let mixed = &chunk.sections()[1];
        assert_eq!(mixed.block(3, 0, 0), stairs());
        assert_eq!(mixed.block(15, 0, 0), stairs());
        assert_eq!(mixed.block(0, 0, 1), state("minecraft:stone"));
        assert_eq!(mixed.block(0, 1, 0), state("minecraft:stone"));
        assert_eq!(mixed.biome(0, 0, 0), biome(PLAINS));
        assert_eq!(mixed.biome(1, 0, 0), biome(RIVER));
        assert_eq!(mixed.biome(3, 3, 3), biome(RIVER));
        assert_eq!(mixed.non_air_count(), 4096);

        let wide = &chunk.sections()[6];
        assert_eq!(wide.blocks().palette().len(), 17);
        assert_eq!(wide.blocks().bits(), 5);
        for index in 0..4096usize {
            let (x, y, z) = (index & 15, index >> 8, (index >> 4) & 15);
            assert_eq!(wide.block(x, y, z), state(SEVENTEEN[index % 17]));
        }

        assert_eq!(chunk.light.sky.len(), 26);
        assert_eq!(chunk.light.block.len(), 26);
        assert_eq!(chunk.light.sky[0], LightArray::from_bytes(&[0xAB; 2048]));
        assert_eq!(chunk.light.sky[1], LightArray::from_bytes(&[0xFF; 2048]));
        assert_eq!(chunk.light.block[1], LightArray::from_bytes(&[0x11; 2048]));
        assert_eq!(chunk.light.sky[25], LightArray::from_bytes(&[0xFF; 2048]));
        assert!(chunk.light.sky[2].is_none());
        assert!(chunk.light.block[0].is_none());

        let surface = chunk.heightmaps.get(HeightmapKind::WorldSurface).unwrap();
        assert_eq!(surface.get(0, 0), 70);
        assert_eq!(surface.get(15, 15), 384);
        assert_eq!(surface.get(1, 0), 0);
        let blocking = chunk.heightmaps.get(HeightmapKind::MotionBlocking).unwrap();
        assert_eq!(blocking.get(7, 7), 5);
        assert_eq!(chunk.heightmaps.maps.len(), 2);

        assert_eq!(chunk.block_entities.len(), 1);
        let chest = &chunk.block_entities[0];
        assert_eq!(chest.pos, rspigot_domain::BlockPos::new(-45, -62, 120));
        assert_eq!(chest.kind.as_str(), "minecraft:chest");
        assert_eq!(
            chest.data,
            Compound::new()
                .with("keepPacked", false)
                .with("Items", Tag::List(Vec::new()))
        );

        let extra_keys: Vec<&str> = chunk.extra.iter().map(|(k, _)| k).collect();
        assert_eq!(
            extra_keys,
            [
                "LastUpdate",
                "entities",
                "block_ticks",
                "fluid_ticks",
                "PostProcessing",
                "structures",
                "ChunkBukkitValues"
            ]
        );
        assert_eq!(chunk.extra.get("LastUpdate"), Some(&Tag::Long(123_456)));
        assert_eq!(
            chunk.extra.get_compound("ChunkBukkitValues"),
            Some(&Compound::new().with("plugin:key", 1))
        );
    }

    fn sample_column() -> ChunkColumn {
        let plains = biome(PLAINS);
        let river = biome(RIVER);
        let mut sections: Vec<ChunkSection> =
            (0..24).map(|_| ChunkSection::empty(plains)).collect();
        let stone = state("minecraft:stone");
        let values: Vec<BlockStateId> = (0..4096)
            .map(|i| if i % 3 == 0 { stone } else { stairs() })
            .collect();
        let blocks = PalettedContainer::<BlockStateId, Blocks>::from_values(&values);
        let biomes = PalettedContainer::<BiomeId, Biomes>::from_values(
            &(0..64)
                .map(|i| if i % 5 == 0 { river } else { plains })
                .collect::<Vec<_>>(),
        );
        sections[3] = ChunkSection::from_parts(blocks, biomes);
        let many: Vec<BlockStateId> = (0..4096).map(|i| state(SEVENTEEN[(i * 7) % 17])).collect();
        sections[9] = ChunkSection::from_parts(
            PalettedContainer::from_values(&many),
            PalettedContainer::single(river),
        );
        let wide_biomes: Vec<BiomeId> = (0..64)
            .map(|i| {
                BiomeId(
                    biome_id(
                        [
                            "minecraft:desert",
                            "minecraft:forest",
                            PLAINS,
                            RIVER,
                            "minecraft:swamp",
                            "minecraft:taiga",
                            "minecraft:jungle",
                            "minecraft:ocean",
                            "minecraft:beach",
                        ][i % 9],
                    )
                    .unwrap(),
                )
            })
            .collect();
        sections[10] = ChunkSection::from_parts(
            PalettedContainer::single(stone),
            PalettedContainer::from_values(&wide_biomes),
        );
        let mut chunk =
            ChunkColumn::from_sections(ChunkPos::new(5, -9), WorldHeight::OVERWORLD, sections)
                .unwrap();
        chunk.status = ChunkStatus::Full;
        chunk.inhabited_time = 4242;
        let mut light = ChunkLight::new(24);
        light.sky[0] = Some(LightArray::filled(15));
        light.sky[4] = Some(LightArray::filled(7));
        light.block[5] = Some(LightArray::filled(3));
        light.block[25] = Some(LightArray::filled(0));
        chunk.light = light;
        let mut surface = Heightmap::default();
        surface.set(2, 3, 100);
        surface.set(15, 15, 384);
        chunk
            .heightmaps
            .maps
            .push((HeightmapKind::WorldSurface, surface));
        let mut floor = Heightmap::default();
        floor.set(0, 0, 64);
        chunk
            .heightmaps
            .maps
            .push((HeightmapKind::OceanFloor, floor));
        chunk
            .block_entities
            .push(rspigot_domain::chunk::BlockEntity {
                pos: rspigot_domain::BlockPos::new(82, 10, -130),
                kind: rspigot_domain::Identifier::minecraft("barrel"),
                data: Compound::new().with("Lock", "").with("CustomName", "x"),
            });
        chunk.extra = Compound::new()
            .with("LastUpdate", 99i64)
            .with(
                "structures",
                Compound::new().with("starts", Compound::new()),
            )
            .with("PostProcessing", Tag::List(vec![Tag::List(Vec::new())]))
            .with("Unknown", Tag::IntArray(vec![1, 2, 3]));
        chunk
    }

    #[test]
    fn save_then_load_returns_an_equal_column() {
        let original = sample_column();
        let nbt = to_nbt(&original);
        let loaded = from_nbt(nbt, original.pos(), Some(original.height()))
            .unwrap()
            .unwrap();
        assert_eq!(loaded, original);
        let bytes = encode(&original);
        let decoded = decode(&bytes, original.pos(), Some(original.height()))
            .unwrap()
            .unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn nether_and_end_heights_round_trip() {
        for height in [WorldHeight::NETHER, WorldHeight::END] {
            let mut chunk = ChunkColumn::empty(ChunkPos::new(0, 0), height, biome(PLAINS));
            chunk.light.sky = vec![None; 18];
            chunk.light.sky[1] = Some(LightArray::filled(15));
            let loaded = decode(&encode(&chunk), chunk.pos(), Some(height))
                .unwrap()
                .unwrap();
            assert_eq!(loaded, chunk);
            let nbt = to_nbt(&chunk);
            assert_eq!(nbt.get_i32("yPos"), Some(0));
        }
    }

    #[test]
    fn encoded_root_follows_vanilla_key_order() {
        let nbt = to_nbt(&sample_column());
        let keys: Vec<&str> = nbt.iter().map(|(k, _)| k).collect();
        assert_eq!(
            keys,
            [
                "DataVersion",
                "xPos",
                "yPos",
                "zPos",
                "LastUpdate",
                "InhabitedTime",
                "Status",
                "sections",
                "isLightOn",
                "block_entities",
                "PostProcessing",
                "Heightmaps",
                "structures",
                "Unknown"
            ]
        );
        assert_eq!(nbt.get_i32("DataVersion"), Some(5023));
        assert_eq!(nbt.get_i32("xPos"), Some(5));
        assert_eq!(nbt.get_i32("yPos"), Some(-4));
        assert_eq!(nbt.get_i32("zPos"), Some(-9));
        assert_eq!(nbt.get_str("Status"), Some("minecraft:full"));
        assert_eq!(nbt.get_i64("InhabitedTime"), Some(4242));
        assert_eq!(nbt.get_bool("isLightOn"), Some(true));
    }

    fn section_compound(nbt: &Compound, y: i32) -> Compound {
        nbt.get_list("sections")
            .unwrap()
            .iter()
            .filter_map(Tag::as_compound)
            .find(|s| s.get_i32("Y") == Some(y))
            .unwrap()
            .clone()
    }

    #[test]
    fn sections_are_encoded_like_vanilla() {
        let nbt = to_nbt(&sample_column());
        let sections = nbt.get_list("sections").unwrap();
        let ys: Vec<i32> = sections
            .iter()
            .map(|s| s.as_compound().unwrap().get_i32("Y").unwrap())
            .collect();
        assert_eq!(ys, (-5..=20).collect::<Vec<_>>());

        let below = section_compound(&nbt, -5);
        let keys: Vec<&str> = below.iter().map(|(k, _)| k).collect();
        assert_eq!(keys, ["SkyLight", "Y"]);
        let above = section_compound(&nbt, 20);
        let keys: Vec<&str> = above.iter().map(|(k, _)| k).collect();
        assert_eq!(keys, ["BlockLight", "Y"]);

        let empty = section_compound(&nbt, -4);
        let keys: Vec<&str> = empty.iter().map(|(k, _)| k).collect();
        assert_eq!(keys, ["block_states", "biomes", "Y"]);
        let states = empty.get_compound("block_states").unwrap();
        assert_eq!(states.get_list("palette").unwrap().len(), 1);
        assert!(states.get("data").is_none());
        assert_eq!(
            states.get_list("palette").unwrap()[0],
            named("minecraft:air")
        );

        let mixed = section_compound(&nbt, -1);
        let states = mixed.get_compound("block_states").unwrap();
        let palette = states.get_list("palette").unwrap();
        assert_eq!(palette, &[named("minecraft:stone"), stairs_tag()]);
        assert_eq!(states.get_long_array("data").unwrap().len(), 256);
        let biomes = mixed.get_compound("biomes").unwrap();
        assert_eq!(
            biomes.get_list("palette").unwrap(),
            &strings(&[RIVER, PLAINS])[..]
        );
        assert_eq!(biomes.get_long_array("data").unwrap().len(), 1);

        let wide = section_compound(&nbt, 5);
        let states = wide.get_compound("block_states").unwrap();
        assert_eq!(states.get_list("palette").unwrap().len(), 17);
        assert_eq!(states.get_long_array("data").unwrap().len(), 342);

        let wide_biomes = section_compound(&nbt, 6);
        let biomes = wide_biomes.get_compound("biomes").unwrap();
        assert_eq!(biomes.get_list("palette").unwrap().len(), 9);
        assert_eq!(biomes.get_long_array("data").unwrap().len(), 4);
        assert_eq!(
            wide_biomes
                .get_compound("block_states")
                .unwrap()
                .get_list("palette")
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn unused_palette_entries_are_not_written() {
        let plains = biome(PLAINS);
        let mut sections: Vec<ChunkSection> =
            (0..24).map(|_| ChunkSection::empty(plains)).collect();
        let mut blocks = PalettedContainer::<BlockStateId, Blocks>::single(BlockStateId::AIR);
        blocks.set(0, state("minecraft:stone"));
        blocks.set(0, state("minecraft:dirt"));
        blocks.set(1, state("minecraft:dirt"));
        assert_eq!(blocks.palette().len(), 3);
        sections[0] = ChunkSection::from_parts(blocks, PalettedContainer::single(plains));
        let chunk =
            ChunkColumn::from_sections(ChunkPos::new(0, 0), WorldHeight::OVERWORLD, sections)
                .unwrap();
        let nbt = to_nbt(&chunk);
        let section = section_compound(&nbt, -4);
        let palette = section
            .get_compound("block_states")
            .unwrap()
            .get_list("palette")
            .unwrap();
        assert_eq!(palette, &[named("minecraft:dirt"), named("minecraft:air")]);
        let loaded = from_nbt(nbt, chunk.pos(), Some(chunk.height()))
            .unwrap()
            .unwrap();
        for i in 0..4096 {
            let (x, y, z) = (i & 15, i >> 8, (i >> 4) & 15);
            assert_eq!(
                loaded.sections()[0].block(x, y, z),
                chunk.sections()[0].block(x, y, z)
            );
        }
    }

    #[test]
    fn newer_data_versions_are_refused() {
        let mut root = hand_built_chunk();
        root.insert("DataVersion", 5024);
        assert!(matches!(
            from_nbt(root, POS, Some(WorldHeight::OVERWORLD)),
            Err(AnvilError::NewerDataVersion {
                found: 5024,
                supported: 5023
            })
        ));
    }

    #[test]
    fn older_data_versions_load_best_effort() {
        let mut root = hand_built_chunk();
        root.insert("DataVersion", 4325);
        assert!(
            from_nbt(root, POS, Some(WorldHeight::OVERWORLD))
                .unwrap()
                .is_some()
        );
        let mut root = hand_built_chunk();
        root.remove("DataVersion");
        assert!(
            from_nbt(root, POS, Some(WorldHeight::OVERWORLD))
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn missing_status_means_no_chunk() {
        let mut root = hand_built_chunk();
        root.remove("Status");
        assert!(
            from_nbt(root, POS, Some(WorldHeight::OVERWORLD))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn pre_1_18_layout_is_refused() {
        let root = Compound::new()
            .with("DataVersion", 2975)
            .with("Level", Compound::new().with("xPos", 0));
        assert!(matches!(
            from_nbt(root, POS, Some(WorldHeight::OVERWORLD)),
            Err(AnvilError::LegacyLayout)
        ));
    }

    fn converted_entry(tag: &Tag) -> Tag {
        let Some(entry) = tag.as_compound() else {
            return tag.clone();
        };
        let mut out = Compound::new();
        for (key, value) in entry.iter() {
            match key {
                "Name" => out.insert("id", value.clone()),
                "Properties" => out.insert("properties", value.clone()),
                other => out.insert(other, value.clone()),
            };
        }
        Tag::Compound(out)
    }

    fn converted_chunk(status: &str) -> Compound {
        let mut root = hand_built_chunk();
        let mut sections: Vec<Tag> = root.get_list("sections").unwrap().to_vec();
        for tag in &mut sections {
            let Tag::Compound(section) = tag else {
                continue;
            };
            let Some(states) = section.get_compound("block_states") else {
                continue;
            };
            let palette: Vec<Tag> = states
                .get_list("palette")
                .unwrap()
                .iter()
                .map(converted_entry)
                .collect();
            let mut rebuilt = states.clone();
            rebuilt.insert("palette", Tag::List(palette));
            section.insert("block_states", rebuilt);
        }
        sections.reverse();
        root.insert("sections", Tag::List(sections));
        root.remove("yPos");
        root.remove("isLightOn");
        root.remove("Heightmaps");
        root.insert("Status", status);
        root.insert("HeightMap", Tag::IntArray(vec![0i32; 256]));
        root.insert("LightPopulated", 1i8);
        root.insert("TerrainPopulated", 1i8);
        root.insert("V", 1i8);
        root.insert("hasLegacyStructureData", 1i8);
        root.insert("CarvingMasks", Compound::new());
        root.insert(
            "UpgradeData",
            Compound::new().with("Indices", Compound::new()),
        );
        root
    }

    #[test]
    fn converted_palette_entries_keyed_by_id_decode_like_the_named_ones() {
        let expected = load(hand_built_chunk());
        let chunk = load(converted_chunk("minecraft:spawn"));
        for (index, (got, want)) in chunk.sections().iter().zip(expected.sections()).enumerate() {
            assert_eq!(
                got.blocks().values().collect::<Vec<_>>(),
                want.blocks().values().collect::<Vec<_>>(),
                "section {index}"
            );
        }
        assert_eq!(chunk.sections()[1].block(3, 0, 0), stairs());
    }

    #[test]
    fn converted_chunks_with_a_partial_status_are_loadable() {
        for name in ["minecraft:terrain", "minecraft:features", "minecraft:spawn"] {
            let chunk = load(converted_chunk(name));
            assert_ne!(chunk.status, ChunkStatus::Empty, "{name}");
            assert!(chunk.sections()[1].non_air_count() > 0, "{name}");
        }
    }

    #[test]
    fn leftover_legacy_keys_are_dropped_and_not_written_back() {
        let chunk = load(converted_chunk("minecraft:spawn"));
        for key in [
            "HeightMap",
            "LightPopulated",
            "TerrainPopulated",
            "V",
            "hasLegacyStructureData",
            "CarvingMasks",
        ] {
            assert!(!chunk.extra.contains_key(key), "{key} kept in extra");
            assert!(!to_nbt(&chunk).contains_key(key), "{key} written back");
        }
        assert!(chunk.light.sky.iter().all(Option::is_none));
    }

    #[test]
    fn status_names_follow_26_3() {
        let mut root = hand_built_chunk();
        root.insert("Status", "minecraft:terrain");
        assert_eq!(load(root).status, ChunkStatus::Carvers);
        let mut root = hand_built_chunk();
        root.insert("Status", "minecraft:features");
        assert_eq!(load(root).status, ChunkStatus::Features);
        let mut root = hand_built_chunk();
        root.insert("Status", "minecraft:bogus");
        assert_eq!(load(root).status, ChunkStatus::Empty);

        let mut chunk = sample_column();
        for (status, name) in [
            (ChunkStatus::Empty, "minecraft:empty"),
            (ChunkStatus::Biomes, "minecraft:biomes"),
            (ChunkStatus::Noise, "minecraft:biomes"),
            (ChunkStatus::Surface, "minecraft:biomes"),
            (ChunkStatus::Carvers, "minecraft:terrain"),
            (ChunkStatus::Features, "minecraft:features"),
            (ChunkStatus::InitializeLight, "minecraft:initialize_light"),
            (ChunkStatus::Light, "minecraft:light"),
            (ChunkStatus::Spawn, "minecraft:spawn"),
            (ChunkStatus::Full, "minecraft:full"),
        ] {
            chunk.status = status;
            assert_eq!(to_nbt(&chunk).get_str("Status"), Some(name));
        }
    }

    #[test]
    fn light_is_ignored_unless_marked_correct() {
        let mut root = hand_built_chunk();
        root.insert("isLightOn", false);
        let chunk = load(root);
        assert!(chunk.light.sky.iter().all(Option::is_none));
        assert!(chunk.light.block.iter().all(Option::is_none));
        assert_eq!(chunk.light.sky.len(), 26);
        let mut root = hand_built_chunk();
        root.remove("isLightOn");
        assert!(load(root).light.sky.iter().all(Option::is_none));

        let mut chunk = sample_column();
        chunk.light = ChunkLight::new(24);
        assert!(to_nbt(&chunk).get("isLightOn").is_none());
    }

    #[test]
    fn unknown_blocks_and_biomes_degrade_gracefully() {
        let mut root = hand_built_chunk();
        let sections = match root.get_mut("sections") {
            Some(Tag::List(sections)) => sections,
            _ => unreachable!(),
        };
        sections[2] = section_tag(
            -3,
            Some(container(
                vec![
                    named("minecraft:stone"),
                    named("modded:unobtainium"),
                    Tag::Compound(
                        Compound::new().with("Name", "minecraft:oak_stairs").with(
                            "Properties",
                            Compound::new()
                                .with("facing", "sideways")
                                .with("half", "top"),
                        ),
                    ),
                ],
                Some(pack(&(0..4096).map(|i| i % 3).collect::<Vec<_>>(), 4)),
            )),
            Some(container(
                strings(&[PLAINS, "modded:nowhere"]),
                Some(pack(&(0..64).map(|i| i % 2).collect::<Vec<_>>(), 1)),
            )),
            None,
            None,
        );
        let chunk = load(root);
        let section = &chunk.sections()[1];
        assert_eq!(section.block(0, 0, 0), state("minecraft:stone"));
        assert_eq!(section.block(1, 0, 0), BlockStateId::AIR);
        let half_top = BlockKind::OakStairs.state_with(&[("half", "top")]).unwrap();
        assert_eq!(section.block(2, 0, 0), half_top);
        assert_eq!(section.biome(1, 0, 0), biome(PLAINS));
    }

    #[test]
    fn malformed_section_data_falls_back_to_defaults() {
        let mut root = hand_built_chunk();
        let sections = match root.get_mut("sections") {
            Some(Tag::List(sections)) => sections,
            _ => unreachable!(),
        };
        sections[2] = section_tag(
            -3,
            Some(container(
                vec![named("minecraft:stone"), named("minecraft:dirt")],
                Some(vec![0; 10]),
            )),
            Some(container(strings(&[PLAINS, RIVER]), None)),
            None,
            None,
        );
        let chunk = load(root);
        assert_eq!(
            chunk.sections()[1].blocks().single_value(),
            Some(BlockStateId::AIR)
        );
        assert_eq!(
            chunk.sections()[1].biomes().single_value(),
            Some(biome(PLAINS))
        );
        assert_eq!(
            chunk.sections()[0].blocks().single_value(),
            Some(state("minecraft:bedrock"))
        );
    }

    #[test]
    fn missing_and_out_of_range_sections_are_handled() {
        let mut root = hand_built_chunk();
        let sections = match root.get_mut("sections") {
            Some(Tag::List(sections)) => sections,
            _ => unreachable!(),
        };
        sections.retain(|s| s.as_compound().and_then(|c| c.get_i32("Y")) != Some(5));
        sections.push(section_tag(
            40,
            Some(container(vec![named("minecraft:stone")], None)),
            None,
            None,
            Some(1),
        ));
        sections.push(Tag::Int(7));
        let chunk = load(root);
        assert_eq!(chunk.sections().len(), 24);
        assert_eq!(
            chunk.sections()[9].blocks().single_value(),
            Some(BlockStateId::AIR)
        );
        assert_eq!(
            chunk.sections()[9].biomes().single_value(),
            Some(biome(PLAINS))
        );
    }

    #[test]
    fn height_is_inferred_when_unknown() {
        let chunk = from_nbt(hand_built_chunk(), POS, None).unwrap().unwrap();
        assert_eq!(chunk.height(), WorldHeight::OVERWORLD);
        let mut root = hand_built_chunk();
        root.remove("sections");
        assert!(from_nbt(root, POS, None).is_err());
    }

    #[test]
    fn wrong_length_heightmaps_are_dropped() {
        let mut root = hand_built_chunk();
        root.insert(
            "Heightmaps",
            Compound::new()
                .with("WORLD_SURFACE", vec![0i64; 5])
                .with("OCEAN_FLOOR", pack(&vec![1usize; 256], 9))
                .with("NOT_A_MAP", vec![0i64; 37]),
        );
        let chunk = load(root);
        assert_eq!(chunk.heightmaps.maps.len(), 1);
        assert_eq!(chunk.heightmaps.maps[0].0, HeightmapKind::OceanFloor);
    }

    #[test]
    fn block_entities_without_a_position_are_skipped() {
        let mut root = hand_built_chunk();
        root.insert(
            "block_entities",
            Tag::List(vec![
                Tag::Compound(Compound::new().with("id", "minecraft:chest")),
                Tag::Compound(Compound::new().with("x", 1).with("y", 2).with("z", 3)),
                Tag::Compound(
                    Compound::new()
                        .with("id", "minecraft:sign")
                        .with("x", -40)
                        .with("y", 70)
                        .with("z", 113),
                ),
            ]),
        );
        let chunk = load(root);
        assert_eq!(chunk.block_entities.len(), 1);
        assert_eq!(chunk.block_entities[0].kind.as_str(), "minecraft:sign");
        assert!(chunk.block_entities[0].data.is_empty());
    }

    #[test]
    fn garbage_bytes_are_an_error() {
        assert!(decode(&[1, 2, 3], POS, Some(WorldHeight::OVERWORLD)).is_err());
    }
}
