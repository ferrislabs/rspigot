use rand::SeedableRng;
use rand::rngs::StdRng;
use rspigot_blocks::BlockKind;
use rspigot_domain::player::PlayerId;
use rspigot_domain::sea_pickle::{merged, plan_bone_meal};
use rspigot_domain::{BlockPos, BlockStateId};

use crate::mobs::ServerRandom;
use crate::plugins::steps::{BuildStep, UseItem};
use crate::ports::ClientLink;
use crate::server::Server;

const CORAL_BLOCKS: &str = "minecraft:coral_blocks";

fn is_coral(state: BlockStateId) -> bool {
    state.kind().is_some_and(|kind| {
        rspigot_data::tags::tag("minecraft:block", CORAL_BLOCKS)
            .is_some_and(|ids| ids.binary_search(&u32::from(kind.id().0)).is_ok())
    })
}

pub(crate) fn is_sea_pickle(state: BlockStateId) -> bool {
    state.kind() == Some(BlockKind::SeaPickle)
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn stack_sea_pickle(
        &mut self,
        using: UseItem,
        pos: BlockPos,
        clicked: BlockStateId,
    ) {
        let id = using.player;
        let Some(next) = merged(clicked) else {
            return;
        };
        if !self.within_reach(id, pos) || self.is_protected(id, pos) {
            self.refuse_block(id, pos, using.sequence);
            return;
        }
        self.ask_can_build(BuildStep {
            using,
            target: pos,
            old: clicked,
            state: next,
            linked: None,
            buildable: true,
        });
    }

    pub(crate) fn grow_sea_pickle(&mut self, player: PlayerId, pos: BlockPos, state: BlockStateId) {
        let seed: u64 = rand::Rng::random(&mut self.rng);
        let planned = {
            let block = |at: BlockPos| self.block_or_air(at);
            let mut rng = StdRng::seed_from_u64(seed);
            plan_bone_meal(pos, state, &block, &is_coral, &mut ServerRandom(&mut rng))
        };
        self.fertilize(player, pos, planned);
    }
}
