use rspigot_blocks::Face;
use rspigot_domain::bed;
use rspigot_domain::{BlockPos, BlockStateId};

use crate::ports::ClientLink;
use crate::server::Server;

impl<L: ClientLink> Server<L> {
    pub(crate) fn bed_neighbour_changed(
        &mut self,
        pos: BlockPos,
        state: BlockStateId,
        towards: Face,
    ) {
        let neighbour = self
            .world
            .chunks
            .block(bed::step(pos, towards))
            .unwrap_or(BlockStateId::AIR);
        if !bed::orphaned(state, neighbour, towards) && !bed::is_partner(state, neighbour) {
            return;
        }
        let (Some(part), Some(facing)) = (bed::part_of(state), bed::facing_of(state)) else {
            return;
        };
        if towards != bed::neighbour_direction(part, facing) {
            return;
        }
        let next = if bed::is_partner(state, neighbour) {
            bed::heal_from_partner(state, neighbour)
        } else {
            BlockStateId::AIR
        };
        if next != state {
            self.set_block(pos, next);
        }
    }
}
