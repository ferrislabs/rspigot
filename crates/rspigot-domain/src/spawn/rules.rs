use rspigot_blocks::BlockKind;
use rspigot_entities::{EntityKind, MobClass, SpawnPredicate};

use super::{SpawnLevel, SpawnReason, sample_count};
use crate::entity::Aabb;
use crate::entity::physics::has_tag;
use crate::random::RandomSource;
use crate::world::Difficulty;
use crate::{BlockPos, Vec3};

const ANIMALS_SPAWNABLE_ON: &str = "minecraft:animals_spawnable_on";
const MOOSHROOMS_SPAWNABLE_ON: &str = "minecraft:mooshrooms_spawnable_on";
const RABBITS_SPAWNABLE_ON: &str = "minecraft:rabbits_spawnable_on";
const LEAVES: &str = "minecraft:leaves";
const OCELOT_SPAWN_CHANCE: i32 = 3;
const ANIMAL_MIN_BRIGHTNESS: i32 = 8;
const SKY_LIGHT_ROLL: i32 = 32;
const UNLIMITED_BLOCK_LIGHT: i32 = 15;
const THUNDER_DARKEN: i32 = 10;
const GRASS_WALK_TARGET: f32 = 10.0;
const DEFAULT_CLUSTER_SIZE: i32 = 4;

pub trait RandomLevel: SpawnLevel + RandomSource {}

impl<T: SpawnLevel + RandomSource> RandomLevel for T {}

pub fn check_spawn_rules(
    level: &mut impl RandomLevel,
    kind: EntityKind,
    reason: SpawnReason,
    pos: BlockPos,
) -> bool {
    if !kind.allowed_in_peaceful() && level.difficulty() == Difficulty::Peaceful {
        return false;
    }
    let Some(placement) = kind.mob_spawn().and_then(|spawn| spawn.placement) else {
        return true;
    };
    check_predicate(placement.predicate, level, kind, reason, pos).unwrap_or(false)
}

pub fn check_predicate(
    predicate: SpawnPredicate,
    level: &mut impl RandomLevel,
    kind: EntityKind,
    reason: SpawnReason,
    pos: BlockPos,
) -> Option<bool> {
    Some(match predicate {
        SpawnPredicate::Mob => check_mob_spawn_rules(level, kind, reason, pos),
        SpawnPredicate::Animal => check_animal_spawn_rules(level, reason, pos),
        SpawnPredicate::Mushroom => check_mushroom_spawn_rules(level, reason, pos),
        SpawnPredicate::Monster => check_monster_spawn_rules(level, kind, reason, pos),
        SpawnPredicate::AnyLightMonster => check_mob_spawn_rules(level, kind, reason, pos),
        SpawnPredicate::SurfaceMonsters => {
            check_monster_spawn_rules(level, kind, reason, pos)
                && (reason.is_spawner() || can_see_sky(level, pos))
        }
        SpawnPredicate::Stray => check_stray_spawn_rules(level, kind, reason, pos),
        SpawnPredicate::Rabbit => check_rabbit_spawn_rules(level, pos),
        SpawnPredicate::Ocelot => level.next_int(OCELOT_SPAWN_CHANCE) != 0,
        _ => return None,
    })
}

fn check_rabbit_spawn_rules(level: &impl SpawnLevel, pos: BlockPos) -> bool {
    has_tag(
        level.block_or_air(pos.offset(0, -1, 0)),
        RABBITS_SPAWNABLE_ON,
    ) && is_bright_enough_to_spawn(level, pos)
}

fn check_stray_spawn_rules(
    level: &mut impl RandomLevel,
    kind: EntityKind,
    reason: SpawnReason,
    pos: BlockPos,
) -> bool {
    let mut above = pos.offset(0, 1, 0);
    while level.block_or_air(above).kind() == Some(BlockKind::PowderSnow) {
        above = above.offset(0, 1, 0);
    }
    check_monster_spawn_rules(level, kind, reason, pos)
        && (reason.is_spawner() || can_see_sky(level, above.offset(0, -1, 0)))
}

