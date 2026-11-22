use rspigot_domain::bed::RespawnConfig;
use rspigot_domain::player::PlayerId;
use rspigot_domain::world_border::{MAX_CENTER, MAX_SIZE};
use rspigot_domain::{BlockPos, ChunkPos, Identifier, Vec3};
use rspigot_entities::SoundSource;
use rspigot_items::ItemKind;
use rustc_hash::FxHashMap;

use crate::commands::{CommandContext, Weather};
use crate::events::{ClientEvent, GameEvent};
use crate::plugins::events::CreatureSpawnReason;
use crate::plugins::resolve::{finite_not_negative, not_negative};
use crate::plugins::{Answer, ItemSpec, stack_of};
use crate::ports::ClientLink;
use crate::server::{Audience, Server};
use crate::stats::StatKind;

const DAY_TICKS: i64 = 24_000;
const SOUND_SOURCES: [(&str, SoundSource); 11] = [
    ("master", SoundSource::Master),
    ("music", SoundSource::Music),
    ("record", SoundSource::Records),
    ("weather", SoundSource::Weather),
    ("block", SoundSource::Blocks),
    ("hostile", SoundSource::Hostile),
    ("neutral", SoundSource::Neutral),
    ("player", SoundSource::Players),
    ("ambient", SoundSource::Ambient),
    ("voice", SoundSource::Voice),
    ("ui", SoundSource::Ui),
];

