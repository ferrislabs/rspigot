mod config;
mod console;
mod dimensions;
mod files;
mod logging;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use anyhow::Context;
use rspigot_anvil::{AnvilOptions, AnvilStore, Compression, DimensionLayout};
use rspigot_auth::MojangSessionVerifier;
use rspigot_core::config::LevelData;
use rspigot_core::events::Inbound;
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_network::NetLink;
use rspigot_worldgen::WorldGenerator;

use crate::config::ServerFiles;

struct Arguments {
    root: PathBuf,
    port: Option<u16>,
    online_mode: Option<bool>,
}

fn arguments() -> Arguments {
    let mut args = Arguments {
        root: PathBuf::from("."),
        port: None,
        online_mode: None,
    };
    let mut iter = std::env::args().skip(1);
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-p" | "--port" | "--server-port" => {
                args.port = iter.next().and_then(|v| v.parse().ok())
            }
            "-W" | "--universe" | "--world-container" | "--world-dir" => {
                if let Some(dir) = iter.next() {
                    args.root = PathBuf::from(dir);
                }
            }
            "-o" | "--online-mode" => args.online_mode = iter.next().map(|v| v == "true"),
            _ => {}
        }
    }
    args
}

fn seed_from(raw: &str) -> i64 {
    if raw.is_empty() {
        return rand_seed();
    }
    raw.parse::<i64>().unwrap_or_else(|_| {
        raw.encode_utf16()
            .fold(0i32, |h, c| h.wrapping_mul(31).wrapping_add(i32::from(c)))
            .into()
    })
}

fn rand_seed() -> i64 {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    (nanos as i64) ^ 0x0005_DEEC_E66D
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .event_format(logging::SpigotFormat)
        .init();
    let started = Instant::now();
    let args = arguments();
    std::fs::create_dir_all(&args.root)?;
    tracing::info!(
        "Starting minecraft server version {}",
        rspigot_data::VERSION_NAME
    );
    let mut files = ServerFiles::load(&args.root).context("loading configuration")?;
    if let Some(port) = args.port {
        files.properties.set("server-port", &port.to_string());
    }
    if let Some(online) = args.online_mode {
        files.properties.set("online-mode", &online.to_string());
    }
    let admin = files::read_admin_lists(
        &args.root,
        files.properties.bool("white-list"),
        SystemTime::now(),
    );
    if admin.whitelist_enabled && admin.whitelist.is_empty() && admin.operators.is_empty() {
        tracing::warn!(
            "The whitelist is enabled and empty: no player can join. Run \"whitelist add <name>\" or set white-list=false in server.properties."
        );
    }
    let level_name = files.properties.string("level-name").to_owned();
    let world_root = args.root.join(&level_name);
    std::fs::create_dir_all(&world_root)?;
    let mut level = rspigot_anvil::read_level_data(&world_root)?.unwrap_or_else(|| LevelData {
        name: level_name.clone(),
        seed: seed_from(files.properties.string("level-seed")),
        ..LevelData::default()
    });
    level.name = level_name;
    level.uuid =
        Some(rspigot_anvil::read_or_create_world_uuid(&world_root).context("reading uid.dat")?);
    tracing::info!("Preparing level \"{}\"", level.name);
    let options = AnvilOptions {
        compression: Compression::from_property(files.properties.string("region-file-compression"))
            .unwrap_or(Compression::Zlib),
        sync_writes: files.properties.bool("sync-chunk-writes"),
        ..AnvilOptions::default()
    };
    let store = Arc::new(
        AnvilStore::open_with(&world_root, DimensionLayout::Spigot, options)
            .context("opening world")?,
    );
    let generator = Arc::new(WorldGenerator::from_level_type(
        files.properties.string("level-type"),
        level.seed,
        files.properties.string("generator-settings"),
    ));
    dimensions::install(Arc::clone(&store), Arc::clone(&generator));
    let status = StatusBoard::new(0);
    let game_config = files.game_config(admin);
    let network_config = files.network_config(files::read_favicon(&args.root))?;
    let plugin_host = files.plugin_host_config(std::env::var("RSPIGOT_PLUGIN_TOKEN").ok())?;
    plugin_host.validate()?;
    let verifier = Arc::new(MojangSessionVerifier::new(
        network_config.prevent_proxy_connections,
    ));
    let (inbound, game_inbox) = crossbeam_channel::unbounded::<Inbound<NetLink>>();
    let (admin_updates, admin_inbox) = crossbeam_channel::unbounded();
    files::spawn_admin_writer(args.root.clone(), admin_inbox);
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(files.spigot.int("settings.netty-threads", 4).clamp(1, 64) as usize)
        .thread_name("netty-worker")
        .enable_all()
        .build()?;
    let plugin_address = SocketAddr::from((plugin_host.host, plugin_host.port));
    let plugin_listener = runtime
        .block_on(tokio::net::TcpListener::bind(plugin_address))
        .with_context(|| format!("binding the plugin port {plugin_address}"))?;
    let (stop, stopped) = tokio::sync::watch::channel(false);
    let plugins = runtime.spawn({
        let serve = rspigot_plugins::serve(
            plugin_listener,
            plugin_host.token,
            inbound.clone(),
            stopped.clone(),
        );
        async move {
            if let Err(error) = serve.await {
                tracing::error!("The plugin host stopped: {error}");
            }
        }
    });
    let mut server: Server<NetLink> = Server::new(
        game_config,
        level,
        Arc::clone(&store),
        generator,
        status.clone(),
    )
    .with_dimension_provider(dimensions::provide)
    .with_admin_sink(admin_updates)
    .with_player_store(Arc::clone(&store))
    .with_startup_inbox(game_inbox.clone());
    let spawn_chunks = server.start(Duration::from_secs(60));
    tracing::info!("Preparing spawn area: {spawn_chunks} chunks");
    let network = runtime.spawn(rspigot_network::run(
        network_config,
        verifier,
        status,
        inbound.clone(),
        stopped,
    ));
    let signal_inbox = inbound.clone();
    runtime.spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            let _ = signal_inbox.send(Inbound::Shutdown);
        }
    });
    console::spawn(inbound);
    tracing::info!(
        "Done ({:.3}s)! For help, type \"help\"",
        started.elapsed().as_secs_f64()
    );
    let level = server.run(&game_inbox);
    drop(game_inbox);
    let _ = stop.send(true);
    runtime.block_on(async {
        let adapters = async {
            let _ = network.await;
            let _ = plugins.await;
        };
        let _ = tokio::time::timeout(Duration::from_secs(5), adapters).await;
    });
    rspigot_anvil::write_level_data(&world_root, &level)?;
    tracing::info!("Server stopped");
    Ok(())
}
