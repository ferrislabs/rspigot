use rspigot_protocol::codec::{Decode, decode_string};
use rspigot_protocol::common::Disconnect;
use rspigot_protocol::ids::{configuration, play};
use rspigot_protocol::login::LoginAcknowledged;
use rspigot_protocol::play::chat::SystemChat;
use rspigot_protocol::play::player::StartConfiguration;
use rspigot_protocol::types::Text;
use rspigot_protocol::{Encode, VarInt, split_packet_id, write_packet_with_id};
use rspigot_proxy_core::{DisplayState, Phase, Server};
use tokio::time::timeout;

use crate::backend::{self, BackendLink};
use crate::forwarding::ForwardedPlayer;
use crate::status;
use crate::wire::{Reader, Writer};
use crate::{ProxyError, Shared};

const BRAND: &str = "minecraft:brand";

pub struct Link {
    pub server: Server,
    pub reader: Reader,
    pub writer: Writer,
}

enum Event {
    Client(Result<Vec<u8>, ProxyError>),
    Backend(Result<Vec<u8>, ProxyError>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Flow {
    Continue,
    Stop,
}

pub struct Session<'a, V> {
    shared: &'a Shared<V>,
    player: &'a ForwardedPlayer,
    client_reader: &'a mut Reader,
    client_writer: &'a mut Writer,
    link: Link,
    phase: Phase,
    display: DisplayState,
    client_information: Option<Vec<u8>>,
    brand: Option<Vec<u8>>,
    chat_session: Option<Vec<u8>>,
    replay_chat_session: bool,
}

impl<'a, V> Session<'a, V> {
    pub fn new(
        shared: &'a Shared<V>,
        player: &'a ForwardedPlayer,
        client_reader: &'a mut Reader,
        client_writer: &'a mut Writer,
        link: Link,
    ) -> Self {
        Self {
            shared,
            player,
            client_reader,
            client_writer,
            link,
            phase: Phase::Configuration,
            display: DisplayState::default(),
            client_information: None,
            brand: None,
            chat_session: None,
            replay_chat_session: false,
        }
    }

    pub async fn run(mut self) -> Result<(), ProxyError> {
        loop {
            let event = tokio::select! {
                packet = self.client_reader.next() => Event::Client(packet),
                packet = self.link.reader.next() => Event::Backend(packet),
            };
            let flow = match event {
                Event::Client(Ok(packet)) => self.client_packets(packet).await?,
                Event::Client(Err(ProxyError::Closed | ProxyError::Io(_))) => Flow::Stop,
                Event::Client(Err(error)) => return Err(error),
                Event::Backend(Ok(packet)) => self.backend_packets(packet).await?,
                Event::Backend(Err(error)) => {
                    tracing::info!(
                        "{} lost {}: {error}",
                        self.player.name,
                        self.link.server.name
                    );
                    self.backend_lost().await?
                }
            };
            if flow == Flow::Stop {
                return Ok(());
            }
        }
    }

    async fn client_packets(&mut self, first: Vec<u8>) -> Result<Flow, ProxyError> {
        self.client_packet(&first)?;
        while let Some(packet) = self.client_reader.ready()? {
            self.client_packet(&packet)?;
        }
        match self.link.writer.flush().await {
            Ok(()) => Ok(Flow::Continue),
            Err(error) => {
                tracing::info!(
                    "{} lost {}: {error}",
                    self.player.name,
                    self.link.server.name
                );
                self.backend_lost().await
            }
        }
    }

    fn client_packet(&mut self, packet: &[u8]) -> Result<(), ProxyError> {
        let Ok((id, body)) = split_packet_id(packet) else {
            return self.link.writer.queue(packet);
        };
        self.remember(id, body);
        let next = self.phase.after_serverbound(id);
        self.link.writer.queue(packet)?;
        if self.phase == Phase::Configuration && next == Phase::Play && self.replay_chat_session {
            self.replay_chat_session = false;
            if let Some(session) = &self.chat_session {
                self.link.writer.queue(session)?;
            }
        }
        self.phase = next;
        Ok(())
    }

    fn remember(&mut self, id: i32, body: &[u8]) {
        match (self.phase, id) {
            (Phase::Configuration, configuration::serverbound::CLIENT_INFORMATION)
            | (Phase::Play, play::serverbound::CLIENT_INFORMATION) => {
                self.client_information = Some(body.to_vec());
            }
            (Phase::Configuration, configuration::serverbound::CUSTOM_PAYLOAD)
            | (Phase::Play, play::serverbound::CUSTOM_PAYLOAD) => {
                if decode_string(&mut &body[..], 32_767).is_ok_and(|channel| channel == BRAND) {
                    self.brand = Some(body.to_vec());
                }
            }
            (Phase::Play, play::serverbound::CHAT_SESSION_UPDATE) => {
                let mut packet = Vec::with_capacity(body.len() + 1);
                VarInt(id).encode(&mut packet);
                packet.extend_from_slice(body);
                self.chat_session = Some(packet);
            }
            _ => {}
        }
    }

