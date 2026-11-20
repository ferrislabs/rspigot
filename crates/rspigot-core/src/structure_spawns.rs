use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use rspigot_data::spawns::{BiomeSpawns, StructureBox, StructureOverride, StructureSpawns};
use rspigot_domain::BlockPos;
use rspigot_domain::ChunkPos;
use rspigot_domain::chunk::ChunkColumn;
use rspigot_nbt::{Compound, Tag};

const STRUCTURES: &str = "structures";
const STARTS: &str = "starts";
const REFERENCES: &str = "References";
const CHILDREN: &str = "Children";
const BOUNDS: &str = "BB";
const CATEGORIES: usize = 8;

type Assignment = [Option<u8>; CATEGORIES];

fn override_of(spawns: &StructureSpawns, slot: usize) -> Option<&StructureOverride> {
    match slot {
        0 => spawns.monster.as_ref(),
        1 => spawns.creature.as_ref(),
        2 => spawns.ambient.as_ref(),
        3 => spawns.axolotls.as_ref(),
        4 => spawns.underground_water_creature.as_ref(),
        5 => spawns.water_creature.as_ref(),
        6 => spawns.water_ambient.as_ref(),
        _ => spawns.misc.as_ref(),
    }
}

fn unpack(packed: i64) -> ChunkPos {
    ChunkPos::new(packed as i32, (packed >> 32) as i32)
}

fn bounds(tag: &Tag) -> Option<[i32; 6]> {
    match tag {
        Tag::IntArray(values) => values.as_slice().try_into().ok(),
        _ => None,
    }
}

fn inside(bounds: &[i32; 6], pos: BlockPos) -> bool {
    pos.x >= bounds[0]
        && pos.x <= bounds[3]
        && pos.y >= bounds[1]
        && pos.y <= bounds[4]
        && pos.z >= bounds[2]
        && pos.z <= bounds[5]
}

fn union(boxes: &[[i32; 6]]) -> Option<[i32; 6]> {
    let (first, rest) = boxes.split_first()?;
    Some(rest.iter().fold(*first, |acc, next| {
        [
            acc[0].min(next[0]),
            acc[1].min(next[1]),
            acc[2].min(next[2]),
            acc[3].max(next[3]),
            acc[4].max(next[4]),
            acc[5].max(next[5]),
        ]
    }))
}

fn piece_boxes(start: &Compound) -> Vec<[i32; 6]> {
    start
        .get_list(CHILDREN)
        .unwrap_or_default()
        .iter()
        .filter_map(Tag::as_compound)
        .filter_map(|piece| piece.get(BOUNDS))
        .filter_map(bounds)
        .collect()
}

fn contains(start: &Compound, mode: StructureBox, pos: BlockPos) -> bool {
    let pieces = piece_boxes(start);
    match mode {
        StructureBox::Piece => pieces.iter().any(|piece| inside(piece, pos)),
        StructureBox::Full => union(&pieces).is_some_and(|full| inside(&full, pos)),
    }
}

pub(crate) fn assignment_at<'a>(
    chunk_at: impl Fn(ChunkPos) -> Option<&'a ChunkColumn>,
    pos: BlockPos,
) -> Assignment {
    let mut found: Assignment = [None; CATEGORIES];
    let Some(references) = chunk_at(pos.chunk())
        .and_then(|chunk| chunk.extra.get_compound(STRUCTURES))
        .and_then(|structures| structures.get_compound(REFERENCES))
    else {
        return found;
    };
    let mut candidates: Vec<(usize, &[i64], &str)> = references
        .iter()
        .filter_map(|(name, tag)| {
            let index = rspigot_data::spawns::structure_index(name)?;
            match tag {
                Tag::LongArray(chunks) => Some((index, chunks.as_slice(), name)),
                _ => None,
            }
        })
        .collect();
    candidates.sort_unstable_by_key(|(index, ..)| *index);
    for (index, chunks, name) in candidates {
        let Some(spawns) = rspigot_data::spawns::structure(index) else {
            continue;
        };
        for packed in chunks {
            let Some(start) = chunk_at(unpack(*packed))
                .and_then(|chunk| chunk.extra.get_compound(STRUCTURES))
                .and_then(|structures| structures.get_compound(STARTS))
                .and_then(|starts| starts.get_compound(name))
            else {
                continue;
            };
            for (category, entry) in found.iter_mut().enumerate() {
                if entry.is_none()
                    && let Some(rule) = override_of(spawns, category)
                    && contains(start, rule.bounding_box, pos)
                {
                    *entry = Some(index as u8);
                }
            }
        }
    }
    found
}

