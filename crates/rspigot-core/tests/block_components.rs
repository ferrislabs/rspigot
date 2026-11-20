mod common;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rand::SeedableRng;
use rand::rngs::StdRng;
use rspigot_blocks::BlockKind;
use rspigot_core::admin::{AdminLists, ListEntry};
use rspigot_core::config::{GameConfig, GameRules, LevelData};
use rspigot_core::events::{ClientEvent, DigAction, Direction, Hand, Inbound, PlayerInput};
use rspigot_core::loot::{self, LootContext};
use rspigot_core::ports::{ChunkGenerator, ChunkStore, ClientLink, EntityStore, StoreError};
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_domain::chunk::{BlockEntity, ChunkColumn};
use rspigot_domain::item::ItemStack;
use rspigot_domain::player::{ClientSettings, GameProfile, PlayerId};
use rspigot_domain::text::Component;
use rspigot_domain::world::Dimension;
use rspigot_domain::{BiomeId, BlockPos, BlockStateId, ChunkPos, Identifier, Vec3};
use rspigot_items::ItemKind;
use rspigot_nbt::{Compound, Tag};

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
            events.push(event);
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

const SHULKER: BlockPos = BlockPos::new(1, 64, 4);
const BANNER: BlockPos = BlockPos::new(3, 64, 4);
const FLOOR: BlockPos = BlockPos::new(2, 63, 2);
const PLACED: BlockPos = BlockPos::new(2, 64, 2);

fn stone_item() -> Compound {
    Compound::new()
        .with("id", "minecraft:stone")
        .with("count", 5)
        .with("Slot", 2_i8)
}

fn pattern() -> Tag {
    Tag::Compound(
        Compound::new()
            .with("color", "red")
            .with("pattern", "minecraft:stripe_top"),
    )
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
        if SHULKER.chunk() == pos {
            chunk.set_block(SHULKER, BlockKind::ShulkerBox.default_state());
            chunk.set_block_entity(BlockEntity {
                pos: SHULKER,
                kind: Identifier::minecraft("shulker_box"),
                data: Compound::new().with("Items", Tag::List(vec![Tag::Compound(stone_item())])),
            });
            chunk.set_block(BANNER, BlockKind::WhiteBanner.default_state());
            chunk.set_block_entity(BlockEntity {
                pos: BANNER,
                kind: Identifier::minecraft("banner"),
                data: Compound::new().with("patterns", Tag::List(vec![pattern()])),
            });
        }
        chunk
    }

    fn spawn_point(&self, _: &Dimension) -> Vec3 {
        Vec3::new(0.5, 64.0, 0.5)
    }
}

struct Player {
    id: PlayerId,
    link: Link,
    log: Vec<ClientEvent>,
}

fn server(rules: GameRules) -> Server<Link> {
    let config = GameConfig {
        view_distance: 3,
        chunk_workers: 2,
        spawn_protection: 0,
        admin: AdminLists {
            operators: vec![ListEntry::named("Alice")],
            whitelist: Vec::new(),
            whitelist_enabled: false,
            ..AdminLists::default()
        },
        rules,
        ..GameConfig::default()
    };
    let mut server = Server::new(
        config,
        LevelData::default(),
        Arc::new(NoStore),
        Arc::new(Floor),
        StatusBoard::new(20),
    );
    server.wait_for_spawn(Duration::from_secs(10));
    server
}

