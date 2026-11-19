use rspigot_domain::{BlockPos, BlockStateId};

use crate::ports::ClientLink;
use crate::server::Server;

impl<L: ClientLink> Server<L> {
    pub(crate) fn random_tick_crop(&mut self, _pos: BlockPos, _state: BlockStateId) -> bool {
        false
    }
}
