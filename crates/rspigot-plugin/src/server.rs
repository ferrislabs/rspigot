use std::fmt;
use std::marker::PhantomData;
use std::net::IpAddr;
use std::time::SystemTime;

use rspigot_plugin_protocol::v1::server_order::Order;
use rspigot_plugin_protocol::v1::{self, answer, from_plugin, question};
use uuid::Uuid;

use crate::Error;
use crate::custom_inventory::CustomInventory;
use crate::link::Link;
use crate::player::Player;
use crate::scoreboard::ScoreboardManager;
use crate::wire::{from_millis, millis, parse_address, parse_uuid};
use crate::world::World;

#[derive(Debug, Clone)]
pub struct Server {
    pub players: Players,
    pub whitelist: Whitelist,
    pub operators: Operators,
    pub bans: Bans,
    pub ip_bans: IpBans,
    link: Link,
}

impl Server {
    pub(crate) fn new(link: Link) -> Self {
        Self {
            players: Players { link: link.clone() },
            whitelist: NameList::new(link.clone()),
            operators: NameList::new(link.clone()),
            bans: BanList::new(link.clone()),
            ip_bans: BanList::new(link.clone()),
            link,
        }
    }

    pub fn broadcast(&self, text: impl Into<String>) {
        order(&self.link, Order::Broadcast(text.into()));
    }

    pub fn dispatch_command(&self, line: impl Into<String>) {
        order(&self.link, Order::DispatchCommand(line.into()));
    }

    pub fn shutdown(&self) {
        order(&self.link, Order::Shutdown(v1::Empty {}));
    }

    pub fn save(&self) {
        order(&self.link, Order::Save(v1::Empty {}));
    }

    pub fn set_whitelist_enabled(&self, enabled: bool) {
        order(&self.link, Order::SetWhitelistEnabled(enabled));
    }

    pub fn world(&self) -> World {
        World::new(self.link.clone())
    }

    pub fn scoreboard_manager(&self) -> ScoreboardManager {
        ScoreboardManager::new(self.link.clone())
    }

    pub fn create_inventory(&self, rows: u8, title: impl Into<String>) -> CustomInventory {
        CustomInventory::create(&self.link, rows, title.into(), true)
    }

    pub fn create_storage(&self, rows: u8, title: impl Into<String>) -> CustomInventory {
        CustomInventory::create(&self.link, rows, title.into(), false)
    }

