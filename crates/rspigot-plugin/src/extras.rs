use std::sync::Arc;
use std::time::Duration;

use rspigot_plugin_protocol::v1::{
    self, answer, chunk_order, extra_answer, extra_question, from_plugin, player_extra, question,
    world_border_order, world_order,
};

use crate::link::Link;
use crate::{
    BlockPos, BlockType, EntityType, Error, ItemStack, Material, PlayerSoundHandle, Position,
    Sound, Weather,
};

const TICKS_PER_SECOND: u128 = 20;

fn ticks_of(duration: Duration) -> i64 {
    i64::try_from(duration.as_millis() * TICKS_PER_SECOND / 1000).unwrap_or(i64::MAX)
}

fn weather_to_wire(weather: Weather) -> v1::Weather {
    match weather {
        Weather::Clear => v1::Weather::Clear,
        Weather::Rain => v1::Weather::Rain,
        Weather::Thunder => v1::Weather::Thunder,
    }
}

fn weather_from_wire(value: i32) -> Result<Weather, Error> {
    match v1::Weather::try_from(value) {
        Ok(v1::Weather::Clear) => Ok(Weather::Clear),
        Ok(v1::Weather::Rain) => Ok(Weather::Rain),
        Ok(v1::Weather::Thunder) => Ok(Weather::Thunder),
        Ok(v1::Weather::Unspecified) | Err(_) => {
            Err(Error::Malformed(format!("an unknown weather {value}")))
        }
    }
}

fn expect_extra<T>(
    answer: v1::ExtraAnswer,
    pick: impl FnOnce(extra_answer::Answer) -> Option<T>,
) -> Result<T, Error> {
    answer
        .answer
        .and_then(pick)
        .ok_or_else(|| Error::Malformed("an answer that is not the one asked for".to_owned()))
}

async fn ask_extra(link: &Link, query: extra_question::Query) -> Result<v1::ExtraAnswer, Error> {
    link.ask(
        question::Question::Extra(v1::ExtraQuestion { query: Some(query) }),
        "an extra answer",
        |body| match body {
            answer::Body::Extra(extra) => Some(extra),
            _ => None,
        },
    )
    .await
}

fn world_order(link: &Link, order: world_order::Order) {
    link.order(from_plugin::Body::World(v1::WorldOrder {
        order: Some(order),
    }));
}

#[derive(Debug, Clone)]
pub struct WorldBorderHandle {
    link: Link,
}

impl WorldBorderHandle {
    pub(crate) fn new(link: Link) -> Self {
        Self { link }
    }

    fn change(&self, change: world_border_order::Change) {
        world_order(
            &self.link,
            world_order::Order::Border(v1::WorldBorderOrder {
                change: Some(change),
            }),
        );
    }

    pub fn set_size(&self, size: f64) {
        self.resize(size, Duration::ZERO);
    }

    pub fn resize(&self, size: f64, over: Duration) {
        self.change(world_border_order::Change::Resize(v1::BorderResize {
            size,
            ticks: ticks_of(over),
        }));
    }

    pub fn set_center(&self, x: f64, z: f64) {
        self.change(world_border_order::Change::SetCenter(v1::BorderCenter {
            x,
            z,
        }));
    }

    pub fn set_damage_amount(&self, damage: f64) {
        self.change(world_border_order::Change::SetDamageAmount(damage));
    }

    pub fn set_damage_buffer(&self, distance: f64) {
        self.change(world_border_order::Change::SetDamageBuffer(distance));
    }

    pub fn set_warning_distance(&self, blocks: i32) {
        self.change(world_border_order::Change::SetWarningDistance(blocks));
    }

    pub fn set_warning_time(&self, time: Duration) {
        let ticks = i32::try_from(ticks_of(time)).unwrap_or(i32::MAX);
        self.change(world_border_order::Change::SetWarningTimeTicks(ticks));
    }
}

#[derive(Debug, Clone)]
pub struct WorldLightningHandle {
    link: Link,
}

impl WorldLightningHandle {
    pub(crate) fn new(link: Link) -> Self {
        Self { link }
    }

    pub fn strike(&self, position: Position) {
        self.send(position, false);
    }

    pub fn strike_effect(&self, position: Position) {
        self.send(position, true);
    }

