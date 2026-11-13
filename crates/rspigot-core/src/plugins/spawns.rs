use rspigot_domain::entity::{Entity, EntityState};
use rspigot_domain::player::PlayerId;
use rspigot_domain::text::Component;
use rspigot_domain::{EntityId, Rotation, Vec3};
use rspigot_entities::EntityKind as Kind;
use uuid::Uuid;

use crate::commands::CommandContext;
use crate::context::SUMMON_DUPLICATE_UUID;
use crate::entities::entity_translation_key;
use crate::plugins::decisions::{Continuation, Lane};
use crate::plugins::entity::refusal;
use crate::plugins::events::{
    CreatureSpawn, CreatureSpawnReason, EntityDropItem, EntityRemove, EntityRemoveCause,
    EntitySpawn, ItemSpawn, ProjectileLaunch,
};
use crate::plugins::steps::{Birth, Follow, SpawnStep, Spawned};
use crate::plugins::{Answer, EntityView, EventKind, EventPayload, ToPlugin};
use crate::ports::ClientLink;
use crate::server::Server;

#[derive(Debug, Clone, Copy)]
struct Newborn {
    id: EntityId,
    uuid: Uuid,
    kind: Kind,
    position: Vec3,
    rotation: Rotation,
}

impl Newborn {
    fn of(entity: &Entity) -> Self {
        let base = &entity.base;
        Self {
            id: base.id,
            uuid: base.uuid,
            kind: base.kind,
            position: base.position,
            rotation: Rotation::new(base.yaw, base.pitch),
        }
    }

    fn view(self) -> EntityView {
        EntityView {
            id: self.id,
            uuid: self.uuid,
            kind: self.kind.name().to_owned(),
            position: self.position,
        }
    }
}

pub(crate) fn spawn_kind(entity: &Entity) -> EventKind {
    match &entity.state {
        EntityState::Living(_) if entity.base.kind != Kind::Player => EventKind::CreatureSpawn,
        EntityState::Item(_) => EventKind::ItemSpawn,
        EntityState::Arrow(_)
        | EntityState::Thrown(_)
        | EntityState::Hurting(_)
        | EntityState::Bullet(_) => EventKind::ProjectileLaunch,
        _ => EventKind::EntitySpawn,
    }
}

pub(crate) fn view_of_entity(entity: &Entity) -> EntityView {
    Newborn::of(entity).view()
}

