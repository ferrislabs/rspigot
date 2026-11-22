use std::collections::HashSet;

use rspigot_nbt::{Compound, Tag};

use super::legacy_tables::{BIOMES_BY_ID, BLOCKS_BEFORE_FEATURE_STATUS, CAVES_AND_CLIFFS_BIOMES};
use super::nbt::{compound_mut, list_mut, walk_item_stacks};

const HEIGHTMAP_TYPES: [&str; 7] = [
    "WORLD_SURFACE_WG",
    "WORLD_SURFACE",
    "WORLD_SURFACE_IGNORE_SNOW",
    "OCEAN_FLOOR_WG",
    "OCEAN_FLOOR",
    "MOTION_BLOCKING",
    "MOTION_BLOCKING_NO_LEAVES",
];
const STATUS_IS_OR_AFTER_SURFACE: [&str; 8] = [
    "surface",
    "carvers",
    "liquid_carvers",
    "features",
    "light",
    "spawn",
    "heightmaps",
    "full",
];
const STATUS_IS_OR_AFTER_NOISE: [&str; 9] = [
    "noise",
    "surface",
    "carvers",
    "liquid_carvers",
    "features",
    "light",
    "spawn",
    "heightmaps",
    "full",
];
const ALWAYS_WATERLOGGED: [&str; 5] = [
    "minecraft:bubble_column",
    "minecraft:kelp",
    "minecraft:kelp_plant",
    "minecraft:seagrass",
    "minecraft:tall_seagrass",
];
const AIR: &str = "minecraft:air";
const PLAINS: &str = "minecraft:plains";

fn empty_list() -> Tag {
    Tag::List(Vec::new())
}

fn number(tag: &Tag) -> Option<i64> {
    match tag {
        Tag::Byte(value) => Some(i64::from(*value)),
        Tag::Short(value) => Some(i64::from(*value)),
        Tag::Int(value) => Some(i64::from(*value)),
        Tag::Long(value) => Some(*value),
        _ => None,
    }
}

fn number_or(compound: &Compound, key: &str, default: i64) -> i64 {
    compound.get(key).and_then(number).unwrap_or(default)
}

fn ceil_log2(value: usize) -> i32 {
    if value == 0 {
        return 0;
    }
    ((value as f64).ln() / 2f64.ln()).ceil() as i32
}

fn state_name(state: &Compound) -> &str {
    state.get_str("Name").unwrap_or(AIR)
}

fn palette_entries(container: &Compound) -> Vec<Compound> {
    container
        .get_list("palette")
        .unwrap_or_default()
        .iter()
        .filter_map(Tag::as_compound)
        .cloned()
        .collect()
}

struct Packed {
    palette: Vec<Compound>,
    data: Vec<i64>,
    bits: i32,
}

impl Packed {
    fn of(container: &Compound) -> Self {
        let palette = palette_entries(container);
        let bits = 4.max(ceil_log2(palette.len()));
        Self {
            palette,
            data: container
                .get_long_array("data")
                .map(<[i64]>::to_vec)
                .unwrap_or_default(),
            bits,
        }
    }

    fn get(&self, x: i32, y: i32, z: i32) -> Option<&Compound> {
        match self.palette.len() {
            0 => return None,
            1 => return self.palette.first(),
            _ => {}
        }
        let index = ((y << 4) | z) << 4 | x;
        let per_long = 64 / self.bits;
        let long_index = usize::try_from(index / per_long).ok()?;
        let word = *self.data.get(long_index)?;
        let shift = (index - (index / per_long) * per_long) * self.bits;
        let mask = (1i64 << self.bits) - 1;
        let value = usize::try_from((word >> shift) & mask).ok()?;
        self.palette.get(value)
    }
}

fn palette_container(palette: Vec<Tag>) -> Compound {
    Compound::new().with("palette", Tag::List(palette))
}

fn biome_name(id: i32) -> &'static str {
    BIOMES_BY_ID
        .iter()
        .find(|(known, _)| *known == id)
        .map_or(PLAINS, |(_, name)| name)
}

fn old_biome(biomes: &[i32], index: usize) -> i32 {
    biomes.get(index).map_or(0, |id| id & 255)
}

