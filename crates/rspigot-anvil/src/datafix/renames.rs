use rspigot_nbt::{Compound, Tag};

use super::nbt::{
    compound_mut, ensure_namespaced, list_mut, lookup, rename_string, walk_compounds,
    walk_item_stacks,
};

type Table = &'static [(&'static str, &'static str)];

const ITEM_POTTERY_SHARD_3438: Table = &[
    (
        "minecraft:pottery_shard_archer",
        "minecraft:archer_pottery_shard",
    ),
    (
        "minecraft:pottery_shard_prize",
        "minecraft:prize_pottery_shard",
    ),
    (
        "minecraft:pottery_shard_arms_up",
        "minecraft:arms_up_pottery_shard",
    ),
    (
        "minecraft:pottery_shard_skull",
        "minecraft:skull_pottery_shard",
    ),
];

const ITEM_POTTERY_SHERD_3447: Table = &[
    (
        "minecraft:angler_pottery_shard",
        "minecraft:angler_pottery_sherd",
    ),
    (
        "minecraft:archer_pottery_shard",
        "minecraft:archer_pottery_sherd",
    ),
    (
        "minecraft:arms_up_pottery_shard",
        "minecraft:arms_up_pottery_sherd",
    ),
    (
        "minecraft:blade_pottery_shard",
        "minecraft:blade_pottery_sherd",
    ),
    (
        "minecraft:brewer_pottery_shard",
        "minecraft:brewer_pottery_sherd",
    ),
    (
        "minecraft:burn_pottery_shard",
        "minecraft:burn_pottery_sherd",
    ),
    (
        "minecraft:danger_pottery_shard",
        "minecraft:danger_pottery_sherd",
    ),
    (
        "minecraft:explorer_pottery_shard",
        "minecraft:explorer_pottery_sherd",
    ),
    (
        "minecraft:friend_pottery_shard",
        "minecraft:friend_pottery_sherd",
    ),
    (
        "minecraft:heart_pottery_shard",
        "minecraft:heart_pottery_sherd",
    ),
    (
        "minecraft:heartbreak_pottery_shard",
        "minecraft:heartbreak_pottery_sherd",
    ),
    (
        "minecraft:howl_pottery_shard",
        "minecraft:howl_pottery_sherd",
    ),
    (
        "minecraft:miner_pottery_shard",
        "minecraft:miner_pottery_sherd",
    ),
    (
        "minecraft:mourner_pottery_shard",
        "minecraft:mourner_pottery_sherd",
    ),
    (
        "minecraft:plenty_pottery_shard",
        "minecraft:plenty_pottery_sherd",
    ),
    (
        "minecraft:prize_pottery_shard",
        "minecraft:prize_pottery_sherd",
    ),
    (
        "minecraft:sheaf_pottery_shard",
        "minecraft:sheaf_pottery_sherd",
    ),
    (
        "minecraft:shelter_pottery_shard",
        "minecraft:shelter_pottery_sherd",
    ),
    (
        "minecraft:skull_pottery_shard",
        "minecraft:skull_pottery_sherd",
    ),
    (
        "minecraft:snort_pottery_shard",
        "minecraft:snort_pottery_sherd",
    ),
];

const GRASS_3692: Table = &[("minecraft:grass", "minecraft:short_grass")];
const SCUTE_3800: Table = &[("minecraft:scute", "minecraft:turtle_scute")];
const CHAIN_4541: Table = &[("minecraft:chain", "minecraft:iron_chain")];

const EXPLORER_MAPS_5012: Table = &[
    (
        "minecraft:ocean_explorer_map",
        "minecraft:ocean_monument_map",
    ),
    ("minecraft:swamp_explorer_map", "minecraft:swamp_hut_map"),
    (
        "minecraft:trial_explorer_map",
        "minecraft:buried_trial_chambers_map",
    ),
    (
        "minecraft:woodland_explorer_map",
        "minecraft:woodland_mansion_map",
    ),
    (
        "minecraft:jungle_explorer_map",
        "minecraft:jungle_pyramid_map",
    ),
];

const BRUSHABLE_BLOCK_ENTITY_3438: Table =
    &[("minecraft:suspicious_sand", "minecraft:brushable_block")];

const BLOCK_STATE_KEYS: [&str; 5] = [
    "BlockState",
    "block_state",
    "carriedBlockState",
    "inBlockState",
    "DisplayState",
];

