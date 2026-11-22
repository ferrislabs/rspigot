use std::sync::OnceLock;

use rspigot_core::ports::{PlaceFailure, PlaceRequest, PlacedBlock};
use rspigot_domain::world::Dimension;
use rspigot_domain::{BlockPos, BlockStateId};
use serde_json::{Map, Value};

use crate::climate::Xoroshiro;
use crate::datapack::worldgen;
use crate::overworld::features::bodies::Compiler;
use crate::overworld::features::place::Run;
use crate::overworld::region::Region;
use crate::overworld::surface_rules::SurfaceSystem;
use crate::resources::Resources;

struct Shared {
    resources: Resources,
    surface: SurfaceSystem,
}

fn shared() -> &'static Shared {
    static SHARED: OnceLock<Shared> = OnceLock::new();
    SHARED.get_or_init(|| {
        let resources = Resources::resolve();
        let surface = SurfaceSystem::with_rule(1, &resources.blocks, None);
        Shared { resources, surface }
    })
}

pub(crate) fn grow(
    dimension: &Dimension,
    request: &PlaceRequest,
) -> Result<Vec<PlacedBlock>, PlaceFailure> {
    let PlaceRequest::Grow {
        feature,
        origin,
        clear,
        seed,
        window,
    } = request
    else {
        return Err(PlaceFailure::Failed);
    };
    let shared = shared();
    let data = worldgen();
    let empty = Map::new();
    let placed = data
        .get("placed")
        .and_then(Value::as_object)
        .unwrap_or(&empty);
    let configured = data
        .get("configured")
        .and_then(Value::as_object)
        .unwrap_or(&empty);
    let key = if feature.contains(':') {
        feature.clone()
    } else {
        format!("minecraft:{feature}")
    };
    if !configured.contains_key(&key) {
        return Err(PlaceFailure::Unknown);
    }
    let mut compiler = Compiler::new(placed, configured);
    let compiled = compiler.configured_ref(&Value::String(key));
    let mut region = Region::snapshot(
        &shared.resources,
        dimension,
        origin.chunk(),
        0,
        |chunk, buffer| {
            for dz in 0..16 {
                for dx in 0..16 {
                    let (x, z) = (chunk.min_block_x() + dx, chunk.min_block_z() + dz);
                    for y in window.min.y..window.min.y + window.size.1 {
                        if let Some(state) = window.get(BlockPos::new(x, y, z))
                            && dimension.height.contains(y)
                        {
                            buffer.set(dx as usize, y, dz as usize, state);
                        }
                    }
                }
            }
        },
    );
    for pos in clear {
        region.set(pos.x, pos.y, pos.z, BlockStateId::AIR);
    }
    region.set_random(Xoroshiro::from_seed(seed.wrapping_add(1000)));
    let mut rng = Xoroshiro::from_seed(*seed);
    let success = {
        let mut run = Run {
            region: &mut region,
            rng: &mut rng,
            surface: &shared.surface,
            feature_id: 0,
            seed: *seed,
            biome_has: &[],
            universal: &[],
            interest: None,
        };
        compiled.place(&mut run, (origin.x, origin.y, origin.z))
    };
    if !success {
        return Err(PlaceFailure::Failed);
    }
    let changed: Vec<PlacedBlock> = window
        .positions()
        .filter(|pos| dimension.height.contains(pos.y))
        .filter_map(|pos| {
            let now = region.get(pos.x, pos.y, pos.z);
            (window.get(pos) != Some(now)).then_some(PlacedBlock { pos, state: now })
        })
        .collect();
    if changed.is_empty() {
        Err(PlaceFailure::Failed)
    } else {
        Ok(changed)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use rspigot_blocks::BlockKind;
    use rspigot_core::grow_window::GrowWindow;
    use rspigot_domain::BlockPos;

    use super::*;

    const GROUND: i32 = 64;

    fn window(
        origin: BlockPos,
        floor: BlockKind,
        extra: &[(BlockPos, BlockStateId)],
    ) -> GrowWindow {
        let (min, size) = GrowWindow::around(origin, 8, 64, 16);
        let mut window = GrowWindow {
            min,
            size,
            states: Arc::from(vec![BlockStateId::AIR; (size.0 * size.1 * size.2) as usize]),
        };
        let mut states = vec![BlockStateId::AIR; window.states.len()];
        for pos in window.positions() {
            if pos.y < GROUND {
                states[window.index(pos).unwrap()] = BlockKind::Stone.default_state();
            } else if pos.y == GROUND {
                states[window.index(pos).unwrap()] = floor.default_state();
            }
        }
        for (pos, state) in extra {
            states[window.index(*pos).unwrap()] = *state;
        }
        window.states = states.into();
        window
    }

    fn request(feature: &str, origin: BlockPos, window: GrowWindow, seed: i64) -> PlaceRequest {
        PlaceRequest::Grow {
            feature: feature.to_owned(),
            origin,
            clear: Vec::new(),
            seed,
            window,
        }
    }

    fn count(blocks: &[PlacedBlock], kind: BlockKind) -> usize {
        blocks
            .iter()
            .filter(|block| block.state.kind() == Some(kind))
            .count()
    }

    fn try_seeds(feature: &str, origin: BlockPos, floor: BlockKind) -> Vec<PlacedBlock> {
        let dimension = Dimension::overworld();
        for seed in 1..40 {
            let window = window(origin, floor, &[]);
            if let Ok(blocks) = grow(&dimension, &request(feature, origin, window, seed)) {
                return blocks;
            }
        }
        panic!("{feature} never grew");
    }

    #[test]
    fn every_sapling_feature_places_its_own_logs_and_leaves() {
        let origin = BlockPos::new(8, GROUND + 1, 8);
        for (feature, log, leaves) in [
            ("oak", BlockKind::OakLog, BlockKind::OakLeaves),
            ("fancy_oak", BlockKind::OakLog, BlockKind::OakLeaves),
            ("birch", BlockKind::BirchLog, BlockKind::BirchLeaves),
            ("spruce", BlockKind::SpruceLog, BlockKind::SpruceLeaves),
            ("acacia", BlockKind::AcaciaLog, BlockKind::AcaciaLeaves),
            ("cherry", BlockKind::CherryLog, BlockKind::CherryLeaves),
            (
                "jungle_tree_no_vine",
                BlockKind::JungleLog,
                BlockKind::JungleLeaves,
            ),
            ("azalea_tree", BlockKind::OakLog, BlockKind::AzaleaLeaves),
            (
                "mangrove",
                BlockKind::MangroveLog,
                BlockKind::MangroveLeaves,
            ),
            (
                "tall_mangrove",
                BlockKind::MangroveLog,
                BlockKind::MangroveLeaves,
            ),
            (
                "red_poplar",
                BlockKind::PoplarLog,
                BlockKind::RedPoplarLeaves,
            ),
            (
                "orange_poplar",
                BlockKind::PoplarLog,
                BlockKind::OrangePoplarLeaves,
            ),
            (
                "yellow_poplar",
                BlockKind::PoplarLog,
                BlockKind::YellowPoplarLeaves,
            ),
        ] {
            let blocks = try_seeds(feature, origin, BlockKind::GrassBlock);
            let kinds: std::collections::BTreeSet<_> =
                blocks.iter().filter_map(|b| b.state.kind()).collect();
            assert!(
                count(&blocks, log) >= 2,
                "{feature} has no trunk: {kinds:?}"
            );
            assert!(count(&blocks, leaves) >= 5, "{feature} has no crown");
        }
    }

    #[test]
    fn the_two_by_two_features_need_their_four_cells_cleared_and_fill_them_with_trunk() {
        let origin = BlockPos::new(8, GROUND + 1, 8);
        let dimension = Dimension::overworld();
        let cells = [
            origin,
            origin.offset(1, 0, 0),
            origin.offset(0, 0, 1),
            origin.offset(1, 0, 1),
        ];
        let saplings: Vec<(BlockPos, BlockStateId)> = cells
            .iter()
            .map(|pos| (*pos, BlockKind::DarkOakSapling.default_state()))
            .collect();
        let mut grown = None;
        for seed in 1..40 {
            let window = window(origin, BlockKind::GrassBlock, &saplings);
            let mut asked = request("dark_oak", origin, window, seed);
            if let PlaceRequest::Grow { clear, .. } = &mut asked {
                clear.extend(cells);
            }
            if let Ok(blocks) = grow(&dimension, &asked) {
                grown = Some(blocks);
                break;
            }
        }
        let blocks = grown.expect("dark oak grew");
        assert!(count(&blocks, BlockKind::DarkOakLog) >= 4);
        for cell in cells {
            assert!(
                blocks.iter().any(|block| block.pos == cell
                    && block.state.kind() != Some(BlockKind::DarkOakSapling)),
                "{cell:?} still holds a sapling"
            );
        }
    }

    #[test]
    fn mega_features_make_a_wider_trunk_than_their_single_counterparts() {
        let origin = BlockPos::new(8, GROUND + 1, 8);
        let blocks = try_seeds("mega_jungle_tree", origin, BlockKind::GrassBlock);
        assert!(count(&blocks, BlockKind::JungleLog) >= 16);
        let blocks = try_seeds("mega_spruce", origin, BlockKind::Podzol);
        assert!(count(&blocks, BlockKind::SpruceLog) >= 16);
        let blocks = try_seeds("mega_pine", origin, BlockKind::Podzol);
        assert!(count(&blocks, BlockKind::SpruceLog) >= 16);
        let blocks = try_seeds("pale_oak_bonemeal", origin, BlockKind::GrassBlock);
        assert!(count(&blocks, BlockKind::PaleOakLog) >= 8);
    }

    #[test]
    fn the_bone_meal_features_place_their_plants_over_their_own_ground() {
        let origin = BlockPos::new(8, GROUND + 1, 8);
        let blocks = try_seeds("nylium_bonemeal", origin, BlockKind::CrimsonNylium);
        assert!(
            blocks.iter().any(|block| matches!(
                block.state.kind(),
                Some(BlockKind::CrimsonRoots | BlockKind::CrimsonFungus | BlockKind::WarpedFungus)
            )),
            "nothing grew on crimson nylium"
        );
        let blocks = try_seeds("nylium_bonemeal", origin, BlockKind::WarpedNylium);
        assert!(!blocks.is_empty());
        let blocks = try_seeds("moss_patch_bonemeal", origin, BlockKind::MossBlock);
        assert!(!blocks.is_empty());
        let blocks = try_seeds("pale_moss_patch_bonemeal", origin, BlockKind::PaleMossBlock);
        assert!(!blocks.is_empty());
        let blocks = try_seeds("huge_red_mushroom", origin, BlockKind::Mycelium);
        assert!(count(&blocks, BlockKind::RedMushroomBlock) > 4);
        let blocks = try_seeds("crimson_fungus_planted", origin, BlockKind::CrimsonNylium);
        assert!(count(&blocks, BlockKind::NetherWartBlock) > 4);
    }

    #[test]
    fn a_bees_variant_may_hang_a_nest_in_the_crown() {
        let origin = BlockPos::new(8, GROUND + 1, 8);
        let dimension = Dimension::overworld();
        let mut nests = 0;
        for seed in 1..200 {
            let window = window(origin, BlockKind::GrassBlock, &[]);
            if let Ok(blocks) = grow(&dimension, &request("oak_bees_005", origin, window, seed)) {
                nests += count(&blocks, BlockKind::BeeNest);
            }
        }
        assert!(nests > 0);
    }

    #[test]
    fn a_blocked_tree_fails_and_an_unknown_feature_is_refused() {
        let origin = BlockPos::new(8, GROUND + 1, 8);
        let dimension = Dimension::overworld();
        let roof: Vec<(BlockPos, BlockStateId)> = (-2..=2)
            .flat_map(|dx| {
                (-2..=2).map(move |dz| (origin.offset(dx, 2, dz), BlockKind::Stone.default_state()))
            })
            .collect();
        let window = window(origin, BlockKind::GrassBlock, &roof);
        assert_eq!(
            grow(&dimension, &request("birch", origin, window, 3)),
            Err(PlaceFailure::Failed)
        );
        let window = window_of_air(origin);
        assert_eq!(
            grow(&dimension, &request("no_such_tree", origin, window, 3)),
            Err(PlaceFailure::Unknown)
        );
    }

    fn window_of_air(origin: BlockPos) -> GrowWindow {
        window(origin, BlockKind::GrassBlock, &[])
    }
}
