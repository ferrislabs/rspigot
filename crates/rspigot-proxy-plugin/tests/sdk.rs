use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;
use std::time::Duration;

use rspigot_proxy_core::{
    ConnectCause, PlayerView, ProxyHooks, ServerRegistry, Verdict as CoreVerdict,
};
use rspigot_proxy_network::{ProxySettings, Shared};
use rspigot_proxy_plugin::{Cause, Hook, Player, Plugin, Proxy, Verdict, command, run_with};
use rspigot_proxy_plugins::{PluginHub, serve};
use tokio::net::TcpListener;

fn id() -> String {
    "00000000-0000-0000-0000-000000000001".to_owned()
}

struct Gate;

impl Plugin for Gate {
    const NAME: &'static str = "gate";
    const VERSION: &'static str = "1.0.0";

    fn subscriptions(&self) -> Vec<(Hook, bool)> {
        vec![(Hook::PreConnect, true)]
    }

    fn commands(&self) -> Vec<rspigot_proxy_plugin::wire::CommandSpec> {
        vec![command("hub").alias("lobby").build()]
    }

    async fn pre_connect(
        &self,
        _proxy: &Proxy,
        player: &Player,
        target: &str,
        cause: Cause,
    ) -> Verdict {
        if target.starts_with("game") && cause == Cause::Plugin {
            Verdict::Deny(format!("§c{} must queue first", player.name))
        } else {
            Verdict::Allow
        }
    }
}

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

fn steve() -> PlayerView {
    PlayerView {
        id: id().parse().unwrap(),
        name: "Steve".to_owned(),
        address: IpAddr::V4(Ipv4Addr::LOCALHOST),
        server: Some("lobby".to_owned()),
    }
}

async fn eventually(check: impl Fn() -> bool) {
    for _ in 0..200 {
        if check() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("condition never met");
}

#[tokio::test]
async fn a_sdk_plugin_registers_commands_decides_hooks_and_asks_questions() {
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
            PluginHub::new(Duration::from_secs(2)),
        )
        .unwrap(),
    );
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(serve(listener, Arc::clone(&shared), Some("t".to_owned())));

    let proxy = Proxy::default();
    tokio::spawn(run_with(Gate, url, Some("t".to_owned()), proxy.clone()));
    eventually(|| proxy.is_connected()).await;
    eventually(|| !shared.state().commands.is_empty()).await;

    let denied = shared
        .hooks()
        .pre_connect(steve(), "game-1".to_owned(), ConnectCause::Plugin)
        .await;
    assert_eq!(
        denied,
        CoreVerdict::Deny("§cSteve must queue first".to_owned())
    );
    let allowed = shared
        .hooks()
        .pre_connect(steve(), "game-1".to_owned(), ConnectCause::Join)
        .await;
    assert_eq!(allowed, CoreVerdict::Allow);

    assert!(proxy.register_server("lobby-2", "127.0.0.1:2", true));
    let servers = proxy.servers().await.unwrap();
    let names: Vec<&str> = servers.iter().map(|server| server.name.as_str()).collect();
    assert_eq!(names, ["lobby", "lobby-2"]);
    assert!(proxy.player("Steve").await.is_err());
}

#[tokio::test]
async fn a_wrong_token_stops_the_plugin() {
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
            PluginHub::new(Duration::from_secs(1)),
        )
        .unwrap(),
    );
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(serve(listener, shared, Some("right".to_owned())));
    let ended = tokio::time::timeout(
        Duration::from_secs(5),
        run_with(Gate, url, Some("wrong".to_owned()), Proxy::default()),
    )
    .await
    .expect("the plugin kept retrying");
    assert!(matches!(
        ended,
        Err(rspigot_proxy_plugin::Error::Refused(_))
    ));
}