pub fn rules_ported(kind: EntityKind) -> bool {
    let Some(spawn) = kind.mob_spawn() else {
        return false;
    };
    let predicate = spawn.placement.is_none_or(|placement| {
        matches!(
            placement.predicate,
            SpawnPredicate::Mob
                | SpawnPredicate::Animal
                | SpawnPredicate::Mushroom
                | SpawnPredicate::Monster
                | SpawnPredicate::AnyLightMonster
                | SpawnPredicate::SurfaceMonsters
                | SpawnPredicate::Stray
                | SpawnPredicate::Rabbit
                | SpawnPredicate::Ocelot
        )
    });
    let walk = match spawn.check_spawn_rules {
        MobClass::Mob => true,
        MobClass::PathfinderMob => matches!(
            spawn.walk_target_value,
            Some(
                MobClass::PathfinderMob
                    | MobClass::Animal
                    | MobClass::MushroomCow
                    | MobClass::Monster
            )
        ),
        _ => false,
    };
    predicate
        && walk
        && matches!(
            spawn.check_spawn_obstruction,
            MobClass::Mob | MobClass::Ocelot
        )
        && spawn.is_max_group_size_reached == MobClass::Mob
        && spawn.max_spawn_cluster_size.is_some()
        && super::despawn::remove_when_far_away(kind, super::despawn::FarAwayFacts::default())
            .is_some()
}

pub fn check_mob_spawn_rules(
    level: &impl SpawnLevel,
    kind: EntityKind,
    reason: SpawnReason,
    pos: BlockPos,
) -> bool {
    reason.is_spawner()
        || super::placement::is_valid_spawn(level.block_or_air(pos.offset(0, -1, 0)), kind)
}

pub fn is_bright_enough_to_spawn(level: &impl SpawnLevel, pos: BlockPos) -> bool {
    level.raw_brightness(pos, 0) > ANIMAL_MIN_BRIGHTNESS
}

pub fn check_animal_spawn_rules(
    level: &impl SpawnLevel,
    reason: SpawnReason,
    pos: BlockPos,
) -> bool {
    check_spawnable_on(level, reason, pos, ANIMALS_SPAWNABLE_ON)
}

pub fn check_mushroom_spawn_rules(
    level: &impl SpawnLevel,
    reason: SpawnReason,
    pos: BlockPos,
) -> bool {
    check_spawnable_on(level, reason, pos, MOOSHROOMS_SPAWNABLE_ON)
}

fn check_spawnable_on(
    level: &impl SpawnLevel,
    reason: SpawnReason,
    pos: BlockPos,
    tag: &str,
) -> bool {
    let bright = reason.ignores_light_requirements() || is_bright_enough_to_spawn(level, pos);
    has_tag(level.block_or_air(pos.offset(0, -1, 0)), tag) && bright
}

pub fn is_dark_enough_to_spawn(level: &mut impl RandomLevel, pos: BlockPos) -> bool {
    if level.sky_light(pos) > level.next_int(SKY_LIGHT_ROLL) {
        return false;
    }
    let dimension = level.dimension();
    let limit = dimension.monster_spawn_block_light_limit;
    if limit < UNLIMITED_BLOCK_LIGHT && level.block_light(pos) > limit {
        return false;
    }
    let light = if level.thundering() {
        level.max_local_raw_brightness(pos, THUNDER_DARKEN)
    } else {
        level.max_local_raw_brightness(pos, level.sky_darken())
    };
    let test = dimension.monster_spawn_light;
    light <= sample_count(test, level)
}

pub fn check_monster_spawn_rules(
    level: &mut impl RandomLevel,
    kind: EntityKind,
    reason: SpawnReason,
    pos: BlockPos,
) -> bool {
    (reason.ignores_light_requirements() || is_dark_enough_to_spawn(level, pos))
        && check_mob_spawn_rules(level, kind, reason, pos)
}

pub fn can_see_sky(level: &impl SpawnLevel, pos: BlockPos) -> bool {
    level.sky_light(pos) >= 15
}

