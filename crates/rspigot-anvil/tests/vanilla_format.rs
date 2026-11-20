mod common;

use std::io::Write;

use common::*;
use flate2::write::{GzEncoder, ZlibEncoder};
use rspigot_anvil::{AnvilStore, DimensionLayout};
use rspigot_core::ports::{ChunkStore, StoreError};
use rspigot_domain::chunk::ChunkStatus;
use rspigot_domain::{BlockPos, ChunkPos};
use rspigot_nbt::{Compound, Tag, write_named};

fn pack(indices: &[usize], bits: usize) -> Vec<i64> {
    let per_long = 64 / bits;
    let mut out = vec![0i64; indices.len().div_ceil(per_long)];
    for (i, value) in indices.iter().enumerate() {
        out[i / per_long] |= (*value as i64) << ((i % per_long) * bits);
    }
    out
}

fn palette_entry(name: &str) -> Tag {
    Tag::Compound(Compound::new().with("Name", name))
}

fn chunk_nbt(pos: ChunkPos, data_version: i32) -> Compound {
    let mut sections = Vec::new();
    for y in -5..=24i8 {
        let mut section = Compound::new();
        if (-4..=19).contains(&y) {
            let states = if y == -4 {
                let indices: Vec<usize> = (0..4096).map(|i| usize::from(i >= 2048)).collect();
                Compound::new()
                    .with(
                        "palette",
                        Tag::List(vec![
                            palette_entry("minecraft:bedrock"),
                            palette_entry("minecraft:stone"),
                        ]),
                    )
                    .with("data", pack(&indices, 4))
            } else {
                Compound::new().with("palette", Tag::List(vec![palette_entry("minecraft:air")]))
            };
            section.insert("block_states", states);
            section.insert(
                "biomes",
                Compound::new().with(
                    "palette",
                    Tag::List(vec![Tag::String("minecraft:desert".into())]),
                ),
            );
        }
        if y == 24 || y == -5 {
            section.insert("SkyLight", Tag::ByteArray(vec![0xFF; 2048]));
        }
        section.insert("Y", y);
        sections.push(Tag::Compound(section));
    }
    Compound::new()
        .with("DataVersion", data_version)
        .with("xPos", pos.x)
        .with("yPos", -4)
        .with("zPos", pos.z)
        .with("LastUpdate", 5i64)
        .with("InhabitedTime", 99i64)
        .with("Status", "minecraft:full")
        .with("sections", Tag::List(sections))
        .with("isLightOn", true)
        .with(
            "block_entities",
            Tag::List(vec![Tag::Compound(
                Compound::new()
                    .with("id", "minecraft:sign")
                    .with("x", pos.x * 16 + 1)
                    .with("y", 64)
                    .with("z", pos.z * 16 + 2)
                    .with("is_waxed", false),
            )]),
        )
        .with(
            "structures",
            Compound::new().with("starts", Compound::new()),
        )
        .with(
            "Heightmaps",
            Compound::new().with("WORLD_SURFACE", pack(&vec![65usize; 256], 9)),
        )
}

fn compress(id: u8, raw: &[u8]) -> Vec<u8> {
    match id {
        1 => {
            let mut encoder = GzEncoder::new(Vec::new(), flate2::Compression::default());
            encoder.write_all(raw).unwrap();
            encoder.finish().unwrap()
        }
        2 => {
            let mut encoder = ZlibEncoder::new(Vec::new(), flate2::Compression::default());
            encoder.write_all(raw).unwrap();
            encoder.finish().unwrap()
        }
        _ => raw.to_vec(),
    }
}

fn region_with(chunks: &[(ChunkPos, u8, Compound)]) -> Vec<u8> {
    let mut header = vec![0u8; 8192];
    let mut body: Vec<u8> = Vec::new();
    let mut next_sector = 2u32;
    for (pos, id, nbt) in chunks {
        let mut raw = Vec::new();
        write_named(&mut raw, "", nbt);
        let packed = compress(*id, &raw);
        let mut record = Vec::new();
        record.extend_from_slice(&(packed.len() as u32 + 1).to_be_bytes());
        record.push(*id);
        record.extend_from_slice(&packed);
        let sectors = record.len().div_ceil(4096);
        record.resize(sectors * 4096, 0);
        let index = (pos.x & 31) as usize + (pos.z & 31) as usize * 32;
        header[index * 4..index * 4 + 4]
            .copy_from_slice(&((next_sector << 8) | sectors as u32).to_be_bytes());
        header[4096 + index * 4..4096 + index * 4 + 4]
            .copy_from_slice(&1_700_000_000u32.to_be_bytes());
        body.extend_from_slice(&record);
        next_sector += sectors as u32;
    }
    header.extend_from_slice(&body);
    header
}

