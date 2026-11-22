use std::collections::BTreeMap;
use std::sync::{Arc, PoisonError, RwLock};

use rspigot_nbt::{Compound, Tag};
use serde_json::Value;

use crate::ports::EnchantmentEntry;

pub const SYNCED_DIRECTORIES: &[&str] = &[
    "dimension_type",
    "chat_type",
    "trim_pattern",
    "trim_material",
    "wolf_variant",
    "wolf_sound_variant",
    "pig_variant",
    "pig_sound_variant",
    "frog_variant",
    "cat_variant",
    "cat_sound_variant",
    "cow_sound_variant",
    "cow_variant",
    "chicken_sound_variant",
    "chicken_variant",
    "zombie_nautilus_variant",
    "painting_variant",
    "sulfur_cube_archetype",
    "damage_type",
    "banner_pattern",
    "enchantment",
    "jukebox_song",
    "instrument",
    "dialog",
    "world_clock",
    "timeline",
    "decorated_pot_pattern",
];

pub const RUNTIME_DIRECTORIES: &[&str] = &[
    "trial_spawner",
    "villager_trade",
    "trade_set",
    "tags/villager_trade",
    "tags/painting_variant",
];

#[derive(Debug, Clone, PartialEq)]
pub struct OverlayEntry {
    pub id: String,
    pub json: String,
    pub data: Option<Compound>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct RegistryOverlay {
    registries: BTreeMap<String, Vec<OverlayEntry>>,
}

impl RegistryOverlay {
    pub fn insert(&mut self, registry: &str, entry: OverlayEntry) {
        let entries = self.registries.entry(registry.to_owned()).or_default();
        match entries.iter_mut().find(|known| known.id == entry.id) {
            Some(known) => *known = entry,
            None => entries.push(entry),
        }
    }

    pub fn entries(&self, registry: &str) -> &[OverlayEntry] {
        self.registries.get(registry).map_or(&[], Vec::as_slice)
    }

    pub fn get(&self, registry: &str, id: &str) -> Option<&OverlayEntry> {
        self.entries(registry).iter().find(|entry| entry.id == id)
    }

    pub fn is_empty(&self) -> bool {
        self.registries.values().all(Vec::is_empty)
    }

    pub fn registries(&self) -> impl Iterator<Item = &str> {
        self.registries.keys().map(String::as_str)
    }
}

static PUBLISHED: RwLock<Option<Arc<RegistryOverlay>>> = RwLock::new(None);

pub fn publish(overlay: Option<Arc<RegistryOverlay>>) {
    *PUBLISHED.write().unwrap_or_else(PoisonError::into_inner) = overlay;
}

pub fn published() -> Option<Arc<RegistryOverlay>> {
    PUBLISHED
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}

pub fn enchantment_entries() -> Vec<EnchantmentEntry> {
    let Some(overlay) = published() else {
        return Vec::new();
    };
    overlay
        .entries("minecraft:enchantment")
        .iter()
        .filter_map(|entry| {
            Some(EnchantmentEntry {
                id: rspigot_domain::Identifier::parse(&entry.id).ok()?,
                json: entry.json.clone(),
            })
        })
        .collect()
}

pub fn json_to_nbt(value: &Value) -> Tag {
    match value {
        Value::Null => Tag::Byte(0),
        Value::Bool(flag) => Tag::Byte(i8::from(*flag)),
        Value::Number(number) => match number.as_i64() {
            Some(whole) => i32::try_from(whole).map_or(Tag::Long(whole), Tag::Int),
            None => Tag::Double(number.as_f64().unwrap_or_default()),
        },
        Value::String(text) => Tag::String(text.clone()),
        Value::Array(items) => Tag::List(items.iter().map(json_to_nbt).collect()),
        Value::Object(map) => {
            let mut compound = Compound::new();
            for (key, item) in map {
                compound.insert(key.clone(), json_to_nbt(item));
            }
            Tag::Compound(compound)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_becomes_nbt_with_integers_doubles_and_nested_values() {
        let value: Value =
            serde_json::from_str(r#"{"a":1,"b":2.5,"c":true,"d":"x","e":[1,2],"f":{"g":3}}"#)
                .unwrap();
        let Tag::Compound(compound) = json_to_nbt(&value) else {
            panic!()
        };
        assert_eq!(compound.get_i32("a"), Some(1));
        assert_eq!(compound.get_f64("b"), Some(2.5));
        assert_eq!(compound.get_bool("c"), Some(true));
        assert_eq!(compound.get_str("d"), Some("x"));
        assert_eq!(compound.get_list("e").map(<[Tag]>::len), Some(2));
        assert_eq!(compound.get_compound("f").unwrap().get_i32("g"), Some(3));
    }

    #[test]
    fn inserting_the_same_id_replaces_the_entry() {
        let mut overlay = RegistryOverlay::default();
        let entry = |json: &str| OverlayEntry {
            id: "t:a".to_owned(),
            json: json.to_owned(),
            data: None,
        };
        overlay.insert("minecraft:dialog", entry("1"));
        overlay.insert("minecraft:dialog", entry("2"));
        assert_eq!(overlay.entries("minecraft:dialog").len(), 1);
        assert_eq!(overlay.get("minecraft:dialog", "t:a").unwrap().json, "2");
    }
}
