use rspigot_data::loot::{
    BlockPredicate, DamageSourcePredicate, EnchantmentPredicate, EntityFlags, EntityPredicate,
    EntitySubPredicate, EntityTarget, IntRange, ItemPredicate, TagPredicate,
};
use serde_json::Value;

use super::json::{
    Parsed, flag, holder_set, int_range, invalid, leak, leak_slice, leak_text, name, object,
    only_keys, optional_flag, required, unsupported,
};

const BLOCK: &str = "minecraft:block";
const ITEM: &str = "minecraft:item";
const ENTITY_TYPE: &str = "minecraft:entity_type";

fn state_pairs(value: &Value) -> Parsed<&'static [(&'static str, &'static str)]> {
    let mut pairs = Vec::new();
    for (key, wanted) in object(value)? {
        let text = match wanted {
            Value::String(text) => text.clone(),
            Value::Bool(flag) => flag.to_string(),
            Value::Number(number) => number.to_string(),
            _ => return unsupported(format!("block state range for \"{key}\"")),
        };
        pairs.push((leak_text(key.clone()), leak_text(text)));
    }
    pairs.sort_unstable();
    Ok(leak_slice(pairs))
}

pub(super) fn block_predicate(value: &Value) -> Parsed<BlockPredicate> {
    let map = object(value)?;
    only_keys(map, &["blocks", "state"], "block predicate")?;
    Ok(BlockPredicate {
        blocks: map
            .get("blocks")
            .map(|blocks| holder_set(BLOCK, blocks))
            .transpose()?,
        state: map.get("state").map_or(Ok(&[][..]), state_pairs)?,
    })
}

fn enchantment_names(value: &Value) -> Parsed<&'static [&'static str]> {
    let mut names = Vec::new();
    let entries = match value {
        Value::Array(items) => items.iter().collect(),
        single => vec![single],
    };
    for entry in entries {
        let Some(text) = entry.as_str() else {
            return invalid(format!("bad enchantments {entry}"));
        };
        match text.strip_prefix('#') {
            Some(tag) => {
                let tag = name(&Value::from(tag))?;
                match rspigot_data::tags::tag("minecraft:enchantment", &tag) {
                    Some(members) => {
                        for id in members {
                            match rspigot_data::registries::synced_entries("minecraft:enchantment")
                                .and_then(|all| all.get(*id as usize))
                            {
                                Some(found) => names.push(*found),
                                None => return invalid(format!("unknown enchantment id {id}")),
                            }
                        }
                    }
                    None => return unsupported(format!("enchantment tag #{tag}")),
                }
            }
            None => names.push(leak_text(name(entry)?)),
        }
    }
    Ok(leak_slice(names))
}

fn enchantment_predicate(value: &Value) -> Parsed<EnchantmentPredicate> {
    let map = object(value)?;
    only_keys(map, &["enchantments", "levels"], "enchantment predicate")?;
    Ok(EnchantmentPredicate {
        enchantments: map.get("enchantments").map(enchantment_names).transpose()?,
        levels: map.get("levels").map(int_range).transpose()?,
    })
}

pub(super) fn item_predicate(value: &Value) -> Parsed<ItemPredicate> {
    let map = object(value)?;
    only_keys(map, &["items", "predicates"], "item predicate")?;
    let mut enchantments = Vec::new();
    if let Some(predicates) = map.get("predicates") {
        let predicates = object(predicates)?;
        only_keys(
            predicates,
            &["minecraft:enchantments"],
            "item sub-predicate",
        )?;
        if let Some(Value::Array(list)) = predicates.get("minecraft:enchantments") {
            for entry in list {
                enchantments.push(enchantment_predicate(entry)?);
            }
        }
    }
    Ok(ItemPredicate {
        items: map
            .get("items")
            .map(|items| holder_set(ITEM, items))
            .transpose()?,
        enchantments: leak_slice(enchantments),
    })
}

fn bare(key: &str) -> &str {
    key.strip_prefix("minecraft:").unwrap_or(key)
}

fn entity_flags(value: &Value) -> Parsed<EntityFlags> {
    let found = object(value)?;
    only_keys(found, &["is_baby", "is_on_fire"], "entity flag")?;
    Ok(EntityFlags {
        baby: optional_flag(found, "is_baby")?,
        on_fire: optional_flag(found, "is_on_fire")?,
    })
}

fn component_pairs(value: &Value) -> Parsed<&'static [(&'static str, &'static str)]> {
    let mut pairs = Vec::new();
    for (component, wanted) in object(value)? {
        let Some(text) = wanted.as_str() else {
            return unsupported(format!("non exact component predicate {component}"));
        };
        pairs.push((leak_text(component.clone()), leak_text(text.to_owned())));
    }
    pairs.sort_unstable();
    Ok(leak_slice(pairs))
}