fn biome_container(old: impl Fn(usize) -> i32) -> Compound {
    let mut indices: Vec<(i32, i64)> = Vec::new();
    for cell in 0..64 {
        let id = old(cell);
        if !indices.iter().any(|(known, _)| *known == id) {
            let next = indices.len() as i64;
            indices.push((id, next));
        }
    }
    let palette: Vec<Tag> = indices
        .iter()
        .map(|(id, _)| Tag::String(biome_name(*id).to_owned()))
        .collect();
    let bits = ceil_log2(indices.len());
    if bits == 0 {
        return palette_container(palette);
    }
    let per_long = 64 / bits;
    let longs = ((64 + per_long - 1) / per_long) as usize;
    let mut data = vec![0i64; longs];
    let (mut at, mut shift) = (0usize, 0);
    for cell in 0..64 {
        let id = old(cell);
        let value = indices
            .iter()
            .find(|(known, _)| *known == id)
            .map_or(0, |(_, index)| *index);
        data[at] |= value << shift;
        shift += bits;
        if shift + bits > 64 {
            at += 1;
            shift = 0;
        }
    }
    palette_container(palette).with("data", Tag::LongArray(data))
}

fn biome_containers(
    level: &Compound,
    overworld: bool,
    offset: i32,
    custom_height: &mut bool,
) -> Vec<Compound> {
    let length = if overworld { 24 } else { 16 };
    let plains = palette_container(vec![Tag::String(PLAINS.to_owned())]);
    let mut containers = vec![plains; length];
    let old: Option<Vec<i32>> = level.get_int_array("Biomes").map(<[i32]>::to_vec);
    let Some(old) = old else {
        return containers;
    };
    if old.len() == 1536 {
        *custom_height = true;
        for (section, slot) in containers.iter_mut().enumerate().take(24) {
            *slot = biome_container(|cell| old_biome(&old, section * 64 + cell));
        }
    } else if old.len() == 1024 {
        for section in 0..16usize {
            let index = section as i32 - offset;
            if let Some(slot) = usize::try_from(index)
                .ok()
                .and_then(|index| containers.get_mut(index))
            {
                *slot = biome_container(|cell| old_biome(&old, section * 64 + cell));
            }
        }
        if overworld {
            let bottom = biome_container(|cell| old_biome(&old, cell % 16));
            let top = biome_container(|cell| old_biome(&old, 1008 + cell % 16));
            for slot in &mut containers[0..4] {
                *slot = bottom.clone();
            }
            for slot in &mut containers[20..24] {
                *slot = top.clone();
            }
        }
    }
    containers
}

fn padded_palette(mut palette: Vec<Tag>, data_longs: usize) -> Vec<Tag> {
    let stored_bits = (data_longs as i64 * 64) / 4096;
    let size = palette.len();
    if stored_bits > i64::from(ceil_log2(size)) {
        let target = (1i64 << (stored_bits - 1)) + 1;
        let missing = target - size as i64;
        let air = Tag::Compound(Compound::new().with("Name", AIR));
        for _ in 0..missing.max(0) {
            palette.push(air.clone());
        }
    }
    palette
}

fn optimized_container(palette: &[Tag], data: &Tag) -> Compound {
    if palette.len() == 1 {
        return palette_container(palette.to_vec());
    }
    let longs = match data {
        Tag::LongArray(values) => values.len(),
        _ => 0,
    };
    palette_container(padded_palette(palette.to_vec(), longs)).with("data", data.clone())
}

fn context(root: &Compound) -> (bool, bool) {
    let context = root.get_compound("__context");
    let read = |key: &str| context.and_then(|c| c.get_str(key)).unwrap_or("");
    (
        read("dimension") == "minecraft:overworld",
        read("generator") == "minecraft:noise",
    )
}

fn section_blocks(
    section: &mut Compound,
    empty: &Compound,
    containers: &[Compound],
    offset: i32,
    seen: &mut HashSet<i32>,
    names: &mut HashSet<String>,
    at_zero: &mut Option<Packed>,
) {
    let y = number_or(section, "Y", 0) as i32;
    let palette: Option<Vec<Tag>> = section.get_list("Palette").map(<[Tag]>::to_vec);
    let blocks = match palette {
        Some(palette) => {
            for entry in palette.iter().filter_map(Tag::as_compound) {
                names.insert(state_name(entry).to_owned());
            }
            match section.get("BlockStates") {
                Some(data) => optimized_container(&palette, data),
                None => empty.clone(),
            }
        }
        None => empty.clone(),
    };
    if let Some(container) = usize::try_from(y - offset)
        .ok()
        .and_then(|index| containers.get(index))
    {
        section.insert("biomes", container.clone());
    }
    seen.insert(y);
    if section
        .get("Y")
        .and_then(number)
        .unwrap_or(i64::from(i32::MAX))
        == 0
    {
        *at_zero = Some(Packed::of(&blocks));
    }
    section.insert("block_states", blocks);
    section.remove("Palette");
    section.remove("BlockStates");
}