pub(crate) fn replacement(
    biome: &'static BiomeSpawns,
    biome_id: u16,
    assignment: Assignment,
) -> &'static BiomeSpawns {
    if assignment.iter().all(Option::is_none) {
        return biome;
    }
    static CACHE: OnceLock<Mutex<HashMap<(u16, Assignment), &'static BiomeSpawns>>> =
        OnceLock::new();
    let Ok(mut cache) = CACHE.get_or_init(Mutex::default).lock() else {
        return biome;
    };
    cache.entry((biome_id, assignment)).or_insert_with(|| {
        let mut merged = *biome;
        for (category, entry) in assignment.iter().enumerate() {
            let Some(rule) = entry
                .and_then(|index| rspigot_data::spawns::structure(usize::from(index)))
                .and_then(|spawns| override_of(spawns, category))
            else {
                continue;
            };
            match category {
                0 => merged.monster = rule.spawns,
                1 => merged.creature = rule.spawns,
                2 => merged.ambient = rule.spawns,
                3 => merged.axolotls = rule.spawns,
                4 => merged.underground_water_creature = rule.spawns,
                5 => merged.water_creature = rule.spawns,
                6 => merged.water_ambient = rule.spawns,
                _ => merged.misc = rule.spawns,
            }
        }
        Box::leak(Box::new(merged))
    })
}

pub(crate) fn spawns_at<'a>(
    chunk_at: impl Fn(ChunkPos) -> Option<&'a ChunkColumn>,
    biome_id: u16,
    pos: BlockPos,
) -> &'static BiomeSpawns {
    replacement(
        rspigot_data::spawns::biome(biome_id),
        biome_id,
        assignment_at(chunk_at, pos),
    )
}

#[cfg(test)]
mod tests {
    use rspigot_domain::BiomeId;
    use rspigot_domain::world::WorldHeight;

    use super::*;

    fn chunk(pos: ChunkPos, structures: Compound) -> ChunkColumn {
        let mut column = ChunkColumn::empty(pos, WorldHeight::OVERWORLD, BiomeId(0));
        column.extra.insert(STRUCTURES, structures);
        column
    }

    fn start(name: &str, at: ChunkPos, boxes: &[[i32; 6]]) -> Compound {
        let children: Vec<Tag> = boxes
            .iter()
            .map(|b| Tag::Compound(Compound::new().with(BOUNDS, Tag::IntArray(b.to_vec()))))
            .collect();
        Compound::new().with(
            STARTS,
            Compound::new().with(
                name,
                Compound::new()
                    .with("ChunkX", at.x)
                    .with("ChunkZ", at.z)
                    .with(CHILDREN, Tag::List(children)),
            ),
        )
    }

    fn reference(name: &str, at: ChunkPos) -> Compound {
        let packed = (i64::from(at.x) & 0xFFFF_FFFF) | ((i64::from(at.z) & 0xFFFF_FFFF) << 32);
        Compound::new().with(
            REFERENCES,
            Compound::new().with(name, Tag::LongArray(vec![packed])),
        )
    }

    fn world(name: &str, origin: ChunkPos, boxes: &[[i32; 6]]) -> HashMap<ChunkPos, ChunkColumn> {
        let mut origin_tag = start(name, origin, boxes);
        for (key, value) in reference(name, origin).iter() {
            origin_tag.insert(key, value.clone());
        }
        let mut map = HashMap::new();
        map.insert(origin, chunk(origin, origin_tag));
        let neighbour = ChunkPos::new(origin.x + 1, origin.z);
        map.insert(neighbour, chunk(neighbour, reference(name, origin)));
        map
    }

    fn entities_of(list: &[rspigot_data::spawns::Spawner]) -> Vec<u16> {
        list.iter().map(|spawner| spawner.entity).collect()
    }

    fn plains() -> u16 {
        rspigot_data::registries::biome_id("minecraft:plains").unwrap()
    }

    fn id(name: &str) -> u16 {
        rspigot_data::entities::id(name).unwrap()
    }

