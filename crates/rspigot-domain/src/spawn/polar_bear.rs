use super::rules::{check_animal_spawn_rules, is_bright_enough_to_spawn};
use super::{SpawnLevel, SpawnReason};
use crate::BlockPos;
use crate::biome;
use crate::entity::physics::has_tag;

const ALTERNATE_BIOMES: &str = "minecraft:polar_bears_spawn_on_alternate_blocks";
const ALTERNATE_BLOCKS: &str = "minecraft:polar_bears_spawnable_on_alternate";

pub fn check_polar_bear_spawn_rules(
    level: &impl SpawnLevel,
    reason: SpawnReason,
    pos: BlockPos,
) -> bool {
    if !biome::in_tag(level.biome(pos), ALTERNATE_BIOMES) {
        return check_animal_spawn_rules(level, reason, pos);
    }
    is_bright_enough_to_spawn(level, pos)
        && has_tag(level.block_or_air(pos.offset(0, -1, 0)), ALTERNATE_BLOCKS)
}

#[cfg(test)]
mod tests {
    use rspigot_blocks::{Block, GrassBlock, Ice, Stone};
    use rspigot_entities::EntityKind;

    use super::*;
    use crate::BiomeId;
    use crate::spawn::rules::{check_spawn_rules, rules_ported};
    use crate::spawn::testing::Level;

    fn level(floor: crate::BlockStateId, biome: &str) -> Level {
        let mut level = Level::floor(floor, 63);
        level.biome = BiomeId(rspigot_data::registries::biome_id(biome).unwrap());
        level
    }

    #[test]
    fn bears_use_the_animal_rules_in_ordinary_biomes() {
        let pos = BlockPos::new(4, 64, 4);
        let mut grass = level(GrassBlock::DEFAULT_STATE, "minecraft:plains");
        assert!(check_spawn_rules(
            &mut grass,
            EntityKind::PolarBear,
            SpawnReason::Natural,
            pos
        ));
        let mut stone = level(Stone::DEFAULT_STATE, "minecraft:plains");
        assert!(!check_spawn_rules(
            &mut stone,
            EntityKind::PolarBear,
            SpawnReason::Natural,
            pos
        ));
        assert!(rules_ported(EntityKind::PolarBear));
    }

    #[test]
    fn bears_walk_on_ice_in_the_frozen_ocean_and_nothing_else() {
        let pos = BlockPos::new(4, 64, 4);
        let mut ice = level(Ice::DEFAULT_STATE, "minecraft:frozen_ocean");
        assert!(check_spawn_rules(
            &mut ice,
            EntityKind::PolarBear,
            SpawnReason::Natural,
            pos
        ));
        let mut grass = level(GrassBlock::DEFAULT_STATE, "minecraft:frozen_ocean");
        assert!(!check_spawn_rules(
            &mut grass,
            EntityKind::PolarBear,
            SpawnReason::Natural,
            pos
        ));
        let mut dark = level(Ice::DEFAULT_STATE, "minecraft:frozen_ocean");
        dark.sky = 0;
        assert!(!check_spawn_rules(
            &mut dark,
            EntityKind::PolarBear,
            SpawnReason::Natural,
            pos
        ));
    }

    #[test]
    fn the_alternate_biomes_ignore_the_spawner_light_exemption() {
        let pos = BlockPos::new(4, 64, 4);
        let mut dark = level(Ice::DEFAULT_STATE, "minecraft:frozen_ocean");
        dark.sky = 0;
        assert!(!check_spawn_rules(
            &mut dark,
            EntityKind::PolarBear,
            SpawnReason::TrialSpawner,
            pos
        ));
    }
}