fn predicted_status(status: &str, names: &mut HashSet<String>) -> String {
    if STATUS_IS_OR_AFTER_SURFACE.contains(&status) {
        return status.to_owned();
    }
    names.remove(AIR);
    let has_blocks = !names.is_empty();
    for before in BLOCKS_BEFORE_FEATURE_STATUS {
        names.remove(before);
    }
    if !names.is_empty() {
        return "liquid_carvers".to_owned();
    }
    if status == "noise" || has_blocks {
        return "noise".to_owned();
    }
    if status == "biomes" {
        return "structure_references".to_owned();
    }
    status.to_owned()
}

fn bytes_to_longs(bytes: &[u8]) -> Vec<i64> {
    bytes
        .chunks(8)
        .map(|chunk| {
            let mut word = [0u8; 8];
            word[..chunk.len()].copy_from_slice(chunk);
            i64::from_le_bytes(word)
        })
        .collect()
}

fn update_carving_masks(level: &mut Compound, sections: usize, offset: usize) {
    let mut updated = Compound::new();
    if let Some(masks) = level.get_compound("CarvingMasks") {
        for (key, value) in masks.iter() {
            let Tag::ByteArray(bytes) = value else {
                updated.insert(key, value.clone());
                continue;
            };
            let source = bytes_to_longs(bytes);
            let mut target = vec![0i64; 64 * sections];
            for (index, word) in source.iter().enumerate() {
                if let Some(slot) = target.get_mut(64 * offset + index) {
                    *slot = *word;
                }
            }
            updated.insert(key, Tag::LongArray(target));
        }
    }
    level.insert("CarvingMasks", updated);
}

fn fixed_heightmap(word: i64) -> i64 {
    let mut result = 0i64;
    let mut at = 0;
    while at + 9 <= 64 {
        let value = (word >> at) & 511;
        let fixed = if value == 0 { 0 } else { (value + 64).min(511) };
        result |= fixed << at;
        at += 9;
    }
    result
}

fn update_heightmaps(level: &mut Compound) {
    let Some(heightmaps) = compound_mut(level, "Heightmaps") else {
        return;
    };
    for name in HEIGHTMAP_TYPES {
        if let Some(Tag::LongArray(words)) = heightmaps.get_mut(name) {
            for word in words.iter_mut() {
                *word = fixed_heightmap(*word);
            }
        }
    }
}

fn add_padding_entries(level: &mut Compound, key: &str) {
    let mut list: Vec<Tag> = level.get_list(key).map(<[Tag]>::to_vec).unwrap_or_default();
    if list.len() == 24 {
        return;
    }
    for _ in 0..4 {
        list.insert(0, empty_list());
        list.push(empty_list());
    }
    level.insert(key, Tag::List(list));
}

fn shift_upgrade_data(level: &mut Compound) {
    let Some(data) = compound_mut(level, "UpgradeData") else {
        return;
    };
    let Some(indices) = data.get_compound("Indices").cloned() else {
        return;
    };
    let mut shifted = Compound::new();
    for (key, value) in indices.iter() {
        if let Ok(section) = key.parse::<i32>() {
            shifted.insert((section + 4).to_string(), value.clone());
        }
    }
    data.insert("Indices", shifted);
}

fn bit_longs(bits: &[bool]) -> Vec<i64> {
    let mut words = vec![0i64; bits.len().div_ceil(64)];
    for (index, set) in bits.iter().enumerate() {
        if *set {
            words[index / 64] |= 1i64 << (index % 64);
        }
    }
    while words.last() == Some(&0) {
        words.pop();
    }
    words
}

