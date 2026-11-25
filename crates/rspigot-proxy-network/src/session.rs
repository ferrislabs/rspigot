use rspigot_protocol::codec::{Decode, decode_exact, decode_string};
use rspigot_protocol::common::Disconnect;
use rspigot_protocol::ids::{configuration, play};
use rspigot_protocol::login::LoginAcknowledged;
use rspigot_protocol::play::chat::SystemChat;
use rspigot_protocol::play::commands::{CommandSuggestions, Commands, Suggestion};
use rspigot_protocol::play::player::StartConfiguration;
use rspigot_protocol::play::serverbound::{ChatCommand, CommandSuggestion};
use rspigot_protocol::types::Text;
use rspigot_protocol::{Encode, VarInt, split_packet_id, write_packet, write_packet_with_id};
use rspigot_proxy_core::{
    ConnectCause, ConnectOutcome, DisplayState, Phase, PlayerView, ProxyHooks, Server, Verdict,
};
use tokio::sync::mpsc;
use tokio::time::timeout;

use crate::backend::{self, BackendLink};
use crate::forwarding::ForwardedPlayer;
use crate::metrics::{add, bump};
use crate::shared::Command;
use crate::wire::{Reader, Writer};
use crate::{ProxyError, Shared, status, text};

const BRAND: &str = "minecraft:brand";

pub struct Link {
    pub server: Server,
    pub reader: Reader,
    pub writer: Writer,
}

enum Event {
    Client(Result<Vec<u8>, ProxyError>),
    Backend(Result<Vec<u8>, ProxyError>),
    Command(Option<Command>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Flow {
    Continue,
    Stop,
}

pub struct Session<'a, V, H> {
    shared: &'a Shared<V, H>,
    player: &'a ForwardedPlayer,
    client_reader: &'a mut Reader,
    client_writer: &'a mut Writer,
    link: Link,
    commands: mpsc::UnboundedReceiver<Command>,
    phase: Phase,
    display: DisplayState,
    tree: Option<Commands>,
    client_information: Option<Vec<u8>>,
    brand: Option<Vec<u8>>,
    chat_session: Option<Vec<u8>>,
    replay_chat_session: bool,
}

impl<'a, V, H: ProxyHooks> Session<'a, V, H> {
    pub fn new(
        shared: &'a Shared<V, H>,
        player: &'a ForwardedPlayer,
        client_reader: &'a mut Reader,
        client_writer: &'a mut Writer,
        link: Link,
        commands: mpsc::UnboundedReceiver<Command>,
    ) -> Self {
        Self {
            shared,
            player,
            client_reader,
            client_writer,
            link,
            commands,
            phase: Phase::Configuration,
            display: DisplayState::default(),
            tree: None,
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
                command = self.commands.recv() => Event::Command(command),
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
                Event::Command(Some(command)) => self.command(command).await?,
                Event::Command(None) => Flow::Continue,
            };
            if flow == Flow::Stop {
                return Ok(());
            }
        }
    }

    fn view(&self) -> PlayerView {
        PlayerView {
            id: self.player.id,
            name: self.player.name.clone(),
            address: self.player.address,
            server: Some(self.link.server.name.clone()),
        }
    }

    async fn command(&mut self, command: Command) -> Result<Flow, ProxyError> {
        match command {
            Command::Message(text) => {
                if self.phase == Phase::Play {
                    self.client_writer
                        .send(&SystemChat {
                            content: text,
                            overlay: false,
                        })
                        .await?;
                }
                Ok(Flow::Continue)
            }
            Command::Kick(reason) => {
                self.client_writer
                    .send_disconnect(self.phase, &reason)
                    .await?;
                Ok(Flow::Stop)
            }
            Command::RefreshCommands => {
                if self.phase == Phase::Play
                    && let Some(tree) = self.tree.clone()
                {
                    self.send_tree(tree).await?;
                }
                Ok(Flow::Continue)
            }
            Command::Connect { server, reply } => {
                let outcome = self.connect_on_request(server).await?;
                let _ = reply.send(outcome);
                Ok(Flow::Continue)
            }
        }
    }

