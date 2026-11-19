use rand::Rng;
use rspigot_blocks::BlockKind;
use rspigot_domain::{BlockPos, BlockStateId};

use crate::plugins::steps::Nature;
use crate::ports::ClientLink;
use crate::server::Server;

const CHANCE: f32 = 0.056_888_89;
const REACH: i32 = 4;
const FIRST_STAGE_FACTOR: f32 = 0.75;

const FAMILIES: [[BlockKind; 4]; 15] = [
    [
        BlockKind::CopperBlock,
        BlockKind::ExposedCopper,
        BlockKind::WeatheredCopper,
        BlockKind::OxidizedCopper,
    ],
    [
        BlockKind::CutCopper,
        BlockKind::ExposedCutCopper,
        BlockKind::WeatheredCutCopper,
        BlockKind::OxidizedCutCopper,
    ],
    [
        BlockKind::CutCopperStairs,
        BlockKind::ExposedCutCopperStairs,
        BlockKind::WeatheredCutCopperStairs,
        BlockKind::OxidizedCutCopperStairs,
    ],
    [
        BlockKind::CutCopperSlab,
        BlockKind::ExposedCutCopperSlab,
        BlockKind::WeatheredCutCopperSlab,
        BlockKind::OxidizedCutCopperSlab,
    ],
    [
        BlockKind::ChiseledCopper,
        BlockKind::ExposedChiseledCopper,
        BlockKind::WeatheredChiseledCopper,
        BlockKind::OxidizedChiseledCopper,
    ],
    [
        BlockKind::CopperGrate,
        BlockKind::ExposedCopperGrate,
        BlockKind::WeatheredCopperGrate,
        BlockKind::OxidizedCopperGrate,
    ],
    [
        BlockKind::CopperBulb,
        BlockKind::ExposedCopperBulb,
        BlockKind::WeatheredCopperBulb,
        BlockKind::OxidizedCopperBulb,
    ],
    [
        BlockKind::CopperDoor,
        BlockKind::ExposedCopperDoor,
        BlockKind::WeatheredCopperDoor,
        BlockKind::OxidizedCopperDoor,
    ],
    [
        BlockKind::CopperTrapdoor,
        BlockKind::ExposedCopperTrapdoor,
        BlockKind::WeatheredCopperTrapdoor,
        BlockKind::OxidizedCopperTrapdoor,
    ],
    [
        BlockKind::CopperChest,
        BlockKind::ExposedCopperChest,
        BlockKind::WeatheredCopperChest,
        BlockKind::OxidizedCopperChest,
    ],
    [
        BlockKind::CopperGolemStatue,
        BlockKind::ExposedCopperGolemStatue,
        BlockKind::WeatheredCopperGolemStatue,
        BlockKind::OxidizedCopperGolemStatue,
    ],
    [
        BlockKind::LightningRod,
        BlockKind::ExposedLightningRod,
        BlockKind::WeatheredLightningRod,
        BlockKind::OxidizedLightningRod,
    ],
    [
        BlockKind::CopperBars,
        BlockKind::ExposedCopperBars,
        BlockKind::WeatheredCopperBars,
        BlockKind::OxidizedCopperBars,
    ],
    [
        BlockKind::CopperChain,
        BlockKind::ExposedCopperChain,
        BlockKind::WeatheredCopperChain,
        BlockKind::OxidizedCopperChain,
    ],
    [
        BlockKind::CopperLantern,
        BlockKind::ExposedCopperLantern,
        BlockKind::WeatheredCopperLantern,
        BlockKind::OxidizedCopperLantern,
    ],
];

fn stage_of(kind: BlockKind) -> Option<(usize, usize)> {
    FAMILIES.iter().enumerate().find_map(|(family, stages)| {
        stages
            .iter()
            .position(|candidate| *candidate == kind)
            .map(|stage| (family, stage))
    })
}

fn oxidation_odds(stage: usize, older: u32, same: u32) -> f32 {
    let share = (older + 1) as f32 / (older + same + 1) as f32;
    let factor = if stage == 0 { FIRST_STAGE_FACTOR } else { 1.0 };
    share * share * factor
}