pub fn walk_target_value(level: &impl SpawnLevel, kind: EntityKind, pos: BlockPos) -> Option<f32> {
    let owner = kind.mob_spawn()?.walk_target_value?;
    Some(match owner {
        MobClass::PathfinderMob => 0.0,
        MobClass::Animal => {
            if level.block_or_air(pos.offset(0, -1, 0)).kind() == Some(BlockKind::GrassBlock) {
                GRASS_WALK_TARGET
            } else {
                level.light_level_dependent_magic_value(pos) - 0.5
            }
        }
        MobClass::MushroomCow => {
            if level.block_or_air(pos.offset(0, -1, 0)).kind() == Some(BlockKind::Mycelium) {
                GRASS_WALK_TARGET
            } else {
                level.light_level_dependent_magic_value(pos) - 0.5
            }
        }
        MobClass::Monster => -(level.light_level_dependent_magic_value(pos) - 0.5),
        _ => return None,
    })
}

pub fn mob_check_spawn_rules(level: &impl SpawnLevel, kind: EntityKind, position: Vec3) -> bool {
    let Some(spawn) = kind.mob_spawn() else {
        return false;
    };
    match spawn.check_spawn_rules {
        MobClass::Mob => true,
        MobClass::PathfinderMob => {
            walk_target_value(level, kind, position.block()).is_some_and(|value| value >= 0.0)
        }
        _ => false,
    }
}

pub fn contains_any_liquid(level: &impl SpawnLevel, area: &Aabb) -> bool {
    let (x0, y0, z0) = (
        area.min_x.floor() as i32,
        area.min_y.floor() as i32,
        area.min_z.floor() as i32,
    );
    let (x1, y1, z1) = (
        area.max_x.floor() as i32,
        area.max_y.floor() as i32,
        area.max_z.floor() as i32,
    );
    (x0..=x1).any(|x| {
        (y0..=y1).any(|y| {
            (z0..=z1).any(|z| {
                level
                    .block_or_air(BlockPos::new(x, y, z))
                    .flags()
                    .has_fluid()
            })
        })
    })
}

pub fn check_spawn_obstruction(level: &impl SpawnLevel, kind: EntityKind, area: &Aabb) -> bool {
    match kind.mob_spawn().map(|spawn| spawn.check_spawn_obstruction) {
        Some(MobClass::Mob) => !contains_any_liquid(level, area) && level.is_unobstructed(area),
        Some(MobClass::Ocelot) => {
            level.is_unobstructed(area)
                && !contains_any_liquid(level, area)
                && ocelot_can_stand(level, area)
        }
        _ => false,
    }
}

fn ocelot_can_stand(level: &impl SpawnLevel, area: &Aabb) -> bool {
    let pos = BlockPos::new(
        ((area.min_x + area.max_x) / 2.0).floor() as i32,
        area.min_y.floor() as i32,
        ((area.min_z + area.max_z) / 2.0).floor() as i32,
    );
    if pos.y < level.sea_level() {
        return false;
    }
    let below = level.block_or_air(pos.offset(0, -1, 0));
    below.kind() == Some(BlockKind::GrassBlock) || has_tag(below, LEAVES)
}

pub fn max_spawn_cluster_size(kind: EntityKind) -> i32 {
    kind.mob_spawn()
        .and_then(|spawn| spawn.max_spawn_cluster_size)
        .map_or(DEFAULT_CLUSTER_SIZE, i32::from)
}

pub fn is_max_group_size_reached(_kind: EntityKind, _size: i32) -> bool {
    false
}

pub fn no_block_collision(level: &impl SpawnLevel, area: &Aabb) -> bool {
    crate::entity::physics::block_colliders(level, area).is_empty()
}

pub fn no_collision(level: &impl SpawnLevel, area: &Aabb) -> bool {
    no_block_collision(level, area) && level.no_entity_collision(area)
}

#[cfg(test)]
mod tests {
    use rspigot_blocks::{Block, GrassBlock, Mycelium, OakLeaves, Sand, Stone, Torch};

