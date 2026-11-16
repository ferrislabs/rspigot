use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use rspigot_core::events::{Inbound, PreLoginVerdict};
use rspigot_core::ports::SessionVerifier;
use rspigot_core::status::{PingHook, StatusPing, StatusReply, StatusSnapshot};
use rspigot_domain::player::{ClientSettings, GameProfile, MainHand, PlayerId, ProfileProperty};
use rspigot_domain::text::Component;
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
    let settings = configure(&shared, &mut session).await?;
    play::run(shared, session, profile, settings).await
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

fn update_tags() -> Result<UpdateTags, ConnectionError> {
    let mut registries = Vec::with_capacity(rspigot_data::tags::TAGS.len());
    for registry in rspigot_data::tags::TAGS {
        let mut tags = Vec::with_capacity(registry.tags.len());
        for (name, entries) in registry.tags {
            tags.push(NetworkTag {
                name: identifier(name)?,
                entries: entries.iter().map(|e| *e as i32).collect(),
            });
        }
        registries.push(RegistryTags {
            registry: identifier(registry.registry)?,
            tags,
        });
    }
    Ok(UpdateTags { registries })
}

async fn configure<V: SessionVerifier>(
    shared: &Shared<V>,
    session: &mut Session,
) -> Result<ClientSettings, ConnectionError> {
    use ids::configuration::clientbound as cb;
    let writer = &mut session.writer;
    writer.queue(
        cb::CUSTOM_PAYLOAD,
        &ClientboundCustomPayload {
            channel: identifier("minecraft:brand")?,
            data: brand_payload(&shared.config.brand),
        },
    )?;
    writer.queue(
        cb::UPDATE_ENABLED_FEATURES,
        &UpdateEnabledFeatures {
            features: vec![identifier("minecraft:vanilla")?],
        },
    )?;
    let core = rspigot_data::KNOWN_PACK;
    writer.queue(
        cb::SELECT_KNOWN_PACKS,
        &ClientboundSelectKnownPacks {
            packs: vec![KnownPack {
                namespace: core.namespace.to_owned(),
                id: core.id.to_owned(),
                version: core.version.to_owned(),
            }],
        },
    )?;
    writer.flush().await?;
    let mut settings = ClientSettings::default();
    let mut registries_sent = false;
    let mut conduct_pending = false;
    loop {
        let packet = session.reader.next().await?;
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
                    send_configuration_disconnect(session, &reason).await?;
                    return Err(ConnectionError::Rejected(
                        "client lacks minecraft:core".to_owned(),
                    ));
                }
                send_registries(session).await?;
                registries_sent = true;
                if let Some(text) = &shared.config.code_of_conduct {
                    session
                        .writer
                        .send(cb::CODE_OF_CONDUCT, &CodeOfConduct { text: text.clone() })
                        .await?;
                    conduct_pending = true;
                } else {
                    session
                        .writer
                        .send(cb::FINISH_CONFIGURATION, &ClientboundFinishConfiguration)
                        .await?;
                }
            }
            ServerboundConfiguration::AcceptCodeOfConduct(_) if conduct_pending => {
                conduct_pending = false;
                session
                    .writer
                    .send(cb::FINISH_CONFIGURATION, &ClientboundFinishConfiguration)
                    .await?;
            }
            ServerboundConfiguration::FinishConfiguration(_)
                if registries_sent && !conduct_pending =>
            {
                return Ok(settings);
            }
            ServerboundConfiguration::KeepAlive(KeepAlive { .. }) => {}
            _ => {}
        }
    }
}

async fn send_configuration_disconnect(
    session: &mut Session,
    reason: &Component,
) -> Result<(), ConnectionError> {
    let packet = rspigot_protocol::common::Disconnect {
        reason: rspigot_protocol::types::Text(crate::text::to_nbt(reason)),
    };
    session
        .writer
        .send(ids::configuration::clientbound::DISCONNECT, &packet)
        .await?;
    session.writer.shutdown().await;
    Ok(())
}

async fn send_registries(session: &mut Session) -> Result<(), ConnectionError> {
    for registry in rspigot_data::registries::SYNCED {
        let entries = registry
            .entries
            .iter()
            .map(|entry| {
                Ok(RegistryEntry {
                    id: identifier(entry)?,
                    data: None,
                })
            })
            .collect::<Result<Vec<_>, ConnectionError>>()?;
        let packet = RegistryData {
            registry: identifier(registry.id)?,
            entries,
        };
        session
            .writer
            .queue(ids::configuration::clientbound::REGISTRY_DATA, &packet)?;
    }
    session.writer.queue(
        ids::configuration::clientbound::UPDATE_TAGS,
        &update_tags()?,
    )?;
    session.writer.flush().await
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
