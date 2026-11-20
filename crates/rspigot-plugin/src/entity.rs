use rspigot_plugin_protocol::v1::{
    self, answer, entity_edit::Edit, entity_order, from_plugin, question,
};
use uuid::Uuid;

use crate::attribute::EntityAttributesHandle;
use crate::entity_groups::{
    EntityAgeHandle, EntityAiHandle, EntityEquipmentHandle, EntityFlagsHandle, EntityLeashHandle,
    EntityNameHandle, EntityOwnerHandle, EntityPassengersHandle, EntityTagsHandle,
    EntityVitalsHandle, LivingState, fetch,
};
use crate::link::Link;
use crate::meta::MetaHandle;
use crate::server::Server;
use crate::wire::parse_uuid;
use crate::{Effect, EffectType, EntityType, Error, Player, Position, Rotation, Vector};

#[derive(Debug, Clone)]
pub struct Entity {
    pub entity_id: i32,
    pub uuid: Uuid,
    pub kind: EntityType,
    pub position: Position,
    pub custom_name: Option<String>,
    pub tags: Vec<String>,
    pub attributes: EntityAttributesHandle,
    pub name: EntityNameHandle,
    pub flags: EntityFlagsHandle,
    pub vitals: EntityVitalsHandle,
    pub scoreboard_tags: EntityTagsHandle,
    pub passengers: EntityPassengersHandle,
    pub equipment: EntityEquipmentHandle,
    pub ai: EntityAiHandle,
    pub leash: EntityLeashHandle,
    pub owner: EntityOwnerHandle,
    pub age: EntityAgeHandle,
    pub meta: MetaHandle,
    pub volatile_meta: MetaHandle,
    pub player: Option<Player>,
    link: Link,
}

impl Entity {
    pub(crate) fn from_wire(snapshot: v1::EntitySnapshot, link: &Link) -> Result<Self, Error> {
        let kind = EntityType::from_key(&snapshot.kind).ok_or_else(|| {
            Error::Malformed(format!("an unknown entity type `{}`", snapshot.kind))
        })?;
        Ok(Self {
            attributes: EntityAttributesHandle::new(snapshot.entity_id, link.clone()),
            name: EntityNameHandle::new(snapshot.entity_id, link.clone()),
            flags: EntityFlagsHandle::new(snapshot.entity_id, link.clone()),
            vitals: EntityVitalsHandle::new(snapshot.entity_id, link.clone()),
            scoreboard_tags: EntityTagsHandle::new(snapshot.entity_id, link.clone()),
            passengers: EntityPassengersHandle::new(snapshot.entity_id, link.clone()),
            equipment: EntityEquipmentHandle::new(snapshot.entity_id, link.clone()),
            ai: EntityAiHandle::new(snapshot.entity_id, link.clone()),
            leash: EntityLeashHandle::new(snapshot.entity_id, link.clone()),
            owner: EntityOwnerHandle::new(snapshot.entity_id, link.clone()),
            age: EntityAgeHandle::new(snapshot.entity_id, link.clone()),
            custom_name: snapshot.custom_name,
            tags: snapshot.tags,
            meta: MetaHandle::of_entity(
                snapshot.entity_id,
                false,
                crate::meta::snapshot(snapshot.meta)?,
                link.clone(),
            ),
            volatile_meta: MetaHandle::of_entity(
                snapshot.entity_id,
                true,
                crate::meta::snapshot(snapshot.volatile_meta)?,
                link.clone(),
            ),
            entity_id: snapshot.entity_id,
            uuid: parse_uuid(&snapshot.uuid, "an entity")?,
            kind,
            position: Position::from_wire(snapshot.position, "position")?,
            player: snapshot
                .player
                .map(|player| Player::from_snapshot(player, link))
                .transpose()?,
            link: link.clone(),
        })
    }

    pub(crate) fn to_wire(&self) -> v1::EntitySnapshot {
        v1::EntitySnapshot {
            entity_id: self.entity_id,
            uuid: self.uuid.hyphenated().to_string(),
            kind: self.kind.key().to_owned(),
            position: Some(self.position.to_wire()),
            player: self.player.as_ref().map(Player::to_snapshot),
            meta: Default::default(),
            volatile_meta: Default::default(),
            custom_name: self.custom_name.clone(),
            tags: self.tags.clone(),
        }
    }

    pub fn teleport(&self, position: Position) {
        self.order(entity_order::Order::Teleport(position.to_wire()));
    }

    pub fn remove(&self) {
        self.order(entity_order::Order::Remove(v1::Empty {}));
    }

    pub fn damage(&self, amount: f64) {
        self.order(entity_order::Order::Damage(amount));
    }

    pub fn kill(&self) {
        self.order(entity_order::Order::Kill(v1::Empty {}));
    }

    pub fn add_effect(&self, effect: Effect) {
        self.order(entity_order::Order::AddEffect(effect.to_wire()));
    }

    pub fn remove_effect(&self, kind: EffectType) {
        self.order(entity_order::Order::RemoveEffect(kind.key().to_owned()));
    }

    pub fn clear_effects(&self) {
        self.order(entity_order::Order::ClearEffects(v1::Empty {}));
    }

