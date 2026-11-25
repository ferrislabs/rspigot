use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;
use tokio_stream::wrappers::UnboundedReceiverStream;
use tonic::Code;
use tonic::transport::Endpoint;

use crate::wire::{
    self, Decision, FromPlugin, Hello, ToPlugin, decision, event, from_plugin,
    proxy_host_client::ProxyHostClient, to_plugin,
};
use crate::{Cause, Error, Plugin, Proxy, Verdict};

pub const DEFAULT_URL: &str = "http://127.0.0.1:25579";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_BACKOFF: Duration = Duration::from_secs(10);

pub async fn run<P: Plugin>(plugin: P) -> Result<(), Error> {
    let url = std::env::var("RSPIGOT_PROXY_URL")
        .ok()
        .filter(|url| !url.is_empty())
        .unwrap_or_else(|| DEFAULT_URL.to_owned());
    let token = std::env::var("RSPIGOT_PROXY_TOKEN").ok();
    run_with(plugin, url, token, Proxy::default()).await
}

pub async fn run_with<P: Plugin>(
    plugin: P,
    url: impl Into<String>,
    token: Option<String>,
    proxy: Proxy,
) -> Result<(), Error> {
    let url = url.into();
    let endpoint = Endpoint::from_shared(url.clone()).map_err(|_| Error::Url(url.clone()))?;
    if !matches!(endpoint.uri().scheme_str(), Some("http" | "https")) {
        return Err(Error::Url(url));
    }
    let endpoint = endpoint.connect_timeout(CONNECT_TIMEOUT);
    let plugin = Arc::new(plugin);
    let hello = Hello {
        protocol_version: rspigot_plugin_protocol::proxy::PROTOCOL_VERSION,
        name: P::NAME.to_owned(),
        version: P::VERSION.to_owned(),
        token: token.unwrap_or_default(),
        subscriptions: plugin
            .subscriptions()
            .into_iter()
            .map(|(hook, decide)| wire::Subscription {
                hook: hook as i32,
                decide,
            })
            .collect(),
        commands: plugin.commands(),
    };
    let mut backoff = Duration::from_millis(500);
    loop {
        match session(&endpoint, &hello, &plugin, &proxy).await {
            Ok(()) => backoff = Duration::from_millis(500),
            Err(Error::Refused(status))
                if matches!(
                    status.code(),
                    Code::Unauthenticated | Code::FailedPrecondition | Code::InvalidArgument
                ) =>
            {
                return Err(Error::Refused(status));
            }
            Err(error) => tracing::warn!("Proxy {url} unavailable: {error}"),
        }
        proxy.detach();
        tracing::info!("Reconnecting to the proxy in {} ms", backoff.as_millis());
        tokio::time::sleep(backoff).await;
        backoff = (backoff * 2).min(MAX_BACKOFF);
    }
}

async fn session<P: Plugin>(
    endpoint: &Endpoint,
    hello: &Hello,
    plugin: &Arc<P>,
    proxy: &Proxy,
) -> Result<(), Error> {
    let channel = endpoint
        .connect()
        .await
        .map_err(|error| Error::Answer(error.to_string()))?;
    let mut client = ProxyHostClient::new(channel);
    let (outbox, outgoing) = mpsc::unbounded_channel();
    let _ = outbox.send(FromPlugin {
        body: Some(from_plugin::Body::Hello(hello.clone())),
        request: None,
    });
    let mut incoming = client
        .connect(UnboundedReceiverStream::new(outgoing))
        .await
        .map_err(Error::Refused)?
        .into_inner();
    proxy.attach(outbox);
    tracing::info!("Connected to the proxy as {}", P::NAME);
    loop {
        match incoming.message().await {
            Ok(Some(message)) => dispatch(plugin, proxy, message),
            Ok(None) => return Ok(()),
            Err(status) => return Err(Error::Answer(status.message().to_owned())),
        }
    }
}

fn dispatch<P: Plugin>(plugin: &Arc<P>, proxy: &Proxy, message: ToPlugin) {
    let Some(body) = message.body else {
        return;
    };
    match body {
        to_plugin::Body::Answer(answer) => proxy.resolve(answer),
        to_plugin::Body::Warning(warning) => {
            tracing::warn!("The proxy warns: {}", warning.message);
        }
        to_plugin::Body::Accepted(accepted) => {
            let (plugin, proxy) = (Arc::clone(plugin), proxy.clone());
            tokio::spawn(async move {
                plugin
                    .accepted(&proxy, &accepted.servers, &accepted.players)
                    .await;
            });
        }
        to_plugin::Body::Invocation(invocation) => {
            let (plugin, proxy) = (Arc::clone(plugin), proxy.clone());
            tokio::spawn(async move { plugin.command(&proxy, &invocation).await });
        }
        to_plugin::Body::Event(event) => {
            let (plugin, proxy) = (Arc::clone(plugin), proxy.clone());
            tokio::spawn(async move {
                let id = event.id;
                let decide = event.decide;
                let Some(body) = event.body else {
                    return;
                };
                let verdict = handle(&*plugin, &proxy, body).await;
                if decide {
                    proxy.send(
                        from_plugin::Body::Decision(Decision {
                            event: id,
                            verdict: Some(wire_verdict(verdict.unwrap_or(Verdict::Allow))),
                        }),
                        None,
                    );
                }
            });
        }
    }
}

async fn handle<P: Plugin>(plugin: &P, proxy: &Proxy, body: event::Body) -> Option<Verdict> {
    let missing = wire::Player::default;
    match body {
        event::Body::Login(login) => {
            plugin
                .login(proxy, &login.player.unwrap_or_else(missing))
                .await;
            None
        }
        event::Body::Disconnect(disconnect) => {
            plugin
                .disconnect(proxy, &disconnect.player.unwrap_or_else(missing))
                .await;
            None
        }
        event::Body::Connected(connected) => {
            plugin
                .connected(
                    proxy,
                    &connected.player.unwrap_or_else(missing),
                    &connected.server,
                    connected.previous.as_deref(),
                )
                .await;
            None
        }
        event::Body::InitialServer(initial) => Some(
            plugin
                .initial_server(
                    proxy,
                    &initial.player.unwrap_or_else(missing),
                    &initial.candidates,
                )
                .await,
        ),
        event::Body::PreConnect(pre) => {
            let cause = match wire::ConnectCause::try_from(pre.cause) {
                Ok(wire::ConnectCause::Plugin) => Cause::Plugin,
                Ok(wire::ConnectCause::Fallback) => Cause::Fallback,
                _ => Cause::Join,
            };
            Some(
                plugin
                    .pre_connect(
                        proxy,
                        &pre.player.unwrap_or_else(missing),
                        &pre.target,
                        cause,
                    )
                    .await,
            )
        }
        event::Body::Kicked(kicked) => Some(
            plugin
                .kicked(
                    proxy,
                    &kicked.player.unwrap_or_else(missing),
                    &kicked.server,
                    &kicked.reason,
                    &kicked.fallbacks,
                )
                .await,
        ),
    }
}

fn wire_verdict(verdict: Verdict) -> decision::Verdict {
    match verdict {
        Verdict::Allow => decision::Verdict::Allow(wire::Allow {}),
        Verdict::Deny(message) => decision::Verdict::Deny(wire::Deny { message }),
        Verdict::Redirect { server, message } => {
            decision::Verdict::Redirect(wire::Redirect { server, message })
        }
    }
}
