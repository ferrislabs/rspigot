use std::collections::BTreeMap;

use rspigot_blocks::BlockKind;
use rspigot_domain::chunk::BlockEntity;
use rspigot_domain::entity::{Entity, EntityState};
use rspigot_domain::game_event::{
    GameEvent, GameEventTag, SourceFacts, Validity, VibrationData, VibrationInfo, is_occluded,
    is_postable, validate,
};
use rspigot_domain::player::GameMode;
use rspigot_domain::{BlockPos, BlockStateId, ChunkPos, EntityId, Identifier, Vec3};
use rspigot_entities::EntityKind;
use rspigot_nbt::Compound;
use uuid::Uuid;

use crate::ports::ClientLink;
use crate::server::Server;

pub(crate) const SENSOR: &str = "sculk_sensor";
pub(crate) const CALIBRATED_SENSOR: &str = "calibrated_sculk_sensor";
pub(crate) const SHRIEKER: &str = "sculk_shrieker";
const LISTENER: &str = "listener";
const SENSOR_RADIUS: i32 = 8;
const CALIBRATED_RADIUS: i32 = 16;
const SHRIEKER_RADIUS: i32 = 8;
const WARDEN_RADIUS: i32 = 16;

#[derive(Debug, Default)]
pub(crate) struct GameEvents {
    pub(crate) due: BTreeMap<u64, Vec<(BlockPos, TimerKind)>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TimerKind {
    Sensor,
    Shrieker,
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct GameEventContext {
    pub(crate) entity: Option<EntityId>,
    pub(crate) state: Option<BlockStateId>,
}

impl GameEventContext {
    pub(crate) fn of(entity: Option<EntityId>) -> Self {
        Self {
            entity,
            state: None,
        }
    }

    pub(crate) fn block(state: BlockStateId) -> Self {
        Self {
            entity: None,
            state: Some(state),
        }
    }

    pub(crate) fn entity_in(entity: Option<EntityId>, state: BlockStateId) -> Self {
        Self {
            entity,
            state: Some(state),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Listener {
    Sensor { pos: BlockPos, calibrated: bool },
    Shrieker(BlockPos),
    Warden(EntityId),
}

pub(crate) fn listener_kind(kind: BlockKind) -> Option<&'static str> {
    match kind {
        BlockKind::SculkSensor => Some(SENSOR),
        BlockKind::CalibratedSculkSensor => Some(CALIBRATED_SENSOR),
        BlockKind::SculkShrieker => Some(SHRIEKER),
        _ => None,
    }
}

fn center(pos: BlockPos) -> Vec3 {
    Vec3::new(
        f64::from(pos.x) + 0.5,
        f64::from(pos.y) + 0.5,
        f64::from(pos.z) + 0.5,
    )
}

fn section_range(origin: i32, radius: i32) -> (i32, i32) {
    ((origin - radius) >> 4, (origin + radius) >> 4)
}

fn owner_uuid(entity: &Entity) -> Option<Uuid> {
    match &entity.state {
        EntityState::Arrow(state) => state.owner,
        EntityState::Thrown(state) => state.owner,
        EntityState::Hurting(state) => state.owner,
        EntityState::Bullet(state) => state.owner,
        _ => None,
    }
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn game_event(&mut self, event: GameEvent, at: Vec3, context: GameEventContext) {
        let radius = event.notification_radius();
        let origin = at.block();
        let (x0, x1) = section_range(origin.x, radius);
        let (y0, y1) = section_range(origin.y, radius);
        let (z0, z1) = section_range(origin.z, radius);
        let mut found = Vec::new();
        for cx in x0..=x1 {
            for cz in z0..=z1 {
                let pos = ChunkPos::new(cx, cz);
                if let Some(chunk) = self.world.chunks.get(pos) {
                    for entity in &chunk.block_entities {
                        let Some(listener) = block_listener(entity) else {
                            continue;
                        };
                        let section = entity.pos.y >> 4;
                        if (y0..=y1).contains(&section) {
                            found.push(listener);
                        }
                    }
                }
                for id in self.entities.store.in_chunk(pos) {
                    let Some(entity) = self.entities.store.get(*id) else {
                        continue;
                    };
                    if entity.base.kind != EntityKind::Warden {
                        continue;
                    }
                    let section = self.warden_listener_section(*id);
                    if section.is_some_and(|s| {
                        (x0..=x1).contains(&s.x)
                            && (y0..=y1).contains(&s.y)
                            && (z0..=z1).contains(&s.z)
                    }) {
                        found.push(Listener::Warden(*id));
                    }
                }
            }
        }
        for listener in found {
            self.handle_game_event(listener, event, at, context);
        }
    }

    pub(crate) fn block_game_event(
        &mut self,
        event: GameEvent,
        pos: BlockPos,
        context: GameEventContext,
    ) {
        self.game_event(event, center(pos), context);
    }

    pub(crate) fn listener_radius(listener: Listener) -> i32 {
        match listener {
            Listener::Sensor { calibrated, .. } => {
                if calibrated {
                    CALIBRATED_RADIUS
                } else {
                    SENSOR_RADIUS
                }
            }
            Listener::Shrieker(_) => SHRIEKER_RADIUS,
            Listener::Warden(_) => WARDEN_RADIUS,
        }
    }

    fn listener_tag(listener: Listener) -> GameEventTag {
        match listener {
            Listener::Sensor { .. } => GameEventTag::Vibrations,
            Listener::Shrieker(_) => GameEventTag::ShriekerCanListen,
            Listener::Warden(_) => GameEventTag::WardenCanListen,
        }
    }

    pub(crate) fn listener_position(&self, listener: Listener) -> Option<Vec3> {
        match listener {
            Listener::Sensor { pos, .. } | Listener::Shrieker(pos) => Some(center(pos)),
            Listener::Warden(id) => {
                let base = &self.entities.store.get(id)?.base;
                Some(Vec3::new(
                    base.position.x,
                    base.position.y + f64::from(base.eye_height()),
                    base.position.z,
                ))
            }
        }
    }

    pub(crate) fn listener_data(&self, listener: Listener) -> VibrationData {
        match listener {
            Listener::Sensor { pos, .. } | Listener::Shrieker(pos) => {
                let nbt = self
                    .world
                    .chunks
                    .get(pos.chunk())
                    .and_then(|chunk| chunk.block_entity(pos))
                    .and_then(|entity| entity.data.get_compound(LISTENER));
                VibrationData::read_or_default(nbt)
            }
            Listener::Warden(id) => self
                .entities
                .mobs
                .entries
                .get(&id)
                .map(|entry| entry.ai.warden_state().vibration.clone())
                .unwrap_or_default(),
        }
    }

    pub(crate) fn store_listener_data(&mut self, listener: Listener, data: VibrationData) {
        match listener {
            Listener::Sensor { pos, .. } | Listener::Shrieker(pos) => {
                let Some(chunk) = self.world.chunks.chunk_mut(pos.chunk()) else {
                    return;
                };
                let Some(existing) = chunk.block_entity(pos).cloned() else {
                    return;
                };
                let mut entity = existing;
                entity.data.insert(LISTENER, data.to_nbt());
                chunk.set_block_entity(entity);
            }
            Listener::Warden(id) => {
                if let Some(entry) = self.entities.mobs.entries.get_mut(&id) {
                    entry.ai.warden_state_mut().vibration = data;
                }
            }
        }
    }

    fn source_facts(&self, id: Option<EntityId>) -> Option<SourceFacts> {
        let id = id?;
        if let Some(player) = self.players.values().find(|p| p.entity_id == id) {
            return Some(SourceFacts {
                spectator: player.game_mode == GameMode::Spectator,
                stepping_carefully: player.sneaking,
                dampens_vibrations: false,
            });
        }
        let entity = self.entities.store.get(id)?;
        Some(SourceFacts {
            spectator: false,
            stepping_carefully: false,
            dampens_vibrations: entity.base.kind == EntityKind::Warden,
        })
    }

    fn sculk_entity_uuid(&self, id: EntityId) -> Option<Uuid> {
        if let Some(player) = self.players.values().find(|p| p.entity_id == id) {
            return Some(player.profile.id.0);
        }
        self.entities.store.get(id).map(|entity| entity.base.uuid)
    }

    fn handle_game_event(
        &mut self,
        listener: Listener,
        event: GameEvent,
        source: Vec3,
        context: GameEventContext,
    ) {
        let Some(listener_at) = self.listener_position(listener) else {
            return;
        };
        if !is_postable(listener_at, source, Self::listener_radius(listener)) {
            return;
        }
        let mut data = self.listener_data(listener);
        if data.current.is_some() {
            return;
        }
        let facts = self.source_facts(context.entity);
        let can_avoid = matches!(listener, Listener::Sensor { .. } | Listener::Warden(_));
        if validate(
            event,
            Self::listener_tag(listener),
            facts,
            context.state,
            can_avoid,
        ) != Validity::Valid
        {
            return;
        }
        if !self.can_receive_vibration(listener, source.block(), event, context) {
            return;
        }
        if is_occluded(&self.world.chunks, listener_at, source) {
            return;
        }
        let distance = rspigot_domain::game_event::distance_between_in_blocks(
            source.block(),
            listener_at.block(),
        );
        let owner = context
            .entity
            .and_then(|id| self.entities.store.get(id))
            .and_then(owner_uuid);
        let info = VibrationInfo::new(
            event,
            distance,
            source,
            context.entity.and_then(|id| self.sculk_entity_uuid(id)),
            owner,
            context.entity,
        );
        data.selector
            .add_candidate(info, self.world.level.game_time);
        self.store_listener_data(listener, data);
    }

    fn can_receive_vibration(
        &self,
        listener: Listener,
        pos: BlockPos,
        event: GameEvent,
        context: GameEventContext,
    ) -> bool {
        match listener {
            Listener::Sensor { pos: own, .. } => {
                if pos == own && matches!(event, GameEvent::BlockDestroy | GameEvent::BlockPlace) {
                    return false;
                }
                event.frequency() != 0
                    && self.world.chunks.block(own).is_some_and(|state| {
                        state.property("sculk_sensor_phase") == Some("inactive")
                    })
            }
            Listener::Shrieker(own) => {
                let shrieking = self
                    .world
                    .chunks
                    .block(own)
                    .is_some_and(|state| state.property("shrieking") == Some("true"));
                !shrieking && self.shrieker_player(context.entity).is_some()
            }
            Listener::Warden(id) => {
                let Some(view) = self.mob_view(id) else {
                    return false;
                };
                let Some(entry) = self.entities.mobs.entries.get(&id) else {
                    return false;
                };
                if self
                    .entities
                    .store
                    .get(id)
                    .is_some_and(|entity| rspigot_domain::entity::living::is_no_ai(&entity.base))
                {
                    return false;
                }
                let world = self.mob_world(crate::mobs::sky_darken(&self.world.level));
                let living = context
                    .entity
                    .and_then(|source| rspigot_ai::AiWorld::living(&world, source));
                entry
                    .ai
                    .warden_can_receive_vibration(&view, &world, pos, living.as_ref())
            }
        }
    }

    pub(crate) fn tick_game_events(&mut self) {
        let now = self.current_tick();
        let due: Vec<u64> = self
            .game_events
            .due
            .range(..=now)
            .map(|(tick, _)| *tick)
            .collect();
        for tick in due {
            for (pos, kind) in self.game_events.due.remove(&tick).unwrap_or_default() {
                match kind {
                    TimerKind::Sensor => self.sensor_tick(pos),
                    TimerKind::Shrieker => self.shrieker_tick(pos),
                }
            }
        }
        let mut listeners = Vec::new();
        for chunk_pos in self.world.chunks.loaded_positions().collect::<Vec<_>>() {
            if let Some(chunk) = self.world.chunks.get(chunk_pos) {
                listeners.extend(chunk.block_entities.iter().filter_map(block_listener));
            }
        }
        listeners.extend(
            self.entities
                .mobs
                .entries
                .keys()
                .copied()
                .filter(|id| {
                    self.entities
                        .store
                        .get(*id)
                        .is_some_and(|e| e.base.kind == EntityKind::Warden)
                })
                .map(Listener::Warden),
        );
        for listener in listeners {
            self.tick_vibrations(listener);
        }
    }

    fn tick_vibrations(&mut self, listener: Listener) {
        let mut data = self.listener_data(listener);
        let now = self.world.level.game_time;
        let mut changed = false;
        if data.current.is_none()
            && let Some(chosen) = data.selector.chosen_candidate(now).cloned()
        {
            data.travel_time_in_ticks =
                rspigot_domain::game_event::travel_time_in_ticks(chosen.distance);
            data.current = Some(chosen);
            data.selector.start_over();
            changed = true;
        }
        if let Some(current) = data.current.clone() {
            data.decrement_travel_time();
            if data.travel_time_in_ticks < 1 {
                let listener_at = self.listener_position(listener).unwrap_or(current.pos);
                let needs_neighbours = !matches!(listener, Listener::Warden(_));
                if !needs_neighbours || self.adjacent_chunks_loaded(listener_at.block()) {
                    data.current = None;
                    self.store_listener_data(listener, data.clone());
                    self.receive_vibration(listener, &current, listener_at);
                    return;
                }
            }
            changed = true;
        }
        if changed {
            self.store_listener_data(listener, data);
        }
    }

    fn adjacent_chunks_loaded(&self, pos: BlockPos) -> bool {
        let chunk = pos.chunk();
        (-1..=1).all(|dx| {
            (-1..=1).all(|dz| {
                self.world
                    .chunks
                    .is_loaded(ChunkPos::new(chunk.x + dx, chunk.z + dz))
            })
        })
    }

    fn receive_vibration(&mut self, listener: Listener, info: &VibrationInfo, listener_at: Vec3) {
        let pos = info.pos.block();
        let distance =
            rspigot_domain::game_event::distance_between_in_blocks(pos, listener_at.block());
        let source = info
            .entity
            .filter(|id| self.entity_exists(*id))
            .or_else(|| info.source.and_then(|uuid| self.sculk_entity_by_uuid(uuid)));
        let owner = info
            .projectile_owner
            .and_then(|uuid| self.sculk_entity_by_uuid(uuid));
        match listener {
            Listener::Sensor {
                pos: own,
                calibrated,
            } => {
                self.sensor_receive(own, calibrated, info.event, source, distance);
            }
            Listener::Shrieker(own) => {
                let player = self.shrieker_player(owner.or(source));
                self.shrieker_try_shriek(own, player);
            }
            Listener::Warden(id) => self.warden_receive(id, pos, source, owner),
        }
    }

    fn entity_exists(&self, id: EntityId) -> bool {
        self.players.values().any(|p| p.entity_id == id) || self.entities.store.get(id).is_some()
    }

    fn sculk_entity_by_uuid(&self, uuid: Uuid) -> Option<EntityId> {
        self.owner_entity(Some(uuid))
    }

    pub(crate) fn ensure_sculk_entity(&mut self, pos: BlockPos, state: BlockStateId) {
        let Some(name) = state.kind().and_then(listener_kind) else {
            return;
        };
        if self.has_block_entity(pos) {
            return;
        }
        if let Some(chunk) = self.world.chunks.chunk_mut(pos.chunk()) {
            chunk.set_block_entity(BlockEntity {
                pos,
                kind: Identifier::minecraft(name),
                data: Compound::new(),
            });
        }
    }

    pub(crate) fn schedule_sculk(&mut self, pos: BlockPos, delay: u64, kind: TimerKind) {
        let due = self.current_tick() + delay;
        self.game_events
            .due
            .entry(due)
            .or_default()
            .push((pos, kind));
    }
}

fn block_listener(entity: &BlockEntity) -> Option<Listener> {
    match entity.kind.as_str() {
        "minecraft:sculk_sensor" => Some(Listener::Sensor {
            pos: entity.pos,
            calibrated: false,
        }),
        "minecraft:calibrated_sculk_sensor" => Some(Listener::Sensor {
            pos: entity.pos,
            calibrated: true,
        }),
        "minecraft:sculk_shrieker" => Some(Listener::Shrieker(entity.pos)),
        _ => None,
    }
}
