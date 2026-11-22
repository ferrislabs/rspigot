use std::net::IpAddr;
use std::time::{Duration, SystemTime};

use crossbeam_channel::Sender;
use rspigot_domain::player::{GameMode, PlayerId};
use rspigot_domain::text::Component;
use rspigot_domain::world::Difficulty;
use uuid::Uuid;

use crate::admin::{AdminLists, Ban, IpBanEntry, ListEntry};
use crate::commands::CommandContext;
use crate::ports::ClientLink;
use crate::server::Server;

const ACTIVITY_INTERVAL: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PlayerRef {
    pub id: Option<Uuid>,
    pub name: Option<String>,
}

impl PlayerRef {
    pub fn named(name: &str) -> Self {
        Self {
            id: None,
            name: Some(name.to_owned()),
        }
    }

    fn of_entry(entry: &ListEntry) -> Self {
        Self {
            id: entry.uuid,
            name: Some(entry.name.clone()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BanRequest {
    pub player: PlayerRef,
    pub reason: Option<String>,
    pub source: Option<String>,
    pub expires: Option<SystemTime>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IpBanRequest {
    pub player: Option<PlayerRef>,
    pub ip: Option<String>,
    pub reason: Option<String>,
    pub source: Option<String>,
    pub expires: Option<SystemTime>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperatorRequest {
    pub player: PlayerRef,
    pub level: Option<u8>,
    pub bypasses_player_limit: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ListAction<A, R> {
    Get,
    Add(Vec<A>),
    Remove(Vec<R>),
    Set(Vec<A>),
    Clear,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Setting {
    Autosave,
    Difficulty,
    EnforceAllowlist,
    MaxPlayers,
    GameMode,
    ForceGameMode,
    AllowFlight,
    OperatorPermissionLevel,
    UseAllowlist,
    ViewDistance,
    SimulationDistance,
    PlayerIdleTimeout,
    SpawnProtectionRadius,
    PauseWhenEmptySeconds,
    StatusHeartbeatInterval,
    Motd,
    HideOnlinePlayers,
    AcceptTransfers,
    EntityBroadcastRange,
    StatusReplies,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingValue {
    Bool(bool),
    Int(i64),
    Text(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum ManagementRequest {
    Allowlist(ListAction<PlayerRef, PlayerRef>),
    Bans(ListAction<BanRequest, PlayerRef>),
    IpBans(ListAction<IpBanRequest, String>),
    Operators(ListAction<OperatorRequest, PlayerRef>),
    Players,
    Kick(Vec<(PlayerRef, Option<String>)>),
    GameRules,
    GameRule {
        key: String,
        value: String,
    },
    Setting {
        setting: Setting,
        value: Option<SettingValue>,
    },
    Status,
    Save {
        flush: bool,
    },
    Stop,
    SystemMessage {
        message: String,
        overlay: bool,
        receivers: Option<Vec<PlayerRef>>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BanView {
    pub player: PlayerRef,
    pub reason: Option<String>,
    pub source: String,
    pub expires: Option<SystemTime>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IpBanView {
    pub ip: String,
    pub reason: Option<String>,
    pub source: String,
    pub expires: Option<SystemTime>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperatorView {
    pub player: PlayerRef,
    pub level: u8,
    pub bypasses_player_limit: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusView {
    pub started: bool,
    pub players: Vec<PlayerRef>,
    pub version_name: String,
    pub protocol: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManagementFault {
    InvalidParams(String),
    Failed(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum ManagementReply {
    Allowlist(Vec<PlayerRef>),
    Bans(Vec<BanView>),
    IpBans(Vec<IpBanView>),
    Operators(Vec<OperatorView>),
    Players(Vec<PlayerRef>),
    GameRules(Vec<(String, String)>),
    GameRule(String, String),
    Setting(SettingValue),
    Status(StatusView),
    Done(bool),
    Fault(ManagementFault),
}

#[derive(Debug, Clone, PartialEq)]
pub enum ManagementNotice {
    Started,
    Stopping,
    Saving,
    Saved,
    PlayerJoined(PlayerRef),
    PlayerLeft(PlayerRef),
    OperatorAdded(OperatorView),
    OperatorRemoved(OperatorView),
    AllowlistAdded(PlayerRef),
    AllowlistRemoved(PlayerRef),
    BanAdded(BanView),
    BanRemoved(PlayerRef),
    IpBanAdded(IpBanView),
    IpBanRemoved(String),
    GameRuleUpdated { key: String, value: String },
    Activity,
    Status(StatusView),
}

pub struct ManagementCall {
    pub request: ManagementRequest,
    pub reply: Sender<ManagementReply>,
}

fn ban_view(ban: &Ban<ListEntry>) -> BanView {
    BanView {
        player: PlayerRef::of_entry(&ban.target),
        reason: ban.reason.clone(),
        source: ban.source.clone(),
        expires: ban.expires,
    }
}

fn ip_ban_view(ban: &IpBanEntry) -> IpBanView {
    IpBanView {
        ip: ban.target.to_string(),
        reason: ban.reason.clone(),
        source: ban.source.clone(),
        expires: ban.expires,
    }
}

fn operator_view(entry: &ListEntry, default_level: u8) -> OperatorView {
    OperatorView {
        player: PlayerRef::of_entry(entry),
        level: entry.level.unwrap_or(default_level),
        bypasses_player_limit: entry.bypasses_player_limit,
    }
}

fn difficulty_name(difficulty: Difficulty) -> &'static str {
    match difficulty {
        Difficulty::Peaceful => "peaceful",
        Difficulty::Easy => "easy",
        Difficulty::Normal => "normal",
        Difficulty::Hard => "hard",
    }
}

fn mode_name(mode: GameMode) -> &'static str {
    match mode {
        GameMode::Survival => "survival",
        GameMode::Creative => "creative",
        GameMode::Adventure => "adventure",
        GameMode::Spectator => "spectator",
    }
}

fn distance(value: i64) -> Option<u8> {
    u8::try_from(value).ok().filter(|n| (2..=32).contains(n))
}

fn same_player(a: &ListEntry, b: &ListEntry) -> bool {
    a.matches_name(&b.name) || (a.uuid.is_some() && a.uuid == b.uuid)
}

pub fn admin_diff(old: &AdminLists, new: &AdminLists, op_level: u8) -> Vec<ManagementNotice> {
    let mut notices = Vec::new();
    for entry in &new.operators {
        match old.operators.iter().find(|e| same_player(e, entry)) {
            Some(before) if before == entry => {}
            _ => notices.push(ManagementNotice::OperatorAdded(operator_view(
                entry, op_level,
            ))),
        }
    }
    for entry in &old.operators {
        if !new.operators.iter().any(|e| same_player(e, entry)) {
            notices.push(ManagementNotice::OperatorRemoved(operator_view(
                entry, op_level,
            )));
        }
    }
    for entry in &new.whitelist {
        if !old.whitelist.iter().any(|e| same_player(e, entry)) {
            notices.push(ManagementNotice::AllowlistAdded(PlayerRef::of_entry(entry)));
        }
    }
    for entry in &old.whitelist {
        if !new.whitelist.iter().any(|e| same_player(e, entry)) {
            notices.push(ManagementNotice::AllowlistRemoved(PlayerRef::of_entry(
                entry,
            )));
        }
    }
    for ban in &new.bans {
        if !old.bans.iter().any(|b| same_player(&b.target, &ban.target)) {
            notices.push(ManagementNotice::BanAdded(ban_view(ban)));
        }
    }
    for ban in &old.bans {
        if !new.bans.iter().any(|b| same_player(&b.target, &ban.target)) {
            notices.push(ManagementNotice::BanRemoved(PlayerRef::of_entry(
                &ban.target,
            )));
        }
    }
    for ban in &new.ip_bans {
        if !old.ip_bans.iter().any(|b| b.target == ban.target) {
            notices.push(ManagementNotice::IpBanAdded(ip_ban_view(ban)));
        }
    }
    for ban in &old.ip_bans {
        if !new.ip_bans.iter().any(|b| b.target == ban.target) {
            notices.push(ManagementNotice::IpBanRemoved(ban.target.to_string()));
        }
    }
    notices
}

impl<L: ClientLink> Server<L> {
    pub fn with_management_sink(mut self, sink: Sender<ManagementNotice>) -> Self {
        self.management_seen = self.admin.clone();
        self.management_sink = Some(sink);
        self
    }

    pub(crate) fn notify_management(&self, notice: ManagementNotice) {
        if let Some(sink) = &self.management_sink {
            let _ = sink.send(notice);
        }
    }

    pub(crate) fn diff_management_lists(&mut self) {
        if self.management_sink.is_none() {
            return;
        }
        let notices = admin_diff(
            &self.management_seen,
            &self.admin,
            self.config.op_permission_level,
        );
        self.management_seen = self.admin.clone();
        for notice in notices {
            self.notify_management(notice);
        }
    }

    fn resolve_entry(&self, player: &PlayerRef) -> Result<ListEntry, ManagementFault> {
        let online = player
            .id
            .and_then(|id| self.players.get(&PlayerId(id)))
            .or_else(|| {
                player
                    .name
                    .as_deref()
                    .and_then(|name| self.find_player(name))
                    .and_then(|id| self.players.get(&id))
            });
        if let Some(online) = online {
            return Ok(ListEntry::of(&online.profile));
        }
        match (&player.name, player.id) {
            (Some(name), id) => {
                let mut entry = self.offline_entry(name);
                if id.is_some() {
                    entry.uuid = id;
                }
                Ok(entry)
            }
            (None, Some(id)) => Ok(self.offline_entry_of(id).unwrap_or_else(|| ListEntry {
                name: id.to_string(),
                uuid: Some(id),
                level: None,
                bypasses_player_limit: false,
            })),
            (None, None) => Err(ManagementFault::InvalidParams(
                "A player needs an id or a name".to_owned(),
            )),
        }
    }

    fn entries(&self, players: &[PlayerRef]) -> Result<Vec<ListEntry>, ManagementFault> {
        players.iter().map(|p| self.resolve_entry(p)).collect()
    }

    fn allowlist_reply(&self) -> ManagementReply {
        ManagementReply::Allowlist(
            self.admin
                .whitelist
                .iter()
                .map(PlayerRef::of_entry)
                .collect(),
        )
    }

    fn manage_allowlist(
        &mut self,
        action: ListAction<PlayerRef, PlayerRef>,
    ) -> Result<ManagementReply, ManagementFault> {
        match action {
            ListAction::Get => {}
            ListAction::Add(players) => {
                for entry in self.entries(&players)? {
                    let name = entry.name.clone();
                    if self.find_player(&name).is_some() {
                        self.whitelist_add(&name);
                    } else if AdminLists::add(&mut self.admin.whitelist, entry) {
                        self.publish_admin();
                    }
                }
            }
            ListAction::Remove(players) => {
                for entry in self.entries(&players)? {
                    self.whitelist_remove(&entry.name);
                }
            }
            ListAction::Set(players) => {
                let entries = self.entries(&players)?;
                self.admin.whitelist = entries;
                self.publish_admin();
            }
            ListAction::Clear => {
                self.admin.whitelist.clear();
                self.publish_admin();
            }
        }
        Ok(self.allowlist_reply())
    }

    fn bans_reply(&self) -> ManagementReply {
        ManagementReply::Bans(self.admin.bans.iter().map(ban_view).collect())
    }

    fn ban_one(&mut self, request: &BanRequest) -> Result<(), ManagementFault> {
        let target = self.resolve_entry(&request.player)?;
        let source = request
            .source
            .clone()
            .unwrap_or_else(|| "Management server".to_owned());
        self.ban_profile(target, &source, request.reason.clone(), request.expires);
        Ok(())
    }

    fn manage_bans(
        &mut self,
        action: ListAction<BanRequest, PlayerRef>,
    ) -> Result<ManagementReply, ManagementFault> {
        match action {
            ListAction::Get => {}
            ListAction::Add(requests) => {
                for request in &requests {
                    self.ban_one(request)?;
                }
            }
            ListAction::Remove(players) => {
                for entry in self.entries(&players)? {
                    self.pardon_profile(&entry);
                }
            }
            ListAction::Set(requests) => {
                self.admin.bans.clear();
                for request in &requests {
                    self.ban_one(request)?;
                }
                self.publish_admin();
            }
            ListAction::Clear => {
                self.admin.bans.clear();
                self.publish_admin();
            }
        }
        Ok(self.bans_reply())
    }

    fn ip_bans_reply(&self) -> ManagementReply {
        ManagementReply::IpBans(self.admin.ip_bans.iter().map(ip_ban_view).collect())
    }

    fn ip_ban_address(&self, request: &IpBanRequest) -> Result<IpAddr, ManagementFault> {
        if let Some(ip) = &request.ip {
            return ip
                .parse()
                .map_err(|_| ManagementFault::InvalidParams(format!("Invalid ip address: {ip}")));
        }
        let player = request.player.as_ref().ok_or_else(|| {
            ManagementFault::InvalidParams("An ip ban needs an ip or a player".to_owned())
        })?;
        let entry = self.resolve_entry(player)?;
        self.find_player(&entry.name)
            .and_then(|id| self.player_address(id))
            .ok_or_else(|| ManagementFault::Failed("The player is not online".to_owned()))
    }

    fn manage_ip_bans(
        &mut self,
        action: ListAction<IpBanRequest, String>,
    ) -> Result<ManagementReply, ManagementFault> {
        match action {
            ListAction::Get => {}
            ListAction::Add(requests) => {
                for request in &requests {
                    let address = self.ip_ban_address(request)?;
                    let source = request
                        .source
                        .clone()
                        .unwrap_or_else(|| "Management server".to_owned());
                    self.ban_address(address, &source, request.reason.clone(), request.expires);
                }
            }
            ListAction::Remove(addresses) => {
                for text in addresses {
                    let address: IpAddr = text.parse().map_err(|_| {
                        ManagementFault::InvalidParams(format!("Invalid ip address: {text}"))
                    })?;
                    self.pardon_address(address);
                }
            }
            ListAction::Set(requests) => {
                self.admin.ip_bans.clear();
                for request in &requests {
                    let address = self.ip_ban_address(request)?;
                    let source = request
                        .source
                        .clone()
                        .unwrap_or_else(|| "Management server".to_owned());
                    self.ban_address(address, &source, request.reason.clone(), request.expires);
                }
                self.publish_admin();
            }
            ListAction::Clear => {
                self.admin.ip_bans.clear();
                self.publish_admin();
            }
        }
        Ok(self.ip_bans_reply())
    }

    fn operators_reply(&self) -> ManagementReply {
        let level = self.config.op_permission_level;
        ManagementReply::Operators(
            self.admin
                .operators
                .iter()
                .map(|e| operator_view(e, level))
                .collect(),
        )
    }

    pub(crate) fn set_operator_entry(&mut self, entry: ListEntry) {
        let name = entry.name.clone();
        let level = entry.level.unwrap_or(self.config.op_permission_level);
        match self
            .admin
            .operators
            .iter_mut()
            .find(|e| same_player(e, &entry))
        {
            Some(existing) => *existing = entry,
            None => self.admin.operators.push(entry),
        }
        self.publish_admin();
        if let Some(id) = self.find_player(&name) {
            if let Some(p) = self.players.get_mut(&id) {
                p.operator = true;
                p.op_level = level;
                p.link.send(crate::events::ClientEvent::OperatorLevel {
                    entity: p.entity_id,
                    level: p.permission_level(),
                });
            }
            self.send_commands(id);
        }
    }

    fn operator_entry_of(&self, request: &OperatorRequest) -> Result<ListEntry, ManagementFault> {
        let mut entry = self.resolve_entry(&request.player)?;
        let level = request
            .level
            .unwrap_or(self.config.op_permission_level)
            .min(crate::admin::MAX_PERMISSION_LEVEL);
        entry.level = Some(level);
        entry.bypasses_player_limit = request.bypasses_player_limit.unwrap_or(false);
        Ok(entry)
    }

    fn manage_operators(
        &mut self,
        action: ListAction<OperatorRequest, PlayerRef>,
    ) -> Result<ManagementReply, ManagementFault> {
        match action {
            ListAction::Get => {}
            ListAction::Add(requests) => {
                for request in &requests {
                    let entry = self.operator_entry_of(request)?;
                    self.set_operator_entry(entry);
                }
            }
            ListAction::Remove(players) => {
                for entry in self.entries(&players)? {
                    self.set_operator(&entry.name, false);
                }
            }
            ListAction::Set(requests) => {
                let wanted: Vec<ListEntry> = requests
                    .iter()
                    .map(|r| self.operator_entry_of(r))
                    .collect::<Result<_, _>>()?;
                let stale: Vec<String> = self
                    .admin
                    .operators
                    .iter()
                    .filter(|e| !wanted.iter().any(|w| same_player(w, e)))
                    .map(|e| e.name.clone())
                    .collect();
                for name in stale {
                    self.set_operator(&name, false);
                }
                for entry in wanted {
                    self.set_operator_entry(entry);
                }
            }
            ListAction::Clear => {
                let names: Vec<String> = self
                    .admin
                    .operators
                    .iter()
                    .map(|e| e.name.clone())
                    .collect();
                for name in names {
                    self.set_operator(&name, false);
                }
            }
        }
        Ok(self.operators_reply())
    }

    fn online_players(&self) -> Vec<PlayerRef> {
        self.players
            .values()
            .map(|p| PlayerRef {
                id: Some(p.profile.id.0),
                name: Some(p.profile.name.clone()),
            })
            .collect()
    }

    fn manage_kick(
        &mut self,
        kicks: Vec<(PlayerRef, Option<String>)>,
    ) -> Result<ManagementReply, ManagementFault> {
        let mut kicked = Vec::new();
        for (player, message) in kicks {
            let entry = self.resolve_entry(&player)?;
            let Some(id) = self.find_player(&entry.name) else {
                continue;
            };
            let reason = message.map_or_else(
                || Component::translatable("multiplayer.disconnect.kicked", Vec::new()),
                Component::text,
            );
            self.kick(id, reason);
            kicked.push(PlayerRef::of_entry(&entry));
        }
        Ok(ManagementReply::Players(kicked))
    }

    fn manage_game_rule(
        &mut self,
        key: &str,
        value: &str,
    ) -> Result<ManagementReply, ManagementFault> {
        let name = key.strip_prefix("minecraft:").unwrap_or(key);
        if self.game_rule_text(name).is_none() {
            return Err(ManagementFault::InvalidParams(format!(
                "Unknown game rule: {key}"
            )));
        }
        self.apply_game_rule(name, value)
            .map_err(|code| ManagementFault::InvalidParams(format!("{code}: {value}")))?;
        let current = self.game_rule_text(name).unwrap_or_default();
        self.notify_management(ManagementNotice::GameRuleUpdated {
            key: name.to_owned(),
            value: current.clone(),
        });
        Ok(ManagementReply::GameRule(name.to_owned(), current))
    }

    fn manage_game_rules(&self) -> ManagementReply {
        ManagementReply::GameRules(
            crate::game_rule_packet::RULE_NAMES
                .iter()
                .filter_map(|name| Some(((*name).to_owned(), self.game_rule_text(name)?)))
                .collect(),
        )
    }

    fn manage_setting(
        &mut self,
        setting: Setting,
        value: Option<SettingValue>,
    ) -> Result<ManagementReply, ManagementFault> {
        let invalid = |what: &str| ManagementFault::InvalidParams(format!("Expected {what}"));
        if let Some(value) = value {
            match (setting, value) {
                (Setting::Autosave, SettingValue::Bool(on)) => {
                    self.saving_enabled = on;
                }
                (Setting::Difficulty, SettingValue::Text(name)) => {
                    let difficulty =
                        Difficulty::from_name(&name).ok_or_else(|| invalid("a difficulty name"))?;
                    self.set_difficulty(difficulty);
                }
                (Setting::UseAllowlist, SettingValue::Bool(on)) => {
                    self.set_whitelist_enabled(on);
                }
                (Setting::EnforceAllowlist, SettingValue::Bool(on)) => {
                    self.config.enforce_whitelist = on;
                    if on {
                        self.enforce_whitelist();
                    }
                }
                (Setting::ViewDistance, SettingValue::Int(n)) => {
                    self.config.view_distance = distance(n).ok_or_else(|| invalid("2 to 32"))?;
                }
                (Setting::SimulationDistance, SettingValue::Int(n)) => {
                    self.config.simulation_distance =
                        distance(n).ok_or_else(|| invalid("2 to 32"))?;
                }
                (Setting::PlayerIdleTimeout, SettingValue::Int(n)) => {
                    self.config.player_idle_timeout =
                        u32::try_from(n).map_err(|_| invalid("a number of minutes"))?;
                }
                (Setting::SpawnProtectionRadius, SettingValue::Int(n)) => {
                    self.config.spawn_protection =
                        u32::try_from(n).map_err(|_| invalid("a radius"))?;
                }
                (Setting::PauseWhenEmptySeconds, SettingValue::Int(n)) => {
                    self.config.pause_when_empty_seconds =
                        u32::try_from(n).map_err(|_| invalid("a number of seconds"))?;
                }
                (Setting::StatusHeartbeatInterval, SettingValue::Int(n)) => {
                    self.config.status_heartbeat_interval =
                        u32::try_from(n).map_err(|_| invalid("a number of seconds"))?;
                }
                (
                    Setting::Motd
                    | Setting::HideOnlinePlayers
                    | Setting::AcceptTransfers
                    | Setting::EntityBroadcastRange
                    | Setting::StatusReplies,
                    _,
                ) => {
                    return Err(ManagementFault::Failed(
                        "This setting is read at startup and cannot change while running"
                            .to_owned(),
                    ));
                }
                (Setting::MaxPlayers, SettingValue::Int(n)) => {
                    self.config.max_players = u32::try_from(n).map_err(|_| invalid("a count"))?;
                }
                (Setting::GameMode, SettingValue::Text(name)) => {
                    let mode =
                        GameMode::from_name(&name).ok_or_else(|| invalid("a game mode name"))?;
                    self.set_default_game_mode(mode);
                }
                (Setting::ForceGameMode, SettingValue::Bool(on)) => {
                    self.config.force_game_mode = on;
                }
                (Setting::AllowFlight, SettingValue::Bool(on)) => {
                    self.config.allow_flight = on;
                }
                (Setting::OperatorPermissionLevel, SettingValue::Int(n)) => {
                    self.config.op_permission_level = u8::try_from(n)
                        .ok()
                        .filter(|level| *level <= crate::admin::MAX_PERMISSION_LEVEL)
                        .ok_or_else(|| invalid("a level between 0 and 4"))?;
                }
                _ => return Err(invalid("a value of the right type")),
            }
        }
        Ok(ManagementReply::Setting(match setting {
            Setting::Autosave => SettingValue::Bool(self.saving_enabled),
            Setting::Difficulty => {
                SettingValue::Text(difficulty_name(self.difficulty()).to_owned())
            }
            Setting::UseAllowlist => SettingValue::Bool(self.admin.whitelist_enabled),
            Setting::EnforceAllowlist => SettingValue::Bool(self.config.enforce_whitelist),
            Setting::ViewDistance => SettingValue::Int(i64::from(self.config.view_distance)),
            Setting::SimulationDistance => {
                SettingValue::Int(i64::from(self.config.simulation_distance))
            }
            Setting::PlayerIdleTimeout => {
                SettingValue::Int(i64::from(self.config.player_idle_timeout))
            }
            Setting::SpawnProtectionRadius => {
                SettingValue::Int(i64::from(self.config.spawn_protection))
            }
            Setting::PauseWhenEmptySeconds => {
                SettingValue::Int(i64::from(self.config.pause_when_empty_seconds))
            }
            Setting::StatusHeartbeatInterval => {
                SettingValue::Int(i64::from(self.config.status_heartbeat_interval))
            }
            Setting::Motd => SettingValue::Text(self.config.motd.clone()),
            Setting::HideOnlinePlayers => SettingValue::Bool(self.config.hide_online_players),
            Setting::AcceptTransfers => SettingValue::Bool(self.config.accepts_transfers),
            Setting::EntityBroadcastRange => {
                SettingValue::Int(i64::from(self.config.entity_broadcast_range_percentage))
            }
            Setting::StatusReplies => SettingValue::Bool(self.config.enable_status),
            Setting::MaxPlayers => SettingValue::Int(i64::from(self.config.max_players)),
            Setting::GameMode => {
                SettingValue::Text(mode_name(self.config.default_game_mode).to_owned())
            }
            Setting::ForceGameMode => SettingValue::Bool(self.config.force_game_mode),
            Setting::AllowFlight => SettingValue::Bool(self.config.allow_flight),
            Setting::OperatorPermissionLevel => {
                SettingValue::Int(i64::from(self.config.op_permission_level))
            }
        }))
    }

    fn manage_system_message(
        &mut self,
        message: String,
        overlay: bool,
        receivers: Option<Vec<PlayerRef>>,
    ) -> Result<ManagementReply, ManagementFault> {
        let component = Component::text(message);
        let targets: Vec<PlayerId> = match receivers {
            None => self.players.keys().copied().collect(),
            Some(list) => {
                let mut ids = Vec::new();
                for player in &list {
                    let entry = self.resolve_entry(player)?;
                    if let Some(id) = self.find_player(&entry.name) {
                        ids.push(id);
                    }
                }
                ids
            }
        };
        for id in &targets {
            if let Some(player) = self.players.get(id) {
                player.link.send(crate::events::ClientEvent::SystemChat {
                    message: component.clone(),
                    overlay,
                });
            }
        }
        Ok(ManagementReply::Done(true))
    }

    fn status_view(&self) -> StatusView {
        StatusView {
            started: true,
            players: self.online_players(),
            version_name: rspigot_data::VERSION_NAME.to_owned(),
            protocol: rspigot_data::PROTOCOL_VERSION,
        }
    }

    pub(crate) fn note_management_activity(&mut self) {
        if self.management_sink.is_none() {
            return;
        }
        let due = self
            .management_activity
            .is_none_or(|last| last.elapsed() >= ACTIVITY_INTERVAL);
        if due {
            self.management_activity = Some(std::time::Instant::now());
            self.notify_management(ManagementNotice::Activity);
        }
    }

    pub(crate) fn handle_management(&mut self, call: ManagementCall) {
        let ManagementCall { request, reply } = call;
        let outcome = match request {
            ManagementRequest::Allowlist(action) => self.manage_allowlist(action),
            ManagementRequest::Bans(action) => self.manage_bans(action),
            ManagementRequest::IpBans(action) => self.manage_ip_bans(action),
            ManagementRequest::Operators(action) => self.manage_operators(action),
            ManagementRequest::Players => Ok(ManagementReply::Players(self.online_players())),
            ManagementRequest::Kick(kicks) => self.manage_kick(kicks),
            ManagementRequest::GameRules => Ok(self.manage_game_rules()),
            ManagementRequest::GameRule { key, value } => self.manage_game_rule(&key, &value),
            ManagementRequest::Setting { setting, value } => self.manage_setting(setting, value),
            ManagementRequest::Status => Ok(ManagementReply::Status(self.status_view())),
            ManagementRequest::Save { flush } => {
                self.notify_management(ManagementNotice::Saving);
                let done = CommandContext::save_all(self, flush);
                self.notify_management(ManagementNotice::Saved);
                Ok(ManagementReply::Done(done))
            }
            ManagementRequest::Stop => {
                self.running = false;
                Ok(ManagementReply::Done(true))
            }
            ManagementRequest::SystemMessage {
                message,
                overlay,
                receivers,
            } => self.manage_system_message(message, overlay, receivers),
        };
        let _ = reply.send(outcome.unwrap_or_else(ManagementReply::Fault));
    }
}

#[cfg(test)]
mod tests {
    use std::time::UNIX_EPOCH;

    use super::*;

    fn entry(name: &str) -> ListEntry {
        ListEntry::named(name)
    }

    #[test]
    fn a_new_operator_and_a_changed_level_are_both_added_notices() {
        let old = AdminLists {
            operators: vec![entry("A").operator(2)],
            ..AdminLists::default()
        };
        let new = AdminLists {
            operators: vec![entry("A").operator(4), entry("B").operator(4)],
            ..AdminLists::default()
        };
        let notices = admin_diff(&old, &new, 4);
        assert_eq!(notices.len(), 2);
        assert!(matches!(&notices[0], ManagementNotice::OperatorAdded(v) if v.level == 4));
    }

    #[test]
    fn removed_entries_are_reported_per_list() {
        let old = AdminLists {
            operators: vec![entry("A")],
            whitelist: vec![entry("W")],
            bans: vec![Ban {
                target: entry("X"),
                created: UNIX_EPOCH,
                source: "s".to_owned(),
                expires: None,
                reason: None,
            }],
            ip_bans: vec![Ban {
                target: "10.0.0.1".parse().unwrap(),
                created: UNIX_EPOCH,
                source: "s".to_owned(),
                expires: None,
                reason: None,
            }],
            ..AdminLists::default()
        };
        let notices = admin_diff(&old, &AdminLists::default(), 4);
        assert_eq!(
            notices,
            vec![
                ManagementNotice::OperatorRemoved(operator_view(&entry("A"), 4)),
                ManagementNotice::AllowlistRemoved(PlayerRef::named("W")),
                ManagementNotice::BanRemoved(PlayerRef::named("X")),
                ManagementNotice::IpBanRemoved("10.0.0.1".to_owned()),
            ]
        );
    }

    fn setting(
        server: &mut Server<crate::end_tests::Tap>,
        setting: Setting,
        value: Option<SettingValue>,
    ) -> ManagementReply {
        let (reply, answer) = crossbeam_channel::bounded(1);
        server.handle_management(ManagementCall {
            request: ManagementRequest::Setting { setting, value },
            reply,
        });
        answer.try_recv().expect("answered")
    }

    #[test]
    fn distances_are_settable_between_two_and_thirty_two() {
        let (mut server, _, _) = crate::end_tests::arrive();
        assert_eq!(
            setting(
                &mut server,
                Setting::ViewDistance,
                Some(SettingValue::Int(12))
            ),
            ManagementReply::Setting(SettingValue::Int(12))
        );
        assert_eq!(server.config.view_distance, 12);
        assert!(matches!(
            setting(
                &mut server,
                Setting::SimulationDistance,
                Some(SettingValue::Int(1))
            ),
            ManagementReply::Fault(ManagementFault::InvalidParams(_))
        ));
    }

    #[test]
    fn use_and_enforce_allowlist_are_two_settings() {
        let (mut server, _, _) = crate::end_tests::arrive();
        setting(
            &mut server,
            Setting::EnforceAllowlist,
            Some(SettingValue::Bool(true)),
        );
        assert_eq!(
            setting(&mut server, Setting::UseAllowlist, None),
            ManagementReply::Setting(SettingValue::Bool(false))
        );
        assert_eq!(
            setting(&mut server, Setting::EnforceAllowlist, None),
            ManagementReply::Setting(SettingValue::Bool(true))
        );
    }

    #[test]
    fn startup_settings_read_back_and_refuse_a_change() {
        let (mut server, _, _) = crate::end_tests::arrive();
        server.config.motd = "Hello".to_owned();
        assert_eq!(
            setting(&mut server, Setting::Motd, None),
            ManagementReply::Setting(SettingValue::Text("Hello".to_owned()))
        );
        assert!(matches!(
            setting(
                &mut server,
                Setting::Motd,
                Some(SettingValue::Text("x".to_owned()))
            ),
            ManagementReply::Fault(ManagementFault::Failed(_))
        ));
    }

    #[test]
    fn activity_is_announced_once_per_interval() {
        let (server, _, _) = crate::end_tests::arrive();
        let (sink, notices) = crossbeam_channel::unbounded();
        let mut server = server.with_management_sink(sink);
        server.note_management_activity();
        server.note_management_activity();
        assert_eq!(notices.try_iter().count(), 1);
    }

    #[test]
    fn identical_lists_yield_no_notice() {
        let lists = AdminLists {
            whitelist: vec![entry("W")],
            ..AdminLists::default()
        };
        assert!(admin_diff(&lists, &lists.clone(), 4).is_empty());
    }
}
