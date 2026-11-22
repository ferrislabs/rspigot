use std::path::{Path, PathBuf};

use rspigot_core::ports::{PlayerStore, ProgressFile, StoreError};
use rspigot_domain::player::PlayerId;
use rspigot_nbt::Compound;

use crate::chunk::DATA_VERSION;
use crate::datafix::{self, DataKind};
use crate::error::AnvilError;
use crate::level::{optional_saved_data, read_gzip_nbt, update_saved_data, write_gzip_nbt};
use crate::store::AnvilStore;

fn upgrade_player(data: Option<Compound>) -> Result<Option<Compound>, AnvilError> {
    data.map(|data| {
        datafix::upgrade_checked(DataKind::Player, data).map(|upgraded| upgraded.compound)
    })
    .transpose()
}

fn player_file(world_root: &Path, player: PlayerId, extension: &str) -> PathBuf {
    world_root
        .join("players")
        .join("data")
        .join(format!("{}.{extension}", player.0.hyphenated()))
}

fn corrupt(player: PlayerId, error: &AnvilError) -> StoreError {
    StoreError::CorruptPlayer {
        player,
        reason: error.to_string(),
    }
}

fn progress_folder(file: ProgressFile) -> &'static str {
    match file {
        ProgressFile::Stats => "stats",
        ProgressFile::Advancements => "advancements",
    }
}

fn progress_file(world_root: &Path, player: PlayerId, file: ProgressFile) -> PathBuf {
    world_root
        .join("players")
        .join(progress_folder(file))
        .join(format!("{}.json", player.0.hyphenated()))
}

fn legacy_progress_file(world_root: &Path, player: PlayerId, file: ProgressFile) -> PathBuf {
    world_root
        .join(progress_folder(file))
        .join(format!("{}.json", player.0.hyphenated()))
}

fn read_optional(path: &Path) -> Result<Option<String>, StoreError> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(StoreError::Io(error)),
    }
}

fn stamp(json: &str) -> Result<String, StoreError> {
    let mut document: serde_json::Value =
        serde_json::from_str(json).map_err(|error| StoreError::Io(std::io::Error::other(error)))?;
    if let Some(object) = document.as_object_mut() {
        object.insert("DataVersion".to_owned(), DATA_VERSION.into());
    }
    serde_json::to_string_pretty(&document)
        .map_err(|error| StoreError::Io(std::io::Error::other(error)))
}

impl PlayerStore for AnvilStore {
    fn load_saved_data(&self, name: &str) -> Result<Option<Compound>, StoreError> {
        Ok(optional_saved_data(self.world_root(), name))
    }

    fn save_saved_data(&self, name: &str, data: &Compound) -> Result<(), StoreError> {
        update_saved_data(self.world_root(), name, |stored| {
            stored.clone_from(data);
            true
        })
        .map(drop)
        .map_err(AnvilError::into_store_io)
    }

    fn saved_data_namespaces(&self, leaf: &str) -> Result<Vec<String>, StoreError> {
        Ok(crate::level::saved_data_namespaces(self.world_root(), leaf))
    }

    fn load_progress(
        &self,
        player: PlayerId,
        file: ProgressFile,
    ) -> Result<Option<String>, StoreError> {
        if let Some(text) = read_optional(&progress_file(self.world_root(), player, file))? {
            return Ok(Some(text));
        }
        read_optional(&legacy_progress_file(self.world_root(), player, file))
    }

    fn save_progress(
        &self,
        player: PlayerId,
        file: ProgressFile,
        json: &str,
    ) -> Result<(), StoreError> {
        let path = progress_file(self.world_root(), player, file);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let temporary = path.with_extension("json.tmp");
        std::fs::write(&temporary, stamp(json)?)?;
        std::fs::rename(&temporary, &path)?;
        Ok(())
    }

