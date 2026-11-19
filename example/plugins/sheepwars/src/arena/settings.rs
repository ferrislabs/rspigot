use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::domain::TeamId;

const FALLBACK_WORLD_NAME: &str = "sheepwars";
const MAP_WORLD_NAME: &str = "sheepwars_steampunk";
const MAP_NAME: &str = "steampunk";
const MAP_SOURCE_DIR: &str = "../../server/maps/sheepwars/steampunk";
const MAP_SERVER_DIR: &str = "../../server";
const MAP_RED_SPAWN: BlockPoint = BlockPoint {
    x: -95,
    y: 132,
    z: -8,
};
const MAP_BLUE_SPAWN: BlockPoint = BlockPoint {
    x: -47,
    y: 132,
    z: 26,
};
const MAP_LOBBY: BlockPoint = BlockPoint {
    x: -47,
    y: 156,
    z: 24,
};
const MAP_DISPENSER_OFFSET: f64 = 2.0;
pub const MAP_VOID_FALL_Y: f64 = 0.0;
const DEFAULT_PLATFORM_BLOCK: &str = "minecraft:quartz_block";
const DEFAULT_PLATFORM_WIDTH: i32 = 25;
const DEFAULT_PLATFORM_LENGTH: i32 = 70;
const DEFAULT_PLATFORM_THICKNESS: i32 = 3;
const DEFAULT_TOP_Y: i32 = 64;
const DEFAULT_SPAWN_INSET: i32 = 8;
const DEFAULT_DISPENSER_OFFSET: i32 = 3;
const FALLBACK_VOID_FALL_Y: f64 = 20.0;
const BLOCK_CENTER: f64 = 0.5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Spot {
    Lobby,
    Spawn(TeamId),
    Dispenser(TeamId),
}

