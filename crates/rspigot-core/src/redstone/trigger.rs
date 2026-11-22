use rspigot_blocks::BlockKind;
use rspigot_domain::beehive::ReleaseStatus;
use rspigot_domain::explosion;
use rspigot_domain::{BlockPos, BlockStateId, Vec3};
use rspigot_entities::SoundSource;

use super::openers::is_opener;
use super::{MAX_POWER, RedstoneChange, RedstoneStep, facing, is_button};
use crate::mobs::ServerRandom;
use crate::ports::ClientLink;
use crate::server::Server;
use crate::signal::flag;

const CANDLE_OUT: &str = "minecraft:block.candle.extinguish";
const NO_RESISTANCE: f32 = 0.0;

fn is_candle(state: BlockStateId) -> bool {
    state
        .kind()
        .is_some_and(|kind| kind.name().ends_with("candle") || kind.name().ends_with("candle_cake"))
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn trigger_blocks_in_burst(&mut self, center: Vec3, radius: f32) {
        let height = self.world.dimension.height;
        let positions = {
            let mut random = ServerRandom(&mut self.rng);
            explosion::exploded_positions_capped(
                &self.world.chunks,
                height,
                center,
                radius,
                &mut random,
                Some(NO_RESISTANCE),
            )
        };
        for pos in positions {
            self.trigger_block(pos);
        }
    }

    fn trigger_block(&mut self, pos: BlockPos) {
        let state = self.block_or_air(pos);
        let Some(kind) = state.kind() else {
            return;
        };
        if kind == BlockKind::Lever {
            let powered = flag(state, "powered");
            let (old, new) = if powered {
                (MAX_POWER, 0)
            } else {
                (0, MAX_POWER)
            };
            self.ask_redstone(RedstoneStep {
                pos,
                old,
                new,
                change: RedstoneChange::Lever { player: None },
            });
        } else if is_button(kind) {
            if !flag(state, "powered") {
                self.ask_redstone(RedstoneStep {
                    pos,
                    old: 0,
                    new: MAX_POWER,
                    change: RedstoneChange::Button {
                        powered: true,
                        player: None,
                    },
                });
            }
        } else if is_opener(state) {
            self.explosion_toggle_opener(pos, state);
        } else if kind == BlockKind::Bell {
            let direction = facing(state).unwrap_or(rspigot_blocks::Face::North);
            self.ring_bell(pos, None, direction);
        } else if is_candle(state) && flag(state, "lit") {
            if let Some(out) = state.with_property("lit", "false") {
                self.set_block(pos, out);
                self.play_sound(
                    None,
                    pos.center(),
                    CANDLE_OUT,
                    SoundSource::Blocks,
                    1.0,
                    1.0,
                );
            }
        } else if crate::beehive::is_hive_block(state) {
            self.empty_hive(pos, state, None, ReleaseStatus::Emergency);
        }
    }
}
