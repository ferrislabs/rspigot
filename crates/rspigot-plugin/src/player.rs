use std::net::IpAddr;
use std::time::SystemTime;

use rspigot_plugin_protocol::v1::player_order::Order;
use rspigot_plugin_protocol::v1::{self, answer, from_plugin, question};
use uuid::Uuid;

use crate::contract::Location;
use crate::custom_inventory::CustomInventory;
use crate::inventory::Inventory;
use crate::link::Link;
use crate::server::Server;
use crate::target::{BlockTarget, Target};
use crate::wire::{millis, parse_address, parse_uuid};
use crate::world::World;
use crate::{Effect, EffectType, Error, Particle, Sound, Title};

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
        }
    }

    pub fn send_message(&self, text: impl Into<String>) {
        self.order(Order::SendMessage(text.into()));
    }

    pub fn send_action_bar(&self, text: impl Into<String>) {
        self.order(Order::SendActionBar(text.into()));
    }

    pub fn send_title(&self, title: Title) {
        self.order(Order::SendTitle(title.into_wire()));
    }

    pub fn reset_title(&self) {
        self.order(Order::ResetTitle(v1::Empty {}));
    }

    pub fn set_tab_list(&self, header: impl Into<String>, footer: impl Into<String>) {
        self.order(Order::SetTabList(v1::TabList {
            header: header.into(),
            footer: footer.into(),
        }));
    }

    pub fn teleport(&self, position: Position) {
        self.order(Order::Teleport(position.to_wire()));
    }

    pub fn teleport_facing(&self, position: Position, rotation: Rotation) {
        self.order(Order::TeleportFacing(v1::Facing {
            position: Some(position.to_wire()),
            rotation: Some(rotation.to_wire()),
        }));
    }

    pub fn teleport_to(&self, location: &Location) {
        self.order(Order::TeleportTo(v1::Location {
            world: location.world.clone(),
            position: Some(location.position.to_wire()),
            rotation: Some(location.rotation.to_wire()),
        }));
    }

    pub fn set_game_mode(&self, mode: GameMode) {
        self.order(Order::SetGameMode(mode.to_wire() as i32));
    }

    pub fn kick(&self, reason: impl Into<String>) {
        self.order(Order::Kick(reason.into()));
    }

    pub fn set_health(&self, health: f32) {
        self.order(Order::SetHealth(health));
    }

    pub fn set_absorption(&self, amount: f32) {
        self.order(Order::SetAbsorption(amount));
    }

    pub fn damage(&self, amount: f64) {
        self.order(Order::Damage(amount));
    }

    pub fn kill(&self) {
        self.order(Order::Kill(v1::Empty {}));
    }

    pub fn set_food_level(&self, food_level: i32) {
        self.order(Order::SetFoodLevel(food_level));
    }

    pub fn set_saturation(&self, saturation: f32) {
        self.order(Order::SetSaturation(saturation));
    }

    pub fn set_exhaustion(&self, exhaustion: f32) {
        self.order(Order::SetExhaustion(exhaustion));
    }

    pub fn give_exp(&self, points: i32) {
        self.order(Order::GiveExp(points));
    }

    pub fn give_levels(&self, levels: i32) {
        self.order(Order::GiveLevels(levels));
    }

    pub fn set_level(&self, level: i32) {
        self.order(Order::SetLevel(level));
    }

    pub fn set_exp(&self, progress: f32) {
        self.order(Order::SetExp(progress));
    }

    pub fn add_effect(&self, effect: Effect) {
        self.order(Order::AddEffect(effect.to_wire()));
    }

    pub fn remove_effect(&self, kind: EffectType) {
        self.order(Order::RemoveEffect(kind.key().to_owned()));
    }

    pub fn clear_effects(&self) {
        self.order(Order::ClearEffects(v1::Empty {}));
    }

    pub fn set_fire_ticks(&self, ticks: i32) {
        self.order(Order::SetFireTicks(ticks));
    }

    pub fn set_velocity(&self, velocity: Vector) {
        self.order(Order::SetVelocity(velocity.to_wire()));
    }

    pub fn set_allow_flight(&self, allowed: bool) {
        self.order(Order::SetAllowFlight(allowed));
    }

    pub fn set_flying(&self, flying: bool) {
        self.order(Order::SetFlying(flying));
    }

    pub fn set_fly_speed(&self, speed: f32) {
        self.order(Order::SetFlySpeed(speed));
    }

    pub fn set_walk_speed(&self, speed: f32) {
        self.order(Order::SetWalkSpeed(speed));
    }

    pub fn set_op(&self, operator: bool) {
        self.order(Order::SetOp(operator));
    }

    pub fn respawn(&self) {
        self.order(Order::Respawn(v1::Empty {}));
    }

    pub fn perform_command(&self, line: impl Into<String>) {
        self.order(Order::PerformCommand(line.into()));
    }

    pub fn chat(&self, text: impl Into<String>) {
        self.order(Order::Chat(text.into()));
    }

    pub fn play_sound(&self, sound: Sound, volume: f32, pitch: f32) {
        self.order(Order::PlaySound(v1::Sound {
            key: sound.key().to_owned(),
            volume,
            pitch,
            position: None,
        }));
    }

    pub fn spawn_particle(
        &self,
        particle: Particle,
        position: Position,
        count: i32,
        spread: Vector,
        speed: f64,
    ) {
        self.order(Order::SpawnParticle(
            particle.to_wire(position, count, spread, speed),
        ));
    }

    pub fn hide_player(&self, target: &Player) {
        self.order(Order::HidePlayer(target.uuid.to_string()));
    }

    pub fn show_player(&self, target: &Player) {
        self.order(Order::ShowPlayer(target.uuid.to_string()));
    }

    pub fn open_inventory(&self, inventory: &CustomInventory) {
        self.link.order_from(
            inventory.generation(),
            self.body(Order::OpenInventory(inventory.id())),
        );
    }

    pub fn close_inventory(&self) {
        self.order(Order::CloseInventory(v1::Empty {}));
    }

    pub fn update_inventory(&self) {
        self.order(Order::UpdateInventory(v1::Empty {}));
    }

    pub fn ban(&self, reason: Option<String>, expires: Option<SystemTime>) {
        self.order(Order::Ban(v1::BanTerms {
            reason,
            expires_ms: expires.map(millis),
        }));
    }

    pub fn inventory(&self) -> Inventory {
        Inventory::new(self.uuid, self.link.clone())
    }

    pub async fn resolve(&self) -> Result<PlayerState, Error> {
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

    fn order(&self, order: Order) {
        self.link.order(self.body(order));
    }

    fn body(&self, order: Order) -> from_plugin::Body {
        from_plugin::Body::Player(v1::PlayerOrder {
            player: self.uuid.to_string(),
            order: Some(order),
        })
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
