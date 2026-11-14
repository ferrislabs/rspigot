use rspigot_blocks::{BlockKind, StateFlags};

use super::rules::{RandomLevel, can_see_sky, is_bright_enough_to_spawn, is_dark_enough_to_spawn};
use super::{SpawnLevel, SpawnReason};
use crate::entity::physics::has_tag;
use crate::world::Difficulty;
use crate::{BiomeId, BlockPos, biome};

const SAND: &str = "minecraft:sand";
const MORE_FREQUENT_DROWNED: &str = "minecraft:more_frequent_drowned_spawns";
const TURTLE_HEIGHT_ABOVE_SEA: i32 = 4;
const TURTLE_WALK_TARGET: f32 = 10.0;
const GUARDIAN_WATER_BONUS: f32 = 10.0;
const GUARDIAN_SKY_ODDS: i32 = 20;
const DROWNED_DEPTH: i32 = 5;
const DROWNED_RIVER_ODDS: i32 = 15;
const DROWNED_OCEAN_ODDS: i32 = 40;
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

fn on_sand(level: &impl SpawnLevel, pos: BlockPos) -> bool {
    has_tag(level.block_or_air(pos.offset(0, -1, 0)), SAND)
}

pub fn check_turtle_spawn_rules(level: &impl SpawnLevel, pos: BlockPos) -> bool {
    pos.y < level.sea_level() + TURTLE_HEIGHT_ABOVE_SEA
        && on_sand(level, pos)
        && is_bright_enough_to_spawn(level, pos)
}

pub fn turtle_walk_target_value(level: &impl SpawnLevel, pos: BlockPos) -> f32 {
    if is_water(level, pos) || on_sand(level, pos) {
        return TURTLE_WALK_TARGET;
    }
    level.light_level_dependent_magic_value(pos) - 0.5
}

pub fn guardian_walk_target_value(level: &impl SpawnLevel, pos: BlockPos) -> f32 {
    if is_water(level, pos) {
        level.light_level_dependent_magic_value(pos) - 0.5 + GUARDIAN_WATER_BONUS
    } else {
        0.0
    }
}

fn can_see_sky_from_below_water(level: &impl SpawnLevel, pos: BlockPos) -> bool {
    if pos.y >= level.sea_level() {
        return can_see_sky(level, pos);
    }
    let mut cursor = BlockPos::new(pos.x, level.sea_level(), pos.z);
    if !can_see_sky(level, cursor) {
        return false;
    }
    cursor = cursor.offset(0, -1, 0);
    while cursor.y > pos.y {
        let state = level.block_or_air(cursor);
        let liquid = matches!(state.kind(), Some(BlockKind::Water | BlockKind::Lava));
        if state.light_dampening() > 0 && !liquid {
            return false;
        }
        cursor = cursor.offset(0, -1, 0);
    }
    true
}

pub fn check_guardian_spawn_rules(
    level: &mut impl RandomLevel,
    reason: SpawnReason,
    pos: BlockPos,
) -> bool {
    if level.next_int(GUARDIAN_SKY_ODDS) != 0 && can_see_sky_from_below_water(level, pos) {
        return false;
    }
    if level.difficulty() == Difficulty::Peaceful {
        return false;
    }
    if !reason.is_spawner() && !is_water(level, pos) {
        return false;
    }
    is_water(level, pos.offset(0, -1, 0))
}

fn deep_enough_for_drowned(level: &impl SpawnLevel, pos: BlockPos) -> bool {
    pos.y < level.sea_level() - DROWNED_DEPTH
}

pub fn check_drowned_spawn_rules(
    level: &mut impl RandomLevel,
    reason: SpawnReason,
    pos: BlockPos,
) -> bool {
    if !is_water(level, pos.offset(0, -1, 0)) && !reason.is_spawner() {
        return false;
    }
    let biome = level.biome(pos);
    let valid = level.difficulty() != Difficulty::Peaceful
        && (reason.ignores_light_requirements() || is_dark_enough_to_spawn(level, pos))
        && (reason.is_spawner() || is_water(level, pos));
    if valid && (reason.is_spawner() || reason == SpawnReason::Reinforcement) {
        return true;
    }
    if biome::in_tag(biome, MORE_FREQUENT_DROWNED) {
        return level.next_int(DROWNED_RIVER_ODDS) == 0 && valid;
    }
    level.next_int(DROWNED_OCEAN_ODDS) == 0 && deep_enough_for_drowned(level, pos) && valid
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

    fn beach() -> Level {
        Level::floor(rspigot_blocks::Sand::DEFAULT_STATE, 62)
    }

    #[test]
    fn a_turtle_spawns_on_sand_below_four_above_sea_level_in_the_light() {
        let mut level = beach();
        level.sky = 15;
        let pos = BlockPos::new(4, 63, 4);
        assert!(rules_ported(EntityKind::Turtle));
        assert!(check_spawn_rules(
            &mut level,
            EntityKind::Turtle,
            SpawnReason::Natural,
            pos
        ));
        assert!(!check_spawn_rules(
            &mut level,
            EntityKind::Turtle,
            SpawnReason::Natural,
            BlockPos::new(4, 68, 4)
        ));
        let mut dark = beach();
        dark.sky = 0;
        assert!(!check_spawn_rules(
            &mut dark,
            EntityKind::Turtle,
            SpawnReason::Natural,
            pos
        ));
        let mut stone = Level::floor(Stone::DEFAULT_STATE, 62);
        stone.sky = 15;
        assert!(!check_spawn_rules(
            &mut stone,
            EntityKind::Turtle,
            SpawnReason::Natural,
            pos
        ));
    }

    #[test]
    fn a_turtle_prefers_water_and_sand_and_a_guardian_adds_ten_in_water() {
        let mut level = beach();
        level.set(BlockPos::new(8, 63, 4), Water::DEFAULT_STATE);
        let turtle = |pos| crate::spawn::rules::walk_target_value(&level, EntityKind::Turtle, pos);
        assert_eq!(turtle(BlockPos::new(8, 63, 4)), Some(10.0));
        assert_eq!(turtle(BlockPos::new(4, 63, 4)), Some(10.0));
        assert!(turtle(BlockPos::new(4, 70, 4)).unwrap() < 10.0);
        let guardian = |pos| {
            crate::spawn::rules::walk_target_value(&level, EntityKind::Guardian, pos).unwrap()
        };
        assert!(guardian(BlockPos::new(8, 63, 4)) >= 9.5);
        assert_eq!(guardian(BlockPos::new(4, 70, 4)), 0.0);
    }
}