    #[test]
    fn a_piece_box_replaces_the_list_only_inside_a_piece() {
        let map = world(
            "minecraft:fortress",
            ChunkPos::new(0, 0),
            &[[0, 60, 0, 9, 70, 9], [40, 60, 40, 49, 70, 49]],
        );
        let at = |pos| spawns_at(|chunk| map.get(&chunk), plains(), pos);
        let inside = at(BlockPos::new(5, 64, 5));
        let monsters = entities_of(inside.monster);
        assert!(monsters.contains(&id("minecraft:blaze")));
        assert!(monsters.contains(&id("minecraft:wither_skeleton")));
        assert!(!monsters.contains(&id("minecraft:zombie")));
        assert_eq!(
            inside.creature,
            rspigot_data::spawns::biome(plains()).creature
        );
        let gap = at(BlockPos::new(20, 64, 5));
        assert!(entities_of(gap.monster).contains(&id("minecraft:zombie")));
        let reached = at(BlockPos::new(17, 64, 5));
        assert_eq!(
            reached.monster,
            rspigot_data::spawns::biome(plains()).monster
        );
    }

    #[test]
    fn a_full_box_covers_the_gaps_between_pieces() {
        let map = world(
            "minecraft:monument",
            ChunkPos::new(0, 0),
            &[[0, 40, 0, 9, 60, 9], [10, 40, 10, 15, 60, 15]],
        );
        let at = |pos| spawns_at(|chunk| map.get(&chunk), plains(), pos);
        let gap = at(BlockPos::new(12, 50, 2));
        assert_eq!(entities_of(gap.monster), vec![id("minecraft:guardian")]);
        assert!(gap.axolotls.is_empty());
        assert!(!at(BlockPos::new(12, 70, 2)).monster.is_empty());
        assert!(
            entities_of(at(BlockPos::new(12, 70, 2)).monster).contains(&id("minecraft:zombie"))
        );
    }

    #[test]
    fn a_swamp_hut_swaps_monsters_and_creatures() {
        let map = world(
            "minecraft:swamp_hut",
            ChunkPos::new(0, 0),
            &[[0, 60, 0, 6, 66, 8]],
        );
        let spawns = spawns_at(|chunk| map.get(&chunk), plains(), BlockPos::new(3, 62, 3));
        assert_eq!(entities_of(spawns.monster), vec![id("minecraft:witch")]);
        assert_eq!(entities_of(spawns.creature), vec![id("minecraft:cat")]);
    }

    #[test]
    fn an_outpost_box_spawns_pillagers() {
        let map = world(
            "minecraft:pillager_outpost",
            ChunkPos::new(0, 0),
            &[[0, 60, 0, 6, 80, 8]],
        );
        let spawns = spawns_at(|chunk| map.get(&chunk), plains(), BlockPos::new(3, 62, 3));
        assert_eq!(entities_of(spawns.monster), vec![id("minecraft:pillager")]);
    }

    #[test]
    fn a_missing_start_chunk_or_unknown_structure_changes_nothing() {
        let mut map = world(
            "minecraft:fortress",
            ChunkPos::new(0, 0),
            &[[0, 60, 0, 9, 70, 9]],
        );
        map.remove(&ChunkPos::new(0, 0));
        let spawns = spawns_at(|chunk| map.get(&chunk), plains(), BlockPos::new(17, 64, 5));
        assert_eq!(
            spawns.monster,
            rspigot_data::spawns::biome(plains()).monster
        );
        let other = world(
            "minecraft:village_plains",
            ChunkPos::new(0, 0),
            &[[0, 60, 0, 9, 70, 9]],
        );
        let spawns = spawns_at(|chunk| other.get(&chunk), plains(), BlockPos::new(5, 64, 5));
        assert_eq!(
            spawns.monster,
            rspigot_data::spawns::biome(plains()).monster
        );
    }

    #[test]
    fn an_empty_override_means_nothing_spawns() {
        let map = world(
            "minecraft:ancient_city",
            ChunkPos::new(0, 0),
            &[[0, -50, 0, 9, -30, 9]],
        );
        let spawns = spawns_at(|chunk| map.get(&chunk), plains(), BlockPos::new(5, -40, 5));
        assert!(spawns.monster.is_empty() && spawns.creature.is_empty());
    }
}
