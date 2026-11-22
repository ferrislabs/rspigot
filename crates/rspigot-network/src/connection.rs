use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use uuid::Uuid;

use rspigot_core::events::{Inbound, PreLoginVerdict};
use rspigot_core::plugins::events::ResourcePackStatus;
use rspigot_core::ports::SessionVerifier;
use rspigot_core::status::{PingHook, StatusPing, StatusReply, StatusSnapshot};
use rspigot_data::tags::TagOverlay;
use rspigot_domain::player::{ClientSettings, GameProfile, MainHand, PlayerId, ProfileProperty};
use rspigot_domain::text::Component;
use rspigot_nbt::Tag;
use rspigot_protocol::codec::Encode;
use rspigot_protocol::common::{
    ClientInformation, ClientboundCustomPayload, Identifier, KeepAlive, NetworkTag, RegistryTags,
    UpdateTags,
};
use rspigot_protocol::configuration::{
    ClientboundFinishConfiguration, ClientboundSelectKnownPacks, CodeOfConduct, RegistryData,
    RegistryEntry, ServerboundConfiguration, UpdateEnabledFeatures,
};
use rspigot_protocol::frame::{Compression, FrameDecoder, FrameEncoder, FrameError};
use rspigot_protocol::handshake::{ClientIntent, Intention, ServerboundHandshake};
use rspigot_protocol::login::{
    ClientboundHello, CustomQuery, LoginCompression, LoginDisconnect, LoginFinished,
    ServerboundLogin,
};
use rspigot_protocol::status::{PongResponse, ServerboundStatus, StatusResponse};
use rspigot_protocol::types::{
    GameProfile as WireProfile, JsonText, KnownPack, ProfileProperty as WireProperty,
};
use rspigot_protocol::{DecodeError, ids, split_packet_id, write_packet_with_id};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};

use crate::NetworkConfig;
use crate::decode::pack_status;
use crate::link::NetLink;
use crate::listener::Shared;
use crate::play;
use crate::proxy::{self, Forwarded, ProxyForwarding};
use crate::status::{Shown, legacy_status, status_json};
use crate::text::to_json;

const STATUS_PATIENCE: Duration = Duration::from_secs(10);
const LEGACY_DRAIN: Duration = Duration::from_secs(1);

