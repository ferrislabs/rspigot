use std::collections::{BTreeMap, BTreeSet};

use rspigot_data::registries;
use rspigot_data::tags::{TagOverlay, static_tag};

use crate::functions::{Reference, TagEntry, TagFile};
use crate::ports::PackContents;

pub const SUPPORTED_REGISTRIES: &[&str] = &[
    "block",
    "item",
    "entity_type",
    "fluid",
    "game_event",
    "damage_type",
    "enchantment",
    "worldgen/biome",
    "point_of_interest_type",
];

#[derive(Default)]
struct Definition {
    replaced: bool,
    entries: Vec<TagEntry>,
}

type Definitions = BTreeMap<String, Definition>;

pub fn merge(packs: &[&PackContents]) -> TagOverlay {
    let mut by_registry: BTreeMap<String, Definitions> = BTreeMap::new();
    for pack in packs {
        for tag in &pack.tags {
            let registry = format!("minecraft:{}", tag.registry);
            if registries::static_entries(&registry).is_none() {
                tracing::warn!(
                    "Skipping tag {} of unknown registry {} in pack {}",
                    tag.file.id,
                    tag.registry,
                    pack.name
                );
                continue;
            }
            match TagFile::parse(&tag.file.text) {
                Ok(file) => {
                    let definition = by_registry
                        .entry(registry)
                        .or_default()
                        .entry(tag.file.id.to_string())
                        .or_default();
                    if file.replace {
                        definition.replaced = true;
                        definition.entries.clear();
                    }
                    definition.entries.extend(file.entries);
                }
                Err(error) => tracing::warn!(
                    "Failed to load tag {} ({}) from pack {}: {error}",
                    tag.file.id,
                    tag.registry,
                    pack.name
                ),
            }
        }
    }
    let mut overlay = TagOverlay::default();
    for (registry, definitions) in &by_registry {
        for name in definitions.keys() {
            let mut stack = vec![name.clone()];
            match resolve(registry, name, definitions, &mut stack) {
                Ok(ids) => {
                    let ids: Vec<u32> = ids.into_iter().collect();
                    overlay.insert(registry, name, Vec::leak(ids));
                }
                Err(missing) => tracing::warn!(
                    "Couldn't load tag {name} of {registry} as it is missing following references: {missing}"
                ),
            }
        }
    }
    overlay
}

fn resolve(
    registry: &str,
    name: &str,
    definitions: &Definitions,
    stack: &mut Vec<String>,
) -> Result<BTreeSet<u32>, String> {
    let Some(definition) = definitions.get(name) else {
        return static_tag(registry, name)
            .map(|ids| ids.iter().copied().collect())
            .ok_or_else(|| format!("#{name}"));
    };
    let mut ids: BTreeSet<u32> = if definition.replaced {
        BTreeSet::new()
    } else {
        static_tag(registry, name)
            .map(|ids| ids.iter().copied().collect())
            .unwrap_or_default()
    };
    let mut missing = Vec::new();
    for entry in &definition.entries {
        match &entry.reference {
            Reference::Function(id) => match registries::static_index(registry, id.as_str()) {
                Some(index) => {
                    ids.insert(index);
                }
                None if entry.required => missing.push(id.to_string()),
                None => {}
            },
            Reference::Tag(nested) => {
                let nested = nested.to_string();
                let known =
                    definitions.contains_key(&nested) || static_tag(registry, &nested).is_some();
                let resolved = if stack.contains(&nested) || !known {
                    Err(format!("#{nested}"))
                } else {
                    stack.push(nested.clone());
                    let resolved = resolve(registry, &nested, definitions, stack);
                    stack.pop();
                    resolved.map_err(|_| format!("#{nested}"))
                };
                match resolved {
                    Ok(found) => ids.extend(found),
                    Err(text) if entry.required => missing.push(text),
                    Err(_) => {}
                }
            }
        }
    }
    if missing.is_empty() {
        Ok(ids)
    } else {
        Err(missing.join(", "))
    }
}
