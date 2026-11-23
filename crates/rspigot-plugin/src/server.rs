use std::net::IpAddr;
use std::time::SystemTime;

use rspigot_plugin_protocol::v1::{self, answer, question};
use uuid::Uuid;

use crate::Error;
use crate::boss_bar::BossBars;
use crate::contract::Trade;
use crate::custom_inventory::{CustomInventory, InventoryType};
use crate::link::Link;
use crate::lookups::{ServerLootHandle, ServerProfilesHandle, ServerRegistriesHandle};
use crate::player::Player;
use crate::scheduler::Scheduler;
use crate::scoreboard::ScoreboardManager;
use crate::wire::{from_millis, parse_address, parse_uuid};
use crate::world::World;
use crate::{
    ServerBansHandle, ServerCommandHandle, ServerIpBansHandle, ServerLifecycleHandle,
    ServerMessageHandle, ServerOperatorsHandle, ServerWhitelistHandle,
};

#[derive(Debug, Clone)]
pub struct Server {
    pub bans: ServerBansHandle,
    pub boss_bars: BossBars,
    pub command: ServerCommandHandle,
    pub ip_bans: ServerIpBansHandle,
    pub lifecycle: ServerLifecycleHandle,
    pub loot: ServerLootHandle,
    pub profiles: ServerProfilesHandle,
    pub message: ServerMessageHandle,
    pub operators: ServerOperatorsHandle,
    pub permissions: crate::ServerPermissionsHandle,
    pub recipes: crate::ServerRecipesHandle,
    pub registries: ServerRegistriesHandle,
    pub scheduler: Scheduler,
    pub scoreboard_manager: ScoreboardManager,
    pub settings: crate::ServerSettingsHandle,
    pub whitelist: ServerWhitelistHandle,
    pub world: World,
    link: Link,
}

impl Server {
    pub(crate) fn new(link: Link) -> Self {
        Self {
            bans: ServerBansHandle::new(link.clone()),
            boss_bars: BossBars::new(link.clone()),
            command: ServerCommandHandle::new(link.clone()),
            ip_bans: ServerIpBansHandle::new(link.clone()),
            lifecycle: ServerLifecycleHandle::new(link.clone()),
            loot: ServerLootHandle::new(link.clone()),
            profiles: ServerProfilesHandle::new(link.clone()),
            message: ServerMessageHandle::new(link.clone()),
            operators: ServerOperatorsHandle::new(link.clone()),
            permissions: crate::ServerPermissionsHandle::new(link.clone()),
            recipes: crate::ServerRecipesHandle::new(link.clone()),
            registries: ServerRegistriesHandle::new(link.clone()),
            scheduler: Scheduler::new(link.clone()),
            scoreboard_manager: ScoreboardManager::new(link.clone()),
            settings: crate::ServerSettingsHandle::new(link.clone()),
            whitelist: ServerWhitelistHandle::new(link.clone()),
            world: World::new(link.clone()),
            link,
        }
    }

    pub async fn create_inventory(
        &self,
        rows: u8,
        title: impl Into<String>,
    ) -> Result<CustomInventory, Error> {
        CustomInventory::create(&self.link, rows, title.into(), true).await
    }

    pub async fn create_storage(
        &self,
        rows: u8,
        title: impl Into<String>,
    ) -> Result<CustomInventory, Error> {
        CustomInventory::create(&self.link, rows, title.into(), false).await
    }

    pub async fn create_inventory_of(
        &self,
        kind: InventoryType,
        title: impl Into<String>,
    ) -> Result<CustomInventory, Error> {
        let slots = if kind == InventoryType::Chest { 27 } else { 0 };
        CustomInventory::create_typed(&self.link, kind, slots, title.into(), true, &[], None).await
    }

    pub async fn create_chest(
        &self,
        slots: u16,
        title: impl Into<String>,
    ) -> Result<CustomInventory, Error> {
        CustomInventory::create_typed(
            &self.link,
            InventoryType::Chest,
            slots,
            title.into(),
            true,
            &[],
            None,
        )
        .await
    }

    pub async fn create_merchant(
        &self,
        title: impl Into<String>,
        trades: &[Trade],
    ) -> Result<CustomInventory, Error> {
        CustomInventory::create_typed(
            &self.link,
            InventoryType::Merchant,
            0,
            title.into(),
            false,
            trades,
            None,
        )
        .await
    }

    pub async fn create_merchant_with_progress(
        &self,
        title: impl Into<String>,
        trades: &[Trade],
        progress: crate::MerchantProgress,
    ) -> Result<CustomInventory, Error> {
        CustomInventory::create_typed(
            &self.link,
            InventoryType::Merchant,
            0,
            title.into(),
            false,
            trades,
            Some(progress),
        )
        .await
    }

    pub async fn get_players(&self) -> Result<Vec<Player>, Error> {
        players(&self.link, question::Question::Players(v1::Empty {})).await
    }

    pub async fn get_players_named(&self, name: impl Into<String>) -> Result<Vec<Player>, Error> {
        players(&self.link, question::Question::PlayerNamed(name.into())).await
    }

    pub async fn get_players_with_meta(&self, key: &str) -> Result<Vec<Player>, Error> {
        players(
            &self.link,
            question::Question::PlayersWithMeta(v1::EntitiesWithMeta {
                key: key.to_owned(),
                volatile: false,
            }),
        )
        .await
    }

    pub async fn get_players_with_volatile_meta(&self, key: &str) -> Result<Vec<Player>, Error> {
        players(
            &self.link,
            question::Question::PlayersWithMeta(v1::EntitiesWithMeta {
                key: key.to_owned(),
                volatile: true,
            }),
        )
        .await
    }

    pub async fn get_state(&self) -> Result<ServerState, Error> {
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

impl ServerWhitelistHandle {
    pub async fn get_names(&self) -> Result<Vec<ListedPlayer>, Error> {
        listed(&self.link, question::Question::Whitelist(v1::Empty {})).await
    }
}

impl ServerOperatorsHandle {
    pub async fn get_names(&self) -> Result<Vec<ListedPlayer>, Error> {
        listed(&self.link, question::Question::Operators(v1::Empty {})).await
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

impl BanEntry {
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

impl IpBanEntry {
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

impl ServerBansHandle {
    pub async fn get_entries(&self) -> Result<Vec<BanEntry>, Error> {
        banned(
            &self.link,
            question::Question::Bans(v1::Empty {}),
            BanEntry::from_wire,
        )
        .await
    }
}

impl ServerIpBansHandle {
    pub async fn get_entries(&self) -> Result<Vec<IpBanEntry>, Error> {
        banned(
            &self.link,
            question::Question::IpBans(v1::Empty {}),
            IpBanEntry::from_wire,
        )
        .await
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

async fn listed(link: &Link, question: question::Question) -> Result<Vec<ListedPlayer>, Error> {
    let list = link
        .ask(question, "a list of names", |body| match body {
            answer::Body::Listed(list) => Some(list),
            _ => None,
        })
        .await?;
    list.players
        .into_iter()
        .map(ListedPlayer::from_wire)
        .collect()
}

async fn banned<T>(
    link: &Link,
    question: question::Question,
    entry: fn(v1::BanEntry) -> Result<T, Error>,
) -> Result<Vec<T>, Error> {
    let list = link
        .ask(question, "a ban list", |body| match body {
            answer::Body::Bans(list) => Some(list),
            _ => None,
        })
        .await?;
    list.entries.into_iter().map(entry).collect()
}
