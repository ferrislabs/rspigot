use std::collections::VecDeque;

use rspigot_domain::Vec3;
use rspigot_domain::player::PlayerId;
use rspigot_domain::text::Component;
use rustc_hash::{FxHashMap, FxHashSet};
use uuid::Uuid;

use crate::events::{ClientEvent, PlayerInput};
use crate::plugins::events::ResourcePackStatus;
use crate::ports::ClientLink;
use crate::server::{Audience, Server};
use crate::streaming::ChunkStreamer;

pub const MAX_COOKIE_BYTES: usize = 5120;
pub const MAX_COOKIE_KEY_CHARS: usize = 32767;
pub const MAX_REPORT_DETAILS: usize = 32;
pub const MAX_REPORT_DETAIL_KEY_CHARS: usize = 128;
pub const MAX_REPORT_DETAIL_VALUE_CHARS: usize = 4096;
pub const MAX_COOKIE_REPLIES: usize = 1024;
pub const REQUIRED_PACK_DISCONNECT: &str = "multiplayer.requiredTexturePrompt.disconnect";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KnownLink {
    BugReport,
    CommunityGuidelines,
    Support,
    Status,
    Feedback,
    Community,
    Website,
    Forums,
    News,
    Announcements,
}

impl KnownLink {
    pub const ALL: [Self; 10] = [
        Self::BugReport,
        Self::CommunityGuidelines,
        Self::Support,
        Self::Status,
        Self::Feedback,
        Self::Community,
        Self::Website,
        Self::Forums,
        Self::News,
        Self::Announcements,
    ];

