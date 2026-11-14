use rspigot_domain::entity::RemovalReason;
use rspigot_domain::player::PlayerId;
use rspigot_domain::{EntityId, Identifier, Rotation, Vec3};
use rspigot_entities::EntityKind as Kind;
use rspigot_nbt::Tag;

use crate::end_platform;
use crate::events::{ClientEvent, GameEvent};
use crate::plugins::events::{
    EntityPortal, EntityPortalPatch, PlayerChangedWorld, PlayerPortal, PlayerPortalPatch,
    PlayerTeleportCause,
};
use crate::plugins::steps::{EntityPortalStep, PortalStep};
use crate::plugins::{Continuation, EventKind, EventPayload, Location, Phase};
use crate::ports::ClientLink;
use crate::server::Server;
use crate::streaming::ChunkStreamer;
use crate::world_state::DimensionKey;

const KEEP_ATTRIBUTES_AND_METADATA: u8 = 3;
const PORTAL_CREATION_RADIUS: i32 = 16;
const PORTAL_SEARCH_RADIUS: i32 = 128;

pub type DimensionProvider<L> = fn(&mut Server<L>, &Identifier);

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Destination {
    pub(crate) dimension: DimensionKey,
    pub(crate) position: Vec3,
    pub(crate) rotation: Option<Rotation>,
    pub(crate) platform: bool,
}

fn end_key() -> DimensionKey {
    Identifier::minecraft("the_end")
}

impl Destination {
    pub(crate) fn end_spawn() -> Self {
        let spawn = end_platform::END_SPAWN_POINT;
        Self {
            dimension: end_key(),
            position: Vec3::new(
                f64::from(spawn.x) + 0.5,
                f64::from(spawn.y),
                f64::from(spawn.z) + 0.5,
            ),
            rotation: Some(Rotation::new(end_platform::END_SPAWN_YAW, 0.0)),
            platform: true,
        }
    }

    fn from_location(location: &Location, fallback: &DimensionKey) -> Self {
        Self {
            dimension: Identifier::parse(&location.world).unwrap_or_else(|_| fallback.clone()),
            position: location.position,
            rotation: Some(location.rotation),
            platform: false,
        }
    }
}

impl<L: ClientLink> Server<L> {
    #[must_use]
    pub fn with_dimension_provider(mut self, provider: DimensionProvider<L>) -> Self {
        self.dimension_provider = Some(provider);
        self
    }

    pub fn player_dimension(&self, player: PlayerId) -> Option<Identifier> {
        self.players.get(&player).map(|p| p.dim.clone())
    }

    pub fn change_dimension(
        &mut self,
        player: PlayerId,
        dimension: Identifier,
        position: Vec3,
        rotation: Option<Rotation>,
        cause: PlayerTeleportCause,
    ) {
        let destination = if dimension == end_key() && cause == PlayerTeleportCause::EndPortal {
            Destination::end_spawn()
        } else {
            Destination {
                dimension,
                position,
                rotation,
                platform: false,
            }
        };
        self.ask_portal(player, destination, cause);
    }

    pub fn teleport_to_dimension(
        &mut self,
        player: PlayerId,
        dimension: Identifier,
        position: Vec3,
        rotation: Option<Rotation>,
    ) {
        self.ask_teleport_in(
            player,
            Some(dimension),
            position,
            rotation,
            PlayerTeleportCause::Plugin,
            None,
        );
    }

    pub fn change_entity_dimension(
        &mut self,
        entity: EntityId,
        dimension: Identifier,
        position: Vec3,
        rotation: Option<Rotation>,
    ) {
        let source = self.dimension_of_entity(entity);
        let destination = Destination {
            dimension,
            position,
            rotation,
            platform: false,
        };
        if let Some(source) = source {
            self.in_dimension(&source, |server| {
                server.ask_entity_portal(entity, destination);
            });
        }
    }

    fn dimension_of_entity(&self, entity: EntityId) -> Option<DimensionKey> {
        if self.entities.store.get(entity).is_some() {
            return Some(self.world.dimension.key.clone());
        }
        let mut found = None;
        for (key, state) in &self.dormant {
            if state.entities.store.get(entity).is_some() {
                found = Some(key.clone());
            }
        }
        found
    }

    pub(crate) fn ensure_dimension(&mut self, key: &DimensionKey) -> bool {
        if self.world.dimension.key == *key || self.dormant.contains_key(key) {
            return true;
        }
        if let Some(provider) = self.dimension_provider {
            provider(self, key);
        }
        self.dormant.contains_key(key)
    }

