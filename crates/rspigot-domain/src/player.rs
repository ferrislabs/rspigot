use std::fmt;

use md5::{Digest, Md5};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PlayerId(pub Uuid);

impl PlayerId {
    pub fn offline(name: &str) -> Self {
        Self(offline_uuid(name))
    }
}

impl fmt::Display for PlayerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

pub fn offline_uuid(name: &str) -> Uuid {
    let digest: [u8; 16] = Md5::digest(format!("OfflinePlayer:{name}").as_bytes()).into();
    uuid::Builder::from_md5_bytes(digest).into_uuid()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileProperty {
    pub name: String,
    pub value: String,
    pub signature: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameProfile {
    pub id: PlayerId,
    pub name: String,
    pub properties: Vec<ProfileProperty>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum GameMode {
    #[default]
    Survival,
    Creative,
    Adventure,
    Spectator,
}

impl GameMode {
    pub const fn id(self) -> u8 {
        match self {
            Self::Survival => 0,
            Self::Creative => 1,
            Self::Adventure => 2,
            Self::Spectator => 3,
        }
    }

    pub fn from_id(id: i32) -> Option<Self> {
        Some(match id {
            0 => Self::Survival,
            1 => Self::Creative,
            2 => Self::Adventure,
            3 => Self::Spectator,
            _ => return None,
        })
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name.to_ascii_lowercase().as_str() {
            "survival" | "s" | "0" => Self::Survival,
            "creative" | "c" | "1" => Self::Creative,
            "adventure" | "a" | "2" => Self::Adventure,
            "spectator" | "sp" | "3" => Self::Spectator,
            _ => return None,
        })
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Survival => "survival",
            Self::Creative => "creative",
            Self::Adventure => "adventure",
            Self::Spectator => "spectator",
        }
    }

    pub const fn can_fly(self) -> bool {
        matches!(self, Self::Creative | Self::Spectator)
    }

    pub const fn instant_break(self) -> bool {
        matches!(self, Self::Creative)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MainHand {
    Left,
    #[default]
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChatVisibility {
    #[default]
    Full,
    System,
    Hidden,
}

impl ChatVisibility {
    pub const fn accepts_chat(self) -> bool {
        matches!(self, Self::Full)
    }

    pub const fn accepts_system(self, overlay: bool) -> bool {
        overlay || !matches!(self, Self::Hidden)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientSettings {
    pub locale: String,
    pub view_distance: u8,
    pub chat_colors: bool,
    pub skin_parts: u8,
    pub main_hand: MainHand,
    pub chat_visibility: ChatVisibility,
    pub allows_listing: bool,
}

impl Default for ClientSettings {
    fn default() -> Self {
        Self {
            locale: "en_us".to_owned(),
            view_distance: 10,
            chat_colors: true,
            skin_parts: 0x7F,
            main_hand: MainHand::Right,
            chat_visibility: ChatVisibility::Full,
            allows_listing: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_visibility_follows_vanilla_acceptance_rules() {
        assert!(ChatVisibility::Full.accepts_chat());
        assert!(!ChatVisibility::System.accepts_chat());
        assert!(!ChatVisibility::Hidden.accepts_chat());
        assert!(ChatVisibility::System.accepts_system(false));
        assert!(!ChatVisibility::Hidden.accepts_system(false));
        assert!(ChatVisibility::Hidden.accepts_system(true));
    }

    #[test]
    fn offline_uuid_matches_vanilla() {
        assert_eq!(
            offline_uuid("Notch").to_string(),
            "b50ad385-829d-3141-a216-7e7d7539ba7f"
        );
    }
}
