use std::sync::Arc;

use rand::Rng;
use rspigot_domain::entity::living::{self, EQUIPMENT_SLOTS};
use rspigot_domain::entity::tracker::{TrackerUpdate, unpack_degrees};
use rspigot_domain::entity::{
    Entities, Entity, EntityBase, EntityState, EntityTracker, RemovalReason, SyncedValue, fluid,
    item, save, tracking_range,
};
use rspigot_domain::{ChunkPos, EntityId, Rotation, Vec3};
use rspigot_entities::EntityKind as Kind;
use rspigot_items::EquipmentSlot as ItemSlot;
use rspigot_nbt::Compound;
use rustc_hash::{FxHashMap, FxHashSet};
use uuid::Uuid;

use crate::chunks::{ChunkJob, ChunkResult, Priority};
use crate::events::{
    ClientEvent, EntityKind, EntityStatus, EquipmentSlot, MetadataEntry, MetadataValue,
};
use crate::player::Player;
use crate::plugins::events::{EntityPotionEffectCause, EntityRemoveCause};
use crate::plugins::{Continuation, Holder, Lane, Potion};
use crate::ports::ClientLink;
use crate::server::{Audience, Server};

#[derive(Debug)]
pub(crate) struct TrackedEntity {
    pub tracker: EntityTracker,
    pub range: i32,
    pub section: (i32, i32, i32),
}

#[derive(Debug, Default)]
pub(crate) struct EntityWorld {
    pub store: Entities,
    pub tracked: FxHashMap<EntityId, TrackedEntity>,
    pub mobs: crate::mobs::MobTable,
    pub poses: FxHashMap<EntityId, rspigot_domain::entity::Pose>,
    pending_saves: FxHashMap<ChunkPos, (u64, Arc<Vec<Compound>>)>,
    stored_empty: FxHashSet<ChunkPos>,
    generation: u64,
}

fn section_of(position: Vec3) -> (i32, i32, i32) {
    let block = position.block();
    (block.x >> 4, block.y >> 4, block.z >> 4)
}

pub(crate) fn wire_kind(kind: Kind) -> EntityKind {
    match kind {
        Kind::Player => EntityKind::Player,
        Kind::Item => EntityKind::Item,
        Kind::FallingBlock => EntityKind::FallingBlock,
        other => EntityKind::Other(other.id().0),
    }
}

pub(crate) fn metadata(values: &[(u8, SyncedValue)]) -> Vec<MetadataEntry> {
    values
        .iter()
        .map(|(index, value)| MetadataEntry {
            index: *index,
            value: match value {
                SyncedValue::Byte(v) => MetadataValue::Byte(*v),
                SyncedValue::Int(v) => MetadataValue::VarInt(*v),
                SyncedValue::Float(v) => MetadataValue::Float(*v),
                SyncedValue::OptionalComponent(c) => MetadataValue::OptionalComponent(c.clone()),
                SyncedValue::Boolean(v) => MetadataValue::Boolean(*v),
                SyncedValue::Pose(p) => MetadataValue::Pose(*p),
                SyncedValue::Item(stack) => MetadataValue::Item(stack.clone()),
                SyncedValue::BlockPos(pos) => MetadataValue::BlockPos(*pos),
                SyncedValue::Variant(kind, id) => MetadataValue::Variant(*kind, *id),
                SyncedValue::Long(v) => MetadataValue::Long(*v),
                SyncedValue::Direction(face) => MetadataValue::Direction(*face as u8),
                SyncedValue::State(kind, id) => MetadataValue::State(*kind, *id),
                SyncedValue::OptionalUnsignedInt(v) => MetadataValue::OptionalUnsignedInt(*v),
                SyncedValue::OptionalEntity(v) => MetadataValue::OptionalEntity(*v),
                SyncedValue::OptionalBlockState(v) => {
                    MetadataValue::OptionalBlockState(v.map(|state| u32::from(state.0)))
                }
                SyncedValue::VillagerData {
                    kind,
                    profession,
                    level,
                } => MetadataValue::VillagerData {
                    kind: *kind,
                    profession: *profession,
                    level: *level,
                },
            },
        })
        .collect()
}

pub(crate) fn wire_slot(slot: ItemSlot) -> EquipmentSlot {
    match slot {
        ItemSlot::Mainhand => EquipmentSlot::MainHand,
        ItemSlot::Offhand => EquipmentSlot::OffHand,
        ItemSlot::Feet => EquipmentSlot::Feet,
        ItemSlot::Legs => EquipmentSlot::Legs,
        ItemSlot::Chest => EquipmentSlot::Chest,
        ItemSlot::Head => EquipmentSlot::Head,
        ItemSlot::Body => EquipmentSlot::Body,
        ItemSlot::Saddle => EquipmentSlot::Saddle,
    }
}

