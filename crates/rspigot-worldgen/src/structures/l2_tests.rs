use std::collections::BTreeSet;

use super::tests::with_extras;
use crate::legacy::JavaRandom;
use crate::nether::Nether;
use crate::resources::Resources;
use rspigot_domain::ChunkPos;
use rspigot_domain::world::Dimension;

const SPACING: i32 = 27;
const SEPARATION: i32 = 4;
const SALT: i64 = 30_084_232;

fn reference_chunk(seed: i64, region_x: i32, region_z: i32) -> (i32, i32) {
    let mut random = JavaRandom::new(0);
    random.set_seed(
        i64::from(region_x)
            .wrapping_mul(341_873_128_712)
            .wrapping_add(i64::from(region_z).wrapping_mul(132_897_987_541))
            .wrapping_add(seed)
            .wrapping_add(SALT),
    );
    let bound = SPACING - SEPARATION;
    let x = random.next_int(bound);
    let z = random.next_int(bound);
    (region_x * SPACING + x, region_z * SPACING + z)
}

fn nether_starts(seed: i64, range: i32) -> Vec<(String, (i32, i32))> {
    let resources = Resources::resolve();
    let nether = Nether::with_library(seed as u64, &resources.blocks, with_extras());
    let dimension = Dimension::nether();
    let mut found = Vec::new();
    for cx in -range..range {
        for cz in -range..range {
            for start in nether.starts_near(&dimension, ChunkPos::new(cx, cz)) {
                if start.chunk == (cx, cz)
                    && matches!(start.name.as_str(), "fortress" | "bastion_remnant")
                {
                    found.push((start.name.clone(), start.chunk));
                }
            }
        }
    }
    found
}

fn check_nether(seed: i64) {
    let range = 60;
    let found = nether_starts(seed, range);
    let mut reference = BTreeSet::new();
    for rx in (-range).div_euclid(SPACING) - 1..=range.div_euclid(SPACING) + 1 {
        for rz in (-range).div_euclid(SPACING) - 1..=range.div_euclid(SPACING) + 1 {
            reference.insert(reference_chunk(seed, rx, rz));
        }
    }
    for (name, chunk) in &found {
        assert!(
            reference.contains(chunk),
            "{name} at {chunk:?} is not a vanilla placement chunk"
        );
    }
    let in_range: Vec<_> = reference
        .iter()
        .filter(|(x, z)| (-range..range).contains(x) && (-range..range).contains(z))
        .collect();
    assert_eq!(
        found.len(),
        in_range.len(),
        "every nether complex chunk must hold a start: {found:?} vs {in_range:?}"
    );
    assert!(
        found.iter().any(|(name, _)| name == "fortress"),
        "{found:?}"
    );
    assert!(
        found.iter().any(|(name, _)| name == "bastion_remnant"),
        "{found:?}"
    );
}

#[test]
fn nether_complexes_land_on_vanilla_chunks_for_seed_one() {
    check_nether(1);
}

#[test]
fn nether_complexes_land_on_vanilla_chunks_for_a_negative_seed() {
    check_nether(-4_172_144_997_902_289_642);
}

#[test]
fn nether_complexes_land_on_vanilla_chunks_for_seed_424242() {
    check_nether(424_242);
}

mod aliases {
    use super::super::alias::{Alias, lookup};
    use serde_json::json;

    fn trial_chambers() -> Vec<Alias> {
        let raw = json!([
            {"type": "minecraft:random_group", "groups": [
                {"weight": 1, "data": [
                    {"type": "minecraft:direct", "alias": "minecraft:a/ranged", "target": "minecraft:a/ranged/skeleton"},
                    {"type": "minecraft:direct", "alias": "minecraft:a/slow", "target": "minecraft:a/slow/skeleton"}]},
                {"weight": 1, "data": [
                    {"type": "minecraft:direct", "alias": "minecraft:a/ranged", "target": "minecraft:a/ranged/stray"},
                    {"type": "minecraft:direct", "alias": "minecraft:a/slow", "target": "minecraft:a/slow/stray"}]}
            ]},
            {"type": "minecraft:random", "alias": "minecraft:a/melee", "targets": [
                {"weight": 1, "data": "minecraft:a/melee/zombie"},
                {"weight": 1, "data": "minecraft:a/melee/husk"}]}
        ]);
        Alias::parse_all(Some(&raw)).unwrap()
    }