fn install(
    world: &std::path::Path,
    dimension: &rspigot_domain::Identifier,
    name: &str,
    bytes: &[u8],
) {
    let dir = DimensionLayout::Spigot
        .region_dir(world, dimension)
        .unwrap();
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(name), bytes).unwrap();
}

#[test]
fn a_hand_built_26_3_world_loads() {
    let dir = TempDir::new("vanilla");
    let world = dir.world();
    let first = ChunkPos::new(0, 0);
    let second = ChunkPos::new(31, 31);
    let third = ChunkPos::new(5, 6);
    let fourth = ChunkPos::new(6, 6);
    let bytes = region_with(&[
        (first, 2, chunk_nbt(first, 5023)),
        (second, 1, chunk_nbt(second, 5023)),
        (third, 3, chunk_nbt(third, 5023)),
        (fourth, 2, chunk_nbt(fourth, 4325)),
    ]);
    install(&world, &overworld(), "r.0.0.mca", &bytes);
    let store = AnvilStore::open(&world, DimensionLayout::Spigot).unwrap();
    for pos in [first, second, third, fourth] {
        let chunk = store.load(&overworld(), pos).unwrap().unwrap();
        assert_eq!(chunk.pos(), pos);
        assert_eq!(chunk.status, ChunkStatus::Full);
        assert_eq!(chunk.inhabited_time, 99);
        assert_eq!(chunk.sections().len(), 24);
        let bedrock = block("minecraft:bedrock");
        let stone = block("minecraft:stone");
        assert_eq!(
            chunk.block(BlockPos::new(pos.x * 16, -64, pos.z * 16)),
            Some(bedrock)
        );
        assert_eq!(
            chunk.block(BlockPos::new(pos.x * 16 + 15, -64 + 15, pos.z * 16 + 15)),
            Some(stone)
        );
        assert_eq!(
            chunk.block(BlockPos::new(pos.x * 16, -48, pos.z * 16)),
            Some(rspigot_domain::BlockStateId::AIR)
        );
        assert_eq!(
            chunk.sections()[0].biome(0, 0, 0),
            biome("minecraft:desert")
        );
        assert_eq!(chunk.block_entities.len(), 1);
        assert_eq!(chunk.block_entities[0].kind.as_str(), "minecraft:sign");
        assert_eq!(
            chunk.block_entities[0].pos,
            BlockPos::new(pos.x * 16 + 1, 64, pos.z * 16 + 2)
        );
        assert_eq!(chunk.heightmaps.maps.len(), 1);
        assert!(chunk.extra.contains_key("structures"));
        assert_eq!(chunk.light.sky[0].is_some(), pos != fourth);
        assert!(chunk.light.sky[25].is_none());
        assert_eq!(chunk.is_dirty(), pos == fourth);
        assert_eq!(chunk.light.sky.len(), 26);
    }
    assert!(
        store
            .load(&overworld(), ChunkPos::new(1, 1))
            .unwrap()
            .is_none()
    );
}