    async fn connect_on_request(&mut self, server: Server) -> Result<ConnectOutcome, ProxyError> {
        if server.name == self.link.server.name {
            return Ok(ConnectOutcome::AlreadyThere);
        }
        if self.phase != Phase::Play {
            return Ok(ConnectOutcome::Denied(
                "the player is not in game yet".to_owned(),
            ));
        }
        let verdict = self
            .shared
            .hooks
            .pre_connect(self.view(), server.name.clone(), ConnectCause::Plugin)
            .await;
        let server = match verdict {
            Verdict::Allow => server,
            Verdict::Deny(message) => return Ok(ConnectOutcome::Denied(message)),
            Verdict::Redirect { server: target, .. } => {
                let target = self.shared.state().registry.get(&target);
                match target {
                    Some(target) => target,
                    None => return Ok(ConnectOutcome::UnknownServer),
                }
            }
        };
        match self.open(&server).await {
            Some(link) => {
                self.switch(server, link, None).await?;
                Ok(ConnectOutcome::Connected)
            }
            None => Ok(ConnectOutcome::Unreachable),
        }
    }

    async fn client_packets(&mut self, first: Vec<u8>) -> Result<Flow, ProxyError> {
        let mut count = 1;
        self.client_packet(first).await?;
        while let Some(packet) = self.client_reader.ready()? {
            count += 1;
            self.client_packet(packet).await?;
        }
        add(&self.shared.metrics.packets_to_backend, count);
        self.client_writer.flush().await?;
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

    async fn client_packet(&mut self, packet: Vec<u8>) -> Result<(), ProxyError> {
        let Ok((id, body)) = split_packet_id(&packet) else {
            return self.link.writer.queue(&packet);
        };
        if self.phase == Phase::Play && self.intercept(id, body)? {
            return Ok(());
        }
        self.remember(id, body);
        let next = self.phase.after_serverbound(id);
        self.link.writer.queue(&packet)?;
        if self.phase == Phase::Configuration && next == Phase::Play && self.replay_chat_session {
            self.replay_chat_session = false;
            if let Some(session) = &self.chat_session {
                self.link.writer.queue(session)?;
            }
        }
        self.phase = next;
        Ok(())
    }

    fn intercept(&mut self, id: i32, body: &[u8]) -> Result<bool, ProxyError> {
        match id {
            play::serverbound::CHAT_COMMAND => {
                let Ok(ChatCommand { command }) = decode_exact(body) else {
                    return Ok(false);
                };
                let found = {
                    let state = self.shared.state();
                    state
                        .commands
                        .parse(&command, |node| state.allows(self.player.id, node))
                };
                let Some(invocation) = found else {
                    return Ok(false);
                };
                bump(&self.shared.metrics.proxy_commands);
                tracing::info!(
                    "{} issued proxy command: /{}",
                    self.player.name,
                    invocation.line
                );
                self.shared.hooks.command(&self.view(), invocation);
                Ok(true)
            }
            play::serverbound::COMMAND_SUGGESTION => {
                let Ok(CommandSuggestion { id, command }) = decode_exact(body) else {
                    return Ok(false);
                };
                let suggestions = {
                    let state = self.shared.state();
                    let servers: Vec<String> =
                        state.registry.servers().map(|server| server.name).collect();
                    let players: Vec<String> = state
                        .sessions
                        .values()
                        .map(|handle| handle.name.clone())
                        .collect();
                    state
                        .commands
                        .suggest(&command, &servers, &players, |node| {
                            state.allows(self.player.id, node)
                        })
                };
                let Some(suggestions) = suggestions else {
                    return Ok(false);
                };
                self.client_writer.queue_packet(&CommandSuggestions {
                    id,
                    start: i32::try_from(suggestions.start).unwrap_or(0),
                    length: i32::try_from(suggestions.length).unwrap_or(0),
                    suggestions: suggestions
                        .matches
                        .into_iter()
                        .map(|text| Suggestion {
                            text,
                            tooltip: None,
                        })
                        .collect(),
                })?;
                Ok(true)
            }
            _ => Ok(false),
        }
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
            bump(&self.shared.metrics.packets_to_client);
            if let Some(reason) = self.kick_reason(&current) {
                self.client_writer.flush().await?;
                return self.kicked(current, reason).await;
            }
            if !self.rewrite_tree(&current)? {
                self.client_writer.queue(&current)?;
            }
            packet = self.link.reader.ready().ok().flatten();
        }
        self.client_writer.flush().await?;
        Ok(Flow::Continue)
    }

