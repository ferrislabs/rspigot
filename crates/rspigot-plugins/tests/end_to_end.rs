use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use rspigot_core::config::{GameConfig, LevelData};
use rspigot_core::events::{ClientEvent, Inbound};
use rspigot_core::ports::{ChunkGenerator, ChunkStore, ClientLink, EntityStore, StoreError};
use rspigot_core::server::Server as Core;
use rspigot_core::status::StatusBoard;
use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::text::Component;
use rspigot_domain::world::Dimension;
use rspigot_domain::{BiomeId, BlockPos, BlockStateId, ChunkPos, Identifier, Vec3};
use rspigot_nbt::Compound;
use rspigot_plugin::{
    Error, FluidCollisionMode, Ingredient, InventoryType, ItemStack, Material, MerchantProgress,
    NamespacedKey, PdcValue, Plugin, Position, RayTraceOptions, Recipe, Server, Trade, Vector,
    listeners, run_with,
};
use rspigot_plugins::serve;
use tokio::net::TcpListener;
use tokio::sync::{mpsc, watch};

struct NoLink;

impl ClientLink for NoLink {
    fn send(&self, _event: ClientEvent) {}

    fn disconnect(&self, _reason: Component) {}

    fn is_connected(&self) -> bool {
        false
    }
}

struct Floor;

impl ChunkGenerator for Floor {
    fn generate(&self, dimension: &Dimension, pos: ChunkPos) -> ChunkColumn {
        let mut chunk = ChunkColumn::empty(pos, dimension.height, BiomeId(0));
        for x in 0..16 {
            for z in 0..16 {
                for y in 60..=63 {
                    chunk.set_block(
                        BlockPos::new(pos.min_block_x() + x, y, pos.min_block_z() + z),
                        BlockStateId(1),
                    );
                }
            }
        }
        chunk
    }

    fn spawn_point(&self, _: &Dimension) -> Vec3 {
        Vec3::new(0.5, 64.0, 0.5)
    }
}

struct NoStore;

impl ChunkStore for NoStore {
    fn load(&self, _: &Identifier, _: ChunkPos) -> Result<Option<ChunkColumn>, StoreError> {
        Ok(None)
    }

    fn save(&self, _: &Identifier, _: &ChunkColumn) -> Result<(), StoreError> {
        Ok(())
    }

    fn flush(&self) -> Result<(), StoreError> {
        Ok(())
    }
}

impl EntityStore for NoStore {
    fn load_entities(
        &self,
        _: &Identifier,
        _: ChunkPos,
    ) -> Result<Option<Vec<Compound>>, StoreError> {
        Ok(None)
    }

    fn save_entities(&self, _: &Identifier, _: ChunkPos, _: &[Compound]) -> Result<(), StoreError> {
        Ok(())
    }
}

#[derive(Debug)]
enum Report {
    Ticked,
    Done(Result<Findings, String>),
}

#[derive(Debug)]
struct Findings {
    inventories: usize,
    merchant_progress: Option<MerchantProgress>,
    merchant_trades: usize,
    pickaxe_has_stone: bool,
    loot_tables: usize,
    loot_items: usize,
    world_data: Option<PdcValue>,
    ground: Option<rspigot_plugin::BlockPos>,
    biome: String,
    unknown_profile: bool,
    recipe_added: bool,
}

#[derive(Plugin)]
#[plugin(name = "e2e", version = "1.0.0")]
struct EndToEnd {
    reports: mpsc::UnboundedSender<Report>,
}

fn failed(error: Error) -> String {
    error.to_string()
}

#[listeners]
impl EndToEnd {
    async fn on_enable(&self, server: &Server) {
        let reports = self.reports.clone();
        let ticking = reports.clone();
        let timer = server.scheduler.run_timer(1, 2, move || {
            let ticking = ticking.clone();
            async move {
                let _ = ticking.send(Report::Ticked);
            }
        });
        let outcome = explore(server).await;
        timer.cancel();
        let _ = reports.send(Report::Done(outcome));
    }
}