    pub const fn key(self) -> &'static str {
        match self {
            Self::BugReport => "bug_report",
            Self::CommunityGuidelines => "community_guidelines",
            Self::Support => "support",
            Self::Status => "status",
            Self::Feedback => "feedback",
            Self::Community => "community",
            Self::Website => "website",
            Self::Forums => "forums",
            Self::News => "news",
            Self::Announcements => "announcements",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|known| known.key() == key)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum LinkLabel {
    Known(KnownLink),
    Custom(Component),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ServerLink {
    pub label: LinkLabel,
    pub url: String,
}

impl ServerLink {
    pub fn labelled(label: &str, url: &str) -> Self {
        Self {
            label: KnownLink::from_key(label).map_or_else(
                || LinkLabel::Custom(Component::text(label)),
                LinkLabel::Known,
            ),
            url: url.to_owned(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResourcePackOffer {
    pub id: Uuid,
    pub url: String,
    pub hash: String,
    pub required: bool,
    pub prompt: Option<Component>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CookieReply {
    pub player: PlayerId,
    pub key: String,
    pub payload: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ClientExtraError {
    #[error("no such player")]
    UnknownPlayer,
    #[error("a cookie holds at most 5120 bytes")]
    CookieTooLarge,
    #[error("a cookie key is a lowercase identifier")]
    InvalidCookieKey,
    #[error("too many or too long report details")]
    ReportDetailsTooLarge,
    #[error("the player is already being reconfigured")]
    AlreadyReconfiguring,
}

#[derive(Default)]
pub(crate) struct Extras {
    required_packs: FxHashMap<PlayerId, FxHashSet<Uuid>>,
    asked_cookies: FxHashMap<PlayerId, FxHashSet<String>>,
    reconfiguring: FxHashSet<PlayerId>,
    cookie_replies: VecDeque<CookieReply>,
}

impl Extras {
    pub(crate) fn forget(&mut self, id: PlayerId) {
        self.required_packs.remove(&id);
        self.asked_cookies.remove(&id);
        self.reconfiguring.remove(&id);
    }

    pub(crate) fn paused(&self, id: PlayerId, input: &PlayerInput) -> bool {
        self.reconfiguring.contains(&id)
            && !matches!(
                input,
                PlayerInput::Reconfigured
                    | PlayerInput::ResourcePack { .. }
                    | PlayerInput::CookieResponse { .. }
            )
    }
}

pub fn parse_text_component(input: &str) -> Option<Component> {
    crate::commands::json_text::parse(input).ok()
}

fn valid_cookie_key(key: &str) -> bool {
    if key.is_empty() || key.len() > MAX_COOKIE_KEY_CHARS {
        return false;
    }
    let (namespace, path) = key.split_once(':').unwrap_or(("minecraft", key));
    let namespace_ok = namespace
        .bytes()
        .all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'_' | b'-' | b'.'));
    let path_ok = !path.is_empty()
        && path
            .bytes()
            .all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'_' | b'-' | b'.' | b'/'));
    namespace_ok && path_ok
}

fn failed(status: ResourcePackStatus) -> bool {
    matches!(
        status,
        ResourcePackStatus::Declined
            | ResourcePackStatus::FailedDownload
            | ResourcePackStatus::InvalidUrl
            | ResourcePackStatus::FailedReload
    )
}

impl<L: ClientLink> Server<L> {
    fn require_player(&self, player: PlayerId) -> Result<(), ClientExtraError> {
        if self.players.contains_key(&player) {
            Ok(())
        } else {
            Err(ClientExtraError::UnknownPlayer)
        }
    }

    pub fn send_resource_pack(
        &mut self,
        player: PlayerId,
        offer: ResourcePackOffer,
    ) -> Result<(), ClientExtraError> {
        self.require_player(player)?;
        let packs = self.extras.required_packs.entry(player).or_default();
        if offer.required {
            packs.insert(offer.id);
        } else {
            packs.remove(&offer.id);
        }
        self.push(Audience::One(player), ClientEvent::ResourcePackPush(offer));
        Ok(())
    }

    pub fn clear_resource_packs(
        &mut self,
        player: PlayerId,
        id: Option<Uuid>,
    ) -> Result<(), ClientExtraError> {
        self.require_player(player)?;
        match id {
            Some(id) => {
                if let Some(packs) = self.extras.required_packs.get_mut(&player) {
                    packs.remove(&id);
                }
            }
            None => {
                self.extras.required_packs.remove(&player);
            }
        }
        self.push(Audience::One(player), ClientEvent::ResourcePackPop(id));
        Ok(())
    }

    pub fn send_server_links(
        &mut self,
        player: PlayerId,
        links: Vec<ServerLink>,
    ) -> Result<(), ClientExtraError> {
        self.require_player(player)?;
        let described = crate::plugins::ServerLinks {
            links: links
                .iter()
                .map(|link| crate::plugins::ServerLinkEntry {
                    label: match &link.label {
                        LinkLabel::Known(known) => known.key().to_owned(),
                        LinkLabel::Custom(text) => text.plain_text(),
                    },
                    url: link.url.clone(),
                })
                .collect(),
        };
        self.push(Audience::One(player), ClientEvent::ServerLinks(links));
        self.fire(
            crate::plugins::Lane::Player(player),
            crate::plugins::Continuation::Notice(crate::plugins::EventKind::PlayerLinksSend),
            move |server| {
                Some(crate::plugins::EventPayload::PlayerLinksSend(
                    crate::plugins::events::PlayerLinksSend {
                        links: described,
                        player: server.snapshot_of(player)?,
                    },
                ))
            },
        );
        Ok(())
    }

    pub fn store_cookie(
        &mut self,
        player: PlayerId,
        key: &str,
        payload: Vec<u8>,
    ) -> Result<(), ClientExtraError> {
        self.require_player(player)?;
        if !valid_cookie_key(key) {
            return Err(ClientExtraError::InvalidCookieKey);
        }
        if payload.len() > MAX_COOKIE_BYTES {
            return Err(ClientExtraError::CookieTooLarge);
        }
        self.push(
            Audience::One(player),
            ClientEvent::StoreCookie {
                key: key.to_owned(),
                payload,
            },
        );
        Ok(())
    }

    pub fn request_cookie(&mut self, player: PlayerId, key: &str) -> Result<(), ClientExtraError> {
        self.require_player(player)?;
        if !valid_cookie_key(key) {
            return Err(ClientExtraError::InvalidCookieKey);
        }
        self.extras
            .asked_cookies
            .entry(player)
            .or_default()
            .insert(key.to_owned());
        self.push(
            Audience::One(player),
            ClientEvent::CookieRequest(key.to_owned()),
        );
        Ok(())
    }

    pub fn take_cookie_replies(&mut self) -> Vec<CookieReply> {
        self.extras.cookie_replies.drain(..).collect()
    }

    pub fn send_report_details(
        &mut self,
        player: PlayerId,
        details: Vec<(String, String)>,
    ) -> Result<(), ClientExtraError> {
        self.require_player(player)?;
        let fits = details.len() <= MAX_REPORT_DETAILS
            && details.iter().all(|(key, value)| {
                key.chars().count() <= MAX_REPORT_DETAIL_KEY_CHARS
                    && value.chars().count() <= MAX_REPORT_DETAIL_VALUE_CHARS
            });
        if !fits {
            return Err(ClientExtraError::ReportDetailsTooLarge);
        }
        self.push(Audience::One(player), ClientEvent::ReportDetails(details));
        Ok(())
    }

    pub fn reconfigure(&mut self, player: PlayerId) -> Result<(), ClientExtraError> {
        self.require_player(player)?;
        if !self.extras.reconfiguring.insert(player) {
            return Err(ClientExtraError::AlreadyReconfiguring);
        }
        self.push(Audience::One(player), ClientEvent::Reconfigure);
        Ok(())
    }

    pub fn is_reconfiguring(&self, player: PlayerId) -> bool {
        self.extras.reconfiguring.contains(&player)
    }

    pub fn player_position(&self, player: PlayerId) -> Option<Vec3> {
        self.players.get(&player).map(|p| p.position)
    }

    pub(crate) fn enforce_required_pack(
        &mut self,
        id: PlayerId,
        pack: Uuid,
        status: ResourcePackStatus,
    ) {
        let required = self
            .extras
            .required_packs
            .get(&id)
            .is_some_and(|packs| packs.contains(&pack));
        if required && failed(status) {
            self.disconnect_player(
                id,
                Component::translatable(REQUIRED_PACK_DISCONNECT, Vec::new()),
            );
        }
    }

    pub(crate) fn handle_cookie_response(
        &mut self,
        id: PlayerId,
        key: String,
        payload: Option<Vec<u8>>,
    ) {
        let asked = self
            .extras
            .asked_cookies
            .get_mut(&id)
            .is_some_and(|keys| keys.remove(&key));
        if !asked {
            tracing::debug!(%key, "dropping an unrequested cookie response");
            return;
        }
        if self.extras.cookie_replies.len() >= MAX_COOKIE_REPLIES {
            self.extras.cookie_replies.pop_front();
        }
        self.answer_cookie_waits(id, &key, payload.as_ref());
        self.extras.cookie_replies.push_back(CookieReply {
            player: id,
            key,
            payload,
        });
    }

    pub(crate) fn finish_reconfiguration(&mut self, id: PlayerId) {
        if !self.extras.reconfiguring.remove(&id) {
            return;
        }
        let Some(mut player) = self.players.remove(&id) else {
            return;
        };
        player.streamer.release_all(&mut self.world.chunks);
        for watched in player.tracked.drain() {
            if let Some(watching) = self.entities.viewers.get_mut(&watched) {
                watching.remove(&id);
            }
        }
        player.loaded = false;
        let view = u8::try_from(player.streamer.view_distance()).unwrap_or(u8::MAX);
        player.streamer = ChunkStreamer::new(self.config.stream, player.position.chunk(), view);
        self.send_join_sequence(&mut player);
        let entries: Vec<_> = self
            .players
            .values()
            .map(Self::list_entry)
            .chain(std::iter::once(Self::list_entry(&player)))
            .collect();
        player.link.send(ClientEvent::PlayerInfo(entries));
        player
            .streamer
            .start(&mut self.world.chunks, &self.workers, &player.link);
        self.players.insert(id, player);
        self.update_player_view(id);
        self.send_initial_advancements(id);
        self.send_commands(id);
    }
}
