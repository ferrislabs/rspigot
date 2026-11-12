use rspigot_domain::EntityId;
use rspigot_domain::entity::Pose as Stance;

use crate::plugins::decisions::Continuation;
use crate::plugins::events::{EntityPoseChange, Pose};
use crate::plugins::{EntityView, EventKind, EventPayload};
use crate::ports::ClientLink;
use crate::server::Server;

const fn pose_of(stance: Stance) -> Pose {
    match stance {
        Stance::Standing => Pose::Standing,
        Stance::FallFlying => Pose::FallFlying,
        Stance::Sleeping => Pose::Sleeping,
        Stance::Swimming => Pose::Swimming,
        Stance::SpinAttack => Pose::SpinAttack,
        Stance::Crouching => Pose::Sneaking,
        Stance::LongJumping => Pose::LongJumping,
        Stance::Dying => Pose::Dying,
        Stance::Croaking => Pose::Croaking,
        Stance::UsingTongue => Pose::UsingTongue,
        Stance::Sitting => Pose::Sitting,
        Stance::Roaring => Pose::Roaring,
        Stance::Sniffing => Pose::Sniffing,
        Stance::Emerging => Pose::Emerging,
        Stance::Digging => Pose::Digging,
        Stance::Sliding => Pose::Sliding,
        Stance::Shooting => Pose::Shooting,
        Stance::Inhaling => Pose::Inhaling,
    }
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn notify_poses(&mut self) {
        if !self.listened(EventKind::EntityPoseChange) {
            self.entities.poses.clear();
            return;
        }
        let mut current: Vec<(EntityId, Stance)> = self
            .entities
            .store
            .iter()
            .filter(|entity| entity.living().is_some())
            .map(|entity| (entity.base.id, entity.base.pose()))
            .collect();
        current.extend(self.players.values().map(|player| {
            let stance = if player.sneaking && !player.flying {
                Stance::Crouching
            } else {
                Stance::Standing
            };
            (player.entity_id, stance)
        }));
        let changed: Vec<(EntityId, Stance)> = current
            .iter()
            .filter(|(id, stance)| {
                self.entities
                    .poses
                    .get(id)
                    .is_some_and(|last| last != stance)
            })
            .copied()
            .collect();
        self.entities.poses = current.into_iter().collect();
        for (id, stance) in changed {
            let Some(entity) = self.subject_view(id) else {
                continue;
            };
            let lane = self.lane_for(None, id);
            self.fire(
                lane,
                Continuation::Notice(EventKind::EntityPoseChange),
                move |_: &Self| Some(pose_payload(entity, stance)),
            );
        }
    }
}

fn pose_payload(entity: EntityView, stance: Stance) -> EventPayload {
    EventPayload::EntityPoseChange(EntityPoseChange {
        entity_type: entity.kind.clone(),
        entity,
        pose: pose_of(stance),
    })
}
