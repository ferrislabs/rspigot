mod config;

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context;
use rspigot_auth::MojangSessionVerifier;
use rspigot_proxy_core::NoHooks;
use rspigot_proxy_network::{Shared, serve};
use tokio::net::TcpListener;

use crate::config::{DEFAULT, FILE, ProxyFile};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();
    let root = std::env::args()
        .nth(1)
        .map_or_else(|| PathBuf::from("."), PathBuf::from);
    let path = root.join(FILE);
    if !path.exists() {
        std::fs::write(&path, DEFAULT)
            .with_context(|| format!("cannot write {}", path.display()))?;
        tracing::info!("Wrote a default {}", path.display());
    }
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("cannot read {}", path.display()))?;
    let file = ProxyFile::parse(&text)?;
    let settings = file.settings(&root)?;
    let registry = file.registry()?;
    let listener = TcpListener::bind(&file.bind)
        .await
        .with_context(|| format!("cannot listen on {}", file.bind))?;
    tracing::info!(
        "rspigot-proxy listening on {} (online mode: {}, try: {})",
        file.bind,
        settings.online_mode,
        file.try_order.join(", ")
    );
    let shared = Arc::new(Shared::new(
        settings,
        registry,
        MojangSessionVerifier::new(false),
        NoHooks,
    )?);
    tokio::select! {
        result = serve(listener, shared) => result?,
        _ = tokio::signal::ctrl_c() => tracing::info!("Stopping"),
    }
    Ok(())
}
