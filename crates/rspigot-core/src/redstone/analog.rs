use rspigot_blocks::{BlockKind, Face};
use rspigot_domain::entity::{Aabb, hanging};
use rspigot_domain::{BlockPos, BlockStateId};

use crate::ports::ClientLink;
use crate::server::Server;
use crate::signal::{flag, level};

const LAVA_CAULDRON_SIGNAL: i32 = 3;
const CAKE_SLICES: i32 = 7;
const ANCHOR_CHARGES: f32 = 4.0;
const CANDLE_CAKE_SIGNAL: i32 = 1;
const STATUE_POSES: [&str; 4] = ["standing", "sitting", "running", "star"];

fn statue_signal(state: BlockStateId) -> i32 {
    STATUE_POSES
        .iter()
        .position(|pose| state.property("copper_golem_pose") == Some(*pose))
        .map_or(0, |index| index as i32 + 1)
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn item_frame_signal(&self, pos: BlockPos, toward: Face) -> Option<i32> {
        let cell = Aabb::block(pos);
        self.entities
            .store
            .iter()
            .filter(|entity| {
                entity.base.removal.is_none()
                    && hanging::is_frame(entity.base.kind)
                    && hanging::facing(&entity.base) == toward
                    && entity.base.bounding_box().intersects(&cell)
            })
            .map(|entity| hanging::analog_output(&entity.base))
            .next()
    }

    pub(crate) fn analog_signal(&self, pos: BlockPos, state: BlockStateId) -> Option<i32> {
        let kind = state.kind()?;
        if super::bulb::is_bulb(kind) {
            return Some(if flag(state, "lit") { 15 } else { 0 });
        }
        let name = kind.name();
        if name.ends_with("candle_cake") {
            return Some(CANDLE_CAKE_SIGNAL);
        }
        if name.ends_with("copper_golem_statue") {
            return Some(statue_signal(state));
        }
        if crate::shelf::is_shelf_kind(kind) {
            return Some(self.shelf_signal(pos));
        }
        match kind {
            BlockKind::Composter => Some(level(state, "level")),
            BlockKind::Cake => Some((CAKE_SLICES - level(state, "bites")) * 2),
            BlockKind::LavaCauldron => Some(LAVA_CAULDRON_SIGNAL),
            BlockKind::WaterCauldron | BlockKind::PowderSnowCauldron => Some(level(state, "level")),
            BlockKind::Beehive | BlockKind::BeeNest => Some(level(state, "honey_level")),
            BlockKind::RespawnAnchor => {
                Some((level(state, "charges") as f32 / ANCHOR_CHARGES * 15.0).floor() as i32)
            }
            BlockKind::EndPortalFrame => Some(if flag(state, "eye") { 15 } else { 0 }),
            BlockKind::DetectorRail => Some(self.detector_comparator(pos, state)),
            BlockKind::ChiseledBookshelf => Some(self.bookshelf_comparator(pos)),
            BlockKind::DecoratedPot => Some(self.pot_comparator(pos)),
            BlockKind::Lectern => Some(self.lectern_signal(pos)),
            BlockKind::Jukebox => Some(self.jukebox_comparator(pos)),
            BlockKind::CommandBlock
            | BlockKind::ChainCommandBlock
            | BlockKind::RepeatingCommandBlock => Some(self.command_block_signal(pos)),
            BlockKind::SculkSensor | BlockKind::CalibratedSculkSensor => {
                Some(level(state, "power"))
            }
            _ => self.container_fill_signal(pos, state),
        }
    }
}
