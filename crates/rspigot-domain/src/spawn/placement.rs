use rspigot_blocks::{BlockKind, Face, SpawnSurface, StateFlags, Support};
use rspigot_entities::{EntityKind, SpawnPlacementType};

use super::{SpawnLevel, block_within_border};
use crate::entity::physics::has_tag;
use crate::{BlockPos, BlockStateId};

const PREVENT_MOB_SPAWNING_INSIDE: &str = "minecraft:prevent_mob_spawning_inside";
const FIRE: &str = "minecraft:fire";
const CAMPFIRES: &str = "minecraft:campfires";
const NO_LIGHT_EMISSION_LIMIT: u8 = 14;

pub fn is_valid_spawn(state: BlockStateId, kind: EntityKind) -> bool {
    let Some(block) = state.kind() else {
        return false;
    };
    match block.spawn_surface() {
        SpawnSurface::Default => {
            state.is_face_sturdy(Face::Up, Support::Full)
                && state.light_emission() < NO_LIGHT_EMISSION_LIMIT
        }
        SpawnSurface::Never => false,
        SpawnSurface::Always => true,
        SpawnSurface::OcelotOrParrot => matches!(kind, EntityKind::Ocelot | EntityKind::Parrot),
        SpawnSurface::PolarBear => kind == EntityKind::PolarBear,
        SpawnSurface::FireImmune => kind.fire_immune(),
    }
}

pub fn is_burning_block(state: BlockStateId) -> bool {
    let lit_campfire = has_tag(state, CAMPFIRES) && state.property("lit") == Some("true");
    has_tag(state, FIRE)
        || matches!(
            state.kind(),
            Some(BlockKind::Lava | BlockKind::MagmaBlock | BlockKind::LavaCauldron)
        )
        || lit_campfire
}

pub fn is_block_dangerous(kind: EntityKind, state: BlockStateId) -> bool {
    let immune_to = kind.mob_spawn().map(|spawn| spawn.immune_to);
    if immune_to.is_some_and(|tag| has_tag(state, tag)) {
        return false;
    }
    if !kind.fire_immune() && is_burning_block(state) {
        return true;
    }
    matches!(
        state.kind(),
        Some(
            BlockKind::WitherRose
                | BlockKind::SweetBerryBush
                | BlockKind::Cactus
                | BlockKind::PowderSnow
        )
    )
}

pub fn is_valid_empty_spawn_block(state: BlockStateId, kind: EntityKind) -> bool {
    !state.flags().has_fluid() && is_valid_empty_spawn_block_ignoring_fluid(state, kind)
}

pub fn is_valid_empty_spawn_block_ignoring_fluid(state: BlockStateId, kind: EntityKind) -> bool {
    let flags = state.flags();
    !flags.contains(StateFlags::FULL_COLLISION_CUBE)
        && !flags.contains(StateFlags::SIGNAL_SOURCE)
        && !has_tag(state, PREVENT_MOB_SPAWNING_INSIDE)
        && !is_block_dangerous(kind, state)
}

pub fn top_face_is_full(state: BlockStateId) -> bool {
    const SLICE: f64 = 1.0 - 1.0e-7;
    let boxes: Vec<_> = state
        .collision_shape()
        .iter()
        .filter(|shape| shape.min_y < SLICE && shape.max_y > SLICE)
        .collect();
    if boxes.is_empty() {
        return false;
    }
    let mut xs = vec![0.0, 1.0];
    let mut zs = vec![0.0, 1.0];
    for shape in &boxes {
        xs.extend([shape.min_x, shape.max_x]);
        zs.extend([shape.min_z, shape.max_z]);
    }
    xs.sort_by(f64::total_cmp);
    zs.sort_by(f64::total_cmp);
    xs.windows(2)
        .filter(|pair| pair[0] >= 0.0 && pair[1] <= 1.0 && pair[1] - pair[0] > 1.0e-7)
        .all(|x| {
            zs.windows(2)
                .filter(|pair| pair[0] >= 0.0 && pair[1] <= 1.0 && pair[1] - pair[0] > 1.0e-7)
                .all(|z| {
                    let (cx, cz) = ((x[0] + x[1]) / 2.0, (z[0] + z[1]) / 2.0);
                    boxes.iter().any(|shape| {
                        shape.min_x <= cx
                            && cx <= shape.max_x
                            && shape.min_z <= cz
                            && cz <= shape.max_z
                    })
                })
        })
}

pub fn is_redstone_conductor(state: BlockStateId) -> bool {
    state.flags().contains(StateFlags::REDSTONE_CONDUCTOR)
}

pub fn placement_type(kind: EntityKind) -> SpawnPlacementType {
    kind.mob_spawn()
        .map_or(SpawnPlacementType::NoRestrictions, |spawn| {
            spawn.placement_type()
        })
}

