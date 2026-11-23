use rspigot_blocks::BlockKind;
use rspigot_domain::BlockStateId;

use crate::tree_growth::is_sapling;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Crop {
    Cactus,
    Cane,
    Melon,
    Mushroom,
    Pumpkin,
    Sapling,
    Beetroot,
    Carrot,
    Potato,
    Wheat,
    NetherWart,
    Vine,
    Cocoa,
    Bamboo,
    SweetBerry,
    Kelp,
    TwistingVines,
    WeepingVines,
    CaveVines,
}

impl Crop {
    pub const ALL: [Self; 19] = [
        Self::Cactus,
        Self::Cane,
        Self::Melon,
        Self::Mushroom,
        Self::Pumpkin,
        Self::Sapling,
        Self::Beetroot,
        Self::Carrot,
        Self::Potato,
        Self::Wheat,
        Self::NetherWart,
        Self::Vine,
        Self::Cocoa,
        Self::Bamboo,
        Self::SweetBerry,
        Self::Kelp,
        Self::TwistingVines,
        Self::WeepingVines,
        Self::CaveVines,
    ];

    pub const fn key(self) -> &'static str {
        match self {
            Self::Cactus => "growth.cactus-modifier",
            Self::Cane => "growth.cane-modifier",
            Self::Melon => "growth.melon-modifier",
            Self::Mushroom => "growth.mushroom-modifier",
            Self::Pumpkin => "growth.pumpkin-modifier",
            Self::Sapling => "growth.sapling-modifier",
            Self::Beetroot => "growth.beetroot-modifier",
            Self::Carrot => "growth.carrot-modifier",
            Self::Potato => "growth.potato-modifier",
            Self::Wheat => "growth.wheat-modifier",
            Self::NetherWart => "growth.netherwart-modifier",
            Self::Vine => "growth.vine-modifier",
            Self::Cocoa => "growth.cocoa-modifier",
            Self::Bamboo => "growth.bamboo-modifier",
            Self::SweetBerry => "growth.sweetberry-modifier",
            Self::Kelp => "growth.kelp-modifier",
            Self::TwistingVines => "growth.twistingvines-modifier",
            Self::WeepingVines => "growth.weepingvines-modifier",
            Self::CaveVines => "growth.cavevines-modifier",
        }
    }

    pub fn of(state: BlockStateId) -> Option<Self> {
        if is_sapling(state) {
            return Some(Self::Sapling);
        }
        Some(match state.kind()? {
            BlockKind::Cactus => Self::Cactus,
            BlockKind::SugarCane => Self::Cane,
            BlockKind::MelonStem => Self::Melon,
            BlockKind::BrownMushroom | BlockKind::RedMushroom => Self::Mushroom,
            BlockKind::PumpkinStem => Self::Pumpkin,
            BlockKind::Beetroots => Self::Beetroot,
            BlockKind::Carrots => Self::Carrot,
            BlockKind::Potatoes => Self::Potato,
            BlockKind::Wheat => Self::Wheat,
            BlockKind::NetherWart => Self::NetherWart,
            BlockKind::Vine => Self::Vine,
            BlockKind::Cocoa => Self::Cocoa,
            BlockKind::Bamboo | BlockKind::BambooSapling => Self::Bamboo,
            BlockKind::SweetBerryBush => Self::SweetBerry,
            BlockKind::Kelp => Self::Kelp,
            BlockKind::TwistingVines => Self::TwistingVines,
            BlockKind::WeepingVines => Self::WeepingVines,
            BlockKind::CaveVines => Self::CaveVines,
            _ => return None,
        })
    }

    const fn index(self) -> usize {
        self as usize
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GrowthModifiers([i32; 19]);

impl Default for GrowthModifiers {
    fn default() -> Self {
        Self([100; 19])
    }
}

impl GrowthModifiers {
    pub fn get(&self, crop: Crop) -> i32 {
        self.0[crop.index()]
    }

    pub fn set(&mut self, crop: Crop, percent: i32) {
        self.0[crop.index()] = if percent == 0 { 100 } else { percent };
    }

    pub fn repeats(&self, crop: Crop, roll: f32) -> u32 {
        let percent = self.get(crop).max(0);
        let whole = (percent / 100) as u32;
        let rest = percent % 100;
        whole + u32::from(roll * 100.0 < rest as f32)
    }

    pub fn is_vanilla(&self, crop: Crop) -> bool {
        self.get(crop) == 100
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StructureSeeds {
    pub village: i32,
    pub desert: i32,
    pub igloo: i32,
    pub jungle: i32,
    pub swamp: i32,
    pub monument: i32,
    pub shipwreck: i32,
    pub ocean: i32,
    pub outpost: i32,
    pub endcity: i32,
    pub slime: i32,
    pub nether: i32,
    pub mansion: i32,
    pub fossil: i32,
    pub portal: i32,
}

impl Default for StructureSeeds {
    fn default() -> Self {
        Self {
            village: 0x009e_7f70,
            desert: 0x00db_1471,
            igloo: 0x00db_1472,
            jungle: 0x00db_1473,
            swamp: 0x00db_1474,
            monument: 0x009e_7f71,
            shipwreck: 0x09e1_128f,
            ocean: 0x00db_1475,
            outpost: 0x09e1_1290,
            endcity: 0x009e_7f71,
            slime: 0x3ad8_025f,
            nether: 0x01cb_0c88,
            mansion: 0x009e_7f77,
            fossil: 0x00db_15a1,
            portal: 0x020a_3235,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WorldKeys {
    pub growth: GrowthModifiers,
    pub nerf_spawner_mobs: bool,
    pub hopper_amount: i32,
    pub hopper_check: i32,
    pub hopper_can_load_chunks: bool,
    pub seeds: StructureSeeds,
    pub hanging_tick_frequency: i32,
    pub unload_frozen_chunks: bool,
    pub zombie_pigmen_portal_spawns: bool,
    pub wither_spawn_sound_radius: i32,
    pub end_portal_sound_radius: i32,
    pub dragon_death_sound_radius: i32,
    pub max_tick_time_tile_ms: i64,
    pub max_tick_time_entity_ms: i64,
    pub tick_inactive_villagers: bool,
    pub trident_despawn_rate: i32,
    pub merge_radius_exp: f64,
}

impl Default for WorldKeys {
    fn default() -> Self {
        Self {
            growth: GrowthModifiers::default(),
            nerf_spawner_mobs: false,
            hopper_amount: 1,
            hopper_check: 1,
            hopper_can_load_chunks: false,
            seeds: StructureSeeds::default(),
            hanging_tick_frequency: 100,
            unload_frozen_chunks: false,
            zombie_pigmen_portal_spawns: true,
            wither_spawn_sound_radius: 0,
            end_portal_sound_radius: 0,
            dragon_death_sound_radius: 0,
            max_tick_time_tile_ms: 50,
            max_tick_time_entity_ms: 50,
            tick_inactive_villagers: true,
            trident_despawn_rate: 1200,
            merge_radius_exp: -1.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_crop_has_a_distinct_spigot_key() {
        let mut keys: Vec<_> = Crop::ALL.iter().map(|crop| crop.key()).collect();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), 19);
        assert!(
            Crop::ALL
                .iter()
                .all(|crop| crop.key().starts_with("growth.") && crop.key().ends_with("-modifier"))
        );
    }

    #[test]
    fn a_zero_modifier_falls_back_to_one_hundred_like_spigot() {
        let mut growth = GrowthModifiers::default();
        growth.set(Crop::Wheat, 0);
        assert_eq!(growth.get(Crop::Wheat), 100);
        growth.set(Crop::Wheat, 250);
        assert_eq!(growth.get(Crop::Wheat), 250);
    }

    #[test]
    fn repeats_scale_the_growth_attempts() {
        let mut growth = GrowthModifiers::default();
        assert_eq!(growth.repeats(Crop::Wheat, 0.99), 1);
        growth.set(Crop::Wheat, 50);
        assert_eq!(growth.repeats(Crop::Wheat, 0.49), 1);
        assert_eq!(growth.repeats(Crop::Wheat, 0.51), 0);
        growth.set(Crop::Wheat, 250);
        assert_eq!(growth.repeats(Crop::Wheat, 0.49), 3);
        assert_eq!(growth.repeats(Crop::Wheat, 0.51), 2);
    }

    #[test]
    fn crops_map_from_their_blocks() {
        assert_eq!(
            Crop::of(BlockKind::Wheat.default_state()),
            Some(Crop::Wheat)
        );
        assert_eq!(
            Crop::of(BlockKind::Cactus.default_state()),
            Some(Crop::Cactus)
        );
        assert_eq!(Crop::of(BlockKind::Stone.default_state()), None);
    }

    #[test]
    fn the_default_seeds_are_the_spigot_values() {
        let seeds = StructureSeeds::default();
        assert_eq!(seeds.village, 10_387_312);
        assert_eq!(seeds.desert, 14_357_617);
        assert_eq!(seeds.slime, 987_234_911);
    }
}