fn join(server: &mut Server<Link>, name: &str) -> Player {
    let link = Link::default();
    let id = PlayerId::offline(name);
    server.handle(Inbound::Join {
        profile: GameProfile {
            id,
            name: name.to_owned(),
            properties: Vec::new(),
        },
        settings: ClientSettings::default(),
        link: link.clone(),
        address: std::net::Ipv4Addr::LOCALHOST.into(),
        session: 0,
    });
    server.deliver();
    let mut player = Player {
        id,
        link,
        log: Vec::new(),
    };
    player.log.extend(player.link.take());
    for event in player.log.clone() {
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
    player
}

fn run_ticks(server: &mut Server<Link>, players: &mut [&mut Player], ticks: usize) {
    for _ in 0..ticks {
        server.wait_for_spawn(Duration::from_millis(20));
        server.tick();
        for player in players.iter_mut() {
            let events = player.link.take();
            let batches = events
                .iter()
                .filter(|e| matches!(e, ClientEvent::ChunkBatchFinished { .. }))
                .count();
            for _ in 0..batches {
                server.handle(Inbound::Input {
                    player: player.id,
                    input: PlayerInput::ChunkBatchReceived {
                        chunks_per_tick: 64.0,
                    },
                });
            }
            player.log.extend(events);
        }
    }
}

struct World {
    server: Server<Link>,
    alice: Player,
}

impl World {
    fn new() -> Self {
        let mut server = server(GameRules::default());
        let mut alice = join(&mut server, "Alice");
        run_ticks(&mut server, &mut [&mut alice], 30);
        Self { server, alice }
    }

    fn settle(&mut self) {
        self.server.deliver();
        run_ticks(&mut self.server, &mut [&mut self.alice], 2);
    }

    fn console(&mut self, line: &str) {
        self.server.handle(Inbound::Console(line.to_owned()));
        self.settle();
    }

    fn confirm_teleport(&mut self) {
        let teleport = self.alice.log.iter().rev().find_map(|event| match event {
            ClientEvent::Teleport { id, .. } => Some(*id),
            _ => None,
        });
        if let Some(teleport) = teleport {
            self.server.handle(Inbound::Input {
                player: self.alice.id,
                input: PlayerInput::TeleportConfirm(teleport),
            });
        }
    }

    fn dig(&mut self, action: DigAction, pos: BlockPos) {
        self.server.handle(Inbound::Input {
            player: self.alice.id,
            input: PlayerInput::Dig {
                action,
                pos,
                face: Direction::Up,
                sequence: 4,
            },
        });
    }

    fn survival_break(&mut self, pos: BlockPos) {
        self.dig(DigAction::Start, pos);
        run_ticks(&mut self.server, &mut [&mut self.alice], 360);
        self.confirm_teleport();
        self.server.handle(Inbound::Input {
            player: self.alice.id,
            input: PlayerInput::Move {
                position: None,
                rotation: None,
                on_ground: true,
            },
        });
        self.dig(DigAction::Finish, pos);
        self.settle();
    }

    fn pick_up_and_place(&mut self, near: BlockPos) {
        self.console(&format!("tp Alice {} 64 {}", near.x, near.z));
        self.confirm_teleport();
        run_ticks(&mut self.server, &mut [&mut self.alice], 40);
        self.server.handle(Inbound::Input {
            player: self.alice.id,
            input: PlayerInput::HeldSlot(0),
        });
        self.console("tp Alice 3 64 2");
        self.confirm_teleport();
        self.server.handle(Inbound::Input {
            player: self.alice.id,
            input: PlayerInput::UseItemOn {
                hand: Hand::Main,
                pos: FLOOR,
                face: Direction::Up,
                cursor: Vec3::new(0.5, 1.0, 0.5),
                inside_block: false,
                sequence: 7,
            },
        });
        self.settle();
    }

    fn ground(&self) -> Vec<ItemStack> {
        self.server
            .ground_items()
            .into_iter()
            .map(|(_, stack)| stack)
            .collect()
    }
}

fn kind_name(stack: &ItemStack) -> &'static str {
    stack.item.kind().map_or("air", ItemKind::name)
}

fn loot_of(block: BlockKind, tool: Option<&ItemStack>, data: &Compound) -> Vec<ItemStack> {
    let context =
        LootContext::block(block.default_state(), tool, None).with_block_entity(Some(data));
    loot::block_drops(&context, &mut StdRng::seed_from_u64(1))
}

fn only(drops: Vec<ItemStack>) -> ItemStack {
    assert_eq!(drops.len(), 1, "{drops:?}");
    drops.into_iter().next().expect("one drop")
}

#[test]
fn breaking_a_loaded_shulker_box_drops_one_box_holding_its_items() {
    let mut world = World::new();
    world.survival_break(SHULKER);

    let drops = world.ground();
    let boxes: Vec<&ItemStack> = drops
        .iter()
        .filter(|stack| kind_name(stack) == "minecraft:shulker_box")
        .collect();
    assert_eq!(boxes.len(), 1, "{drops:?}");
    assert!(
        drops
            .iter()
            .all(|stack| kind_name(stack) != "minecraft:stone")
    );
    let container = boxes[0]
        .components
        .get_list("minecraft:container")
        .expect("container component");
    let entry = container[0].as_compound().expect("entry");
    assert_eq!(entry.get_i32("slot"), Some(2));
    assert_eq!(
        entry
            .get_compound("item")
            .and_then(|item| item.get_str("id")),
        Some("minecraft:stone")
    );
}

