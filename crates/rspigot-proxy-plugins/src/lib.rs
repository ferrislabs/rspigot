mod convert;
mod hub;
mod service;

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use rspigot_plugin_protocol::proxy::v1::proxy_host_server::ProxyHostServer;
use rspigot_proxy_network::Shared;
use tokio::net::TcpListener;
use tonic::transport::Server;
use tonic::transport::server::TcpIncoming;

pub use crate::hub::PluginHub;
pub use crate::service::Host;

#[derive(Debug, thiserror::Error)]
pub enum PluginHostError {
    #[error("i/o: {0}")]
    Io(#[from] std::io::Error),
    #[error("gRPC transport: {0}")]
    Transport(#[from] tonic::transport::Error),
}

pub async fn serve<V: Send + Sync + 'static>(
    listener: TcpListener,
    shared: Arc<Shared<V, PluginHub>>,
    token: Option<String>,
) -> Result<(), PluginHostError> {
    let address: SocketAddr = listener.local_addr()?;
    tracing::info!("Proxy plugin host listening on {address}");
    let incoming = TcpIncoming::from(listener).with_nodelay(Some(true));
    Server::builder()
        .http2_keepalive_interval(Some(Duration::from_secs(10)))
        .http2_keepalive_timeout(Some(Duration::from_secs(20)))
        .add_service(ProxyHostServer::new(Host::new(shared, token)))
        .serve_with_incoming(incoming)
        .await?;
    Ok(())
}
