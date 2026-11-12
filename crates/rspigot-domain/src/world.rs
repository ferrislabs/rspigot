use crate::ident::Identifier;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error("world height min_y={min_y} height={height} is not a multiple of 16")]
pub struct InvalidWorldHeight {
    pub min_y: i32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WorldHeight {
    pub min_y: i32,
    pub height: u32,
}

impl WorldHeight {
    pub fn new(min_y: i32, height: u32) -> Result<Self, InvalidWorldHeight> {
        if min_y % 16 != 0 || !height.is_multiple_of(16) || height == 0 {
            return Err(InvalidWorldHeight { min_y, height });
        }
        Ok(Self { min_y, height })
    }

    pub const OVERWORLD: Self = Self {
        min_y: -64,
        height: 384,
    };
    pub const NETHER: Self = Self {
        min_y: 0,
        height: 256,
    };
    pub const END: Self = Self {
        min_y: 0,
        height: 256,
    };

    pub const fn section_count(self) -> usize {
        (self.height / 16) as usize
    }

    pub const fn max_y(self) -> i32 {
        self.min_y + self.height as i32 - 1
    }

    pub const fn contains(self, y: i32) -> bool {
        y >= self.min_y && y <= self.max_y()
    }

    pub const fn min_section(self) -> i32 {
        self.min_y >> 4
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Difficulty {
    Peaceful,
    #[default]
    Easy,
    Normal,
    Hard,
}

impl Difficulty {
    pub const fn id(self) -> u8 {
        match self {
            Self::Peaceful => 0,
            Self::Easy => 1,
            Self::Normal => 2,
            Self::Hard => 3,
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name.to_ascii_lowercase().as_str() {
            "peaceful" | "0" => Self::Peaceful,
            "easy" | "1" => Self::Easy,
            "normal" | "2" => Self::Normal,
            "hard" | "3" => Self::Hard,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Dimension {
    pub key: Identifier,
    pub dimension_type: Identifier,
    pub height: WorldHeight,
    pub has_skylight: bool,
    pub sea_level: i32,
    pub piglins_zombify: bool,
}

impl Dimension {
    pub fn overworld() -> Self {
        Self {
            key: Identifier::minecraft("overworld"),
            dimension_type: Identifier::minecraft("overworld"),
            height: WorldHeight::OVERWORLD,
            has_skylight: true,
            sea_level: 63,
            piglins_zombify: true,
        }
    }

    pub fn nether() -> Self {
        Self {
            key: Identifier::minecraft("the_nether"),
            dimension_type: Identifier::minecraft("the_nether"),
            height: WorldHeight::NETHER,
            has_skylight: false,
            sea_level: 32,
            piglins_zombify: false,
        }
    }

    pub fn end() -> Self {
        Self {
            key: Identifier::minecraft("the_end"),
            dimension_type: Identifier::minecraft("the_end"),
            height: WorldHeight::END,
            has_skylight: false,
            sea_level: 0,
            piglins_zombify: true,
        }
    }
}
