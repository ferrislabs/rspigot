use rspigot_entities::EntityKind;

use super::SpawnReason;
use super::rules::{RandomLevel, check_mob_spawn_rules};
use crate::BlockPos;
use crate::difficulty::moon_brightness;
use crate::random::{LegacyRandom, RandomSource};
use crate::world::Difficulty;

pub const DEFAULT_SLIME_SEED: i64 = 987_234_911;
const SURFACE_BIOMES: &str = "minecraft:allows_surface_slime_spawns";
const SURFACE_MIN_Y: i32 = 50;
const SURFACE_MAX_Y: i32 = 70;
const SURFACE_LIGHT_BOUND: i32 = 8;
const SLIME_CHUNK_ODDS: i32 = 10;
const SLIME_CHUNK_MAX_Y: i32 = 40;
const MOON_CHANCE_SCALE: f32 = 0.5;
const CHUNK_X_SQUARED: i32 = 4_987_142;
const CHUNK_X: i32 = 5_947_611;
const CHUNK_Z_SQUARED: i64 = 4_392_871;
const CHUNK_Z: i32 = 389_711;

pub fn slime_chunk_random(chunk_x: i32, chunk_z: i32, seed: i64, salt: i64) -> LegacyRandom {
    let mixed = seed
        .wrapping_add(i64::from(
            chunk_x.wrapping_mul(chunk_x).wrapping_mul(CHUNK_X_SQUARED),
        ))
        .wrapping_add(i64::from(chunk_x.wrapping_mul(CHUNK_X)))
        .wrapping_add(i64::from(chunk_z.wrapping_mul(chunk_z)).wrapping_mul(CHUNK_Z_SQUARED))
        .wrapping_add(i64::from(chunk_z.wrapping_mul(CHUNK_Z)));
    LegacyRandom::new(mixed ^ salt)
}

pub fn is_slime_chunk(chunk_x: i32, chunk_z: i32, seed: i64, salt: i64) -> bool {
    slime_chunk_random(chunk_x, chunk_z, seed, salt).next_int(SLIME_CHUNK_ODDS) == 0
}

pub fn surface_spawn_chance(day_time: i64, moon_clock: bool) -> f32 {
    if moon_clock {
        moon_brightness(day_time, true) * MOON_CHANCE_SCALE
    } else {
        0.0
    }
}

pub fn check_slime_spawn_rules(
    level: &mut impl RandomLevel,
    kind: EntityKind,
    reason: SpawnReason,
    pos: BlockPos,
) -> bool {
    if level.difficulty() == Difficulty::Peaceful {
        return false;
    }
    if reason.is_spawner() {
        return check_mob_spawn_rules(level, kind, reason, pos);
    }
    if crate::biome::in_tag(level.biome(pos), SURFACE_BIOMES)
        && pos.y > SURFACE_MIN_Y
        && pos.y < SURFACE_MAX_Y
        && level.next_float() < level.surface_slime_spawn_chance()
    {
        let brightness = level.max_local_raw_brightness(pos, level.sky_darken());
        if brightness <= level.next_int(SURFACE_LIGHT_BOUND) {
            return check_mob_spawn_rules(level, kind, reason, pos);
        }
    }
    let in_slime_chunk = level.is_slime_chunk(pos.chunk());
    if level.next_int(SLIME_CHUNK_ODDS) == 0 && in_slime_chunk && pos.y < SLIME_CHUNK_MAX_Y {
        return check_mob_spawn_rules(level, kind, reason, pos);
    }
    false
}

pub fn check_magma_cube_spawn_rules(level: &impl RandomLevel) -> bool {
    level.difficulty() != Difficulty::Peaceful
}

#[cfg(test)]
mod tests {
    use rspigot_blocks::{Block, Stone};

    use super::*;
    use crate::BiomeId;
    use crate::spawn::rules::check_spawn_rules;
    use crate::spawn::testing::Level;

    const SWAMP_POS: BlockPos = BlockPos::new(4, 60, 4);
    const DEEP_POS: BlockPos = BlockPos::new(4, 30, 4);

    fn swamp() -> BiomeId {
        BiomeId(rspigot_data::registries::biome_id("minecraft:swamp").unwrap())
    }

    fn level(y: i32) -> Level {
        Level::floor(Stone::DEFAULT_STATE, y - 1)
    }

    fn spawns(level: &mut Level, kind: EntityKind, reason: SpawnReason, pos: BlockPos) -> bool {
        check_spawn_rules(level, kind, reason, pos)
    }

    #[test]
    fn a_slime_chunk_follows_the_vanilla_seed_mix() {
        let mixed = |x: i32, z: i32, seed: i64| {
            seed.wrapping_add(i64::from(x.wrapping_mul(x).wrapping_mul(4_987_142)))
                .wrapping_add(i64::from(x.wrapping_mul(5_947_611)))
                .wrapping_add(i64::from(z.wrapping_mul(z)).wrapping_mul(4_392_871))
                .wrapping_add(i64::from(z.wrapping_mul(389_711)))
                ^ DEFAULT_SLIME_SEED
        };
        for (x, z) in [(0, 0), (3, -4), (-100, 77), (1234, 5678)] {
            let mut expected = LegacyRandom::new(mixed(x, z, 42));
            assert_eq!(
                is_slime_chunk(x, z, 42, DEFAULT_SLIME_SEED),
                expected.next_int(10) == 0
            );
        }
    }