#[test]
fn placing_the_dropped_shulker_box_fills_the_new_block_entity() {
    let mut world = World::new();
    world.survival_break(SHULKER);
    world.pick_up_and_place(SHULKER);

    let data = world.server.block_entity_nbt(PLACED).expect("block entity");
    let items = data.get_list("Items").expect("items");
    assert_eq!(items.len(), 1);
    let item = items[0].as_compound().expect("item");
    assert_eq!(item.get_i32("Slot"), Some(2));
    assert_eq!(item.get_str("id"), Some("minecraft:stone"));
    assert_eq!(item.get_i32("count"), Some(5));
}

#[test]
fn a_creative_player_breaking_a_loaded_shulker_box_drops_the_box_with_its_items() {
    let mut world = World::new();
    world.console("gamemode creative Alice");
    world.dig(DigAction::Start, SHULKER);
    world.settle();

    assert_eq!(
        world.server.block_state_at(SHULKER),
        Some(BlockStateId::AIR)
    );
    let drops = world.ground();
    assert_eq!(drops.len(), 1, "{drops:?}");
    assert_eq!(kind_name(&drops[0]), "minecraft:shulker_box");
    assert!(
        drops[0]
            .components
            .get_list("minecraft:container")
            .is_some()
    );
}

#[test]
fn a_creative_player_breaking_an_empty_shulker_box_drops_nothing() {
    let mut world = World::new();
    world.console("setblock 5 64 5 minecraft:shulker_box");
    world.console("gamemode creative Alice");
    world.dig(DigAction::Start, BlockPos::new(5, 64, 5));
    world.settle();

    assert_eq!(
        world.server.block_state_at(BlockPos::new(5, 64, 5)),
        Some(BlockStateId::AIR)
    );
    assert!(world.ground().is_empty());
}

#[test]
fn a_banner_keeps_its_patterns_through_break_and_place() {
    let mut world = World::new();
    world.survival_break(BANNER);
    let drops = world.ground();
    let banner = drops
        .iter()
        .find(|stack| kind_name(stack) == "minecraft:white_banner")
        .expect("banner item");
    assert_eq!(
        banner.components.get_list("minecraft:banner_patterns"),
        Some(&[pattern()][..])
    );

    world.pick_up_and_place(BANNER);
    let data = world
        .server
        .block_entity_nbt(PLACED)
        .expect("banner entity");
    assert_eq!(data.get_list("patterns"), Some(&[pattern()][..]));
}

#[test]
fn a_named_chest_keeps_its_custom_name() {
    let data = Compound::new().with("CustomName", "Treasure");
    let drop = only(loot_of(BlockKind::Chest, None, &data));
    assert_eq!(
        drop.components.get_str("minecraft:custom_name"),
        Some("Treasure")
    );
}

#[test]
fn a_skull_keeps_its_profile() {
    let profile = Compound::new().with("name", "Notch");
    let data = Compound::new().with("profile", profile.clone());
    let drop = only(loot_of(BlockKind::PlayerHead, None, &data));
    assert_eq!(
        drop.components.get_compound("minecraft:profile"),
        Some(&profile)
    );
}

#[test]
fn a_bee_nest_keeps_its_bees_when_mined_with_silk_touch() {
    let bees = Tag::List(vec![Tag::Compound(
        Compound::new()
            .with("ticks_in_hive", 12)
            .with("min_ticks_in_hive", 600),
    )]);
    let data = Compound::new().with("bees", bees.clone());
    let mut pickaxe = ItemStack::new(ItemKind::DiamondPickaxe.id(), std::num::NonZeroU8::MIN);
    pickaxe.set_enchantments(&[("minecraft:silk_touch".to_owned(), 1)]);

    let drop = only(loot_of(BlockKind::BeeNest, Some(&pickaxe), &data));
    assert_eq!(drop.components.get("minecraft:bees"), Some(&bees));
    assert!(loot_of(BlockKind::BeeNest, None, &data).is_empty());
}

#[test]
fn a_block_without_entity_data_drops_a_plain_item() {
    let drop = only(loot::block_drops(
        &LootContext::block(BlockKind::Chest.default_state(), None, None),
        &mut StdRng::seed_from_u64(1),
    ));
    assert!(drop.components.is_empty());
}