    fn rewrite_tree(&mut self, packet: &[u8]) -> Result<bool, ProxyError> {
        if self.phase != Phase::Play {
            return Ok(false);
        }
        let Ok((id, body)) = split_packet_id(packet) else {
            return Ok(false);
        };
        if id != play::clientbound::COMMANDS {
            return Ok(false);
        }
        let Ok(tree) = decode_exact::<Commands>(body) else {
            return Ok(false);
        };
        let injected = self.inject(tree.clone());
        self.tree = Some(tree);
        self.client_writer.queue_packet(&injected)?;
        Ok(true)
    }

    fn inject(&self, mut tree: Commands) -> Commands {
        let state = self.shared.state();
        state
            .commands
            .inject(&mut tree, |node| state.allows(self.player.id, node));
        tree
    }

    async fn send_tree(&mut self, tree: Commands) -> Result<(), ProxyError> {
        let injected = self.inject(tree);
        let mut packet = Vec::new();
        write_packet(&injected, &mut packet);
        self.client_writer.queue(&packet)?;
        self.client_writer.flush().await
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
        bump(&self.shared.metrics.kicks);
        tracing::info!("{} was kicked from {from}", self.player.name);
        if self.phase != Phase::Play {
            return self.forward_kick(&packet).await;
        }
        let fallbacks: Vec<String> = self
            .shared
            .state()
            .registry
            .fallbacks_from(&from)
            .into_iter()
            .map(|server| server.name)
            .collect();
        let verdict = self
            .shared
            .hooks
            .kicked(self.view(), from, text::to_legacy(&reason), fallbacks)
            .await;
        match verdict {
            Verdict::Allow => {
                if self.switch_to_fallback(Some(reason)).await? {
                    return Ok(Flow::Continue);
                }
                self.forward_kick(&packet).await
            }
            Verdict::Deny(message) => {
                self.client_writer
                    .send_disconnect(self.phase, &text::legacy(&message))
                    .await?;
                Ok(Flow::Stop)
            }
            Verdict::Redirect { server, message } => {
                let target = self.shared.state().registry.get(&server);
                let notice = message
                    .map(|message| text::legacy(&message))
                    .or(Some(reason));
                if let Some(target) = target
                    && let Some(link) = self.open(&target).await
                {
                    self.switch(target, link, notice).await?;
                    return Ok(Flow::Continue);
                }
                self.forward_kick(&packet).await
            }
        }
    }

    async fn forward_kick(&mut self, packet: &[u8]) -> Result<Flow, ProxyError> {
        self.client_writer.queue(packet)?;
        self.client_writer.flush().await?;
        Ok(Flow::Stop)
    }

    async fn backend_lost(&mut self) -> Result<Flow, ProxyError> {
        bump(&self.shared.metrics.backends_lost);
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
            let verdict = self
                .shared
                .hooks
                .pre_connect(self.view(), server.name.clone(), ConnectCause::Fallback)
                .await;
            if verdict != Verdict::Allow {
                continue;
            }
            if let Some(link) = self.open(&server).await {
                self.switch(server, link, notice).await?;
                return Ok(true);
            }
        }
        bump(&self.shared.metrics.switch_failures);
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
                bump(&self.shared.metrics.backend_unreachable);
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
        writer.queue_packet(&LoginAcknowledged)?;
        let previous = std::mem::replace(
            &mut self.link,
            Link {
                server,
                reader,
                writer,
            },
        )
        .server
        .name;
        self.tree = None;
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
        bump(&self.shared.metrics.switches);
        tracing::info!("{} moved to {}", self.player.name, self.link.server.name);
        let server = self.link.server.name.clone();
        self.shared
            .hooks
            .connected(&self.view(), &server, Some(&previous));
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
