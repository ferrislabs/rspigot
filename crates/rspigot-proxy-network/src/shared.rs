use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};

use rspigot_protocol::crypto::ServerKey;
use rspigot_protocol::types::Text;
use rspigot_proxy_core::{
    CommandRegistry, CommandSpec, ConnectOutcome, Permissions, PlayerView, ProxyHooks, Roster,
    Server, ServerRegistry,
};
use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;

use crate::metrics::{Gauges, Metrics};
use crate::{ProxyError, ProxySettings, text};

pub(crate) enum Command {
    Connect {
        server: Server,
        reply: oneshot::Sender<ConnectOutcome>,
    },
    Message(Text),
    Kick(Text),
    RefreshCommands,
}

pub(crate) struct Handle {
    pub address: IpAddr,
    pub name: String,
    pub sender: mpsc::UnboundedSender<Command>,
}

pub struct State {
    pub registry: ServerRegistry,
    pub roster: Roster,
    pub commands: CommandRegistry,
    pub(crate) permissions: HashMap<Uuid, Permissions>,
    pub(crate) sessions: HashMap<Uuid, Handle>,
}

impl State {
    pub(crate) fn view(&self, id: Uuid) -> Option<PlayerView> {
        let handle = self.sessions.get(&id)?;
        Some(PlayerView {
            id,
            name: handle.name.clone(),
            address: handle.address,
            server: self
                .roster
                .get(id)
                .and_then(|presence| presence.server.clone()),
        })
    }

    pub(crate) fn allows(&self, id: Uuid, node: &str) -> bool {
        self.permissions
            .get(&id)
            .is_some_and(|permissions| permissions.allows(node))
    }

    fn refresh_all(&self) {
        for handle in self.sessions.values() {
            let _ = handle.sender.send(Command::RefreshCommands);
        }
    }
}

pub struct Shared<V, H> {
    pub(crate) settings: ProxySettings,
    pub(crate) verifier: V,
    pub(crate) hooks: H,
    pub(crate) key: ServerKey,
    pub(crate) metrics: Metrics,
    shutting_down: AtomicBool,
    state: Mutex<State>,
}

impl<V, H: ProxyHooks> Shared<V, H> {
    pub fn new(
        settings: ProxySettings,
        registry: ServerRegistry,
        verifier: V,
        hooks: H,
    ) -> Result<Self, ProxyError> {
        Ok(Self {
            settings,
            verifier,
            hooks,
            key: ServerKey::generate()?,
            metrics: Metrics::default(),
            shutting_down: AtomicBool::new(false),
            state: Mutex::new(State {
                registry,
                roster: Roster::default(),
                commands: CommandRegistry::default(),
                permissions: HashMap::new(),
                sessions: HashMap::new(),
            }),
        })
    }

    pub fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn hooks(&self) -> &H {
        &self.hooks
    }

    pub fn metrics(&self) -> &Metrics {
        &self.metrics
    }

    pub fn render_metrics(&self) -> String {
        let (online, per_server) = {
            let state = self.state();
            let per_server: Vec<(String, usize)> = state
                .registry
                .servers()
                .map(|server| {
                    let players = state.roster.on_server(&server.name);
                    (server.name, players)
                })
                .collect();
            (state.roster.online(), per_server)
        };
        self.metrics.render(&Gauges {
            online,
            per_server: &per_server,
            shutting_down: self.is_shutting_down(),
        })
    }

    pub fn is_shutting_down(&self) -> bool {
        self.shutting_down.load(Ordering::Relaxed)
    }

    pub fn begin_shutdown(&self, message: &str) -> usize {
        self.shutting_down.store(true, Ordering::Relaxed);
        let reason = text::legacy(message);
        let state = self.state();
        for handle in state.sessions.values() {
            let _ = handle.sender.send(Command::Kick(reason.clone()));
        }
        state.roster.online()
    }

    pub fn online(&self) -> usize {
        self.state().roster.online()
    }

    pub fn players(&self) -> Vec<PlayerView> {
        let state = self.state();
        state
            .sessions
            .keys()
            .filter_map(|id| state.view(*id))
            .collect()
    }

    pub fn player(&self, id_or_name: &str) -> Option<PlayerView> {
        let state = self.state();
        let id = Uuid::parse_str(id_or_name).ok().or_else(|| {
            state
                .sessions
                .iter()
                .find(|(_, handle)| handle.name.eq_ignore_ascii_case(id_or_name))
                .map(|(id, _)| *id)
        })?;
        state.view(id)
    }

    pub fn servers(&self) -> Vec<(Server, usize)> {
        let state = self.state();
        state
            .registry
            .servers()
            .map(|server| {
                let players = state.roster.on_server(&server.name);
                (server, players)
            })
            .collect()
    }

    pub fn register_server(&self, name: &str, address: &str, fallback: bool) {
        let mut state = self.state();
        if fallback {
            state.registry.register_fallback(name, address);
        } else {
            state.registry.register(name, address);
        }
        tracing::info!("Registered server {name} at {address}");
    }

    pub fn unregister_server(&self, name: &str) {
        if self.state().registry.unregister(name).is_some() {
            tracing::info!("Unregistered server {name}");
        }
    }

    pub fn set_permissions(&self, player: Uuid, permissions: Permissions) {
        let state = &mut *self.state();
        state.permissions.insert(player, permissions);
        if let Some(handle) = state.sessions.get(&player) {
            let _ = handle.sender.send(Command::RefreshCommands);
        }
    }

    pub fn register_command(&self, owner: u64, spec: CommandSpec) {
        let mut state = self.state();
        state.commands.register(owner, spec);
        state.refresh_all();
    }

    pub fn unregister_command(&self, owner: u64, name: &str) {
        let mut state = self.state();
        state.commands.unregister(owner, name);
        state.refresh_all();
    }

    pub fn remove_commands(&self, owner: u64) {
        let mut state = self.state();
        state.commands.remove_owner(owner);
        state.refresh_all();
    }

    pub fn send_message(&self, players: Option<&[Uuid]>, text: &Text) {
        let state = self.state();
        for (id, handle) in &state.sessions {
            if players.is_none_or(|wanted| wanted.contains(id)) {
                let _ = handle.sender.send(Command::Message(text.clone()));
            }
        }
    }

    pub fn kick(&self, player: Uuid, reason: Text) -> bool {
        self.state()
            .sessions
            .get(&player)
            .is_some_and(|handle| handle.sender.send(Command::Kick(reason)).is_ok())
    }

    pub async fn connect(&self, player: Uuid, server: &str) -> ConnectOutcome {
        let (reply, answer) = oneshot::channel();
        {
            let state = self.state();
            let Some(target) = state.registry.get(server) else {
                return ConnectOutcome::UnknownServer;
            };
            let Some(handle) = state.sessions.get(&player) else {
                return ConnectOutcome::Offline;
            };
            if handle
                .sender
                .send(Command::Connect {
                    server: target,
                    reply,
                })
                .is_err()
            {
                return ConnectOutcome::Offline;
            }
        }
        answer.await.unwrap_or(ConnectOutcome::Offline)
    }
}
