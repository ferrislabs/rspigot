use super::SpawnLevel;
use super::rules::is_bright_enough_to_spawn;
use crate::BlockPos;
use crate::entity::physics::has_tag;

const WOLVES_SPAWNABLE_ON: &str = "minecraft:wolves_spawnable_on";

pub fn check_wolf_spawn_rules(level: &impl SpawnLevel, pos: BlockPos) -> bool {
    has_tag(
        level.block_or_air(pos.offset(0, -1, 0)),
        WOLVES_SPAWNABLE_ON,
    ) && is_bright_enough_to_spawn(level, pos)
}