pub(crate) fn name_of_entity(entity: &Entity) -> Component {
    entity.base.custom_name().cloned().unwrap_or_else(|| {
        Component::translatable(entity_translation_key(entity.base.kind), Vec::new())
    })
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn player_of_entity(&self, id: EntityId) -> Option<PlayerId> {
        self.players
            .values()
            .find(|player| player.entity_id == id)
            .map(|player| player.id)
    }

    pub(crate) fn subject_lane(&self, id: EntityId) -> Lane {
        self.player_of_entity(id)
            .map_or(Lane::Entity(id), Lane::Player)
    }

    pub(crate) fn lane_for(&self, cause: Option<PlayerId>, subject: EntityId) -> Lane {
        if let Some(lane) = self.decisions.current() {
            return lane;
        }
        cause.map_or_else(|| self.subject_lane(subject), Lane::Player)
    }

    pub(crate) fn subject_view(&self, id: EntityId) -> Option<EntityView> {
        if let Some(player) = self.players.values().find(|p| p.entity_id == id) {
            return Some(EntityView {
                id,
                uuid: player.id.0,
                kind: Kind::Player.name().to_owned(),
                position: player.position,
            });
        }
        self.entity_view(id)
    }

    pub(crate) fn add_entity(&mut self, entity: Entity, reason: CreatureSpawnReason) {
        self.spawn_entity(entity, Birth::of(reason));
    }

    pub(crate) fn spawn_entity(&mut self, entity: Entity, birth: Birth) {
        let newborn = Newborn::of(&entity);
        let kind = spawn_kind(&entity);
        let reason = birth.reason;
        let lane = self.lane_for(birth.cause, newborn.id);
        let loaded = self.world.chunks.is_loaded(entity.base.chunk());
        let step = SpawnStep {
            kind,
            entity,
            loaded,
            birth,
        };
        self.fire(lane, Continuation::Spawn(Box::new(step)), move |server| {
            Some(server.spawn_payload(kind, newborn, reason))
        });
    }

    fn spawn_payload(
        &self,
        kind: EventKind,
        newborn: Newborn,
        spawn_reason: CreatureSpawnReason,
    ) -> EventPayload {
        let entity = newborn.view();
        let entity_type = newborn.kind.name().to_owned();
        let location = self.location_of(newborn.position, newborn.rotation);
        match kind {
            EventKind::CreatureSpawn => EventPayload::CreatureSpawn(CreatureSpawn {
                entity,
                entity_type,
                location,
                spawn_reason,
            }),
            EventKind::ItemSpawn => EventPayload::ItemSpawn(ItemSpawn {
                entity,
                entity_type,
                location,
            }),
            EventKind::ProjectileLaunch => EventPayload::ProjectileLaunch(ProjectileLaunch {
                entity,
                entity_type,
                location,
            }),
            _ => EventPayload::EntitySpawn(EntitySpawn {
                entity,
                entity_type,
                location,
            }),
        }
    }

    pub(crate) fn spawning_now(&self, id: EntityId) -> bool {
        self.entities.store.get(id).is_some()
            || self.decisions.anywhere(
                |then| matches!(then, Continuation::Spawn(step) if step.entity.base.id == id),
            )
    }

    pub(super) fn finish_spawn(&mut self, step: SpawnStep, cancelled: bool) {
        let SpawnStep {
            entity,
            loaded,
            birth,
            ..
        } = step;
        let id = entity.base.id;
        let mut placed = None;
        let orphaned = match birth.then {
            Spawned::Converted {
                old,
                follow: Follow::Infection { .. },
                ..
            } => self
                .entities
                .store
                .get(old)
                .is_none_or(|entity| entity.base.is_removed()),
            Spawned::Converted { old, .. } => !self.living_entity_alive(old),
            _ => false,
        };
        if cancelled || orphaned {
            self.entities.mobs.initial_targets.remove(&id);
        } else if loaded && !self.world.chunks.is_loaded(entity.base.chunk()) {
            self.entities.mobs.initial_targets.remove(&id);
            tracing::warn!(
                "Dropping entity {} whose chunk unloaded while it spawned",
                id.0
            );
        } else {
            let name = name_of_entity(&entity);
            if let Some(random) = birth.random {
                self.entities.mobs.spawn_randoms.insert(id, random);
            }
            placed = self.place_entity(entity).ok().map(|id| (id, name));
            self.entities.mobs.spawn_randoms.remove(&id);
            if placed.is_none() {
                self.entities.mobs.initial_targets.remove(&id);
            }
        }
        self.after_spawn(birth.then, placed);
    }

    fn after_spawn(&mut self, then: Spawned, placed: Option<(EntityId, Component)>) {
        match then {
            Spawned::Nothing => {}
            Spawned::Summoned { sender, name } => match placed {
                Some(_) => CommandContext::reply(
                    self,
                    sender,
                    Component::translatable("commands.summon.success", vec![name]),
                ),
                None => CommandContext::reply(
                    self,
                    sender,
                    Component::translatable(SUMMON_DUPLICATE_UUID, Vec::new())
                        .color(rspigot_domain::text::NamedColor::Red),
                ),
            },
            Spawned::Hatched { player, slot, item } => {
                if placed.is_some() {
                    self.use_spawn_egg(player, slot, &item);
                }
            }
            Spawned::Converted { old, event, follow } => {
                self.follow_conversion(old, placed.as_ref().map(|(id, _)| *id), follow);
                self.finish_conversion(old, event, matches!(follow, Follow::Infection { .. }));
            }
            Spawned::Poof => {
                if let Some((id, _)) = placed {
                    self.spawn_anim(id);
                }
            }
            Spawned::Rides { vehicle } => {
                if let Some((rider, _)) = placed {
                    self.start_riding(rider, vehicle, false, false);
                }
            }
            Spawned::Mounted { rider } => {
                if let Some((vehicle, _)) = placed {
                    self.start_riding(rider, vehicle, false, false);
                }
            }
            Spawned::Pattern(blocks) => {
                if placed.is_some() {
                    self.clear_pattern_blocks(&blocks);
                }
            }
            Spawned::Hive(release) => {
                self.finish_hive_release(*release, placed.map(|(id, _)| id));
            }
            Spawned::Answered {
                connection,
                request,
            } => {
                let answer = placed.and_then(|(id, _)| self.entity_view(id)).map_or_else(
                    || Answer::Refused(refusal(SUMMON_DUPLICATE_UUID)),
                    Answer::Spawned,
                );
                self.plugins
                    .send(connection, ToPlugin::Answer { request, answer });
            }
        }
    }

    pub(crate) fn drop_from(&mut self, dropper: EntityView, item: Entity) {
        let lane = self.lane_for(None, dropper.id);
        let drop = Newborn::of(&item);
        self.fire(lane, Continuation::EntityDrop(Box::new(item)), move |_| {
            Some(EventPayload::EntityDropItem(EntityDropItem {
                entity_type: dropper.kind.clone(),
                entity: dropper,
                item_drop: drop.view(),
            }))
        });
    }

    pub(super) fn finish_entity_drop(&mut self, item: Entity, cancelled: bool) {
        if !cancelled {
            self.add_entity(item, CreatureSpawnReason::Default);
        }
    }

    pub(crate) fn notify_removal(&mut self, id: EntityId, cause: EntityRemoveCause) {
        let lane = self.lane_for(None, id);
        self.fire(
            lane,
            Continuation::Notice(EventKind::EntityRemove),
            move |server| {
                let entity = server.entity_view(id)?;
                Some(EventPayload::EntityRemove(EntityRemove {
                    cause,
                    entity_type: entity.kind.clone(),
                    entity,
                }))
            },
        );
    }
}