fn retrogen(level: &mut Compound, container: &Packed) {
    let Some(status) = level.get_str("Status").map(str::to_owned) else {
        return;
    };
    if status == "empty" {
        return;
    }
    let old_noise = STATUS_IS_OR_AFTER_NOISE.contains(&status.as_str());
    level.insert(
        "blending_data",
        Compound::new().with("old_noise", Tag::Byte(i8::from(old_noise))),
    );
    let mut missing = vec![false; 256];
    let mut flag = status == "noise";
    for z in 0..16 {
        for x in 0..16 {
            let block = container.get(x, 0, z);
            let bedrock =
                block.is_some_and(|state| state.get_str("Name") == Some("minecraft:bedrock"));
            let air = block.is_some_and(|state| state.get_str("Name").unwrap_or("") == AIR);
            if air {
                missing[(z * 16 + x) as usize] = true;
            }
            flag |= bedrock;
        }
    }
    if flag && missing.iter().filter(|set| **set).count() != 256 {
        let target = if status == "full" {
            "heightmaps".to_owned()
        } else {
            status
        };
        level.insert(
            "below_zero_retrogen",
            Compound::new()
                .with("target_status", target)
                .with("missing_bedrock", Tag::LongArray(bit_longs(&missing))),
        );
        level.insert("Status", "empty");
    }
    level.insert("isLightOn", Tag::Byte(0));
}

fn update_chunk_tag(
    level: &mut Compound,
    overworld: bool,
    custom_height: bool,
    noise: bool,
    container: Option<&Packed>,
) {
    level.remove("Biomes");
    if !overworld {
        update_carving_masks(level, 16, 0);
        return;
    }
    if custom_height {
        update_carving_masks(level, 24, 0);
        return;
    }
    update_heightmaps(level);
    for key in ["LiquidsToBeTicked", "PostProcessing", "ToBeTicked"] {
        add_padding_entries(level, key);
    }
    update_carving_masks(level, 24, 4);
    shift_upgrade_data(level);
    if !noise {
        return;
    }
    if let Some(container) = container {
        retrogen(level, container);
    }
}

pub fn height_and_biome_2832(root: &mut Compound) {
    let (overworld, noise) = context(root);
    let Some(level) = compound_mut(root, "Level") else {
        return;
    };
    let offset = if overworld { -4 } else { 0 };
    let mut custom_height = false;
    let containers = biome_containers(level, overworld, offset, &mut custom_height);
    let empty = palette_container(vec![Tag::Compound(Compound::new().with("Name", AIR))]);
    let mut names = HashSet::new();
    let mut seen = HashSet::new();
    let mut at_zero = None;
    let mut sections: Vec<Tag> = match level.remove("Sections") {
        Some(Tag::List(items)) => items,
        _ => Vec::new(),
    };
    for section in sections.iter_mut().filter_map(Tag::as_compound_mut) {
        section_blocks(
            section,
            &empty,
            &containers,
            offset,
            &mut seen,
            &mut names,
            &mut at_zero,
        );
    }
    for (index, container) in containers.iter().enumerate() {
        let y = index as i32 + offset;
        if seen.insert(y) {
            sections.push(Tag::Compound(
                Compound::new()
                    .with("Y", Tag::Byte(y as i8))
                    .with("block_states", empty.clone())
                    .with("biomes", container.clone()),
            ));
        }
    }
    level.insert("Sections", Tag::List(sections));
    if overworld && let Some(status) = level.get_str("Status").map(str::to_owned) {
        level.insert("Status", predicted_status(&status, &mut names));
    }
    update_chunk_tag(level, overworld, custom_height, noise, at_zero.as_ref());
}

fn level_sections(root: &mut Compound) -> impl Iterator<Item = &mut Compound> {
    compound_mut(root, "Level")
        .and_then(|level| list_mut(level, "Sections"))
        .into_iter()
        .flatten()
        .filter_map(Tag::as_compound_mut)
}

fn old_palettes(root: &mut Compound) -> impl Iterator<Item = &mut Compound> {
    level_sections(root)
        .filter_map(|section| list_mut(section, "Palette"))
        .flatten()
        .filter_map(Tag::as_compound_mut)
}

pub fn cauldron_2679(root: &mut Compound) {
    for state in old_palettes(root) {
        if state.get_str("Name") != Some("minecraft:cauldron") {
            continue;
        }
        let empty = Compound::new();
        let level = state
            .get_compound("Properties")
            .unwrap_or(&empty)
            .get_str("level")
            .unwrap_or("0")
            .to_owned();
        if level == "0" {
            state.remove("Properties");
        } else {
            state.insert("Name", "minecraft:water_cauldron");
        }
    }
}

pub fn dirt_path_blocks_2680(root: &mut Compound) {
    for state in old_palettes(root) {
        if state.get_str("Name") == Some("minecraft:grass_path") {
            state.insert("Name", "minecraft:dirt_path");
        }
    }
}