#[derive(Debug, thiserror::Error)]
pub enum ConnectionError {
    #[error("i/o: {0}")]
    Io(#[from] std::io::Error),
    #[error("frame: {0}")]
    Frame(#[from] FrameError),
    #[error("decode: {0}")]
    Decode(#[from] DecodeError),
    #[error("closed by peer")]
    Closed,
    #[error("{0}")]
    Rejected(String),
}

pub struct PacketReader {
    stream: OwnedReadHalf,
    pub decoder: FrameDecoder,
    buffer: Box<[u8]>,
}

impl PacketReader {
    pub async fn next(&mut self) -> Result<Vec<u8>, ConnectionError> {
        loop {
            if let Some(packet) = self.decoder.next_packet()? {
                return Ok(packet);
            }
            let read = self.stream.read(&mut self.buffer).await?;
            if read == 0 {
                return Err(ConnectionError::Closed);
            }
            self.decoder.feed(&self.buffer[..read]);
        }
    }
}

pub struct PacketWriter {
    stream: OwnedWriteHalf,
    pub encoder: FrameEncoder,
    body: Vec<u8>,
    pub pending: Vec<u8>,
}

impl PacketWriter {
    pub fn queue<P: Encode + ?Sized>(
        &mut self,
        id: i32,
        packet: &P,
    ) -> Result<(), ConnectionError> {
        self.body.clear();
        write_packet_with_id(id, packet, &mut self.body);
        self.encoder.encode_packet(&self.body, &mut self.pending)?;
        Ok(())
    }

    pub fn queue_prebuilt(&mut self, frame: &[u8]) {
        self.encoder.append_prebuilt(frame, &mut self.pending);
    }

    pub async fn flush(&mut self) -> Result<(), ConnectionError> {
        if !self.pending.is_empty() {
            self.stream.write_all(&self.pending).await?;
            self.pending.clear();
        }
        Ok(())
    }

    pub async fn send<P: Encode + ?Sized>(
        &mut self,
        id: i32,
        packet: &P,
    ) -> Result<(), ConnectionError> {
        self.queue(id, packet)?;
        self.flush().await
    }

    pub async fn shutdown(&mut self) {
        let _ = self.stream.shutdown().await;
    }
}

pub struct Session {
    pub reader: PacketReader,
    pub writer: PacketWriter,
    pub addr: SocketAddr,
}

fn identifier(raw: &str) -> Result<Identifier, ConnectionError> {
    Identifier::parse(raw).map_err(ConnectionError::from)
}

pub async fn handle<V: SessionVerifier>(
    shared: Arc<Shared<V>>,
    stream: TcpStream,
    addr: SocketAddr,
) -> Result<(), ConnectionError> {
    let mut first = [0u8; 1];
    let peeked = stream.peek(&mut first).await?;
    if peeked == 1 && first[0] == 0xFE {
        let mut stream = stream;
        let answer = answer_ping(&shared, addr.ip().to_canonical(), true, String::new).await;
        let bytes = legacy_status(&shared.config, &answer.shown(&shared.config));
        stream.write_all(&bytes).await?;
        stream.shutdown().await?;
        let mut rest = [0u8; 512];
        let drained =
            async { while matches!(stream.read(&mut rest).await, Ok(read) if read > 0) {} };
        let _ = tokio::time::timeout(LEGACY_DRAIN, drained).await;
        return Ok(());
    }
    let (read, write) = stream.into_split();
    let mut session = Session {
        reader: PacketReader {
            stream: read,
            decoder: FrameDecoder::new(),
            buffer: vec![0u8; 16 * 1024].into_boxed_slice(),
        },
        writer: PacketWriter {
            stream: write,
            encoder: FrameEncoder::new(),
            body: Vec::with_capacity(256),
            pending: Vec::with_capacity(16 * 1024),
        },
        addr,
    };
    let packet = session.reader.next().await?;
    let (id, body) = split_packet_id(&packet)?;
    let ServerboundHandshake::Intention(intention) = ServerboundHandshake::decode(id, body)?;
    match intention.intent {
        ClientIntent::Status => status(&shared, &mut session, &intention).await,
        ClientIntent::Login | ClientIntent::Transfer => {
            if intention.intent == ClientIntent::Transfer && !shared.config.accepts_transfers {
                return reject(
                    &mut session,
                    Component::translatable(
                        "multiplayer.disconnect.transfers_disabled",
                        Vec::new(),
                    ),
                )
                .await;
            }
            if intention.protocol_version != rspigot_data::PROTOCOL_VERSION {
                let template = if intention.protocol_version < rspigot_data::PROTOCOL_VERSION {
                    &shared.config.outdated_client_message
                } else {
                    &shared.config.outdated_server_message
                };
                let message = template.replace("{0}", rspigot_data::VERSION_NAME);
                return reject(&mut session, Component::text(message)).await;
            }
            let transferred = intention.intent == ClientIntent::Transfer;
            let bungee = if shared.config.proxy == ProxyForwarding::BungeeCord {
                match proxy::parse_bungee(&intention.host) {
                    Ok(forwarded) => Some(forwarded),
                    Err(_) => {
                        return reject(&mut session, Component::text(proxy::BUNGEE_MISSING)).await;
                    }
                }
            } else {
                None
            };
            login(shared, session, transferred, bungee).await
        }
    }
}

async fn reject(session: &mut Session, reason: Component) -> Result<(), ConnectionError> {
    let packet = LoginDisconnect {
        reason: JsonText(to_json(&reason)),
    };
    session
        .writer
        .send(ids::login::clientbound::LOGIN_DISCONNECT, &packet)
        .await?;
    session.writer.shutdown().await;
    Ok(())
}

enum Answer {
    Board(StatusSnapshot),
    Replied(StatusReply),
}

impl Answer {
    fn shown<'a>(&'a self, config: &'a NetworkConfig) -> Shown<'a> {
        match self {
            Self::Board(snapshot) => Shown::board(config, snapshot),
            Self::Replied(reply) => Shown::replied(reply),
        }
    }
}

async fn answer_ping<V>(
    shared: &Shared<V>,
    address: IpAddr,
    legacy: bool,
    hostname: impl FnOnce() -> String,
) -> Answer {
    let snapshot = shared.status.snapshot();
    let ping = || StatusPing {
        address,
        hostname: hostname(),
        motd: shared.config.motd.clone(),
        legacy,
    };
    match snapshot.ping {
        PingHook::Off => Answer::Board(snapshot),
        PingHook::Notify => {
            let _ = shared.game.send(Inbound::StatusPing {
                ping: ping(),
                reply: None,
            });
            Answer::Board(snapshot)
        }
        PingHook::Ask => match settled_ping(shared, ping()).await {
            Some(reply) => Answer::Replied(reply),
            None => Answer::Board(snapshot),
        },
    }
}

async fn settled_ping<V>(shared: &Shared<V>, ping: StatusPing) -> Option<StatusReply> {
    let (reply, answer) = futures_channel::oneshot::channel();
    shared
        .game
        .send(Inbound::StatusPing {
            ping,
            reply: Some(reply),
        })
        .ok()?;
    tokio::time::timeout(STATUS_PATIENCE, answer)
        .await
        .ok()?
        .ok()
}

async fn status<V: SessionVerifier>(
    shared: &Shared<V>,
    session: &mut Session,
    intention: &Intention,
) -> Result<(), ConnectionError> {
    loop {
        let packet = session.reader.next().await?;
        let (id, body) = split_packet_id(&packet)?;
        match ServerboundStatus::decode(id, body)? {
            ServerboundStatus::StatusRequest(_) => {
                if !shared.config.enable_status {
                    return Ok(());
                }
                let address = session.addr.ip().to_canonical();
                let hostname = || format!("{}:{}", intention.host, intention.port);
                let answer = answer_ping(shared, address, false, hostname).await;
                let json = status_json(&shared.config, &answer.shown(&shared.config));
                session
                    .writer
                    .send(
                        ids::status::clientbound::STATUS_RESPONSE,
                        &StatusResponse { json },
                    )
                    .await?;
            }
            ServerboundStatus::PingRequest(ping) => {
                session
                    .writer
                    .send(
                        ids::status::clientbound::PONG_RESPONSE,
                        &PongResponse { time: ping.time },
                    )
                    .await?;
                return Ok(());
            }
        }
    }
}

fn valid_name(name: &str) -> bool {
    (1..=16).contains(&name.len()) && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

async fn next_login(session: &mut Session) -> Result<ServerboundLogin, ConnectionError> {
    let packet = session.reader.next().await?;
    let (id, body) = split_packet_id(&packet)?;
    Ok(ServerboundLogin::decode(id, body)?)
}

async fn login<V: SessionVerifier>(
    shared: Arc<Shared<V>>,
    mut session: Session,
    transferred: bool,
    bungee: Option<Forwarded>,
) -> Result<(), ConnectionError> {
    let ServerboundLogin::Hello(hello) = next_login(&mut session).await? else {
        return Err(ConnectionError::Rejected("expected hello".to_owned()));
    };
    let forwarded = match (&shared.config.proxy, bungee) {
        (ProxyForwarding::Velocity(secret), _) => match velocity(&mut session, secret).await? {
            Some(forwarded) => Some(forwarded),
            None => return Ok(()),
        },
        (_, forwarded) => forwarded,
    };
    if let Some(forwarded) = &forwarded {
        session.addr.set_ip(forwarded.address);
    }
    if !valid_name(&hello.name) {
        return reject(
            &mut session,
            Component::translatable("multiplayer.disconnect.invalid_player_data", Vec::new()),
        )
        .await;
    }
    if shared.throttled(session.addr.ip()) {
        return reject(
            &mut session,
            Component::text("Connection throttled! Please wait before reconnecting."),
        )
        .await;
    }
    let profile = if let Some(forwarded) = forwarded {
        GameProfile {
            id: PlayerId(forwarded.id),
            name: forwarded.name.unwrap_or_else(|| hello.name.clone()),
            properties: forwarded.properties,
        }
    } else if shared.config.online_mode {
        match authenticate(&shared, &mut session, &hello.name).await? {
            Some(profile) => profile,
            None => return Ok(()),
        }
    } else {
        GameProfile {
            id: PlayerId::offline(&hello.name),
            name: hello.name.clone(),
            properties: Vec::new(),
        }
    };
    tracing::info!("UUID of player {} is {}", profile.name, profile.id);
    if let PreLoginVerdict::Refused(message) =
        pre_login(&shared, &profile, session.addr.ip(), transferred).await?
    {
        return reject(&mut session, Component::text(message)).await;
    }
    if let Some(threshold) = shared.config.compression_threshold {
        session
            .writer
            .send(
                ids::login::clientbound::LOGIN_COMPRESSION,
                &LoginCompression {
                    threshold: threshold as i32,
                },
            )
            .await?;
        session.writer.encoder.set_compression(Some(Compression {
            threshold,
            level: shared.config.compression_level,
        }));
        session.reader.decoder.set_compression(Some(threshold));
    }
    let finished = LoginFinished {
        profile: wire_profile(&profile),
        session_id: shared.session_id,
    };
    session
        .writer
        .send(ids::login::clientbound::LOGIN_FINISHED, &finished)
        .await?;
    loop {
        match next_login(&mut session).await? {
            ServerboundLogin::LoginAcknowledged(_) => break,
            ServerboundLogin::CustomQueryAnswer(_) | ServerboundLogin::CookieResponse(_) => {}
            _ => {
                return Err(ConnectionError::Rejected(
                    "unexpected login packet".to_owned(),
                ));
            }
        }
    }
    let configured = configure(&shared.config, &mut session).await?;
    play::run(shared, session, profile, configured).await
}

async fn velocity(
    session: &mut Session,
    secret: &[u8],
) -> Result<Option<Forwarded>, ConnectionError> {
    const TRANSACTION: i32 = 100;
    let query = CustomQuery {
        transaction_id: TRANSACTION,
        channel: identifier(proxy::VELOCITY_CHANNEL)?,
        data: vec![proxy::MAX_FORWARDING_VERSION],
    };
    session
        .writer
        .send(ids::login::clientbound::CUSTOM_QUERY, &query)
        .await?;
    loop {
        let ServerboundLogin::CustomQueryAnswer(answer) = next_login(session).await? else {
            return Err(ConnectionError::Rejected(
                "expected the forwarding answer".to_owned(),
            ));
        };
        if answer.transaction_id != TRANSACTION {
            continue;
        }
        let Some(payload) = answer.payload else {
            reject(session, Component::text(proxy::VELOCITY_REQUIRED)).await?;
            return Ok(None);
        };
        return match proxy::verify_velocity(secret, &payload) {
            Ok(forwarded) => Ok(Some(forwarded)),
            Err(error) => {
                tracing::warn!("Rejected forwarding data from {}: {error}", session.addr);
                reject(session, Component::text(proxy::VELOCITY_BAD_SIGNATURE)).await?;
                Ok(None)
            }
        };
    }
}

async fn pre_login<V>(
    shared: &Shared<V>,
    profile: &GameProfile,
    address: std::net::IpAddr,
    transferred: bool,
) -> Result<PreLoginVerdict, ConnectionError> {
    let (reply, verdict) = crossbeam_channel::bounded(1);
    shared
        .game
        .send(Inbound::PreLogin {
            name: profile.name.clone(),
            id: profile.id,
            address,
            transferred,
            reply,
        })
        .map_err(|_| ConnectionError::Closed)?;
    tokio::task::spawn_blocking(move || verdict.recv())
        .await
        .map_err(|e| ConnectionError::Rejected(e.to_string()))?
        .map_err(|_| ConnectionError::Closed)
}

async fn authenticate<V: SessionVerifier>(
    shared: &Arc<Shared<V>>,
    session: &mut Session,
    name: &str,
) -> Result<Option<GameProfile>, ConnectionError> {
    let challenge: [u8; 4] = rand::random();
    let hello = ClientboundHello {
        server_id: String::new(),
        public_key: shared.key.public_der().to_vec(),
        challenge: challenge.to_vec(),
        should_authenticate: true,
    };
    session
        .writer
        .send(ids::login::clientbound::HELLO, &hello)
        .await?;
    let ServerboundLogin::Key(key) = next_login(session).await? else {
        return Err(ConnectionError::Rejected("expected key".to_owned()));
    };
    let decrypted_challenge = shared
        .key
        .decrypt(&key.encrypted_challenge)
        .map_err(|e| ConnectionError::Rejected(e.to_string()))?;
    if decrypted_challenge != challenge {
        return Err(ConnectionError::Rejected("challenge mismatch".to_owned()));
    }
    let secret = shared
        .key
        .decrypt(&key.encrypted_secret)
        .map_err(|e| ConnectionError::Rejected(e.to_string()))?;
    let secret: [u8; 16] = secret
        .try_into()
        .map_err(|_| ConnectionError::Rejected("bad shared secret".to_owned()))?;
    session.writer.encoder.enable_encryption(&secret);
    session.reader.decoder.enable_encryption(&secret);
    let hash = rspigot_protocol::crypto::server_hash("", &secret, shared.key.public_der());
    let verifier = Arc::clone(&shared.verifier);
    let username = name.to_owned();
    let ip = shared
        .config
        .prevent_proxy_connections
        .then(|| session.addr.ip());
    let result = tokio::task::spawn_blocking(move || verifier.verify(&username, &hash, ip))
        .await
        .map_err(|e| ConnectionError::Rejected(e.to_string()))?;
    match result {
        Ok(profile) => Ok(Some(profile)),
        Err(error) => {
            tracing::info!("Disconnecting {name} ({}): {error}", session.addr);
            reject(
                session,
                Component::translatable("multiplayer.disconnect.unverified_username", Vec::new()),
            )
            .await?;
            Ok(None)
        }
    }
}

fn wire_profile(profile: &GameProfile) -> WireProfile {
    WireProfile {
        id: profile.id.0,
        name: profile.name.clone(),
        properties: profile
            .properties
            .iter()
            .map(|p: &ProfileProperty| WireProperty {
                name: p.name.clone(),
                value: p.value.clone(),
                signature: p.signature.clone(),
            })
            .collect(),
    }
}

pub fn client_settings(info: &ClientInformation) -> ClientSettings {
    ClientSettings {
        locale: info.language.clone(),
        view_distance: info.view_distance.clamp(2, 32) as u8,
        chat_colors: info.chat_colors,
        skin_parts: info.model_customisation,
        main_hand: match info.main_hand {
            rspigot_protocol::common::MainHand::Left => MainHand::Left,
            rspigot_protocol::common::MainHand::Right => MainHand::Right,
        },
    }
}

fn brand_payload(brand: &str) -> Vec<u8> {
    let mut data = Vec::with_capacity(brand.len() + 2);
    brand.encode(&mut data);
    data
}

pub(crate) fn update_tags() -> Result<UpdateTags, ConnectionError> {
    update_tags_with(rspigot_data::tags::published().as_deref())
}

fn network_tag(name: &str, entries: &[u32]) -> Result<NetworkTag, ConnectionError> {
    Ok(NetworkTag {
        name: identifier(name)?,
        entries: entries.iter().map(|e| *e as i32).collect(),
    })
}

fn update_tags_with(overlay: Option<&TagOverlay>) -> Result<UpdateTags, ConnectionError> {
    let mut registries = Vec::with_capacity(rspigot_data::tags::TAGS.len());
    for registry in rspigot_data::tags::TAGS {
        let changed = overlay.and_then(|overlay| {
            overlay
                .registries()
                .find(|(name, _)| *name == registry.registry)
                .map(|(_, tags)| tags)
        });
        let mut tags = Vec::with_capacity(registry.tags.len());
        for (name, entries) in registry.tags {
            let entries = changed
                .and_then(|changed| changed.get(*name))
                .map_or(*entries, |merged| *merged);
            tags.push(network_tag(name, entries)?);
        }
        if let Some(changed) = changed {
            for (name, entries) in changed {
                if rspigot_data::tags::static_tag(registry.registry, name).is_none() {
                    tags.push(network_tag(name, entries)?);
                }
            }
        }
        registries.push(RegistryTags {
            registry: identifier(registry.registry)?,
            tags,
        });
    }
    if let Some(overlay) = overlay {
        for (registry, changed) in overlay.registries() {
            if rspigot_data::tags::TAGS
                .iter()
                .any(|known| known.registry == registry)
            {
                continue;
            }
            let mut tags = Vec::with_capacity(changed.len());
            for (name, entries) in changed {
                tags.push(network_tag(name, entries)?);
            }
            registries.push(RegistryTags {
                registry: identifier(registry)?,
                tags,
            });
        }
    }
    Ok(UpdateTags { registries })
}

pub struct Configured {
    pub settings: ClientSettings,
    pub packs: Vec<(Uuid, ResourcePackStatus)>,
    pub inputs: Vec<rspigot_core::events::PlayerInput>,
}

pub(crate) trait ConfigWire {
    async fn read(&mut self) -> Result<Vec<u8>, ConnectionError>;
    fn queue<P: Encode + ?Sized>(&mut self, id: i32, packet: &P) -> Result<(), ConnectionError>;
    async fn flush(&mut self) -> Result<(), ConnectionError>;
    async fn close(&mut self);
}

impl ConfigWire for Session {
    async fn read(&mut self) -> Result<Vec<u8>, ConnectionError> {
        self.reader.next().await
    }

    fn queue<P: Encode + ?Sized>(&mut self, id: i32, packet: &P) -> Result<(), ConnectionError> {
        self.writer.queue(id, packet)
    }

    async fn flush(&mut self) -> Result<(), ConnectionError> {
        self.writer.flush().await
    }

    async fn close(&mut self) {
        self.writer.shutdown().await;
    }
}

pub(crate) struct Relay<'a> {
    pub reader: &'a mut PacketReader,
    pub link: &'a NetLink,
}

impl ConfigWire for Relay<'_> {
    async fn read(&mut self) -> Result<Vec<u8>, ConnectionError> {
        self.reader.next().await
    }

    fn queue<P: Encode + ?Sized>(&mut self, id: i32, packet: &P) -> Result<(), ConnectionError> {
        let mut body = Vec::new();
        packet.encode(&mut body);
        self.link.raw(id, body);
        Ok(())
    }

    async fn flush(&mut self) -> Result<(), ConnectionError> {
        Ok(())
    }

    async fn close(&mut self) {
        self.link.close();
    }
}

pub(crate) struct Raw<'a>(pub &'a [u8]);

impl Encode for Raw<'_> {
    fn encode(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(self.0);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    Known,
    Conduct,
    Pack,
    Finish,
}

fn terminal(status: ResourcePackStatus) -> bool {
    !matches!(
        status,
        ResourcePackStatus::Accepted | ResourcePackStatus::Downloaded
    )
}

fn pack_failed(status: ResourcePackStatus) -> bool {
    matches!(
        status,
        ResourcePackStatus::Declined
            | ResourcePackStatus::FailedDownload
            | ResourcePackStatus::InvalidUrl
            | ResourcePackStatus::FailedReload
    )
}

async fn enter<W: ConfigWire>(
    config: &NetworkConfig,
    wire: &mut W,
    mut next: Stage,
) -> Result<Stage, ConnectionError> {
    use ids::configuration::clientbound as cb;
    if next == Stage::Conduct {
        if let Some(text) = &config.code_of_conduct {
            wire.queue(cb::CODE_OF_CONDUCT, &CodeOfConduct { text: text.clone() })?;
            wire.flush().await?;
            return Ok(Stage::Conduct);
        }
        next = Stage::Pack;
    }
    if next == Stage::Pack
        && let Some(offer) = &config.resource_pack
    {
        wire.queue(cb::RESOURCE_PACK_PUSH, &crate::extras::wire_pack(offer))?;
        wire.flush().await?;
        return Ok(Stage::Pack);
    }
    wire.queue(cb::FINISH_CONFIGURATION, &ClientboundFinishConfiguration)?;
    wire.flush().await?;
    Ok(Stage::Finish)
}

pub(crate) async fn configure<W: ConfigWire>(
    config: &NetworkConfig,
    wire: &mut W,
) -> Result<Configured, ConnectionError> {
    use ids::configuration::clientbound as cb;
    wire.queue(
        cb::CUSTOM_PAYLOAD,
        &ClientboundCustomPayload {
            channel: identifier("minecraft:brand")?,
            data: brand_payload(&config.brand),
        },
    )?;
    if !config.server_links.is_empty() {
        wire.queue(
            cb::SERVER_LINKS,
            &crate::extras::wire_links(&config.server_links),
        )?;
    }
    wire.queue(
        cb::UPDATE_ENABLED_FEATURES,
        &UpdateEnabledFeatures {
            features: vec![identifier("minecraft:vanilla")?],
        },
    )?;
    let core = rspigot_data::KNOWN_PACK;
    wire.queue(
        cb::SELECT_KNOWN_PACKS,
        &ClientboundSelectKnownPacks {
            packs: vec![KnownPack {
                namespace: core.namespace.to_owned(),
                id: core.id.to_owned(),
                version: core.version.to_owned(),
            }],
        },
    )?;
    wire.flush().await?;
    let mut settings = ClientSettings::default();
    let mut packs = Vec::new();
    let mut inputs = Vec::new();
    let mut registries_sent = false;
    let mut stage = Stage::Known;
    loop {
        let packet = wire.read().await?;
        let (id, body) = split_packet_id(&packet)?;
        match ServerboundConfiguration::decode(id, body)? {
            ServerboundConfiguration::ClientInformation(info) => settings = client_settings(&info),
            ServerboundConfiguration::SelectKnownPacks(selected) if !registries_sent => {
                let knows_core = selected.packs.iter().any(|p| {
                    p.namespace == core.namespace && p.id == core.id && p.version == core.version
                });
                if !knows_core {
                    let reason = Component::text(format!(
                        "This server requires the vanilla {} data (minecraft:core).",
                        rspigot_data::VERSION_NAME
                    ));
                    send_configuration_disconnect(wire, &reason).await?;
                    return Err(ConnectionError::Rejected(
                        "client lacks minecraft:core".to_owned(),
                    ));
                }
                send_registries(wire).await?;
                registries_sent = true;
                stage = enter(config, wire, Stage::Conduct).await?;
            }
            ServerboundConfiguration::AcceptCodeOfConduct(_) if stage == Stage::Conduct => {
                stage = enter(config, wire, Stage::Pack).await?;
            }
            ServerboundConfiguration::ResourcePack(answer) => {
                let status = pack_status(answer.action);
                packs.push((answer.id, status));
                let required = config
                    .resource_pack
                    .as_ref()
                    .is_some_and(|pack| pack.required);
                if stage == Stage::Pack && terminal(status) {
                    if required && pack_failed(status) {
                        let reason = Component::translatable(
                            rspigot_core::client_extras::REQUIRED_PACK_DISCONNECT,
                            Vec::new(),
                        );
                        send_configuration_disconnect(wire, &reason).await?;
                        return Err(ConnectionError::Rejected(
                            "required resource pack refused".to_owned(),
                        ));
                    }
                    stage = enter(config, wire, Stage::Finish).await?;
                }
            }
            ServerboundConfiguration::FinishConfiguration(_) if stage == Stage::Finish => {
                return Ok(Configured {
                    settings,
                    packs,
                    inputs,
                });
            }
            ServerboundConfiguration::KeepAlive(KeepAlive { .. }) => {}
            other => inputs.extend(crate::decode::configuration_input(other)),
        }
    }
}

pub(crate) async fn reconfigure<V>(
    shared: &Shared<V>,
    reader: &mut PacketReader,
    link: &NetLink,
) -> Result<Configured, ConnectionError> {
    let mut relay = Relay { reader, link };
    configure(&shared.config, &mut relay).await
}

async fn send_configuration_disconnect<W: ConfigWire>(
    wire: &mut W,
    reason: &Component,
) -> Result<(), ConnectionError> {
    let packet = rspigot_protocol::common::Disconnect {
        reason: rspigot_protocol::types::Text(crate::text::to_nbt(reason)),
    };
    wire.queue(ids::configuration::clientbound::DISCONNECT, &packet)?;
    wire.flush().await?;
    wire.close().await;
    Ok(())
}

fn registry_entries(
    registry: &rspigot_data::registries::SyncedRegistry,
    overlay: Option<&rspigot_core::pack_data::registry_overlay::RegistryOverlay>,
) -> Result<Vec<RegistryEntry>, ConnectionError> {
    let replacement = |entry: &str| {
        overlay
            .and_then(|overlay| overlay.get(registry.id, entry))
            .and_then(|found| found.data.clone())
            .map(Tag::Compound)
    };
    let mut entries = registry
        .entries
        .iter()
        .map(|entry| {
            Ok(RegistryEntry {
                id: identifier(entry)?,
                data: replacement(entry),
            })
        })
        .collect::<Result<Vec<_>, ConnectionError>>()?;
    for added in overlay.map_or(&[][..], |overlay| overlay.entries(registry.id)) {
        if registry.entries.contains(&added.id.as_str()) {
            continue;
        }
        entries.push(RegistryEntry {
            id: identifier(&added.id)?,
            data: added.data.clone().map(Tag::Compound),
        });
    }
    Ok(entries)
}

async fn send_registries<W: ConfigWire>(wire: &mut W) -> Result<(), ConnectionError> {
    let overlay = rspigot_core::pack_data::registry_overlay::published();
    for registry in rspigot_data::registries::SYNCED {
        let packet = RegistryData {
            registry: identifier(registry.id)?,
            entries: registry_entries(registry, overlay.as_deref())?,
        };
        wire.queue(ids::configuration::clientbound::REGISTRY_DATA, &packet)?;
    }
    wire.queue(
        ids::configuration::clientbound::UPDATE_TAGS,
        &update_tags()?,
    )?;
    wire.flush().await
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Script {
        incoming: std::collections::VecDeque<Vec<u8>>,
        sent: Vec<(i32, Vec<u8>)>,
        closed: bool,
    }

    impl Script {
        fn new() -> Self {
            Self {
                incoming: std::collections::VecDeque::new(),
                sent: Vec::new(),
                closed: false,
            }
        }

        fn says<P: Encode>(&mut self, id: i32, packet: &P) {
            let mut bytes = Vec::new();
            rspigot_protocol::write_packet_with_id(id, packet, &mut bytes);
            self.incoming.push_back(bytes);
        }

        fn knows_core(&mut self) {
            let core = rspigot_data::KNOWN_PACK;
            self.says(
                ids::configuration::serverbound::SELECT_KNOWN_PACKS,
                &rspigot_protocol::configuration::ServerboundSelectKnownPacks {
                    packs: vec![KnownPack {
                        namespace: core.namespace.to_owned(),
                        id: core.id.to_owned(),
                        version: core.version.to_owned(),
                    }],
                },
            );
        }

        fn answers(&mut self, action: rspigot_protocol::common::ResourcePackAction) {
            self.says(
                ids::configuration::serverbound::RESOURCE_PACK,
                &rspigot_protocol::common::ResourcePackResponse {
                    id: Uuid::from_u128(5),
                    action,
                },
            );
        }

        fn finishes(&mut self) {
            self.says(
                ids::configuration::serverbound::FINISH_CONFIGURATION,
                &rspigot_protocol::configuration::ServerboundFinishConfiguration,
            );
        }

        fn ids(&self) -> Vec<i32> {
            self.sent.iter().map(|(id, _)| *id).collect()
        }
    }

    impl ConfigWire for Script {
        async fn read(&mut self) -> Result<Vec<u8>, ConnectionError> {
            self.incoming.pop_front().ok_or(ConnectionError::Closed)
        }

        fn queue<P: Encode + ?Sized>(
            &mut self,
            id: i32,
            packet: &P,
        ) -> Result<(), ConnectionError> {
            let mut body = Vec::new();
            packet.encode(&mut body);
            self.sent.push((id, body));
            Ok(())
        }

        async fn flush(&mut self) -> Result<(), ConnectionError> {
            Ok(())
        }

        async fn close(&mut self) {
            self.closed = true;
        }
    }

    fn pack_config(required: bool) -> crate::NetworkConfig {
        crate::NetworkConfig {
            resource_pack: Some(rspigot_core::client_extras::ResourcePackOffer {
                id: Uuid::from_u128(5),
                url: "https://example.org/p.zip".to_owned(),
                hash: "ab".to_owned(),
                required,
                prompt: None,
            }),
            ..crate::NetworkConfig::default()
        }
    }

    fn run<T>(future: impl std::future::Future<Output = T>) -> T {
        tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap()
            .block_on(future)
    }

    #[test]
    fn a_required_pack_holds_the_finish_until_it_is_loaded() {
        use ids::configuration::clientbound as cb;
        use rspigot_protocol::common::ResourcePackAction as Action;
        let config = pack_config(true);
        let mut wire = Script::new();
        wire.knows_core();
        wire.answers(Action::Accepted);
        wire.answers(Action::Downloaded);
        wire.answers(Action::SuccessfullyLoaded);
        wire.finishes();
        let configured = run(configure(&config, &mut wire)).unwrap();
        let sent = wire.ids();
        let push = sent
            .iter()
            .position(|id| *id == cb::RESOURCE_PACK_PUSH)
            .unwrap();
        let finish = sent
            .iter()
            .position(|id| *id == cb::FINISH_CONFIGURATION)
            .unwrap();
        assert!(push < finish);
        assert_eq!(
            sent.iter()
                .filter(|id| **id == cb::FINISH_CONFIGURATION)
                .count(),
            1
        );
        assert_eq!(configured.packs.len(), 3);
        assert_eq!(
            configured.packs[2],
            (Uuid::from_u128(5), ResourcePackStatus::SuccessfullyLoaded)
        );
    }

    #[test]
    fn configuration_cookie_click_and_brand_packets_are_kept_for_play() {
        use rspigot_core::events::PlayerInput;
        use rspigot_protocol::common::{
            CookieResponse, CustomClickAction, ServerboundCustomPayload,
        };
        let config = crate::NetworkConfig::default();
        let mut wire = Script::new();
        wire.knows_core();
        let mut brand = vec![6];
        brand.extend_from_slice(b"fabric");
        wire.says(
            ids::configuration::serverbound::CUSTOM_PAYLOAD,
            &ServerboundCustomPayload {
                channel: identifier("minecraft:brand").unwrap(),
                data: brand,
            },
        );
        wire.says(
            ids::configuration::serverbound::COOKIE_RESPONSE,
            &CookieResponse {
                key: identifier("a:b").unwrap(),
                payload: Some(vec![1, 2]),
            },
        );
        wire.says(
            ids::configuration::serverbound::CUSTOM_CLICK_ACTION,
            &CustomClickAction {
                id: identifier("shop:buy").unwrap(),
                payload: None,
            },
        );
        wire.finishes();
        let configured = run(configure(&config, &mut wire)).unwrap();
        assert_eq!(
            configured.inputs,
            vec![
                PlayerInput::ClientBrand("fabric".to_owned()),
                PlayerInput::CookieResponse {
                    key: "a:b".to_owned(),
                    payload: Some(vec![1, 2]),
                },
                PlayerInput::CustomClick {
                    id: "shop:buy".to_owned(),
                    data: None,
                },
            ]
        );
    }

    #[test]
    fn the_finish_is_not_sent_while_the_pack_is_only_accepted() {
        use ids::configuration::clientbound as cb;
        use rspigot_protocol::common::ResourcePackAction as Action;
        let config = pack_config(true);
        let mut wire = Script::new();
        wire.knows_core();
        wire.answers(Action::Accepted);
        wire.answers(Action::Downloaded);
        let result = run(configure(&config, &mut wire));
        assert!(matches!(result, Err(ConnectionError::Closed)));
        assert!(!wire.ids().contains(&cb::FINISH_CONFIGURATION));
    }

    #[test]
    fn a_declined_required_pack_is_kicked_with_the_vanilla_message() {
        use ids::configuration::clientbound as cb;
        use rspigot_protocol::common::ResourcePackAction as Action;
        let config = pack_config(true);
        let mut wire = Script::new();
        wire.knows_core();
        wire.answers(Action::Declined);
        let result = run(configure(&config, &mut wire));
        assert!(matches!(result, Err(ConnectionError::Rejected(_))));
        assert!(wire.closed);
        assert!(!wire.ids().contains(&cb::FINISH_CONFIGURATION));
        let (id, body) = wire.sent.last().unwrap();
        assert_eq!(*id, cb::DISCONNECT);
        let text = String::from_utf8_lossy(body);
        assert!(text.contains("multiplayer.requiredTexturePrompt.disconnect"));
    }

    #[test]
    fn a_failed_download_of_a_required_pack_is_kicked() {
        use rspigot_protocol::common::ResourcePackAction as Action;
        let config = pack_config(true);
        let mut wire = Script::new();
        wire.knows_core();
        wire.answers(Action::FailedDownload);
        assert!(matches!(
            run(configure(&config, &mut wire)),
            Err(ConnectionError::Rejected(_))
        ));
    }

    #[test]
    fn a_declined_optional_pack_goes_on_to_play() {
        use ids::configuration::clientbound as cb;
        use rspigot_protocol::common::ResourcePackAction as Action;
        let config = pack_config(false);
        let mut wire = Script::new();
        wire.knows_core();
        wire.answers(Action::Declined);
        wire.finishes();
        let configured = run(configure(&config, &mut wire)).unwrap();
        assert!(wire.ids().contains(&cb::FINISH_CONFIGURATION));
        assert!(!wire.closed);
        assert_eq!(
            configured.packs,
            vec![(Uuid::from_u128(5), ResourcePackStatus::Declined)]
        );
    }

    #[test]
    fn a_datapack_entry_overrides_or_extends_a_synced_registry() {
        use rspigot_core::pack_data::registry_overlay::{OverlayEntry, RegistryOverlay};
        let registry = rspigot_data::registries::SYNCED
            .iter()
            .find(|registry| registry.id == "minecraft:trim_pattern")
            .unwrap();
        let first = registry.entries[0];
        let mut overlay = RegistryOverlay::default();
        let data = |value: i32| Some(rspigot_nbt::Compound::new().with("v", value));
        overlay.insert(
            registry.id,
            OverlayEntry {
                id: first.to_owned(),
                json: String::new(),
                data: data(1),
            },
        );
        overlay.insert(
            registry.id,
            OverlayEntry {
                id: "t:added".to_owned(),
                json: String::new(),
                data: data(2),
            },
        );
        let plain = registry_entries(registry, None).unwrap();
        assert!(plain.iter().all(|entry| entry.data.is_none()));
        let entries = registry_entries(registry, Some(&overlay)).unwrap();
        assert_eq!(entries.len(), registry.entries.len() + 1);
        assert!(entries[0].data.is_some());
        assert!(
            entries[1..entries.len() - 1]
                .iter()
                .all(|e| e.data.is_none())
        );
        let last = entries.last().unwrap();
        assert_eq!(last.id.to_string(), "t:added");
        assert!(last.data.is_some());
    }

    #[test]
    fn without_a_pack_the_finish_follows_the_registries() {
        use ids::configuration::clientbound as cb;
        let config = crate::NetworkConfig::default();
        let mut wire = Script::new();
        wire.knows_core();
        wire.finishes();
        run(configure(&config, &mut wire)).unwrap();
        let sent = wire.ids();
        assert!(!sent.contains(&cb::RESOURCE_PACK_PUSH));
        assert_eq!(*sent.last().unwrap(), cb::FINISH_CONFIGURATION);
    }

    #[test]
    fn the_server_links_are_sent_before_the_features_with_the_bug_report_link() {
        use ids::configuration::clientbound as cb;
        use rspigot_core::client_extras::{KnownLink, LinkLabel, ServerLink};
        let config = crate::NetworkConfig {
            server_links: vec![ServerLink {
                label: LinkLabel::Known(KnownLink::BugReport),
                url: "x".to_owned(),
            }],
            ..crate::NetworkConfig::default()
        };
        let mut wire = Script::new();
        wire.knows_core();
        wire.finishes();
        run(configure(&config, &mut wire)).unwrap();
        let sent = wire.ids();
        let links = sent.iter().position(|id| *id == cb::SERVER_LINKS).unwrap();
        let features = sent
            .iter()
            .position(|id| *id == cb::UPDATE_ENABLED_FEATURES)
            .unwrap();
        assert!(links < features);
        assert_eq!(wire.sent[links].1, vec![1, 1, 0, 1, b'x']);
    }

    #[test]
    fn no_links_packet_is_sent_without_links() {
        use ids::configuration::clientbound as cb;
        let mut wire = Script::new();
        wire.knows_core();
        wire.finishes();
        run(configure(&crate::NetworkConfig::default(), &mut wire)).unwrap();
        assert!(!wire.ids().contains(&cb::SERVER_LINKS));
    }

    #[test]
    fn the_code_of_conduct_comes_before_the_pack() {
        use ids::configuration::clientbound as cb;
        use rspigot_protocol::common::ResourcePackAction as Action;
        let config = crate::NetworkConfig {
            code_of_conduct: Some("be nice".to_owned()),
            ..pack_config(false)
        };
        let mut wire = Script::new();
        wire.knows_core();
        wire.says(
            ids::configuration::serverbound::ACCEPT_CODE_OF_CONDUCT,
            &rspigot_protocol::configuration::AcceptCodeOfConduct,
        );
        wire.answers(Action::SuccessfullyLoaded);
        wire.finishes();
        run(configure(&config, &mut wire)).unwrap();
        let sent = wire.ids();
        let conduct = sent
            .iter()
            .position(|id| *id == cb::CODE_OF_CONDUCT)
            .unwrap();
        let push = sent
            .iter()
            .position(|id| *id == cb::RESOURCE_PACK_PUSH)
            .unwrap();
        assert!(conduct < push);
    }

    #[test]
    fn names_follow_vanilla_rules() {
        assert!(valid_name("Notch"));
        assert!(valid_name("a_b_9"));
        assert!(!valid_name("bad name"));
        assert!(!valid_name("seventeen_chars__"));
    }

    #[test]
    fn default_brand_is_rspigot() {
        let brand = crate::NetworkConfig::default().brand;
        assert_eq!(brand_payload(&brand), b"\x07Rspigot".to_vec());
    }

    #[test]
    fn tags_cover_blocks_and_items() {
        let tags = update_tags().unwrap();
        let names: Vec<&str> = tags
            .registries
            .iter()
            .map(|r| r.registry.as_str())
            .collect();
        assert!(names.contains(&"minecraft:block"));
        assert!(names.contains(&"minecraft:item"));
    }

    #[test]
    fn merged_tags_replace_static_ones_and_add_new_ones() {
        let stone =
            rspigot_data::registries::static_index("minecraft:block", "minecraft:stone").unwrap();
        let dirt =
            rspigot_data::registries::static_index("minecraft:block", "minecraft:dirt").unwrap();
        let mut overlay = TagOverlay::default();
        overlay.insert(
            "minecraft:block",
            "minecraft:mineable/pickaxe",
            Vec::leak(vec![dirt]),
        );
        overlay.insert(
            "minecraft:block",
            "t:brand_new",
            Vec::leak(vec![stone, dirt]),
        );
        let tags = update_tags_with(Some(&overlay)).unwrap();
        let blocks = tags
            .registries
            .iter()
            .find(|r| r.registry.as_str() == "minecraft:block")
            .unwrap();
        let find = |name: &str| blocks.tags.iter().find(|t| t.name.as_str() == name);
        assert_eq!(
            find("minecraft:mineable/pickaxe").unwrap().entries,
            vec![dirt as i32]
        );
        assert_eq!(
            find("t:brand_new").unwrap().entries,
            vec![stone as i32, dirt as i32]
        );
        assert!(find("minecraft:mineable/axe").unwrap().entries.len() > 1);
        let plain = update_tags_with(None).unwrap();
        let plain_blocks = plain
            .registries
            .iter()
            .find(|r| r.registry.as_str() == "minecraft:block")
            .unwrap();
        assert!(
            plain_blocks
                .tags
                .iter()
                .find(|t| t.name.as_str() == "minecraft:mineable/pickaxe")
                .unwrap()
                .entries
                .len()
                > 1
        );
        assert!(
            plain_blocks
                .tags
                .iter()
                .all(|t| t.name.as_str() != "t:brand_new")
        );
    }
}
