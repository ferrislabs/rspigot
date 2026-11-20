use std::sync::Arc;

use super::bbox::Bbox;
use super::shaped::Shaped;
use super::world::Start;
use crate::nether::Nether;
use crate::resources::Resources;
use rspigot_blocks::BlockKind;
use rspigot_domain::world::Dimension;
use rspigot_domain::{BlockPos, ChunkPos, Vec3};
use rspigot_nbt::Tag;

fn fortresses(nether: &Nether, dimension: &Dimension, range: i32) -> Vec<Arc<Start>> {
    let mut found: Vec<Arc<Start>> = Vec::new();
    for cx in -range..range {
        for cz in -range..range {
            for start in nether.starts_near(dimension, ChunkPos::new(cx, cz)) {
                if start.name == "fortress"
                    && start.chunk == (cx, cz)
                    && !found.iter().any(|known| known.chunk == start.chunk)
                {
                    found.push(start);
                }
            }
        }
    }
    found
}

fn first(nether: &Nether, dimension: &Dimension) -> Arc<Start> {
    fortresses(nether, dimension, 60)
        .into_iter()
        .next()
        .expect("a fortress start")
}

fn classes(start: &Start) -> Vec<String> {
    start.shapes.iter().map(Shaped::class).collect()
}

#[test]
fn a_fortress_is_laid_out_from_a_bridge_crossing() {
    let resources = Resources::resolve();
    let nether = Nether::new(1, &resources.blocks);
    let dimension = Dimension::nether();
    let start = first(&nether, &dimension);
    let names = classes(&start);
    assert_eq!(names[0], "StartPiece");
    assert!(names.len() > 4, "{names:?}");
    let origin = start.shapes[0].bbox();
    assert_eq!(origin.max.0 - origin.min.0, 18);
    assert_eq!(origin.max.2 - origin.min.2, 18);
    assert_eq!(origin.max.1 - origin.min.1, 9);
    for (index, shape) in start.shapes.iter().enumerate() {
        let bbox = shape.bbox();
        assert!(bbox.min.1 > 10);
        assert!((bbox.min.0 - origin.min.0).abs() < 113 + 20);
        assert!((bbox.min.2 - origin.min.2).abs() < 113 + 20);
        for other in &start.shapes[index + 1..] {
            let filler = names[index].ends_with("BridgeEndFiller")
                || classes_of(other).ends_with("BridgeEndFiller");
            if !filler {
                assert!(!bbox.intersects(other.bbox()), "{index} overlaps");
            }
        }
    }
}

fn classes_of(shape: &Shaped) -> String {
    shape.class()
}

#[test]
fn a_fortress_sits_between_y_48_and_70() {
    let resources = Resources::resolve();
    let nether = Nether::new(1, &resources.blocks);
    let dimension = Dimension::nether();
    let found = fortresses(&nether, &dimension, 60);
    assert!(found.len() >= 2, "{}", found.len());
    for start in found {
        let mut whole = *start.shapes[0].bbox();
        for shape in &start.shapes {
            whole.encapsulate(shape.bbox());
        }
        assert!(whole.min.1 >= 48, "{whole:?}");
        assert!(whole.max.1 <= 70 || whole.min.1 == 48, "{whole:?}");
        assert_eq!(whole, start.bbox);
    }
}

#[test]
fn a_fortress_is_built_of_nether_bricks_and_records_its_pieces() {
    let resources = Resources::resolve();
    let nether = Nether::new(1, &resources.blocks);
    let dimension = Dimension::nether();
    let start = first(&nether, &dimension);
    let (ox, oz) = start.chunk;
    let (mut bricks, mut fences) = (0, 0);
    let mut recorded = false;
    for cx in ox - 4..=ox + 4 {
        for cz in oz - 4..=oz + 4 {
            let column = nether.generate(&resources, &dimension, ChunkPos::new(cx, cz));
            for y in 0..128 {
                for z in 0..16 {
                    for x in 0..16 {
                        let pos = BlockPos::new(cx * 16 + x, y, cz * 16 + z);
                        match column.block(pos).and_then(|state| state.kind()) {
                            Some(BlockKind::NetherBricks) => bricks += 1,
                            Some(BlockKind::NetherBrickFence) => fences += 1,
                            _ => {}
                        }
                    }
                }
            }
            if (cx, cz) == (ox, oz) {
                let starts = column
                    .extra
                    .get_compound("structures")
                    .and_then(|tag| tag.get_compound("starts"))
                    .and_then(|starts| starts.get_compound("minecraft:fortress"))
                    .expect("the start is recorded");
                let children = starts.get_list("Children").expect("children");
                assert_eq!(children.len(), start.shapes.len());
                for child in children {
                    let Tag::Compound(child) = child else {
                        panic!("a child is a compound");
                    };
                    assert_eq!(child.get_int_array("BB").map(<[i32]>::len), Some(6));
                }
                recorded = true;
            }
        }
    }
    assert!(recorded);
    assert!(bricks > 2000, "bricks {bricks}");
    assert!(fences > 50, "fences {fences}");
}

