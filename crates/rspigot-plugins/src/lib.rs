mod config;
mod convert;
mod limits;
mod outbox;
mod roster;
mod service;
mod supervisor;

use std::time::Duration;

use crossbeam_channel::Sender;
use rspigot_core::events::Inbound;
use rspigot_core::ports::ClientLink;
use rspigot_plugin_protocol::v1::plugin_host_server::PluginHostServer;
use tokio::net::TcpListener;
use tokio::sync::watch;
use tonic::transport::Server;
use tonic::transport::server::TcpIncoming;
use tonic_health::ServingStatus;

pub use config::PluginHostConfig;
pub use limits::{
    MAX_DEPENDENCIES, MAX_HANDLERS, MAX_NAME_LENGTH, MESSAGE_BURST, MESSAGES_PER_SECOND,
    OUTBOX_CAPACITY,
};
pub use roster::{CONNECT_LIMIT, CONNECT_WINDOW, Dependencies, PluginState, Roster, RosterError};
pub use supervisor::{ExpectedPlugin, Launch, Launcher, MAX_STARTS, ProcessLauncher, START_WINDOW};

use crate::service::{Health, Host, Timing};

const INTRODUCTION_TIMEOUT: Duration = Duration::from_secs(10);
const SOFT_WAIT: Duration = Duration::from_secs(5);

#[derive(Clone)]
pub struct HostOptions {
    pub token: Option<String>,
    pub expected: Vec<ExpectedPlugin>,
    pub soft_wait: Duration,
}

