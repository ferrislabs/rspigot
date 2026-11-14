use rspigot_domain::Identifier;
use rspigot_domain::player::PlayerId;
use rspigot_domain::{Rotation, Vec3};

use crate::events::ClientEvent;
use crate::health::{DamageSource, SourceEntity};
use crate::plugins::decisions::{Continuation, MoveStep, Undone};
use crate::plugins::events::{
    PlayerTeleport, PlayerTeleportCause, PlayerTeleportPatch, PlayerVelocity, PlayerVelocityPatch,
};
use crate::plugins::steps::{TeleportStep, VelocityStep};
use crate::plugins::{DamageSourceView, EntityView, EventPayload, Location, Phase};
use crate::ports::ClientLink;
use crate::server::{Audience, Server};
use crate::travel::Destination;
use crate::world_state::DimensionKey;

impl<L: ClientLink> Server<L> {
    pub(crate) fn location_of(&self, position: Vec3, rotation: Rotation) -> Location {
        Location {
            world: self.world.dimension.key.to_string(),
            position,
            rotation,
        }
    }

    pub(crate) fn ask_teleport(
        &mut self,
        id: PlayerId,
        position: Vec3,
        rotation: Option<Rotation>,
        cause: PlayerTeleportCause,
        fallback: Option<MoveStep>,
    ) {
        self.ask_teleport_in(id, None, position, rotation, cause, fallback);
    }

    pub(crate) fn ask_teleport_in(
        &mut self,
        id: PlayerId,
        dimension: Option<DimensionKey>,
        position: Vec3,
        rotation: Option<Rotation>,
        cause: PlayerTeleportCause,
        fallback: Option<MoveStep>,
    ) {
        let Some(player) = self.players.get(&id) else {
            return;
        };
        if cause == PlayerTeleportCause::Plugin && player.vitals.is_dead() {
            return;
        }
        let step = TeleportStep {
            player: id,
            position,
            rotation,
            cause,
            fallback,
            dimension: dimension.clone(),
        };
        self.fire(id, Continuation::Teleport(step), move |server| {
            let player = server.snapshot_of(id)?;
            let current = server.players.get(&id)?.dim.clone();
            let from = Location {
                world: current.to_string(),
                position: player.position,
                rotation: player.rotation,
            };
            let to = Location {
                world: dimension.unwrap_or(current).to_string(),
                position,
                rotation: rotation.unwrap_or(player.rotation),
            };
            Some(EventPayload::PlayerTeleport(PlayerTeleport {
                cause,
                from,
                player,
                to: Some(to),
            }))
        });
    }

    pub(super) fn finish_teleport(
        &mut self,
        step: TeleportStep,
        cancelled: bool,
        patch: PlayerTeleportPatch,
    ) -> Undone {
        let Some(player) = self.players.get_mut(&step.player) else {
            return Vec::new();
        };
        if cancelled {
            if let Some(fallback) = step.fallback {
                self.apply_move(
                    fallback.player,
                    Some(fallback.to),
                    fallback.rotation,
                    fallback.on_ground,
                );
            }
            return Vec::new();
        }
        if step.cause == PlayerTeleportCause::Plugin && player.vitals.is_dead() {
            return Vec::new();
        }
        let (position, rotation) = match &patch.to {
            Some(to) => (to.position, Some(to.rotation)),
            None => (step.position, step.rotation),
        };
        let elsewhere = patch
            .to
            .as_ref()
            .and_then(|to| Identifier::parse(&to.world).ok())
            .or_else(|| step.dimension.clone())
            .filter(|key| *key != player.dim);
        if let Some(dimension) = elsewhere {
            let dropped = self.decisions.forget(step.player);
            self.change_dimension_now(
                step.player,
                Destination {
                    dimension,
                    position,
                    rotation,
                },
            );
            return dropped;
        }
        if let Some(rotation) = rotation {
            player.rotation = rotation;
        }
        let dropped = if step.fallback.is_some() {
            self.decisions.drop_pending(step.player)
        } else {
            self.decisions.forget(step.player)
        };
        self.teleport_player(step.player, position);
        dropped
    }

    pub(crate) fn teleport_now(&mut self, id: PlayerId, position: Vec3) {
        let forgotten = self.decisions.forget(id);
        self.notify_undone(forgotten, Phase::Settled);
        self.teleport_player(id, position);
    }

    pub(crate) fn sync_velocity(&mut self, id: PlayerId) {
        self.push_velocity(id, None);
    }

    pub(crate) fn knock_velocity(&mut self, victim: PlayerId, before: Vec3) {
        self.push_velocity(victim, Some(before));
    }

    fn push_velocity(&mut self, id: PlayerId, restore: Option<Vec3>) {
        let step = VelocityStep {
            player: id,
            restore,
        };
        self.fire(id, Continuation::Velocity(step), move |server| {
            let current = server.players.get(&id)?.vitals.velocity;
            Some(EventPayload::PlayerVelocity(PlayerVelocity {
                player: server.snapshot_of(id)?,
                velocity: restore.unwrap_or(current),
            }))
        });
    }

    pub(super) fn finish_velocity(&mut self, step: VelocityStep, patch: PlayerVelocityPatch) {
        let Some(player) = self.players.get_mut(&step.player) else {
            return;
        };
        if !player.vitals.sync_velocity || step.restore.is_some() && player.vitals.is_dead() {
            return;
        }
        if let Some(velocity) = patch.velocity {
            player.vitals.velocity = velocity;
        }
        let entity = player.entity_id;
        let event = ClientEvent::EntityVelocity {
            entity,
            velocity: player.vitals.velocity,
        };
        player.vitals.sync_velocity = false;
        player.vitals.velocity = step.restore.unwrap_or(Vec3::ZERO);
        self.push(Audience::One(step.player), event.clone());
        if step.restore.is_none() {
            self.push(Audience::Trackers(entity), event);
        }
    }

    pub(crate) fn damage_view(&self, source: &DamageSource) -> DamageSourceView {
        DamageSourceView {
            kind: source.kind.name().to_owned(),
            causing: source
                .causing
                .as_ref()
                .map(|entity| self.source_view(entity)),
            direct: source
                .direct
                .as_ref()
                .map(|entity| self.source_view(entity)),
            location: source.position,
        }
    }

    fn source_view(&self, source: &SourceEntity) -> EntityView {
        let uuid = source.player.map(|player| player.0).or_else(|| {
            self.entities
                .store
                .get(source.entity)
                .map(|entity| entity.base.uuid)
        });
        EntityView {
            id: source.entity,
            uuid: uuid.unwrap_or_default(),
            kind: source.kind.name().to_owned(),
            position: source.position,
        }
    }
}