fn pairing_events(entity: &Entity, tracker: &EntityTracker, data: i32) -> Vec<ClientEvent> {
    let pairing = tracker.pairing();
    let id = entity.base.id;
    let mut events = vec![
        ClientEvent::BundleDelimiter,
        ClientEvent::SpawnEntity {
            entity: id,
            uuid: entity.base.uuid,
            kind: wire_kind(entity.base.kind),
            position: pairing.position,
            rotation: Rotation::new(pairing.yaw, pairing.pitch),
            head_yaw: pairing.head_yaw,
            data,
            velocity: pairing.movement,
        },
    ];
    if !pairing.data.is_empty() {
        events.push(ClientEvent::EntityMetadata {
            entity: id,
            entries: metadata(&pairing.data),
        });
    }
    if let EntityState::Living(state) = &entity.state {
        let attributes = state.attributes.syncable(&state.effects);
        if !attributes.is_empty() {
            events.push(ClientEvent::Attributes {
                entity: id,
                attributes,
            });
        }
        let slots: Vec<(EquipmentSlot, rspigot_domain::item::Slot)> = EQUIPMENT_SLOTS
            .iter()
            .filter_map(|slot| {
                state
                    .equipment(*slot)
                    .map(|stack| (wire_slot(*slot), Some(stack.clone())))
            })
            .collect();
        if !slots.is_empty() {
            events.push(ClientEvent::EntityEquipment { entity: id, slots });
        }
    }
    events.push(ClientEvent::BundleDelimiter);
    events
}

