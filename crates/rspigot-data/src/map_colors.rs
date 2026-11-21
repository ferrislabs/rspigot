use crate::generated::map_colors::{
    BRIGHTNESS_MODIFIERS, COLOR_RGB, DECORATION_TYPES, STATE_COLOR,
};

pub const NONE: u8 = 0;
pub const WATER: u8 = 12;
pub const DIRT: u8 = 10;
pub const STONE: u8 = 11;

pub const PLAYER: u8 = 0;
pub const FRAME: u8 = 1;
pub const PLAYER_OFF_MAP: u8 = 6;
pub const PLAYER_OFF_LIMITS: u8 = 7;
pub const BANNER_BASE: u8 = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecorationType {
    pub name: &'static str,
    pub asset: &'static str,
    pub show_on_item_frame: bool,
    pub track_count: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Brightness {
    Low = 0,
    Normal = 1,
    High = 2,
    Lowest = 3,
}

pub fn color_of_state(state: u16) -> u8 {
    STATE_COLOR.get(usize::from(state)).copied().unwrap_or(NONE)
}

pub fn state_count() -> usize {
    STATE_COLOR.len()
}

pub fn rgb(color: u8) -> u32 {
    COLOR_RGB.get(usize::from(color)).copied().unwrap_or(0)
}

pub fn packed(color: u8, brightness: Brightness) -> u8 {
    color.wrapping_mul(4).wrapping_add(brightness as u8)
}

pub fn packed_argb(packed: u8) -> u32 {
    let color = packed >> 2;
    if color == NONE {
        return 0;
    }
    let modifier = BRIGHTNESS_MODIFIERS[usize::from(packed & 3)];
    let rgb = rgb(color);
    let channel = |shift: u32| (((rgb >> shift) & 0xff) * modifier / 255) & 0xff;
    0xff00_0000 | (channel(0) << 16) | (channel(8) << 8) | channel(16)
}

pub fn decoration_types() -> &'static [DecorationType] {
    &DECORATION_TYPES
}

pub fn decoration_type_id(name: &str) -> Option<u8> {
    let bare = name.strip_prefix("minecraft:").unwrap_or(name);
    DECORATION_TYPES
        .iter()
        .position(|t| t.name.strip_prefix("minecraft:") == Some(bare))
        .and_then(|i| u8::try_from(i).ok())
}

pub fn decoration_type(id: u8) -> Option<&'static DecorationType> {
    DECORATION_TYPES.get(usize::from(id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn well_known_constants_match_the_dump() {
        assert_eq!(rgb(WATER), 0x4040ff);
        assert_eq!(rgb(DIRT), 0x976d4d);
        assert_eq!(rgb(STONE), 0x707070);
        assert_eq!(decoration_type_id("minecraft:player"), Some(PLAYER));
        assert_eq!(decoration_type_id("frame"), Some(FRAME));
        assert_eq!(
            decoration_type_id("minecraft:banner_white"),
            Some(BANNER_BASE)
        );
        assert_eq!(
            decoration_type_id("minecraft:banner_black"),
            Some(BANNER_BASE + 15)
        );
    }

    #[test]
    fn brightness_shades_use_the_vanilla_modifiers() {
        let normal = packed_argb(packed(STONE, Brightness::Normal));
        assert_eq!(normal, 0xff00_0000 | ((0x70 * 220 / 255) * 0x0001_0101));
        assert_eq!(packed(STONE, Brightness::High), STONE * 4 + 2);
        assert_eq!(packed_argb(0), 0);
    }

    #[test]
    fn state_table_covers_air_and_water() {
        assert_eq!(color_of_state(0), NONE);
        assert!(state_count() > 30_000);
    }
}
