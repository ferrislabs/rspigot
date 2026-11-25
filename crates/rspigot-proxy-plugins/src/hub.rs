use std::collections::HashMap;
use std::future::Future;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use rspigot_plugin_protocol::proxy::v1::{
    self, ConnectCause as WireCause, Event, Hook, ToPlugin, event, to_plugin,
};
use rspigot_proxy_core::{ConnectCause, Invocation, PlayerView, ProxyHooks, Verdict};
use tokio::sync::{mpsc, oneshot};
use tonic::Status;

use crate::convert;

pub type Outbox = mpsc::Sender<Result<ToPlugin, Status>>;

struct Plugin {
    id: u64,
    name: String,
    subscriptions: HashMap<Hook, bool>,
    outbox: Outbox,
}

#[derive(Default)]
struct Inner {
    plugins: Mutex<Vec<Plugin>>,
    pending: Mutex<HashMap<u64, oneshot::Sender<Verdict>>>,
    next_event: AtomicU64,
    next_plugin: AtomicU64,
    decide_timeout: Duration,
}

#[derive(Clone)]
pub struct PluginHub {
    inner: Arc<Inner>,
}

impl PluginHub {
    pub fn new(decide_timeout: Duration) -> Self {
        Self {
            inner: Arc::new(Inner {
                decide_timeout,
                next_plugin: AtomicU64::new(1),
                next_event: AtomicU64::new(1),
                ..Inner::default()
            }),
        }
    }

    fn plugins(&self) -> MutexGuard<'_, Vec<Plugin>> {
        self.inner
            .plugins
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    fn pending(&self) -> MutexGuard<'_, HashMap<u64, oneshot::Sender<Verdict>>> {
        self.inner
            .pending
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    pub fn attach(&self, name: &str, subscriptions: &[v1::Subscription], outbox: Outbox) -> u64 {
        let id = self.inner.next_plugin.fetch_add(1, Ordering::Relaxed);
        let subscriptions = subscriptions
            .iter()
            .filter_map(|subscription| {
                Hook::try_from(subscription.hook)
                    .ok()
                    .filter(|hook| *hook != Hook::Unspecified)
                    .map(|hook| (hook, subscription.decide))
            })
            .collect();
        let mut plugins = self.plugins();
        plugins.retain(|plugin| plugin.name != name);
        plugins.push(Plugin {
            id,
            name: name.to_owned(),
            subscriptions,
            outbox,
        });
        id
    }

    pub fn detach(&self, id: u64) {
        self.plugins().retain(|plugin| plugin.id != id);
    }

    pub fn is_attached(&self, id: u64) -> bool {
        self.plugins().iter().any(|plugin| plugin.id == id)
    }

    pub fn names(&self) -> Vec<String> {
        self.plugins()
            .iter()
            .map(|plugin| plugin.name.clone())
            .collect()
    }

    pub fn resolve(&self, event: u64, verdict: Verdict) {
        if let Some(waiter) = self.pending().remove(&event) {
            let _ = waiter.send(verdict);
        }
    }

    fn notify(&self, hook: Hook, body: event::Body) {
        for plugin in self.plugins().iter() {
            if plugin.subscriptions.contains_key(&hook) {
                let id = self.inner.next_event.fetch_add(1, Ordering::Relaxed);
                send(
                    &plugin.outbox,
                    Event {
                        id,
                        decide: false,
                        body: Some(body.clone()),
                    },
                );
            }
        }
    }

    fn deciders(&self, hook: Hook) -> Vec<(String, Outbox)> {
        self.plugins()
            .iter()
            .filter(|plugin| plugin.subscriptions.get(&hook) == Some(&true))
            .map(|plugin| (plugin.name.clone(), plugin.outbox.clone()))
            .collect()
    }