fn update_events(id: EntityId, updates: Vec<TrackerUpdate>) -> Vec<ClientEvent> {
    updates
        .into_iter()
        .map(|update| match update {
            TrackerUpdate::Motion(velocity) | TrackerUpdate::SelfMotion(velocity) => {
                ClientEvent::EntityVelocity {
                    entity: id,
                    velocity,
                }
            }
            TrackerUpdate::Move(movement) => ClientEvent::EntityMove {
                entity: id,
                movement,
            },
            TrackerUpdate::Data(values) => ClientEvent::EntityMetadata {
                entity: id,
                entries: metadata(&values),
            },
            TrackerUpdate::Attributes(attributes) => ClientEvent::Attributes {
                entity: id,
                attributes,
            },
            TrackerUpdate::HeadRotation(packed) => ClientEvent::EntityHeadRotation {
                entity: id,
                yaw: unpack_degrees(packed),
            },
        })
        .collect()
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn random_uuid(&mut self) -> Uuid {
        uuid::Builder::from_random_bytes(self.rng.random()).into_uuid()
    }

    pub(crate) fn new_entity_base(&mut self, kind: Kind) -> EntityBase {
        let id = self.allocate_entity_id();
        let uuid = self.random_uuid();
        EntityBase::new(id, uuid, kind)
    }

    pub(crate) fn place_entity(&mut self, mut entity: Entity) -> Result<EntityId, Box<Entity>> {
        if entity.base.kind == Kind::Fireball
            && let EntityState::Hurting(state) = &mut entity.state
        {
            state.incendiary = self.config.rules.mob_griefing;
        }
        let simulated = entity.is_simulated();
        let kind = entity.base.kind;
        let section = section_of(entity.base.position);
        let tracker = EntityTracker::new(&entity);
        let id = match self.entities.store.insert(entity) {
            Ok(id) => id,
            Err(rejected) => return Err(Box::new(rejected.1)),
        };
        let range = tracking_range(kind, &self.config.tracking);
        if simulated && range > 0 {
            self.entities.tracked.insert(
                id,
                TrackedEntity {
                    tracker,
                    range,
                    section,
                },
            );
            self.update_entity_viewers(id);
        }
        self.register_mob(id);
        Ok(id)
    }

    pub(crate) fn remove_entity(
        &mut self,
        id: EntityId,
        reason: RemovalReason,
        cause: Option<EntityRemoveCause>,
    ) -> Option<Entity> {
        self.deliver();
        if let Some(cause) = cause {
            self.notify_removal(id, cause);
        }
        self.stop_mob_brain(id);
        let mut entity = self.entities.store.remove(id)?;
        entity.base.removal = Some(reason);
        self.entities.tracked.remove(&id);
        self.entities.mobs.entries.remove(&id);
        self.entities.mobs.initial_targets.remove(&id);
        for player in self.players.values_mut() {
            if player.tracked.remove(&id) {
                player.link.send(ClientEvent::RemoveEntities(vec![id]));
            }
        }
        Some(entity)
    }

    fn view_distance_blocks(&self, player: &Player<L>) -> i32 {
        player
            .streamer
            .view_distance()
            .clamp(2, i32::from(self.config.view_distance))
            * 16
    }

    fn sees(&self, player: &Player<L>, entity: &Entity, range: i32) -> bool {
        let range = f64::from(range.min(self.view_distance_blocks(player)));
        let dx = player.position.x - entity.base.position.x;
        let dz = player.position.z - entity.base.position.z;
        dx * dx + dz * dz <= range * range && player.streamer.has_sent(entity.base.chunk())
    }

    fn update_viewer(&mut self, id: EntityId, player_id: rspigot_domain::player::PlayerId) {
        let (Some(entity), Some(tracked), Some(player)) = (
            self.entities.store.get(id),
            self.entities.tracked.get(&id),
            self.players.get(&player_id),
        ) else {
            return;
        };
        let visible = self.sees(player, entity, tracked.range);
        let tracking = player.tracked.contains(&id);
        if visible && !tracking {
            let data = self.projectile_owner_id(entity);
            let events = pairing_events(entity, &tracked.tracker, data);
            if let Some(player) = self.players.get_mut(&player_id) {
                player.tracked.insert(id);
                for event in events {
                    player.link.send(event);
                }
            }
        } else if !visible
            && tracking
            && let Some(player) = self.players.get_mut(&player_id)
        {
            player.tracked.remove(&id);
            player.link.send(ClientEvent::RemoveEntities(vec![id]));
        }
    }

    pub(crate) fn update_entity_viewers(&mut self, id: EntityId) {
        let players: Vec<rspigot_domain::player::PlayerId> = self.players.keys().copied().collect();
        for player in players {
            self.update_viewer(id, player);
        }
    }

    pub(crate) fn update_player_view(&mut self, player: rspigot_domain::player::PlayerId) {
        let ids: Vec<EntityId> = self.entities.tracked.keys().copied().collect();
        for id in ids {
            self.update_viewer(id, player);
        }
    }

    pub(crate) fn entity_ticking(&self, chunk: ChunkPos) -> bool {
        let simulation = i32::from(self.config.simulation_distance);
        let near = self
            .players
            .values()
            .any(|p| p.position.chunk().chebyshev_distance(chunk) <= simulation);
        near && (-1..=1).all(|dx| {
            (-1..=1).all(|dz| {
                self.world
                    .chunks
                    .is_loaded(ChunkPos::new(chunk.x + dx, chunk.z + dz))
            })
        })
    }

    pub(crate) fn send_entity_changes(&mut self) {
        let ids: Vec<EntityId> = self.entities.tracked.keys().copied().collect();
        for id in ids {
            let Some(position) = self.entities.store.get(id).map(|e| e.base.position) else {
                continue;
            };
            let section = section_of(position);
            let changed = self.entities.tracked.get_mut(&id).is_some_and(|tracked| {
                let changed = tracked.section != section;
                tracked.section = section;
                changed
            });
            if changed {
                self.update_entity_viewers(id);
            }
            let ticking = self.entity_ticking(position.chunk());
            let (Some(entity), Some(tracked)) = (
                self.entities.store.get_mut(id),
                self.entities.tracked.get_mut(&id),
            ) else {
                continue;
            };
            if changed || entity.base.needs_sync || ticking {
                let updates = tracked.tracker.send_changes(entity);
                for event in update_events(id, updates) {
                    self.outbox.push((Audience::Trackers(id), event));
                }
            }
        }
    }

    pub(crate) fn tick_entities(&mut self) {
        self.refresh_mob_players();
        self.activate_entities();
        for id in self.entities.store.ids() {
            let Some(entity) = self.entities.store.get(id) else {
                continue;
            };
            if !entity.is_simulated() || !self.entity_ticking(entity.base.chunk()) {
                continue;
            }
            match &entity.state {
                EntityState::Item(_) => self.tick_item_entity(id),
                EntityState::Falling(_) => self.tick_falling_entity(id),
                EntityState::Living(_) => {
                    if self.check_mob_despawn(id) {
                        continue;
                    }
                    self.tick_living_entity(id);
                }
                EntityState::Orb(_) => self.tick_orb(id),
                EntityState::Arrow(_) => self.tick_arrow(id),
                EntityState::Thrown(_) => self.tick_thrown(id),
                EntityState::Hurting(_) => self.tick_hurting(id),
                EntityState::Bullet(_) => self.tick_bullet(id),
                EntityState::Inert(_) => {}
            }
            self.entities.store.reindex(id);
        }
    }

    fn tick_living_entity(&mut self, id: EntityId) {
        if !self.mob_active(id) {
            self.inactive_tick(id);
            return;
        }
        if let Some(entity) = self.entities.store.get_mut(id) {
            let base = &mut entity.base;
            base.old_position = base.position;
            base.tick_count = base.tick_count.wrapping_add(1);
            base.was_in_powder_snow = base.in_powder_snow;
            fluid::update(base, &self.world.chunks);
        }
        if !self.species_pre_tick(id) {
            return;
        }
        self.void_check(id);
        let Some(Entity {
            base,
            state: EntityState::Living(state),
            ..
        }) = self.entities.store.get_mut(id)
        else {
            return;
        };
        let outcome = living::base_tick(base, state);
        self.tick_mob_effects(id);
        let Some(Entity {
            state: EntityState::Living(state),
            ..
        }) = self.entities.store.get_mut(id)
        else {
            return;
        };
        let spent = state.effects.countdown();
        for effect in spent {
            self.potion(
                Holder::Mob(id),
                Potion::Expire(effect),
                EntityPotionEffectCause::Expiration,
            );
        }
        if outcome.burn {
            self.burn_damage(id);
        }
        if outcome.removed {
            self.push(
                Audience::Trackers(id),
                ClientEvent::EntityStatus {
                    entity: id,
                    status: EntityStatus::Poof,
                },
            );
            self.remove_entity(id, RemovalReason::Killed, Some(EntityRemoveCause::Death));
            return;
        }
        self.mob_ambient_tick(id);
        self.forget_dead_attackers(id);
        self.kind_ai_step(id);
        self.crossbow_use_tick(id);
        if let Some(Entity {
            base,
            state: EntityState::Living(state),
            ..
        }) = self.entities.store.get_mut(id)
        {
            living::tick_using_item(base, state);
            living::tick_arrows(base, state);
            living::begin_ai_step(base, state);
        }
        self.mob_server_ai_step(id);
        let can_float = self.mob_can_float(id);
        let chunks = &self.world.chunks;
        let mut landing = None;
        if let Some(Entity {
            base,
            state: EntityState::Living(state),
            ..
        }) = self.entities.store.get_mut(id)
        {
            landing = living::finish_ai_step(base, state, chunks, can_float);
        }
        if let Some(landing) = landing {
            self.mob_landed(id, landing);
        }
        self.apply_fire_inside(id);
        if self.entities.store.get(id).is_none() {
            return;
        }
        self.mob_ai_step_after(id);
        self.mob_after_tick(id);
    }

    fn inactive_tick(&mut self, id: EntityId) {
        if let Some(Entity {
            base,
            state: EntityState::Living(state),
            ..
        }) = self.entities.store.get_mut(id)
            && let Some(ageable) = state.mob.as_mut().and_then(|mob| mob.ageable.as_mut())
        {
            rspigot_domain::entity::ageable::inactive_tick(base, ageable);
        }
    }

    pub(crate) fn handle_chunk_events(&mut self) {
        self.drain_startup();
        let events = self.world.chunks.take_events();
        for (pos, generation) in events.entities_saved {
            if self
                .entities
                .pending_saves
                .get(&pos)
                .is_some_and(|(pending, _)| *pending == generation)
            {
                self.entities.pending_saves.remove(&pos);
            }
        }
        for (pos, new_chunk) in events.loaded {
            self.chunk_loaded(pos, new_chunk);
        }
        for (pos, entities) in events.entity_loads {
            self.load_chunk_entities(pos, entities);
            self.entities_loaded(pos);
        }
        for pos in events.unloaded {
            self.save_chunk_entities(pos, true);
        }
        if events.autosaved {
            self.world_saving();
            let decisions = &self.decisions;
            self.world.chunks.save_dirty_except(&self.workers, |pos| {
                decisions.in_flight(Lane::Chunk(pos), |then| {
                    matches!(then, Continuation::ChunkUnload(_))
                })
            });
            self.save_all_entities();
            self.persist_all_players();
        }
    }

    fn load_chunk_entities(&mut self, pos: ChunkPos, entities: Vec<Compound>) {
        let entities = match self.entities.pending_saves.get(&pos) {
            Some((_, pending)) => pending.as_ref().clone(),
            None => entities,
        };
        if entities.is_empty() {
            self.entities.stored_empty.insert(pos);
        } else {
            self.entities.stored_empty.remove(&pos);
        }
        for nbt in entities {
            let id = self.allocate_entity_id();
            let uuid = self.random_uuid();
            match save::load_entity(&nbt, id, uuid) {
                Ok(entity) => {
                    let uuid = entity.base.uuid;
                    if self.entities.store.by_uuid(uuid).is_some() {
                        tracing::warn!("UUID of added entity already exists: {uuid}");
                        continue;
                    }
                    let _ = self.place_entity(entity);
                }
                Err(error) => {
                    tracing::warn!(?pos, %error, "Skipping entity that failed to load");
                }
            }
        }
    }

    fn save_chunk_entities(&mut self, pos: ChunkPos, unload: bool) {
        let world_uuid = self.world.level.uuid;
        let ids = self.entities.store.in_chunk(pos).to_vec();
        let (kept, saved): (Vec<EntityId>, Vec<Compound>) = ids
            .iter()
            .filter_map(|id| self.entities.store.get(*id))
            .filter_map(|entity| Some((entity.base.id, self.entity_nbt(entity, world_uuid)?)))
            .unzip();
        let merge = !unload && !self.world.chunks.is_loaded(pos);
        if unload {
            self.entities_unloading(pos, &kept);
            for id in ids {
                self.remove_entity(
                    id,
                    RemovalReason::UnloadedToChunk,
                    Some(EntityRemoveCause::Unload),
                );
            }
        }
        if saved.is_empty() {
            if merge || !self.entities.stored_empty.insert(pos) {
                return;
            }
        } else {
            self.entities.stored_empty.remove(&pos);
        }
        self.entities.generation += 1;
        let generation = self.entities.generation;
        let data = Arc::new(saved);
        if !merge {
            self.entities
                .pending_saves
                .insert(pos, (generation, Arc::clone(&data)));
        }
        self.workers.submit(
            Priority::BACKGROUND,
            ChunkJob::SaveEntities {
                dimension: Arc::clone(&self.world.dimension),
                pos,
                entities: data,
                merge,
                generation,
            },
        );
    }

    pub(crate) fn save_all_entities(&mut self) {
        let mut chunks: Vec<ChunkPos> = self.world.chunks.loaded_positions().collect();
        chunks.extend(self.entities.store.chunks());
        chunks.sort_unstable();
        chunks.dedup();
        for pos in chunks {
            self.save_chunk_entities(pos, false);
        }
    }

    pub(crate) fn entity_name(&self, id: EntityId) -> Option<rspigot_domain::text::Component> {
        let entity = self.entities.store.get(id)?;
        Some(match entity.base.custom_name() {
            Some(name) => name.clone(),
            None => rspigot_domain::text::Component::translatable(
                entity_translation_key(entity.base.kind),
                Vec::new(),
            ),
        })
    }

    pub(crate) fn item_stack_of(&self, id: EntityId) -> Option<rspigot_domain::item::ItemStack> {
        self.entities
            .store
            .get(id)
            .and_then(|e| item::stack(&e.base).cloned())
    }

    pub fn entity_count(&self) -> usize {
        self.entities.store.len()
    }

    pub fn wait_for_entity_saves(&mut self) {
        while !self.entities.pending_saves.is_empty() {
            let Ok(result) = self.workers.results().recv() else {
                return;
            };
            let failed = matches!(
                &result,
                ChunkResult::Failed { pos, .. } if self.entities.pending_saves.contains_key(pos)
            );
            self.world.chunks.accept(result);
            self.handle_chunk_events();
            if failed {
                return;
            }
        }
    }

    pub fn saved_entity(&self, id: EntityId) -> Option<Compound> {
        self.entity_nbt(self.entities.store.get(id)?, None)
    }

    pub fn entities_of(&self, kind: Kind) -> Vec<EntitySnapshot> {
        self.entities
            .store
            .iter()
            .filter(|e| e.base.kind == kind)
            .map(|e| EntitySnapshot {
                id: e.base.id,
                uuid: e.base.uuid,
                position: e.base.position,
                velocity: e.base.velocity,
                yaw: e.base.yaw,
                on_ground: e.base.on_ground,
                health: e.living().map(|_| living::health(&e.base)),
                simulated: e.is_simulated(),
            })
            .collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EntitySnapshot {
    pub id: EntityId,
    pub uuid: Uuid,
    pub position: Vec3,
    pub velocity: Vec3,
    pub yaw: f32,
    pub on_ground: bool,
    pub health: Option<f32>,
    pub simulated: bool,
}

pub(crate) fn entity_translation_key(kind: Kind) -> String {
    let name = kind.name();
    let path = name.strip_prefix("minecraft:").unwrap_or(name);
    format!("entity.minecraft.{path}")
}