fn sub_predicate(key: &str, value: &Value) -> Parsed<EntitySubPredicate> {
    let map = object(value)?;
    match key {
        "type_specific/sheep" => {
            only_keys(map, &["sheared"], "sheep predicate")?;
            Ok(EntitySubPredicate::Sheep {
                sheared: optional_flag(map, "sheared")?,
            })
        }
        "type_specific/cube_mob" => {
            only_keys(map, &["size"], "cube mob predicate")?;
            Ok(EntitySubPredicate::CubeMob {
                size: map.get("size").map_or(
                    Ok(IntRange {
                        min: None,
                        max: None,
                    }),
                    int_range,
                )?,
            })
        }
        "type_specific/raider" => {
            only_keys(map, &["has_raid", "is_captain"], "raider predicate")?;
            Ok(EntitySubPredicate::Raider {
                has_raid: optional_flag(map, "has_raid")?,
                captain: optional_flag(map, "is_captain")?,
            })
        }
        _ => {
            only_keys(map, &["in_open_water"], "fishing hook predicate")?;
            Ok(EntitySubPredicate::FishingHook {
                in_open_water: optional_flag(map, "in_open_water")?,
            })
        }
    }
}

const SUB_KEYS: [&str; 4] = [
    "type_specific/sheep",
    "type_specific/cube_mob",
    "type_specific/raider",
    "type_specific/fishing_hook",
];

pub(super) fn entity_predicate(value: &Value) -> Parsed<EntityPredicate> {
    let map = object(value)?;
    let mut mainhand = None;
    let mut kind = None;
    let mut flags = EntityFlags {
        baby: None,
        on_fire: None,
    };
    let mut vehicle = None;
    let mut components: &'static [(&'static str, &'static str)] = &[];
    let mut sub = Vec::new();
    for (key, found) in map {
        match bare(key) {
            "entity_type" | "type" => kind = Some(holder_set(ENTITY_TYPE, found)?),
            "flags" => flags = entity_flags(found)?,
            "equipment" => {
                let equipment = object(found)?;
                only_keys(equipment, &["mainhand"], "equipment slot")?;
                mainhand = equipment.get("mainhand").map(item_predicate).transpose()?;
            }
            "vehicle" => vehicle = Some(leak(entity_predicate(found)?)),
            "components" => components = component_pairs(found)?,
            other if SUB_KEYS.contains(&other) => sub.push(sub_predicate(other, found)?),
            _ => return unsupported(format!("entity predicate key \"{key}\"")),
        }
    }
    sub.sort_by_key(|entry| match entry {
        EntitySubPredicate::Sheep { .. } => 0,
        EntitySubPredicate::CubeMob { .. } => 1,
        EntitySubPredicate::Raider { .. } => 2,
        EntitySubPredicate::FishingHook { .. } => 3,
    });
    Ok(EntityPredicate {
        kind,
        flags,
        mainhand,
        vehicle,
        components,
        sub: leak_slice(sub),
    })
}

pub(super) fn target(value: &Value) -> Parsed<EntityTarget> {
    match value.as_str() {
        Some("this") => Ok(EntityTarget::This),
        Some("attacker") => Ok(EntityTarget::Attacker),
        Some("direct_attacker") => Ok(EntityTarget::DirectAttacker),
        Some("attacking_player") => Ok(EntityTarget::AttackingPlayer),
        Some("target_entity") => Ok(EntityTarget::TargetEntity),
        Some("interacting_entity") => Ok(EntityTarget::InteractingEntity),
        _ => invalid(format!("unknown entity target {value}")),
    }
}

pub(super) fn damage_predicate(value: &Value) -> Parsed<DamageSourcePredicate> {
    let map = object(value)?;
    only_keys(
        map,
        &["tags", "source_entity", "direct_entity", "is_direct"],
        "damage source predicate",
    )?;
    let mut tags = Vec::new();
    if let Some(Value::Array(list)) = map.get("tags") {
        for entry in list {
            let entry = object(entry)?;
            let id = required(entry, "id")?;
            let text = id.as_str().unwrap_or_default();
            tags.push(TagPredicate {
                tag: leak_text(name(&Value::from(text.trim_start_matches('#')))?),
                expected: flag(required(entry, "expected")?)?,
            });
        }
    }
    Ok(DamageSourcePredicate {
        tags: leak_slice(tags),
        source_entity: map
            .get("source_entity")
            .map(|entity| entity_predicate(entity).map(leak))
            .transpose()?,
        direct_entity: map
            .get("direct_entity")
            .map(|entity| entity_predicate(entity).map(leak))
            .transpose()?,
        is_direct: optional_flag(map, "is_direct")?,
    })
}
