use rspigot_blocks::{BlockKind, StateFlags};

use super::SpawnLevel;
use crate::{BiomeId, BlockPos, biome};

const SURFACE_DEPTH: i32 = 13;
const GLOW_SQUID_DEPTH: i32 = 33;
const TROPICAL_ANY_HEIGHT: &str = "minecraft:allows_tropical_fish_spawns_at_any_height";

fn is_water(level: &impl SpawnLevel, pos: BlockPos) -> bool {
    level.block_or_air(pos).flags().contains(StateFlags::WATER)
}

fn is_water_block(level: &impl SpawnLevel, pos: BlockPos) -> bool {
    level.block_or_air(pos).kind() == Some(BlockKind::Water)
}

pub fn check_surface_water_spawn_rules(level: &impl SpawnLevel, pos: BlockPos) -> bool {
    let sea_level = level.sea_level();
    let low = sea_level - SURFACE_DEPTH;
    low <= pos.y
        && pos.y <= sea_level
        && is_water(level, pos.offset(0, -1, 0))
        && is_water_block(level, pos.offset(0, 1, 0))
}

pub fn check_tropical_fish_spawn_rules(level: &impl SpawnLevel, pos: BlockPos) -> bool {
    if !is_water(level, pos.offset(0, -1, 0)) || !is_water_block(level, pos.offset(0, 1, 0)) {
        return false;
    }
    let any_height = biome::in_tag(biome_at(level, pos), TROPICAL_ANY_HEIGHT);
    any_height || check_surface_water_spawn_rules(level, pos)
}

fn biome_at(level: &impl SpawnLevel, pos: BlockPos) -> BiomeId {
    level.biome(pos)
}

pub fn check_glow_squid_spawn_rules(level: &impl SpawnLevel, pos: BlockPos) -> bool {
    pos.y <= level.sea_level() - GLOW_SQUID_DEPTH
        && level.raw_brightness(pos, 0) == 0
        && is_water_block(level, pos)
}

#[cfg(test)]
mod tests {
    use rspigot_blocks::{Block, Stone, Water};

    use super::*;
    use crate::spawn::SpawnReason;
    use crate::spawn::rules::{check_spawn_rules, rules_ported};
    use crate::spawn::testing::Level;
    use rspigot_entities::EntityKind;

    fn sea() -> Level {
        let mut level = Level::floor(Stone::DEFAULT_STATE, 55);
        for y in 56..=70 {
            level.set(BlockPos::new(4, y, 4), Water::DEFAULT_STATE);
        }
        level
    }

    #[test]
    fn a_surface_water_animal_needs_water_below_water_above_and_the_top_thirteen_levels() {
        let level = sea();
        let ok = BlockPos::new(4, 60, 4);
        assert!(check_surface_water_spawn_rules(&level, ok));
        assert!(check_surface_water_spawn_rules(
            &level,
            BlockPos::new(4, 63, 4)
        ));
        assert!(!check_surface_water_spawn_rules(
            &level,
            BlockPos::new(4, 64, 4)
        ));
        assert!(check_surface_water_spawn_rules(
            &level,
            BlockPos::new(4, 57, 4)
        ));
        assert!(!check_surface_water_spawn_rules(
            &level,
            BlockPos::new(4, 56, 4)
        ));
        let mut deep = Level::floor(Stone::DEFAULT_STATE, 40);
        for y in 41..=70 {
            deep.set(BlockPos::new(4, y, 4), Water::DEFAULT_STATE);
        }
        assert!(!check_surface_water_spawn_rules(
            &deep,
            BlockPos::new(4, 49, 4)
        ));
        assert!(check_surface_water_spawn_rules(
            &deep,
            BlockPos::new(4, 50, 4)
        ));
        assert!(!check_surface_water_spawn_rules(
            &level,
            BlockPos::new(4, 55, 4)
        ));
    }

    #[test]
    fn the_fish_and_the_surface_creatures_use_the_surface_rule_in_any_difficulty() {
        let mut level = sea();
        level.difficulty = crate::world::Difficulty::Peaceful;
        let pos = BlockPos::new(4, 60, 4);
        for kind in [
            EntityKind::Cod,
            EntityKind::Salmon,
            EntityKind::Pufferfish,
            EntityKind::Squid,
            EntityKind::Dolphin,
        ] {
            assert!(rules_ported(kind), "{kind:?}");
            assert!(
                check_spawn_rules(&mut level, kind, SpawnReason::Natural, pos),
                "{kind:?}"
            );
            assert!(!check_spawn_rules(
                &mut level,
                kind,
                SpawnReason::Natural,
                BlockPos::new(4, 66, 4)
            ));
        }
    }

    #[test]
    fn tropical_fish_also_spawn_at_any_height_in_the_lush_caves_biomes() {
        let mut level = Level::floor(Stone::DEFAULT_STATE, 40);
        for y in 41..=70 {
            level.set(BlockPos::new(4, y, 4), Water::DEFAULT_STATE);
        }
        let pos = BlockPos::new(4, 45, 4);
        assert!(rules_ported(EntityKind::TropicalFish));
        assert!(!check_spawn_rules(
            &mut level,
            EntityKind::TropicalFish,
            SpawnReason::Natural,
            pos
        ));
        let lush = rspigot_data::registries::biome_id("minecraft:lush_caves").unwrap();
        level.biome = BiomeId(lush);
        assert!(check_spawn_rules(
            &mut level,
            EntityKind::TropicalFish,
            SpawnReason::Natural,
            pos
        ));
        assert!(!check_spawn_rules(
            &mut level,
            EntityKind::TropicalFish,
            SpawnReason::Natural,
            BlockPos::new(4, 41, 5)
        ));
    }

    #[test]
    fn a_glow_squid_needs_total_darkness_in_water_thirty_three_levels_under_the_sea() {
        let mut level = Level::floor(Stone::DEFAULT_STATE, 20);
        for y in 21..=40 {
            level.set(BlockPos::new(4, y, 4), Water::DEFAULT_STATE);
        }
        level.sky = 0;
        level.block = 0;
        assert!(rules_ported(EntityKind::GlowSquid));
        let pos = BlockPos::new(4, 30, 4);
        assert!(check_spawn_rules(
            &mut level,
            EntityKind::GlowSquid,
            SpawnReason::Natural,
            pos
        ));
        assert!(!check_spawn_rules(
            &mut level,
            EntityKind::GlowSquid,
            SpawnReason::Natural,
            BlockPos::new(4, 31, 4)
        ));
        level.block = 1;
        assert!(!check_spawn_rules(
            &mut level,
            EntityKind::GlowSquid,
            SpawnReason::Natural,
            pos
        ));
        level.block = 0;
        level.sky = 5;
        assert!(!check_spawn_rules(
            &mut level,
            EntityKind::GlowSquid,
            SpawnReason::Natural,
            pos
        ));
    }
}