    pub(crate) fn ask_portal(
        &mut self,
        id: PlayerId,
        destination: Destination,
        cause: PlayerTeleportCause,
    ) {
        if !self.players.contains_key(&id) {
            return;
        }
        let step = PortalStep {
            player: id,
            destination: destination.clone(),
        };
        self.fire(id, Continuation::Portal(step), move |server| {
            let player = server.snapshot_of(id)?;
            let current = server.players.get(&id)?.dim.clone();
            let from = Location {
                world: current.to_string(),
                position: player.position,
                rotation: player.rotation,
            };
            let to = Location {
                world: destination.dimension.to_string(),
                position: destination.position,
                rotation: destination.rotation.unwrap_or(player.rotation),
            };
            Some(EventPayload::PlayerPortal(PlayerPortal {
                can_create_portal: false,
                cause,
                creation_radius: PORTAL_CREATION_RADIUS,
                from,
                player,
                search_radius: PORTAL_SEARCH_RADIUS,
                to: Some(to),
            }))
        });
    }

    pub(crate) fn finish_portal(
        &mut self,
        step: PortalStep,
        cancelled: bool,
        patch: PlayerPortalPatch,
    ) -> Vec<(u64, EventPayload)> {
        if cancelled || !self.players.contains_key(&step.player) {
            return Vec::new();
        }
        let destination = match &patch.to {
            Some(to) => Destination::from_location(to, &step.destination.dimension),
            None => step.destination,
        };
        let dropped = self.decisions.forget(step.player);
        self.change_dimension_now(step.player, destination);
        dropped
    }

    pub(crate) fn change_dimension_now(&mut self, id: PlayerId, destination: Destination) -> bool {
        let Some(source) = self.players.get(&id).map(|player| player.dim.clone()) else {
            return false;
        };
        if source == destination.dimension {
            self.in_dimension(&source, |server| {
                if let Some(rotation) = destination.rotation
                    && let Some(player) = server.players.get_mut(&id)
                {
                    player.rotation = rotation;
                }
                server.teleport_now(id, destination.position);
            });
            return true;
        }
        if !self.ensure_dimension(&destination.dimension) {
            tracing::warn!(dimension = %destination.dimension, "no such dimension to travel to");
            return false;
        }
        self.in_dimension(&source, |server| server.leave_dimension(id));
        let target = destination.dimension.clone();
        if destination.platform {
            self.in_dimension(&target, |server| server.make_end_platform());
        }
        self.in_dimension(&target, |server| server.enter_dimension(id, destination));
        true
    }

    fn leave_dimension(&mut self, id: PlayerId) {
        self.close_open_menu(id, false);
        if let Some(entity) = self.players.get(&id).map(|player| player.entity_id) {
            self.unride(entity);
        }
        let forgotten = self.decisions.forget(id);
        self.notify_undone(forgotten, Phase::Settled);
        let Some(player) = self.players.get_mut(&id) else {
            return;
        };
        player.streamer.release_all(&mut self.world.chunks);
        player.tracked.clear();
        let entity = player.entity_id;
        for other in self.players.values_mut() {
            if other.id != id && other.tracked.remove(&entity) {
                other.link.send(ClientEvent::RemoveEntities(vec![entity]));
            }
        }
    }

