use rspigot_domain::BlockPos;
use rspigot_domain::BlockStateId;

use crate::plugins::block::{data_of, material_of, snapshot_of};
use crate::plugins::decisions::Continuation;
use crate::plugins::events::{EntityBlockForm, EntityChangeBlock};
use crate::plugins::steps::{ChangeStep, FormStep, Grief};
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

    pub(crate) fn form_block(&mut self, entity: EntityView, step: FormStep) {
        let lane = self.lane_for(None, entity.id);
        let FormStep { pos, state, .. } = step;
        self.fire(lane, Continuation::Form(step), move |_| {
            Some(EventPayload::EntityBlockForm(EntityBlockForm {
                block: pos,
                entity,
                new_state: snapshot_of(pos, state),
            }))
        });
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
            Grief::Merge { silverfish, pos } => self.finish_merge(silverfish, pos, cancelled),
            Grief::WakeFriends {
                silverfish,
                origin,
                next,
                pos,
            } => self.finish_wake_friend(silverfish, origin, next, pos, cancelled),
            Grief::EndermanTake {
                enderman,
                pos,
                carried,
            } => {
                if !cancelled {
                    self.finish_enderman_take(enderman, pos, carried);
                }
            }
            Grief::EndermanPlace {
                enderman,
                pos,
                state,
            } => {
                if !cancelled {
                    self.finish_enderman_place(enderman, pos, state);
                }
            }
        }
    }
}
