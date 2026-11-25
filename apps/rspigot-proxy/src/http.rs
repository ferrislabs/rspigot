use std::sync::Arc;

use axum::Router;
use axum::extract::State;
use axum::http::{StatusCode, header};
use axum::response::IntoResponse;
use axum::routing::get;
use rspigot_proxy_network::Shared;
use rspigot_proxy_plugins::PluginHub;
use tokio::net::TcpListener;

pub async fn serve<V: Send + Sync + 'static>(
    listener: TcpListener,
    shared: Arc<Shared<V, PluginHub>>,
) -> std::io::Result<()> {
    let app = Router::new()
        .route("/metrics", get(metrics::<V>))
        .route("/health/live", get(live))
        .route("/health/ready", get(ready::<V>))
        .with_state(shared);
    axum::serve(listener, app).await
}

async fn metrics<V: Send + Sync + 'static>(
    State(shared): State<Arc<Shared<V, PluginHub>>>,
) -> impl IntoResponse {
    let mut body = shared.render_metrics();
    body.push_str("# HELP rspigot_proxy_plugins Plugins connected to the proxy.\n");
    body.push_str("# TYPE rspigot_proxy_plugins gauge\n");
    body.push_str(&format!(
        "rspigot_proxy_plugins {}\n",
        shared.hooks().names().len()
    ));
    (
        [(
            header::CONTENT_TYPE,
            "text/plain; version=0.0.4; charset=utf-8",
        )],
        body,
    )
}

async fn live() -> StatusCode {
    StatusCode::OK
}

async fn ready<V: Send + Sync + 'static>(
    State(shared): State<Arc<Shared<V, PluginHub>>>,
) -> StatusCode {
    if shared.is_shutting_down() {
        StatusCode::SERVICE_UNAVAILABLE
    } else {
        StatusCode::OK
    }
}
