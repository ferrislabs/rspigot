use rand::Rng;
use rspigot_blocks::{BlockKind, Face};
use rspigot_domain::{BlockPos, BlockStateId};
use rspigot_entities::SoundSource;

use super::{MAX_POWER, RedstoneChange, RedstoneStep, facing, step};
use crate::ports::ClientLink;
use crate::server::Server;
use crate::signal::flag;

const TOGGLE_WINDOW: u64 = 60;
const BURNOUT_TOGGLES: usize = 8;
const BURNOUT_DELAY: u64 = 160;
const TICK_DELAY: u64 = 2;
const BURNOUT_SOUND: &str = "minecraft:block.redstone_torch.burnout";

impl<L: ClientLink> Server<L> {
    fn torch_signal(&self, pos: BlockPos, state: BlockStateId) -> bool {
        if state.kind() == Some(BlockKind::RedstoneWallTorch) {
            let toward = facing(state).map_or(Face::South, Face::opposite);
            self.has_signal(step(pos, toward), toward)
        } else {
            self.has_signal(step(pos, Face::Down), Face::Down)
        }
    }

    fn toggled_too_frequently(&mut self, pos: BlockPos, add: bool) -> bool {
        let now = self.world.level.game_time as u64;
        if add {
            self.falls.redstone.toggles.push((pos, now));
        }
        self.falls
            .redstone
            .toggles
            .iter()
            .filter(|(at, _)| *at == pos)
            .count()
            >= BURNOUT_TOGGLES
    }

    pub(crate) fn torch_neighbor_changed(&mut self, pos: BlockPos, state: BlockStateId) {
        let Some(kind) = state.kind() else {
            return;
        };
        if flag(state, "lit") == self.torch_signal(pos, state)
            && !self.will_tick_this_tick(pos, kind)
        {
            self.schedule(pos, kind, TICK_DELAY);
        }
    }

    pub(crate) fn torch_tick(&mut self, pos: BlockPos, state: BlockStateId) {
        let now = self.world.level.game_time as u64;
        self.falls
            .redstone
            .toggles
            .retain(|(_, when)| now.saturating_sub(*when) <= TOGGLE_WINDOW);
        let powered = self.torch_signal(pos, state);
        let lit = flag(state, "lit");
        if lit && powered {
            self.ask_redstone(RedstoneStep {
                pos,
                old: MAX_POWER,
                new: 0,
                change: RedstoneChange::Torch { lit: false },
            });
        } else if !lit && !powered && !self.toggled_too_frequently(pos, false) {
            self.ask_redstone(RedstoneStep {
                pos,
                old: 0,
                new: MAX_POWER,
                change: RedstoneChange::Torch { lit: true },
            });
        }
    }

    pub(crate) fn finish_torch(&mut self, pos: BlockPos, new: i32, lit: bool) {
        let state = self.block_or_air(pos);
        let Some(kind) = state.kind() else {
            return;
        };
        if !matches!(
            kind,
            BlockKind::RedstoneTorch | BlockKind::RedstoneWallTorch
        ) || flag(state, "lit") == lit
        {
            return;
        }
        if new != if lit { MAX_POWER } else { 0 } {
            return;
        }
        let Some(next) = state.with_property("lit", if lit { "true" } else { "false" }) else {
            return;
        };
        self.set_block(pos, next);
        if !lit && self.toggled_too_frequently(pos, true) {
            let pitch = 2.6 + (self.rng.random::<f32>() - self.rng.random::<f32>()) * 0.8;
            self.play_sound(
                None,
                pos.center(),
                BURNOUT_SOUND,
                SoundSource::Blocks,
                0.5,
                pitch,
            );
            self.schedule(pos, kind, BURNOUT_DELAY);
        }
    }
}