#[derive(Debug, Error)]
pub enum ArenaError {
    #[error("invalid arena settings: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("the platform has no volume")]
    EmptyPlatform,
    #[error("{0:?} is not above the platform")]
    OffPlatform(Spot),
    #[error("`{0}` is not a plain map folder name")]
    InvalidMapName(String),
    #[error("{0}")]
    Install(#[from] super::install::InstallError),
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

    pub const fn min_x(&self) -> i32 {
        self.center.x - self.width / 2
    }

    pub const fn min_z(&self) -> i32 {
        self.center.z - self.depth / 2
    }

    pub const fn min_y(&self) -> i32 {
        self.center.y - self.thickness + 1
    }

    fn is_above(&self, x: f64, y: f64, z: f64) -> bool {
        let (x, z) = (x.floor(), z.floor());
        x >= f64::from(self.min_x())
            && x < f64::from(self.min_x() + self.width)
            && z >= f64::from(self.min_z())
            && z < f64::from(self.min_z() + self.depth)
            && y >= f64::from(self.center.y + 1)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MapSettings {
    pub name: String,
    pub source_dir: PathBuf,
    pub server_dir: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeamSite {
    pub spawn: Point,
    pub dispenser: BlockPoint,
}

fn fallback_platform_value() -> Platform {
    ArenaSettings::fallback_platform().platform
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArenaSettings {
    pub world_name: String,
    pub lobby: Point,
    pub void_fall_y: f64,
    #[serde(default)]
    pub map: Option<MapSettings>,
    #[serde(default = "fallback_platform_value")]
    pub platform: Platform,
    pub red: TeamSite,
    pub blue: TeamSite,
}

impl ArenaSettings {
    pub fn from_json(json: &str) -> Result<Self, ArenaError> {
        let settings: Self = serde_json::from_str(json)?;
        settings.validate()?;
        Ok(settings)
    }

    fn validate(&self) -> Result<(), ArenaError> {
        if let Some(map) = &self.map {
            return map.validate();
        }
        if self.platform.is_empty() {
            return Err(ArenaError::EmptyPlatform);
        }
        self.require_above(Spot::Lobby, self.lobby)?;
        for team in TeamId::ALL {
            let site = self.site(team);
            self.require_above(Spot::Spawn(team), site.spawn)?;
            let dispenser = Point {
                x: f64::from(site.dispenser.x),
                y: f64::from(site.dispenser.y),
                z: f64::from(site.dispenser.z),
            };
            self.require_above(Spot::Dispenser(team), dispenser)?;
        }
        Ok(())
    }

    fn require_above(&self, spot: Spot, point: Point) -> Result<(), ArenaError> {
        if self.platform.is_above(point.x, point.y, point.z) {
            Ok(())
        } else {
            Err(ArenaError::OffPlatform(spot))
        }
    }

    pub const fn site(&self, team: TeamId) -> &TeamSite {
        match team {
            TeamId::Red => &self.red,
            TeamId::Blue => &self.blue,
        }
    }
}

impl MapSettings {
    fn validate(&self) -> Result<(), ArenaError> {
        let plain = !self.name.is_empty() && self.name != ".." && !self.name.contains(['/', '\\']);
        if plain {
            Ok(())
        } else {
            Err(ArenaError::InvalidMapName(self.name.clone()))
        }
    }
}

fn centre(block: BlockPoint) -> Point {
    Point {
        x: f64::from(block.x) + BLOCK_CENTER,
        y: f64::from(block.y),
        z: f64::from(block.z) + BLOCK_CENTER,
    }
}

fn toward(from: BlockPoint, to: BlockPoint, distance: f64) -> BlockPoint {
    let dx = f64::from(to.x - from.x);
    let dz = f64::from(to.z - from.z);
    let length = dx.hypot(dz);
    if length == 0.0 {
        return from;
    }
    BlockPoint {
        x: from.x + (dx / length * distance).round() as i32,
        y: from.y,
        z: from.z + (dz / length * distance).round() as i32,
    }
}

fn map_site(spawn: BlockPoint, other: BlockPoint) -> TeamSite {
    TeamSite {
        spawn: centre(spawn),
        dispenser: toward(spawn, other, MAP_DISPENSER_OFFSET),
    }
}

fn default_site(spawn_z: i32, dispenser_z: i32) -> TeamSite {
    TeamSite {
        spawn: Point {
            x: BLOCK_CENTER,
            y: f64::from(DEFAULT_TOP_Y + 1),
            z: f64::from(spawn_z) + BLOCK_CENTER,
        },
        dispenser: BlockPoint {
            x: 0,
            y: DEFAULT_TOP_Y + 1,
            z: dispenser_z,
        },
    }
}

impl ArenaSettings {
    pub fn fallback_platform() -> Self {
        let platform = Platform {
            center: BlockPoint {
                x: 0,
                y: DEFAULT_TOP_Y,
                z: 0,
            },
            width: DEFAULT_PLATFORM_WIDTH,
            depth: DEFAULT_PLATFORM_LENGTH,
            thickness: DEFAULT_PLATFORM_THICKNESS,
            block: DEFAULT_PLATFORM_BLOCK.to_owned(),
        };
        let red_z = platform.min_z() + DEFAULT_SPAWN_INSET;
        let blue_z = platform.min_z() + platform.depth - 1 - DEFAULT_SPAWN_INSET;
        Self {
            world_name: FALLBACK_WORLD_NAME.to_owned(),
            lobby: Point {
                x: BLOCK_CENTER,
                y: f64::from(DEFAULT_TOP_Y + 1),
                z: BLOCK_CENTER,
            },
            void_fall_y: FALLBACK_VOID_FALL_Y,
            map: None,
            platform,
            red: default_site(red_z, red_z + DEFAULT_DISPENSER_OFFSET),
            blue: default_site(blue_z, blue_z - DEFAULT_DISPENSER_OFFSET),
        }
    }
}

impl Default for ArenaSettings {
    fn default() -> Self {
        Self {
            map: Some(MapSettings {
                name: MAP_NAME.to_owned(),
                source_dir: PathBuf::from(MAP_SOURCE_DIR),
                server_dir: PathBuf::from(MAP_SERVER_DIR),
            }),
            world_name: MAP_WORLD_NAME.to_owned(),
            lobby: centre(MAP_LOBBY),
            void_fall_y: MAP_VOID_FALL_Y,
            red: map_site(MAP_RED_SPAWN, MAP_BLUE_SPAWN),
            blue: map_site(MAP_BLUE_SPAWN, MAP_RED_SPAWN),
            ..Self::fallback_platform()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_settings_are_the_steampunk_map_with_the_owner_spawns() {
        let settings = ArenaSettings::default();
        let map = settings.map.as_ref().expect("map");
        assert_eq!(map.name, "steampunk");
        assert_eq!(
            map.source_dir,
            PathBuf::from("../../server/maps/sheepwars/steampunk")
        );
        assert_eq!(map.server_dir, PathBuf::from("../../server"));
        assert_eq!(settings.world_name, "sheepwars_steampunk");
        let point = |x, y, z| Point { x, y, z };
        assert_eq!(settings.red.spawn, point(-94.5, 132.0, -7.5));
        assert_eq!(settings.blue.spawn, point(-46.5, 132.0, 26.5));
        assert_eq!(settings.lobby, point(-46.5, 156.0, 24.5));
        assert_eq!(settings.void_fall_y, MAP_VOID_FALL_Y);
        assert_eq!(settings.void_fall_y, 0.0);
    }

    #[test]
    fn default_dispensers_sit_two_blocks_toward_the_other_spawn() {
        let settings = ArenaSettings::default();
        let block = |x, y, z| BlockPoint { x, y, z };
        assert_eq!(settings.red.dispenser, block(-93, 132, -7));
        assert_eq!(settings.blue.dispenser, block(-49, 132, 25));
    }

    #[test]
    fn default_map_settings_round_trip_through_json() {
        let settings = ArenaSettings::default();
        let json = serde_json::to_string(&settings).expect("serialize");
        assert_eq!(ArenaSettings::from_json(&json).expect("parse"), settings);
    }

    #[test]
    fn a_map_arena_skips_the_platform_validation() {
        let mut settings = ArenaSettings::default();
        settings.platform.width = 0;
        settings.red.spawn.x = 9999.5;
        let json = serde_json::to_string(&settings).expect("serialize");
        assert!(ArenaSettings::from_json(&json).is_ok());
    }

    #[test]
    fn a_map_name_must_be_a_plain_folder_name() {
        for name in ["", "a/b", "..", "a\\b"] {
            let mut settings = ArenaSettings::default();
            settings.map.as_mut().expect("map").name = name.to_owned();
            assert!(matches!(rejected(&settings), ArenaError::InvalidMapName(_)));
        }
    }

    #[test]
    fn a_json_without_map_is_the_fallback_platform() {
        let mut settings = ArenaSettings::fallback_platform();
        settings.map = None;
        let mut value = serde_json::to_value(&settings).expect("value");
        value.as_object_mut().expect("object").remove("map");
        let parsed = ArenaSettings::from_json(&value.to_string()).expect("parse");
        assert!(parsed.map.is_none());
        assert_eq!(parsed, ArenaSettings::fallback_platform());
    }

    #[test]
    fn the_fallback_platform_has_no_map_and_its_own_world() {
        let settings = ArenaSettings::fallback_platform();
        assert!(settings.map.is_none());
        assert_eq!(settings.world_name, "sheepwars");
        assert_eq!(settings.void_fall_y, 20.0);
    }

    fn rejected(settings: &ArenaSettings) -> ArenaError {
        let json = serde_json::to_string(settings).expect("serialize");
        ArenaSettings::from_json(&json).expect_err("must be rejected")
    }

    #[test]
    fn default_settings_round_trip_through_json() {
        let settings = ArenaSettings::fallback_platform();
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
    fn the_old_two_platform_shape_is_rejected() {
        let old = r#"{"world_name":"w","lobby":{"x":0.5,"y":100.0,"z":0.5},
            "lobby_platform":{"center":{"x":0,"y":99,"z":0},"width":9,"depth":9,"thickness":1,"block":"minecraft:stone"},
            "void_fall_y":20.0,
            "red":{"platform":{"center":{"x":0,"y":64,"z":-24},"width":9,"depth":9,"thickness":3,"block":"minecraft:stone"},
                   "spawn":{"x":0.5,"y":65.0,"z":-23.5},"dispenser":{"x":0,"y":65,"z":-21}},
            "blue":{"platform":{"center":{"x":0,"y":64,"z":23},"width":9,"depth":9,"thickness":3,"block":"minecraft:stone"},
                    "spawn":{"x":0.5,"y":65.0,"z":23.5},"dispenser":{"x":0,"y":65,"z":20}}}"#;
        assert!(matches!(
            ArenaSettings::from_json(old),
            Err(ArenaError::Parse(_))
        ));
    }

    #[test]
    fn empty_platform_is_rejected() {
        for field in 0..3 {
            let mut settings = ArenaSettings::fallback_platform();
            match field {
                0 => settings.platform.width = 0,
                1 => settings.platform.depth = 0,
                _ => settings.platform.thickness = 0,
            }
            assert!(matches!(rejected(&settings), ArenaError::EmptyPlatform));
        }
    }

    #[test]
    fn a_spawn_off_the_platform_is_rejected() {
        let mut settings = ArenaSettings::fallback_platform();
        settings.blue.spawn.x = 500.5;
        assert!(matches!(
            rejected(&settings),
            ArenaError::OffPlatform(Spot::Spawn(TeamId::Blue))
        ));
    }

    #[test]
    fn a_spawn_below_the_platform_top_is_rejected() {
        let mut settings = ArenaSettings::fallback_platform();
        settings.red.spawn.y = 10.0;
        assert!(matches!(
            rejected(&settings),
            ArenaError::OffPlatform(Spot::Spawn(TeamId::Red))
        ));
    }

    #[test]
    fn a_dispenser_off_the_platform_is_rejected() {
        let mut settings = ArenaSettings::fallback_platform();
        settings.red.dispenser.z = -500;
        assert!(matches!(
            rejected(&settings),
            ArenaError::OffPlatform(Spot::Dispenser(TeamId::Red))
        ));
    }

    #[test]
    fn a_lobby_off_the_platform_is_rejected() {
        let mut settings = ArenaSettings::fallback_platform();
        settings.lobby.z = 500.5;
        assert!(matches!(
            rejected(&settings),
            ArenaError::OffPlatform(Spot::Lobby)
        ));
    }

    #[test]
    fn the_platform_edges_are_inside() {
        let mut settings = ArenaSettings::fallback_platform();
        settings.red.spawn.x = f64::from(settings.platform.min_x()) + 0.5;
        settings.red.spawn.z = f64::from(settings.platform.min_z()) + 0.5;
        let json = serde_json::to_string(&settings).expect("serialize");
        assert!(ArenaSettings::from_json(&json).is_ok());
        settings.red.spawn.x -= 1.0;
        assert!(matches!(
            rejected(&settings),
            ArenaError::OffPlatform(Spot::Spawn(TeamId::Red))
        ));
    }
}
