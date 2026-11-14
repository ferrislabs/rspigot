use rspigot_blocks::infested::is_compatible_host_block;
use rspigot_entities::EntityKind;

use super::rules::check_mob_spawn_rules;
use super::{SpawnLevel, SpawnReason};
use crate::{BlockPos, Vec3};

const PLAYER_CLEARANCE: f64 = 5.0;
const HOST_WALK_TARGET: f32 = 10.0;

pub fn check_spawn_rules(
    level: &impl SpawnLevel,
    kind: EntityKind,
    reason: SpawnReason,
    pos: BlockPos,
) -> bool {
    if !check_mob_spawn_rules(level, kind, reason, pos) {
        return false;
    }
    if reason.is_spawner() {
        return true;
    }
    let center = Vec3::new(
        f64::from(pos.x) + 0.5,
        f64::from(pos.y) + 0.5,
        f64::from(pos.z) + 0.5,
    );
    !level.survival_player_within(center, PLAYER_CLEARANCE)
}

pub fn walk_target_value(level: &impl SpawnLevel, pos: BlockPos) -> f32 {
    let below = level.block_or_air(pos.offset(0, -1, 0));
    if is_compatible_host_block(below) {
        HOST_WALK_TARGET
    } else {
        -(level.light_level_dependent_magic_value(pos) - 0.5)
    }
}

#[cfg(test)]
mod tests {
    use rspigot_blocks::Block;
    use rspigot_blocks::{Dirt, InfestedStone, Stone};

    use super::*;
    use crate::spawn::rules::{
        check_spawn_rules as check, mob_check_spawn_rules, rules_ported, walk_target_value as walk,
    };
    use crate::spawn::testing::Level;

    const POS: BlockPos = BlockPos::new(4, 64, 4);

    #[test]
    fn spawns_on_valid_ground_without_light_limit() {
        let mut level = Level::floor(Stone::DEFAULT_STATE, 63);
        level.sky = 15;
        assert!(check(
            &mut level,
            EntityKind::Silverfish,
            SpawnReason::Natural,
            POS
        ));
    }

    #[test]
    fn needs_ground_to_spawn_on() {
        let mut level = Level::floor(rspigot_blocks::Water::DEFAULT_STATE, 63);
        assert!(!check(
            &mut level,
            EntityKind::Silverfish,
            SpawnReason::Natural,
            POS
        ));
    }

    #[test]
    fn refuses_spawns_within_five_blocks_of_a_player() {
        let mut level = Level::floor(Stone::DEFAULT_STATE, 63);
        level.players.push(Vec3::new(4.5, 64.5, 9.4));
        assert!(!check(
            &mut level,
            EntityKind::Silverfish,
            SpawnReason::Natural,
            POS
        ));
        level.players = vec![Vec3::new(4.5, 64.5, 9.6)];
        assert!(check(
            &mut level,
            EntityKind::Silverfish,
            SpawnReason::Natural,
            POS
        ));
    }

    #[test]
    fn spawners_ignore_players() {
        let mut level = Level::floor(Dirt::DEFAULT_STATE, 63);
        level.players.push(Vec3::new(4.5, 64.5, 4.5));
        assert!(check(
            &mut level,
            EntityKind::Silverfish,
            SpawnReason::Spawner,
            POS
        ));
        assert!(check(
            &mut level,
            EntityKind::Silverfish,
            SpawnReason::TrialSpawner,
            POS
        ));
    }

    #[test]
    fn peaceful_blocks_silverfish() {
        let mut level = Level::floor(Stone::DEFAULT_STATE, 63);
        level.difficulty = crate::world::Difficulty::Peaceful;
        assert!(!check(
            &mut level,
            EntityKind::Silverfish,
            SpawnReason::Natural,
            POS
        ));
    }

    #[test]
    fn walks_towards_infestable_blocks() {
        let mut level = Level::floor(InfestedStone::DEFAULT_STATE, 63);
        assert_eq!(
            walk(&level, EntityKind::Silverfish, POS),
            Some(-(level.light_level_dependent_magic_value(POS) - 0.5))
        );
        level.set(POS.offset(0, -1, 0), Stone::DEFAULT_STATE);
        assert_eq!(
            walk(&level, EntityKind::Silverfish, POS),
            Some(HOST_WALK_TARGET)
        );
        assert!(mob_check_spawn_rules(
            &level,
            EntityKind::Silverfish,
            POS.center()
        ));
    }

    #[test]
    fn darkness_decides_the_walk_value_over_other_blocks() {
        let mut level = Level::floor(Dirt::DEFAULT_STATE, 63);
        level.sky = 15;
        assert!(walk(&level, EntityKind::Silverfish, POS).is_some_and(|value| value < 0.0));
        assert!(!mob_check_spawn_rules(
            &level,
            EntityKind::Silverfish,
            POS.center()
        ));
        level.sky = 0;
        assert!(mob_check_spawn_rules(
            &level,
            EntityKind::Silverfish,
            POS.center()
        ));
    }

    #[test]
    fn rules_are_ported() {
        assert!(rules_ported(EntityKind::Silverfish));
    }

    #[test]
    fn giants_prefer_light_instead_of_darkness() {
        let mut level = Level::floor(Dirt::DEFAULT_STATE, 63);
        level.sky = 15;
        assert!(walk(&level, EntityKind::Giant, POS).is_some_and(|value| value > 0.0));
        assert!(mob_check_spawn_rules(
            &level,
            EntityKind::Giant,
            POS.center()
        ));
        level.sky = 0;
        assert!(!mob_check_spawn_rules(
            &level,
            EntityKind::Giant,
            POS.center()
        ));
        assert!(rules_ported(EntityKind::Giant));
    }
}
