use rspigot_ai::{TargetReason, TargetRequest};
use rspigot_domain::EntityId;

use crate::plugins::decisions::Continuation;
use crate::plugins::events::{EntityTargetLivingEntity, EntityTargetReason};
use crate::plugins::steps::{AimStep, Seeker};
use crate::plugins::{Access, EntityView, EventKind, EventPatch, EventPayload};
use crate::ports::ClientLink;
use crate::server::Server;

pub(crate) fn settled_target(patch: Option<EventPatch>) -> Option<Option<EntityId>> {
    match patch {
        Some(EventPatch::EntityTargetLivingEntity(patch)) => {
            patch.target.map(|target| target.map(|target| target.id))
        }
        Some(EventPatch::EntityTarget(patch)) => {
            patch.target.map(|target| target.map(|target| target.id))
        }
        _ => None,
    }
}

const fn reason_of(reason: TargetReason) -> EntityTargetReason {
    match reason {
        TargetReason::ClosestPlayer => EntityTargetReason::ClosestPlayer,
        TargetReason::ClosestEntity => EntityTargetReason::ClosestEntity,
        TargetReason::TargetAttackedEntity => EntityTargetReason::TargetAttackedEntity,
        TargetReason::TargetAttackedNearbyEntity => EntityTargetReason::TargetAttackedNearbyEntity,
        TargetReason::ReinforcementTarget => EntityTargetReason::ReinforcementTarget,
        TargetReason::ForgotTarget => EntityTargetReason::ForgotTarget,
        TargetReason::Tempt => EntityTargetReason::Tempt,
        TargetReason::TargetAttackedOwner => EntityTargetReason::TargetAttackedOwner,
        TargetReason::OwnerAttackedTarget => EntityTargetReason::OwnerAttackedTarget,
        TargetReason::RandomTarget => EntityTargetReason::RandomTarget,
        TargetReason::DefendVillage => EntityTargetReason::DefendVillage,
        TargetReason::Collision => EntityTargetReason::Collision,
        TargetReason::PigZombieTarget => EntityTargetReason::PigZombieTarget,
        TargetReason::FollowLeader => EntityTargetReason::FollowLeader,
        TargetReason::Unknown => EntityTargetReason::Unknown,
    }
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn defers_targets(&self) -> bool {
        self.plugins
            .listens(EventKind::EntityTargetLivingEntity, Access::Decide)
    }

    pub(crate) fn target_event(
        &mut self,
        mob: EntityView,
        request: TargetRequest,
        seeker: Seeker,
        deferred: bool,
    ) {
        let reason = match (request.reason, request.to, request.from) {
            (TargetReason::Unknown, None, Some(from)) if self.living_entity_alive(from) => {
                EntityTargetReason::ForgotTarget
            }
            (TargetReason::Unknown, None, Some(_)) => EntityTargetReason::TargetDied,
            (reason, ..) => reason_of(reason),
        };
        let lane = self.lane_for(None, mob.id);
        let step = AimStep {
            mob: mob.id,
            from: request.from,
            to: request.to,
            seeker,
            deferred,
        };
        let to = request.to;
        self.fire(lane, Continuation::Aim(step), move |server| {
            let target = match to {
                Some(target) => Some(server.subject_view(target)?),
                None => None,
            };
            Some(EventPayload::EntityTargetLivingEntity(
                EntityTargetLivingEntity {
                    entity_type: mob.kind.clone(),
                    entity: mob,
                    reason,
                    target,
                },
            ))
        });
    }

    pub(crate) fn mob_target_requests(
        &mut self,
        id: EntityId,
        requests: Vec<TargetRequest>,
        deferred: bool,
    ) {
        if requests.is_empty() {
            return;
        }
        let Some(mob) = self.entity_view(id) else {
            return;
        };
        for request in requests {
            let seeker = if request.tempt {
                Seeker::Tempt
            } else {
                Seeker::Mob
            };
            self.target_event(mob.clone(), request, seeker, deferred);
        }
    }

    pub(crate) fn request_target(&mut self, mob: EntityView, to: EntityId, reason: TargetReason) {
        let deferred = self.defers_targets();
        let from = self.mob_target(mob.id);
        if from == Some(to) {
            return;
        }
        if !deferred {
            self.point_mob(mob.id, Some(to));
        }
        let request = TargetRequest {
            from,
            to: Some(to),
            reason,
            tempt: false,
        };
        self.target_event(mob, request, Seeker::Mob, deferred);
    }

    pub(crate) fn forget_target(&mut self, mob: EntityId, reason: TargetReason) {
        let Some(view) = self.entity_view(mob) else {
            return;
        };
        let Some(from) = self.mob_target(mob) else {
            return;
        };
        let deferred = self.defers_targets();
        if !deferred {
            self.point_mob(mob, None);
        }
        let request = TargetRequest {
            from: Some(from),
            to: None,
            reason,
            tempt: false,
        };
        self.target_event(view, request, Seeker::Mob, deferred);
    }

    fn mob_known(&self, mob: EntityId) -> bool {
        self.entities.store.get(mob).is_some()
            || self.decisions.anywhere(
                |then| matches!(then, Continuation::Spawn(step) if step.entity.base.id == mob),
            )
    }

    fn point_mob(&mut self, mob: EntityId, target: Option<EntityId>) {
        match self.entities.mobs.entries.get_mut(&mob) {
            Some(entry) => {
                entry.ai.set_target(target);
                if target.is_none() {
                    self.kind_target_cleared(mob);
                }
            }
            None => match target {
                Some(target) => {
                    self.entities.mobs.initial_targets.insert(mob, target);
                }
                None => {
                    self.entities.mobs.initial_targets.remove(&mob);
                }
            },
        }
    }

    pub(super) fn finish_aim(
        &mut self,
        step: AimStep,
        cancelled: bool,
        settled: Option<Option<EntityId>>,
    ) {
        let AimStep {
            mob,
            from,
            to,
            seeker,
            deferred,
        } = step;
        if !deferred {
            return;
        }
        let target = if cancelled {
            None
        } else {
            settled.unwrap_or(to)
        };
        match seeker {
            Seeker::Tempt => {
                if let Some(entry) = self.entities.mobs.entries.get_mut(&mob) {
                    entry.ai.settle_tempt(target);
                }
            }
            Seeker::Mob => {
                if cancelled {
                    return;
                }
                let current = match self.entities.mobs.entries.get(&mob) {
                    Some(entry) => entry.ai.target(),
                    None if self.mob_known(mob) => {
                        self.entities.mobs.initial_targets.get(&mob).copied()
                    }
                    None => return,
                };
                if current == from {
                    self.point_mob(mob, target);
                }
            }
            Seeker::Orb => {
                if !cancelled {
                    self.settle_orb(mob, from, target);
                }
            }
        }
    }
}
