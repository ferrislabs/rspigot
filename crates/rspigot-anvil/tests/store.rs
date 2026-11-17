mod common;

use std::sync::Arc;

use common::*;
use rspigot_anvil::{AnvilOptions, AnvilStore, Compression, DimensionLayout};
use rspigot_core::ports::{ChunkStore, StoreError};
use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::world::WorldHeight;
use rspigot_domain::{BlockStateId, ChunkPos};

fn region_file(
    root: &std::path::Path,
    dimension: &rspigot_domain::Identifier,
    name: &str,
) -> std::path::PathBuf {
    DimensionLayout::Spigot
        .region_dir(root, dimension)
        .unwrap()
        .join(name)
}

#[test]
fn save_then_load_returns_an_equal_column() {
    let dir = TempDir::new("roundtrip");
    let store = AnvilStore::open(&dir.world(), DimensionLayout::Spigot).unwrap();
    let original = rich_column(ChunkPos::new(-7, 40));
    store.save(&overworld(), &original).unwrap();
    store.flush().unwrap();
    let loaded = store.load(&overworld(), original.pos()).unwrap().unwrap();
    assert_eq!(loaded, original);
    assert_eq!(loaded.extra, original.extra);
    assert_eq!(loaded.light, original.light);
    assert_eq!(loaded.heightmaps, original.heightmaps);
    assert_eq!(loaded.block_entities, original.block_entities);
    for (a, b) in loaded.sections().iter().zip(original.sections()) {
        assert_eq!(
            a.blocks().values().collect::<Vec<_>>(),
            b.blocks().values().collect::<Vec<_>>()
        );
        assert_eq!(
            a.biomes().values().collect::<Vec<_>>(),
            b.biomes().values().collect::<Vec<_>>()
        );
    }
    assert!(!loaded.is_dirty());
}

#[test]
fn exists_checks_the_region_header_only() {
    let dir = TempDir::new("exists");
    let store = AnvilStore::open(&dir.world(), DimensionLayout::Spigot).unwrap();
    let pos = ChunkPos::new(3, -40);
    assert!(!store.exists(&overworld(), pos).unwrap());
    assert!(!dir.world().exists());
    store.save(&overworld(), &simple_column(pos, 1)).unwrap();
    assert!(store.exists(&overworld(), pos).unwrap());
    assert!(!store.exists(&overworld(), ChunkPos::new(4, -40)).unwrap());
    assert!(!store.exists(&nether(), pos).unwrap());
}

#[test]
fn reopening_the_store_reads_what_was_written() {
    let dir = TempDir::new("reopen");
    let original = rich_column(ChunkPos::new(2, 3));
    {
        let store = AnvilStore::open(&dir.world(), DimensionLayout::Spigot).unwrap();
        store.save(&overworld(), &original).unwrap();
    }
    let store = AnvilStore::open(&dir.world(), DimensionLayout::Spigot).unwrap();
    assert_eq!(
        store.load(&overworld(), original.pos()).unwrap().unwrap(),
        original
    );
}

#[test]
fn missing_chunks_and_regions_load_as_none_without_creating_files() {
    let dir = TempDir::new("missing");
    let store = AnvilStore::open(&dir.world(), DimensionLayout::Spigot).unwrap();
    assert!(
        store
            .load(&overworld(), ChunkPos::new(0, 0))
            .unwrap()
            .is_none()
    );
    assert!(!dir.world().exists());
    store
        .save(&overworld(), &simple_column(ChunkPos::new(0, 0), 1))
        .unwrap();
    assert!(
        store
            .load(&overworld(), ChunkPos::new(1, 0))
            .unwrap()
            .is_none()
    );
    assert!(
        store
            .load(&overworld(), ChunkPos::new(100, 100))
            .unwrap()
            .is_none()
    );
}

#[test]
fn dimensions_use_the_spigot_folders() {
    let dir = TempDir::new("layout");
    let root = dir.world();
    let store = AnvilStore::open(&root, DimensionLayout::Spigot).unwrap();
    let pos = ChunkPos::new(33, -1);
    store.save(&overworld(), &simple_column(pos, 1)).unwrap();
    store
        .save(&nether(), &simple_column_in(pos, WorldHeight::NETHER, 2))
        .unwrap();
    store
        .save(&end(), &simple_column_in(pos, WorldHeight::END, 3))
        .unwrap();
    let name = "r.1.-1.mca";
    assert!(
        root.join("dimensions/minecraft/overworld/region")
            .join(name)
            .is_file()
    );
    assert!(
        dir.path()
            .join("world_nether/dimensions/minecraft/the_nether/region")
            .join(name)
            .is_file()
    );
    assert!(
        dir.path()
            .join("world_the_end/dimensions/minecraft/the_end/region")
            .join(name)
            .is_file()
    );
    let from_overworld = store.load(&overworld(), pos).unwrap().unwrap();
    let from_nether = store.load(&nether(), pos).unwrap().unwrap();
    let from_end = store.load(&end(), pos).unwrap().unwrap();
    assert_eq!(from_overworld.height(), WorldHeight::OVERWORLD);
    assert_eq!(from_nether, simple_column_in(pos, WorldHeight::NETHER, 2));
    assert_eq!(from_end, simple_column_in(pos, WorldHeight::END, 3));
    assert_eq!(from_overworld, simple_column(pos, 1));
}