    #[test]
    fn a_group_binds_its_aliases_together() {
        let aliases = trial_chambers();
        for x in 0..40 {
            let table = lookup(&aliases, (x * 16, -30, 7), 1);
            assert_eq!(table.len(), 3);
            let ranged = table["minecraft:a/ranged"].rsplit('/').next().unwrap();
            let slow = table["minecraft:a/slow"].rsplit('/').next().unwrap();
            assert_eq!(ranged, slow);
        }
    }

    #[test]
    fn the_same_position_and_seed_resolve_the_same_way() {
        let aliases = trial_chambers();
        assert_eq!(
            lookup(&aliases, (160, -30, 7), 9),
            lookup(&aliases, (160, -30, 7), 9)
        );
        let varied = (0..40)
            .map(|x| lookup(&aliases, (x * 16, -30, 7), 9)["minecraft:a/melee"].clone())
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(varied.len(), 2);
    }

    #[test]
    fn no_alias_means_an_empty_table() {
        assert!(lookup(&[], (0, 0, 0), 1).is_empty());
    }
}

mod trial_chambers {
    use super::with_extras;
    use crate::overworld::Overworld;
    use crate::resources::Resources;
    use rspigot_data::multi_noise::ClimateVariant;
    use rspigot_domain::ChunkPos;
    use rspigot_domain::world::Dimension;

    #[test]
    fn a_trial_chamber_is_generated_underground_with_its_spawners() {
        let resources = Resources::resolve();
        let overworld =
            Overworld::with_library(1, ClimateVariant::Normal, &resources.blocks, with_extras());
        let dimension = Dimension::overworld();
        let (cx, cz) = (-22, 11);
        let centre = overworld.generate(&resources, &dimension, ChunkPos::new(cx, cz));
        let start = centre
            .extra
            .get_compound("structures")
            .and_then(|tag| tag.get_compound("starts"))
            .and_then(|starts| starts.get_compound("minecraft:trial_chambers"))
            .expect("a trial chambers start at the vanilla chunk");
        let pieces = start.get_list("Children").unwrap_or_default();
        assert!(pieces.len() > 20, "{}", pieces.len());
        let spawners = (cx - 3..=cx + 3)
            .flat_map(|x| (cz - 3..=cz + 3).map(move |z| (x, z)))
            .map(|(x, z)| overworld.generate(&resources, &dimension, ChunkPos::new(x, z)))
            .flat_map(|column| column.block_entities.into_iter())
            .filter(|entity| {
                matches!(
                    entity.kind.as_str(),
                    "minecraft:trial_spawner" | "minecraft:vault"
                )
            })
            .count();
        assert!(spawners > 0);
    }
}

mod mansion {
    use crate::overworld::Overworld;
    use crate::resources::Resources;
    use crate::structures::Library;
    use rspigot_blocks::BlockKind;
    use rspigot_data::multi_noise::ClimateVariant;
    use rspigot_domain::world::Dimension;
    use rspigot_domain::{BlockPos, ChunkPos};

    const CHUNKS: [(i32, i32); 1] = [(118, 15)];