    pub async fn resolve(&self) -> Result<ServerState, Error> {
        let state = self
            .link
            .ask(
                question::Question::Server(v1::Empty {}),
                "a server state",
                |body| match body {
                    answer::Body::Server(state) => Some(state),
                    _ => None,
                },
            )
            .await?;
        ServerState::from_wire(state)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ServerState {
    pub max_players: u32,
    pub online: u32,
    pub tps: [f64; 3],
    pub whitelist_enabled: bool,
}

impl ServerState {
    fn from_wire(state: v1::ServerState) -> Result<Self, Error> {
        let count = |count: i32| {
            u32::try_from(count).map_err(|_| Error::Malformed(format!("a player count {count}")))
        };
        Ok(Self {
            max_players: count(state.max_players)?,
            online: count(state.online)?,
            tps: [state.tps_1m, state.tps_5m, state.tps_15m],
            whitelist_enabled: state.whitelist_enabled,
        })
    }
}

#[derive(Debug, Clone)]
pub struct Players {
    link: Link,
}

impl Players {
    pub async fn fetch(&self) -> Result<Vec<Player>, Error> {
        players(&self.link, question::Question::Players(v1::Empty {})).await
    }

    pub fn named(&self, name: impl Into<String>) -> NamedPlayers {
        NamedPlayers {
            name: name.into(),
            link: self.link.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct NamedPlayers {
    name: String,
    link: Link,
}

impl NamedPlayers {
    pub async fn fetch(&self) -> Result<Vec<Player>, Error> {
        players(
            &self.link,
            question::Question::PlayerNamed(self.name.clone()),
        )
        .await
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListedPlayer {
    pub name: String,
    pub uuid: Option<Uuid>,
}

impl ListedPlayer {
    fn from_wire(listed: v1::ListedPlayer) -> Result<Self, Error> {
        Ok(Self {
            uuid: listed
                .uuid
                .as_deref()
                .map(|text| parse_uuid(text, "a player"))
                .transpose()?,
            name: listed.name,
        })
    }
}

pub trait NameListKind: sealed::NameWire {}

#[derive(Debug, Clone, Copy)]
pub enum WhitelistKind {}

#[derive(Debug, Clone, Copy)]
pub enum OperatorKind {}

impl NameListKind for WhitelistKind {}

impl NameListKind for OperatorKind {}

impl sealed::NameWire for WhitelistKind {
    const ADD: fn(String) -> Order = Order::WhitelistAdd;
    const REMOVE: fn(String) -> Order = Order::WhitelistRemove;
    const QUESTION: question::Question = question::Question::Whitelist(v1::Empty {});
}

impl sealed::NameWire for OperatorKind {
    const ADD: fn(String) -> Order = Order::OperatorAdd;
    const REMOVE: fn(String) -> Order = Order::OperatorRemove;
    const QUESTION: question::Question = question::Question::Operators(v1::Empty {});
}

pub type Whitelist = NameList<WhitelistKind>;

pub type Operators = NameList<OperatorKind>;

#[derive(Debug, Clone)]
pub struct NameList<K> {
    link: Link,
    kind: PhantomData<K>,
}

impl<K: NameListKind> NameList<K> {
    const fn new(link: Link) -> Self {
        Self {
            link,
            kind: PhantomData,
        }
    }

    pub fn add(&self, name: impl Into<String>) {
        order(&self.link, K::ADD(name.into()));
    }

    pub fn remove(&self, name: impl Into<String>) {
        order(&self.link, K::REMOVE(name.into()));
    }

    pub async fn fetch(&self) -> Result<Vec<ListedPlayer>, Error> {
        let list = self
            .link
            .ask(K::QUESTION, "a list of names", |body| match body {
                answer::Body::Listed(list) => Some(list),
                _ => None,
            })
            .await?;
        list.players
            .into_iter()
            .map(ListedPlayer::from_wire)
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BanEntry {
    pub target: String,
    pub uuid: Option<Uuid>,
    pub created: SystemTime,
    pub source: String,
    pub expires: Option<SystemTime>,
    pub reason: Option<String>,
}

impl sealed::EntryWire for BanEntry {
    fn from_wire(entry: v1::BanEntry) -> Result<Self, Error> {
        Ok(Self {
            uuid: entry
                .uuid
                .as_deref()
                .map(|text| parse_uuid(text, "a player"))
                .transpose()?,
            created: from_millis(entry.created_ms)?,
            expires: entry.expires_ms.map(from_millis).transpose()?,
            target: entry.target,
            source: entry.source,
            reason: entry.reason,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IpBanEntry {
    pub address: IpAddr,
    pub created: SystemTime,
    pub source: String,
    pub expires: Option<SystemTime>,
    pub reason: Option<String>,
}

impl sealed::EntryWire for IpBanEntry {
    fn from_wire(entry: v1::BanEntry) -> Result<Self, Error> {
        Ok(Self {
            address: parse_address(&entry.target)?,
            created: from_millis(entry.created_ms)?,
            expires: entry.expires_ms.map(from_millis).transpose()?,
            source: entry.source,
            reason: entry.reason,
        })
    }
}

pub trait BanListKind: sealed::BanWire {
    type Target: fmt::Display;
    type Entry: sealed::EntryWire;
}

#[derive(Debug, Clone, Copy)]
pub enum ByName {}

#[derive(Debug, Clone, Copy)]
pub enum ByAddress {}

impl BanListKind for ByName {
    type Target = String;
    type Entry = BanEntry;
}

impl BanListKind for ByAddress {
    type Target = IpAddr;
    type Entry = IpBanEntry;
}

impl sealed::BanWire for ByName {
    const ADD: fn(v1::Ban) -> Order = Order::BanAdd;
    const REMOVE: fn(String) -> Order = Order::BanRemove;
    const QUESTION: question::Question = question::Question::Bans(v1::Empty {});
}

impl sealed::BanWire for ByAddress {
    const ADD: fn(v1::Ban) -> Order = Order::IpBanAdd;
    const REMOVE: fn(String) -> Order = Order::IpBanRemove;
    const QUESTION: question::Question = question::Question::IpBans(v1::Empty {});
}

pub type Bans = BanList<ByName>;

pub type IpBans = BanList<ByAddress>;

#[derive(Debug, Clone)]
pub struct BanList<K> {
    link: Link,
    kind: PhantomData<K>,
}

impl<K: BanListKind> BanList<K> {
    const fn new(link: Link) -> Self {
        Self {
            link,
            kind: PhantomData,
        }
    }

    pub fn add(
        &self,
        target: impl Into<K::Target>,
        reason: Option<String>,
        expires: Option<SystemTime>,
    ) {
        order(
            &self.link,
            K::ADD(v1::Ban {
                target: target.into().to_string(),
                reason,
                expires_ms: expires.map(millis),
            }),
        );
    }

    pub fn remove(&self, target: impl Into<K::Target>) {
        order(&self.link, K::REMOVE(target.into().to_string()));
    }

    pub async fn fetch(&self) -> Result<Vec<K::Entry>, Error> {
        let list = self
            .link
            .ask(K::QUESTION, "a ban list", |body| match body {
                answer::Body::Bans(list) => Some(list),
                _ => None,
            })
            .await?;
        list.entries
            .into_iter()
            .map(sealed::EntryWire::from_wire)
            .collect()
    }
}

mod sealed {
    use rspigot_plugin_protocol::v1::server_order::Order;
    use rspigot_plugin_protocol::v1::{self, question};

    use crate::Error;

    pub trait NameWire {
        const ADD: fn(String) -> Order;
        const REMOVE: fn(String) -> Order;
        const QUESTION: question::Question;
    }

    pub trait BanWire {
        const ADD: fn(v1::Ban) -> Order;
        const REMOVE: fn(String) -> Order;
        const QUESTION: question::Question;
    }

    pub trait EntryWire: Sized {
        fn from_wire(entry: v1::BanEntry) -> Result<Self, Error>;
    }
}

async fn players(link: &Link, question: question::Question) -> Result<Vec<Player>, Error> {
    let list = link
        .ask(question, "a player list", |body| match body {
            answer::Body::Players(list) => Some(list),
            _ => None,
        })
        .await?;
    list.players
        .into_iter()
        .map(|player| Player::from_wire(Some(player), link))
        .collect()
}

fn order(link: &Link, order: Order) {
    link.order(from_plugin::Body::Server(v1::ServerOrder {
        order: Some(order),
    }));
}