#[test]
fn legacy_vanilla_layout_reads_dim_folders() {
    let dir = TempDir::new("legacy");
    let root = dir.world();
    let store = AnvilStore::open(&root, DimensionLayout::LegacyVanilla).unwrap();
    store
        .save(
            &nether(),
            &simple_column_in(ChunkPos::new(0, 0), WorldHeight::NETHER, 1),
        )
        .unwrap();
    assert!(root.join("DIM-1/region/r.0.0.mca").is_file());
}

#[test]
fn overwriting_with_a_bigger_chunk_relocates_the_sectors() {
    let dir = TempDir::new("relocate");
    let store = AnvilStore::open(&dir.world(), DimensionLayout::Spigot).unwrap();
    let small_pos = ChunkPos::new(0, 0);
    let other_pos = ChunkPos::new(1, 0);
    let path = region_file(&dir.world(), &overworld(), "r.0.0.mca");
    store
        .save(&overworld(), &simple_column(small_pos, 0))
        .unwrap();
    store
        .save(&overworld(), &simple_column(other_pos, 0))
        .unwrap();
    let before = region_entries(&path);
    let small = before.iter().find(|e| e.0 == 0).copied().unwrap();
    let other = before.iter().find(|e| e.0 == 1).copied().unwrap();
    assert_eq!(small.1, 2);
    assert_eq!(other.1, 2 + small.2);

    let pool: Vec<BlockStateId> = (0..64).map(|i| BlockStateId(i + 1)).collect();
    let bigger = heavy_column(small_pos, 7, &pool);
    store.save(&overworld(), &bigger).unwrap();
    let after = region_entries(&path);
    let moved = after.iter().find(|e| e.0 == 0).copied().unwrap();
    assert!(moved.2 > small.2);
    assert!(moved.1 >= other.1 + other.2);
    assert_no_overlap(&path);
    assert_eq!(
        store.load(&overworld(), small_pos).unwrap().unwrap(),
        bigger
    );

    store
        .save(&overworld(), &simple_column(ChunkPos::new(2, 0), 0))
        .unwrap();
    let reused = region_entries(&path)
        .into_iter()
        .find(|e| e.0 == 2)
        .unwrap();
    assert_eq!(reused.1, small.1);
    assert_no_overlap(&path);
    assert!(store.load(&overworld(), other_pos).unwrap().is_some());
}

#[test]
fn concurrent_saves_in_shared_and_separate_regions() {
    let dir = TempDir::new("concurrent");
    let store = Arc::new(AnvilStore::open(&dir.world(), DimensionLayout::Spigot).unwrap());
    let pool: Vec<BlockStateId> = (0..300).map(|i| BlockStateId(i + 1)).collect();
    let pool = Arc::new(pool);
    let threads: Vec<_> = (0..8u64)
        .map(|thread| {
            let store = Arc::clone(&store);
            let pool = Arc::clone(&pool);
            std::thread::spawn(move || {
                let mut written = Vec::new();
                for round in 0..6u64 {
                    for slot in 0..6i32 {
                        let shared = ChunkPos::new(slot, 0);
                        let own = ChunkPos::new(64 * (thread as i32 + 1) + slot, 5);
                        let column = if round % 2 == 0 {
                            simple_column(own, (round as usize % 3) + 1)
                        } else {
                            heavy_column(own, thread * 100 + round, &pool)
                        };
                        store.save(&overworld(), &column).unwrap();
                        written.push(column);
                        let contested = simple_column(shared, ((thread + round) % 4) as usize);
                        store.save(&overworld(), &contested).unwrap();
                        assert!(store.load(&overworld(), shared).unwrap().is_some());
                    }
                }
                let mut latest = std::collections::HashMap::new();
                for column in written {
                    latest.insert(column.pos(), column);
                }
                latest
            })
        })
        .collect();
    let mut expected: Vec<ChunkColumn> = Vec::new();
    for handle in threads {
        expected.extend(handle.join().unwrap().into_values());
    }
    store.flush().unwrap();
    for column in &expected {
        assert_eq!(
            &store.load(&overworld(), column.pos()).unwrap().unwrap(),
            column
        );
    }
    for slot in 0..6 {
        assert!(
            store
                .load(&overworld(), ChunkPos::new(slot, 0))
                .unwrap()
                .is_some()
        );
    }
    let shared_region = region_file(&dir.world(), &overworld(), "r.0.0.mca");
    assert_no_overlap(&shared_region);
    for thread in 1..=8 {
        let name = format!("r.{}.0.mca", (64 * thread) >> 5);
        assert_no_overlap(&region_file(&dir.world(), &overworld(), &name));
    }
}