    fn decide(
        &self,
        hook: Hook,
        body: event::Body,
    ) -> impl Future<Output = Verdict> + Send + use<> {
        let observers: Vec<Outbox> = self
            .plugins()
            .iter()
            .filter(|plugin| plugin.subscriptions.get(&hook) == Some(&false))
            .map(|plugin| plugin.outbox.clone())
            .collect();
        for outbox in &observers {
            let id = self.inner.next_event.fetch_add(1, Ordering::Relaxed);
            send(
                outbox,
                Event {
                    id,
                    decide: false,
                    body: Some(body.clone()),
                },
            );
        }
        let deciders = self.deciders(hook);
        let hub = self.clone();
        async move {
            for (name, outbox) in deciders {
                let id = hub.inner.next_event.fetch_add(1, Ordering::Relaxed);
                let (waiter, verdict) = oneshot::channel();
                hub.pending().insert(id, waiter);
                send(
                    &outbox,
                    Event {
                        id,
                        decide: true,
                        body: Some(body.clone()),
                    },
                );
                let answer = tokio::time::timeout(hub.inner.decide_timeout, verdict).await;
                hub.pending().remove(&id);
                match answer {
                    Ok(Ok(Verdict::Allow)) => {}
                    Ok(Ok(verdict)) => return verdict,
                    Ok(Err(_)) => {}
                    Err(_) => {
                        tracing::warn!("Plugin {name} did not decide on {hook:?} in time");
                    }
                }
            }
            Verdict::Allow
        }
    }

    pub fn invoke(&self, owner: u64, player: &PlayerView, invocation: Invocation) {
        if let Some(plugin) = self.plugins().iter().find(|plugin| plugin.id == owner) {
            let _ = plugin.outbox.try_send(Ok(ToPlugin {
                body: Some(to_plugin::Body::Invocation(v1::Invocation {
                    command: invocation.command,
                    label: invocation.label,
                    player: Some(convert::player(player)),
                    arguments: invocation.arguments,
                    line: invocation.line,
                })),
            }));
        }
    }
}

fn send(outbox: &Outbox, event: Event) {
    if outbox
        .try_send(Ok(ToPlugin {
            body: Some(to_plugin::Body::Event(event)),
        }))
        .is_err()
    {
        tracing::warn!("A plugin is not keeping up with proxy events, one was dropped");
    }
}

impl ProxyHooks for PluginHub {
    fn login(&self, player: &PlayerView) {
        self.notify(
            Hook::Login,
            event::Body::Login(v1::Login {
                player: Some(convert::player(player)),
            }),
        );
    }

    fn disconnect(&self, player: &PlayerView) {
        self.notify(
            Hook::Disconnect,
            event::Body::Disconnect(v1::Disconnect {
                player: Some(convert::player(player)),
            }),
        );
    }

    fn connected(&self, player: &PlayerView, server: &str, previous: Option<&str>) {
        self.notify(
            Hook::Connected,
            event::Body::Connected(v1::Connected {
                player: Some(convert::player(player)),
                server: server.to_owned(),
                previous: previous.map(str::to_owned),
            }),
        );
    }

    fn command(&self, player: &PlayerView, invocation: Invocation) {
        self.invoke(invocation.owner, player, invocation);
    }

    fn initial_server(
        &self,
        player: PlayerView,
        candidates: Vec<String>,
    ) -> impl Future<Output = Verdict> + Send {
        self.decide(
            Hook::InitialServer,
            event::Body::InitialServer(v1::InitialServer {
                player: Some(convert::player(&player)),
                candidates,
            }),
        )
    }

    fn pre_connect(
        &self,
        player: PlayerView,
        target: String,
        cause: ConnectCause,
    ) -> impl Future<Output = Verdict> + Send {
        let cause = match cause {
            ConnectCause::Join => WireCause::Join,
            ConnectCause::Plugin => WireCause::Plugin,
            ConnectCause::Fallback => WireCause::Fallback,
        };
        self.decide(
            Hook::PreConnect,
            event::Body::PreConnect(v1::PreConnect {
                player: Some(convert::player(&player)),
                target,
                cause: cause as i32,
            }),
        )
    }

