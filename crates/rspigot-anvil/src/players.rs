use std::path::{Path, PathBuf};

use rspigot_core::ports::{PlayerStore, ProgressFile, StoreError};
use rspigot_domain::player::PlayerId;
use rspigot_nbt::Compound;

use crate::chunk::DATA_VERSION;
use crate::error::AnvilError;
use crate::level::{read_gzip_nbt, write_gzip_nbt};
use crate::store::AnvilStore;

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

fn progress_file(world_root: &Path, player: PlayerId, file: ProgressFile) -> PathBuf {
    let folder = match file {
        ProgressFile::Stats => "stats",
        ProgressFile::Advancements => "advancements",
    };
    world_root
        .join(folder)
        .join(format!("{}.json", player.0.hyphenated()))
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
    fn load_progress(
        &self,
        player: PlayerId,
        file: ProgressFile,
    ) -> Result<Option<String>, StoreError> {
        match std::fs::read_to_string(progress_file(self.world_root(), player, file)) {
            Ok(text) => Ok(Some(text)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(StoreError::Io(error)),
        }
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
        let current_error = match read_gzip_nbt(&current) {
            Ok(Some(data)) => return Ok(Some(data)),
            Ok(None) => None,
            Err(error) => Some(error),
        };
        match read_gzip_nbt(&backup) {
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
        assert!(dir.path().join("stats").is_dir());
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
