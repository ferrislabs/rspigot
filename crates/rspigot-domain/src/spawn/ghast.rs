use rspigot_entities::EntityKind;

use super::SpawnReason;
use super::rules::{RandomLevel, check_mob_spawn_rules};
use crate::BlockPos;
use crate::world::Difficulty;

const SPAWN_ODDS: i32 = 20;

pub fn check_ghast_spawn_rules(
    level: &mut impl RandomLevel,
    kind: EntityKind,
    reason: SpawnReason,
    pos: BlockPos,
) -> bool {
    level.difficulty() != Difficulty::Peaceful
        && level.next_int(SPAWN_ODDS) == 0
        && check_mob_spawn_rules(level, kind, reason, pos)
}

#[cfg(test)]
mod tests {
    use rspigot_blocks::{Block, Netherrack};

    use super::*;
    use crate::spawn::rules::{check_spawn_obstruction, check_spawn_rules, rules_ported};
    use crate::spawn::testing::Level;
    use crate::{Vec3, entity::Aabb};

    const POS: BlockPos = BlockPos::new(4, 64, 4);

    fn nether() -> Level {
        Level::floor(Netherrack::DEFAULT_STATE, 63)
    }

    #[test]
    fn a_ghast_spawns_on_one_roll_in_twenty() {
        for (roll, expected) in [(0, true), (1, false), (19, false)] {
            let mut level = nether();
            level.rolls(&[roll]);
            assert_eq!(
                check_spawn_rules(&mut level, EntityKind::Ghast, SpawnReason::Natural, POS),
                expected,
                "roll {roll}"
            );
        }
    }

    #[test]
    fn the_roll_is_drawn_after_the_peaceful_check_and_before_the_ground_check() {
        let mut level = nether();
        level.difficulty = Difficulty::Peaceful;
        level.rolls(&[0]);
        assert!(!check_spawn_rules(
            &mut level,
            EntityKind::Ghast,
            SpawnReason::Natural,
            POS
        ));
        assert_eq!(level.used(), 0);
        let mut bare = Level::floor(rspigot_blocks::Air::DEFAULT_STATE, 63);
        bare.rolls(&[0]);
        assert!(!check_spawn_rules(
            &mut bare,
            EntityKind::Ghast,
            SpawnReason::Natural,
            POS
        ));
        assert_eq!(bare.used(), 1);
    }

    #[test]
    fn a_spawner_ghast_needs_no_valid_ground_but_still_the_roll() {
        let mut bare = Level::floor(rspigot_blocks::Air::DEFAULT_STATE, 63);
        bare.rolls(&[0]);
        assert!(check_spawn_rules(
            &mut bare,
            EntityKind::Ghast,
            SpawnReason::Spawner,
            POS
        ));
        let mut missed = Level::floor(rspigot_blocks::Air::DEFAULT_STATE, 63);
        missed.rolls(&[7]);
        assert!(!check_spawn_rules(
            &mut missed,
            EntityKind::Ghast,
            SpawnReason::Spawner,
            POS
        ));
    }

    #[test]
    fn the_ghast_rules_are_ported_and_its_obstruction_refuses_crowds() {
        assert!(rules_ported(EntityKind::Ghast));
        let area = Aabb::of_size(Vec3::new(4.5, 64.0, 4.5), 4.0, 4.0);
        let mut level = nether();
        assert!(check_spawn_obstruction(&level, EntityKind::Ghast, &area));
        level.obstacles.push(area);
        assert!(!check_spawn_obstruction(&level, EntityKind::Ghast, &area));
    }
}
