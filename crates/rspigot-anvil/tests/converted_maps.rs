use std::path::PathBuf;

use rspigot_anvil::{AnvilStore, DimensionLayout};
use rspigot_core::ports::ChunkStore;
use rspigot_domain::{BlockStateId, ChunkPos, Identifier};

fn map_dir() -> Option<PathBuf> {
    std::env::var_os("SHEEPWARS_MAP_DIR").map(PathBuf::from)
}

#[test]
#[ignore = "local only: needs SHEEPWARS_MAP_DIR pointing at a converted map (third-party data)"]
fn every_stored_chunk_of_the_converted_map_loads_with_its_blocks() {
    let Some(dir) = map_dir() else {
        return;
    };
    let store = AnvilStore::open(&dir, DimensionLayout::LegacyVanilla).unwrap();
    let overworld = Identifier::minecraft("overworld");
    let (mut loaded, mut failed, mut non_air) = (0usize, 0usize, 0usize);
    for x in -32..32 {
        for z in -32..32 {
            match store.load(&overworld, ChunkPos::new(x, z)) {
                Ok(Some(chunk)) => {
                    loaded += 1;
                    non_air += chunk
                        .sections()
                        .iter()
                        .map(|section| {
                            section
                                .blocks()
                                .values()
                                .filter(|state| *state != BlockStateId::AIR)
                                .count()
                        })
                        .sum::<usize>();
                }
                Ok(None) => {}
                Err(error) => {
                    failed += 1;
                    eprintln!("chunk {x},{z}: {error}");
                }
            }
        }
    }
    eprintln!("loaded={loaded} failed={failed} non_air={non_air}");
    assert!(loaded > 0);
    assert_eq!(failed, 0);
    assert!(non_air > 0, "the chunks decoded to nothing but air");
}
