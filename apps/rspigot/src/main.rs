mod bundled_pack;
mod config;
mod console;
mod crash;
mod created_worlds;
mod dimensions;
mod files;
mod log_files;
mod logging;
mod restart;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use anyhow::Context;
use rspigot_anvil::{AnvilOptions, AnvilStore, Compression, DimensionLayout, DirectoryDatapacks};
use rspigot_auth::MojangSessionVerifier;
use rspigot_core::config::LevelData;
use rspigot_core::events::Inbound;
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_network::NetLink;
use tracing_subscriber::Layer;

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

fn init_logging(root: &std::path::Path) {
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;
    let filter =
        || tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into());
    let console = tracing_subscriber::fmt::layer()
        .event_format(logging::SpigotFormat)
        .with_filter(filter());
    let file = match log_files::LatestLog::open(root) {
        Ok(log) => Some(
            tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .with_writer(log)
                .event_format(logging::FileFormat)
                .with_filter(filter()),
        ),
        Err(error) => {
            eprintln!("could not open logs/latest.log: {error}");
            None
        }
    };
    tracing_subscriber::registry()
        .with(console)
        .with(file)
        .init();
}

fn main() -> anyhow::Result<()> {
    let args = arguments();
    std::fs::create_dir_all(&args.root)?;
    init_logging(&args.root);
    crash::install(args.root.clone());
    let started = Instant::now();
    tracing::info!(
        "Starting minecraft server version {}",
        rspigot_data::VERSION_NAME
    );
    let mut files = ServerFiles::load(&args.root).context("loading configuration")?;
    for warning in files.unapplied_keys() {
        tracing::warn!("{warning}");
    }
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
    let datapacks = world_root.join("datapacks");
    if let Err(error) = bundled_pack::install(&datapacks) {
        tracing::warn!("Could not install the bundled datapack: {error}");
    }
    for issue in rspigot_worldgen::install_datapacks(&datapacks) {
        tracing::warn!("Datapack problem in {issue}");
    }
    let generator = Arc::new(files.world_generator(level.seed));
    dimensions::install(Arc::clone(&store), Arc::clone(&generator));
    let status = StatusBoard::new(0);
    let mut game_config = files.game_config(admin);
    let user_cache_on_stop_only = files
        .spigot
        .string("settings.save-user-cache-on-stop-only", "false")
        .eq_ignore_ascii_case("true");
    game_config.user_cache = files::read_user_cache(
        &args.root,
        files
            .spigot
            .int("settings.user-cache-size", 1000)
            .clamp(0, 1_000_000) as usize,
    );
    if !user_cache_on_stop_only {
        let (user_cache_updates, user_cache_inbox) = crossbeam_channel::unbounded();
        files::spawn_user_cache_writer(args.root.clone(), user_cache_inbox);
        game_config.user_cache = game_config.user_cache.with_sink(user_cache_updates);
    }
    let user_cache = game_config.user_cache.clone();
    let mut network_config = files.network_config(files::read_favicon(&args.root))?;
    if network_config.online_mode {
        game_config.chat.services_keys =
            rspigot_auth::fetch_profile_keys(&rspigot_auth::UreqClient::new(), None);
    }
    network_config.code_of_conduct = files.code_of_conduct(&args.root);
    let enforce_secure_profile =
        files.secure_profile_requested() && !game_config.chat.services_keys.is_empty();
    game_config.chat.enforce_secure_profile = enforce_secure_profile;
    network_config.enforce_secure_profile = enforce_secure_profile;
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
    let management_config = files.management_config();
    let management_notices = management_config.as_ref().map(|config| {
        config.validate()?;
        let address = config.address();
        let listener = runtime
            .block_on(tokio::net::TcpListener::bind(&address))
            .with_context(|| format!("binding the management port {address}"))?;
        let acceptor = config.tls_acceptor()?;
        anyhow::Ok((listener, acceptor))
    });
    let management_listener = management_notices.transpose()?;
    let permissions_file = files.permissions_file();
    let permissions = config::permissions::load_or_create(&args.root.join(&permissions_file))
        .with_context(|| format!("loading {permissions_file}"))?;
    let (notice_sender, notice_receiver) = crossbeam_channel::unbounded();
    let created_worlds_file = created_worlds::path(&args.root);
    let (world_changes, world_updates) = crossbeam_channel::unbounded();
    created_worlds::spawn_writer(
        created_worlds_file.clone(),
        Arc::clone(&store),
        world_updates,
    );
    let restart_requested = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let restart_script = game_config.messages.restart_script.clone();
    let (timings_reports, timings_inbox) = crossbeam_channel::unbounded();
    files::spawn_timings_writer(args.root.clone(), timings_inbox);
    let mut server: Server<NetLink> = Server::new(
        game_config,
        level,
        Arc::clone(&store),
        generator,
        status.clone(),
    )
    .with_dimension_provider(dimensions::provide)
    .with_world_factory(dimensions::create)
    .with_worlds_sink(world_changes)
    .with_admin_sink(admin_updates)
    .with_player_store(Arc::clone(&store))
    .with_permissions(permissions)
    .with_datapacks(Arc::new(DirectoryDatapacks::new(&datapacks)))
    .with_startup_inbox(game_inbox.clone())
    .with_timings_sink(timings_reports)
    .with_restart_signal(Arc::clone(&restart_requested));
    let (reload_requests, reload_inbox) = crossbeam_channel::unbounded();
    files::spawn_reloader(args.root.clone(), reload_inbox, inbound.clone());
    server = server.with_reload_sink(reload_requests);
    if management_listener.is_some() {
        server = server.with_management_sink(notice_sender);
    }
    server.restore_worlds(created_worlds::load(&created_worlds_file));
    let spawn_chunks = server.start(Duration::from_secs(60));
    tracing::info!("Preparing spawn area: {spawn_chunks} chunks");
    let query = files.query_config().map(|config| {
        let status = status.clone();
        let stopped = stopped.clone();
        runtime.spawn(async move {
            if let Err(error) = rspigot_network::serve_query(config, status, stopped).await {
                tracing::error!("Unable to initialise query system: {error}");
            }
        })
    });
    let network = runtime.spawn(rspigot_network::run(
        network_config,
        verifier,
        status,
        inbound.clone(),
        stopped.clone(),
    ));
    let management = match (management_listener, management_config) {
        (Some((listener, acceptor)), Some(config)) => {
            Some(runtime.spawn(rspigot_management::serve(
                listener,
                config,
                inbound.clone(),
                rspigot_management::bridge(notice_receiver),
                stopped.clone(),
                acceptor,
            )))
        }
        _ => None,
    };
    let rcon = start_rcon(&runtime, files.rcon_config(), &inbound, &stopped);
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
    if user_cache_on_stop_only {
        files::save_user_cache(&args.root, &user_cache);
    }
    let _ = stop.send(true);
    runtime.block_on(async {
        let adapters = async {
            let _ = network.await;
            let _ = plugins.await;
            if let Some(query) = query {
                let _ = query.await;
            }
            if let Some(management) = management {
                let _ = management.await;
            }
            if let Some(rcon) = rcon {
                let _ = rcon.await;
            }
        };
        let _ = tokio::time::timeout(Duration::from_secs(5), adapters).await;
    });
    rspigot_anvil::write_level_data(&world_root, &level)?;
    tracing::info!("Server stopped");
    if restart_requested.load(std::sync::atomic::Ordering::SeqCst) {
        restart::perform(&args.root, &restart_script);
    }
    Ok(())
}

fn start_rcon(
    runtime: &tokio::runtime::Runtime,
    config: Option<rspigot_rcon::RconConfig>,
    inbound: &crossbeam_channel::Sender<Inbound<NetLink>>,
    stopped: &tokio::sync::watch::Receiver<bool>,
) -> Option<tokio::task::JoinHandle<()>> {
    let config = config?;
    if let Err(error) = config.validate() {
        tracing::error!("{error}");
        return None;
    }
    let address = config.address();
    let listener = match runtime.block_on(tokio::net::TcpListener::bind(&address)) {
        Ok(listener) => listener,
        Err(error) => {
            tracing::error!("Failed to start RCON listener on {address}: {error}");
            return None;
        }
    };
    let executor = Arc::new(rspigot_core::rcon::RconGateway::new(inbound.clone()));
    Some(runtime.spawn(rspigot_rcon::serve(
        listener,
        config,
        executor,
        stopped.clone(),
    )))
}