fn advance(state: BlockStateId, next: BlockKind) -> Option<BlockStateId> {
    let properties: Vec<(&str, &str)> = state.properties().collect();
    next.state_with(&properties)
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn random_tick_copper(&mut self, pos: BlockPos, state: BlockStateId) -> bool {
        let Some(kind) = state.kind() else {
            return false;
        };
        let Some((family, stage)) = stage_of(kind) else {
            return false;
        };
        if self.rng.random::<f32>() >= CHANCE {
            return true;
        }
        let Some((older, same)) = self.copper_neighbours(pos, stage) else {
            return true;
        };
        if stage + 1 >= FAMILIES[family].len() {
            return true;
        }
        if self.rng.random::<f32>() >= oxidation_odds(stage, older, same) {
            return true;
        }
        if let Some(next) = advance(state, FAMILIES[family][stage + 1]) {
            self.nature_change(Nature::Form, pos, next, pos);
        }
        true
    }

    fn copper_neighbours(&self, pos: BlockPos, stage: usize) -> Option<(u32, u32)> {
        let mut older = 0;
        let mut same = 0;
        for dx in -REACH..=REACH {
            for dy in -(REACH - dx.abs())..=(REACH - dx.abs()) {
                let rest = REACH - dx.abs() - dy.abs();
                for dz in -rest..=rest {
                    if dx == 0 && dy == 0 && dz == 0 {
                        continue;
                    }
                    let Some(other) = self.block_or_air(pos.offset(dx, dy, dz)).kind() else {
                        continue;
                    };
                    let Some((_, other_stage)) = stage_of(other) else {
                        continue;
                    };
                    match other_stage.cmp(&stage) {
                        std::cmp::Ordering::Less => return None,
                        std::cmp::Ordering::Equal => same += 1,
                        std::cmp::Ordering::Greater => older += 1,
                    }
                }
            }
        }
        Some((older, same))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use super::*;
    use crate::config::{GameConfig, LevelData};
    use crate::status::StatusBoard;
    use crate::world_state::tests::Quiet;

    fn server() -> Server<Quiet> {
        let config = GameConfig {
            chunk_workers: 1,
            spawn_chunk_radius: 1,
            ..GameConfig::default()
        };
        let mut server = Server::new(
            config,
            LevelData::default(),
            Arc::new(Quiet),
            Arc::new(Quiet),
            StatusBoard::new(1),
        )
        .with_rng_seed(5);
        server.wait_for_spawn(Duration::from_secs(10));
        server
    }

    fn put(server: &mut Server<Quiet>, pos: BlockPos, kind: BlockKind) -> BlockStateId {
        let state = kind.default_state();
        server.set_block(pos, state);
        state
    }

    fn tick_many(server: &mut Server<Quiet>, pos: BlockPos, times: u32) -> bool {
        let mut weathering = true;
        for _ in 0..times {
            let state = server.block_or_air(pos);
            weathering &= server.random_tick_copper(pos, state);
            if server.block_or_air(pos) != state {
                break;
            }
        }
        weathering
    }

    #[test]
    fn odds_follow_the_neighbour_counts() {
        assert_eq!(oxidation_odds(0, 0, 0), 0.75);
        assert_eq!(oxidation_odds(1, 0, 0), 1.0);
        assert_eq!(oxidation_odds(1, 1, 1), (2.0f32 / 3.0).powi(2));
        assert_eq!(oxidation_odds(2, 0, 3), 0.0625);
    }

    #[test]
    fn other_blocks_are_not_copper() {
        let mut server = server();
        let pos = BlockPos::new(0, 80, 0);
        let stone = put(&mut server, pos, BlockKind::Stone);
        assert!(!server.random_tick_copper(pos, stone));
        let waxed = put(&mut server, pos, BlockKind::WaxedCopperBlock);
        assert!(!server.random_tick_copper(pos, waxed));
    }

    #[test]
    fn a_lone_copper_block_oxidises_one_stage() {
        let mut server = server();
        let pos = BlockPos::new(0, 80, 0);
        put(&mut server, pos, BlockKind::CopperBlock);
        assert!(tick_many(&mut server, pos, 2000));
        let kind = server.block_or_air(pos).kind();
        assert_ne!(kind, Some(BlockKind::CopperBlock));
        assert!(kind.is_some_and(|kind| stage_of(kind).is_some()));
    }

    #[test]
    fn oxidised_copper_stays_put() {
        let mut server = server();
        let pos = BlockPos::new(0, 80, 0);
        put(&mut server, pos, BlockKind::OxidizedCopper);
        assert!(tick_many(&mut server, pos, 2000));
        assert_eq!(
            server.block_or_air(pos).kind(),
            Some(BlockKind::OxidizedCopper)
        );
    }

    #[test]
    fn a_less_weathered_neighbour_blocks_the_change() {
        let mut server = server();
        let pos = BlockPos::new(0, 80, 0);
        put(&mut server, pos, BlockKind::ExposedCopper);
        put(&mut server, pos.offset(3, 1, 0), BlockKind::CopperBlock);
        assert!(tick_many(&mut server, pos, 2000));
        assert_eq!(
            server.block_or_air(pos).kind(),
            Some(BlockKind::ExposedCopper)
        );
    }

    #[test]
    fn a_neighbour_past_the_reach_does_not_block() {
        let mut server = server();
        let pos = BlockPos::new(0, 80, 0);
        put(&mut server, pos, BlockKind::ExposedCopper);
        put(&mut server, pos.offset(3, 2, 0), BlockKind::CopperBlock);
        tick_many(&mut server, pos, 2000);
        assert_eq!(
            server.block_or_air(pos).kind(),
            Some(BlockKind::WeatheredCopper)
        );
    }

    #[test]
    fn waxed_neighbours_are_ignored() {
        let mut server = server();
        let pos = BlockPos::new(0, 80, 0);
        put(&mut server, pos, BlockKind::ExposedCopper);
        put(
            &mut server,
            pos.offset(1, 0, 0),
            BlockKind::WaxedCopperBlock,
        );
        tick_many(&mut server, pos, 2000);
        assert_eq!(
            server.block_or_air(pos).kind(),
            Some(BlockKind::WeatheredCopper)
        );
    }

    #[test]
    fn the_change_keeps_the_block_properties() {
        let mut server = server();
        let pos = BlockPos::new(0, 80, 0);
        let stairs = BlockKind::CutCopperStairs
            .state_with(&[("facing", "east"), ("half", "top"), ("shape", "straight")])
            .unwrap_or_default();
        server.set_block(pos, stairs);
        tick_many(&mut server, pos, 2000);
        let after = server.block_or_air(pos);
        assert_eq!(after.kind(), Some(BlockKind::ExposedCutCopperStairs));
        assert_eq!(after.property("facing"), Some("east"));
        assert_eq!(after.property("half"), Some("top"));
    }
}
