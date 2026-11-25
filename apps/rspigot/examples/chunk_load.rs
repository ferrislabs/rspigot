use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use anyhow::Context;
use rspigot_anvil::{AnvilStore, DimensionLayout};
use rspigot_core::config::{GameConfig, LevelData};
use rspigot_core::events::{ClientEvent, Inbound};
use rspigot_core::ports::ClientLink;
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_domain::player::{ClientSettings, GameProfile, PlayerId};
use rspigot_domain::text::Component;
use rspigot_worldgen::WorldGenerator;

#[derive(Clone, Default)]
struct Mute {
    events: Arc<AtomicU64>,
    shared: Arc<AtomicU64>,
}

impl ClientLink for Mute {
    fn send(&self, _: ClientEvent) {
        self.events.fetch_add(1, Ordering::Relaxed);
    }

    fn send_shared(&self, _: Arc<ClientEvent>) {
        self.shared.fetch_add(1, Ordering::Relaxed);
    }

    fn disconnect(&self, _: Component) {}

    fn is_connected(&self) -> bool {
        true
    }
}

fn join_players(server: &mut Server<Mute>, count: usize) -> Mute {
    let link = Mute::default();
    for index in 0..count {
        let name = format!("bot{index}");
        server.handle(Inbound::Join {
            profile: GameProfile {
                id: PlayerId::offline(&name),
                name,
                properties: Vec::new(),
            },
            settings: ClientSettings::default(),
            link: link.clone(),
            address: IpAddr::from([127, 0, 0, 1]),
            session: index as u64 + 1,
        });
        server.deliver();
    }
    link
}

struct Arguments {
    world: PathBuf,
    radius: i32,
    ticks: usize,
}

fn arguments() -> anyhow::Result<Arguments> {
    let mut args = std::env::args().skip(1);
    let world = PathBuf::from(
        args.next()
            .context("usage: chunk_load <world> <radius> <ticks>")?,
    );
    let radius = args.next().context("missing radius")?.parse()?;
    let ticks = args.next().context("missing ticks")?.parse()?;
    Ok(Arguments {
        world,
        radius,
        ticks,
    })
}

fn rss_kib() -> Option<u64> {
    let output = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .ok()?;
    String::from_utf8(output.stdout).ok()?.trim().parse().ok()
}

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn percentile(sorted: &[Duration], fraction: f64) -> Duration {
    let index = ((sorted.len() - 1) as f64 * fraction).round() as usize;
    sorted[index]
}

fn main() -> anyhow::Result<()> {
    let args = arguments()?;
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "warn".into()),
        )
        .init();
    let level = rspigot_anvil::read_level_data(&args.world)?.unwrap_or_else(|| LevelData {
        seed: 20_261_124,
        ..LevelData::default()
    });
    let store =
        Arc::new(AnvilStore::open(&args.world, DimensionLayout::Spigot).context("opening world")?);
    for issue in rspigot_worldgen::install_datapacks(&args.world.join("datapacks")) {
        eprintln!("datapack problem in {issue}");
    }
    let generator = Arc::new(WorldGenerator::from_level_type("default", level.seed, ""));
    let mut config = GameConfig {
        spawn_chunk_radius: args.radius,
        ..GameConfig::default()
    };
    if let Some(workers) = std::env::var("RSPIGOT_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
    {
        config.chunk_workers = workers;
    }
    let before = rss_kib();
    let started = Instant::now();
    let mut server: Server<Mute> =
        Server::new(config, level, store, generator, StatusBoard::new(0));
    let loaded = server.start(Duration::from_secs(240));
    let load_time = started.elapsed();
    let after_load = rss_kib();
    let players = std::env::var("RSPIGOT_PLAYERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0usize);
    let link = join_players(&mut server, players);
    for _ in 0..40 {
        server.tick();
    }
    link.events.store(0, Ordering::Relaxed);
    link.shared.store(0, Ordering::Relaxed);
    let profile_before = server.tick_profile();
    let mut samples = Vec::with_capacity(args.ticks);
    for _ in 0..args.ticks {
        let tick = Instant::now();
        server.tick();
        samples.push(tick.elapsed());
    }
    let after_ticks = rss_kib();
    let profile = server.tick_profile().since(&profile_before);
    samples.sort_unstable();
    let stats = server.chunk_stats();
    let total: Duration = samples.iter().sum();
    println!("chunks_loaded: {loaded}");
    println!("players: {players}");
    if !samples.is_empty() {
        let ticks = samples.len() as f64;
        println!(
            "events_per_tick: {:.1}",
            link.events.load(Ordering::Relaxed) as f64 / ticks
        );
        println!(
            "shared_events_per_tick: {:.1}",
            link.shared.load(Ordering::Relaxed) as f64 / ticks
        );
    }
    println!("chunks_pending: {}", stats.pending);
    println!("chunk_copies: {}", stats.copies);
    println!("stitch_queued_after_ticks: {}", stats.stitch_queued);
    println!("load_seconds: {:.1}", load_time.as_secs_f64());
    println!(
        "avg_load_ms_per_chunk: {:.2}",
        stats.average_load_time().as_secs_f64() * 1000.0
    );
    let kib = |value: Option<u64>| {
        value.map_or("n/a".to_owned(), |v| format!("{:.0}", v as f64 / 1024.0))
    };
    println!("rss_mib_start: {}", kib(before));
    println!("rss_mib_after_load: {}", kib(after_load));
    println!("rss_mib_after_ticks: {}", kib(after_ticks));
    if let (Some(start), Some(end)) = (before, after_load)
        && loaded > 0
    {
        println!(
            "kib_per_chunk: {:.1}",
            (end.saturating_sub(start)) as f64 / loaded as f64
        );
    }
    println!("ticks: {}", samples.len());
    if !samples.is_empty() {
        println!("tick_ms_min: {:.3}", samples[0].as_secs_f64() * 1000.0);
        println!(
            "tick_ms_p50: {:.3}",
            percentile(&samples, 0.5).as_secs_f64() * 1000.0
        );
        println!(
            "tick_ms_p99: {:.3}",
            percentile(&samples, 0.99).as_secs_f64() * 1000.0
        );
        println!(
            "tick_ms_max: {:.3}",
            samples[samples.len() - 1].as_secs_f64() * 1000.0
        );
        println!(
            "tick_ms_mean: {:.3}",
            total.as_secs_f64() * 1000.0 / samples.len() as f64
        );
    }
    println!("passes_per_tick_us (top 15):");
    for line in profile.lines().iter().take(15) {
        println!("  {line}");
    }
    Ok(())
}