    use super::*;
    use crate::spawn::testing::Level;

    #[test]
    fn mooshrooms_need_mycelium_and_light() {
        let pos = BlockPos::new(4, 64, 4);
        let mut mycelium = Level::floor(Mycelium::DEFAULT_STATE, 63);
        assert!(check_spawn_rules(
            &mut mycelium,
            EntityKind::Mooshroom,
            SpawnReason::Natural,
            pos
        ));
        mycelium.sky = 8;
        assert!(!check_spawn_rules(
            &mut mycelium,
            EntityKind::Mooshroom,
            SpawnReason::Natural,
            pos
        ));
        let mut grass = Level::floor(GrassBlock::DEFAULT_STATE, 63);
        assert!(!check_spawn_rules(
            &mut grass,
            EntityKind::Mooshroom,
            SpawnReason::Natural,
            pos
        ));
    }

    #[test]
    fn strays_spawn_in_the_dark_under_the_open_sky() {
        let pos = BlockPos::new(4, 64, 4);
        let mut level = Level::floor(Stone::DEFAULT_STATE, 63);
        level.sky_darken = 15;
        assert!(check_spawn_rules(
            &mut level,
            EntityKind::Stray,
            SpawnReason::Natural,
            pos
        ));
        level.sky_darken = 0;
        assert!(!check_spawn_rules(
            &mut level,
            EntityKind::Stray,
            SpawnReason::Natural,
            pos
        ));
        assert!(rules_ported(EntityKind::Stray));
    }

    #[test]
    fn rabbits_need_their_own_ground_and_light() {
        let pos = BlockPos::new(4, 64, 4);
        let mut sand = Level::floor(Sand::DEFAULT_STATE, 63);
        assert!(check_spawn_rules(
            &mut sand,
            EntityKind::Rabbit,
            SpawnReason::Natural,
            pos
        ));
        sand.sky = 8;
        assert!(!check_spawn_rules(
            &mut sand,
            EntityKind::Rabbit,
            SpawnReason::Natural,
            pos
        ));
        let mut stone = Level::floor(Stone::DEFAULT_STATE, 63);
        assert!(!check_spawn_rules(
            &mut stone,
            EntityKind::Rabbit,
            SpawnReason::Natural,
            pos
        ));
        assert!(rules_ported(EntityKind::Rabbit));
    }

    #[test]
    fn rabbits_ignore_the_light_exemption_of_spawners() {
        let pos = BlockPos::new(4, 64, 4);
        let mut sand = Level::floor(Sand::DEFAULT_STATE, 63);
        sand.sky = 0;
        assert!(!check_spawn_rules(
            &mut sand,
            EntityKind::Rabbit,
            SpawnReason::Spawner,
            pos
        ));
    }

    #[test]
    fn ocelots_spawn_two_times_out_of_three() {
        let pos = BlockPos::new(4, 64, 4);
        let mut level = Level::floor(GrassBlock::DEFAULT_STATE, 63);
        level.rolls(&[0]);
        assert!(!check_spawn_rules(
            &mut level,
            EntityKind::Ocelot,
            SpawnReason::Natural,
            pos
        ));
        level.rolls(&[1]);
        assert!(check_spawn_rules(
            &mut level,
            EntityKind::Ocelot,
            SpawnReason::Natural,
            pos
        ));
        level.rolls(&[2]);
        assert!(check_spawn_rules(
            &mut level,
            EntityKind::Ocelot,
            SpawnReason::Natural,
            pos
        ));
        assert!(rules_ported(EntityKind::Ocelot));
    }