    #[test]
    fn a_woodland_mansion_is_generated_in_a_dark_forest_at_a_vanilla_chunk() {
        let resources = Resources::resolve();
        let overworld = Overworld::with_library(
            1,
            ClimateVariant::Normal,
            &resources.blocks,
            Library::embedded(),
        );
        let dimension = Dimension::overworld();
        let mut found = Vec::new();
        for (cx, cz) in CHUNKS {
            let column = overworld.generate(&resources, &dimension, ChunkPos::new(cx, cz));
            let recorded = column
                .extra
                .get_compound("structures")
                .and_then(|tag| tag.get_compound("starts"))
                .and_then(|starts| starts.get_compound("minecraft:mansion"))
                .map(|start| start.get_list("Children").map_or(0, <[_]>::len));
            if let Some(pieces) = recorded {
                found.push(((cx, cz), pieces));
            }
        }
        assert!(!found.is_empty(), "no mansion started at {CHUNKS:?}");
        for (_, pieces) in &found {
            assert!(*pieces > 60, "{pieces}");
        }
        let ((cx, cz), _) = found[0];
        let (mut planks, mut chests, mut cobble) = (0, 0, 0);
        for x in cx - 3..=cx + 3 {
            for z in cz - 3..=cz + 3 {
                let column = overworld.generate(&resources, &dimension, ChunkPos::new(x, z));
                chests += column
                    .block_entities
                    .iter()
                    .filter(|entity| entity.kind.as_str() == "minecraft:chest")
                    .count();
                for y in 40..140 {
                    for dz in 0..16 {
                        for dx in 0..16 {
                            let pos = BlockPos::new(x * 16 + dx, y, z * 16 + dz);
                            match column.block(pos).and_then(|state| state.kind()) {
                                Some(BlockKind::DarkOakPlanks) => planks += 1,
                                Some(BlockKind::Cobblestone) => cobble += 1,
                                _ => {}
                            }
                        }
                    }
                }
            }
        }
        assert!(planks > 1000, "planks {planks}");
        assert!(chests > 0, "chests {chests}");
        assert!(cobble > 100, "cobble {cobble}");
    }
}

mod end_city {
    use crate::end::End;
    use crate::resources::Resources;
    use rspigot_blocks::BlockKind;
    use rspigot_domain::world::Dimension;
    use rspigot_domain::{BlockPos, ChunkPos, Vec3};

    const SEED: u64 = 1;
    const CITY: (i32, i32) = (6, 103);

