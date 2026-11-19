use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::domain::TeamId;

const DEFAULT_WORLD_NAME: &str = "sheepwars";
const DEFAULT_PLATFORM_BLOCK: &str = "minecraft:quartz_block";
const DEFAULT_PLATFORM_WIDTH: i32 = 9;
const DEFAULT_PLATFORM_DEPTH: i32 = 9;
const DEFAULT_PLATFORM_THICKNESS: i32 = 3;
const DEFAULT_TOP_Y: i32 = 64;
const DEFAULT_RED_CENTER_Z: i32 = -24;
const DEFAULT_BLUE_CENTER_Z: i32 = 23;
const DEFAULT_DISPENSER_OFFSET: i32 = 3;
const DEFAULT_LOBBY_Y: f64 = 100.0;
const DEFAULT_LOBBY_PLATFORM_SIZE: i32 = 9;
const DEFAULT_LOBBY_PLATFORM_THICKNESS: i32 = 1;
const DEFAULT_LOBBY_PLATFORM_BLOCK: &str = "minecraft:smooth_stone";
const DEFAULT_VOID_FALL_Y: f64 = 20.0;
const BLOCK_CENTER: f64 = 0.5;

#[derive(Debug, Error)]
pub enum ArenaError {
    #[error("invalid arena settings: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("the {0:?} platform has no volume")]
    EmptyPlatform(TeamId),
    #[error("the lobby platform has no volume")]
    EmptyLobbyPlatform,
    #[error("unknown block `{0}`")]
    UnknownBlock(String),
    #[error("could not create the arena world: {0}")]
    World(#[from] rspigot_plugin::Error),
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockPoint {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Platform {
    pub center: BlockPoint,
    pub width: i32,
    pub depth: i32,
    pub thickness: i32,
    pub block: String,
}

impl Platform {
    const fn is_empty(&self) -> bool {
        self.width <= 0 || self.depth <= 0 || self.thickness <= 0
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TeamSite {
    pub platform: Platform,
    pub spawn: Point,
    pub dispenser: BlockPoint,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArenaSettings {
    pub world_name: String,
    pub lobby: Point,
    pub lobby_platform: Platform,
    pub void_fall_y: f64,
    pub red: TeamSite,
    pub blue: TeamSite,
}

impl ArenaSettings {
    pub fn from_json(json: &str) -> Result<Self, ArenaError> {
        let settings: Self = serde_json::from_str(json)?;
        for team in TeamId::ALL {
            if settings.site(team).platform.is_empty() {
                return Err(ArenaError::EmptyPlatform(team));
            }
        }
        if settings.lobby_platform.is_empty() {
            return Err(ArenaError::EmptyLobbyPlatform);
        }
        Ok(settings)
    }

    pub const fn site(&self, team: TeamId) -> &TeamSite {
        match team {
            TeamId::Red => &self.red,
            TeamId::Blue => &self.blue,
        }
    }
}

fn default_site(center_z: i32, dispenser_z: i32) -> TeamSite {
    TeamSite {
        platform: Platform {
            center: BlockPoint {
                x: 0,
                y: DEFAULT_TOP_Y,
                z: center_z,
            },
            width: DEFAULT_PLATFORM_WIDTH,
            depth: DEFAULT_PLATFORM_DEPTH,
            thickness: DEFAULT_PLATFORM_THICKNESS,
            block: DEFAULT_PLATFORM_BLOCK.to_owned(),
        },
        spawn: Point {
            x: BLOCK_CENTER,
            y: f64::from(DEFAULT_TOP_Y + 1),
            z: f64::from(center_z) + BLOCK_CENTER,
        },
        dispenser: BlockPoint {
            x: 0,
            y: DEFAULT_TOP_Y + 1,
            z: dispenser_z,
        },
    }
}

impl Default for ArenaSettings {
    fn default() -> Self {
        Self {
            world_name: DEFAULT_WORLD_NAME.to_owned(),
            lobby: Point {
                x: BLOCK_CENTER,
                y: DEFAULT_LOBBY_Y,
                z: BLOCK_CENTER,
            },
            lobby_platform: Platform {
                center: BlockPoint {
                    x: 0,
                    y: DEFAULT_LOBBY_Y as i32 - 1,
                    z: 0,
                },
                width: DEFAULT_LOBBY_PLATFORM_SIZE,
                depth: DEFAULT_LOBBY_PLATFORM_SIZE,
                thickness: DEFAULT_LOBBY_PLATFORM_THICKNESS,
                block: DEFAULT_LOBBY_PLATFORM_BLOCK.to_owned(),
            },
            void_fall_y: DEFAULT_VOID_FALL_Y,
            red: default_site(
                DEFAULT_RED_CENTER_Z,
                DEFAULT_RED_CENTER_Z + DEFAULT_DISPENSER_OFFSET,
            ),
            blue: default_site(
                DEFAULT_BLUE_CENTER_Z,
                DEFAULT_BLUE_CENTER_Z - DEFAULT_DISPENSER_OFFSET,
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_settings_round_trip_through_json() {
        let settings = ArenaSettings::default();
        let json = serde_json::to_string(&settings).expect("serialize");
        assert_eq!(ArenaSettings::from_json(&json).expect("parse"), settings);
    }

    #[test]
    fn bad_json_gives_a_parse_error() {
        assert!(matches!(
            ArenaSettings::from_json("{not json"),
            Err(ArenaError::Parse(_))
        ));
    }

    #[test]
    fn missing_field_gives_a_parse_error() {
        assert!(matches!(
            ArenaSettings::from_json("{}"),
            Err(ArenaError::Parse(_))
        ));
    }

    #[test]
    fn empty_platform_is_rejected() {
        let mut settings = ArenaSettings::default();
        settings.blue.platform.width = 0;
        let json = serde_json::to_string(&settings).expect("serialize");
        assert!(matches!(
            ArenaSettings::from_json(&json),
            Err(ArenaError::EmptyPlatform(TeamId::Blue))
        ));
    }

    #[test]
    fn empty_lobby_platform_is_rejected() {
        let mut settings = ArenaSettings::default();
        settings.lobby_platform.thickness = 0;
        let json = serde_json::to_string(&settings).expect("serialize");
        assert!(matches!(
            ArenaSettings::from_json(&json),
            Err(ArenaError::EmptyLobbyPlatform)
        ));
    }
}