#[test]
fn oversized_chunks_use_an_external_file() {
    let dir = TempDir::new("oversized");
    let store = AnvilStore::open(&dir.world(), DimensionLayout::Spigot).unwrap();
    let pool: Vec<BlockStateId> = (0..rspigot_blocks::STATE_COUNT as u16)
        .map(BlockStateId)
        .collect();
    let pos = ChunkPos::new(-33, 5);
    let big = heavy_column(pos, 99, &pool);
    store.save(&overworld(), &big).unwrap();
    let region_dir = DimensionLayout::Spigot
        .region_dir(&dir.world(), &overworld())
        .unwrap();
    let mcc = region_dir.join("c.-33.5.mcc");
    assert!(mcc.is_file());
    assert!(std::fs::metadata(&mcc).unwrap().len() > 1024 * 1024);
    let entries = region_entries(&region_dir.join("r.-2.0.mca"));
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].2, 1);
    let bytes = std::fs::read(region_dir.join("r.-2.0.mca")).unwrap();
    assert_eq!(bytes[8196] & 0x80, 0x80);
    assert_eq!(store.load(&overworld(), pos).unwrap().unwrap(), big);

    let reopened = AnvilStore::open(&dir.world(), DimensionLayout::Spigot).unwrap();
    assert_eq!(reopened.load(&overworld(), pos).unwrap().unwrap(), big);

    store.save(&overworld(), &simple_column(pos, 1)).unwrap();
    assert!(!mcc.exists());
    assert_eq!(
        store.load(&overworld(), pos).unwrap().unwrap(),
        simple_column(pos, 1)
    );
    let leftovers: Vec<_> = std::fs::read_dir(&region_dir)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
        .collect();
    assert!(leftovers.is_empty());
}

#[test]
fn every_compression_round_trips_and_mixes() {
    let dir = TempDir::new("compression");
    let pos = |i| ChunkPos::new(i, 0);
    let all = [
        Compression::Gzip,
        Compression::Zlib,
        Compression::Uncompressed,
        Compression::Lz4,
    ];
    for (i, compression) in all.into_iter().enumerate() {
        let options = AnvilOptions {
            compression,
            ..AnvilOptions::default()
        };
        let store = AnvilStore::open_with(&dir.world(), DimensionLayout::Spigot, options).unwrap();
        store
            .save(&overworld(), &rich_column(pos(i as i32)))
            .unwrap();
    }
    let store = AnvilStore::open(&dir.world(), DimensionLayout::Spigot).unwrap();
    for i in 0..4 {
        assert_eq!(
            store.load(&overworld(), pos(i)).unwrap().unwrap(),
            rich_column(pos(i))
        );
    }
    let path = region_file(&dir.world(), &overworld(), "r.0.0.mca");
    let bytes = std::fs::read(&path).unwrap();
    let ids: Vec<u8> = region_entries(&path)
        .iter()
        .map(|(_, sector, _)| bytes[*sector as usize * 4096 + 4])
        .collect();
    assert_eq!(ids, vec![1, 2, 3, 4]);
}

#[test]
fn the_open_region_cache_is_bounded() {
    let dir = TempDir::new("lru");
    let options = AnvilOptions {
        max_open_regions: 3,
        ..AnvilOptions::default()
    };
    let store = AnvilStore::open_with(&dir.world(), DimensionLayout::Spigot, options).unwrap();
    for region in 0..10 {
        store
            .save(
                &overworld(),
                &simple_column(ChunkPos::new(region * 32, 0), 1),
            )
            .unwrap();
        assert!(store.cached_regions() <= 3);
    }
    for region in 0..10 {
        let pos = ChunkPos::new(region * 32, 0);
        assert_eq!(
            store.load(&overworld(), pos).unwrap().unwrap(),
            simple_column(pos, 1)
        );
        assert!(store.cached_regions() <= 3);
    }
}