pub fn dirt_path_items_2680(root: &mut Compound) {
    walk_item_stacks(root, &mut |stack| {
        if stack.get_str("id") == Some("minecraft:grass_path") {
            stack.insert("id", "minecraft:dirt_path");
        }
    });
}

fn wrap_spawner(spawner: &mut Compound) {
    if let Some(Tag::Compound(data)) = spawner.remove("SpawnData") {
        spawner.insert("SpawnData", Compound::new().with("entity", data));
    }
    let Some(Tag::List(potentials)) = spawner.remove("SpawnPotentials") else {
        return;
    };
    let wrapped: Vec<Tag> = potentials
        .into_iter()
        .map(|entry| {
            let Tag::Compound(mut old) = entry else {
                return entry;
            };
            let weight = number_or(&old, "Weight", 1) as i32;
            let entity = old.remove("Entity");
            old.remove("Weight");
            let mut data = Compound::new();
            if let Some(entity) = entity {
                data.insert("entity", entity);
            }
            old.insert("data", data);
            old.insert("weight", weight);
            Tag::Compound(old)
        })
        .collect();
    spawner.insert("SpawnPotentials", Tag::List(wrapped));
}

pub fn spawner_data_2831(root: &mut Compound) {
    let Some(level) = compound_mut(root, "Level") else {
        return;
    };
    for (key, id) in [
        ("TileEntities", "minecraft:mob_spawner"),
        ("Entities", "minecraft:spawner_minecart"),
    ] {
        for spawner in list_mut(level, key)
            .into_iter()
            .flatten()
            .filter_map(Tag::as_compound_mut)
            .filter(|entry| entry.get_str("id") == Some(id))
        {
            wrap_spawner(spawner);
        }
    }
}

fn rename_biome_palettes(
    sections: Option<&mut Vec<Tag>>,
    rename: &impl Fn(&str) -> Option<&'static str>,
) {
    for section in sections
        .into_iter()
        .flatten()
        .filter_map(Tag::as_compound_mut)
    {
        let Some(palette) = compound_mut(section, "biomes").and_then(|b| list_mut(b, "palette"))
        else {
            continue;
        };
        for entry in palette {
            if let Tag::String(name) = entry
                && let Some(renamed) = rename(name)
            {
                *name = renamed.to_owned();
            }
        }
    }
}

pub fn caves_and_cliffs_biomes_2838(root: &mut Compound) {
    let sections = compound_mut(root, "Level").and_then(|level| list_mut(level, "Sections"));
    rename_biome_palettes(sections, &|name| {
        CAVES_AND_CLIFFS_BIOMES
            .iter()
            .find(|(from, _)| *from == name)
            .map(|(_, to)| *to)
    });
}

pub fn deep_warm_ocean_2843(root: &mut Compound) {
    rename_biome_palettes(list_mut(root, "sections"), &|name| {
        (name == "minecraft:deep_warm_ocean").then_some("minecraft:warm_ocean")
    });
}

fn get_liquid(state: Option<&Compound>) -> &'static str {
    let Some(state) = state else {
        return "minecraft:empty";
    };
    let name = state.get_str("Name").unwrap_or("");
    let property = |key: &str| {
        state
            .get_compound("Properties")
            .and_then(|properties| properties.get(key))
            .and_then(number)
    };
    match name {
        "minecraft:water" => {
            if property("level").unwrap_or(0) == 0 {
                "minecraft:water"
            } else {
                "minecraft:flowing_water"
            }
        }
        "minecraft:lava" => {
            if property("level").unwrap_or(0) == 0 {
                "minecraft:lava"
            } else {
                "minecraft:flowing_lava"
            }
        }
        _ if ALWAYS_WATERLOGGED.contains(&name) || property("waterlogged").unwrap_or(0) != 0 => {
            "minecraft:water"
        }
        _ => "minecraft:empty",
    }
}

fn get_block(state: Option<&Compound>) -> &str {
    state.map_or(AIR, state_name)
}

