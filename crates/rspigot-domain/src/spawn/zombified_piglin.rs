use rspigot_blocks::BlockKind;

use super::SpawnLevel;
use crate::BlockPos;
use crate::world::Difficulty;

pub fn check_zombified_piglin_spawn_rules(level: &impl SpawnLevel, pos: BlockPos) -> bool {
    level.difficulty() != Difficulty::Peaceful
        && level.block_or_air(pos.offset(0, -1, 0)).kind() != Some(BlockKind::NetherWartBlock)
}

#[cfg(test)]
mod tests {
    use rspigot_blocks::{Block, NetherWartBlock, Netherrack};
    use rspigot_entities::EntityKind;

    use super::*;
    use crate::spawn::SpawnReason;
    use crate::spawn::rules::{check_spawn_obstruction, check_spawn_rules, rules_ported};
    use crate::spawn::testing::Level;
    use crate::{Vec3, entity::Aabb};

    const POS: BlockPos = BlockPos::new(4, 64, 4);

    #[test]
    fn piglins_ignore_light_but_not_nether_wart_blocks() {
        let mut netherrack = Level::floor(Netherrack::DEFAULT_STATE, 63);
        netherrack.sky = 15;
        assert!(check_spawn_rules(
            &mut netherrack,
            EntityKind::ZombifiedPiglin,
            SpawnReason::Natural,
            POS
        ));
        let mut wart = Level::floor(NetherWartBlock::DEFAULT_STATE, 63);
        assert!(!check_spawn_rules(
            &mut wart,
            EntityKind::ZombifiedPiglin,
            SpawnReason::Natural,
            POS
        ));
    }

    #[test]
    fn piglins_never_spawn_in_peaceful() {
        let mut level = Level::floor(Netherrack::DEFAULT_STATE, 63);
        level.difficulty = Difficulty::Peaceful;
        assert!(!check_spawn_rules(
            &mut level,
            EntityKind::ZombifiedPiglin,
            SpawnReason::Natural,
            POS
        ));
    }

    #[test]
    fn piglin_spawn_obstruction_refuses_liquids_and_crowds() {
        let area = Aabb::of_size(Vec3::new(4.5, 64.0, 4.5), 0.6, 1.95);
        let mut level = Level::floor(Netherrack::DEFAULT_STATE, 63);
        assert!(check_spawn_obstruction(
            &level,
            EntityKind::ZombifiedPiglin,
            &area
        ));
        level.set(POS, rspigot_blocks::Lava::DEFAULT_STATE);
        assert!(!check_spawn_obstruction(
            &level,
            EntityKind::ZombifiedPiglin,
            &area
        ));
        level.set(POS, rspigot_blocks::Air::DEFAULT_STATE);
        level
            .obstacles
            .push(Aabb::of_size(Vec3::new(4.7, 64.0, 4.5), 0.6, 1.95));
        assert!(!check_spawn_obstruction(
            &level,
            EntityKind::ZombifiedPiglin,
            &area
        ));
        assert!(rules_ported(EntityKind::ZombifiedPiglin));
    }
}
