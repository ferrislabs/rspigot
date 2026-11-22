use rspigot_blocks::BlockKind;
use rspigot_domain::{BlockPos, BlockStateId, Vec3};
use rspigot_entities::SoundSource;

use crate::ports::ClientLink;
use crate::server::Server;
use crate::signal::flag;

const TURN_ON: &str = "minecraft:block.copper_bulb.turn_on";
const TURN_OFF: &str = "minecraft:block.copper_bulb.turn_off";

pub(crate) fn is_bulb(kind: BlockKind) -> bool {
    kind.name().ends_with("copper_bulb")
}

fn with(state: BlockStateId, name: &str, value: bool) -> BlockStateId {
    state
        .with_property(name, if value { "true" } else { "false" })
        .unwrap_or(state)
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn bulb_check(&mut self, pos: BlockPos, state: BlockStateId) {
        let signal = self.has_neighbor_signal(pos);
        if signal == flag(state, "powered") {
            return;
        }
        let mut next = state;
        if !flag(state, "powered") {
            let lit = !flag(state, "lit");
            next = with(next, "lit", lit);
            self.play_sound(
                None,
                Vec3::new(
                    f64::from(pos.x) + 0.5,
                    f64::from(pos.y) + 0.5,
                    f64::from(pos.z) + 0.5,
                ),
                if lit { TURN_ON } else { TURN_OFF },
                SoundSource::Blocks,
                1.0,
                1.0,
            );
        }
        self.set_block(pos, with(next, "powered", signal));
    }
}
