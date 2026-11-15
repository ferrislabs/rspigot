use rspigot_domain::BlockPos;
use rspigot_domain::player::PlayerId;

use crate::plugins::events::{BlockRedstone, BlockRedstonePatch};
use crate::plugins::{Continuation, EventPayload, Lane};
use crate::ports::ClientLink;
use crate::server::Server;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RedstoneChange {
    Wire,
    Torch {
        lit: bool,
    },
    Lamp {
        lit: bool,
    },
    Lever {
        player: Option<PlayerId>,
    },
    Button {
        powered: bool,
        player: Option<PlayerId>,
    },
    Plate,
    Diode {
        on: bool,
    },
    Observer {
        on: bool,
    },
    Opener,
    Detector,
    Daylight,
    LightningRod,
}

#[derive(Debug, Clone)]
pub(crate) struct RedstoneStep {
    pub(crate) pos: BlockPos,
    pub(crate) old: i32,
    pub(crate) new: i32,
    pub(crate) change: RedstoneChange,
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn ask_redstone(&mut self, step: RedstoneStep) {
        let (pos, old, new) = (step.pos, step.old, step.new);
        self.fire(
            Lane::Block(pos),
            Continuation::Redstone(Box::new(step)),
            move |_| {
                Some(EventPayload::BlockRedstone(BlockRedstone {
                    block: pos,
                    new_current: new,
                    old_current: old,
                }))
            },
        );
    }

    pub(crate) fn finish_redstone(&mut self, step: RedstoneStep, patch: BlockRedstonePatch) {
        let RedstoneStep {
            pos,
            old,
            new,
            change,
        } = step;
        let new = patch.new_current.unwrap_or(new);
        if !self.world.chunks.is_loaded(pos.chunk()) {
            return;
        }
        match change {
            RedstoneChange::Wire => self.finish_wire(pos, old, new),
            RedstoneChange::Torch { lit } => self.finish_torch(pos, new, lit),
            RedstoneChange::Lamp { lit } => self.finish_lamp(pos, new, lit),
            RedstoneChange::Lever { player } => self.finish_lever(pos, old, new, player),
            RedstoneChange::Button { powered, player } => {
                self.finish_button(pos, new, powered, player);
            }
            RedstoneChange::Plate => self.finish_plate(pos, old, new),
            RedstoneChange::Diode { on } => self.finish_diode(pos, new, on),
            RedstoneChange::Observer { on } => self.finish_observer(pos, new, on),
            RedstoneChange::Opener => self.finish_opener(pos, new),
            RedstoneChange::Detector => self.finish_detector(pos, new),
            RedstoneChange::Daylight => self.finish_daylight(pos, new),
            RedstoneChange::LightningRod => self.finish_lightning_rod(pos, new),
        }
    }
}
