use rand::Rng;
use rspigot_blocks::BlockKind;
use rspigot_domain::damage::DamageType;
use rspigot_domain::entity::FLAG_INVISIBLE;
use rspigot_domain::entity::{Entity, EntityState, FallingState, RemovalReason, living};
use rspigot_domain::text::Component;
use rspigot_domain::{BlockStateId, EntityId, Rotation, Vec3};
use rspigot_entities::EntityKind as Kind;

use crate::commands::{CommandContext, Target};
use crate::console::console_text;
use crate::context::SUMMON_FAILED;
use crate::health::DamageSource;
use crate::items::PICKUP_DELAY;
use crate::plugins::events::{CreatureSpawnReason, EntityPotionEffectCause, EntityRemoveCause};
use crate::plugins::items::stack_of;
use crate::plugins::resolve::{self, finite_not_negative};
use crate::plugins::steps::{Birth, Spawned};
use crate::plugins::{
    ActionError, ActionErrorKind, EntityDetails, EntityFlags, EntityOrder, EntityView, ItemSpec,
};
use crate::ports::ClientLink;
use crate::server::Server;
use rspigot_domain::spawn::SpawnReason;

impl<L: ClientLink> Server<L> {
    pub(super) fn apply_entity_order(&mut self, connection: u64, id: EntityId, order: EntityOrder) {
        let name = order.name();
        if let EntityOrder::SetAttribute { attribute, value } = &order {
            if let Err(reason) = self.set_attribute_for_plugin(id, attribute, *value) {
                self.warn_plugin(connection, name, reason);
            }
            return;
        }
        if self.entities.store.get(id).is_none() {
            self.warn_plugin(connection, name, "the entity is gone");
            return;
        }
        if let Err(reason) = self.entity_order(id, order) {
            self.warn_plugin(connection, name, reason);
        }
    }

    fn entity_order(&mut self, id: EntityId, order: EntityOrder) -> Result<(), String> {
        let target = Target::Entity(id);
        match order {
            EntityOrder::Teleport(position) => {
                let position = resolve::position_of(position)?;
                self.relocate(id, position, false);
            }
            EntityOrder::Remove => {
                self.remove_entity(
                    id,
                    RemovalReason::Discarded,
                    Some(EntityRemoveCause::Plugin),
                );
            }
            EntityOrder::Damage(amount) => {
                let amount = resolve::damage_of(amount)?;
                self.hurt_entity(id, &DamageSource::new(DamageType::Generic), amount);
            }
            EntityOrder::Kill => {
                CommandContext::kill_target(self, target);
            }
            EntityOrder::AddEffect(spec) => {
                let effect = resolve::effect_of(&spec)?;
                self.apply_entity_effect(id, effect, EntityPotionEffectCause::Plugin);
            }
            EntityOrder::RemoveEffect(key) => {
                let effect = resolve::effect_id(&key)?;
                self.remove_entity_effects(id, Some(effect), EntityPotionEffectCause::Plugin);
            }
            EntityOrder::ClearEffects => {
                self.remove_entity_effects(id, None, EntityPotionEffectCause::Plugin);
            }
            EntityOrder::SetAttribute { .. } => {}
            EntityOrder::Meta(order) => {
                self.entity_meta_change(id, order).map_err(|e| e.reason)?;
            }
            EntityOrder::Change(change) => self.change_entity(id, change)?,
            EntityOrder::Edit(edit) => self.edit_entity(id, edit)?,
            EntityOrder::Object(order) => self.object_order(id, order)?,
            EntityOrder::SetVelocity(velocity) => {
                let velocity = resolve::velocity_of(velocity)?;
                if let Some(entity) = self.entities.store.get_mut(id) {
                    entity.base.velocity = velocity;
                    entity.base.needs_sync = true;
                }
            }
        }
        Ok(())
    }

    pub(crate) fn entity_view(&self, id: EntityId) -> Option<EntityView> {
        self.entities
            .store
            .get(id)
            .map(view_of)
            .or_else(|| self.entities.npcs.get(id).map(crate::npc::view_of))
    }

    pub(super) fn entity_details(&self, id: EntityId) -> Option<EntityDetails> {
        let entity = self.entities.store.get(id)?;
        let base = &entity.base;
        Some(EntityDetails {
            entity: view_of(entity),
            health: entity.living().map(|_| living::health(base)),
            velocity: base.velocity,
            on_ground: base.on_ground,
            custom_name: base.custom_name().map(Component::to_legacy),
            fire_ticks: self.entity_fire_ticks(id)?,
            flags: EntityFlags {
                custom_name_visible: base.custom_name_visible(),
                glowing: base.has_glowing_tag,
                invisible: base.shared_flag(FLAG_INVISIBLE),
                silent: base.is_silent(),
                gravity: !base.is_no_gravity(),
                invulnerable: base.invulnerable,
                persistent: entity
                    .living()
                    .and_then(|living| living.mob.as_ref())
                    .is_some_and(|mob| mob.persistence_required),
            },
            freeze_ticks: base.ticks_frozen(),
            air: base.air_supply(),
            fall_distance: base.fall_distance as f32,
            rotation: Rotation {
                yaw: base.yaw,
                pitch: base.pitch,
            },
            ticks_lived: base.tick_count,
            tags: base.tags.clone(),
            passengers: self.entities.rides.passengers_of(id).to_vec(),
            vehicle: self.entities.rides.vehicle_of(id),
            living: self.living_details(id),
        })
    }

