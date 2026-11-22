use std::collections::BTreeMap;
use std::sync::Arc;

use rspigot_data::advancements::Advancement;
use rspigot_data::trading::TradeOverlay;
use rspigot_data::trial_spawner::ConfigOverlay;
use rspigot_domain::entity::hanging::PaintingOverlay;
use rspigot_domain::jukebox::SongOverlay;
use serde_json::Value;

use super::registry_overlay::{
    OverlayEntry, RUNTIME_DIRECTORIES, RegistryOverlay, SYNCED_DIRECTORIES, json_to_nbt,
};
use super::{advancement_files, media_files, trade_files, trial_files};
use crate::ports::{PackExtras, PackFile};

#[derive(Debug, Clone, Default)]
pub struct Registries {
    overlay: Arc<RegistryOverlay>,
    trades: Arc<TradeOverlay>,
    trials: Arc<ConfigOverlay>,
    songs: Arc<SongOverlay>,
    paintings: Arc<PaintingOverlay>,
    advancements: Option<&'static [Advancement]>,
    recipes: Vec<PackFile>,
}

fn newest<'a>(files: impl Iterator<Item = &'a PackFile>) -> Vec<&'a PackFile> {
    let mut by_id: BTreeMap<&str, &PackFile> = BTreeMap::new();
    for file in files {
        by_id.insert(file.id.as_str(), file);
    }
    by_id.into_values().collect()
}

impl Registries {
    pub fn build(packs: &[&PackExtras]) -> Self {
        let mut overlay = RegistryOverlay::default();
        for pack in packs {
            for entry in &pack.registries {
                let known = SYNCED_DIRECTORIES.contains(&entry.registry.as_str());
                if !known && !RUNTIME_DIRECTORIES.contains(&entry.registry.as_str()) {
                    tracing::warn!(
                        "Skipping {} of unknown registry {} in pack {}",
                        entry.file.id,
                        entry.registry,
                        pack.name
                    );
                    continue;
                }
                let value: Value = match serde_json::from_str(&entry.file.text) {
                    Ok(value) => value,
                    Err(error) => {
                        tracing::warn!(
                            "Failed to load {} ({}) from pack {}: {error}",
                            entry.file.id,
                            entry.registry,
                            pack.name
                        );
                        continue;
                    }
                };
                let data = match (known, json_to_nbt(&value)) {
                    (true, rspigot_nbt::Tag::Compound(compound)) => Some(compound),
                    (true, _) => {
                        tracing::warn!(
                            "Skipping {} ({}) from pack {}: not an object",
                            entry.file.id,
                            entry.registry,
                            pack.name
                        );
                        continue;
                    }
                    (false, _) => None,
                };
                overlay.insert(
                    &format!("minecraft:{}", entry.registry),
                    OverlayEntry {
                        id: entry.file.id.to_string(),
                        json: entry.file.text.clone(),
                        data,
                    },
                );
            }
        }
        let advancements = newest(packs.iter().flat_map(|pack| pack.advancements.iter()));
        let recipes = newest(packs.iter().flat_map(|pack| pack.recipes.iter()));
        Self {
            trades: Arc::new(trade_files::build(&overlay)),
            trials: Arc::new(trial_files::build(&overlay)),
            songs: Arc::new(media_files::songs(&overlay)),
            paintings: Arc::new(media_files::paintings(&overlay)),
            overlay: Arc::new(overlay),
            advancements: advancement_files::merge(&advancements),
            recipes: recipes.into_iter().cloned().collect(),
        }
    }

    pub fn overlay(&self) -> &Arc<RegistryOverlay> {
        &self.overlay
    }

    pub fn trades(&self) -> &Arc<TradeOverlay> {
        &self.trades
    }

    pub fn trials(&self) -> &Arc<ConfigOverlay> {
        &self.trials
    }

    pub fn songs(&self) -> &Arc<SongOverlay> {
        &self.songs
    }

    pub fn paintings(&self) -> &Arc<PaintingOverlay> {
        &self.paintings
    }

    pub fn advancements(&self) -> Option<&'static [Advancement]> {
        self.advancements
    }

    pub fn recipe_files(&self) -> &[PackFile] {
        &self.recipes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::PackRegistryFile;

    fn file(id: &str, text: &str) -> PackFile {
        PackFile {
            id: rspigot_domain::Identifier::parse(id).unwrap(),
            text: text.to_owned(),
        }
    }

    #[test]
    fn registries_load_with_later_packs_overriding_and_bad_files_skipped() {
        let entry = |registry: &str, id: &str, text: &str| PackRegistryFile {
            registry: registry.to_owned(),
            file: file(id, text),
        };
        let first = PackExtras {
            name: "a".to_owned(),
            registries: vec![
                entry("trim_pattern", "t:p", r#"{"v":1}"#),
                entry("trial_spawner", "t:s", r#"{"x":1}"#),
                entry("nope", "t:n", "{}"),
                entry("dialog", "t:bad", "{"),
            ],
            ..PackExtras::default()
        };
        let second = PackExtras {
            name: "b".to_owned(),
            registries: vec![entry("trim_pattern", "t:p", r#"{"v":2}"#)],
            ..PackExtras::default()
        };
        let built = Registries::build(&[&first, &second]);
        let pattern = built
            .overlay()
            .get("minecraft:trim_pattern", "t:p")
            .unwrap();
        assert_eq!(pattern.data.as_ref().unwrap().get_i32("v"), Some(2));
        let spawner = built
            .overlay()
            .get("minecraft:trial_spawner", "t:s")
            .unwrap();
        assert!(spawner.data.is_none());
        assert!(built.overlay().get("minecraft:dialog", "t:bad").is_none());
        assert_eq!(built.overlay().registries().count(), 2);
    }
}