    fn load_player(&self, player: PlayerId) -> Result<Option<Compound>, StoreError> {
        let current = player_file(self.world_root(), player, "dat");
        let backup = player_file(self.world_root(), player, "dat_old");
        let current_error = match read_gzip_nbt(&current).and_then(upgrade_player) {
            Ok(Some(data)) => return Ok(Some(data)),
            Ok(None) => None,
            Err(error) => Some(error),
        };
        match read_gzip_nbt(&backup).and_then(upgrade_player) {
            Ok(Some(data)) => {
                if let Some(error) = current_error {
                    tracing::warn!("Restored player data for {player} from its backup: {error}");
                }
                Ok(Some(data))
            }
            Ok(None) => current_error.map_or(Ok(None), |error| Err(corrupt(player, &error))),
            Err(error) => Err(corrupt(player, &error)),
        }
    }

    fn save_player(&self, player: PlayerId, data: &Compound) -> Result<(), StoreError> {
        let current = player_file(self.world_root(), player, "dat");
        let backup = player_file(self.world_root(), player, "dat_old");
        let mut stamped = data.clone();
        stamped.insert("DataVersion", DATA_VERSION);
        write_gzip_nbt(&current, &stamped, Some(&backup)).map_err(AnvilError::into_store_io)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::DimensionLayout;
    use crate::testutil::TempDir;

    fn store(dir: &TempDir) -> AnvilStore {
        AnvilStore::open(dir.path(), DimensionLayout::Spigot).expect("opens")
    }

    fn sample(level: i32) -> Compound {
        Compound::new().with("XpLevel", level)
    }

    #[test]
    fn progress_files_round_trip_with_a_data_version() {
        let dir = TempDir::new("player-progress");
        let store = store(&dir);
        let alice = PlayerId::offline("Alice");
        assert_eq!(
            store
                .load_progress(alice, ProgressFile::Stats)
                .expect("loads"),
            None
        );
        store
            .save_progress(alice, ProgressFile::Stats, "{\"stats\":{}}")
            .expect("saves");
        let text = store
            .load_progress(alice, ProgressFile::Stats)
            .expect("loads")
            .expect("a file");
        let document: serde_json::Value = serde_json::from_str(&text).expect("json");
        assert_eq!(document["DataVersion"], DATA_VERSION);
        assert!(document["stats"].is_object());
        assert!(dir.path().join("players").join("stats").is_dir());
        assert!(!dir.path().join("stats").exists());
    }

    #[test]
    fn progress_falls_back_to_the_legacy_world_folders() {
        let dir = TempDir::new("player-progress-legacy");
        let store = store(&dir);
        let alice = PlayerId::offline("Alice");
        for (file, folder) in [
            (ProgressFile::Stats, "stats"),
            (ProgressFile::Advancements, "advancements"),
        ] {
            let path = legacy_progress_file(dir.path(), alice, file);
            std::fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
            std::fs::write(&path, format!("{{\"old\":\"{folder}\"}}")).expect("writes");
            let text = store.load_progress(alice, file).expect("loads");
            assert_eq!(text, Some(format!("{{\"old\":\"{folder}\"}}")));
            store
                .save_progress(alice, file, "{\"new\":1}")
                .expect("saves");
            let text = store
                .load_progress(alice, file)
                .expect("loads")
                .expect("present");
            assert!(text.contains("\"new\""));
        }
    }

    #[test]
    fn saved_data_files_live_under_data_minecraft_and_round_trip() {
        let dir = TempDir::new("saved-data");
        let store = store(&dir);
        assert_eq!(store.load_saved_data("maps/3").expect("loads"), None);
        store
            .save_saved_data("maps/3", &Compound::new().with("xCenter", 64))
            .expect("saves");
        store
            .save_saved_data("maps/last_id", &Compound::new().with("map", 3))
            .expect("saves");
        let path = dir.path().join("data").join("minecraft").join("maps");
        assert!(path.join("3.dat").is_file());
        assert!(path.join("last_id.dat").is_file());
        let root = read_gzip_nbt(&path.join("3.dat"))
            .expect("reads")
            .expect("present");
        assert_eq!(root.get_i32("DataVersion"), Some(DATA_VERSION));
        assert_eq!(
            root.get_compound("data").and_then(|d| d.get_i32("xCenter")),
            Some(64)
        );
        let loaded = store
            .load_saved_data("maps/3")
            .expect("loads")
            .expect("present");
        assert_eq!(loaded.get_i32("xCenter"), Some(64));
        let index = store
            .load_saved_data("maps/last_id")
            .expect("loads")
            .expect("present");
        assert_eq!(index.get_i32("map"), Some(3));
    }

    #[test]
    fn namespaced_saved_data_lives_under_its_own_data_folder() {
        let dir = TempDir::new("saved-data-namespace");
        let store = store(&dir);
        store
            .save_saved_data("pack:command_storage", &Compound::new().with("a", 1))
            .expect("saves");
        assert!(
            dir.path()
                .join("data")
                .join("pack")
                .join("command_storage.dat")
                .is_file()
        );
        assert_eq!(
            store
                .saved_data_namespaces("command_storage")
                .expect("lists"),
            vec!["pack".to_owned()]
        );
        let loaded = store
            .load_saved_data("pack:command_storage")
            .expect("loads")
            .expect("present");
        assert_eq!(loaded.get_i32("a"), Some(1));
    }

    #[test]
    fn a_missing_file_loads_as_none() {
        let dir = TempDir::new("player-missing");
        assert_eq!(
            store(&dir)
                .load_player(PlayerId::offline("Alice"))
                .expect("loads"),
            None
        );
    }

    #[test]
    fn saved_data_loads_back_and_is_stamped_with_the_data_version() {
        let dir = TempDir::new("player-roundtrip");
        let store = store(&dir);
        let id = PlayerId::offline("Alice");
        store.save_player(id, &sample(7)).expect("saves");
        let loaded = store.load_player(id).expect("loads").expect("present");
        assert_eq!(loaded.get_i32("XpLevel"), Some(7));
        assert_eq!(loaded.get_i32("DataVersion"), Some(DATA_VERSION));
    }

    #[test]
    fn the_file_is_a_gzip_under_players_data() {
        let dir = TempDir::new("player-layout");
        let id = PlayerId::offline("Alice");
        store(&dir).save_player(id, &sample(1)).expect("saves");
        let path = player_file(dir.path(), id, "dat");
        assert!(path.starts_with(dir.path().join("players").join("data")));
        let bytes = std::fs::read(&path).expect("file exists");
        assert!(bytes.starts_with(&[0x1f, 0x8b]));
    }

    #[test]
    fn the_previous_save_is_kept_as_the_backup() {
        let dir = TempDir::new("player-backup");
        let store = store(&dir);
        let id = PlayerId::offline("Alice");
        store.save_player(id, &sample(1)).expect("saves");
        store.save_player(id, &sample(2)).expect("saves");
        let backup = read_gzip_nbt(&player_file(dir.path(), id, "dat_old"))
            .expect("reads")
            .expect("present");
        assert_eq!(backup.get_i32("XpLevel"), Some(1));
    }

    #[test]
    fn a_corrupt_file_falls_back_to_the_backup() {
        let dir = TempDir::new("player-fallback");
        let store = store(&dir);
        let id = PlayerId::offline("Alice");
        store.save_player(id, &sample(1)).expect("saves");
        store.save_player(id, &sample(2)).expect("saves");
        std::fs::write(player_file(dir.path(), id, "dat"), b"\x1f\x8bbroken").expect("writes");
        let loaded = store.load_player(id).expect("loads").expect("present");
        assert_eq!(loaded.get_i32("XpLevel"), Some(1));
    }

    #[test]
    fn corrupt_data_without_a_backup_is_an_error() {
        let dir = TempDir::new("player-corrupt");
        let store = store(&dir);
        let id = PlayerId::offline("Alice");
        store.save_player(id, &sample(1)).expect("saves");
        std::fs::write(player_file(dir.path(), id, "dat"), b"\x1f\x8bbroken").expect("writes");
        assert!(matches!(
            store.load_player(id),
            Err(StoreError::CorruptPlayer { .. })
        ));
    }

    #[test]
    fn players_do_not_share_files() {
        let dir = TempDir::new("player-separate");
        let store = store(&dir);
        store
            .save_player(PlayerId::offline("Alice"), &sample(1))
            .expect("saves");
        assert_eq!(
            store.load_player(PlayerId::offline("Bob")).expect("loads"),
            None
        );
    }
}
