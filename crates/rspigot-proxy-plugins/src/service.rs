use std::sync::Arc;
use std::time::Duration;

use rspigot_plugin_protocol::proxy::v1::{
    self, FromPlugin, ToPlugin, answer, from_plugin, order, proxy_host_server::ProxyHost, question,
    send_message, to_plugin,
};
use rspigot_proxy_core::{ConnectOutcome, Permissions, Verdict};
use rspigot_proxy_network::{Shared, text};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status, Streaming};
use uuid::Uuid;

use crate::convert;
use crate::hub::{Outbox, PluginHub};

const HELLO_TIMEOUT: Duration = Duration::from_secs(10);
const OUTBOX: usize = 1024;

pub struct Host<V> {
    shared: Arc<Shared<V, PluginHub>>,
    token: Option<String>,
    proxy_version: String,
}

impl<V> Host<V> {
    pub fn new(shared: Arc<Shared<V, PluginHub>>, token: Option<String>) -> Self {
        Self {
            shared,
            token,
            proxy_version: env!("CARGO_PKG_VERSION").to_owned(),
        }
    }
}

fn same_token(expected: &str, given: &str) -> bool {
    expected.len() == given.len()
        && expected
            .bytes()
            .zip(given.bytes())
            .fold(0, |difference, (a, b)| difference | (a ^ b))
            == 0
}

#[tonic::async_trait]
impl<V: Send + Sync + 'static> ProxyHost for Host<V> {
    type ConnectStream = ReceiverStream<Result<ToPlugin, Status>>;

    async fn connect(
        &self,
        request: Request<Streaming<FromPlugin>>,
    ) -> Result<Response<Self::ConnectStream>, Status> {
        let mut incoming = request.into_inner();
        let first = tokio::time::timeout(HELLO_TIMEOUT, incoming.message())
            .await
            .map_err(|_| Status::deadline_exceeded("no hello within 10 seconds"))??;
        let Some(FromPlugin {
            body: Some(from_plugin::Body::Hello(hello)),
            ..
        }) = first
        else {
            return Err(Status::invalid_argument(
                "the first message must be a hello",
            ));
        };
        if hello.protocol_version != rspigot_plugin_protocol::proxy::PROTOCOL_VERSION {
            return Err(Status::failed_precondition(format!(
                "proxy protocol {} is not supported, this proxy speaks {}",
                hello.protocol_version,
                rspigot_plugin_protocol::proxy::PROTOCOL_VERSION
            )));
        }
        if let Some(token) = &self.token
            && !same_token(token, &hello.token)
        {
            tracing::warn!("Refused proxy plugin {}: wrong token", hello.name);
            return Err(Status::unauthenticated("wrong token"));
        }
        let (outbox, replies) = mpsc::channel(OUTBOX);
        let hub = self.shared.hooks().clone();
        let id = hub.attach(&hello.name, &hello.subscriptions, outbox.clone());
        for spec in hello.commands {
            self.shared.register_command(id, convert::command(spec));
        }
        let accepted = v1::Accepted {
            proxy_version: self.proxy_version.clone(),
            players: self.shared.players().iter().map(convert::player).collect(),
            servers: self
                .shared
                .servers()
                .iter()
                .map(|(server, players)| convert::server(server, *players))
                .collect(),
        };
        let _ = outbox
            .send(Ok(ToPlugin {
                body: Some(to_plugin::Body::Accepted(accepted)),
            }))
            .await;
        tracing::info!("Proxy plugin {} {} connected", hello.name, hello.version);
        let shared = Arc::clone(&self.shared);
        let name = hello.name;
        tokio::spawn(async move {
            let link = Link {
                shared: Arc::clone(&shared),
                id,
                outbox,
            };
            loop {
                match incoming.message().await {
                    Ok(Some(message)) => link.handle(message),
                    Ok(None) => break,
                    Err(status) => {
                        tracing::info!("Proxy plugin {name} stream ended: {}", status.message());
                        break;
                    }
                }
            }
            hub.detach(id);
            shared.remove_commands(id);
            tracing::info!("Proxy plugin {name} disconnected");
        });
        Ok(Response::new(ReceiverStream::new(replies)))
    }
}

struct Link<V> {
    shared: Arc<Shared<V, PluginHub>>,
    id: u64,
    outbox: Outbox,
}

