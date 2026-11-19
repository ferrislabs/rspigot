use std::io;

use crate::arena::ArenaSettings;

pub const SETTINGS_FILE: &str = "arena.json";

pub fn settings_from(source: io::Result<String>) -> ArenaSettings {
    match source {
        Ok(json) => ArenaSettings::from_json(&json).unwrap_or_else(|error| {
            tracing::error!("{SETTINGS_FILE} is invalid, using the default arena: {error}");
            ArenaSettings::default()
        }),
        Err(error) if error.kind() == io::ErrorKind::NotFound => ArenaSettings::default(),
        Err(error) => {
            tracing::error!("could not read {SETTINGS_FILE}, using the default arena: {error}");
            ArenaSettings::default()
        }
    }
}

pub fn load_settings() -> ArenaSettings {
    settings_from(std::fs::read_to_string(SETTINGS_FILE))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_gives_the_default() {
        let settings = settings_from(Err(io::ErrorKind::NotFound.into()));
        assert_eq!(settings, ArenaSettings::default());
    }

    #[test]
    fn unreadable_file_gives_the_default() {
        let settings = settings_from(Err(io::ErrorKind::PermissionDenied.into()));
        assert_eq!(settings, ArenaSettings::default());
    }

    #[test]
    fn invalid_json_gives_the_default() {
        let settings = settings_from(Ok("{ nope".to_owned()));
        assert_eq!(settings, ArenaSettings::default());
    }

    #[test]
    fn valid_json_is_used() {
        let custom = ArenaSettings {
            world_name: "custom".to_owned(),
            ..ArenaSettings::default()
        };
        let json = serde_json::to_string(&custom).expect("serializes");
        assert_eq!(settings_from(Ok(json)).world_name, "custom");
    }
}