    fn enter_dimension(&mut self, id: PlayerId, destination: Destination) {
        let Destination {
            dimension,
            position,
            rotation,
            platform: _,
        } = destination;
        let Some(player) = self.players.get_mut(&id) else {
            return;
        };
        let from = std::mem::replace(&mut player.dim, dimension).to_string();
        player.position = position;
        if let Some(rotation) = rotation {
            player.rotation = rotation;
        }
        player.loaded = false;
        let view = u8::try_from(player.streamer.view_distance()).unwrap_or(u8::MAX);
        player.streamer = ChunkStreamer::new(self.config.stream, position.chunk(), view);
        let Some(player) = self.players.get(&id) else {
            return;
        };
        let mut spawn = self.spawn_info(player);
        spawn.death_location = player.vitals.death_location.map(|pos| {
            (
                player
                    .vitals
                    .death_dimension
                    .clone()
                    .unwrap_or_else(|| self.world.dimension.key.clone()),
                pos,
            )
        });
        let time = self.time_event();
        let difficulty = self.world.difficulty;
        let raining = self.world.level.raining;
        let thundering = self.world.level.thundering;
        let spawn_point = self.world.spawn.block();
        let angle = self.world.level.spawn_angle;
        let key = self.world.dimension.key.clone();
        let Some(player) = self.players.get_mut(&id) else {
            return;
        };
        let entity = player.entity_id;
        player.link.send(ClientEvent::Respawn {
            spawn,
            keep_data: KEEP_ATTRIBUTES_AND_METADATA,
        });
        player.link.send(ClientEvent::Difficulty {
            difficulty,
            locked: false,
        });
        player.link.send(ClientEvent::OperatorLevel {
            entity,
            level: if player.operator { 4 } else { 0 },
        });
        let teleport = player.next_teleport_id();
        player.link.send(ClientEvent::Teleport {
            id: teleport,
            position,
            rotation: player.rotation,
        });
        player.link.send(ClientEvent::Abilities(player.abilities()));
        player.link.send(ClientEvent::SpawnPosition {
            dimension: key,
            pos: spawn_point,
            yaw: angle,
            pitch: 0.0,
        });
        player.link.send(time);
        if raining {
            player
                .link
                .send(ClientEvent::GameEvent(GameEvent::StartRaining));
            player
                .link
                .send(ClientEvent::GameEvent(GameEvent::RainLevelChange(1.0)));
            player
                .link
                .send(ClientEvent::GameEvent(GameEvent::ThunderLevelChange(
                    if thundering { 1.0 } else { 0.0 },
                )));
        }
        player
            .link
            .send(ClientEvent::GameEvent(GameEvent::LevelChunksLoadStart));
        player
            .link
            .send(ClientEvent::HeldSlot(player.inventory.selected_hotbar));
        let state_id = player.next_inventory_state();
        player.link.send(ClientEvent::InventoryContent {
            state_id,
            slots: player.inventory.slots().to_vec(),
            carried: None,
        });
        player.menu = rspigot_domain::menu::InventoryMenu::synced(&player.inventory);
        player.link.send(ClientEvent::Health {
            health: player.vitals.health,
            food: player.vitals.food.food,
            saturation: player.vitals.food.saturation,
        });
        let experience = player.vitals.experience;
        player.link.send(ClientEvent::Experience {
            progress: experience.progress,
            level: experience.level,
            total: experience.total,
        });
        player.vitals.mark_sent();
        for effect in player.effects.iter() {
            player.link.send(ClientEvent::EffectApplied {
                entity,
                effect: *effect,
                blend: false,
            });
        }
        player
            .streamer
            .start(&mut self.world.chunks, &self.workers, &player.link);
        self.update_player_view(id);
        self.send_commands(id);
        self.fire(
            id,
            Continuation::Notice(EventKind::PlayerChangedWorld),
            move |server| {
                Some(EventPayload::PlayerChangedWorld(PlayerChangedWorld {
                    from,
                    player: server.snapshot_of(id)?,
                }))
            },
        );
    }

    fn may_travel(&self, id: EntityId) -> bool {
        self.entities.store.get(id).is_some_and(|entity| {
            entity.base.kind != Kind::EnderDragon && !self.entities.dragons.contains_key(&id)
        })
    }

    pub(crate) fn ask_entity_portal(&mut self, id: EntityId, destination: Destination) {
        if !self.may_travel(id) {
            return;
        }
        let lane = self.lane_for(None, id);
        let step = EntityPortalStep {
            entity: id,
            destination: destination.clone(),
        };
        self.fire(lane, Continuation::EntityPortal(step), move |server| {
            let view = server.entity_view(id)?;
            let base = &server.entities.store.get(id)?.base;
            let rotation = Rotation::new(base.yaw, base.pitch);
            let from = server.location_of(base.position, rotation);
            let to = Location {
                world: destination.dimension.to_string(),
                position: destination.position,
                rotation: destination.rotation.unwrap_or(rotation),
            };
            Some(EventPayload::EntityPortal(EntityPortal {
                can_create_portal: false,
                creation_radius: PORTAL_CREATION_RADIUS,
                entity_type: view.kind.clone(),
                entity: view,
                from,
                search_radius: PORTAL_SEARCH_RADIUS,
                to: Some(to),
            }))
        });
    }

    pub(crate) fn finish_entity_portal(
        &mut self,
        step: EntityPortalStep,
        cancelled: bool,
        patch: EntityPortalPatch,
    ) {
        if cancelled {
            return;
        }
        let destination = match &patch.to {
            None => step.destination,
            Some(None) => return,
            Some(Some(to)) => Destination::from_location(to, &step.destination.dimension),
        };
        self.change_entity_dimension_now(step.entity, destination);
    }

