use rspigot_nbt::{Compound, Tag};

const ITEMS: &str = "Items";
const LOOT_TABLE: &str = "LootTable";
const LOOT_SEED: &str = "LootTableSeed";

const CONTAINER: &str = "minecraft:container";
const CONTAINER_LOOT: &str = "minecraft:container_loot";

const RENAMED: &[(&str, &str)] = &[
    ("minecraft:custom_name", "CustomName"),
    ("minecraft:lock", "lock"),
    ("minecraft:bees", "bees"),
    ("minecraft:banner_patterns", "patterns"),
    ("minecraft:pot_decorations", "sherds"),
    ("minecraft:profile", "profile"),
    ("minecraft:note_block_sound", "note_block_sound"),
];

pub(crate) fn entity_kind_for(block: &str) -> Option<&'static str> {
    let name = block.strip_prefix("minecraft:").unwrap_or(block);
    if name.ends_with("_banner") {
        Some("banner")
    } else if name.ends_with("_head") || name.ends_with("_skull") {
        Some("skull")
    } else if name == "decorated_pot" {
        Some("decorated_pot")
    } else {
        None
    }
}

fn container_of(items: &[Tag]) -> Tag {
    let list = items
        .iter()
        .filter_map(|entry| {
            let entry = entry.as_compound()?;
            let slot = entry.get_i32("Slot")?;
            let mut item = entry.clone();
            item.remove("Slot");
            Some(Tag::Compound(
                Compound::new().with("slot", slot).with("item", item),
            ))
        })
        .collect();
    Tag::List(list)
}

fn items_of(container: &[Tag]) -> Tag {
    let list = container
        .iter()
        .filter_map(|entry| {
            let entry = entry.as_compound()?;
            let slot = entry.get_i32("slot")?;
            let mut item = entry.get_compound("item")?.clone();
            item.insert("Slot", slot as i8);
            Some(Tag::Compound(item))
        })
        .collect();
    Tag::List(list)
}

pub(crate) fn collect(data: &Compound, include: &[&str]) -> Compound {
    let mut out = Compound::new();
    for name in include {
        match *name {
            CONTAINER => {
                if let Some(items) = data.get_list(ITEMS).filter(|items| !items.is_empty()) {
                    out.insert(CONTAINER, container_of(items));
                }
            }
            CONTAINER_LOOT => {
                if let Some(table) = data.get_str(LOOT_TABLE) {
                    let mut loot = Compound::new().with("loot_table", table);
                    if let Some(seed) = data.get_i64(LOOT_SEED) {
                        loot.insert("seed", seed);
                    }
                    out.insert(CONTAINER_LOOT, loot);
                }
            }
            other => {
                let key = RENAMED
                    .iter()
                    .find_map(|(component, key)| (*component == other).then_some(*key));
                if let Some(value) = key.and_then(|key| data.get(key)) {
                    out.insert(other, value.clone());
                }
            }
        }
    }
    out
}

pub(crate) fn apply(data: &mut Compound, components: &Compound) -> bool {
    let mut changed = false;
    if let Some(list) = components.get_list(CONTAINER) {
        data.insert(ITEMS, items_of(list));
        changed = true;
    }
    if let Some(loot) = components.get_compound(CONTAINER_LOOT)
        && let Some(table) = loot.get_str("loot_table")
    {
        data.insert(LOOT_TABLE, table);
        if let Some(seed) = loot.get_i64("seed") {
            data.insert(LOOT_SEED, seed);
        }
        changed = true;
    }
    for (component, key) in RENAMED {
        if let Some(value) = components.get(component) {
            data.insert(*key, value.clone());
            changed = true;
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shulker_data() -> Compound {
        let item = Compound::new()
            .with("id", "minecraft:diamond")
            .with("count", 3)
            .with("Slot", 4_i8);
        Compound::new().with("Items", Tag::List(vec![Tag::Compound(item)]))
    }

    #[test]
    fn container_round_trips_through_the_component() {
        let data = shulker_data();
        let components = collect(&data, &[CONTAINER]);
        let entry = components.get_list(CONTAINER).unwrap()[0]
            .as_compound()
            .unwrap();
        assert_eq!(entry.get_i32("slot"), Some(4));
        assert_eq!(
            entry.get_compound("item").unwrap().get_str("id"),
            Some("minecraft:diamond")
        );
        let mut placed = Compound::new();
        assert!(apply(&mut placed, &components));
        assert_eq!(placed, data);
    }

    #[test]
    fn empty_container_is_not_copied() {
        let data = Compound::new().with("Items", Tag::List(Vec::new()));
        assert!(collect(&data, &[CONTAINER]).is_empty());
    }

    #[test]
    fn only_included_components_are_copied() {
        let data = Compound::new()
            .with("CustomName", "Boxy")
            .with("patterns", Tag::List(Vec::new()));
        let components = collect(&data, &["minecraft:custom_name"]);
        assert_eq!(components.len(), 1);
        assert_eq!(components.get_str("minecraft:custom_name"), Some("Boxy"));
    }

    #[test]
    fn loot_table_maps_to_container_loot() {
        let data = Compound::new()
            .with("LootTable", "minecraft:chests/x")
            .with("LootTableSeed", 9_i64);
        let components = collect(&data, &[CONTAINER_LOOT]);
        let mut placed = Compound::new();
        apply(&mut placed, &components);
        assert_eq!(placed, data);
    }

    #[test]
    fn entity_kinds_for_blocks_without_a_default_entity() {
        assert_eq!(entity_kind_for("minecraft:red_wall_banner"), Some("banner"));
        assert_eq!(entity_kind_for("minecraft:player_wall_head"), Some("skull"));
        assert_eq!(entity_kind_for("minecraft:skeleton_skull"), Some("skull"));
        assert_eq!(entity_kind_for("minecraft:stone"), None);
    }
}
