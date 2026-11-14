use rspigot_blocks::BlockKind;

use super::SpawnLevel;
use crate::BlockPos;

pub fn check_piglin_spawn_rules(level: &impl SpawnLevel, pos: BlockPos) -> bool {
    level.block_or_air(pos.offset(0, -1, 0)).kind() != Some(BlockKind::NetherWartBlock)
}

pub fn hoglin_walk_target_value(level: &impl SpawnLevel, pos: BlockPos) -> f32 {
    if level.block_or_air(pos.offset(0, -1, 0)).kind() == Some(BlockKind::CrimsonNylium) {
        CRIMSON_NYLIUM_WALK_TARGET
    } else {
        0.0
    }
}

const CRIMSON_NYLIUM_WALK_TARGET: f32 = 10.0;

#[cfg(test)]
mod tests {
    use rspigot_blocks::{Block, CrimsonNylium, NetherWartBlock, Netherrack};
    use rspigot_entities::EntityKind;

    use super::*;
    use crate::spawn::SpawnReason;
    use crate::spawn::rules::{check_spawn_rules, rules_ported, walk_target_value};
    use crate::spawn::testing::Level;
    use crate::world::Difficulty;

    const POS: BlockPos = BlockPos::new(4, 64, 4);

    #[test]
    fn piglins_and_hoglins_refuse_nether_wart_blocks_but_not_netherrack() {
        for kind in [EntityKind::Piglin, EntityKind::Hoglin] {
            let mut floor = Level::floor(Netherrack::DEFAULT_STATE, 63);
            assert!(check_spawn_rules(
                &mut floor,
                kind,
                SpawnReason::Natural,
                POS
            ));
            let mut wart = Level::floor(NetherWartBlock::DEFAULT_STATE, 63);
            assert!(!check_spawn_rules(
                &mut wart,
                kind,
                SpawnReason::Natural,
                POS
            ));
        }
    }

    #[test]
    fn hoglins_never_spawn_in_peaceful_but_piglins_do() {
        let mut level = Level::floor(Netherrack::DEFAULT_STATE, 63);
        level.difficulty = Difficulty::Peaceful;
        assert!(!check_spawn_rules(
            &mut level,
            EntityKind::Hoglin,
            SpawnReason::Natural,
            POS
        ));
        assert!(check_spawn_rules(
            &mut level,
            EntityKind::Piglin,
            SpawnReason::Natural,
            POS
        ));
    }

    #[test]
    fn hoglins_like_crimson_nylium() {
        let nylium = Level::floor(CrimsonNylium::DEFAULT_STATE, 63);
        let rock = Level::floor(Netherrack::DEFAULT_STATE, 63);
        assert_eq!(
            walk_target_value(&nylium, EntityKind::Hoglin, POS),
            Some(10.0)
        );
        assert_eq!(walk_target_value(&rock, EntityKind::Hoglin, POS), Some(0.0));
    }

    #[test]
    fn the_nether_mobs_have_their_spawn_rules_ported() {
        for kind in [
            EntityKind::Piglin,
            EntityKind::PiglinBrute,
            EntityKind::Hoglin,
            EntityKind::Zoglin,
        ] {
            assert!(rules_ported(kind), "{kind:?}");
        }
    }
}
