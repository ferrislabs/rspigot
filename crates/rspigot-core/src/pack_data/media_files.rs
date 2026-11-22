use rspigot_domain::entity::hanging::{self, PaintingOverlay};
use rspigot_domain::jukebox::{Song, SongOverlay};
use serde_json::Value;

use super::registry_overlay::RegistryOverlay;

const SONGS: &str = "minecraft:jukebox_song";
const PAINTINGS: &str = "minecraft:painting_variant";
const PAINTING_TAGS: &str = "minecraft:tags/painting_variant";
const PLACEABLE: &str = "minecraft:placeable";

fn leak(text: &str) -> &'static str {
    Box::leak(text.to_owned().into_boxed_str())
}

pub fn songs(registry: &RegistryOverlay) -> SongOverlay {
    let mut overlay = SongOverlay::default();
    for entry in registry.entries(SONGS) {
        let Ok(value) = serde_json::from_str::<Value>(&entry.json) else {
            continue;
        };
        let (Some(seconds), Some(comparator)) = (
            value.get("length_in_seconds").and_then(Value::as_f64),
            value.get("comparator_output").and_then(Value::as_i64),
        ) else {
            tracing::warn!(
                "Jukebox song {} needs a length and a comparator output",
                entry.id
            );
            continue;
        };
        overlay.insert(Song {
            name: leak(&entry.id),
            seconds: seconds as f32,
            comparator: i32::try_from(comparator).unwrap_or(0),
        });
    }
    overlay
}

pub fn paintings(registry: &RegistryOverlay) -> PaintingOverlay {
    let mut overlay = PaintingOverlay::default();
    for entry in registry.entries(PAINTINGS) {
        let Ok(value) = serde_json::from_str::<Value>(&entry.json) else {
            continue;
        };
        let size = |key: &str| {
            value
                .get(key)
                .and_then(Value::as_i64)
                .and_then(|number| i32::try_from(number).ok())
        };
        let (Some(width), Some(height)) = (size("width"), size("height")) else {
            tracing::warn!("Painting variant {} needs a width and a height", entry.id);
            continue;
        };
        let bare = entry.id.strip_prefix("minecraft:").unwrap_or(&entry.id);
        match hanging::PAINTINGS
            .iter()
            .position(|(name, _, _)| *name == bare)
        {
            Some(index) => overlay.resized.push((index, width, height)),
            None => overlay.extras.push((leak(bare), width, height)),
        }
    }
    let listed = registry
        .get(PAINTING_TAGS, PLACEABLE)
        .and_then(|entry| serde_json::from_str::<Value>(&entry.json).ok());
    let values = listed
        .as_ref()
        .and_then(|file| file.get("values"))
        .and_then(Value::as_array);
    for name in values
        .map_or(&[][..], Vec::as_slice)
        .iter()
        .filter_map(Value::as_str)
    {
        let bare = name.strip_prefix("minecraft:").unwrap_or(name);
        let found = overlay
            .extras
            .iter()
            .position(|(candidate, _, _)| *candidate == bare)
            .map(|index| hanging::PAINTINGS.len() + index);
        if let Some(id) = found.and_then(|id| u32::try_from(id).ok()) {
            overlay.placeable.push(id);
        }
    }
    overlay
}

#[cfg(test)]
mod tests {
    use super::super::registry_overlay::OverlayEntry;
    use super::*;

    fn entry(id: &str, json: &str) -> OverlayEntry {
        OverlayEntry {
            id: id.to_owned(),
            json: json.to_owned(),
            data: None,
        }
    }

    #[test]
    fn songs_and_paintings_load_with_overrides_and_placeable_extras() {
        let mut registry = RegistryOverlay::default();
        registry.insert(
            SONGS,
            entry("t:s", r#"{"length_in_seconds":5.0,"comparator_output":9}"#),
        );
        registry.insert(SONGS, entry("t:bad", r#"{}"#));
        registry.insert(PAINTINGS, entry("t:wide", r#"{"width":4,"height":2}"#));
        registry.insert(
            PAINTINGS,
            entry("minecraft:alban", r#"{"width":2,"height":3}"#),
        );
        registry.insert(PAINTING_TAGS, entry(PLACEABLE, r#"{"values":["t:wide"]}"#));
        let songs = songs(&registry);
        assert!(!songs.is_empty());
        let paintings = paintings(&registry);
        assert_eq!(paintings.extras, [("t:wide", 4, 2)]);
        assert_eq!(paintings.resized, [(0, 2, 3)]);
        assert_eq!(
            paintings.placeable,
            [u32::try_from(hanging::PAINTINGS.len()).unwrap()]
        );
    }
}
