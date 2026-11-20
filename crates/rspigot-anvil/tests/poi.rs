mod common;

use std::io::Write;

use common::*;
use flate2::write::ZlibEncoder;
use rspigot_anvil::{AnvilStore, DimensionLayout};
use rspigot_core::ports::EntityStore;
use rspigot_domain::poi::{PoiChunkData, PoiRecord, PoiSectionData, PoiType};
use rspigot_domain::{BlockPos, ChunkPos, Identifier};
use rspigot_nbt::{Compound, Tag, write_named};

fn sample() -> PoiChunkData {
    PoiChunkData {
        sections: vec![
            PoiSectionData {
                section_y: 4,
                valid: true,
                records: vec![
                    PoiRecord::with_free_tickets(BlockPos::new(35, 64, -3), PoiType::Home, 0),
                    PoiRecord::new(BlockPos::new(36, 65, -3), PoiType::Meeting),
                ],
            },
            PoiSectionData {
                section_y: -1,
                valid: true,
                records: vec![PoiRecord::new(
                    BlockPos::new(33, -9, -4),
                    PoiType::Lodestone,
                )],
            },
        ],
    }
}

#[test]
fn poi_records_round_trip_through_the_store_and_a_reopen() {
    let dir = TempDir::new("poi-roundtrip");
    let pos = ChunkPos::new(2, -1);
    let mut expected = sample();
    expected.sections.sort_by_key(|section| section.section_y);
    {
        let store = AnvilStore::open(&dir.world(), DimensionLayout::Spigot).unwrap();
        assert!(store.load_poi(&overworld(), pos).unwrap().is_none());
        store.save_poi(&overworld(), pos, &sample()).unwrap();
        assert_eq!(
            store.load_poi(&overworld(), pos).unwrap(),
            Some(expected.clone())
        );
    }
    let store = AnvilStore::open(&dir.world(), DimensionLayout::Spigot).unwrap();
    assert_eq!(store.load_poi(&overworld(), pos).unwrap(), Some(expected));
    let file = DimensionLayout::Spigot
        .poi_dir(&dir.world(), &overworld())
        .unwrap()
        .join("r.0.-1.mca");
    assert!(file.exists());
}

#[test]
fn saving_an_empty_poi_chunk_removes_the_entry() {
    let dir = TempDir::new("poi-empty");
    let store = AnvilStore::open(&dir.world(), DimensionLayout::Spigot).unwrap();
    let pos = ChunkPos::new(0, 0);
    store.save_poi(&overworld(), pos, &sample()).unwrap();
    store
        .save_poi(&overworld(), pos, &PoiChunkData::default())
        .unwrap();
    assert!(store.load_poi(&overworld(), pos).unwrap().is_none());
}

#[test]
fn poi_does_not_leak_into_the_region_or_entity_stores() {
    let dir = TempDir::new("poi-isolated");
    let store = AnvilStore::open(&dir.world(), DimensionLayout::Spigot).unwrap();
    let pos = ChunkPos::new(1, 1);
    store.save_poi(&overworld(), pos, &sample()).unwrap();
    assert!(store.load_entities(&overworld(), pos).unwrap().is_none());
}

fn dimension_folder_case(layout: DimensionLayout, dimension: &Identifier, label: &str) {
    let dir = TempDir::new(label);
    let store = AnvilStore::open(&dir.world(), layout).unwrap();
    let pos = ChunkPos::new(-33, 40);
    store.save_poi(dimension, pos, &sample()).unwrap();
    let folder = layout.poi_dir(&dir.world(), dimension).unwrap();
    assert!(folder.join("r.-2.1.mca").exists(), "{folder:?}");
    assert_eq!(
        store
            .load_poi(dimension, pos)
            .unwrap()
            .map(|d| d.sections.len()),
        Some(2)
    );
    assert!(store.load_poi(&overworld(), pos).unwrap().is_none() || *dimension == overworld());
}

#[test]
fn nether_and_end_poi_use_their_dimension_folders() {
    dimension_folder_case(DimensionLayout::Spigot, &nether(), "poi-nether");
    dimension_folder_case(DimensionLayout::Spigot, &end(), "poi-end");
    dimension_folder_case(DimensionLayout::LegacyVanilla, &nether(), "poi-dim-1");
    dimension_folder_case(DimensionLayout::LegacyVanilla, &end(), "poi-dim1");
    dimension_folder_case(DimensionLayout::Vanilla, &nether(), "poi-vanilla");
}

fn region_with(chunks: &[(ChunkPos, Compound)]) -> Vec<u8> {
    let mut header = vec![0u8; 8192];
    let mut body: Vec<u8> = Vec::new();
    let mut next_sector = 2u32;
    for (pos, nbt) in chunks {
        let mut raw = Vec::new();
        write_named(&mut raw, "", nbt);
        let mut encoder = ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(&raw).unwrap();
        let packed = encoder.finish().unwrap();
        let mut record = Vec::new();
        record.extend_from_slice(&(packed.len() as u32 + 1).to_be_bytes());
        record.push(2);
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

fn vanilla_record(pos: [i32; 3], kind: &str, free: Option<i32>) -> Tag {
    let mut record = Compound::new()
        .with("pos", Tag::IntArray(pos.to_vec()))
        .with("type", kind);
    if let Some(free) = free {
        record.insert("free_tickets", free);
    }
    Tag::Compound(record)
}

#[test]
fn a_vanilla_shaped_poi_region_file_loads() {
    let dir = TempDir::new("poi-vanilla");
    let pos = ChunkPos::new(2, 0);
    let nbt = Compound::new().with("DataVersion", 4671).with(
        "Sections",
        Compound::new()
            .with(
                "4",
                Compound::new().with("Valid", 1i8).with(
                    "Records",
                    Tag::List(vec![
                        vanilla_record([34, 64, 3], "minecraft:home", Some(0)),
                        vanilla_record([40, 70, 5], "minecraft:meeting", Some(31)),
                    ]),
                ),
            )
            .with(
                "5",
                Compound::new().with("Valid", 0i8).with(
                    "Records",
                    Tag::List(vec![vanilla_record([35, 80, 1], "minecraft:armorer", None)]),
                ),
            ),
    );
    let folder = DimensionLayout::Spigot
        .poi_dir(&dir.world(), &overworld())
        .unwrap();
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("r.0.0.mca"), region_with(&[(pos, nbt)])).unwrap();
    let store = AnvilStore::open(&dir.world(), DimensionLayout::Spigot).unwrap();
    let data = store.load_poi(&overworld(), pos).unwrap().unwrap();
    assert_eq!(data.sections.len(), 2);
    let home = data.section(4).unwrap();
    assert!(home.valid);
    assert_eq!(
        home.records,
        vec![
            PoiRecord::with_free_tickets(BlockPos::new(34, 64, 3), PoiType::Home, 0),
            PoiRecord::with_free_tickets(BlockPos::new(40, 70, 5), PoiType::Meeting, 31),
        ]
    );
    let stale = data.section(5).unwrap();
    assert!(!stale.valid);
    assert_eq!(
        stale.records,
        vec![PoiRecord::new(BlockPos::new(35, 80, 1), PoiType::Armorer)]
    );
    assert!(
        store
            .load_poi(&overworld(), ChunkPos::new(3, 0))
            .unwrap()
            .is_none()
    );
}