#[test]
fn rspigot_rewrites_loaded_chunks_without_losing_unknown_fields() {
    let dir = TempDir::new("rewrite");
    let world = dir.world();
    let pos = ChunkPos::new(-1, -1);
    let mut nbt = chunk_nbt(pos, 5023);
    nbt.insert(
        "ChunkBukkitValues",
        Compound::new().with("plugin:key", "value"),
    );
    nbt.insert("blending_data", Compound::new().with("min_section", -4));
    nbt.insert("future_field", Tag::LongArray(vec![1, 2, 3]));
    install(
        &world,
        &overworld(),
        "r.-1.-1.mca",
        &region_with(&[(pos, 2, nbt)]),
    );
    let store = AnvilStore::open(&world, DimensionLayout::Spigot).unwrap();
    let loaded = store.load(&overworld(), pos).unwrap().unwrap();
    store.save(&overworld(), &loaded).unwrap();
    let again = AnvilStore::open(&world, DimensionLayout::Spigot)
        .unwrap()
        .load(&overworld(), pos)
        .unwrap()
        .unwrap();
    assert_eq!(again, loaded);
    assert_eq!(
        again.extra.get_compound("ChunkBukkitValues"),
        Some(&Compound::new().with("plugin:key", "value"))
    );
    assert_eq!(
        again.extra.get("future_field"),
        Some(&Tag::LongArray(vec![1, 2, 3]))
    );
    assert!(again.extra.contains_key("blending_data"));
}

#[test]
fn chunks_from_a_newer_version_are_corrupt() {
    let dir = TempDir::new("newer");
    let world = dir.world();
    let pos = ChunkPos::new(2, 2);
    install(
        &world,
        &overworld(),
        "r.0.0.mca",
        &region_with(&[(pos, 2, chunk_nbt(pos, 5024))]),
    );
    let store = AnvilStore::open(&world, DimensionLayout::Spigot).unwrap();
    match store.load(&overworld(), pos) {
        Err(StoreError::Corrupt {
            pos: reported,
            reason,
        }) => {
            assert_eq!(reported, pos);
            assert!(reason.contains("5024"), "{reason}");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn unknown_compression_ids_are_corrupt() {
    let dir = TempDir::new("unknown-compression");
    let world = dir.world();
    let pos = ChunkPos::new(0, 3);
    let mut bytes = region_with(&[(pos, 3, chunk_nbt(pos, 5023))]);
    bytes[8196] = 127;
    install(&world, &overworld(), "r.0.0.mca", &bytes);
    let store = AnvilStore::open(&world, DimensionLayout::Spigot).unwrap();
    assert!(matches!(
        store.load(&overworld(), pos),
        Err(StoreError::Corrupt { .. })
    ));
}

#[test]
fn old_entity_files_are_upgraded_on_load_and_pre_1_18_ones_are_refused() {
    use rspigot_core::ports::EntityStore;
    let dir = TempDir::new("old-entities");
    let world = dir.world();
    let current = ChunkPos::new(0, 0);
    let ancient = ChunkPos::new(1, 0);
    let zombie = Compound::new()
        .with("id", "minecraft:zombie")
        .with(
            "HandItems",
            Tag::List(vec![
                Tag::Compound(
                    Compound::new()
                        .with("id", "minecraft:iron_sword")
                        .with("count", 1),
                ),
                Tag::Compound(Compound::new()),
            ]),
        )
        .with("CustomName", "{\"text\":\"Bob\"}");
    let file = |pos: ChunkPos, version: i32| {
        Compound::new()
            .with("DataVersion", version)
            .with("Entities", Tag::List(vec![Tag::Compound(zombie.clone())]))
            .with("Position", Tag::IntArray(vec![pos.x, pos.z]))
    };
    let dir_path = DimensionLayout::Spigot
        .entities_dir(&world, &overworld())
        .unwrap();
    std::fs::create_dir_all(&dir_path).unwrap();
    std::fs::write(
        dir_path.join("r.0.0.mca"),
        region_with(&[
            (current, 2, file(current, 4189)),
            (ancient, 2, file(ancient, 2586)),
        ]),
    )
    .unwrap();
    let store = AnvilStore::open(&world, DimensionLayout::Spigot).unwrap();
    let loaded = store.load_entities(&overworld(), current).unwrap().unwrap();
    let equipment = loaded[0].get_compound("equipment").unwrap();
    assert_eq!(
        equipment.get_compound("mainhand").unwrap().get_str("id"),
        Some("minecraft:iron_sword")
    );
    assert_eq!(
        loaded[0]
            .get_compound("CustomName")
            .unwrap()
            .get_str("text"),
        Some("Bob")
    );
    let error = store
        .load_entities(&overworld(), ancient)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains(
            "older than the oldest supported version 2860; open it with a vanilla server first"
        ),
        "{error}"
    );
}