    fn send(&self, position: Position, effect_only: bool) {
        world_order(
            &self.link,
            world_order::Order::StrikeLightning(v1::StrikeLightning {
                position: Some(position.to_wire()),
                effect_only,
            }),
        );
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChunkState {
    pub x: i32,
    pub z: i32,
    pub loaded: bool,
    pub force_loaded: bool,
}

#[derive(Debug, Clone)]
pub struct WorldChunksHandle {
    link: Link,
}

impl WorldChunksHandle {
    pub(crate) fn new(link: Link) -> Self {
        Self { link }
    }

    pub fn at(&self, x: i32, z: i32) -> Chunk {
        Chunk {
            x,
            z,
            link: self.link.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Chunk {
    pub x: i32,
    pub z: i32,
    link: Link,
}

impl Chunk {
    fn order(&self, order: chunk_order::Order) {
        world_order(
            &self.link,
            world_order::Order::Chunk(v1::ChunkOrder { order: Some(order) }),
        );
    }

    pub fn load(&self) {
        self.order(chunk_order::Order::Load(v1::ChunkCoords {
            x: self.x,
            z: self.z,
        }));
    }

    pub fn set_force_loaded(&self, forced: bool) {
        self.order(chunk_order::Order::SetForceLoaded(v1::ChunkForce {
            x: self.x,
            z: self.z,
            forced,
        }));
    }

    pub fn regenerate(&self) {
        self.order(chunk_order::Order::Regenerate(v1::ChunkCoords {
            x: self.x,
            z: self.z,
        }));
    }

    pub async fn get_state(&self, load: bool) -> Result<ChunkState, Error> {
        let answer = ask_extra(
            &self.link,
            extra_question::Query::Chunk(v1::ChunkQuery {
                x: self.x,
                z: self.z,
                load,
            }),
        )
        .await?;
        expect_extra(answer, |answer| match answer {
            extra_answer::Answer::Chunk(info) => Some(ChunkState {
                x: info.x,
                z: info.z,
                loaded: info.loaded,
                force_loaded: info.force_loaded,
            }),
            _ => None,
        })
    }

    pub async fn is_loaded(&self) -> Result<bool, Error> {
        Ok(self.get_state(false).await?.loaded)
    }
}

fn player_order(target: &Arc<str>, link: &Link, order: player_extra::Order) {
    link.order(from_plugin::Body::Player(v1::PlayerOrder {
        player: target.to_string(),
        order: Some(v1::player_order::Order::Extra(v1::PlayerExtra {
            order: Some(order),
        })),
    }));
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlayerTime {
    pub time: i64,
    pub relative: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlayerClock {
    pub time: Option<PlayerTime>,
    pub weather: Option<Weather>,
}

#[derive(Debug, Clone)]
pub struct PlayerClockHandle {
    target: Arc<str>,
    link: Link,
}

impl PlayerClockHandle {
    pub(crate) fn new(target: Arc<str>, link: Link) -> Self {
        Self { target, link }
    }

    fn order(&self, order: player_extra::Order) {
        player_order(&self.target, &self.link, order);
    }

    pub fn set_time(&self, time: i64) {
        self.order(player_extra::Order::SetPlayerTime(v1::PlayerTime {
            time,
            relative: false,
        }));
    }

    pub fn set_relative_time(&self, offset: i64) {
        self.order(player_extra::Order::SetPlayerTime(v1::PlayerTime {
            time: offset,
            relative: true,
        }));
    }

    pub fn reset_time(&self) {
        self.order(player_extra::Order::ResetPlayerTime(v1::Empty {}));
    }

    pub fn set_weather(&self, weather: Weather) {
        self.order(player_extra::Order::SetPlayerWeather(
            weather_to_wire(weather) as i32,
        ));
    }

    pub fn reset_weather(&self) {
        self.order(player_extra::Order::ResetPlayerWeather(v1::Empty {}));
    }

    pub async fn get_state(&self) -> Result<PlayerClock, Error> {
        let answer = ask_extra(
            &self.link,
            extra_question::Query::PlayerClock(self.target.to_string()),
        )
        .await?;
        let clock = expect_extra(answer, |answer| match answer {
            extra_answer::Answer::Clock(clock) => Some(clock),
            _ => None,
        })?;
        Ok(PlayerClock {
            time: clock.time.map(|time| PlayerTime {
                time: time.time,
                relative: time.relative,
            }),
            weather: clock.weather.map(weather_from_wire).transpose()?,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SoundCategory {
    Master,
    Music,
    Records,
    Weather,
    Blocks,
    Hostile,
    Neutral,
    Players,
    Ambient,
    Voice,
    Ui,
}

impl SoundCategory {
    const fn wire(self) -> &'static str {
        match self {
            Self::Master => "master",
            Self::Music => "music",
            Self::Records => "record",
            Self::Weather => "weather",
            Self::Blocks => "block",
            Self::Hostile => "hostile",
            Self::Neutral => "neutral",
            Self::Players => "player",
            Self::Ambient => "ambient",
            Self::Voice => "voice",
            Self::Ui => "ui",
        }
    }
}

impl PlayerSoundHandle {
    fn stop_order(&self, sound: Option<Sound>, category: Option<SoundCategory>) {
        player_order(
            &self.target,
            &self.link,
            player_extra::Order::StopSound(v1::StopSound {
                sound: sound.map(|sound| sound.key().to_owned()),
                category: category.map(|category| category.wire().to_owned()),
            }),
        );
    }

    pub fn stop_all(&self) {
        self.stop_order(None, None);
    }

    pub fn stop(&self, sound: Sound) {
        self.stop_order(Some(sound), None);
    }

    pub fn stop_category(&self, category: SoundCategory) {
        self.stop_order(None, Some(category));
    }

    pub fn stop_in(&self, category: SoundCategory, sound: Sound) {
        self.stop_order(Some(sound), Some(category));
    }
}

#[derive(Debug, Clone)]
pub struct PlayerCooldownHandle {
    target: Arc<str>,
    link: Link,
}

impl PlayerCooldownHandle {
    pub(crate) fn new(target: Arc<str>, link: Link) -> Self {
        Self { target, link }
    }

    pub fn set(&self, item: Material, ticks: u32) {
        player_order(
            &self.target,
            &self.link,
            player_extra::Order::SetCooldown(v1::CooldownSet {
                item: item.key().to_owned(),
                ticks: i32::try_from(ticks).unwrap_or(i32::MAX),
            }),
        );
    }

    pub fn clear(&self, item: Material) {
        self.set(item, 0);
    }

    pub async fn get(&self, item: Material) -> Result<u32, Error> {
        let answer = ask_extra(
            &self.link,
            extra_question::Query::Cooldown(v1::CooldownQuery {
                player: self.target.to_string(),
                item: item.key().to_owned(),
            }),
        )
        .await?;
        let ticks = expect_extra(answer, |answer| match answer {
            extra_answer::Answer::Count(count) => Some(count),
            _ => None,
        })?;
        u32::try_from(ticks).map_err(|_| Error::Malformed(format!("a cooldown of {ticks} ticks")))
    }

    pub async fn has(&self, item: Material) -> Result<bool, Error> {
        Ok(self.get(item).await? > 0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatisticKind {
    kind: &'static str,
    key: String,
}

impl StatisticKind {
    fn of(kind: &'static str, key: impl Into<String>) -> Self {
        Self {
            kind,
            key: key.into(),
        }
    }

    pub fn mined(block: BlockType) -> Self {
        Self::of("minecraft:mined", block.key())
    }

    pub fn crafted(item: Material) -> Self {
        Self::of("minecraft:crafted", item.key())
    }

    pub fn used(item: Material) -> Self {
        Self::of("minecraft:used", item.key())
    }

    pub fn broken(item: Material) -> Self {
        Self::of("minecraft:broken", item.key())
    }

    pub fn picked_up(item: Material) -> Self {
        Self::of("minecraft:picked_up", item.key())
    }

    pub fn dropped(item: Material) -> Self {
        Self::of("minecraft:dropped", item.key())
    }

    pub fn killed(entity: EntityType) -> Self {
        Self::of("minecraft:killed", entity.key())
    }

    pub fn killed_by(entity: EntityType) -> Self {
        Self::of("minecraft:killed_by", entity.key())
    }

    pub fn custom(key: impl Into<String>) -> Self {
        Self::of("minecraft:custom", key)
    }
}

#[derive(Debug, Clone)]
pub struct PlayerStatisticsHandle {
    target: Arc<str>,
    link: Link,
}

impl PlayerStatisticsHandle {
    pub(crate) fn new(target: Arc<str>, link: Link) -> Self {
        Self { target, link }
    }

    fn change(&self, statistic: &StatisticKind, amount: u32, mode: v1::StatisticMode) {
        player_order(
            &self.target,
            &self.link,
            player_extra::Order::Statistic(v1::StatisticChange {
                kind: statistic.kind.to_owned(),
                key: statistic.key.clone(),
                amount: i32::try_from(amount).unwrap_or(i32::MAX),
                mode: mode as i32,
            }),
        );
    }

    pub fn increment(&self, statistic: &StatisticKind, amount: u32) {
        self.change(statistic, amount, v1::StatisticMode::Increment);
    }

    pub fn decrement(&self, statistic: &StatisticKind, amount: u32) {
        self.change(statistic, amount, v1::StatisticMode::Decrement);
    }

    pub fn set(&self, statistic: &StatisticKind, value: u32) {
        self.change(statistic, value, v1::StatisticMode::Set);
    }

    pub async fn get(&self, statistic: &StatisticKind) -> Result<u32, Error> {
        let answer = ask_extra(
            &self.link,
            extra_question::Query::Statistic(v1::StatisticQuery {
                player: self.target.to_string(),
                kind: statistic.kind.to_owned(),
                key: statistic.key.clone(),
            }),
        )
        .await?;
        let value = expect_extra(answer, |answer| match answer {
            extra_answer::Answer::Count(count) => Some(count),
            _ => None,
        })?;
        u32::try_from(value).map_err(|_| Error::Malformed(format!("a statistic of {value}")))
    }
}

#[derive(Debug, Clone)]
pub struct PlayerCompassHandle {
    target: Arc<str>,
    link: Link,
}

impl PlayerCompassHandle {
    pub(crate) fn new(target: Arc<str>, link: Link) -> Self {
        Self { target, link }
    }

    pub fn set_target(&self, position: Position) {
        player_order(
            &self.target,
            &self.link,
            player_extra::Order::SetCompassTarget(position.to_wire()),
        );
    }

    pub async fn get_target(&self) -> Result<Position, Error> {
        let answer = ask_extra(
            &self.link,
            extra_question::Query::CompassTarget(self.target.to_string()),
        )
        .await?;
        let target = expect_extra(answer, |answer| match answer {
            extra_answer::Answer::CompassTarget(target) => Some(target),
            _ => None,
        })?;
        Position::from_wire(Some(target), "compass target")
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct BedSpawn {
    pub pos: BlockPos,
    pub world: Option<String>,
    pub yaw: f32,
    pub force: bool,
}

impl BedSpawn {
    pub fn new(pos: BlockPos) -> Self {
        Self {
            pos,
            world: None,
            yaw: 0.0,
            force: false,
        }
    }

    pub fn world(mut self, world: impl Into<String>) -> Self {
        self.world = Some(world.into());
        self
    }

    pub fn yaw(mut self, yaw: f32) -> Self {
        self.yaw = yaw;
        self
    }

    pub fn force(mut self, force: bool) -> Self {
        self.force = force;
        self
    }

    fn into_wire(self) -> v1::BedSpawnSet {
        v1::BedSpawnSet {
            pos: Some(self.pos.to_wire()),
            world: self.world.unwrap_or_default(),
            yaw: self.yaw,
            force: self.force,
        }
    }

    fn from_wire(spawn: v1::BedSpawnSet) -> Result<Self, Error> {
        Ok(Self {
            pos: BlockPos::from_wire(spawn.pos, "bed spawn")?,
            world: Some(spawn.world),
            yaw: spawn.yaw,
            force: spawn.force,
        })
    }
}

#[derive(Debug, Clone)]
pub struct PlayerBedHandle {
    target: Arc<str>,
    link: Link,
}

impl PlayerBedHandle {
    pub(crate) fn new(target: Arc<str>, link: Link) -> Self {
        Self { target, link }
    }

    pub fn set_spawn(&self, spawn: BedSpawn) {
        player_order(
            &self.target,
            &self.link,
            player_extra::Order::SetBedSpawn(spawn.into_wire()),
        );
    }

    pub fn clear_spawn(&self) {
        player_order(
            &self.target,
            &self.link,
            player_extra::Order::ClearBedSpawn(v1::Empty {}),
        );
    }

    pub async fn get_spawn(&self) -> Result<Option<BedSpawn>, Error> {
        let answer = ask_extra(
            &self.link,
            extra_question::Query::BedSpawn(self.target.to_string()),
        )
        .await?;
        let info = expect_extra(answer, |answer| match answer {
            extra_answer::Answer::BedSpawn(info) => Some(info),
            _ => None,
        })?;
        info.spawn.map(BedSpawn::from_wire).transpose()
    }
}

#[derive(Debug, Clone)]
pub struct PlayerBookHandle {
    target: Arc<str>,
    link: Link,
}

impl PlayerBookHandle {
    pub(crate) fn new(target: Arc<str>, link: Link) -> Self {
        Self { target, link }
    }

    pub fn open(&self, book: &ItemStack) {
        player_order(
            &self.target,
            &self.link,
            player_extra::Order::OpenBook(book.to_wire()),
        );
    }
}
