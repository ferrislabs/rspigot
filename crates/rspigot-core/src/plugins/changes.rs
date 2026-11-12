use rspigot_domain::BlockPos;
use rspigot_domain::BlockStateId;

use crate::plugins::block::{data_of, material_of};
use crate::plugins::decisions::Continuation;
use crate::plugins::events::EntityChangeBlock;
use crate::plugins::steps::{ChangeStep, Grief};
use crate::plugins::{EntityView, EventPayload};
use crate::ports::ClientLink;
use crate::server::Server;

impl<L: ClientLink> Server<L> {
    pub(crate) fn change_block(
        &mut self,
        entity: EntityView,
        pos: BlockPos,
        to: BlockStateId,
        change: Grief,
        refused: bool,
    ) {
        let lane = self.lane_for(None, entity.id);
        let then = Continuation::ChangeBlock(Box::new(ChangeStep { change }));
        let describe = move |_: &Self| {
            Some(EventPayload::EntityChangeBlock(EntityChangeBlock {
                block: pos,
                block_data: data_of(to),
                entity_type: entity.kind.clone(),
                entity,
                to: material_of(to).to_owned(),
            }))
        };
        if refused {
            self.fire_refused(lane, then, describe);
        } else {
            self.fire(lane, then, describe);
        }
    }

    pub(super) fn finish_change(&mut self, step: ChangeStep, cancelled: bool) {
        match step.change {
            Grief::Fall { pos, state, entity } => {
                if !cancelled {
                    self.finish_fall(pos, state, *entity);
                }
            }
            Grief::Land(landing) => self.finish_land(*landing, cancelled),
            Grief::Graze { sheep, pos, below } => self.finish_graze(sheep, pos, below, cancelled),
            Grief::Raid { pos, rabbit } => {
                if !cancelled {
                    self.finish_raid(pos, rabbit);
                }
            }
            Grief::Berries { fox, pos, glow } => {
                if !cancelled {
                    self.finish_berries(fox, pos, glow);
                }
            }
        }
    }
}
