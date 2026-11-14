mod common;

use common::*;
use rspigot_anvil::{AnvilStore, Compression, DimensionLayout};
use rspigot_core::ports::{ChunkStore, EntityStore};
use rspigot_domain::ChunkPos;
use rspigot_nbt::{Compound, Tag, read_named, write_named};

fn entity(id: &str, uuid: [i32; 4], x: f64) -> Compound {
    Compound::new()
        .with("id", id)
        .with(
            "Pos",
            Tag::List(vec![Tag::Double(x), Tag::Double(64.0), Tag::Double(-3.5)]),
        )
        .with("UUID", Tag::IntArray(uuid.to_vec()))
        .with("Custom", Tag::LongArray(vec![1, i64::MIN]))
}

fn raw_entity_chunk(root: &std::path::Path, pos: ChunkPos) -> Option<Vec<u8>> {
    let path = DimensionLayout::Spigot
        .entities_dir(root, &overworld())
        .unwrap()
        .join(format!("r.{}.{}.mca", pos.x >> 5, pos.z >> 5));
    let bytes = std::fs::read(path).ok()?;
    let index = ((pos.x & 31) + (pos.z & 31) * 32) as usize * 4;
    let entry = u32::from_be_bytes(bytes[index..index + 4].try_into().ok()?);
    if entry == 0 {
        return None;
    }
    let offset = (entry >> 8) as usize * 4096;
    let length = u32::from_be_bytes(bytes[offset..offset + 4].try_into().ok()?) as usize;
    let compression = Compression::from_id(bytes[offset + 4])?;
    compression
        .decompress(&bytes[offset + 5..offset + 4 + length])
        .ok()
}

#[test]
fn entity_chunks_live_next_to_region_and_keep_the_vanilla_root() {
    let dir = TempDir::new("entities");
    let store = AnvilStore::open(&dir.world(), DimensionLayout::Spigot).unwrap();
    let pos = ChunkPos::new(-3, 7);
    let cow = entity("minecraft:cow", [1, 2, 3, 4], -40.5);
    let stand = entity("minecraft:armor_stand", [5, 6, 7, 8], -41.5);
    store
        .save_entities(&overworld(), pos, &[cow.clone(), stand.clone()])
        .unwrap();
    store.flush().unwrap();
    let bytes = raw_entity_chunk(&dir.world(), pos).expect("entity chunk written");
    let mut input = bytes.as_slice();
    let (name, root) = read_named(&mut input).unwrap();
    assert!(name.is_empty());
    let keys: Vec<&str> = root.iter().map(|(k, _)| k).collect();
    assert_eq!(keys, vec!["DataVersion", "Entities", "Position"]);
    assert_eq!(root.get_i32("DataVersion"), Some(5023));
    assert_eq!(root.get_int_array("Position"), Some(&[-3, 7][..]));
    let loaded = store.load_entities(&overworld(), pos).unwrap().unwrap();
    assert_eq!(loaded, vec![cow.clone(), stand.clone()]);
    let mut first = Vec::new();
    write_named(&mut first, "", &stand);
    let mut second = Vec::new();
    write_named(&mut second, "", &loaded[1]);
    assert_eq!(first, second);
    assert!(store.load(&overworld(), pos).unwrap().is_none());
}

#[test]
fn saving_no_entities_deletes_the_chunk_entry() {
    let dir = TempDir::new("entities-empty");
    let store = AnvilStore::open(&dir.world(), DimensionLayout::Spigot).unwrap();
    let pos = ChunkPos::new(4, 4);
    assert!(store.load_entities(&overworld(), pos).unwrap().is_none());
    store.save_entities(&overworld(), pos, &[]).unwrap();
    store
        .save_entities(
            &overworld(),
            pos,
            &[entity("minecraft:pig", [0, 0, 0, 9], 64.5)],
        )
        .unwrap();
    assert_eq!(
        store
            .load_entities(&overworld(), pos)
            .unwrap()
            .map(|e| e.len()),
        Some(1)
    );
    store.save_entities(&overworld(), pos, &[]).unwrap();
    assert!(store.load_entities(&overworld(), pos).unwrap().is_none());
    assert!(raw_entity_chunk(&dir.world(), pos).is_none());
}

#[test]
fn the_end_world_data_chunks_and_entities_live_in_dim1() {
    let dir = TempDir::new("end-data");
    let root = dir.world();
    let end = rspigot_domain::Identifier::minecraft("the_end");
    let fight = Compound::new()
        .with("DragonKilled", true)
        .with("Gateways", Tag::IntArray(vec![3, 1, 2]));
    let raids = Compound::new().with("next_id", 4);
    let pos = ChunkPos::new(1, 1);
    {
        let store = AnvilStore::open(&root, DimensionLayout::LegacyVanilla).unwrap();
        store
            .save_entities(&end, pos, &[entity("minecraft:pig", [1, 2, 3, 4], 20.5)])
            .unwrap();
        store
            .save_world_data(&end, "ender_dragon_fight", &fight)
            .unwrap();
        store.save_world_data(&end, "raids", &raids).unwrap();
        store.flush().unwrap();
    }
    assert!(root.join("DIM1/entities/r.0.0.mca").exists());
    assert!(
        root.join("DIM1/data/minecraft/ender_dragon_fight.dat")
            .exists()
    );
    assert!(root.join("DIM1/data/minecraft/raids.dat").exists());
    assert!(!root.join("data").exists());
    let store = AnvilStore::open(&root, DimensionLayout::LegacyVanilla).unwrap();
    assert_eq!(
        store.load_world_data(&end, "ender_dragon_fight").unwrap(),
        Some(fight)
    );
    assert_eq!(store.load_world_data(&end, "raids").unwrap(), Some(raids));
    assert_eq!(store.load_entities(&end, pos).unwrap().unwrap().len(), 1);
    assert!(store.load_entities(&overworld(), pos).unwrap().is_none());
    assert!(
        store
            .load_world_data(&overworld(), "ender_dragon_fight")
            .unwrap()
            .is_none()
    );
}