impl<V: Send + Sync + 'static> Link<V> {
    fn handle(&self, message: FromPlugin) {
        let request = message.request;
        match message.body {
            Some(from_plugin::Body::Decision(decision)) => {
                let verdict = decision.verdict.map_or(Verdict::Allow, convert::verdict);
                self.shared.hooks().resolve(decision.event, verdict);
            }
            Some(from_plugin::Body::Order(order)) => {
                if let Some(body) = order.body {
                    self.order(body, request);
                }
            }
            Some(from_plugin::Body::Question(question)) => {
                if let (Some(body), Some(request)) = (question.body, request) {
                    self.question(body, request);
                }
            }
            Some(from_plugin::Body::Hello(_)) => {
                self.warn("hello is only accepted as the first message".to_owned());
            }
            None => {}
        }
    }

    fn resolve(&self, player: &str) -> Option<Uuid> {
        Uuid::parse_str(player)
            .ok()
            .or_else(|| self.shared.player(player).map(|view| view.id))
    }

    fn order(&self, body: order::Body, request: Option<u64>) {
        match body {
            order::Body::Connect(connect) => {
                let Some(player) = self.resolve(&connect.player) else {
                    self.answer_connect(request, ConnectOutcome::Offline);
                    return;
                };
                let shared = Arc::clone(&self.shared);
                let outbox = self.outbox.clone();
                tokio::spawn(async move {
                    let outcome = shared.connect(player, &connect.server).await;
                    if let Some(request) = request {
                        let _ = outbox.try_send(Ok(answer_message(
                            request,
                            answer::Body::Connect(connect_result(outcome)),
                        )));
                    }
                });
            }
            order::Body::Message(message) => {
                let content = match message.content {
                    Some(send_message::Content::Text(value)) => text::legacy(&value),
                    Some(send_message::Content::Rich(rich)) => text::rich(&convert::rich(rich)),
                    None => return,
                };
                if message.all {
                    self.shared.send_message(None, &content);
                } else {
                    let players: Vec<Uuid> = message
                        .players
                        .iter()
                        .filter_map(|player| self.resolve(player))
                        .collect();
                    self.shared.send_message(Some(&players), &content);
                }
            }
            order::Body::Kick(kick) => {
                let kicked = self
                    .resolve(&kick.player)
                    .is_some_and(|player| self.shared.kick(player, text::legacy(&kick.reason)));
                if !kicked {
                    self.warn(format!("cannot kick {}: not connected", kick.player));
                }
            }
            order::Body::RegisterServer(server) => {
                self.shared
                    .register_server(&server.name, &server.address, server.fallback);
            }
            order::Body::UnregisterServer(name) => self.shared.unregister_server(&name),
            order::Body::Permissions(permissions) => match self.resolve(&permissions.player) {
                Some(player) => self.shared.set_permissions(
                    player,
                    Permissions::new(permissions.granted, permissions.denied),
                ),
                None => self.warn(format!(
                    "cannot set permissions of {}: unknown player",
                    permissions.player
                )),
            },
            order::Body::RegisterCommand(spec) => {
                self.shared
                    .register_command(self.id, convert::command(spec));
            }
            order::Body::UnregisterCommand(name) => {
                self.shared.unregister_command(self.id, &name);
            }
        }
    }

    fn answer_connect(&self, request: Option<u64>, outcome: ConnectOutcome) {
        if let Some(request) = request {
            let _ = self.outbox.try_send(Ok(answer_message(
                request,
                answer::Body::Connect(connect_result(outcome)),
            )));
        }
    }

    fn question(&self, body: question::Body, request: u64) {
        let answer = match body {
            question::Body::Players(_) => answer::Body::Players(v1::PlayerList {
                players: self.shared.players().iter().map(convert::player).collect(),
            }),
            question::Body::Servers(_) => answer::Body::Servers(v1::ServerList {
                servers: self
                    .shared
                    .servers()
                    .iter()
                    .map(|(server, players)| convert::server(server, *players))
                    .collect(),
            }),
            question::Body::Player(player) => match self.shared.player(&player) {
                Some(view) => answer::Body::Player(convert::player(&view)),
                None => answer::Body::Error(format!("{player} is not connected")),
            },
        };
        let _ = self.outbox.try_send(Ok(answer_message(request, answer)));
    }

    fn warn(&self, message: String) {
        let _ = self.outbox.try_send(Ok(ToPlugin {
            body: Some(to_plugin::Body::Warning(v1::Warning { message })),
        }));
    }
}

fn answer_message(request: u64, body: answer::Body) -> ToPlugin {
    ToPlugin {
        body: Some(to_plugin::Body::Answer(v1::Answer {
            request,
            body: Some(body),
        })),
    }
}

fn connect_result(outcome: ConnectOutcome) -> v1::ConnectResult {
    let (outcome, message) = match outcome {
        ConnectOutcome::Connected => (v1::ConnectOutcome::Connected, None),
        ConnectOutcome::AlreadyThere => (v1::ConnectOutcome::AlreadyThere, None),
        ConnectOutcome::Denied(message) => (v1::ConnectOutcome::Denied, Some(message)),
        ConnectOutcome::Unreachable => (v1::ConnectOutcome::Unreachable, None),
        ConnectOutcome::UnknownServer => (v1::ConnectOutcome::UnknownServer, None),
        ConnectOutcome::Offline => (v1::ConnectOutcome::Offline, None),
    };
    v1::ConnectResult {
        outcome: outcome as i32,
        message,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_compare_in_full() {
        assert!(same_token("secret", "secret"));
        assert!(!same_token("secret", "secreT"));
        assert!(!same_token("secret", "secret2"));
    }

    #[test]
    fn every_outcome_has_a_wire_value() {
        assert_eq!(
            connect_result(ConnectOutcome::Denied("full".to_owned())),
            v1::ConnectResult {
                outcome: v1::ConnectOutcome::Denied as i32,
                message: Some("full".to_owned()),
            }
        );
        assert_eq!(
            connect_result(ConnectOutcome::Connected).outcome,
            v1::ConnectOutcome::Connected as i32
        );
    }
}
