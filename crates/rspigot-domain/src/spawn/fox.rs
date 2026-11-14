use super::SpawnLevel;
use super::rules::is_bright_enough_to_spawn;
use crate::BlockPos;
use crate::entity::physics::has_tag;

const FOXES_SPAWNABLE_ON: &str = "minecraft:foxes_spawnable_on";

pub fn check_fox_spawn_rules(level: &impl SpawnLevel, pos: BlockPos) -> bool {
    has_tag(level.block_or_air(pos.offset(0, -1, 0)), FOXES_SPAWNABLE_ON)
        && is_bright_enough_to_spawn(level, pos)
}

#[cfg(test)]
mod tests {
    use rspigot_blocks::{Block, GrassBlock, Stone};
    use rspigot_entities::EntityKind;

    use crate::BlockPos;
    use crate::spawn::SpawnReason;
    use crate::spawn::rules::{check_spawn_rules, rules_ported};
    use crate::spawn::testing::Level;

    #[test]
    fn foxes_need_their_own_ground_and_light() {
        let pos = BlockPos::new(4, 64, 4);
        let mut grass = Level::floor(GrassBlock::DEFAULT_STATE, 63);
        assert!(check_spawn_rules(
            &mut grass,
            EntityKind::Fox,
            SpawnReason::Natural,
            pos
        ));
        grass.sky = 8;
        assert!(!check_spawn_rules(
            &mut grass,
            EntityKind::Fox,
            SpawnReason::Natural,
            pos
        ));
        let mut stone = Level::floor(Stone::DEFAULT_STATE, 63);
        assert!(!check_spawn_rules(
            &mut stone,
            EntityKind::Fox,
            SpawnReason::Natural,
            pos
        ));
        assert!(rules_ported(EntityKind::Fox));
    }
}
