use rspigot_domain::BlockPos;
use rspigot_domain::entity::living;

use crate::plugins::decisions::Continuation;
use crate::plugins::events::{EntityCombust, EntityCombustByBlock, EntityCombustByEntity};
use crate::plugins::steps::{CombustStep, Holder, Igniter};
use crate::plugins::{EntityView, EventKind, EventPatch, EventPayload};
use crate::ports::ClientLink;
use crate::server::Server;

#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)]
pub(crate) enum Combuster {
    Nothing,
    Block(Option<BlockPos>),
    Entity(EntityView),
}

pub(crate) fn settled_duration(patch: Option<EventPatch>) -> Option<f32> {
    match patch {
        Some(EventPatch::EntityCombust(patch)) => patch.duration,
        Some(EventPatch::EntityCombustByBlock(patch)) => patch.duration,
        Some(EventPatch::EntityCombustByEntity(patch)) => patch.duration,
        _ => None,
    }
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn combust(&mut self, target: Holder, seconds: f32, combuster: Combuster) {
        let entity = match target {
            Holder::Player(id) => self.players.get(&id).map(|player| player.entity_id),
            Holder::Mob(id) => Some(id),
        };
        let Some(entity) = entity else {
            return;
        };
        let (kind, igniter) = match &combuster {
            Combuster::Nothing => (EventKind::EntityCombust, Igniter::Nothing),
            Combuster::Block(block) => (EventKind::EntityCombustByBlock, Igniter::Block(*block)),
            Combuster::Entity(source) => {
                (EventKind::EntityCombustByEntity, Igniter::Entity(source.id))
            }
        };
        let lane = self.lane_for(None, entity);
        let step = CombustStep {
            kind,
            target,
            igniter,
            seconds,
        };
        self.fire(lane, Continuation::Combust(step), move |server| {
            let entity = server.subject_view(entity)?;
            let entity_type = entity.kind.clone();
            Some(match combuster {
                Combuster::Nothing => EventPayload::EntityCombust(EntityCombust {
                    duration: seconds,
                    entity,
                    entity_type,
                }),
                Combuster::Block(block) => {
                    EventPayload::EntityCombustByBlock(EntityCombustByBlock {
                        combuster: block,
                        duration: seconds,
                        entity,
                        entity_type,
                    })
                }
                Combuster::Entity(source) => {
                    EventPayload::EntityCombustByEntity(EntityCombustByEntity {
                        combuster: source,
                        duration: seconds,
                        entity,
                        entity_type,
                    })
                }
            })
        });
    }

    pub(super) fn finish_combust(
        &mut self,
        step: CombustStep,
        cancelled: bool,
        duration: Option<f32>,
    ) {
        if cancelled {
            return;
        }
        let seconds = duration.unwrap_or(step.seconds);
        match step.target {
            Holder::Player(id) => {
                if self
                    .players
                    .get(&id)
                    .is_some_and(|player| !player.vitals.is_dead())
                {
                    self.ignite_player(id, seconds);
                }
            }
            Holder::Mob(id) => {
                let burnable = self.entities.store.get(id).is_some_and(|entity| {
                    !entity.base.is_removed()
                        && !entity.base.kind.fire_immune()
                        && entity
                            .living()
                            .is_none_or(|state| !living::is_dead_or_dying(&entity.base, state))
                });
                if burnable {
                    self.ignite_entity(id, seconds);
                }
            }
        }
    }
}
