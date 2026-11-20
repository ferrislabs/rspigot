use rspigot_plugin_protocol::v1::{self, answer, entity_order, from_plugin, question};
use uuid::Uuid;

use crate::attribute::EntityAttributesHandle;
use crate::link::Link;
use crate::meta::MetaHandle;
use crate::server::Server;
use crate::wire::parse_uuid;
use crate::{Effect, EffectType, EntityType, Error, Player, Position, Vector};

#[derive(Debug, Clone)]
pub struct Entity {
    pub entity_id: i32,
    pub uuid: Uuid,
    pub kind: EntityType,
    pub position: Position,
    pub attributes: EntityAttributesHandle,
    pub meta: MetaHandle,
    pub volatile_meta: MetaHandle,
    player: Option<Player>,
    link: Link,
}

impl Entity {
    pub(crate) fn from_wire(snapshot: v1::EntitySnapshot, link: &Link) -> Result<Self, Error> {
        let kind = EntityType::from_key(&snapshot.kind).ok_or_else(|| {
            Error::Malformed(format!("an unknown entity type `{}`", snapshot.kind))
        })?;
        Ok(Self {
            attributes: EntityAttributesHandle::new(snapshot.entity_id, link.clone()),
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

    pub fn player(&self) -> Option<&Player> {
        self.player.as_ref()
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

    pub fn server(&self) -> Server {
        Server::new(self.link.clone())
    }

    pub async fn get_state(&self) -> Result<EntityState, Error> {
        let state = self
            .link
            .ask(
                question::Question::Entity(self.entity_id),
                "an entity state",
                |body| match body {
                    answer::Body::Entity(state) => Some(state),
                    _ => None,
                },
            )
            .await?;
        EntityState::from_wire(state, &self.link)
    }

    pub(crate) const fn link(&self) -> &Link {
        &self.link
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
}

impl EntityState {
    fn from_wire(state: v1::EntityState, link: &Link) -> Result<Self, Error> {
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
