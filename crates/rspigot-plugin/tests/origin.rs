mod fake_host;

use std::sync::OnceLock;

use fake_host::*;
use rspigot_plugin::{BroadcastMessage, Plugin, Server, listeners, run_with};
use rspigot_plugin_protocol::v1::{self, event, from_plugin, server_order};

#[derive(Plugin)]
#[plugin(name = "echo", version = "1.0.0")]
struct Echo {
    server: OnceLock<Server>,
}

#[listeners]
impl Echo {
    async fn on_enable(&self, server: &Server) {
        server.message.broadcast("enabled");
        let _ = self.server.set(server.clone());
    }

    async fn decide_broadcast(&self, event: &mut BroadcastMessage) {
        if let Some(server) = self.server.get() {
            server
                .message
                .broadcast(format!("inner {}", event.message()));
        }
    }

    async fn watch_broadcast(&self, event: &BroadcastMessage) {
        if let Some(server) = self.server.get() {
            server
                .message
                .broadcast(format!("echo {}", event.message()));
        }
    }
}

fn handler(hello: &v1::Hello, access: v1::Access) -> u32 {
    hello
        .handlers
        .iter()
        .find(|spec| spec.access == access as i32)
        .map(|spec| spec.id)
        .expect("the plugin declares the handler")
}

fn said(text: &str, origin: Option<u64>) -> v1::FromPlugin {
    v1::FromPlugin {
        body: Some(from_plugin::Body::Server(v1::ServerOrder {
            order: Some(server_order::Order::Broadcast(text.to_owned())),
        })),
        origin,
        request: None,
        in_world: None,
    }
}

fn broadcast(message: &str) -> event::Payload {
    event::Payload::BroadcastMessage(v1::BroadcastMessage {
        message: message.to_owned(),
        recipients: Vec::new(),
    })
}

async fn next(session: &mut Session) -> v1::FromPlugin {
    within(session.hears.message())
        .await
        .expect("the stream is open")
        .expect("the plugin speaks")
}

#[tokio::test]
async fn an_order_sent_while_a_handler_runs_carries_the_event_it_handles() {
    let mut host = Host::start().await;
    let running = tokio::spawn(run_with(
        Echo {
            server: OnceLock::new(),
        },
        host.url.clone(),
        None,
    ));
    let knock = host.knock().await;
    let decider = handler(&knock.hello, v1::Access::Decide);
    let observer = handler(&knock.hello, v1::Access::Observe);
    let mut session = knock.accept();
    assert_eq!(next(&mut session).await, said("enabled", None));

    session.say(event(
        5,
        &[decider],
        v1::Phase::Decide,
        false,
        broadcast("hi"),
    ));
    assert_eq!(next(&mut session).await, said("inner hi", Some(5)));
    let decision = next(&mut session).await;
    assert!(
        matches!(
            decision,
            v1::FromPlugin {
                body: Some(from_plugin::Body::Decision(v1::Decision { event: 5, .. })),
                origin: None,
                request: None,
                in_world: None,
            }
        ),
        "{decision:?}"
    );

    session.say(event(
        6,
        &[observer],
        v1::Phase::Settled,
        false,
        broadcast("hi"),
    ));
    assert_eq!(next(&mut session).await, said("echo hi", Some(6)));
    running.abort();
}
