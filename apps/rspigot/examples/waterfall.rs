//! Reproduction harness: a stone platform in a void world, a row of water
//! sources on its edge, the water falls to the void (or onto a floor).
//! Prints a per-window timeline of tick time and outbound traffic.
//!
//! usage: waterfall <world-dir> <scenario: void|floor|idle> <width> <ticks> [players]

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
use rspigot_domain::BlockPos;
use rspigot_domain::player::{ClientSettings, GameProfile, PlayerId};
use rspigot_domain::text::Component;
use rspigot_worldgen::WorldGenerator;

#[derive(Clone, Default)]
struct Meter {
    light_packets: Arc<AtomicU64>,
    light_bytes: Arc<AtomicU64>,
    block_packets: Arc<AtomicU64>,
    other: Arc<AtomicU64>,
}

impl Meter {
    fn count(&self, event: &ClientEvent) {
        match event {
            ClientEvent::LightUpdate { light, .. } => {
                self.light_packets.fetch_add(1, Ordering::Relaxed);
                let arrays = light.sky.iter().filter(|(_, a)| a.is_some()).count()
                    + light.block.iter().filter(|(_, a)| a.is_some()).count();
                self.light_bytes
                    .fetch_add(arrays as u64 * 2048, Ordering::Relaxed);
            }
            ClientEvent::SectionBlocks { .. } | ClientEvent::BlockUpdate { .. } => {
                self.block_packets.fetch_add(1, Ordering::Relaxed);
            }
            _ => {
                self.other.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    fn take(&self) -> (u64, u64, u64, u64) {
        (
            self.light_packets.swap(0, Ordering::Relaxed),
            self.light_bytes.swap(0, Ordering::Relaxed),
            self.block_packets.swap(0, Ordering::Relaxed),
            self.other.swap(0, Ordering::Relaxed),
        )
    }
}

impl ClientLink for Meter {
    fn send(&self, event: ClientEvent) {
        self.count(&event);
    }

    fn send_shared(&self, event: Arc<ClientEvent>) {
        self.count(&event);
    }

    fn disconnect(&self, _: Component) {}

    fn is_connected(&self) -> bool {
        true
    }
}

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn console(server: &mut Server<Meter>, line: &str) {
    server.handle(Inbound::Console(line.to_owned()));
    server.deliver();
}

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let world = PathBuf::from(
        args.next()
            .context("usage: waterfall <world> <void|floor|idle> <width> <ticks> [players]")?,
    );
    let scenario = args.next().context("missing scenario")?;
    let width: i32 = args.next().context("missing width")?.parse()?;
    let ticks: usize = args.next().context("missing ticks")?.parse()?;
    let players: usize = args.next().map(|v| v.parse()).transpose()?.unwrap_or(2);
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "warn".into()),
        )
        .init();
    std::fs::create_dir_all(&world)?;
    let level = LevelData {
        seed: 1,
        spawn: Some(BlockPos::new(0, 100, 0)),
        ..LevelData::default()
    };
    let store =
        Arc::new(AnvilStore::open(&world, DimensionLayout::Spigot).context("opening world")?);
    let generator = Arc::new(WorldGenerator::from_level_type("void", level.seed, ""));
    let config = GameConfig {
        spawn_chunk_radius: 8,
        ..GameConfig::default()
    };
    let mut server: Server<Meter> =
        Server::new(config, level, store, generator, StatusBoard::new(0));
    let loaded = server.start(Duration::from_secs(120));
    println!("chunks_loaded: {loaded}");

    let link = Meter::default();
    for index in 0..players {
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
    let half = width / 2;
    // Platform 100 high, `width` wide along x, 24 deep along z (z in -12..=11).
    console(
        &mut server,
        &format!(
            "fill {} 100 -12 {} 100 11 minecraft:stone",
            -half - 2,
            half + 2
        ),
    );
    if scenario == "floor" {
        console(
            &mut server,
            &format!(
                "fill {} 30 -40 {} 30 60 minecraft:stone",
                -half - 40,
                half + 40
            ),
        );
    }
    if scenario == "shade" {
        // A roof over the platform and a back wall: the room under it is lit
        // only from the open side where the water falls, with a light gradient.
        console(
            &mut server,
            &format!(
                "fill {} 130 -12 {} 130 11 minecraft:stone",
                -half - 2,
                half + 2
            ),
        );
        console(
            &mut server,
            &format!(
                "fill {} 101 -12 {} 129 -12 minecraft:stone",
                -half - 2,
                half + 2
            ),
        );
        console(
            &mut server,
            &format!(
                "fill {} 101 -12 {} 129 11 minecraft:stone",
                -half - 2,
                -half - 2
            ),
        );
        console(
            &mut server,
            &format!(
                "fill {} 101 -12 {} 129 11 minecraft:stone",
                half + 2,
                half + 2
            ),
        );
    }
    console(&mut server, "tp @a 0 102 0");
    for _ in 0..100 {
        server.tick();
    }
    link.take();
    let profile_before = server.tick_profile();
    if scenario != "idle" {
        // Sources on the far edge row z=11: they flow to z=12 and fall.
        console(
            &mut server,
            &format!("fill {} 101 11 {} 101 11 minecraft:water", -half, half),
        );
    }
    println!("scenario: {scenario} width: {width} players: {players}");
    println!("window  tick_mean_ms  tick_max_ms  light_pkts  light_kib  block_pkts  other_pkts");
    let window = 100;
    let mut samples = Vec::with_capacity(ticks);
    let mut window_samples: Vec<Duration> = Vec::with_capacity(window);
    for index in 0..ticks {
        let started = Instant::now();
        server.tick();
        let took = started.elapsed();
        samples.push(took);
        window_samples.push(took);
        if window_samples.len() == window || index + 1 == ticks {
            let total: Duration = window_samples.iter().sum();
            let max = window_samples.iter().max().copied().unwrap_or_default();
            let (lp, lb, bp, op) = link.take();
            println!(
                "{:>6}  {:>12.3}  {:>11.3}  {:>10}  {:>9.0}  {:>10}  {:>10}",
                index + 1,
                total.as_secs_f64() * 1000.0 / window_samples.len() as f64,
                max.as_secs_f64() * 1000.0,
                lp,
                lb as f64 / 1024.0,
                bp,
                op
            );
            window_samples.clear();
        }
    }
    let profile = server.tick_profile().since(&profile_before);
    samples.sort_unstable();
    let pct =
        |f: f64| samples[((samples.len() - 1) as f64 * f).round() as usize].as_secs_f64() * 1000.0;
    println!("tick_ms_p50: {:.3}", pct(0.5));
    println!("tick_ms_p99: {:.3}", pct(0.99));
    println!("tick_ms_max: {:.3}", pct(1.0));
    let stats = server.chunk_stats();
    println!("chunk_copies: {}", stats.copies);
    println!("passes_per_tick_us (top 15):");
    for line in profile.lines().iter().take(15) {
        println!("  {line}");
    }
    Ok(())
}