fn renamer(table: Table) -> impl Fn(&str) -> Option<String> {
    move |name| lookup(table, &ensure_namespaced(name))
}

fn rename_state_name(state: &mut Compound, rename: &impl Fn(&str) -> Option<String>) {
    rename_string(state, "Name", rename);
    rename_string(state, "id", rename);
}

fn rename_palette(root: &mut Compound, rename: &impl Fn(&str) -> Option<String>) {
    let Some(sections) = list_mut(root, "sections") else {
        return;
    };
    for section in sections {
        let Some(states) = section
            .as_compound_mut()
            .and_then(|s| compound_mut(s, "block_states"))
        else {
            continue;
        };
        let Some(palette) = list_mut(states, "palette") else {
            continue;
        };
        for entry in palette {
            if let Some(state) = entry.as_compound_mut() {
                rename_state_name(state, rename);
            }
        }
    }
}

fn rename_block_ticks(root: &mut Compound, rename: &impl Fn(&str) -> Option<String>) {
    let Some(ticks) = list_mut(root, "block_ticks") else {
        return;
    };
    for tick in ticks {
        if let Some(tick) = tick.as_compound_mut() {
            rename_string(tick, "i", rename);
        }
    }
}

pub fn rename_blocks_in_chunk(root: &mut Compound, table: Table) {
    let rename = renamer(table);
    rename_palette(root, &rename);
    rename_block_ticks(root, &rename);
    rename_entity_states(root, &rename);
}

fn rename_entity_states(root: &mut Compound, rename: &impl Fn(&str) -> Option<String>) {
    walk_compounds(root, &mut |compound| {
        for key in BLOCK_STATE_KEYS {
            if let Some(state) = compound_mut(compound, key) {
                rename_state_name(state, rename);
            }
        }
    });
}

pub fn rename_blocks_in_entity(root: &mut Compound, table: Table) {
    rename_entity_states(root, &renamer(table));
}

pub fn rename_items(root: &mut Compound, table: Table) {
    let rename = renamer(table);
    walk_item_stacks(root, &mut |stack| rename_string(stack, "id", &rename));
}

pub fn rename_block_entity_ids(root: &mut Compound, table: Table) {
    let rename = renamer(table);
    let Some(entities) = list_mut(root, "block_entities") else {
        return;
    };
    for entity in entities {
        if let Tag::Compound(entity) = entity {
            rename_string(entity, "id", &rename);
        }
    }
}

pub fn pottery_shard_items_3438(root: &mut Compound) {
    rename_items(root, ITEM_POTTERY_SHARD_3438);
}

pub fn pottery_sherd_items_3447(root: &mut Compound) {
    rename_items(root, ITEM_POTTERY_SHERD_3447);
}

pub fn grass_item_3692(root: &mut Compound) {
    rename_items(root, GRASS_3692);
}

pub fn grass_block_chunk_3692(root: &mut Compound) {
    rename_blocks_in_chunk(root, GRASS_3692);
}

pub fn grass_block_entity_3692(root: &mut Compound) {
    rename_blocks_in_entity(root, GRASS_3692);
}

pub fn scute_item_3800(root: &mut Compound) {
    rename_items(root, SCUTE_3800);
}

pub fn chain_item_4541(root: &mut Compound) {
    rename_items(root, CHAIN_4541);
}

pub fn chain_block_chunk_4541(root: &mut Compound) {
    rename_blocks_in_chunk(root, CHAIN_4541);
}

pub fn chain_block_entity_4541(root: &mut Compound) {
    rename_blocks_in_entity(root, CHAIN_4541);
}

pub fn explorer_map_items_5012(root: &mut Compound) {
    rename_items(root, EXPLORER_MAPS_5012);
}

