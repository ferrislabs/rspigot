use rspigot_entities::EntityKind;

use super::SpawnReason;
use super::rules::{RandomLevel, check_mob_spawn_rules};
use crate::BlockPos;
use crate::entity::physics::has_tag;
use rspigot_entities::SpawnHeightmap;

const BATS_SPAWNABLE_ON: &str = "minecraft:bats_spawnable_on";
const LIGHT_ROLL: i32 = 4;

pub fn check_bat_spawn_rules(
    level: &mut impl RandomLevel,
    kind: EntityKind,
    reason: SpawnReason,
    pos: BlockPos,
) -> bool {
    if pos.y >= level.top_height(SpawnHeightmap::WorldSurface, pos.x, pos.z) {
        return false;
    }
    if level.next_boolean() {
        return false;
    }
    let light = level.max_local_raw_brightness(pos, level.sky_darken());
    if level.next_int(LIGHT_ROLL) < light {
        return false;
    }
    has_tag(level.block_or_air(pos.offset(0, -1, 0)), BATS_SPAWNABLE_ON)
        && check_mob_spawn_rules(level, kind, reason, pos)
}

#[cfg(test)]
mod tests {
    use rspigot_blocks::{Air, Block, Stone};

    use super::*;
    use crate::spawn::rules::{check_spawn_rules, rules_ported};
    use crate::spawn::testing::Level;

    const POS: BlockPos = BlockPos::new(4, 40, 4);

    fn cave() -> Level {
        let mut level = Level::floor(Stone::DEFAULT_STATE, 39);
        level
            .blocks
            .insert(BlockPos::new(4, 70, 4), Stone::DEFAULT_STATE);
        level.sky = 0;
        level
    }

    #[test]
    fn a_bat_spawns_under_the_surface_in_the_dark_on_the_flip_of_a_coin() {
        let mut level = cave();
        level.rolls(&[0, 0]);
        assert!(check_spawn_rules(
            &mut level,
            EntityKind::Bat,
            SpawnReason::Natural,
            POS
        ));
        assert_eq!(level.used(), 2);
    }

    #[test]
    fn a_heads_flip_refuses_before_the_light_roll_is_drawn() {
        let mut level = cave();
        level.rolls(&[1]);
        assert!(!check_spawn_rules(
            &mut level,
            EntityKind::Bat,
            SpawnReason::Natural,
            POS
        ));
        assert_eq!(level.used(), 1);
    }

    #[test]
    fn a_bat_at_or_above_the_surface_never_rolls() {
        let mut level = Level::floor(Stone::DEFAULT_STATE, 39);
        level.rolls(&[0, 0]);
        assert!(!check_spawn_rules(
            &mut level,
            EntityKind::Bat,
            SpawnReason::Natural,
            BlockPos::new(4, 40, 4)
        ));
        assert_eq!(level.used(), 0);
    }

    #[test]
    fn the_light_roll_must_reach_the_brightness_of_the_spot() {
        let mut level = cave();
        level.block = 2;
        level.rolls(&[0, 2]);
        assert!(check_spawn_rules(
            &mut level,
            EntityKind::Bat,
            SpawnReason::Natural,
            POS
        ));
        let mut lit = cave();
        lit.block = 2;
        lit.rolls(&[0, 1]);
        assert!(!check_spawn_rules(
            &mut lit,
            EntityKind::Bat,
            SpawnReason::Natural,
            POS
        ));
    }

    #[test]
    fn the_ground_must_be_a_bat_spawnable_block() {
        let mut level = Level::floor(Air::DEFAULT_STATE, 39);
        level
            .blocks
            .insert(BlockPos::new(4, 70, 4), Stone::DEFAULT_STATE);
        level.rolls(&[0, 0]);
        assert!(!check_spawn_rules(
            &mut level,
            EntityKind::Bat,
            SpawnReason::Natural,
            POS
        ));
    }

    #[test]
    fn the_bat_rules_are_ported() {
        assert!(rules_ported(EntityKind::Bat));
    }
}
