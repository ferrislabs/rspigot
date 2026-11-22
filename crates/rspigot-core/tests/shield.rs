use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rspigot_core::admin::{AdminLists, ListEntry};
use rspigot_core::config::{GameConfig, LevelData};
use rspigot_core::events::{ClientEvent, Hand, Inbound, PlayerInput};
use rspigot_core::ports::{ChunkGenerator, ChunkStore, ClientLink, EntityStore, StoreError};
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::player::{ClientSettings, GameProfile, PlayerId};
use rspigot_domain::text::Component;
use rspigot_domain::world::Dimension;
use rspigot_domain::{BiomeId, BlockPos, BlockStateId, ChunkPos, Identifier, Rotation, Vec3};
use rspigot_nbt::Compound;

#[derive(Clone, Default)]
struct Link {
    events: Arc<Mutex<Vec<ClientEvent>>>,
    closed: Arc<AtomicBool>,
}

impl Link {
    fn take(&self) -> Vec<ClientEvent> {
        self.events
            .lock()
            .map(|mut e| std::mem::take(&mut *e))
            .unwrap_or_default()
    }
}

impl ClientLink for Link {
    fn send(&self, event: ClientEvent) {
        if let Ok(mut events) = self.events.lock() {
            events.extend(event.expanded());
        }
    }

    fn disconnect(&self, _: Component) {
        self.closed.store(true, Ordering::Release);
    }

    fn is_connected(&self) -> bool {
        !self.closed.load(Ordering::Acquire)
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

const AIM: Rotation = Rotation::new(0.0, 0.0);

fn server() -> Server<Link> {
    let config = GameConfig {
        view_distance: 3,
        chunk_workers: 2,
        admin: AdminLists {
            operators: vec![ListEntry::named("Alice")],
            whitelist: Vec::new(),
            whitelist_enabled: false,
            ..AdminLists::default()
        },
        ..GameConfig::default()
    };
    let mut server = Server::new(
        config,
        LevelData::default(),
        Arc::new(NoStore),
        Arc::new(Floor),
        StatusBoard::new(20),
    )
    .with_rng_seed(5);
    server.wait_for_spawn(Duration::from_secs(10));
    server
}

struct Client {
    id: PlayerId,
    link: Link,
    log: Vec<ClientEvent>,
}

fn join_client(server: &mut Server<Link>) -> Client {
    let link = Link::default();
    let id = PlayerId::offline("Alice");
    server.handle(Inbound::Join {
        profile: GameProfile {
            id,
            name: "Alice".to_owned(),
            properties: Vec::new(),
        },
        settings: ClientSettings::default(),
        link: link.clone(),
        address: std::net::Ipv4Addr::LOCALHOST.into(),
        session: 0,
    });
    server.deliver();
    let mut client = Client {
        id,
        link,
        log: Vec::new(),
    };
    client.log.extend(client.link.take());
    for event in client.log.clone() {
        if let ClientEvent::Teleport { id: teleport, .. } = event {
            server.handle(Inbound::Input {
                player: id,
                input: PlayerInput::TeleportConfirm(teleport),
            });
        }
    }
    server.handle(Inbound::Input {
        player: id,
        input: PlayerInput::Loaded,
    });
    run_ticks(server, &mut client, 30);
    client
}

fn run_ticks(server: &mut Server<Link>, client: &mut Client, ticks: usize) {
    for _ in 0..ticks {
        server.wait_for_spawn(Duration::from_millis(20));
        server.tick();
        let events = client.link.take();
        let batches = events
            .iter()
            .filter(|e| matches!(e, ClientEvent::ChunkBatchFinished { .. }))
            .count();
        for _ in 0..batches {
            server.handle(Inbound::Input {
                player: client.id,
                input: PlayerInput::ChunkBatchReceived {
                    chunks_per_tick: 64.0,
                },
            });
        }
        client.log.extend(events);
    }
}

fn console(server: &mut Server<Link>, line: &str) {
    server.handle(Inbound::Console(line.to_owned()));
    server.deliver();
}

fn stand(server: &mut Server<Link>, client: &Client) {
    server.handle(Inbound::Input {
        player: client.id,
        input: PlayerInput::Move {
            position: Some(Vec3::new(0.5, 64.0, 0.5)),
            rotation: Some(AIM),
            on_ground: true,
        },
    });
}

#[test]
fn a_raised_shield_blocks_a_zombie_hit_from_the_front() {
    let mut server = server();
    let mut client = join_client(&mut server);
    console(&mut server, "time set 18000");
    console(&mut server, "give Alice minecraft:shield");
    run_ticks(&mut server, &mut client, 2);
    server.handle(Inbound::Input {
        player: client.id,
        input: PlayerInput::UseItem {
            hand: Hand::Main,
            sequence: 1,
            rotation: AIM,
        },
    });
    for _ in 0..8 {
        stand(&mut server, &client);
        run_ticks(&mut server, &mut client, 1);
    }
    console(&mut server, "summon minecraft:zombie 0.5 64 1.5");
    let mut blocked = false;
    let mut hurt = false;
    let player = server.player_entity_id(client.id).unwrap();
    for _ in 0..300 {
        stand(&mut server, &client);
        run_ticks(&mut server, &mut client, 1);
        blocked |= client.log.iter().any(|event| {
            matches!(event, ClientEvent::Sound { sound, .. } if *sound == "minecraft:item.shield.block")
        });
        hurt |= client.log.iter().any(
            |event| matches!(event, ClientEvent::DamageEvent { entity, .. } if *entity == player),
        );
        if blocked || hurt {
            break;
        }
    }
    assert!(blocked, "the shield never blocked the zombie, hurt={hurt}");
    assert!(!hurt, "the player was hurt through the shield");
    assert_eq!(server.player_health(client.id), Some(20.0));
}
