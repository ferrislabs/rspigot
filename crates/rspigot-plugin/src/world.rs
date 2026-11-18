use rspigot_plugin_protocol::v1::{self, answer, from_plugin, question, server_order, world_order};
use uuid::Uuid;

use crate::block::{Block, BlockPos, BlockState};
use crate::entity::{Entities, Entity};
use crate::link::Link;
use crate::npc::Npcs;
use crate::wire::parse_uuid;
use crate::{
    EntityType, Error, ItemStack, Position, WorldDifficultyHandle, WorldExplosionHandle,
    WorldGameRuleHandle, WorldParticleHandle, WorldPvpHandle, WorldSoundHandle,
    WorldSpawnPointHandle, WorldStorageHandle, WorldTimeHandle, WorldWeatherHandle,
};

const DEFAULT_WORLD: &str = "minecraft:overworld";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Dimension {
    Overworld,
    Nether,
    End,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layer {
    pub block: String,
    pub height: u32,
}

impl Layer {
    pub fn new(block: impl Into<String>, height: u32) -> Self {
        Self {
            block: block.into(),
            height,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Generator {
    Vanilla,
    Flat(Vec<Layer>),
    Void,
    Datapack(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorldOptions {
    pub name: String,
    pub dimension: Dimension,
    pub seed: i64,
    pub generator: Generator,
    pub structures: bool,
}

impl WorldOptions {
    fn into_wire(self) -> v1::CreateWorld {
        let dimension = match self.dimension {
            Dimension::Overworld => v1::WorldDimension::Overworld,
            Dimension::Nether => v1::WorldDimension::Nether,
            Dimension::End => v1::WorldDimension::End,
        };
        let kind = match self.generator {
            Generator::Vanilla => v1::world_generator::Kind::Vanilla(v1::Empty {}),
            Generator::Flat(layers) => v1::world_generator::Kind::Flat(v1::FlatLayers {
                layers: layers
                    .into_iter()
                    .map(|layer| v1::WorldLayer {
                        block: layer.block,
                        height: layer.height,
                    })
                    .collect(),
            }),
            Generator::Void => v1::world_generator::Kind::Void(v1::Empty {}),
            Generator::Datapack(id) => v1::world_generator::Kind::Datapack(id),
        };
        v1::CreateWorld {
            name: self.name,
            dimension: dimension as i32,
            seed: self.seed,
            generator: Some(v1::WorldGenerator { kind: Some(kind) }),
            structures: self.structures,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Weather {
    Clear,
    Rain,
    Thunder,
}

impl From<Weather> for v1::Weather {
    fn from(weather: Weather) -> Self {
        weather.to_wire()
    }
}

impl From<Difficulty> for v1::Difficulty {
    fn from(difficulty: Difficulty) -> Self {
        difficulty.to_wire()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GameRuleSetting {
    pub rule: GameRule,
    pub value: bool,
}

impl GameRuleSetting {
    pub const fn new(rule: GameRule, value: bool) -> Self {
        Self { rule, value }
    }
}

impl From<GameRuleSetting> for v1::SetGameRule {
    fn from(setting: GameRuleSetting) -> Self {
        Self {
            rule: setting.rule.to_wire() as i32,
            value: setting.value,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Explosion {
    pub position: Position,
    pub power: f32,
    pub breaks_blocks: bool,
}

impl Explosion {
    pub const fn new(position: Position, power: f32, breaks_blocks: bool) -> Self {
        Self {
            position,
            power,
            breaks_blocks,
        }
    }
}

impl From<Explosion> for v1::Explode {
    fn from(explosion: Explosion) -> Self {
        Self {
            position: Some(explosion.position.to_wire()),
            power: explosion.power,
            breaks_blocks: explosion.breaks_blocks,
        }
    }
}

impl From<BlockPos> for v1::BlockPos {
    fn from(pos: BlockPos) -> Self {
        pos.to_wire()
    }
}

impl Weather {
    const fn to_wire(self) -> v1::Weather {
        match self {
            Self::Clear => v1::Weather::Clear,
            Self::Rain => v1::Weather::Rain,
            Self::Thunder => v1::Weather::Thunder,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Difficulty {
    Peaceful,
    Easy,
    Normal,
    Hard,
}

impl Difficulty {
    fn from_wire(difficulty: i32) -> Result<Self, Error> {
        match v1::Difficulty::try_from(difficulty) {
            Ok(v1::Difficulty::Peaceful) => Ok(Self::Peaceful),
            Ok(v1::Difficulty::Easy) => Ok(Self::Easy),
            Ok(v1::Difficulty::Normal) => Ok(Self::Normal),
            Ok(v1::Difficulty::Hard) => Ok(Self::Hard),
            Ok(v1::Difficulty::Unspecified) | Err(_) => Err(Error::Malformed(format!(
                "an unknown difficulty {difficulty}"
            ))),
        }
    }

    const fn to_wire(self) -> v1::Difficulty {
        match self {
            Self::Peaceful => v1::Difficulty::Peaceful,
            Self::Easy => v1::Difficulty::Easy,
            Self::Normal => v1::Difficulty::Normal,
            Self::Hard => v1::Difficulty::Hard,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GameRule {
    DaylightCycle,
    NaturalRegeneration,
    KeepInventory,
    ShowDeathMessages,
    ImmediateRespawn,
    FallDamage,
    MobDrops,
    MobGriefing,
    MobExplosionDropDecay,
    BlockDrops,
    UniversalAnger,
}

impl GameRule {
    fn from_wire(rule: i32) -> Result<Self, Error> {
        match v1::GameRule::try_from(rule) {
            Ok(v1::GameRule::DaylightCycle) => Ok(Self::DaylightCycle),
            Ok(v1::GameRule::NaturalRegeneration) => Ok(Self::NaturalRegeneration),
            Ok(v1::GameRule::KeepInventory) => Ok(Self::KeepInventory),
            Ok(v1::GameRule::ShowDeathMessages) => Ok(Self::ShowDeathMessages),
            Ok(v1::GameRule::ImmediateRespawn) => Ok(Self::ImmediateRespawn),
            Ok(v1::GameRule::FallDamage) => Ok(Self::FallDamage),
            Ok(v1::GameRule::MobDrops) => Ok(Self::MobDrops),
            Ok(v1::GameRule::MobGriefing) => Ok(Self::MobGriefing),
            Ok(v1::GameRule::MobExplosionDropDecay) => Ok(Self::MobExplosionDropDecay),
            Ok(v1::GameRule::BlockDrops) => Ok(Self::BlockDrops),
            Ok(v1::GameRule::UniversalAnger) => Ok(Self::UniversalAnger),
            Ok(v1::GameRule::Unspecified) | Err(_) => {
                Err(Error::Malformed(format!("an unknown game rule {rule}")))
            }
        }
    }

    const fn to_wire(self) -> v1::GameRule {
        match self {
            Self::DaylightCycle => v1::GameRule::DaylightCycle,
            Self::NaturalRegeneration => v1::GameRule::NaturalRegeneration,
            Self::KeepInventory => v1::GameRule::KeepInventory,
            Self::ShowDeathMessages => v1::GameRule::ShowDeathMessages,
            Self::ImmediateRespawn => v1::GameRule::ImmediateRespawn,
            Self::FallDamage => v1::GameRule::FallDamage,
            Self::MobDrops => v1::GameRule::MobDrops,
            Self::MobGriefing => v1::GameRule::MobGriefing,
            Self::MobExplosionDropDecay => v1::GameRule::MobExplosionDropDecay,
            Self::BlockDrops => v1::GameRule::BlockDrops,
            Self::UniversalAnger => v1::GameRule::UniversalAnger,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct WorldState {
    pub name: String,
    pub uuid: Option<Uuid>,
    pub seed: i64,
    pub day_time: i64,
    pub game_time: i64,
    pub raining: bool,
    pub thundering: bool,
    pub difficulty: Difficulty,
    pub spawn: BlockPos,
    pub pvp: bool,
    pub rules: Vec<(GameRule, bool)>,
}

impl WorldState {
    fn from_wire(state: v1::WorldState) -> Result<Self, Error> {
        Ok(Self {
            uuid: state
                .uuid
                .as_deref()
                .map(|text| parse_uuid(text, "a world"))
                .transpose()?,
            seed: state.seed,
            day_time: state.day_time,
            game_time: state.game_time,
            raining: state.raining,
            thundering: state.thundering,
            difficulty: Difficulty::from_wire(state.difficulty)?,
            spawn: BlockPos::from_wire(state.spawn, "spawn")?,
            pvp: state.pvp,
            rules: state
                .rules
                .into_iter()
                .map(|rule| Ok((GameRule::from_wire(rule.rule)?, rule.value)))
                .collect::<Result<_, Error>>()?,
            name: state.name,
        })
    }
}

#[derive(Debug, Clone)]
pub struct World {
    pub entities: Entities,
    pub difficulty: WorldDifficultyHandle,
    pub explosion: WorldExplosionHandle,
    pub npcs: Npcs,
    pub game_rule: WorldGameRuleHandle,
    pub particle: WorldParticleHandle,
    pub pvp: WorldPvpHandle,
    pub sound: WorldSoundHandle,
    pub spawn_point: WorldSpawnPointHandle,
    pub storage: WorldStorageHandle,
    pub time: WorldTimeHandle,
    pub weather: WorldWeatherHandle,
    name: Option<String>,
    link: Link,
}

impl World {
    pub(crate) fn new(link: Link) -> Self {
        Self {
            entities: Entities::new(link.clone()),
            difficulty: WorldDifficultyHandle::new(link.clone()),
            explosion: WorldExplosionHandle::new(link.clone()),
            npcs: Npcs::new(link.clone()),
            game_rule: WorldGameRuleHandle::new(link.clone()),
            particle: WorldParticleHandle::new(link.clone()),
            pvp: WorldPvpHandle::new(link.clone()),
            sound: WorldSoundHandle::new(link.clone()),
            spawn_point: WorldSpawnPointHandle::new(link.clone()),
            storage: WorldStorageHandle::new(link.clone()),
            time: WorldTimeHandle::new(link.clone()),
            weather: WorldWeatherHandle::new(link.clone()),
            name: None,
            link,
        }
    }

    pub(crate) fn named(name: String, link: Link) -> Self {
        let link = link.in_world(&name);
        Self {
            name: Some(name),
            ..Self::new(link)
        }
    }

    pub(crate) fn resolved(name: String, link: &Link) -> Self {
        if name == DEFAULT_WORLD {
            Self::new(link.clone())
        } else {
            Self::named(name, link.clone())
        }
    }

    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    pub fn id(&self) -> &str {
        self.name.as_deref().unwrap_or(DEFAULT_WORLD)
    }

    pub async fn create(&self, options: WorldOptions) -> Result<World, Error> {
        let created = self
            .link
            .act_for(
                from_plugin::Body::Server(v1::ServerOrder {
                    order: Some(server_order::Order::CreateWorld(options.into_wire())),
                }),
                "a created world",
                |body| match body {
                    answer::Body::WorldCreated(id) => Some(id),
                    _ => None,
                },
            )
            .await?;
        Ok(World::named(created, self.link.clone()))
    }

    pub async fn delete(&self, files: bool) -> Result<(), Error> {
        self.link
            .act(from_plugin::Body::World(v1::WorldOrder {
                order: Some(world_order::Order::DeleteWorld(v1::DeleteWorld { files })),
            }))
            .await
    }

    pub fn block(&self, pos: BlockPos) -> Block {
        Block::new(pos, self.link.clone())
    }

    pub async fn get_block(&self, pos: BlockPos) -> Result<BlockState, Error> {
        self.block(pos).get_state().await
    }

    pub async fn spawn(&self, kind: EntityType, position: Position) -> Result<Entity, Error> {
        self.created(question::Question::Spawn(v1::Spawn {
            kind: kind.key().to_owned(),
            position: Some(position.to_wire()),
        }))
        .await
    }

    pub async fn drop_item(&self, position: Position, item: ItemStack) -> Result<Entity, Error> {
        self.created(question::Question::DropItem(v1::DropItem {
            position: Some(position.to_wire()),
            item: Some(item.into_wire()),
        }))
        .await
    }

    pub async fn get_state(&self) -> Result<WorldState, Error> {
        let state = self
            .link
            .ask(
                question::Question::World(v1::Empty {}),
                "a world state",
                |body| match body {
                    answer::Body::World(state) => Some(state),
                    _ => None,
                },
            )
            .await?;
        WorldState::from_wire(state)
    }

    async fn created(&self, question: question::Question) -> Result<Entity, Error> {
        let snapshot = self
            .link
            .ask(question, "a created entity", |body| match body {
                answer::Body::Spawned(snapshot) => Some(snapshot),
                _ => None,
            })
            .await?;
        Entity::from_wire(snapshot, &self.link)
    }
}