    async fn backend_packets(&mut self, first: Vec<u8>) -> Result<Flow, ProxyError> {
        let mut packet = Some(first);
        while let Some(current) = packet.take() {
            if let Some(reason) = self.kick_reason(&current) {
                self.client_writer.flush().await?;
                return self.kicked(current, reason).await;
            }
            self.client_writer.queue(&current)?;
            packet = self.link.reader.ready().ok().flatten();
        }
        self.client_writer.flush().await?;
        Ok(Flow::Continue)
    }

    fn kick_reason(&mut self, packet: &[u8]) -> Option<Text> {
        let (id, body) = split_packet_id(packet).ok()?;
        if id == self.phase.disconnect_id() {
            return Some(
                Disconnect::decode(&mut &body[..])
                    .map(|disconnect| disconnect.reason)
                    .unwrap_or_else(|_| status::nbt_text("Kicked")),
            );
        }
        if self.phase == Phase::Play {
            self.display.observe(id, body);
        }
        None
    }

    async fn kicked(&mut self, packet: Vec<u8>, reason: Text) -> Result<Flow, ProxyError> {
        let from = self.link.server.name.clone();
        tracing::info!("{} was kicked from {from}", self.player.name);
        if self.phase == Phase::Play && self.switch_to_fallback(Some(reason)).await? {
            return Ok(Flow::Continue);
        }
        self.client_writer.queue(&packet)?;
        self.client_writer.flush().await?;
        Ok(Flow::Stop)
    }

    async fn backend_lost(&mut self) -> Result<Flow, ProxyError> {
        if self.phase == Phase::Play
            && self
                .switch_to_fallback(Some(status::nbt_text(
                    "The server you were on went down, you have been moved.",
                )))
                .await?
        {
            return Ok(Flow::Continue);
        }
        self.client_writer
            .send_disconnect(
                self.phase,
                &status::nbt_text("The server you were on went down."),
            )
            .await?;
        Ok(Flow::Stop)
    }

    async fn switch_to_fallback(&mut self, notice: Option<Text>) -> Result<bool, ProxyError> {
        let fallbacks = self
            .shared
            .state()
            .registry
            .fallbacks_from(&self.link.server.name);
        for server in fallbacks {
            if let Some(link) = self.open(&server).await {
                self.switch(server, link, notice).await?;
                return Ok(true);
            }
        }
        Ok(false)
    }

    async fn open(&self, server: &Server) -> Option<BackendLink> {
        let settings = &self.shared.settings;
        let attempt = timeout(
            settings.connect_timeout,
            backend::login(
                &server.address,
                self.player,
                &settings.forwarding_secret,
                settings.compression_level,
            ),
        )
        .await
        .unwrap_or(Err(ProxyError::Timeout));
        match attempt {
            Ok(link) => Some(link),
            Err(error) => {
                tracing::info!(
                    "{} cannot move to {}: {error}",
                    self.player.name,
                    server.name
                );
                None
            }
        }
    }

    async fn switch(
        &mut self,
        server: Server,
        link: BackendLink,
        notice: Option<Text>,
    ) -> Result<(), ProxyError> {
        if let Some(notice) = notice {
            self.client_writer.queue_packet(&SystemChat {
                content: notice,
                overlay: false,
            })?;
        }
        for packet in self.display.clear_packets() {
            self.client_writer.queue(&packet)?;
        }
        self.client_writer.queue_packet(&StartConfiguration)?;
        self.client_writer.flush().await?;
        let BackendLink {
            reader,
            mut writer,
            finished: _,
        } = link;
        self.link = Link {
            server,
            reader,
            writer: {
                writer.queue_packet(&LoginAcknowledged)?;
                writer
            },
        };
        self.await_configuration().await?;
        self.phase = Phase::Configuration;
        if let Some(information) = &self.client_information {
            queue_with_id(
                &mut self.link.writer,
                configuration::serverbound::CLIENT_INFORMATION,
                information,
            )?;
        }
        if let Some(brand) = &self.brand {
            queue_with_id(
                &mut self.link.writer,
                configuration::serverbound::CUSTOM_PAYLOAD,
                brand,
            )?;
        }
        self.link.writer.flush().await?;
        self.replay_chat_session = self.chat_session.is_some();
        self.shared
            .state()
            .roster
            .connected(self.player.id, &self.link.server.name);
        tracing::info!("{} moved to {}", self.player.name, self.link.server.name);
        Ok(())
    }

    async fn await_configuration(&mut self) -> Result<(), ProxyError> {
        let deadline = self.shared.settings.login_timeout;
        timeout(deadline, async {
            loop {
                let packet = self.client_reader.next().await?;
                if split_packet_id(&packet)
                    .is_ok_and(|(id, _)| id == play::serverbound::CONFIGURATION_ACKNOWLEDGED)
                {
                    return Ok(());
                }
            }
        })
        .await
        .map_err(|_| ProxyError::Timeout)?
    }
}

fn queue_with_id(writer: &mut Writer, id: i32, body: &[u8]) -> Result<(), ProxyError> {
    let mut packet = Vec::with_capacity(body.len() + 2);
    write_packet_with_id(id, &RawBody(body), &mut packet);
    writer.queue(&packet)
}

struct RawBody<'a>(&'a [u8]);

impl Encode for RawBody<'_> {
    fn encode(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(self.0);
    }
}