    #[test]
    fn ocelots_stand_on_grass_or_leaves_above_sea_level() {
        let area = Aabb::of_size(Vec3::new(4.5, 64.0, 4.5), 0.6, 0.7);
        let grass = Level::floor(GrassBlock::DEFAULT_STATE, 63);
        assert!(check_spawn_obstruction(&grass, EntityKind::Ocelot, &area));
        let stone = Level::floor(Stone::DEFAULT_STATE, 63);
        assert!(!check_spawn_obstruction(&stone, EntityKind::Ocelot, &area));
        let leaves = Level::floor(OakLeaves::DEFAULT_STATE, 63);
        assert!(check_spawn_obstruction(&leaves, EntityKind::Ocelot, &area));
        let low = Aabb::of_size(Vec3::new(4.5, 62.0, 4.5), 0.6, 0.7);
        let below = Level::floor(GrassBlock::DEFAULT_STATE, 61);
        assert!(!check_spawn_obstruction(&below, EntityKind::Ocelot, &low));
    }

    #[test]
    fn ocelots_refuse_liquids_and_crowds_when_spawning() {
        let area = Aabb::of_size(Vec3::new(4.5, 64.0, 4.5), 0.6, 0.7);
        let mut level = Level::floor(GrassBlock::DEFAULT_STATE, 63);
        level
            .obstacles
            .push(Aabb::of_size(Vec3::new(4.7, 64.0, 4.5), 0.6, 0.7));
        assert!(!check_spawn_obstruction(&level, EntityKind::Ocelot, &area));
        level.obstacles.clear();
        level.set(
            BlockPos::new(4, 64, 4),
            rspigot_blocks::Water::DEFAULT_STATE,
        );
        assert!(!check_spawn_obstruction(&level, EntityKind::Ocelot, &area));
    }

    #[test]
    fn mooshrooms_prefer_to_walk_on_mycelium() {
        let pos = BlockPos::new(4, 64, 4);
        let mycelium = Level::floor(Mycelium::DEFAULT_STATE, 63);
        assert_eq!(
            walk_target_value(&mycelium, EntityKind::Mooshroom, pos),
            Some(GRASS_WALK_TARGET)
        );
        let stone = Level::floor(Stone::DEFAULT_STATE, 63);
        assert!(
            walk_target_value(&stone, EntityKind::Mooshroom, pos)
                .is_some_and(|v| v < GRASS_WALK_TARGET)
        );
    }

    #[test]
    fn animals_need_spawnable_ground_and_light() {
        let mut level = Level::floor(GrassBlock::DEFAULT_STATE, 63);
        let pos = BlockPos::new(4, 64, 4);
        assert!(check_spawn_rules(
            &mut level,
            EntityKind::Cow,
            SpawnReason::Natural,
            pos
        ));
        level.sky = 8;
        assert!(!check_spawn_rules(
            &mut level,
            EntityKind::Cow,
            SpawnReason::Natural,
            pos
        ));
        level.block = 9;
        assert!(check_spawn_rules(
            &mut level,
            EntityKind::Cow,
            SpawnReason::Natural,
            pos
        ));
        level.set(pos.offset(0, -1, 0), Stone::DEFAULT_STATE);
        assert!(!check_spawn_rules(
            &mut level,
            EntityKind::Cow,
            SpawnReason::Natural,
            pos
        ));
    }

    #[test]
    fn monsters_roll_against_sky_light_then_the_dimension_light_test() {
        let mut level = Level::floor(Stone::DEFAULT_STATE, 63);
        let pos = BlockPos::new(4, 64, 4);
        level.sky = 15;
        level.rolls(&[14]);
        assert!(!check_spawn_rules(
            &mut level,
            EntityKind::Zombie,
            SpawnReason::Natural,
            pos
        ));
        assert_eq!(level.used(), 1);
        level.sky_darken = 11;
        level.rolls(&[20, 4]);
        assert!(check_spawn_rules(
            &mut level,
            EntityKind::Zombie,
            SpawnReason::Natural,
            pos
        ));
        assert_eq!(level.used(), 2);
        level.rolls(&[20, 3]);
        assert!(!check_spawn_rules(
            &mut level,
            EntityKind::Zombie,
            SpawnReason::Natural,
            pos
        ));
        level.block = 1;
        level.rolls(&[31, 7]);
        assert!(!check_spawn_rules(
            &mut level,
            EntityKind::Zombie,
            SpawnReason::Natural,
            pos
        ));
        assert_eq!(level.used(), 1);
    }

