use rspigot_plugin_protocol::v1::{self, answer, question};
use std::net::IpAddr;
use uuid::Uuid;

use crate::link::Link;
use crate::server::Server;
use crate::target::{BlockTarget, Target};
use crate::wire::{parse_address, parse_uuid};
use crate::world::World;
use crate::{
    Effect, Error, PlayerAbilitiesHandle, PlayerActionBarHandle, PlayerCommandHandle,
    PlayerConnectionHandle, PlayerEffectsHandle, PlayerExperienceHandle, PlayerFoodHandle,
    PlayerGameModeHandle, PlayerHealthHandle, PlayerInventoryHandle, PlayerLocationHandle,
    PlayerMessageHandle, PlayerMovementHandle, PlayerOperatorHandle, PlayerParticleHandle,
    PlayerPermissionsHandle, PlayerScoreboardHandle, PlayerSoundHandle, PlayerTabListHandle,
    PlayerTitleHandle, PlayerVisibilityHandle,
};

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Position {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Vector {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rotation {
    pub yaw: f32,
    pub pitch: f32,
}

impl Rotation {
    pub(crate) fn from_wire(rotation: Option<v1::Rotation>) -> Result<Self, Error> {
        let rotation =
            rotation.ok_or_else(|| Error::Malformed("a message without `rotation`".to_owned()))?;
        Ok(Self {
            yaw: rotation.yaw,
            pitch: rotation.pitch,
        })
    }

    pub(crate) const fn to_wire(self) -> v1::Rotation {
        v1::Rotation {
            yaw: self.yaw,
            pitch: self.pitch,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GameMode {
    Survival,
    Creative,
    Adventure,
    Spectator,
}

impl GameMode {
    pub(crate) fn from_wire(mode: i32) -> Result<Self, Error> {
        match v1::GameMode::try_from(mode) {
            Ok(v1::GameMode::Survival) => Ok(Self::Survival),
            Ok(v1::GameMode::Creative) => Ok(Self::Creative),
            Ok(v1::GameMode::Adventure) => Ok(Self::Adventure),
            Ok(v1::GameMode::Spectator) => Ok(Self::Spectator),
            Ok(v1::GameMode::Unspecified) | Err(_) => {
                Err(Error::Malformed(format!("an unknown game mode {mode}")))
            }
        }
    }

    pub(crate) const fn to_wire(self) -> v1::GameMode {
        match self {
            Self::Survival => v1::GameMode::Survival,
            Self::Creative => v1::GameMode::Creative,
            Self::Adventure => v1::GameMode::Adventure,
            Self::Spectator => v1::GameMode::Spectator,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Player {
    pub uuid: Uuid,
    pub name: String,
    pub entity_id: i32,
    pub world: String,
    pub position: Position,
    pub rotation: Rotation,
    pub game_mode: GameMode,
    permissions: Vec<String>,
    link: Link,
}

impl Player {
    pub(crate) fn from_wire(
        snapshot: Option<v1::PlayerSnapshot>,
        link: &Link,
    ) -> Result<Self, Error> {
        let snapshot =
            snapshot.ok_or_else(|| Error::Malformed("a message without its player".to_owned()))?;
        Self::from_snapshot(snapshot, link)
    }

    pub(crate) fn from_snapshot(snapshot: v1::PlayerSnapshot, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            uuid: parse_uuid(&snapshot.uuid, "a player")?,
            game_mode: GameMode::from_wire(snapshot.game_mode)?,
            name: snapshot.name,
            entity_id: snapshot.entity_id,
            world: snapshot.world,
            position: Position::from_wire(snapshot.position, "position")?,
            rotation: Rotation::from_wire(snapshot.rotation)?,
            permissions: snapshot.permissions,
            link: link.clone(),
        })
    }

    pub(crate) fn to_snapshot(&self) -> v1::PlayerSnapshot {
        v1::PlayerSnapshot {
            uuid: self.uuid.hyphenated().to_string(),
            name: self.name.clone(),
            entity_id: self.entity_id,
            world: self.world.clone(),
            position: Some(self.position.to_wire()),
            rotation: Some(self.rotation.to_wire()),
            game_mode: self.game_mode.to_wire() as i32,
            permissions: self.permissions.clone(),
        }
    }

    pub fn permissions(&self) -> PlayerPermissionsHandle<'_> {
        PlayerPermissionsHandle::new(&self.permissions)
    }

    pub fn inventory(&self) -> PlayerInventoryHandle {
        PlayerInventoryHandle::new(self.uuid.to_string(), self.link.clone())
    }

    pub fn health(&self) -> PlayerHealthHandle {
        PlayerHealthHandle::new(self.uuid.to_string(), self.link.clone())
    }

    pub fn food(&self) -> PlayerFoodHandle {
        PlayerFoodHandle::new(self.uuid.to_string(), self.link.clone())
    }

    pub fn experience(&self) -> PlayerExperienceHandle {
        PlayerExperienceHandle::new(self.uuid.to_string(), self.link.clone())
    }

    pub fn effects(&self) -> PlayerEffectsHandle {
        PlayerEffectsHandle::new(self.uuid.to_string(), self.link.clone())
    }

    pub fn abilities(&self) -> PlayerAbilitiesHandle {
        PlayerAbilitiesHandle::new(self.uuid.to_string(), self.link.clone())
    }

    pub fn movement(&self) -> PlayerMovementHandle {
        PlayerMovementHandle::new(self.uuid.to_string(), self.link.clone())
    }

    pub fn operator(&self) -> PlayerOperatorHandle {
        PlayerOperatorHandle::new(self.uuid.to_string(), self.link.clone())
    }

    pub fn connection(&self) -> PlayerConnectionHandle {
        PlayerConnectionHandle::new(self.uuid.to_string(), self.link.clone())
    }

    pub fn sound(&self) -> PlayerSoundHandle {
        PlayerSoundHandle::new(self.uuid.to_string(), self.link.clone())
    }

    pub fn particle(&self) -> PlayerParticleHandle {
        PlayerParticleHandle::new(self.uuid.to_string(), self.link.clone())
    }

    pub fn message(&self) -> PlayerMessageHandle {
        PlayerMessageHandle::new(self.uuid.to_string(), self.link.clone())
    }

    pub fn title(&self) -> PlayerTitleHandle {
        PlayerTitleHandle::new(self.uuid.to_string(), self.link.clone())
    }

    pub fn action_bar(&self) -> PlayerActionBarHandle {
        PlayerActionBarHandle::new(self.uuid.to_string(), self.link.clone())
    }

    pub fn tab_list(&self) -> PlayerTabListHandle {
        PlayerTabListHandle::new(self.uuid.to_string(), self.link.clone())
    }

    pub fn location(&self) -> PlayerLocationHandle {
        PlayerLocationHandle::new(self.uuid.to_string(), self.link.clone())
    }

    pub fn game_mode(&self) -> PlayerGameModeHandle {
        PlayerGameModeHandle::new(self.uuid.to_string(), self.link.clone())
    }

    pub fn command(&self) -> PlayerCommandHandle {
        PlayerCommandHandle::new(self.uuid.to_string(), self.link.clone())
    }

    pub fn visibility(&self) -> PlayerVisibilityHandle {
        PlayerVisibilityHandle::new(self.uuid.to_string(), self.link.clone())
    }

    pub fn scoreboard(&self) -> PlayerScoreboardHandle {
        PlayerScoreboardHandle::new(self.uuid.to_string(), self.link.clone())
    }

    pub async fn get_position(&self) -> Result<Position, Error> {
        Ok(self.get_state().await?.player.position)
    }

    pub async fn get_state(&self) -> Result<PlayerState, Error> {
        let state = self
            .link
            .ask(
                question::Question::Player(self.uuid.to_string()),
                "a player state",
                |body| match body {
                    answer::Body::Player(state) => Some(state),
                    _ => None,
                },
            )
            .await?;
        PlayerState::from_wire(state, &self.link)
    }

    pub async fn target_block(&self, max_distance: f64) -> Result<Option<BlockTarget>, Error> {
        let hit = self
            .aim(question::Question::TargetBlock, max_distance)
            .await?;
        BlockTarget::from_wire(hit, &self.link)
    }

    pub async fn target_entity(&self, max_distance: f64) -> Result<Option<Target>, Error> {
        let hit = self
            .aim(question::Question::TargetEntity, max_distance)
            .await?;
        Target::from_wire(hit, &self.link)
    }

    pub fn server(&self) -> Server {
        Server::new(self.link.clone())
    }

    pub fn world(&self) -> World {
        World::new(self.link.clone())
    }

    async fn aim(
        &self,
        question: fn(v1::TargetQuery) -> question::Question,
        max_distance: f64,
    ) -> Result<Option<v1::target::Hit>, Error> {
        let query = v1::TargetQuery {
            player: self.uuid.to_string(),
            max_distance,
        };
        self.link
            .ask(question(query), "a target", |body| match body {
                answer::Body::Target(target) => Some(target.hit),
                _ => None,
            })
            .await
    }
}

#[derive(Debug, Clone)]
pub struct PlayerState {
    pub player: Player,
    pub health: f32,
    pub max_health: f32,
    pub absorption: f32,
    pub food_level: i32,
    pub saturation: f32,
    pub exhaustion: f32,
    pub level: i32,
    pub exp: f32,
    pub total_experience: i32,
    pub op: bool,
    pub allow_flight: bool,
    pub flying: bool,
    pub fly_speed: f32,
    pub walk_speed: f32,
    pub sneaking: bool,
    pub sprinting: bool,
    pub on_ground: bool,
    pub fire_ticks: i32,
    pub dead: bool,
    pub effects: Vec<Effect>,
    pub ping: i32,
    pub locale: String,
    pub address: IpAddr,
    pub hidden: Vec<Uuid>,
}

impl PlayerState {
    fn from_wire(state: v1::PlayerState, link: &Link) -> Result<Self, Error> {
        let address = parse_address(&state.address)?;
        let hidden = state
            .hidden
            .iter()
            .map(|hidden| parse_uuid(hidden, "a player"))
            .collect::<Result<_, Error>>()?;
        Ok(Self {
            player: Player::from_wire(state.player, link)?,
            health: state.health,
            max_health: state.max_health,
            absorption: state.absorption,
            food_level: state.food,
            saturation: state.saturation,
            exhaustion: state.exhaustion,
            level: state.level,
            exp: state.exp,
            total_experience: state.total_exp,
            op: state.op,
            allow_flight: state.allow_flight,
            flying: state.flying,
            fly_speed: state.fly_speed,
            walk_speed: state.walk_speed,
            sneaking: state.sneaking,
            sprinting: state.sprinting,
            on_ground: state.on_ground,
            fire_ticks: state.fire_ticks,
            dead: state.dead,
            effects: state
                .effects
                .into_iter()
                .map(Effect::from_wire)
                .collect::<Result<_, Error>>()?,
            ping: state.ping,
            locale: state.locale,
            address,
            hidden,
        })
    }
}
