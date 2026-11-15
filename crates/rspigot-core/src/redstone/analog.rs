use rspigot_blocks::BlockKind;
use rspigot_domain::{BlockPos, BlockStateId};

use crate::ports::ClientLink;
use crate::server::Server;
use crate::signal::{flag, level};

const LAVA_CAULDRON_SIGNAL: i32 = 3;
const CAKE_SLICES: i32 = 7;
const ANCHOR_CHARGES: f32 = 4.0;

impl<L: ClientLink> Server<L> {
    pub(crate) fn analog_signal(&self, pos: BlockPos, state: BlockStateId) -> Option<i32> {
        match state.kind()? {
            BlockKind::Composter => Some(level(state, "level")),
            BlockKind::Cake => Some((CAKE_SLICES - level(state, "bites")) * 2),
            BlockKind::LavaCauldron => Some(LAVA_CAULDRON_SIGNAL),
            BlockKind::WaterCauldron | BlockKind::PowderSnowCauldron => Some(level(state, "level")),
            BlockKind::Beehive | BlockKind::BeeNest => Some(level(state, "honey_level")),
            BlockKind::RespawnAnchor => {
                Some((level(state, "charges") as f32 / ANCHOR_CHARGES * 15.0).floor() as i32)
            }
            BlockKind::EndPortalFrame => Some(if flag(state, "eye") { 15 } else { 0 }),
            BlockKind::Lectern => Some(self.lectern_signal(pos)),
            BlockKind::SculkSensor | BlockKind::CalibratedSculkSensor => {
                Some(level(state, "power"))
            }
            _ => self.container_fill_signal(pos, state),
        }
    }
}