    #[test]
    fn thunder_darkens_the_sky_for_monsters() {
        let mut level = Level::floor(Stone::DEFAULT_STATE, 63);
        let pos = BlockPos::new(4, 64, 4);
        level.sky = 12;
        level.thundering = true;
        level.rolls(&[20, 2]);
        assert!(check_spawn_rules(
            &mut level,
            EntityKind::Creeper,
            SpawnReason::Natural,
            pos
        ));
    }

    #[test]
    fn peaceful_blocks_monsters_but_not_animals() {
        let mut level = Level::floor(GrassBlock::DEFAULT_STATE, 63);
        level.difficulty = Difficulty::Peaceful;
        let pos = BlockPos::new(4, 64, 4);
        level.rolls(&[31, 7]);
        level.sky = 0;
        assert!(!check_spawn_rules(
            &mut level,
            EntityKind::Zombie,
            SpawnReason::Natural,
            pos
        ));
        assert_eq!(level.used(), 0);
        level.sky = 15;
        assert!(check_spawn_rules(
            &mut level,
            EntityKind::Pig,
            SpawnReason::Natural,
            pos
        ));
    }

    #[test]
    fn walk_targets_prefer_grass_for_animals_and_darkness_for_monsters() {
        let mut level = Level::floor(GrassBlock::DEFAULT_STATE, 63);
        let pos = BlockPos::new(4, 64, 4);
        assert_eq!(
            walk_target_value(&level, EntityKind::Sheep, pos),
            Some(10.0)
        );
        level.sky = 15;
        assert!(walk_target_value(&level, EntityKind::Spider, pos).is_some_and(|v| v < 0.0));
        assert!(!mob_check_spawn_rules(
            &level,
            EntityKind::Spider,
            pos.center()
        ));
        level.sky = 0;
        assert!(mob_check_spawn_rules(
            &level,
            EntityKind::Spider,
            pos.center()
        ));
        level.set(pos, Torch::DEFAULT_STATE);
        assert!(walk_target_value(&level, EntityKind::Witch, pos).is_some());
    }

    #[test]
    fn obstruction_rejects_liquids_and_entities() {
        let mut level = Level::floor(Stone::DEFAULT_STATE, 63);
        let area = Aabb::of_size(Vec3::new(4.5, 64.0, 4.5), 0.6, 1.95);
        assert!(check_spawn_obstruction(&level, EntityKind::Zombie, &area));
        level
            .obstacles
            .push(Aabb::of_size(Vec3::new(4.7, 64.0, 4.5), 0.6, 1.8));
        assert!(!check_spawn_obstruction(&level, EntityKind::Zombie, &area));
        level.obstacles.clear();
        level.set(
            BlockPos::new(4, 65, 4),
            rspigot_blocks::Water::DEFAULT_STATE,
        );
        assert!(!check_spawn_obstruction(&level, EntityKind::Zombie, &area));
    }

    #[test]
    fn farm_animals_and_basic_monsters_have_ported_rules() {
        for kind in [
            EntityKind::Cow,
            EntityKind::Pig,
            EntityKind::Sheep,
            EntityKind::Chicken,
            EntityKind::Zombie,
            EntityKind::Skeleton,
            EntityKind::Creeper,
            EntityKind::Spider,
        ] {
            assert!(rules_ported(kind), "{kind:?}");
        }
        assert!(!rules_ported(EntityKind::Slime));
        assert!(!rules_ported(EntityKind::TropicalFish));
        assert!(!rules_ported(EntityKind::Item));
    }

    #[test]
    fn cluster_sizes_come_from_the_entity_table() {
        assert_eq!(max_spawn_cluster_size(EntityKind::Zombie), 4);
        assert_eq!(max_spawn_cluster_size(EntityKind::Wolf), 8);
        assert_eq!(max_spawn_cluster_size(EntityKind::Ghast), 1);
        assert!(!is_max_group_size_reached(EntityKind::Cow, 10));
    }
}