    fn pieces(column: &rspigot_domain::chunk::ChunkColumn) -> Vec<String> {
        column
            .extra
            .get_compound("structures")
            .and_then(|tag| tag.get_compound("starts"))
            .and_then(|starts| starts.get_compound("minecraft:end_city"))
            .and_then(|start| start.get_list("Children"))
            .map(|list| {
                list.iter()
                    .filter_map(|tag| tag.as_compound())
                    .filter_map(|piece| piece.get_str("Template").map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn ship_bounds(column: &rspigot_domain::chunk::ChunkColumn) -> Option<[i32; 6]> {
        let start = column
            .extra
            .get_compound("structures")?
            .get_compound("starts")?
            .get_compound("minecraft:end_city")?;
        let piece = start
            .get_list("Children")?
            .iter()
            .filter_map(|tag| tag.as_compound())
            .find(|piece| piece.get_str("Template") == Some("ship"))?;
        match piece.get("BB")? {
            rspigot_nbt::Tag::IntArray(values) => values.as_slice().try_into().ok(),
            _ => None,
        }
    }

    #[test]
    fn an_end_city_with_a_ship_starts_in_the_outer_islands_at_the_vanilla_chunk() {
        let resources = Resources::resolve();
        let end = End::new(SEED, &resources.blocks);
        let dimension = Dimension::end();
        let column = end.generate(&resources, &dimension, ChunkPos::new(CITY.0, CITY.1));
        let names = pieces(&column);
        assert!(names.len() > 50, "{}", names.len());
        assert!(names.iter().any(|name| name == "base_floor"));
        assert!(names.iter().any(|name| name == "tower_top"));
        assert!(names.iter().any(|name| name == "ship"));
    }

    #[test]
    fn an_end_city_is_built_of_purpur_with_loot_and_sentries() {
        let resources = Resources::resolve();
        let end = End::new(SEED, &resources.blocks);
        let dimension = Dimension::end();
        let (mut purpur, mut bricks, mut chests, mut shulkers, mut frames) = (0, 0, 0, 0, 0);
        let origin = end.generate(&resources, &dimension, ChunkPos::new(CITY.0, CITY.1));
        let ship = ship_bounds(&origin).expect("a ship");
        let mut area: Vec<(i32, i32)> = Vec::new();
        for x in CITY.0 - 4..=CITY.0 + 4 {
            for z in CITY.1 - 4..=CITY.1 + 4 {
                area.push((x, z));
            }
        }
        for x in ship[0] >> 4..=ship[3] >> 4 {
            for z in ship[2] >> 4..=ship[5] >> 4 {
                area.push((x, z));
            }
        }
        area.sort_unstable();
        area.dedup();
        for (x, z) in area {
            {
                let pos = ChunkPos::new(x, z);
                let column = end.generate(&resources, &dimension, pos);
                chests += column
                    .block_entities
                    .iter()
                    .filter(|entity| {
                        entity.kind.as_str() == "minecraft:chest"
                            && entity.data.get_str("LootTable")
                                == Some("minecraft:chests/end_city_treasure")
                    })
                    .count();
                for entity in end.structure_entities(&resources, &dimension, pos) {
                    match entity.get_str("id") {
                        Some("minecraft:shulker") => shulkers += 1,
                        Some("minecraft:item_frame") => frames += 1,
                        _ => {}
                    }
                }
                for y in 40..160 {
                    for dz in 0..16 {
                        for dx in 0..16 {
                            let at = BlockPos::new(x * 16 + dx, y, z * 16 + dz);
                            match column.block(at).and_then(|state| state.kind()) {
                                Some(BlockKind::PurpurBlock) => purpur += 1,
                                Some(BlockKind::EndStoneBricks) => bricks += 1,
                                _ => {}
                            }
                        }
                    }
                }
            }
        }
        assert!(purpur > 500, "purpur {purpur}");
        assert!(bricks > 500, "bricks {bricks}");
        assert!(chests > 0, "chests {chests}");
        assert!(shulkers > 0, "shulkers {shulkers}");
        assert!(frames > 0, "frames {frames}");
    }

    #[test]
    fn an_end_city_can_be_located_from_the_central_island() {
        let resources = Resources::resolve();
        let end = End::new(SEED, &resources.blocks);
        let dimension = Dimension::end();
        let found = end.nearest_structures(
            &resources,
            &dimension,
            Vec3::new(0.0, 64.0, 0.0),
            &["minecraft:end_city".to_owned()],
        );
        let (name, at) = found.expect("an end city");
        assert_eq!(name, "minecraft:end_city");
        assert!(at.x.abs() > 800.0 || at.z.abs() > 800.0, "{at:?}");
    }

    #[test]
    fn the_central_island_has_no_end_city() {
        let resources = Resources::resolve();
        let end = End::new(SEED, &resources.blocks);
        let dimension = Dimension::end();
        for x in -20..20 {
            for z in -20..20 {
                assert!(
                    end.starts_near(&resources, &dimension, ChunkPos::new(x, z))
                        .is_empty()
                );
            }
        }
    }
}

mod spawns {
    use super::with_extras;
    use crate::nether::Nether;
    use crate::overworld::Overworld;
    use crate::resources::Resources;
    use rspigot_data::multi_noise::ClimateVariant;
    use rspigot_data::spawns::{StructureBox, structure, structure_index};
    use rspigot_domain::chunk::ChunkColumn;
    use rspigot_domain::world::Dimension;
    use rspigot_domain::{BlockPos, ChunkPos};
    use rspigot_nbt::{Compound, Tag};

    fn start<'a>(column: &'a ChunkColumn, id: &str) -> Option<&'a Compound> {
        column
            .extra
            .get_compound("structures")?
            .get_compound("starts")?
            .get_compound(id)
    }

    fn boxes(start: &Compound) -> Vec<[i32; 6]> {
        start
            .get_list("Children")
            .unwrap_or_default()
            .iter()
            .filter_map(Tag::as_compound)
            .filter_map(|piece| match piece.get("BB")? {
                Tag::IntArray(values) => values.as_slice().try_into().ok(),
                _ => None,
            })
            .collect()
    }

    fn inside(bounds: &[i32; 6], pos: BlockPos) -> bool {
        (bounds[0]..=bounds[3]).contains(&pos.x)
            && (bounds[1]..=bounds[4]).contains(&pos.y)
            && (bounds[2]..=bounds[5]).contains(&pos.z)
    }

    fn references(column: &ChunkColumn, id: &str) -> Vec<ChunkPos> {
        match column
            .extra
            .get_compound("structures")
            .and_then(|tag| tag.get_compound("References"))
            .and_then(|tag| tag.get(id))
        {
            Some(Tag::LongArray(packed)) => packed
                .iter()
                .map(|value| ChunkPos::new(*value as i32, (*value >> 32) as i32))
                .collect(),
            _ => Vec::new(),
        }
    }

    fn covered(
        id: &str,
        origin: ChunkPos,
        mode: StructureBox,
        generate: impl Fn(ChunkPos) -> ChunkColumn,
    ) {
        let index = structure_index(id).unwrap_or_else(|| panic!("{id} has no spawn override"));
        let spawns = structure(index).expect("the override table");
        assert!(
            spawns.monster.is_some_and(|rule| rule.bounding_box == mode),
            "{id}"
        );
        let centre = generate(origin);
        let own = start(&centre, id).unwrap_or_else(|| panic!("no {id} start at {origin:?}"));
        let pieces = boxes(own);
        assert!(!pieces.is_empty(), "{id}");
        let piece = pieces[0];
        let pos = BlockPos::new(
            (piece[0] + piece[3]) / 2,
            (piece[1] + piece[4]) / 2,
            (piece[2] + piece[5]) / 2,
        );
        let there = generate(pos.chunk());
        let owners = references(&there, id);
        assert!(owners.contains(&origin), "{id} {owners:?}");
        let found = owners.iter().any(|owner| {
            let column = generate(*owner);
            start(&column, id).is_some_and(|start| {
                let list = boxes(start);
                match mode {
                    StructureBox::Piece => list.iter().any(|bounds| inside(bounds, pos)),
                    StructureBox::Full => {
                        let full = list.iter().fold(list[0], |acc, next| {
                            [
                                acc[0].min(next[0]),
                                acc[1].min(next[1]),
                                acc[2].min(next[2]),
                                acc[3].max(next[3]),
                                acc[4].max(next[4]),
                                acc[5].max(next[5]),
                            ]
                        });
                        inside(&full, pos)
                    }
                }
            })
        });
        assert!(found, "{id} bounds do not hold {pos:?}");
    }

    #[test]
    fn a_fortress_exposes_its_piece_boxes_to_the_spawn_lookup() {
        let resources = Resources::resolve();
        let nether = Nether::with_library(1, &resources.blocks, with_extras());
        let dimension = Dimension::nether();
        let mut origin = None;
        'scan: for cx in -60..60 {
            for cz in -60..60 {
                if nether
                    .starts_near(&dimension, ChunkPos::new(cx, cz))
                    .iter()
                    .any(|start| start.name == "fortress" && start.chunk == (cx, cz))
                {
                    origin = Some(ChunkPos::new(cx, cz));
                    break 'scan;
                }
            }
        }
        covered(
            "minecraft:fortress",
            origin.expect("a fortress"),
            StructureBox::Piece,
            |pos| nether.generate(&resources, &dimension, pos),
        );
    }

    #[test]
    fn an_ancient_city_exposes_its_full_box_to_the_spawn_lookup() {
        let resources = Resources::resolve();
        let overworld =
            Overworld::with_library(1, ClimateVariant::Normal, &resources.blocks, with_extras());
        let dimension = Dimension::overworld();
        covered(
            "minecraft:ancient_city",
            ChunkPos::new(-201, -36),
            StructureBox::Full,
            |pos| overworld.generate(&resources, &dimension, pos),
        );
    }

    #[test]
    fn trial_chambers_expose_their_piece_boxes_to_the_spawn_lookup() {
        let resources = Resources::resolve();
        let overworld =
            Overworld::with_library(1, ClimateVariant::Normal, &resources.blocks, with_extras());
        let dimension = Dimension::overworld();
        covered(
            "minecraft:trial_chambers",
            ChunkPos::new(-22, 11),
            StructureBox::Piece,
            |pos| overworld.generate(&resources, &dimension, pos),
        );
    }

    #[test]
    fn bastions_mansions_and_end_cities_have_no_spawn_override_in_vanilla() {
        for id in [
            "minecraft:bastion_remnant",
            "minecraft:mansion",
            "minecraft:end_city",
        ] {
            assert!(structure_index(id).is_none(), "{id}");
        }
    }
}