impl HostOptions {
    pub fn new(token: Option<String>) -> Self {
        Self {
            token,
            expected: Vec::new(),
            soft_wait: SOFT_WAIT,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PluginHostError {
    #[error(
        "the plugin port listens beyond loopback, so it needs a token (`token` in plugins.yml or RSPIGOT_PLUGIN_TOKEN)"
    )]
    TokenRequired,
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
    #[error("gRPC transport error: {}", causes(.0))]
    Transport(tonic::transport::Error),
}

fn causes(error: &tonic::transport::Error) -> String {
    let mut causes: Vec<String> =
        std::iter::successors(std::error::Error::source(error), |cause| cause.source())
            .map(ToString::to_string)
            .collect();
    causes.dedup();
    if causes.is_empty() {
        error.to_string()
    } else {
        causes.join(": ")
    }
}

pub async fn run<L: ClientLink>(
    config: PluginHostConfig,
    inbound: Sender<Inbound<L>>,
    stop: watch::Receiver<bool>,
) -> Result<(), PluginHostError> {
    config.validate()?;
    let listener = TcpListener::bind((config.host, config.port)).await?;
    serve(listener, config.token, inbound, stop).await
}

pub async fn serve<L: ClientLink>(
    listener: TcpListener,
    token: Option<String>,
    inbound: Sender<Inbound<L>>,
    stop: watch::Receiver<bool>,
) -> Result<(), PluginHostError> {
    let roster = Roster::new(Vec::new());
    serve_supervised(listener, HostOptions::new(token), roster, inbound, stop).await
}

pub async fn serve_supervised<L: ClientLink>(
    listener: TcpListener,
    options: HostOptions,
    roster: Roster,
    inbound: Sender<Inbound<L>>,
    stop: watch::Receiver<bool>,
) -> Result<(), PluginHostError> {
    let timing = Timing {
        introduction: INTRODUCTION_TIMEOUT,
        soft_wait: options.soft_wait,
    };
    serve_with(listener, options, roster, inbound, stop, timing).await
}

async fn serve_with<L: ClientLink>(
    listener: TcpListener,
    options: HostOptions,
    roster: Roster,
    inbound: Sender<Inbound<L>>,
    stop: watch::Receiver<bool>,
    timing: Timing,
) -> Result<(), PluginHostError> {
    let address = listener.local_addr()?;
    tracing::info!("Starting plugin host on {address}");
    let (reporter, health_service) = tonic_health::server::health_reporter();
    for name in roster.expected() {
        reporter
            .set_service_status(&name, ServingStatus::NotServing)
            .await;
    }
    let health = Health::new(reporter.clone());
    let url = format!(
        "http://{}",
        std::net::SocketAddr::new(
            if address.ip().is_unspecified() {
                std::net::Ipv4Addr::LOCALHOST.into()
            } else {
                address.ip()
            },
            address.port()
        )
    );
    let launcher = std::sync::Arc::new(ProcessLauncher {
        url,
        token: options.token.clone(),
    });
    for plugin in options.expected {
        if plugin.launch.is_some() {
            let (launcher, roster, stop) = (launcher.clone(), roster.clone(), stop.clone());
            tokio::spawn(async move {
                supervisor::supervise(plugin, launcher.as_ref(), roster, stop).await;
            });
        }
    }
    let host = Host::new(options.token, inbound, health, stop.clone(), timing, roster);
    let incoming = TcpIncoming::from(listener).with_nodelay(Some(true));
    let mut stopped = stop;
    Server::builder()
        .http2_keepalive_interval(Some(Duration::from_secs(10)))
        .http2_keepalive_timeout(Some(Duration::from_secs(20)))
        .add_service(health_service)
        .add_service(PluginHostServer::new(host))
        .serve_with_incoming_shutdown(incoming, async move {
            service::stopped(&mut stopped).await;
            reporter
                .set_service_status("", ServingStatus::NotServing)
                .await;
        })
        .await
        .map_err(PluginHostError::Transport)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use rspigot_core::events::ClientEvent;
    use rspigot_domain::text::Component;
    use rspigot_plugin_protocol::v1;
    use rspigot_plugin_protocol::v1::plugin_host_client::PluginHostClient;
    use tokio::sync::mpsc;
    use tokio_stream::wrappers::ReceiverStream;
    use tonic::Code;
    use tonic::transport::Endpoint;

    use super::*;

    struct NoLink;

    impl ClientLink for NoLink {
        fn send(&self, _event: ClientEvent) {}

        fn disconnect(&self, _reason: Component) {}

        fn is_connected(&self) -> bool {
            false
        }
    }

    #[tokio::test]
    async fn a_silent_client_is_refused_after_ten_seconds() {
        assert_eq!(INTRODUCTION_TIMEOUT, Duration::from_secs(10));
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (inbound, inbox) = crossbeam_channel::unbounded::<Inbound<NoLink>>();
        let (_stop, stopped) = watch::channel(false);
        let wait = Duration::from_millis(200);
        let timing = Timing {
            introduction: wait,
            soft_wait: SOFT_WAIT,
        };
        tokio::spawn(serve_with(
            listener,
            HostOptions::new(None),
            Roster::new(Vec::new()),
            inbound,
            stopped,
            timing,
        ));
        let channel = Endpoint::from_shared(format!("http://{address}"))
            .unwrap()
            .connect()
            .await
            .unwrap();
        let (_says, said) = mpsc::channel::<v1::FromPlugin>(1);
        let started = Instant::now();

        let refused = tokio::time::timeout(
            Duration::from_secs(5),
            PluginHostClient::new(channel).connect(ReceiverStream::new(said)),
        )
        .await
        .expect("timed out")
        .unwrap_err();
        assert_eq!(refused.code(), Code::DeadlineExceeded);
        assert_eq!(refused.message(), "no introduction");
        assert!(started.elapsed() >= wait);
        assert!(inbox.try_recv().is_err());
    }

    #[tokio::test]
    async fn a_transport_error_shows_its_causes() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        drop(listener);
        let error = Endpoint::from_shared(format!("http://{address}"))
            .unwrap()
            .connect()
            .await
            .unwrap_err();
        let message = PluginHostError::Transport(error).to_string();
        assert!(message.starts_with("gRPC transport error: "), "{message}");
        assert!(message.contains("refused"), "{message}");
        let causes: Vec<&str> = message.split(": ").collect();
        assert!(
            causes.windows(2).all(|pair| pair[0] != pair[1]),
            "{message}"
        );
    }
}
