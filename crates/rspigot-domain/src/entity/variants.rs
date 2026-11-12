use rspigot_data::variants::VariantFamily;

use super::neutral::WOLF_DATA_ANGER_END_TIME;

pub const WOLF_DATA_INTERESTED: u8 = 20;
pub const WOLF_DATA_COLLAR_COLOR: u8 = 21;
pub const WOLF_DATA_ANGER: u8 = WOLF_DATA_ANGER_END_TIME;
pub const WOLF_DATA_VARIANT: u8 = 23;
pub const WOLF_DATA_SOUND_VARIANT: u8 = 24;

pub const CAT_DATA_VARIANT: u8 = 20;
pub const CAT_DATA_IS_LYING: u8 = 21;
pub const CAT_DATA_RELAX_STATE_ONE: u8 = 22;
pub const CAT_DATA_COLLAR_COLOR: u8 = 23;
pub const CAT_DATA_SOUND_VARIANT: u8 = 24;

pub const DEFAULT_COLLAR_COLOR: i32 = 14;

pub const fn default_variant(family: VariantFamily) -> &'static str {
    match family {
        VariantFamily::Cow | VariantFamily::Pig | VariantFamily::Chicken => "minecraft:temperate",
        VariantFamily::Wolf => "minecraft:pale",
        VariantFamily::Cat => "minecraft:black",
    }
}

pub const fn default_sound_variant(_family: VariantFamily) -> &'static str {
    "minecraft:classic"
}

#[derive(Debug, Clone, Copy)]
pub enum StructureProbe {
    Unknown,
    Within(fn(&str) -> bool),
}

impl StructureProbe {
    pub fn contains(self, structure_tag: &str) -> bool {
        match self {
            Self::Unknown => false,
            Self::Within(probe) => probe(structure_tag),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct VariantContext {
    pub biome: crate::BiomeId,
    pub moon_brightness: f32,
    pub structures: StructureProbe,
}

impl VariantContext {
    pub fn in_biome(biome: crate::BiomeId) -> Self {
        Self {
            biome,
            moon_brightness: 0.0,
            structures: StructureProbe::Unknown,
        }
    }
}