pub fn brushable_block_entity_3438(root: &mut Compound) {
    rename_block_entity_ids(root, BRUSHABLE_BLOCK_ENTITY_3438);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(name: &str) -> Tag {
        Tag::Compound(Compound::new().with("Name", name))
    }

    fn chunk_with_palette(names: &[&str]) -> Compound {
        Compound::new()
            .with(
                "sections",
                Tag::List(vec![Tag::Compound(Compound::new().with(
                    "block_states",
                    Compound::new().with(
                        "palette",
                        Tag::List(names.iter().map(|n| state(n)).collect()),
                    ),
                ))]),
            )
            .with(
                "block_ticks",
                Tag::List(vec![Tag::Compound(
                    Compound::new().with("i", "minecraft:grass"),
                )]),
            )
    }

    #[test]
    fn grass_becomes_short_grass_in_palettes_and_block_ticks() {
        let mut chunk =
            chunk_with_palette(&["minecraft:air", "minecraft:grass", "minecraft:grass_block"]);
        grass_block_chunk_3692(&mut chunk);
        let palette = chunk.get_list("sections").unwrap()[0]
            .as_compound()
            .unwrap()
            .get_compound("block_states")
            .unwrap()
            .get_list("palette")
            .unwrap();
        assert_eq!(
            palette[1].as_compound().unwrap().get_str("Name"),
            Some("minecraft:short_grass")
        );
        assert_eq!(
            palette[2].as_compound().unwrap().get_str("Name"),
            Some("minecraft:grass_block")
        );
        assert_eq!(
            chunk.get_list("block_ticks").unwrap()[0]
                .as_compound()
                .unwrap()
                .get_str("i"),
            Some("minecraft:short_grass")
        );
    }

    #[test]
    fn renames_also_reach_palette_entries_keyed_by_id() {
        let mut chunk = Compound::new().with(
            "sections",
            Tag::List(vec![Tag::Compound(Compound::new().with(
                "block_states",
                Compound::new().with(
                    "palette",
                    Tag::List(vec![Tag::Compound(
                        Compound::new().with("id", "minecraft:chain"),
                    )]),
                ),
            ))]),
        );
        chain_block_chunk_4541(&mut chunk);
        let section = chunk.get_list("sections").unwrap()[0]
            .as_compound()
            .unwrap();
        let entry = section
            .get_compound("block_states")
            .unwrap()
            .get_list("palette")
            .unwrap()[0]
            .as_compound()
            .unwrap();
        assert_eq!(entry.get_str("id"), Some("minecraft:iron_chain"));
    }

    #[test]
    fn items_are_renamed_wherever_they_sit() {
        let mut block_entity = Compound::new().with(
            "Items",
            Tag::List(vec![
                Tag::Compound(
                    Compound::new()
                        .with("id", "minecraft:scute")
                        .with("count", 1),
                ),
                Tag::Compound(
                    Compound::new()
                        .with("id", "minecraft:stick")
                        .with("count", 1),
                ),
            ]),
        );
        scute_item_3800(&mut block_entity);
        let items = block_entity.get_list("Items").unwrap();
        assert_eq!(
            items[0].as_compound().unwrap().get_str("id"),
            Some("minecraft:turtle_scute")
        );
        assert_eq!(
            items[1].as_compound().unwrap().get_str("id"),
            Some("minecraft:stick")
        );
    }

    #[test]
    fn pottery_shards_become_sherds_in_two_steps() {
        let mut item = Compound::new()
            .with("id", "minecraft:pottery_shard_archer")
            .with("Count", 1i8);
        pottery_shard_items_3438(&mut item);
        assert_eq!(item.get_str("id"), Some("minecraft:archer_pottery_shard"));
        pottery_sherd_items_3447(&mut item);
        assert_eq!(item.get_str("id"), Some("minecraft:archer_pottery_sherd"));
    }

    #[test]
    fn explorer_maps_take_their_structure_names() {
        let mut item = Compound::new()
            .with("id", "minecraft:woodland_explorer_map")
            .with("count", 1);
        explorer_map_items_5012(&mut item);
        assert_eq!(item.get_str("id"), Some("minecraft:woodland_mansion_map"));
    }

    #[test]
    fn falling_blocks_and_displays_follow_the_block_rename() {
        let mut entity = Compound::new().with("id", "minecraft:falling_block").with(
            "BlockState",
            Compound::new().with("Name", "minecraft:chain"),
        );
        chain_block_entity_4541(&mut entity);
        assert_eq!(
            entity.get_compound("BlockState").unwrap().get_str("Name"),
            Some("minecraft:iron_chain")
        );
    }

    #[test]
    fn suspicious_sand_block_entities_become_brushable_blocks() {
        let mut chunk = Compound::new().with(
            "block_entities",
            Tag::List(vec![Tag::Compound(
                Compound::new().with("id", "minecraft:suspicious_sand"),
            )]),
        );
        brushable_block_entity_3438(&mut chunk);
        assert_eq!(
            chunk.get_list("block_entities").unwrap()[0]
                .as_compound()
                .unwrap()
                .get_str("id"),
            Some("minecraft:brushable_block")
        );
    }
}
