use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use tokio::sync::{mpsc, oneshot};

use crate::Error;
use crate::wire::{
    self, CommandSpec, ConnectOutcome, Empty, FromPlugin, Order, Player, Question, RichMessage,
    Server, answer, from_plugin, order, question, send_message,
};

const ANSWER_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectResult {
    pub outcome: ConnectOutcome,
    pub message: Option<String>,
}

#[derive(Default)]
struct Inner {
    outbox: Mutex<Option<mpsc::UnboundedSender<FromPlugin>>>,
    pending: Mutex<HashMap<u64, oneshot::Sender<answer::Body>>>,
    next_request: AtomicU64,
}

#[derive(Clone, Default)]
pub struct Proxy {
    inner: Arc<Inner>,
}

fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Proxy {
    pub(crate) fn attach(&self, outbox: mpsc::UnboundedSender<FromPlugin>) {
        *locked(&self.inner.outbox) = Some(outbox);
    }

    pub(crate) fn detach(&self) {
        *locked(&self.inner.outbox) = None;
        locked(&self.inner.pending).clear();
    }

    pub(crate) fn resolve(&self, answer: wire::Answer) {
        if let (Some(waiter), Some(body)) = (
            locked(&self.inner.pending).remove(&answer.request),
            answer.body,
        ) {
            let _ = waiter.send(body);
        }
    }

    pub(crate) fn send(&self, body: from_plugin::Body, request: Option<u64>) -> bool {
        locked(&self.inner.outbox).as_ref().is_some_and(|outbox| {
            outbox
                .send(FromPlugin {
                    body: Some(body),
                    request,
                })
                .is_ok()
        })
    }

    fn order(&self, body: order::Body) -> bool {
        self.send(from_plugin::Body::Order(Order { body: Some(body) }), None)
    }

    async fn ask(&self, body: from_plugin::Body) -> Result<answer::Body, Error> {
        let request = self.inner.next_request.fetch_add(1, Ordering::Relaxed) + 1;
        let (waiter, answer) = oneshot::channel();
        locked(&self.inner.pending).insert(request, waiter);
        if !self.send(body, Some(request)) {
            locked(&self.inner.pending).remove(&request);
            return Err(Error::Disconnected);
        }
        match tokio::time::timeout(ANSWER_TIMEOUT, answer).await {
            Ok(Ok(answer::Body::Error(message))) => Err(Error::Answer(message)),
            Ok(Ok(body)) => Ok(body),
            Ok(Err(_)) => Err(Error::Disconnected),
            Err(_) => {
                locked(&self.inner.pending).remove(&request);
                Err(Error::Timeout)
            }
        }
    }

    pub fn is_connected(&self) -> bool {
        locked(&self.inner.outbox).is_some()
    }

    pub async fn connect(&self, player: &str, server: &str) -> Result<ConnectResult, Error> {
        let body = from_plugin::Body::Order(Order {
            body: Some(order::Body::Connect(wire::ConnectPlayer {
                player: player.to_owned(),
                server: server.to_owned(),
            })),
        });
        match self.ask(body).await? {
            answer::Body::Connect(result) => Ok(ConnectResult {
                outcome: ConnectOutcome::try_from(result.outcome)
                    .unwrap_or(ConnectOutcome::Unspecified),
                message: result.message,
            }),
            _ => Err(Error::Answer("unexpected answer to connect".to_owned())),
        }
    }

    pub fn message(&self, player: &str, text: &str) -> bool {
        self.order(order::Body::Message(wire::SendMessage {
            players: vec![player.to_owned()],
            all: false,
            content: Some(send_message::Content::Text(text.to_owned())),
        }))
    }

    pub fn message_rich(&self, player: &str, message: RichMessage) -> bool {
        self.order(order::Body::Message(wire::SendMessage {
            players: vec![player.to_owned()],
            all: false,
            content: Some(send_message::Content::Rich(message)),
        }))
    }

    pub fn broadcast(&self, text: &str) -> bool {
        self.order(order::Body::Message(wire::SendMessage {
            players: Vec::new(),
            all: true,
            content: Some(send_message::Content::Text(text.to_owned())),
        }))
    }

    pub fn kick(&self, player: &str, reason: &str) -> bool {
        self.order(order::Body::Kick(wire::KickPlayer {
            player: player.to_owned(),
            reason: reason.to_owned(),
        }))
    }

    pub fn register_server(&self, name: &str, address: &str, fallback: bool) -> bool {
        self.order(order::Body::RegisterServer(wire::RegisterServer {
            name: name.to_owned(),
            address: address.to_owned(),
            fallback,
        }))
    }

    pub fn unregister_server(&self, name: &str) -> bool {
        self.order(order::Body::UnregisterServer(name.to_owned()))
    }

    pub fn set_permissions(&self, player: &str, granted: &[&str], denied: &[&str]) -> bool {
        let owned = |nodes: &[&str]| nodes.iter().map(|node| (*node).to_owned()).collect();
        self.order(order::Body::Permissions(wire::Permissions {
            player: player.to_owned(),
            granted: owned(granted),
            denied: owned(denied),
        }))
    }

    pub fn register_command(&self, spec: CommandSpec) -> bool {
        self.order(order::Body::RegisterCommand(spec))
    }

    pub fn unregister_command(&self, name: &str) -> bool {
        self.order(order::Body::UnregisterCommand(name.to_owned()))
    }

    pub async fn players(&self) -> Result<Vec<Player>, Error> {
        match self
            .ask(from_plugin::Body::Question(Question {
                body: Some(question::Body::Players(Empty {})),
            }))
            .await?
        {
            answer::Body::Players(list) => Ok(list.players),
            _ => Err(Error::Answer("unexpected answer to players".to_owned())),
        }
    }

    pub async fn servers(&self) -> Result<Vec<Server>, Error> {
        match self
            .ask(from_plugin::Body::Question(Question {
                body: Some(question::Body::Servers(Empty {})),
            }))
            .await?
        {
            answer::Body::Servers(list) => Ok(list.servers),
            _ => Err(Error::Answer("unexpected answer to servers".to_owned())),
        }
    }

    pub async fn player(&self, name: &str) -> Result<Player, Error> {
        match self
            .ask(from_plugin::Body::Question(Question {
                body: Some(question::Body::Player(name.to_owned())),
            }))
            .await?
        {
            answer::Body::Player(player) => Ok(player),
            _ => Err(Error::Answer("unexpected answer to player".to_owned())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orders_fail_while_disconnected() {
        let proxy = Proxy::default();
        assert!(!proxy.is_connected());
        assert!(!proxy.broadcast("hello"));
    }

    #[tokio::test]
    async fn questions_fail_fast_while_disconnected() {
        let proxy = Proxy::default();
        assert!(matches!(proxy.servers().await, Err(Error::Disconnected)));
    }

    #[tokio::test]
    async fn an_answer_reaches_the_waiting_question() {
        let proxy = Proxy::default();
        let (outbox, mut sent) = mpsc::unbounded_channel();
        proxy.attach(outbox);
        let asking = tokio::spawn({
            let proxy = proxy.clone();
            async move { proxy.servers().await }
        });
        let question = sent.recv().await.unwrap();
        proxy.resolve(wire::Answer {
            request: question.request.unwrap(),
            body: Some(answer::Body::Servers(wire::ServerList {
                servers: vec![Server {
                    name: "lobby".to_owned(),
                    address: "a:1".to_owned(),
                    players: 2,
                }],
            })),
        });
        let servers = asking.await.unwrap().unwrap();
        assert_eq!(servers[0].name, "lobby");
    }
}