    pub(crate) fn change_entity_dimension_now(
        &mut self,
        id: EntityId,
        destination: Destination,
    ) -> Option<EntityId> {
        if !self.may_travel(id) || destination.dimension == self.world.dimension.key {
            return None;
        }
        if !self.ensure_dimension(&destination.dimension) {
            tracing::warn!(dimension = %destination.dimension, "no such dimension to travel to");
            return None;
        }
        self.unride(id);
        let world = self.world.level.uuid;
        let mut nbt = {
            let entity = self.entities.store.get(id)?;
            self.entity_nbt(entity, world)?
        };
        let rotation = destination.rotation.unwrap_or_else(|| {
            self.entities
                .store
                .get(id)
                .map_or(Rotation::default(), |e| {
                    Rotation::new(e.base.yaw, e.base.pitch)
                })
        });
        let at = destination.position;
        nbt.insert(
            "Pos",
            Tag::List(vec![
                Tag::Double(at.x),
                Tag::Double(at.y),
                Tag::Double(at.z),
            ]),
        );
        nbt.insert(
            "Rotation",
            Tag::List(vec![Tag::Float(rotation.yaw), Tag::Float(rotation.pitch)]),
        );
        self.remove_entity(id, RemovalReason::ChangedDimension, None)?;
        let target = destination.dimension;
        if destination.platform {
            self.in_dimension(&target, |server| server.make_end_platform());
        }
        self.in_dimension(&target, |server| server.arrive_entity(&nbt))
    }