async fn explore(server: &Server) -> Result<Findings, String> {
    let mut inventories = 0;
    for kind in InventoryType::ALL {
        let made = server
            .create_inventory_of(kind, "Any")
            .await
            .map_err(failed)?;
        let state = made.get_state().await.map_err(failed)?;
        if state.kind == kind {
            inventories += 1;
        }
    }
    let trade = Trade {
        result: ItemStack::new(Material::Diamond, 1),
        ingredients: vec![ItemStack::new(Material::Emerald, 2)],
        uses: 0,
        max_uses: 4,
        experience_reward: false,
        villager_experience: 0,
        price_multiplier: 0.0,
        demand: 0,
        special_price: 0,
    };
    let progress = MerchantProgress {
        level: 4,
        xp: 11,
        show_progress: true,
        can_restock: true,
    };
    let merchant = server
        .create_merchant_with_progress("Shop", std::slice::from_ref(&trade), progress)
        .await
        .map_err(failed)?;
    let state = merchant.get_state().await.map_err(failed)?;

    let pickaxe = server
        .registries
        .tag_members("block", "mineable/pickaxe")
        .await
        .map_err(failed)?;
    let tables = server.loot.tables().await.map_err(failed)?;
    let loot = server
        .loot
        .generate(&rspigot_plugin::LootRequest::new("minecraft:chests/spawn_bonus_chest").seed(1))
        .await
        .map_err(failed)?;

    let key = NamespacedKey::new("e2e", "mode").map_err(failed)?;
    server.world.pdc().set(&key, "arena");
    let world_data = server.world.pdc().get(&key).await.map_err(failed)?;

    let hit = server
        .world
        .ray_trace(
            Position {
                x: 5.5,
                y: 70.0,
                z: 5.5,
            },
            Vector {
                x: 0.0,
                y: -1.0,
                z: 0.0,
            },
            &RayTraceOptions {
                fluid: FluidCollisionMode::Never,
                ..Default::default()
            },
        )
        .await
        .map_err(failed)?;

    let block = server
        .world
        .block(rspigot_plugin::BlockPos { x: 4, y: 70, z: 4 });
    block.set_biome("minecraft:desert");
    let biome = block.get_biome().await.map_err(failed)?;

    let unknown_profile = matches!(
        server.profiles.by_name("Nobody").await,
        Err(Error::NotFound)
    );
    let recipe_added = server
        .recipes
        .add(Recipe::transmute(
            "e2e:sword",
            Ingredient::item(Material::DiamondSword),
            Ingredient::item(Material::Emerald),
            ItemStack::new(Material::NetheriteSword, 1),
        ))
        .await
        .is_ok();

    Ok(Findings {
        inventories,
        merchant_progress: state.progress,
        merchant_trades: state.trades.len(),
        pickaxe_has_stone: pickaxe.iter().any(|name| name == "minecraft:stone"),
        loot_tables: tables.len(),
        loot_items: loot.len(),
        world_data,
        ground: hit.block.map(|target| target.block.pos),
        biome,
        unknown_profile,
        recipe_added,
    })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_real_sdk_plugin_drives_the_real_core_through_the_host() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let (inbound, inbox) = crossbeam_channel::unbounded::<Inbound<NoLink>>();
    let (stop, stopped) = watch::channel(false);
    let running = Arc::new(AtomicBool::new(true));
    let alive = Arc::clone(&running);
    let core = std::thread::spawn(move || {
        let config = GameConfig {
            view_distance: 3,
            chunk_workers: 2,
            spawn_protection: 0,
            ..GameConfig::default()
        };
        let level = LevelData {
            spawn_mobs: false,
            ..LevelData::default()
        };
        let mut server = Core::<NoLink>::new(
            config,
            level,
            Arc::new(NoStore),
            Arc::new(Floor),
            StatusBoard::new(20),
        );
        server.wait_for_spawn(Duration::from_secs(10));
        while alive.load(Ordering::Acquire) {
            while let Ok(message) = inbox.try_recv() {
                server.handle(message);
            }
            server.tick();
            server.deliver();
            std::thread::sleep(Duration::from_millis(5));
        }
    });
    tokio::spawn(serve(listener, None, inbound, stopped));
    let (reports, mut heard) = mpsc::unbounded_channel();
    tokio::spawn(run_with(EndToEnd { reports }, url, None));

    let mut ticks = 0;
    let findings = loop {
        let report = tokio::time::timeout(Duration::from_secs(30), heard.recv())
            .await
            .expect("the plugin reports in time")
            .expect("the plugin is running");
        match report {
            Report::Ticked => ticks += 1,
            Report::Done(done) => break done.expect("the plugin explored the core"),
        }
    };
    assert_eq!(findings.inventories, InventoryType::ALL.len());
    assert_eq!(findings.merchant_trades, 1);
    assert_eq!(
        findings.merchant_progress,
        Some(MerchantProgress {
            level: 4,
            xp: 11,
            show_progress: true,
            can_restock: true
        })
    );
    assert!(findings.pickaxe_has_stone);
    assert!(findings.loot_tables > 1000);
    assert!(findings.loot_items > 0);
    assert_eq!(
        findings.world_data,
        Some(PdcValue::Text("arena".to_owned()))
    );
    assert_eq!(
        findings.ground,
        Some(rspigot_plugin::BlockPos { x: 5, y: 63, z: 5 })
    );
    assert_eq!(findings.biome, "minecraft:desert");
    assert!(findings.unknown_profile);
    assert!(findings.recipe_added);

    while ticks < 3 {
        match tokio::time::timeout(Duration::from_secs(10), heard.recv()).await {
            Ok(Some(Report::Ticked)) => ticks += 1,
            other => panic!("the repeating task stopped early: {other:?}"),
        }
    }
    let _ = stop.send(true);
    running.store(false, Ordering::Release);
    core.join().unwrap();
}