#[test]
fn many_threads_over_a_tiny_cache_stay_consistent() {
    let dir = TempDir::new("lru-threads");
    let options = AnvilOptions {
        max_open_regions: 2,
        ..AnvilOptions::default()
    };
    let store =
        Arc::new(AnvilStore::open_with(&dir.world(), DimensionLayout::Spigot, options).unwrap());
    let threads: Vec<_> = (0..8)
        .map(|thread| {
            let store = Arc::clone(&store);
            std::thread::spawn(move || {
                for i in 0..40 {
                    let pos = ChunkPos::new(((thread + i) % 6) * 32 + thread, i % 3);
                    let column = simple_column(pos, 1);
                    store.save(&overworld(), &column).unwrap();
                    let loaded = store.load(&overworld(), pos).unwrap().unwrap();
                    assert_eq!(loaded.pos(), pos);
                }
            })
        })
        .collect();
    for handle in threads {
        handle.join().unwrap();
    }
    for region in 0..6 {
        let path = region_file(&dir.world(), &overworld(), &format!("r.{region}.0.mca"));
        if path.exists() {
            assert_no_overlap(&path);
        }
    }
}

#[test]
fn corrupt_compressed_data_is_reported_with_the_position() {
    let dir = TempDir::new("corrupt");
    let store = AnvilStore::open(&dir.world(), DimensionLayout::Spigot).unwrap();
    let pos = ChunkPos::new(4, 4);
    store.save(&overworld(), &simple_column(pos, 1)).unwrap();
    let path = region_file(&dir.world(), &overworld(), "r.0.0.mca");
    let mut bytes = std::fs::read(&path).unwrap();
    let sector = region_entries(&path)[0].1 as usize;
    for byte in &mut bytes[sector * 4096 + 9..sector * 4096 + 40] {
        *byte ^= 0xFF;
    }
    std::fs::write(&path, &bytes).unwrap();
    let fresh = AnvilStore::open(&dir.world(), DimensionLayout::Spigot).unwrap();
    match fresh.load(&overworld(), pos) {
        Err(StoreError::Corrupt { pos: reported, .. }) => assert_eq!(reported, pos),
        other => panic!("{other:?}"),
    }
}

#[test]
fn flush_on_a_fresh_store_succeeds() {
    let dir = TempDir::new("flush");
    let store = AnvilStore::open(&dir.world(), DimensionLayout::Spigot).unwrap();
    store.flush().unwrap();
}

#[test]
fn unnamed_world_roots_are_rejected_for_spigot_layouts() {
    assert!(AnvilStore::open(std::path::Path::new("/"), DimensionLayout::Spigot).is_err());
    assert!(AnvilStore::open(std::path::Path::new("/"), DimensionLayout::Vanilla).is_ok());
}

#[test]
fn chunks_with_the_wrong_height_for_their_dimension_are_refused() {
    let dir = TempDir::new("height");
    let store = AnvilStore::open(&dir.world(), DimensionLayout::Spigot).unwrap();
    let nether_column = simple_column_in(ChunkPos::new(0, 0), WorldHeight::NETHER, 1);
    assert!(matches!(
        store.save(&overworld(), &nether_column),
        Err(StoreError::Corrupt { .. })
    ));
    assert!(!dir.world().exists());
    let custom = rspigot_domain::Identifier::parse("pack:deep").unwrap();
    let tall = WorldHeight::new(-128, 512).unwrap();
    let store = store.with_dimension_height(custom.clone(), tall);
    let column = simple_column_in(ChunkPos::new(1, 1), tall, 2);
    store.save(&custom, &column).unwrap();
    assert_eq!(store.load(&custom, column.pos()).unwrap().unwrap(), column);
}

#[test]
fn erasing_a_dimension_removes_its_files_and_forgets_its_open_regions() {
    let dir = TempDir::new("erase");
    let store = AnvilStore::open(&dir.world(), DimensionLayout::Spigot).unwrap();
    let arena = rspigot_domain::Identifier::parse("rspigot:arena").unwrap();
    let pos = ChunkPos::new(2, 2);
    store.save(&arena, &simple_column(pos, 1)).unwrap();
    store.save(&overworld(), &simple_column(pos, 1)).unwrap();
    store.flush().unwrap();
    let folder = DimensionLayout::Spigot
        .dimension_dir(&dir.world(), &arena)
        .unwrap();
    assert!(folder.exists());
    store.erase_dimension(&arena).unwrap();
    assert!(!folder.exists());
    assert!(!store.exists(&arena, pos).unwrap());
    assert!(store.exists(&overworld(), pos).unwrap());
    store.save(&arena, &simple_column(pos, 2)).unwrap();
    store.flush().unwrap();
    assert!(store.exists(&arena, pos).unwrap());
}