    fn arrive_entity(&mut self, nbt: &rspigot_nbt::Compound) -> Option<EntityId> {
        let id = match nbt.get_str("id").and_then(Kind::from_name) {
            Some(kind) => self.allocate_entity_id_for(kind),
            None => self.allocate_entity_id(),
        };
        let uuid = self.random_uuid();
        match rspigot_domain::entity::save::load_entity(nbt, id, uuid) {
            Ok(entity) => {
                if self.entities.store.by_uuid(entity.base.uuid).is_some() {
                    return None;
                }
                self.place_entity(entity).ok()
            }
            Err(error) => {
                tracing::warn!(%error, "entity failed to arrive in the new dimension");
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use rspigot_domain::item::ItemStack;
    use rspigot_domain::player::{ClientSettings, GameProfile};
    use rspigot_domain::text::Component;
    use rspigot_domain::world::Dimension;
    use rspigot_items::ItemKind;

    use super::*;
    use crate::config::{GameConfig, LevelData};
    use crate::events::Inbound;
    use crate::mob_actions::new_mob;
    use crate::status::StatusBoard;
    use crate::world_state::tests::Quiet;

    #[derive(Clone, Default)]
    struct Tap(Arc<Mutex<Vec<ClientEvent>>>);

    impl Tap {
        fn take(&self) -> Vec<ClientEvent> {
            self.0
                .lock()
                .map(|mut events| std::mem::take(&mut *events))
                .unwrap_or_default()
        }
    }

    impl ClientLink for Tap {
        fn send(&self, event: ClientEvent) {
            if let Ok(mut events) = self.0.lock() {
                events.push(event);
            }
        }

        fn disconnect(&self, _: Component) {}

        fn is_connected(&self) -> bool {
            true
        }
    }

    fn nether() -> Identifier {
        Identifier::minecraft("the_nether")
    }

    fn provide(server: &mut Server<Tap>, key: &Identifier) {
        if *key == nether() {
            server.load_dimension(
                key.clone(),
                Dimension::nether(),
                Arc::new(Quiet),
                Arc::new(Quiet),
            );
        }
    }

    fn server() -> Server<Tap> {
        let config = GameConfig {
            chunk_workers: 1,
            spawn_chunk_radius: 1,
            ..GameConfig::default()
        };
        let mut server = Server::new(
            config,
            LevelData::default(),
            Arc::new(Quiet),
            Arc::new(Quiet),
            StatusBoard::new(1),
        )
        .with_rng_seed(5)
        .with_dimension_provider(provide);
        server.wait_for_spawn(Duration::from_secs(10));
        server
    }

    fn join(server: &mut Server<Tap>, name: &str) -> (PlayerId, Tap) {
        let link = Tap::default();
        let id = PlayerId::offline(name);
        server.handle(Inbound::Join {
            profile: GameProfile {
                id,
                name: name.to_owned(),
                properties: Vec::new(),
            },
            settings: ClientSettings::default(),
            link: link.clone(),
            address: [127, 0, 0, 1].into(),
            session: 1,
        });
        server.deliver();
        link.take();
        (id, link)
    }

    fn at() -> Vec3 {
        Vec3::new(8.5, 70.0, 8.5)
    }

    #[test]
    fn a_player_receives_the_respawn_then_the_chunks_of_the_target() {
        let mut server = server();
        let (alice, link) = join(&mut server, "Alice");
        let (bob, watcher) = join(&mut server, "Bob");
        for _ in 0..20 {
            server.tick();
            std::thread::sleep(Duration::from_millis(5));
        }
        let entity = server.players[&alice].entity_id;
        link.take();
        watcher.take();
        assert!(server.change_dimension_now(
            alice,
            Destination {
                dimension: nether(),
                position: at(),
                rotation: None,
                platform: false,
            }
        ));
        server.deliver();
        assert_eq!(server.players[&alice].dim, nether());
        let mut events = link.take();
        for _ in 0..100 {
            server.tick();
            std::thread::sleep(Duration::from_millis(5));
            events.extend(link.take());
            if events
                .iter()
                .any(|e| matches!(e, ClientEvent::Chunk { .. }))
            {
                break;
            }
        }
        let respawn = events
            .iter()
            .position(|e| matches!(e, ClientEvent::Respawn { .. }))
            .expect("respawn sent");
        let chunk = events
            .iter()
            .position(|e| matches!(e, ClientEvent::Chunk { .. }))
            .expect("chunk sent");
        assert!(respawn < chunk);
        let ClientEvent::Respawn { spawn, keep_data } = &events[respawn] else {
            unreachable!()
        };
        assert_eq!(spawn.dimension.key, nether());
        assert_eq!(*keep_data, 3);
        assert!(
            events.iter().any(
                |e| matches!(e, ClientEvent::Chunk { dimension, .. } if *dimension == nether())
            )
        );
        let position = events
            .iter()
            .position(|e| matches!(e, ClientEvent::Teleport { .. }))
            .expect("position sent");
        assert!(respawn < position);
        assert!(!server.players[&bob].tracked.contains(&entity));
        assert!(
            watcher
                .take()
                .iter()
                .any(|e| matches!(e, ClientEvent::RemoveEntities(ids) if ids.contains(&entity)))
        );
    }

    #[test]
    fn a_traveller_keeps_inventory_and_experience() {
        let mut server = server();
        let (alice, _) = join(&mut server, "Alice");
        if let Some(player) = server.players.get_mut(&alice) {
            player.inventory.set(
                0,
                Some(ItemStack::new(
                    ItemKind::Stick.id(),
                    std::num::NonZeroU8::MIN,
                )),
            );
            player.vitals.experience.give_points(30, 0);
        }
        let before = server.players[&alice].inventory.slots().to_vec();
        let total = server.players[&alice].vitals.experience.total;
        assert!(total > 0);
        server.change_dimension_now(
            alice,
            Destination {
                dimension: nether(),
                position: at(),
                rotation: None,
                platform: false,
            },
        );
        let player = &server.players[&alice];
        assert_eq!(player.inventory.slots(), before.as_slice());
        assert_eq!(player.vitals.experience.total, total);
        assert_eq!(player.position, at());
    }

    #[test]
    fn an_unknown_dimension_leaves_the_player_where_it_is() {
        let mut server = server();
        let (alice, _) = join(&mut server, "Alice");
        let home = server.players[&alice].dim.clone();
        assert!(!server.change_dimension_now(
            alice,
            Destination {
                dimension: Identifier::minecraft("nowhere"),
                position: at(),
                rotation: None,
                platform: false,
            },
        ));
        assert_eq!(server.players[&alice].dim, home);
    }

    #[test]
    fn an_entity_keeps_its_data_and_gets_a_new_id() {
        let mut server = server();
        let mut base = server.new_entity_base(Kind::Pig);
        base.position = Vec3::new(0.5, 64.0, 0.5);
        base.set_custom_name(Some(Component::text("Wilbur")));
        let uuid = base.uuid;
        let old = server.place_entity(new_mob(base)).expect("placed");
        let new = server
            .change_entity_dimension_now(
                old,
                Destination {
                    dimension: nether(),
                    position: at(),
                    rotation: None,
                    platform: false,
                },
            )
            .expect("travelled");
        assert_ne!(new, old);
        assert!(server.entities.store.get(old).is_none());
        let arrived = server.dormant[&nether()]
            .entities
            .store
            .get(new)
            .expect("arrived");
        assert_eq!(arrived.base.uuid, uuid);
        assert_eq!(arrived.base.position, at());
        assert_eq!(
            arrived.base.custom_name().cloned(),
            Some(Component::text("Wilbur"))
        );
    }
}