#[test]
fn neighbouring_chunks_reference_the_fortress_start() {
    let resources = Resources::resolve();
    let nether = Nether::new(1, &resources.blocks);
    let dimension = Dimension::nether();
    let start = first(&nether, &dimension);
    let packed =
        (i64::from(start.chunk.0) & 0xFFFF_FFFF) | ((i64::from(start.chunk.1) & 0xFFFF_FFFF) << 32);
    let mut referenced = 0;
    for dx in -4..=4 {
        let column = nether.generate(
            &resources,
            &dimension,
            ChunkPos::new(start.chunk.0 + dx, start.chunk.1),
        );
        let refs = column
            .extra
            .get_compound("structures")
            .and_then(|tag| tag.get_compound("References"))
            .and_then(|refs| refs.get_long_array("minecraft:fortress"));
        if refs.is_some_and(|list| list.contains(&packed)) {
            referenced += 1;
        }
    }
    assert!(referenced >= 2, "referenced {referenced}");
}

#[test]
fn every_special_room_of_a_fortress_is_furnished() {
    let resources = Resources::resolve();
    let nether = Nether::new(1, &resources.blocks);
    let dimension = Dimension::nether();
    let mut spawners = 0;
    let mut wart = 0;
    let mut chests = 0;
    for start in fortresses(&nether, &dimension, 60).iter().take(6) {
        let names = classes(start);
        let throne = names.iter().any(|name| name.ends_with("MonsterThrone"));
        let stalk = names.iter().any(|name| name.ends_with("CastleStalkRoom"));
        let (mut found_spawner, mut found_wart) = (false, false);
        for shape in &start.shapes {
            let bbox = *shape.bbox();
            let chunk_min = (bbox.min.0 >> 4, bbox.min.2 >> 4);
            let chunk_max = (bbox.max.0 >> 4, bbox.max.2 >> 4);
            for cx in chunk_min.0..=chunk_max.0 {
                for cz in chunk_min.1..=chunk_max.1 {
                    let column = nether.generate(&resources, &dimension, ChunkPos::new(cx, cz));
                    scan(&column, &bbox, |kind| match kind {
                        BlockKind::Spawner => found_spawner = true,
                        BlockKind::NetherWart => found_wart = true,
                        BlockKind::Chest => chests += 1,
                        _ => {}
                    });
                }
            }
        }
        assert_eq!(found_spawner, throne);
        assert_eq!(found_wart, stalk);
        spawners += usize::from(found_spawner);
        wart += usize::from(found_wart);
    }
    assert!(spawners + wart + chests > 0);
}

fn scan(
    column: &rspigot_domain::chunk::ChunkColumn,
    bbox: &Bbox,
    mut visit: impl FnMut(BlockKind),
) {
    let pos = column.pos();
    for y in bbox.min.1..=bbox.max.1 {
        for z in pos.z * 16..pos.z * 16 + 16 {
            for x in pos.x * 16..pos.x * 16 + 16 {
                if !bbox.contains((x, y, z)) {
                    continue;
                }
                if let Some(kind) = column
                    .block(BlockPos::new(x, y, z))
                    .and_then(|state| state.kind())
                {
                    visit(kind);
                }
            }
        }
    }
}

#[test]
fn a_monster_throne_holds_a_blaze_spawner() {
    let resources = Resources::resolve();
    let nether = Nether::new(1, &resources.blocks);
    let dimension = Dimension::nether();
    let mut checked = 0;
    for start in fortresses(&nether, &dimension, 80) {
        for shape in &start.shapes {
            if !shape.class().ends_with("MonsterThrone") {
                continue;
            }
            let bbox = shape.bbox();
            let mut spawner = None;
            for cx in bbox.min.0 >> 4..=bbox.max.0 >> 4 {
                for cz in bbox.min.2 >> 4..=bbox.max.2 >> 4 {
                    let column = nether.generate(&resources, &dimension, ChunkPos::new(cx, cz));
                    for y in bbox.min.1..=bbox.max.1 {
                        for z in bbox.min.2..=bbox.max.2 {
                            for x in bbox.min.0..=bbox.max.0 {
                                let pos = BlockPos::new(x, y, z);
                                if pos.chunk() == column.pos()
                                    && column.block(pos).and_then(|state| state.kind())
                                        == Some(BlockKind::Spawner)
                                {
                                    spawner = column.block_entity(pos).map(|_| pos);
                                }
                            }
                        }
                    }
                }
            }
            assert!(spawner.is_some(), "{bbox:?}");
            checked += 1;
        }
    }
    assert!(checked > 0, "no monster throne found");
}

#[test]
fn a_fortress_can_be_located() {
    let resources = Resources::resolve();
    let nether = Nether::new(1, &resources.blocks);
    let dimension = Dimension::nether();
    let (name, at) = nether
        .nearest_structures(
            &dimension,
            Vec3::new(0.0, 64.0, 0.0),
            &["minecraft:fortress".to_owned()],
        )
        .expect("a fortress");
    assert_eq!(name, "minecraft:fortress");
    let chunk = (at.x as i32 >> 4, at.z as i32 >> 4);
    assert!(
        nether
            .starts_near(&dimension, ChunkPos::new(chunk.0, chunk.1))
            .iter()
            .any(|start| start.name == "fortress")
    );
}

#[test]
fn a_missing_bastion_pack_does_not_turn_bastion_spots_into_fortresses() {
    let resources = Resources::resolve();
    let dimension = Dimension::nether();
    let embedded = Nether::new(1, &resources.blocks);
    let full = Nether::with_library(1, &resources.blocks, super::tests::with_extras());
    let chunks = |nether: &Nether| -> Vec<(i32, i32)> {
        fortresses(nether, &dimension, 60)
            .iter()
            .map(|start| start.chunk)
            .collect()
    };
    let (alone, together) = (chunks(&embedded), chunks(&full));
    assert!(!alone.is_empty());
    assert!(alone.len() < together.len() + 1);
    assert!(alone.iter().all(|chunk| together.contains(chunk)));
}