    #[test]
    fn roughly_one_chunk_in_ten_is_a_slime_chunk() {
        let count = (0..60)
            .flat_map(|x| (0..60).map(move |z| (x, z)))
            .filter(|(x, z)| is_slime_chunk(*x, *z, 12345, DEFAULT_SLIME_SEED))
            .count();
        assert!((260..460).contains(&count), "{count}");
    }

    #[test]
    fn the_surface_chance_follows_the_moon_and_is_zero_off_the_overworld_clock() {
        let days = |day: i64| day * 24_000 + 5;
        let expected = [0.5, 0.375, 0.25, 0.125, 0.0, 0.125, 0.25, 0.375];
        for (day, chance) in expected.into_iter().enumerate() {
            assert_eq!(surface_spawn_chance(days(day as i64), true), chance);
            assert_eq!(surface_spawn_chance(days(day as i64 + 8), true), chance);
        }
        assert_eq!(surface_spawn_chance(0, false), 0.0);
    }

    #[test]
    fn a_slime_spawns_in_a_swamp_between_heights_fifty_and_seventy_by_moon_chance() {
        let mut level = level(60);
        level.biome = swamp();
        level.slime_chance = 0.5;
        level.sky_darken = 15;
        level.sky = 0;
        level.rolls(&[0, 3]);
        assert!(spawns(
            &mut level,
            EntityKind::Slime,
            SpawnReason::Natural,
            SWAMP_POS
        ));
        assert_eq!(level.used(), 2);
    }

    #[test]
    fn a_failed_moon_roll_falls_through_to_the_slime_chunk_rule() {
        let mut level = level(60);
        level.biome = swamp();
        level.slime_chance = 0.0;
        level.rolls(&[1]);
        assert!(!spawns(
            &mut level,
            EntityKind::Slime,
            SpawnReason::Natural,
            SWAMP_POS
        ));
    }

    #[test]
    fn the_surface_rule_needs_a_bright_roll_below_the_light_level() {
        let mut level = level(60);
        level.biome = swamp();
        level.slime_chance = 1.0;
        level.sky = 15;
        level.rolls(&[0, 3, 9]);
        assert!(!spawns(
            &mut level,
            EntityKind::Slime,
            SpawnReason::Natural,
            SWAMP_POS
        ));
    }

    #[test]
    fn other_biomes_and_heights_skip_the_surface_rule() {
        let mut plains = level(60);
        plains.slime_chance = 1.0;
        plains.sky = 0;
        plains.rolls(&[1]);
        assert!(!spawns(
            &mut plains,
            EntityKind::Slime,
            SpawnReason::Natural,
            SWAMP_POS
        ));
        assert_eq!(plains.used(), 1);
        let mut high = level(80);
        high.biome = swamp();
        high.slime_chance = 1.0;
        high.sky = 0;
        high.rolls(&[1]);
        assert!(!spawns(
            &mut high,
            EntityKind::Slime,
            SpawnReason::Natural,
            BlockPos::new(4, 80, 4)
        ));
        assert_eq!(high.used(), 1);
    }

    #[test]
    fn a_slime_chunk_below_height_forty_spawns_on_one_roll_in_ten() {
        let mut level = level(30);
        level.slime_chunk = true;
        level.rolls(&[0]);
        assert!(spawns(
            &mut level,
            EntityKind::Slime,
            SpawnReason::Natural,
            DEEP_POS
        ));
        let mut level_two = self::level(30);
        level_two.slime_chunk = true;
        level_two.rolls(&[4]);
        assert!(!spawns(
            &mut level_two,
            EntityKind::Slime,
            SpawnReason::Natural,
            DEEP_POS
        ));
        let mut plain = self::level(30);
        plain.rolls(&[0]);
        assert!(!spawns(
            &mut plain,
            EntityKind::Slime,
            SpawnReason::Natural,
            DEEP_POS
        ));
        let mut too_high = self::level(45);
        too_high.slime_chunk = true;
        too_high.rolls(&[0]);
        assert!(!spawns(
            &mut too_high,
            EntityKind::Slime,
            SpawnReason::Natural,
            BlockPos::new(4, 45, 4)
        ));
    }

    #[test]
    fn spawners_skip_every_roll_and_peaceful_blocks_everything() {
        let mut level = level(30);
        assert!(spawns(
            &mut level,
            EntityKind::Slime,
            SpawnReason::Spawner,
            DEEP_POS
        ));
        assert_eq!(level.used(), 0);
        level.difficulty = Difficulty::Peaceful;
        assert!(!spawns(
            &mut level,
            EntityKind::Slime,
            SpawnReason::Spawner,
            DEEP_POS
        ));
    }

    #[test]
    fn a_magma_cube_only_needs_a_non_peaceful_world_and_valid_ground() {
        let mut level = level(30);
        assert!(spawns(
            &mut level,
            EntityKind::MagmaCube,
            SpawnReason::Natural,
            DEEP_POS
        ));
        assert_eq!(level.used(), 0);
        level.difficulty = Difficulty::Peaceful;
        assert!(!spawns(
            &mut level,
            EntityKind::MagmaCube,
            SpawnReason::Natural,
            DEEP_POS
        ));
    }
}