#[derive(Debug, Clone, PartialEq)]
pub enum BorderOrder {
    Resize { size: f64, ticks: i64 },
    SetCenter { x: f64, z: f64 },
    SetDamageAmount(f64),
    SetDamageBuffer(f64),
    SetWarningDistance(i32),
    SetWarningTime(i32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChunkOrder {
    Load(ChunkPos),
    SetForceLoaded { pos: ChunkPos, forced: bool },
    Regenerate(ChunkPos),
}

#[derive(Debug, Clone, PartialEq)]
pub enum WorldAction {
    Border(BorderOrder),
    StrikeLightning { position: Vec3, effect_only: bool },
    Chunk(ChunkOrder),
}

impl WorldAction {
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Border(_) => "world.border",
            Self::StrikeLightning { .. } => "world.strike_lightning",
            Self::Chunk(ChunkOrder::Load(_)) => "world.chunk.load",
            Self::Chunk(ChunkOrder::SetForceLoaded { .. }) => "world.chunk.set_force_loaded",
            Self::Chunk(ChunkOrder::Regenerate(_)) => "world.chunk.regenerate",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatisticMode {
    Increment,
    Decrement,
    Set,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BedSpawn {
    pub pos: BlockPos,
    pub world: String,
    pub yaw: f32,
    pub force: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PlayerAction {
    SetTime {
        time: i64,
        relative: bool,
    },
    ResetTime,
    SetWeather(Weather),
    ResetWeather,
    StopSound {
        sound: Option<String>,
        source: Option<String>,
    },
    SetCooldown {
        item: String,
        ticks: i32,
    },
    Statistic {
        kind: String,
        key: String,
        amount: i32,
        mode: StatisticMode,
    },
    SetCompassTarget(Vec3),
    SetBedSpawn(BedSpawn),
    ClearBedSpawn,
    OpenBook(ItemSpec),
}

impl PlayerAction {
    pub const fn name(&self) -> &'static str {
        match self {
            Self::SetTime { .. } => "player.time.set",
            Self::ResetTime => "player.time.reset",
            Self::SetWeather(_) => "player.weather.set",
            Self::ResetWeather => "player.weather.reset",
            Self::StopSound { .. } => "player.sound.stop",
            Self::SetCooldown { .. } => "player.cooldown.set",
            Self::Statistic { .. } => "player.statistic.change",
            Self::SetCompassTarget(_) => "player.compass.set_target",
            Self::SetBedSpawn(_) => "player.bed_spawn.set",
            Self::ClearBedSpawn => "player.bed_spawn.clear",
            Self::OpenBook(_) => "player.book.open",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExtraQuestion {
    Cooldown {
        player: PlayerId,
        item: String,
    },
    Statistic {
        player: PlayerId,
        kind: String,
        key: String,
    },
    BedSpawn(PlayerId),
    CompassTarget(PlayerId),
    PlayerClock(PlayerId),
    Chunk {
        pos: ChunkPos,
        load: bool,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExtraAnswer {
    Count(i32),
    BedSpawn(Option<BedSpawn>),
    CompassTarget(Vec3),
    Clock {
        time: Option<(i64, bool)>,
        weather: Option<Weather>,
    },
    Chunk {
        pos: ChunkPos,
        loaded: bool,
        force_loaded: bool,
    },
}

#[derive(Debug, Default)]
struct PlayerView {
    time: Option<(i64, bool)>,
    weather: Option<Weather>,
    compass: Option<Vec3>,
}

#[derive(Debug, Default)]
pub(crate) struct PlayerViews(FxHashMap<PlayerId, PlayerView>);

impl PlayerViews {
    fn entry(&mut self, player: PlayerId) -> &mut PlayerView {
        self.0.entry(player).or_default()
    }

    fn get(&self, player: PlayerId) -> Option<&PlayerView> {
        self.0.get(&player)
    }

    fn retain_online(&mut self, online: impl Fn(PlayerId) -> bool) {
        self.0.retain(|player, _| online(*player));
    }
}

fn source_of(name: &str) -> Result<SoundSource, String> {
    SOUND_SOURCES
        .iter()
        .find_map(|(label, source)| (*label == name).then_some(*source))
        .ok_or_else(|| format!("unknown sound category {name}"))
}

fn stat_kind_of(name: &str) -> Result<StatKind, String> {
    StatKind::ALL
        .into_iter()
        .find(|kind| kind.registry_name() == name)
        .ok_or_else(|| format!("unknown statistic kind {name}"))
}

fn namespaced(key: &str) -> String {
    if key.contains(':') {
        key.to_owned()
    } else {
        format!("minecraft:{key}")
    }
}

fn weather_events(weather: Weather) -> [ClientEvent; 3] {
    let (rain, thunder) = match weather {
        Weather::Clear => (0.0, 0.0),
        Weather::Rain => (1.0, 0.0),
        Weather::Thunder => (1.0, 1.0),
    };
    [
        ClientEvent::GameEvent(if rain > 0.0 {
            GameEvent::StartRaining
        } else {
            GameEvent::StopRaining
        }),
        ClientEvent::GameEvent(GameEvent::RainLevelChange(rain)),
        ClientEvent::GameEvent(GameEvent::ThunderLevelChange(thunder)),
    ]
}

impl<L: ClientLink> Server<L> {
    pub(super) fn world_action(&mut self, action: WorldAction) -> Result<(), String> {
        match action {
            WorldAction::Border(order) => self.border_order(order),
            WorldAction::StrikeLightning {
                position,
                effect_only,
            } => {
                let position = super::resolve::position_of(position)?;
                self.spawn_bolt(position, effect_only, CreatureSpawnReason::Custom);
                Ok(())
            }
            WorldAction::Chunk(order) => {
                self.chunk_order(order);
                Ok(())
            }
        }
    }

    fn border_order(&mut self, order: BorderOrder) -> Result<(), String> {
        match order {
            BorderOrder::Resize { size, ticks } => {
                if !size.is_finite() || !(1.0..=MAX_SIZE).contains(&size) {
                    return Err(format!("border size {size} is outside 1..={MAX_SIZE}"));
                }
                if ticks < 0 {
                    return Err(format!("border resize time {ticks} is negative"));
                }
                CommandContext::change_world_border(self, |border, now| {
                    if ticks > 0 {
                        let current = border.size(now);
                        border.lerp_size(current, size, now, ticks);
                    } else {
                        border.set_size(size);
                    }
                });
            }
            BorderOrder::SetCenter { x, z } => {
                if !x.is_finite() || !z.is_finite() || x.abs() > MAX_CENTER || z.abs() > MAX_CENTER
                {
                    return Err(format!("border center {x}, {z} is outside ±{MAX_CENTER}"));
                }
                CommandContext::change_world_border(self, |border, _| border.set_center(x, z));
            }
            BorderOrder::SetDamageAmount(amount) => {
                finite_not_negative("border damage amount", amount)?;
                CommandContext::change_world_border(self, |border, _| {
                    border.set_damage_per_block(amount)
                });
            }
            BorderOrder::SetDamageBuffer(distance) => {
                finite_not_negative("border damage buffer", distance)?;
                CommandContext::change_world_border(self, |border, _| {
                    border.set_safe_zone(distance)
                });
            }
            BorderOrder::SetWarningDistance(blocks) => {
                not_negative("border warning distance", blocks)?;
                CommandContext::change_world_border(self, |border, _| {
                    border.set_warning_blocks(blocks)
                });
            }
            BorderOrder::SetWarningTime(ticks) => {
                not_negative("border warning time", ticks)?;
                CommandContext::change_world_border(self, |border, _| {
                    border.set_warning_time(ticks)
                });
            }
        }
        Ok(())
    }

    fn chunk_order(&mut self, order: ChunkOrder) {
        match order {
            ChunkOrder::Load(pos) => self.load_chunks_now(&[pos]),
            ChunkOrder::SetForceLoaded { pos, forced } => {
                self.world.chunks.set_forced(pos, forced, &self.workers);
            }
            ChunkOrder::Regenerate(pos) => self.regenerate_chunk(pos),
        }
    }

    fn regenerate_chunk(&mut self, pos: ChunkPos) {
        if !self.world.chunks.regenerate(pos, &self.workers) {
            return;
        }
        self.wait_for_spawn(std::time::Duration::from_secs(10));
        self.world.chunks.remove_ticket(pos);
        for player in self.players.values_mut() {
            player.streamer.resend(pos);
        }
    }

    pub(super) fn player_action(
        &mut self,
        id: PlayerId,
        action: PlayerAction,
    ) -> Result<(), String> {
        match action {
            PlayerAction::SetTime { time, relative } => {
                self.plugins.views.entry(id).time = Some((time, relative));
                self.send_player_time(id);
            }
            PlayerAction::ResetTime => {
                if let Some(view) = self.plugins.views.0.get_mut(&id) {
                    view.time = None;
                }
                let event = self.time_event();
                self.push(Audience::One(id), event);
            }
            PlayerAction::SetWeather(weather) => {
                self.plugins.views.entry(id).weather = Some(weather);
                self.send_player_weather(id);
            }
            PlayerAction::ResetWeather => {
                if let Some(view) = self.plugins.views.0.get_mut(&id) {
                    view.weather = None;
                }
                let world = self.world_weather();
                for event in weather_events(world) {
                    self.push(Audience::One(id), event);
                }
            }
            PlayerAction::StopSound { sound, source } => {
                let source = source.as_deref().map(source_of).transpose()?;
                let sound = sound.as_deref().map(namespaced);
                self.push(Audience::One(id), ClientEvent::StopSound { source, sound });
            }
            PlayerAction::SetCooldown { item, ticks } => {
                not_negative("cooldown ticks", ticks)?;
                let kind = ItemKind::from_name(&namespaced(&item))
                    .ok_or_else(|| format!("unknown item {item}"))?;
                self.start_cooldown(id, kind.id(), ticks);
            }
            PlayerAction::Statistic {
                kind,
                key,
                amount,
                mode,
            } => self.change_statistic(id, &kind, &key, amount, mode)?,
            PlayerAction::SetCompassTarget(target) => {
                let target = super::resolve::position_of(target)?;
                self.plugins.views.entry(id).compass = Some(target);
                self.send_compass(id, target);
            }
            PlayerAction::SetBedSpawn(spawn) => {
                let dimension = if spawn.world.is_empty() {
                    self.world.dimension.key.clone()
                } else {
                    Identifier::parse(&spawn.world)
                        .map_err(|_| format!("{} is not a world", spawn.world))?
                };
                let config = RespawnConfig {
                    dimension,
                    pos: spawn.pos,
                    yaw: spawn.yaw,
                    pitch: 0.0,
                    forced: spawn.force,
                };
                if let Some(player) = self.players.get_mut(&id) {
                    player.respawn = Some(config);
                }
            }
            PlayerAction::ClearBedSpawn => {
                if let Some(player) = self.players.get_mut(&id) {
                    player.respawn = None;
                }
            }
            PlayerAction::OpenBook(spec) => self.open_book(id, &spec)?,
        }
        Ok(())
    }

    fn world_weather(&self) -> Weather {
        let level = &self.world.level;
        match (level.raining, level.thundering) {
            (false, _) => Weather::Clear,
            (true, false) => Weather::Rain,
            (true, true) => Weather::Thunder,
        }
    }

    fn send_player_time(&mut self, id: PlayerId) {
        let Some((time, relative)) = self.plugins.views.get(id).and_then(|view| view.time) else {
            return;
        };
        let level = &self.world.level;
        let event = if relative {
            ClientEvent::Time {
                game_time: level.game_time,
                day_time: level.day_time.wrapping_add(time),
                advance_time: level.daylight_cycle && !self.world.dimension.has_fixed_time,
            }
        } else {
            ClientEvent::Time {
                game_time: level.game_time,
                day_time: time.rem_euclid(DAY_TICKS),
                advance_time: false,
            }
        };
        self.push(Audience::One(id), event);
    }

    fn send_player_weather(&mut self, id: PlayerId) {
        let Some(weather) = self.plugins.views.get(id).and_then(|view| view.weather) else {
            return;
        };
        for event in weather_events(weather) {
            self.push(Audience::One(id), event);
        }
    }

    fn send_compass(&mut self, id: PlayerId, target: Vec3) {
        let dimension = self.world.dimension.key.clone();
        self.push(
            Audience::One(id),
            ClientEvent::SpawnPosition {
                dimension,
                pos: target.block(),
                yaw: 0.0,
                pitch: 0.0,
            },
        );
    }

    pub(crate) fn sync_world_time(&mut self) {
        let time = self.time_event();
        self.push(Audience::Dimension(self.world.dimension.key.clone()), time);
        let online: Vec<PlayerId> = self.players.keys().copied().collect();
        self.plugins
            .views
            .retain_online(|player| online.contains(&player));
        for id in online {
            self.send_player_time(id);
            self.send_player_weather(id);
        }
    }

    fn change_statistic(
        &mut self,
        id: PlayerId,
        kind: &str,
        key: &str,
        amount: i32,
        mode: StatisticMode,
    ) -> Result<(), String> {
        let kind = stat_kind_of(kind)?;
        let key = namespaced(key);
        if !kind.knows(&key) {
            return Err(format!("unknown statistic qualifier {key}"));
        }
        if amount < 0 || (mode != StatisticMode::Set && amount == 0) {
            return Err(format!("statistic amount {amount} is not positive"));
        }
        let Some(current) = self.players.get(&id).map(|p| p.stats.get(kind, &key)) else {
            return Ok(());
        };
        let target = match mode {
            StatisticMode::Increment => {
                self.award(id, kind, &key, amount);
                return Ok(());
            }
            StatisticMode::Decrement => current.saturating_sub(amount).max(0),
            StatisticMode::Set => amount,
        };
        if let Some(player) = self.players.get_mut(&id) {
            player.stats.reset(kind, &key);
            player.stats.add(kind, &key, target);
        }
        self.stat_score_reset(id, kind, &key);
        if target > 0 {
            self.stat_score_added(id, kind, &key, target);
        }
        Ok(())
    }

    fn open_book(&mut self, id: PlayerId, spec: &ItemSpec) -> Result<(), String> {
        let book = stack_of(spec)?
            .filter(|stack| stack.item.kind() == Some(ItemKind::WrittenBook))
            .ok_or_else(|| "only a written book can be opened".to_owned())?;
        let Some(player) = self.players.get(&id) else {
            return Ok(());
        };
        let slot = i32::from(player.inventory.selected_hotbar);
        let held = self.held(id, crate::events::Hand::Main);
        self.push(
            Audience::One(id),
            ClientEvent::PlayerInventorySlot {
                slot,
                stack: Some(book),
            },
        );
        self.push(
            Audience::One(id),
            ClientEvent::OpenBook(crate::events::Hand::Main),
        );
        self.push(
            Audience::One(id),
            ClientEvent::PlayerInventorySlot { slot, stack: held },
        );
        Ok(())
    }

    pub(super) fn extra_answer(&mut self, question: ExtraQuestion) -> Answer {
        let answer = match question {
            ExtraQuestion::Cooldown { player, item } => {
                let Some(kind) = ItemKind::from_name(&namespaced(&item)) else {
                    return Answer::Refused(format!("unknown item {item}"));
                };
                let group = crate::cooldown::group_of(kind.id());
                let now = self.current_tick();
                let ticks = self
                    .players
                    .get(&player)
                    .zip(group)
                    .and_then(|(player, group)| player.cooldowns.get(group))
                    .map_or(0, |expiry| expiry.saturating_sub(now));
                ExtraAnswer::Count(i32::try_from(ticks).unwrap_or(i32::MAX))
            }
            ExtraQuestion::Statistic { player, kind, key } => {
                let kind = match stat_kind_of(&kind) {
                    Ok(kind) => kind,
                    Err(reason) => return Answer::Refused(reason),
                };
                let key = namespaced(&key);
                if !kind.knows(&key) {
                    return Answer::Refused(format!("unknown statistic qualifier {key}"));
                }
                let Some(player) = self.players.get(&player) else {
                    return Answer::NotFound;
                };
                ExtraAnswer::Count(player.stats.get(kind, &key))
            }
            ExtraQuestion::BedSpawn(player) => {
                let Some(player) = self.players.get(&player) else {
                    return Answer::NotFound;
                };
                ExtraAnswer::BedSpawn(player.respawn.as_ref().map(|config| BedSpawn {
                    pos: config.pos,
                    world: config.dimension.to_string(),
                    yaw: config.yaw,
                    force: config.forced,
                }))
            }
            ExtraQuestion::CompassTarget(player) => {
                if !self.players.contains_key(&player) {
                    return Answer::NotFound;
                }
                let spawn = self.world.spawn.block();
                ExtraAnswer::CompassTarget(
                    self.plugins
                        .views
                        .get(player)
                        .and_then(|view| view.compass)
                        .unwrap_or_else(|| spawn.center()),
                )
            }
            ExtraQuestion::PlayerClock(player) => {
                if !self.players.contains_key(&player) {
                    return Answer::NotFound;
                }
                let view = self.plugins.views.get(player);
                ExtraAnswer::Clock {
                    time: view.and_then(|view| view.time),
                    weather: view.and_then(|view| view.weather),
                }
            }
            ExtraQuestion::Chunk { pos, load } => {
                if load {
                    self.load_chunks_now(&[pos]);
                }
                ExtraAnswer::Chunk {
                    pos,
                    loaded: self.world.chunks.is_loaded(pos),
                    force_loaded: self.world.chunks.is_forced(pos),
                }
            }
        };
        Answer::Extra(answer)
    }

    pub(super) fn set_motd(&mut self, motd: String) -> Result<(), String> {
        if motd.chars().count() > 32_767 {
            return Err("the MOTD is longer than 32767 characters".to_owned());
        }
        self.config.motd.clone_from(&motd);
        self.status.update(|status| status.motd = Some(motd));
        Ok(())
    }

    pub(super) fn set_max_players(&mut self, count: i32) -> Result<(), String> {
        let count = u32::try_from(count).map_err(|_| format!("{count} players is negative"))?;
        self.config.max_players = count;
        self.status.update(|status| status.max_players = count);
        Ok(())
    }
}