    pub(super) fn listed_entities(
        &self,
        near: Option<(Vec3, f64)>,
        kind: Option<&str>,
    ) -> Result<Vec<EntityView>, String> {
        let near = match near {
            Some((center, radius)) => {
                finite_not_negative("radius", radius)?;
                Some((resolve::position_of(center)?, radius * radius))
            }
            None => None,
        };
        let kind = kind.map(entity_kind).transpose()?;
        let mut views: Vec<EntityView> = self
            .entities
            .store
            .iter()
            .filter(|entity| kind.is_none_or(|kind| entity.base.kind == kind))
            .filter(|entity| {
                near.is_none_or(|(center, reach)| {
                    entity.base.position.distance_squared(center) <= reach
                })
            })
            .map(view_of)
            .collect();
        views.sort_by_key(|view| view.id);
        Ok(views)
    }

    pub(super) fn spawn_for_plugin(
        &mut self,
        connection: u64,
        request: u64,
        kind: &str,
        position: Vec3,
    ) -> Result<(), ActionError> {
        self.spawn_with_for_plugin(connection, request, kind, position, None, None)
    }

    pub(super) fn spawn_with_for_plugin(
        &mut self,
        connection: u64,
        request: u64,
        kind: &str,
        position: Vec3,
        block: Option<&str>,
        velocity: Option<Vec3>,
    ) -> Result<(), ActionError> {
        let kind = entity_kind(kind)?;
        let state = block.map(|key| falling_state_of(kind, key)).transpose()?;
        let velocity = velocity.map(launch_velocity).transpose()?;
        let position = self.loaded_position(position)?;
        let (mut entity, random) = match state {
            Some(state) => (self.falling_entity(position, state), None),
            None => {
                let (entity, random) = self
                    .summon_entity(kind, position, None, SpawnReason::Command)
                    .map_err(|key| {
                        ActionError::new(ActionErrorKind::IncompatibleState, refusal(key))
                    })?;
                (entity, Some(random))
            }
        };
        if let Some(velocity) = velocity {
            entity.base.velocity = velocity;
        }
        let birth = Birth::of(CreatureSpawnReason::Custom).then(Spawned::Answered {
            connection,
            request,
        });
        self.spawn_entity(
            entity,
            match random {
                Some(random) => birth.with_random(random),
                None => birth,
            },
        );
        Ok(())
    }

    fn falling_entity(&mut self, position: Vec3, state: BlockStateId) -> Entity {
        let mut base = self.new_entity_base(Kind::FallingBlock);
        base.snap_to(position, 0.0, 0.0);
        let mut falling = FallingState::new(&mut base, state, position.block());
        falling.drop_item = THROWN_BLOCK_DROPS_ITEM;
        falling.hurt_entities = THROWN_BLOCK_HURTS_ENTITIES;
        Entity::new(base, EntityState::Falling(falling))
    }

    fn loaded_position(&self, position: Vec3) -> Result<Vec3, ActionError> {
        let position = resolve::position_of(position)?;
        if self.world.chunks.is_loaded(position.chunk()) {
            Ok(position)
        } else {
            Err(ActionError::new(
                ActionErrorKind::IncompatibleState,
                refusal(SUMMON_FAILED),
            ))
        }
    }

    pub(super) fn drop_for_plugin(
        &mut self,
        connection: u64,
        request: u64,
        position: Vec3,
        item: &ItemSpec,
    ) -> Result<(), ActionError> {
        let stack =
            stack_of(item)?.ok_or_else(|| ActionError::from("air cannot be dropped".to_owned()))?;
        let position = self.loaded_position(position)?;
        let velocity = Vec3::new(
            self.rng.random::<f64>() * 0.2 - 0.1,
            0.2,
            self.rng.random::<f64>() * 0.2 - 0.1,
        );
        let item = self.item_entity(position, velocity, PICKUP_DELAY, stack);
        self.spawn_entity(
            item,
            Birth::of(CreatureSpawnReason::Default).then(Spawned::Answered {
                connection,
                request,
            }),
        );
        Ok(())
    }
}

fn view_of(entity: &Entity) -> EntityView {
    let base = &entity.base;
    EntityView {
        id: base.id,
        uuid: base.uuid,
        kind: base.kind.name().to_owned(),
        position: base.position,
        player: None,
        meta: crate::plugins::MetaSnapshot::of(base),
        marks: crate::plugins::EntityMarks::of(base),
    }
}

const MAX_LAUNCH_SPEED: f64 = 3.9;
const THROWN_BLOCK_DROPS_ITEM: bool = false;
const THROWN_BLOCK_HURTS_ENTITIES: bool = false;

fn launch_velocity(velocity: Vec3) -> Result<Vec3, ActionError> {
    let velocity = resolve::velocity_of(velocity)?;
    let clamp = |axis: f64| axis.clamp(-MAX_LAUNCH_SPEED, MAX_LAUNCH_SPEED);
    Ok(Vec3::new(
        clamp(velocity.x),
        clamp(velocity.y),
        clamp(velocity.z),
    ))
}

fn falling_state_of(kind: Kind, key: &str) -> Result<BlockStateId, ActionError> {
    if kind != Kind::FallingBlock {
        return Err(ActionError::from(format!(
            "a block is only valid with minecraft:falling_block, not {}",
            kind.name()
        )));
    }
    let state = BlockKind::from_name(key)
        .ok_or_else(|| ActionError::from(format!("unknown block {key}")))?
        .default_state();
    if state.is_air() {
        return Err(ActionError::from(format!(
            "{key} cannot be a falling block"
        )));
    }
    Ok(state)
}

fn entity_kind(key: &str) -> Result<Kind, String> {
    Kind::from_name(key).ok_or_else(|| format!("unknown entity type {key}"))
}

pub(super) fn refusal(key: &str) -> String {
    console_text(&Component::translatable(key, Vec::new()))
}