fn tick_list(
    level: &Compound,
    containers: &[(i32, Packed)],
    min_section: i32,
    chunk: (i32, i32),
    key: &str,
    name_of: &dyn Fn(Option<&Compound>) -> String,
) -> Tag {
    let mut ticks = Vec::new();
    for (index, entry) in level.get_list(key).unwrap_or_default().iter().enumerate() {
        let section_y = index as i32 + min_section;
        let container = containers
            .iter()
            .find(|(y, _)| *y == section_y)
            .map(|(_, packed)| packed);
        let Tag::List(positions) = entry else {
            continue;
        };
        for position in positions {
            let packed = match position {
                Tag::Short(value) => i32::from(*value),
                other => number(other).map_or(-1, |value| value as i16 as i32),
            };
            if packed <= 0 {
                continue;
            }
            let (x, y, z) = (packed & 15, (packed >> 4) & 15, (packed >> 8) & 15);
            let name = name_of(container.and_then(|packed| packed.get(x, y, z)));
            ticks.push(Tag::Compound(
                Compound::new()
                    .with("i", name)
                    .with("x", chunk.0 * 16 + x)
                    .with("y", section_y * 16 + y)
                    .with("z", chunk.1 * 16 + z)
                    .with("t", 0)
                    .with("p", 0),
            ));
        }
    }
    Tag::List(ticks)
}

pub fn proto_ticks_2841(root: &mut Compound) {
    let Some(level) = compound_mut(root, "Level") else {
        return;
    };
    if let Some(liquid) = level.remove("LiquidTicks") {
        level.insert("fluid_ticks", liquid);
    }
    let mut min_section = 0;
    let mut containers: Vec<(i32, Packed)> = Vec::new();
    for section in level
        .get_list("Sections")
        .unwrap_or_default()
        .iter()
        .filter_map(Tag::as_compound)
    {
        let y = number_or(section, "Y", i64::from(i32::MAX)) as i32;
        if y == i32::MAX {
            continue;
        }
        if let Some(states) = section.get_compound("block_states") {
            min_section = min_section.min(y);
            containers.push((y, Packed::of(states)));
        }
    }
    if level.contains_key("yPos") {
        level.insert("yPos", Tag::Byte(min_section as i8));
    }
    if level.contains_key("TileTicks") || level.contains_key("fluid_ticks") {
        return;
    }
    let chunk = (
        number_or(level, "xPos", 0) as i32,
        number_or(level, "zPos", 0) as i32,
    );
    let liquids = tick_list(
        level,
        &containers,
        min_section,
        chunk,
        "LiquidsToBeTicked",
        &|s| get_liquid(s).to_owned(),
    );
    let blocks = tick_list(level, &containers, min_section, chunk, "ToBeTicked", &|s| {
        get_block(s).to_owned()
    });
    level.insert("TileTicks", blocks);
    level.remove("ToBeTicked");
    level.remove("LiquidsToBeTicked");
    level.insert("fluid_ticks", liquids);
}

pub fn chunk_renames_2842(root: &mut Compound) {
    let Some(Tag::Compound(level)) = root.remove("Level") else {
        return;
    };
    for (key, value) in level.iter() {
        let renamed = match key {
            "TileEntities" => "block_entities",
            "TileTicks" => "block_ticks",
            "Entities" => "entities",
            "Sections" => "sections",
            "Structures" => "structures",
            other => other,
        };
        let mut value = value.clone();
        if key == "Structures"
            && let Tag::Compound(structures) = &mut value
            && let Some(starts) = structures.remove("Starts")
        {
            structures.insert("starts", starts);
        }
        root.insert(renamed, value);
    }
}

fn neighbour_ticks(ticks: &[Tag], chunk: (i32, i32)) -> Vec<Tag> {
    ticks
        .iter()
        .filter(|tick| {
            let Some(tick) = tick.as_compound() else {
                return false;
            };
            let dx = (chunk.0 - (number_or(tick, "x", 0) as i32 >> 4)).abs();
            let dz = (chunk.1 - (number_or(tick, "z", 0) as i32 >> 4)).abs();
            (dx != 0 || dz != 0) && dx <= 1 && dz <= 1
        })
        .cloned()
        .collect()
}

pub fn overreaching_ticks_2843(root: &mut Compound) {
    let chunk = (
        number_or(root, "xPos", 0) as i32,
        number_or(root, "zPos", 0) as i32,
    );
    for (source, target) in [
        ("block_ticks", "neighbor_block_ticks"),
        ("fluid_ticks", "neighbor_fluid_ticks"),
    ] {
        let Some(ticks) = root.get_list(source) else {
            continue;
        };
        let extracted = neighbour_ticks(ticks, chunk);
        if extracted.is_empty() {
            continue;
        }
        let mut upgrade = root
            .get_compound("UpgradeData")
            .cloned()
            .unwrap_or_default();
        upgrade.insert(target, Tag::List(extracted));
        root.insert("UpgradeData", upgrade);
    }
}

#[cfg(test)]
mod tests;