    fn kicked(
        &self,
        player: PlayerView,
        server: String,
        reason: String,
        fallbacks: Vec<String>,
    ) -> impl Future<Output = Verdict> + Send {
        self.decide(
            Hook::Kicked,
            event::Body::Kicked(v1::Kicked {
                player: Some(convert::player(&player)),
                server,
                reason,
                fallbacks,
            }),
        )
    }
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr};

    use uuid::Uuid;

    use super::*;

    fn player() -> PlayerView {
        PlayerView {
            id: Uuid::from_u128(1),
            name: "Steve".to_owned(),
            address: IpAddr::V4(Ipv4Addr::LOCALHOST),
            server: None,
        }
    }

    fn subscribe(hook: Hook, decide: bool) -> Vec<v1::Subscription> {
        vec![v1::Subscription {
            hook: hook as i32,
            decide,
        }]
    }

    fn event_of(message: Result<ToPlugin, Status>) -> Event {
        match message.unwrap().body {
            Some(to_plugin::Body::Event(event)) => event,
            other => panic!("expected an event, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn a_deciding_plugin_answers_the_hook() {
        let hub = PluginHub::new(Duration::from_secs(1));
        let (outbox, mut inbox) = mpsc::channel(8);
        hub.attach("network", &subscribe(Hook::PreConnect, true), outbox);
        let decision = tokio::spawn({
            let hub = hub.clone();
            async move {
                hub.pre_connect(player(), "game".to_owned(), ConnectCause::Plugin)
                    .await
            }
        });
        let event = event_of(inbox.recv().await.unwrap());
        assert!(event.decide);
        let Some(event::Body::PreConnect(pre)) = event.body else {
            panic!("expected pre_connect");
        };
        assert_eq!(pre.target, "game");
        assert_eq!(pre.cause, WireCause::Plugin as i32);
        hub.resolve(event.id, Verdict::Deny("full".to_owned()));
        assert_eq!(decision.await.unwrap(), Verdict::Deny("full".to_owned()));
    }

    #[tokio::test]
    async fn a_silent_plugin_falls_back_to_allow() {
        let hub = PluginHub::new(Duration::from_millis(50));
        let (outbox, mut inbox) = mpsc::channel(8);
        hub.attach("network", &subscribe(Hook::Kicked, true), outbox);
        let verdict = hub
            .kicked(player(), "game".to_owned(), "bye".to_owned(), Vec::new())
            .await;
        assert_eq!(verdict, Verdict::Allow);
        assert!(event_of(inbox.recv().await.unwrap()).decide);
    }

    #[tokio::test]
    async fn without_plugins_every_hook_allows() {
        let hub = PluginHub::new(Duration::from_secs(1));
        let verdict = hub.initial_server(player(), vec!["lobby".to_owned()]).await;
        assert_eq!(verdict, Verdict::Allow);
    }

    #[tokio::test]
    async fn observers_are_told_but_never_asked() {
        let hub = PluginHub::new(Duration::from_secs(1));
        let (outbox, mut inbox) = mpsc::channel(8);
        hub.attach("stats", &subscribe(Hook::PreConnect, false), outbox);
        let verdict = hub
            .pre_connect(player(), "game".to_owned(), ConnectCause::Join)
            .await;
        assert_eq!(verdict, Verdict::Allow);
        assert!(!event_of(inbox.recv().await.unwrap()).decide);
        hub.login(&player());
        assert!(inbox.try_recv().is_err());
    }

    #[tokio::test]
    async fn a_reconnecting_plugin_replaces_its_old_connection() {
        let hub = PluginHub::new(Duration::from_secs(1));
        let (first, _first_inbox) = mpsc::channel(8);
        let (second, _second_inbox) = mpsc::channel(8);
        let old = hub.attach("network", &[], first);
        let new = hub.attach("network", &[], second);
        assert!(!hub.is_attached(old));
        assert!(hub.is_attached(new));
        assert_eq!(hub.names(), ["network"]);
    }
}
