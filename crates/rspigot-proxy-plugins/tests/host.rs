use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use rspigot_plugin_protocol::proxy::v1::proxy_host_client::ProxyHostClient;
use rspigot_plugin_protocol::proxy::v1::{
    self, CommandSpec, Empty, FromPlugin, Hello, Order, Question, RegisterServer, ToPlugin, answer,
    from_plugin, order, question, to_plugin,
};
use rspigot_proxy_core::ServerRegistry;
use rspigot_proxy_network::{ProxySettings, Shared};
use rspigot_proxy_plugins::{PluginHub, serve};
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::Streaming;

fn settings() -> ProxySettings {
    ProxySettings {
        online_mode: false,
        motd: "proxy".to_owned(),
        max_players: 10,
        favicon: None,
        compression_threshold: None,
        compression_level: 6,
        forwarding_secret: b"s".to_vec(),
        login_timeout: Duration::from_secs(5),
        connect_timeout: Duration::from_secs(1),
    }
}

async fn start(token: Option<&str>) -> (String, Arc<Shared<(), PluginHub>>) {
    let registry = ServerRegistry::new(
        BTreeMap::from([("lobby".to_owned(), "127.0.0.1:1".to_owned())]),
        vec!["lobby".to_owned()],
    )
    .unwrap();
    let shared = Arc::new(
        Shared::new(
            settings(),
            registry,
            (),
            PluginHub::new(Duration::from_millis(200)),
        )
        .unwrap(),
    );
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(serve(
        listener,
        Arc::clone(&shared),
        token.map(str::to_owned),
    ));
    (url, shared)
}

fn hello(token: &str) -> FromPlugin {
    FromPlugin {
        body: Some(from_plugin::Body::Hello(Hello {
            protocol_version: rspigot_plugin_protocol::proxy::PROTOCOL_VERSION,
            name: "network".to_owned(),
            version: "1.0.0".to_owned(),
            token: token.to_owned(),
            subscriptions: Vec::new(),
            commands: vec![CommandSpec {
                name: "hub".to_owned(),
                aliases: vec!["lobby".to_owned()],
                permission: None,
                arguments: Vec::new(),
            }],
        })),
        request: None,
    }
}

async fn open(
    url: &str,
    first: FromPlugin,
) -> Result<(mpsc::Sender<FromPlugin>, Streaming<ToPlugin>), tonic::Status> {
    let channel = tonic::transport::Endpoint::from_shared(url.to_owned())
        .unwrap()
        .connect()
        .await
        .unwrap();
    let mut client = ProxyHostClient::new(channel);
    let (sender, receiver) = mpsc::channel(16);
    sender.send(first).await.unwrap();
    let stream = client
        .connect(ReceiverStream::new(receiver))
        .await?
        .into_inner();
    Ok((sender, stream))
}

async fn next(stream: &mut Streaming<ToPlugin>) -> to_plugin::Body {
    tokio::time::timeout(Duration::from_secs(5), stream.message())
        .await
        .expect("timed out")
        .unwrap()
        .unwrap()
        .body
        .unwrap()
}

async fn eventually(check: impl Fn() -> bool) {
    for _ in 0..100 {
        if check() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("condition never met");
}

fn send(sender: &mpsc::Sender<FromPlugin>, body: from_plugin::Body, request: Option<u64>) {
    sender
        .try_send(FromPlugin {
            body: Some(body),
            request,
        })
        .unwrap();
}

#[tokio::test]
async fn a_plugin_says_hello_and_gets_the_current_servers() {
    let (url, shared) = start(Some("t0k3n")).await;
    let (_sender, mut stream) = open(&url, hello("t0k3n")).await.unwrap();
    let to_plugin::Body::Accepted(accepted) = next(&mut stream).await else {
        panic!("expected accepted");
    };
    assert_eq!(accepted.servers.len(), 1);
    assert_eq!(accepted.servers[0].name, "lobby");
    assert!(accepted.players.is_empty());
    assert_eq!(shared.hooks().names(), ["network"]);
    let found = shared.state().commands.parse("lobby", |_| false);
    assert_eq!(
        found.map(|invocation| invocation.command).as_deref(),
        Some("hub")
    );
}

#[tokio::test]
async fn a_wrong_token_is_refused() {
    let (url, shared) = start(Some("t0k3n")).await;
    let refused = open(&url, hello("nope")).await;
    assert_eq!(refused.err().unwrap().code(), tonic::Code::Unauthenticated);
    assert!(shared.hooks().names().is_empty());
}

#[tokio::test]
async fn orders_and_questions_reach_the_proxy() {
    let (url, shared) = start(None).await;
    let (sender, mut stream) = open(&url, hello("")).await.unwrap();
    let _ = next(&mut stream).await;

    send(
        &sender,
        from_plugin::Body::Order(Order {
            body: Some(order::Body::RegisterServer(RegisterServer {
                name: "lobby-2".to_owned(),
                address: "127.0.0.1:2".to_owned(),
                fallback: true,
            })),
        }),
        None,
    );
    send(
        &sender,
        from_plugin::Body::Question(Question {
            body: Some(question::Body::Servers(Empty {})),
        }),
        Some(5),
    );
    let to_plugin::Body::Answer(v1::Answer {
        request,
        body: Some(answer::Body::Servers(list)),
    }) = next(&mut stream).await
    else {
        panic!("expected the server list");
    };
    assert_eq!(request, 5);
    let names: Vec<&str> = list
        .servers
        .iter()
        .map(|server| server.name.as_str())
        .collect();
    assert_eq!(names, ["lobby", "lobby-2"]);
    let candidates: Vec<String> = shared
        .state()
        .registry
        .join_candidates()
        .into_iter()
        .map(|server| server.name)
        .collect();
    assert_eq!(candidates, ["lobby", "lobby-2"]);

    send(
        &sender,
        from_plugin::Body::Question(Question {
            body: Some(question::Body::Player("Steve".to_owned())),
        }),
        Some(6),
    );
    let to_plugin::Body::Answer(v1::Answer {
        request: 6,
        body: Some(answer::Body::Error(_)),
    }) = next(&mut stream).await
    else {
        panic!("expected an error for an offline player");
    };
}

#[tokio::test]
async fn a_plugin_that_leaves_takes_its_commands_with_it() {
    let (url, shared) = start(None).await;
    let (sender, mut stream) = open(&url, hello("")).await.unwrap();
    let _ = next(&mut stream).await;
    assert!(!shared.state().commands.is_empty());
    drop(sender);
    drop(stream);
    eventually(|| shared.state().commands.is_empty()).await;
    eventually(|| shared.hooks().names().is_empty()).await;
}