    pub fn set_velocity(&self, velocity: Vector) {
        self.order(entity_order::Order::SetVelocity(velocity.to_wire()));
    }

    pub fn set_rotation(&self, rotation: Rotation) {
        self.edit(Edit::SetRotation(rotation.to_wire()));
    }

    pub fn look_at(&self, target: Position) {
        self.edit(Edit::LookAt(target.to_wire()));
    }

    pub fn set_ticks_lived(&self, ticks: i32) {
        self.edit(Edit::SetTicksLived(ticks));
    }

    pub fn server(&self) -> Server {
        Server::new(self.link.clone())
    }

    pub async fn get_state(&self) -> Result<EntityState, Error> {
        fetch(&self.link, self.entity_id).await
    }

    pub(crate) const fn link(&self) -> &Link {
        &self.link
    }

    fn edit(&self, edit: Edit) {
        self.order(entity_order::Order::Edit(v1::EntityEdit {
            edit: Some(edit),
        }));
    }

    pub(crate) fn order(&self, order: entity_order::Order) {
        self.link.order(from_plugin::Body::Entity(v1::EntityOrder {
            entity: self.entity_id,
            order: Some(order),
        }));
    }
}

#[derive(Debug, Clone)]
pub struct EntityState {
    pub entity: Entity,
    pub health: Option<f32>,
    pub velocity: Vector,
    pub on_ground: bool,
    pub custom_name: Option<String>,
    pub fire_ticks: i32,
    pub custom_name_visible: bool,
    pub glowing: bool,
    pub invisible: bool,
    pub silent: bool,
    pub gravity: bool,
    pub invulnerable: bool,
    pub persistent: bool,
    pub freeze_ticks: i32,
    pub air: i32,
    pub fall_distance: f32,
    pub rotation: Rotation,
    pub ticks_lived: i32,
    pub tags: Vec<String>,
    pub passengers: Vec<i32>,
    pub vehicle: Option<i32>,
    pub living: Option<LivingState>,
}

impl EntityState {
    pub(crate) fn from_wire(state: v1::EntityState, link: &Link) -> Result<Self, Error> {
        let entity = state
            .entity
            .ok_or_else(|| Error::Malformed("a message without its entity".to_owned()))?;
        Ok(Self {
            entity: Entity::from_wire(entity, link)?,
            health: state.health,
            velocity: Vector::from_wire(state.velocity, "velocity")?,
            on_ground: state.on_ground,
            custom_name: state.custom_name,
            fire_ticks: state.fire_ticks,
            custom_name_visible: state.custom_name_visible,
            glowing: state.glowing,
            invisible: state.invisible,
            silent: state.silent,
            gravity: state.gravity,
            invulnerable: state.invulnerable,
            persistent: state.persistent,
            freeze_ticks: state.freeze_ticks,
            air: state.air,
            fall_distance: state.fall_distance,
            rotation: Rotation::from_wire(state.rotation)?,
            ticks_lived: state.ticks_lived,
            tags: state.tags,
            passengers: state.passengers,
            vehicle: state.vehicle,
            living: state.living.map(LivingState::from_wire).transpose()?,
        })
    }
}

#[derive(Debug, Clone)]
pub struct Entities {
    near: Option<(Position, f64)>,
    kind: Option<EntityType>,
    link: Link,
}

impl Entities {
    pub async fn with_meta(&self, key: &str) -> Result<Vec<Entity>, Error> {
        self.holding(key, false).await
    }

    pub async fn with_volatile_meta(&self, key: &str) -> Result<Vec<Entity>, Error> {
        self.holding(key, true).await
    }

    async fn holding(&self, key: &str, volatile: bool) -> Result<Vec<Entity>, Error> {
        let list = self
            .link
            .ask(
                question::Question::EntitiesWithMeta(v1::EntitiesWithMeta {
                    key: key.to_owned(),
                    volatile,
                }),
                "an entity list",
                |body| match body {
                    answer::Body::Entities(list) => Some(list),
                    _ => None,
                },
            )
            .await?;
        list.entities
            .into_iter()
            .map(|entity| Entity::from_wire(entity, &self.link))
            .collect()
    }
}

impl Entities {
    pub(crate) const fn new(link: Link) -> Self {
        Self {
            near: None,
            kind: None,
            link,
        }
    }

    pub fn near(&self, position: Position, radius: f64) -> Self {
        Self {
            near: Some((position, radius)),
            ..self.clone()
        }
    }

    pub fn of(&self, kind: EntityType) -> Self {
        Self {
            kind: Some(kind),
            ..self.clone()
        }
    }

    pub async fn get_entities(&self) -> Result<Vec<Entity>, Error> {
        let query = v1::EntityQuery {
            near: self.near.map(|(position, radius)| v1::Near {
                position: Some(position.to_wire()),
                radius,
            }),
            kind: self.kind.map(|kind| kind.key().to_owned()),
        };
        let list = self
            .link
            .ask(
                question::Question::Entities(query),
                "an entity list",
                |body| match body {
                    answer::Body::Entities(list) => Some(list),
                    _ => None,
                },
            )
            .await?;
        list.entities
            .into_iter()
            .map(|entity| Entity::from_wire(entity, &self.link))
            .collect()
    }
}