pub fn is_spawn_position_ok(level: &impl SpawnLevel, kind: EntityKind, pos: BlockPos) -> bool {
    match placement_type(kind) {
        SpawnPlacementType::NoRestrictions => true,
        SpawnPlacementType::InWater => {
            block_within_border(pos)
                && level.block_or_air(pos).flags().contains(StateFlags::WATER)
                && !is_redstone_conductor(level.block_or_air(pos.offset(0, 1, 0)))
        }
        SpawnPlacementType::InLava => {
            block_within_border(pos) && level.block_or_air(pos).flags().contains(StateFlags::LAVA)
        }
        SpawnPlacementType::OnGround => {
            if !block_within_border(pos) {
                return false;
            }
            let above = pos.offset(0, 1, 0);
            let below = pos.offset(0, -1, 0);
            is_valid_spawn(level.block_or_air(below), kind)
                && is_valid_empty_spawn_block(level.block_or_air(pos), kind)
                && is_valid_empty_spawn_block(level.block_or_air(above), kind)
        }
    }
}

pub fn adjust_spawn_position(level: &impl SpawnLevel, kind: EntityKind, pos: BlockPos) -> BlockPos {
    if placement_type(kind) != SpawnPlacementType::OnGround {
        return pos;
    }
    let below = pos.offset(0, -1, 0);
    if level
        .block_or_air(below)
        .flags()
        .contains(StateFlags::PATHFINDABLE_LAND)
    {
        below
    } else {
        pos
    }
}

#[cfg(test)]
mod tests {
    use rspigot_blocks::{
        Block, Glass, GrassBlock, Lava, MagmaBlock, OakLeaves, RedstoneTorch, ShortGrass, Stone,
        Water,
    };

    use super::*;
    use crate::spawn::testing::Level;

    #[test]
    fn block_surfaces_follow_their_spawn_predicate() {
        assert!(is_valid_spawn(Stone::DEFAULT_STATE, EntityKind::Zombie));
        assert!(!is_valid_spawn(Glass::DEFAULT_STATE, EntityKind::Zombie));
        assert!(!is_valid_spawn(OakLeaves::DEFAULT_STATE, EntityKind::Cow));
        assert!(is_valid_spawn(OakLeaves::DEFAULT_STATE, EntityKind::Parrot));
        assert!(!is_valid_spawn(
            MagmaBlock::DEFAULT_STATE,
            EntityKind::Zombie
        ));
        assert!(is_valid_spawn(MagmaBlock::DEFAULT_STATE, EntityKind::Blaze));
        assert!(!is_valid_spawn(BlockStateId::AIR, EntityKind::Zombie));
    }

    #[test]
    fn empty_spawn_blocks_reject_solids_fluids_and_signals() {
        assert!(is_valid_empty_spawn_block(
            BlockStateId::AIR,
            EntityKind::Cow
        ));
        assert!(is_valid_empty_spawn_block(
            ShortGrass::DEFAULT_STATE,
            EntityKind::Cow
        ));
        assert!(!is_valid_empty_spawn_block(
            Stone::DEFAULT_STATE,
            EntityKind::Cow
        ));
        assert!(!is_valid_empty_spawn_block(
            Water::DEFAULT_STATE,
            EntityKind::Cow
        ));
        assert!(!is_valid_empty_spawn_block(
            Lava::DEFAULT_STATE,
            EntityKind::Cow
        ));
        assert!(!is_valid_empty_spawn_block(
            RedstoneTorch::DEFAULT_STATE,
            EntityKind::Cow
        ));
    }

    #[test]
    fn on_ground_needs_a_valid_floor_and_two_free_blocks() {
        let mut level = Level::floor(GrassBlock::DEFAULT_STATE, 63);
        let pos = BlockPos::new(3, 64, 3);
        assert!(is_spawn_position_ok(&level, EntityKind::Cow, pos));
        level.set(pos.offset(0, 1, 0), Stone::DEFAULT_STATE);
        assert!(!is_spawn_position_ok(&level, EntityKind::Cow, pos));
        assert!(!is_spawn_position_ok(
            &level,
            EntityKind::Cow,
            BlockPos::new(3, 70, 3)
        ));
        assert!(is_spawn_position_ok(
            &level,
            EntityKind::Phantom,
            BlockPos::new(3, 70, 3)
        ));
        assert!(is_spawn_position_ok(
            &level,
            EntityKind::Bee,
            BlockPos::new(3, 70, 3)
        ));
    }

    #[test]
    fn on_ground_positions_step_down_onto_pathfindable_blocks() {
        let mut level = Level::floor(GrassBlock::DEFAULT_STATE, 63);
        level.set(BlockPos::new(1, 64, 1), ShortGrass::DEFAULT_STATE);
        assert_eq!(
            adjust_spawn_position(&level, EntityKind::Cow, BlockPos::new(1, 65, 1)),
            BlockPos::new(1, 64, 1)
        );
        assert_eq!(
            adjust_spawn_position(&level, EntityKind::Cow, BlockPos::new(2, 64, 2)),
            BlockPos::new(2, 64, 2)
        );
    }
}
