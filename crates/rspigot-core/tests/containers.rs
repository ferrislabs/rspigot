mod common;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rspigot_blocks::BlockKind;
use rspigot_core::admin::{AdminLists, ListEntry};
use rspigot_core::config::{GameConfig, GameRules, LevelData};
use rspigot_core::events::{
    ClickInput, ClientEvent, ContainerClick, DigAction, Direction, Hand, HashedSlot, Inbound,
    InputState, PlayerInput,
};
use rspigot_core::plugins::events::{BlockExp, BlockExpPatch};
use rspigot_core::plugins::{
    EventKind, EventPatch, EventPayload, FromPlugin, PluginIntro, Verdict,
};
use rspigot_core::ports::{ChunkGenerator, ChunkStore, ClientLink, EntityStore, StoreError};
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_domain::chunk::{BlockEntity, ChunkColumn};
use rspigot_domain::player::{ClientSettings, GameProfile, PlayerId};
use rspigot_domain::text::Component;
use rspigot_domain::world::Dimension;
use rspigot_domain::{BiomeId, BlockPos, BlockStateId, ChunkPos, Identifier, Vec3};
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

const LECTERN: BlockPos = BlockPos::new(4, 64, 4);

fn seed_lectern(chunk: &mut ChunkColumn) {
    let state = BlockKind::Lectern
        .default_state()
        .with_property("has_book", "true")
        .expect("has_book property");
    chunk.set_block(LECTERN, state);
    let pages: Vec<Tag> = ["one", "two", "three"]
        .into_iter()
        .map(|page| Tag::String(page.to_owned()))
        .collect();
    let book = Compound::new()
        .with("id", "minecraft:written_book")
        .with("count", 1)
        .with(
            "components",
            Compound::new().with(
                "minecraft:written_book_content",
                Compound::new().with("pages", Tag::List(pages)),
            ),
        );
    chunk.set_block_entity(BlockEntity {
        pos: LECTERN,
        kind: Identifier::minecraft("lectern"),
        data: Compound::new().with("Book", book).with("Page", 0),
    });
}

const FULL_BEACON: BlockPos = BlockPos::new(-4, 64, 4);
const SMALL_BEACON: BlockPos = BlockPos::new(4, 64, -4);

fn seed_beacons(chunk: &mut ChunkColumn, pos: ChunkPos) {
    let iron = BlockKind::IronBlock.default_state();
    for (beacon, layers) in [(FULL_BEACON, 4), (SMALL_BEACON, 1)] {
        for step in 1..=layers {
            for dx in -step..=step {
                for dz in -step..=step {
                    let at = BlockPos::new(beacon.x + dx, beacon.y - step, beacon.z + dz);
                    if at.chunk() == pos {
                        chunk.set_block(at, iron);
                    }
                }
            }
        }
        if beacon.chunk() == pos {
            chunk.set_block(beacon, BlockKind::Beacon.default_state());
            chunk.set_block_entity(BlockEntity {
                pos: beacon,
                kind: Identifier::minecraft("beacon"),
                data: Compound::new(),
            });
        }
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
        if LECTERN.chunk() == pos {
            seed_lectern(&mut chunk);
        }
        seed_beacons(&mut chunk, pos);
        chunk
    }

    fn spawn_point(&self, _: &Dimension) -> Vec3 {
        Vec3::new(0.5, 64.0, 0.5)
    }
}

const FLOOR: BlockPos = BlockPos::new(2, 63, 2);
const CHEST: BlockPos = BlockPos::new(2, 64, 2);
const ABOVE: BlockPos = BlockPos::new(2, 65, 2);

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
    bob: Player,
}

fn world() -> World {
    world_with(GameRules::default())
}

fn world_with(rules: GameRules) -> World {
    let mut server = server(rules);
    let mut alice = join(&mut server, "Alice");
    let mut bob = join(&mut server, "Bob");
    run_ticks(&mut server, &mut [&mut alice, &mut bob], 30);
    World { server, alice, bob }
}

impl World {
    fn settle(&mut self) {
        self.server.deliver();
        run_ticks(&mut self.server, &mut [&mut self.alice, &mut self.bob], 2);
    }

    fn console(&mut self, line: &str) {
        self.server.handle(Inbound::Console(line.to_owned()));
        self.settle();
    }

    fn select(&mut self, slot: u8) {
        self.server.handle(Inbound::Input {
            player: self.alice.id,
            input: PlayerInput::HeldSlot(slot),
        });
    }

    fn use_on(&mut self, pos: BlockPos, face: Direction) {
        self.server.handle(Inbound::Input {
            player: self.alice.id,
            input: PlayerInput::UseItemOn {
                hand: Hand::Main,
                pos,
                face,
                cursor: Vec3::new(0.5, 1.0, 0.5),
                inside_block: false,
                sequence: 7,
            },
        });
        self.settle();
    }

    fn give(&mut self, item: &str) {
        self.console(&format!("give Alice minecraft:{item}"));
    }
}

fn last_update(player: &Player, pos: BlockPos) -> Option<BlockStateId> {
    player.log.iter().rev().find_map(|event| match event {
        ClientEvent::BlockUpdate { pos: at, state } if *at == pos => Some(*state),
        _ => None,
    })
}

fn menu_events(player: &Player) -> Vec<&ClientEvent> {
    player
        .log
        .iter()
        .filter(|event| {
            matches!(
                event,
                ClientEvent::OpenScreen { .. }
                    | ClientEvent::ContainerContent { .. }
                    | ClientEvent::ContainerSlot { .. }
                    | ClientEvent::ContainerClose(_)
            )
        })
        .collect()
}

fn opened(player: &Player) -> Option<(i32, i32)> {
    player.log.iter().rev().find_map(|event| match event {
        ClientEvent::OpenScreen { window, menu, .. } => Some((*window, *menu)),
        _ => None,
    })
}

fn state_id(player: &Player, window: i32) -> i32 {
    player
        .log
        .iter()
        .rev()
        .find_map(|event| match event {
            ClientEvent::ContainerContent {
                window: at,
                state_id,
                ..
            }
            | ClientEvent::ContainerSlot {
                window: at,
                state_id,
                ..
            } if *at == window => Some(*state_id),
            _ => None,
        })
        .unwrap_or(0)
}

fn content(player: &Player, window: i32) -> Option<Vec<Option<(u16, u8)>>> {
    player.log.iter().rev().find_map(|event| match event {
        ClientEvent::ContainerContent {
            window: at, slots, ..
        } if *at == window => Some(
            slots
                .iter()
                .map(|slot| slot.as_ref().map(|stack| (stack.item.0, stack.count.get())))
                .collect(),
        ),
        _ => None,
    })
}

fn slot_update(player: &Player, window: i32, slot: i16) -> Option<Option<(u16, u8)>> {
    player.log.iter().rev().find_map(|event| match event {
        ClientEvent::ContainerSlot {
            window: at,
            slot: index,
            stack,
            ..
        } if *at == window && *index == slot => Some(
            stack
                .as_ref()
                .map(|stack| (stack.item.0, stack.count.get())),
        ),
        _ => None,
    })
}

fn closed(player: &Player, window: i32) -> bool {
    player
        .log
        .iter()
        .any(|event| matches!(event, ClientEvent::ContainerClose(at) if *at == window))
}

fn item(name: &str) -> u16 {
    rspigot_items::ItemKind::from_name(name)
        .expect("known item")
        .id()
        .0
}

impl World {
    fn click_as(&mut self, who: &Player, window: i32, slot: i16, button: i8, input: ClickInput) {
        let state = state_id(who, window);
        self.server.handle(Inbound::Input {
            player: who.id,
            input: PlayerInput::ContainerClick(ContainerClick {
                container: window,
                state_id: state,
                slot,
                button,
                input,
                changed: Vec::new(),
                carried: HashedSlot::default(),
            }),
        });
        self.settle();
    }

    fn click(&mut self, window: i32, slot: i16, button: i8, input: ClickInput) {
        let alice = Player {
            id: self.alice.id,
            link: self.alice.link.clone(),
            log: self.alice.log.clone(),
        };
        self.click_as(&alice, window, slot, button, input);
    }

    fn close(&mut self, window: i32) {
        self.server.handle(Inbound::Input {
            player: self.alice.id,
            input: PlayerInput::ContainerClose(window),
        });
        self.settle();
    }

    fn place_chest(&mut self) {
        self.give("chest");
        self.select(0);
        self.use_on(FLOOR, Direction::Up);
    }

    fn sneak(&mut self, shift: bool) {
        self.server.handle(Inbound::Input {
            player: self.alice.id,
            input: PlayerInput::Input(InputState {
                shift,
                ..InputState::default()
            }),
        });
    }

    fn open_chest(&mut self) {
        self.select(8);
        self.use_on(CHEST, Direction::Up);
    }

    fn open_chest_as_bob(&mut self) {
        self.server.handle(Inbound::Input {
            player: self.bob.id,
            input: PlayerInput::UseItemOn {
                hand: Hand::Main,
                pos: CHEST,
                face: Direction::Up,
                cursor: Vec3::new(0.5, 1.0, 0.5),
                inside_block: false,
                sequence: 9,
            },
        });
        self.settle();
    }
}

const HOTBAR_0: i16 = 54;
const WINDOW: i32 = 1;

#[test]
fn opening_a_chest_sends_the_screen_then_its_content() {
    let mut world = world();
    world.place_chest();
    world.open_chest();

    assert_eq!(opened(&world.alice), Some((WINDOW, 2)));
    let slots = content(&world.alice, WINDOW).expect("content");
    assert_eq!(slots.len(), 63);
    assert!(slots.iter().all(Option::is_none));
}

#[test]
fn items_put_in_the_chest_are_still_there_when_it_is_reopened() {
    let mut world = world();
    world.place_chest();
    world.give("stone");
    world.open_chest();
    world.click(WINDOW, HOTBAR_0, 0, ClickInput::Pickup);
    world.click(WINDOW, 3, 0, ClickInput::Pickup);
    world.close(WINDOW);
    world.open_chest();

    let slots = content(&world.alice, 2).expect("second window content");
    assert_eq!(slots[3], Some((item("minecraft:stone"), 1)));
    assert_eq!(slots[HOTBAR_0 as usize], None);
}

#[test]
fn a_click_for_a_window_that_is_not_open_changes_nothing() {
    let mut world = world();
    world.place_chest();
    world.give("stone");
    world.open_chest();
    world.click(77, HOTBAR_0, 0, ClickInput::Pickup);
    world.click(77, 3, 0, ClickInput::Pickup);
    world.close(WINDOW);
    world.open_chest();

    let slots = content(&world.alice, 2).expect("content");
    assert_eq!(slots[3], None);
}

#[test]
fn two_viewers_of_one_chest_see_each_others_changes() {
    let mut world = world();
    world.place_chest();
    world.give("stone");
    world.open_chest();
    world.open_chest_as_bob();
    assert_eq!(opened(&world.bob), Some((WINDOW, 2)));

    world.click(WINDOW, HOTBAR_0, 0, ClickInput::Pickup);
    world.click(WINDOW, 5, 0, ClickInput::Pickup);
    assert_eq!(
        slot_update(&world.bob, WINDOW, 5),
        Some(Some((item("minecraft:stone"), 1)))
    );
}

#[test]
fn closing_the_window_gives_back_the_stack_in_hand() {
    let mut world = world();
    world.place_chest();
    world.give("stone");
    world.open_chest();
    world.click(WINDOW, HOTBAR_0, 0, ClickInput::Pickup);
    world.close(WINDOW);

    let back = world.alice.log.iter().any(|event| {
        matches!(
            event,
            ClientEvent::PlayerInventorySlot { stack: Some(stack), .. }
                if stack.item.0 == item("minecraft:stone")
        )
    });
    assert!(back, "the carried stack returns to the inventory");
}

#[test]
fn a_solid_block_above_a_chest_keeps_it_shut() {
    let mut world = world();
    world.place_chest();
    world.give("stone");
    world.select(0);
    world.sneak(true);
    world.use_on(CHEST, Direction::Up);
    world.sneak(false);
    assert!(last_update(&world.alice, ABOVE).is_some());
    world.open_chest();

    assert_eq!(opened(&world.alice), None);
}

#[test]
fn breaking_the_chest_closes_the_window() {
    let mut world = world();
    world.place_chest();
    world.open_chest();
    world.server.handle(Inbound::Input {
        player: world.alice.id,
        input: PlayerInput::Dig {
            action: DigAction::Start,
            pos: CHEST,
            face: Direction::Up,
            sequence: 4,
        },
    });
    world.wait_to_finish();
    world.server.handle(Inbound::Input {
        player: world.alice.id,
        input: PlayerInput::Dig {
            action: DigAction::Finish,
            pos: CHEST,
            face: Direction::Up,
            sequence: 4,
        },
    });
    world.settle();

    assert!(closed(&world.alice, WINDOW));
}

#[test]
fn walking_away_closes_the_window() {
    let mut world = world();
    world.place_chest();
    world.open_chest();
    world.console("tp Alice 100 64 100");

    assert!(closed(&world.alice, WINDOW));
}

#[test]
fn a_stale_state_id_resends_the_whole_window() {
    let mut world = world();
    world.place_chest();
    world.open_chest();
    let before = menu_events(&world.alice)
        .iter()
        .filter(|event| matches!(event, ClientEvent::ContainerContent { .. }))
        .count();
    world.server.handle(Inbound::Input {
        player: world.alice.id,
        input: PlayerInput::ContainerClick(ContainerClick {
            container: WINDOW,
            state_id: 12345,
            slot: 0,
            button: 0,
            input: ClickInput::Pickup,
            changed: Vec::new(),
            carried: HashedSlot::default(),
        }),
    });
    world.settle();
    let after = menu_events(&world.alice)
        .iter()
        .filter(|event| matches!(event, ClientEvent::ContainerContent { .. }))
        .count();
    assert_eq!(after, before + 1);
}

fn lid_events(player: &Player) -> Vec<u8> {
    player
        .log
        .iter()
        .filter_map(|event| match event {
            ClientEvent::BlockEvent {
                pos,
                action: 1,
                data,
                ..
            } if *pos == CHEST => Some(*data),
            _ => None,
        })
        .collect()
}

fn sounds_heard(player: &Player, name: &str) -> usize {
    player
        .log
        .iter()
        .filter(|event| matches!(event, ClientEvent::Sound { sound, .. } if *sound == name))
        .count()
}

impl World {
    fn place_storage(&mut self, item: &str) {
        self.give(item);
        self.select(0);
        self.use_on(FLOOR, Direction::Up);
    }

    fn close_as_bob(&mut self, window: i32) {
        self.server.handle(Inbound::Input {
            player: self.bob.id,
            input: PlayerInput::ContainerClose(window),
        });
        self.settle();
    }
}

#[test]
fn each_storage_block_opens_its_own_menu() {
    let blocks = [
        ("trapped_chest", 2, 63),
        ("barrel", 2, 63),
        ("shulker_box", 20, 63),
        ("blue_shulker_box", 20, 63),
        ("hopper", 16, 41),
        ("dispenser", 6, 45),
        ("dropper", 6, 45),
    ];
    for (item, menu, slots) in blocks {
        let mut world = world();
        world.place_storage(item);
        world.open_chest();

        assert_eq!(opened(&world.alice), Some((WINDOW, menu)), "{item}");
        let content = content(&world.alice, WINDOW).unwrap_or_else(|| panic!("{item} content"));
        assert_eq!(content.len(), slots, "{item}");
    }
}

#[test]
fn the_chest_lid_follows_the_viewers_and_the_sound_the_first_and_last() {
    let mut world = world();
    world.place_chest();
    world.open_chest();
    assert_eq!(lid_events(&world.bob), vec![1]);
    assert_eq!(sounds_heard(&world.bob, "minecraft:block.chest.open"), 1);

    world.open_chest_as_bob();
    assert_eq!(lid_events(&world.bob), vec![1, 2]);
    assert_eq!(sounds_heard(&world.bob, "minecraft:block.chest.open"), 1);

    world.close(WINDOW);
    assert_eq!(lid_events(&world.bob), vec![1, 2, 1]);
    assert_eq!(sounds_heard(&world.bob, "minecraft:block.chest.close"), 0);

    world.close_as_bob(WINDOW);
    assert_eq!(lid_events(&world.bob), vec![1, 2, 1, 0]);
    assert_eq!(sounds_heard(&world.bob, "minecraft:block.chest.close"), 1);
}

#[test]
fn a_barrel_shows_it_is_open_while_it_is_viewed() {
    let open_state = |open: &str| {
        BlockKind::Barrel
            .default_state()
            .with_property("open", open)
            .expect("open property")
    };
    let mut world = world();
    world.place_storage("barrel");
    let shut = last_update(&world.alice, CHEST).expect("placed");
    assert_eq!(shut.property("open"), Some("false"));

    world.open_chest();
    let viewed = last_update(&world.alice, CHEST).expect("updated");
    assert_eq!(viewed.property("open"), Some("true"));
    assert_eq!(viewed.kind(), open_state("true").kind());
    assert_eq!(sounds_heard(&world.bob, "minecraft:block.barrel.open"), 1);

    world.close(WINDOW);
    let closed_again = last_update(&world.alice, CHEST).expect("updated");
    assert_eq!(closed_again.property("open"), Some("false"));
    assert_eq!(sounds_heard(&world.bob, "minecraft:block.barrel.close"), 1);
}

#[test]
fn a_shulker_box_with_a_block_in_front_does_not_open() {
    let mut world = world();
    world.place_storage("shulker_box");
    world.give("stone");
    world.select(0);
    world.sneak(true);
    world.use_on(CHEST, Direction::Up);
    world.sneak(false);
    assert!(last_update(&world.alice, ABOVE).is_some());
    world.open_chest();

    assert_eq!(opened(&world.alice), None);
}

#[test]
fn a_hopper_has_no_lid_and_makes_no_sound() {
    let mut world = world();
    world.place_storage("hopper");
    world.open_chest();

    assert!(lid_events(&world.bob).is_empty());
    assert!(
        !world
            .bob
            .log
            .iter()
            .any(|event| matches!(event, ClientEvent::Sound { .. }))
    );
}

fn data_events(player: &Player, window: i32) -> Vec<(i16, i16)> {
    player
        .log
        .iter()
        .filter_map(|event| match event {
            ClientEvent::ContainerData {
                window: at,
                id,
                value,
            } if *at == window => Some((*id, *value)),
            _ => None,
        })
        .collect()
}

fn open_as_bob(world: &mut World) {
    world.open_chest_as_bob();
}

const GRID_HOTBAR_0: i16 = 37;

#[test]
fn the_crafting_table_opens_a_grid_menu_with_a_result_slot() {
    let mut world = world();
    world.place_storage("crafting_table");
    world.open_chest();

    assert_eq!(opened(&world.alice), Some((WINDOW, 12)));
    let slots = content(&world.alice, WINDOW).expect("content");
    assert_eq!(slots.len(), 46);
}

#[test]
fn the_crafting_grid_comes_back_when_the_window_closes_and_is_not_kept() {
    let mut world = world();
    world.place_storage("crafting_table");
    world.give("stone");
    world.open_chest();
    world.click(WINDOW, GRID_HOTBAR_0, 0, ClickInput::Pickup);
    world.click(WINDOW, 1, 0, ClickInput::Pickup);
    world.close(WINDOW);

    let back = world.alice.log.iter().any(|event| {
        matches!(
            event,
            ClientEvent::PlayerInventorySlot { stack: Some(stack), .. }
                if stack.item.0 == item("minecraft:stone")
        )
    });
    assert!(back, "the grid item returns to the inventory");
    world.open_chest();
    let slots = content(&world.alice, 2).expect("second window");
    assert!(slots[..10].iter().all(Option::is_none));
}

#[test]
fn two_players_have_separate_crafting_grids() {
    let mut world = world();
    world.place_storage("crafting_table");
    world.give("stone");
    world.open_chest();
    open_as_bob(&mut world);
    world.click(WINDOW, GRID_HOTBAR_0, 0, ClickInput::Pickup);
    world.click(WINDOW, 1, 0, ClickInput::Pickup);

    assert_eq!(slot_update(&world.bob, WINDOW, 1), None);
    let bob_slots = content(&world.bob, WINDOW).expect("bob content");
    assert!(bob_slots[..10].iter().all(Option::is_none));
}

#[test]
fn the_crafting_result_slot_refuses_what_is_put_on_it() {
    let mut world = world();
    world.place_storage("crafting_table");
    world.give("stone");
    world.open_chest();
    world.click(WINDOW, GRID_HOTBAR_0, 0, ClickInput::Pickup);
    world.click(WINDOW, 0, 0, ClickInput::Pickup);

    assert_eq!(slot_update(&world.alice, WINDOW, 0), None);
    world.close(WINDOW);
    world.open_chest();
    let slots = content(&world.alice, 2).expect("second window");
    assert_eq!(slots[0], None);
}

fn inventory_slot(player: &Player, slot: usize) -> Option<(u16, u8)> {
    player
        .log
        .iter()
        .rev()
        .find_map(|event| match event {
            ClientEvent::InventorySlot {
                slot: index, stack, ..
            } if usize::try_from(*index) == Ok(slot) => Some(stack.clone()),
            ClientEvent::InventoryContent { slots, .. } => slots.get(slot).cloned(),
            _ => None,
        })
        .flatten()
        .map(|stack| (stack.item.0, stack.count.get()))
}

const PLAYER_GRID_HOTBAR_0: i16 = 36;

#[test]
fn a_recipe_in_the_crafting_table_shows_its_result_and_crafting_uses_the_ingredients() {
    let mut world = world();
    world.place_storage("crafting_table");
    world.give("oak_log");
    world.open_chest();
    world.click(WINDOW, GRID_HOTBAR_0, 0, ClickInput::Pickup);
    world.click(WINDOW, 1, 0, ClickInput::Pickup);

    assert_eq!(
        slot_update(&world.alice, WINDOW, 0),
        Some(Some((item("minecraft:oak_planks"), 4)))
    );
    world.click(WINDOW, 0, 0, ClickInput::QuickMove);

    assert_eq!(slot_update(&world.alice, WINDOW, 1), Some(None));
    assert_eq!(slot_update(&world.alice, WINDOW, 0), Some(None));
    assert_eq!(
        slot_update(&world.alice, WINDOW, 45),
        Some(Some((item("minecraft:oak_planks"), 4)))
    );
}

#[test]
fn dyeing_leather_armor_shows_the_dyed_result_to_the_client() {
    let mut world = world();
    world.place_storage("crafting_table");
    world.give("leather_boots");
    world.give("red_dye");
    world.open_chest();
    world.click(WINDOW, GRID_HOTBAR_0, 0, ClickInput::Pickup);
    world.click(WINDOW, 1, 0, ClickInput::Pickup);
    world.click(WINDOW, GRID_HOTBAR_0 + 1, 0, ClickInput::Pickup);
    world.click(WINDOW, 2, 0, ClickInput::Pickup);

    let result = world
        .alice
        .log
        .iter()
        .rev()
        .find_map(|event| match event {
            ClientEvent::ContainerSlot {
                window: WINDOW,
                slot: 0,
                stack: Some(stack),
                ..
            } => Some(stack.clone()),
            _ => None,
        })
        .expect("a result was sent");
    assert_eq!(result.item.0, item("minecraft:leather_boots"));
    assert_eq!(
        result.components.get_i32("minecraft:dyed_color"),
        Some(0xB02E26)
    );
}

#[test]
fn a_recipe_with_no_match_leaves_the_result_slot_empty() {
    let mut world = world();
    world.place_storage("crafting_table");
    world.give("dirt");
    world.open_chest();
    world.click(WINDOW, GRID_HOTBAR_0, 0, ClickInput::Pickup);
    world.click(WINDOW, 1, 0, ClickInput::Pickup);

    assert_eq!(slot_update(&world.alice, WINDOW, 0), None);
}

#[test]
fn the_player_crafting_grid_crafts_too() {
    let mut world = world();
    world.give("oak_log");
    world.click(0, PLAYER_GRID_HOTBAR_0, 0, ClickInput::Pickup);
    world.click(0, 1, 0, ClickInput::Pickup);

    assert_eq!(
        inventory_slot(&world.alice, 0),
        Some((item("minecraft:oak_planks"), 4))
    );
    world.click(0, 0, 0, ClickInput::QuickMove);

    assert_eq!(inventory_slot(&world.alice, 1), None);
    assert_eq!(inventory_slot(&world.alice, 0), None);
    assert_eq!(
        inventory_slot(&world.alice, 44),
        Some((item("minecraft:oak_planks"), 4))
    );
}

#[test]
fn each_furnace_opens_with_its_own_menu_and_four_data_slots() {
    for (item, menu) in [("furnace", 14), ("blast_furnace", 10), ("smoker", 22)] {
        let mut world = world();
        world.place_storage(item);
        world.open_chest();

        assert_eq!(opened(&world.alice), Some((WINDOW, menu)), "{item}");
        let slots = content(&world.alice, WINDOW).unwrap_or_else(|| panic!("{item} content"));
        assert_eq!(slots.len(), 39, "{item}");
        assert_eq!(
            data_events(&world.alice, WINDOW),
            vec![(0, 0), (1, 0), (2, 0), (3, 0)],
            "{item}"
        );
    }
}

#[test]
fn furnace_items_are_kept_and_the_result_slot_refuses_them() {
    let mut world = world();
    world.place_storage("furnace");
    world.give("iron_ore");
    world.open_chest();
    let main_slot = 3 + 27;
    world.click(WINDOW, main_slot, 0, ClickInput::Pickup);
    world.click(WINDOW, 2, 0, ClickInput::Pickup);
    world.click(WINDOW, 0, 0, ClickInput::Pickup);
    world.close(WINDOW);
    world.open_chest();

    let slots = content(&world.alice, 2).expect("second window");
    assert_eq!(slots[0], Some((item("minecraft:iron_ore"), 1)));
    assert_eq!(slots[2], None);
}

const RIGHT_HALF: BlockPos = BlockPos::new(3, 64, 2);

fn chest_type_at(player: &Player, pos: BlockPos) -> Option<String> {
    last_update(player, pos).and_then(|state| state.property("type").map(str::to_owned))
}

impl World {
    fn place_double_chest(&mut self) {
        self.give("chest");
        self.give("chest");
        self.select(0);
        self.use_on(FLOOR, Direction::Up);
        self.use_on(BlockPos::new(3, 63, 2), Direction::Up);
    }

    fn open_at(&mut self, pos: BlockPos) {
        self.select(8);
        self.use_on(pos, Direction::Up);
    }
}

#[test]
fn two_chests_side_by_side_become_the_two_halves_of_a_double_chest() {
    let mut world = world();
    world.place_double_chest();

    assert_eq!(chest_type_at(&world.alice, CHEST).as_deref(), Some("left"));
    assert_eq!(
        chest_type_at(&world.alice, RIGHT_HALF).as_deref(),
        Some("right")
    );
    assert_eq!(chest_type_at(&world.bob, CHEST).as_deref(), Some("left"));
}

#[test]
fn a_double_chest_opens_a_54_slot_window_from_either_half() {
    let mut world = world();
    world.place_double_chest();
    world.open_at(CHEST);

    assert_eq!(opened(&world.alice), Some((WINDOW, 5)));
    assert_eq!(content(&world.alice, WINDOW).expect("content").len(), 90);
    let title_is_double = world.alice.log.iter().any(|event| {
        matches!(event, ClientEvent::OpenScreen { title, .. }
            if format!("{title:?}").contains("container.chestDouble"))
    });
    assert!(title_is_double);
    world.close(WINDOW);
    world.open_at(RIGHT_HALF);
    assert_eq!(opened(&world.alice), Some((2, 5)));
}

#[test]
fn the_two_halves_hold_the_first_and_the_last_27_slots() {
    let mut world = world();
    world.place_double_chest();
    world.give("stone");
    world.open_at(CHEST);
    let hotbar_0 = 54 + 27;
    world.click(WINDOW, hotbar_0 as i16, 0, ClickInput::Pickup);
    world.click(WINDOW, 40, 0, ClickInput::Pickup);
    world.close(WINDOW);
    world.open_at(RIGHT_HALF);

    let slots = content(&world.alice, 2).expect("second window");
    assert_eq!(slots[40], Some((item("minecraft:stone"), 1)));
    assert_eq!(slots[..40].iter().flatten().count(), 0);
}

#[test]
fn both_halves_animate_and_the_sound_comes_once_from_the_middle() {
    let mut world = world();
    world.place_double_chest();
    world.open_at(CHEST);

    let events = |player: &Player, pos: BlockPos| -> Vec<u8> {
        player
            .log
            .iter()
            .filter_map(|event| match event {
                ClientEvent::BlockEvent {
                    pos: at,
                    action: 1,
                    data,
                    ..
                } if *at == pos => Some(*data),
                _ => None,
            })
            .collect()
    };
    assert_eq!(events(&world.bob, CHEST), vec![1]);
    assert_eq!(events(&world.bob, RIGHT_HALF), vec![1]);
    assert_eq!(sounds_heard(&world.bob, "minecraft:block.chest.open"), 1);
    let middle = world.bob.log.iter().find_map(|event| match event {
        ClientEvent::Sound {
            sound, position, ..
        } if *sound == "minecraft:block.chest.open" => Some(*position),
        _ => None,
    });
    assert_eq!(middle, Some(Vec3::new(3.0, 64.5, 2.5)));
}

#[test]
fn breaking_one_half_makes_the_other_a_single_chest_and_closes_the_window() {
    let mut world = world();
    world.place_double_chest();
    world.open_at(CHEST);
    world.server.handle(Inbound::Input {
        player: world.alice.id,
        input: PlayerInput::Dig {
            action: DigAction::Start,
            pos: RIGHT_HALF,
            face: Direction::Up,
            sequence: 4,
        },
    });
    world.wait_to_finish();
    world.server.handle(Inbound::Input {
        player: world.alice.id,
        input: PlayerInput::Dig {
            action: DigAction::Finish,
            pos: RIGHT_HALF,
            face: Direction::Up,
            sequence: 4,
        },
    });
    world.settle();

    assert!(closed(&world.alice, WINDOW));
    assert_eq!(
        chest_type_at(&world.alice, CHEST).as_deref(),
        Some("single")
    );
}

#[test]
fn a_blocked_partner_opens_only_the_clicked_half() {
    let mut world = world();
    world.place_double_chest();
    world.give("stone");
    world.select(0);
    world.sneak(true);
    world.use_on(RIGHT_HALF, Direction::Up);
    world.sneak(false);
    world.open_at(CHEST);

    assert_eq!(opened(&world.alice), Some((WINDOW, 2)));
    assert_eq!(content(&world.alice, WINDOW).expect("content").len(), 63);
}

#[test]
fn sneaking_keeps_two_neighbouring_chests_apart() {
    let mut world = world();
    world.give("chest");
    world.give("chest");
    world.select(0);
    world.sneak(true);
    world.use_on(FLOOR, Direction::Up);
    world.use_on(BlockPos::new(3, 63, 2), Direction::Up);
    world.sneak(false);

    assert_eq!(
        chest_type_at(&world.alice, CHEST).as_deref(),
        Some("single")
    );
    assert_eq!(
        chest_type_at(&world.alice, RIGHT_HALF).as_deref(),
        Some("single")
    );
}

fn carries_a_block_entity(player: &Player, pos: BlockPos, kind: &str) -> bool {
    player.log.iter().any(|event| match event {
        ClientEvent::Chunk { chunk, .. } => chunk
            .block_entity(pos)
            .is_some_and(|entity| entity.kind.to_string() == kind),
        _ => false,
    })
}

#[test]
fn a_container_placed_but_never_opened_is_in_the_chunk_sent_to_a_late_joiner() {
    let blocks = [
        ("chest", "minecraft:chest"),
        ("barrel", "minecraft:barrel"),
        ("furnace", "minecraft:furnace"),
        ("hopper", "minecraft:hopper"),
        ("shulker_box", "minecraft:shulker_box"),
    ];
    for (item, kind) in blocks {
        let mut world = world();
        world.place_storage(item);
        let mut carol = join(&mut world.server, "Carol");
        run_ticks(&mut world.server, &mut [&mut carol], 30);

        assert!(carries_a_block_entity(&carol, CHEST, kind), "{item}");
    }
}

#[test]
fn a_crafting_table_has_no_block_entity() {
    let mut world = world();
    world.place_storage("crafting_table");
    let mut carol = join(&mut world.server, "Carol");
    run_ticks(&mut world.server, &mut [&mut carol], 30);

    assert!(!carries_a_block_entity(
        &carol,
        CHEST,
        "minecraft:crafting_table"
    ));
}

fn ender_title(player: &Player) -> bool {
    player.log.iter().any(|event| {
        matches!(event, ClientEvent::OpenScreen { title, .. }
            if format!("{title:?}").contains("container.enderchest"))
    })
}

#[test]
fn an_ender_chest_opens_a_27_slot_window_with_its_own_title() {
    let mut world = world();
    world.place_storage("ender_chest");
    world.open_chest();

    assert_eq!(opened(&world.alice), Some((WINDOW, 2)));
    assert_eq!(content(&world.alice, WINDOW).expect("content").len(), 63);
    assert!(ender_title(&world.alice));
}

#[test]
fn the_ender_chest_items_belong_to_the_player_and_not_to_the_block() {
    let mut world = world();
    world.place_storage("ender_chest");
    world.give("stone");
    world.open_chest();
    world.click(WINDOW, HOTBAR_0, 0, ClickInput::Pickup);
    world.click(WINDOW, 4, 0, ClickInput::Pickup);
    world.close(WINDOW);
    world.open_chest_as_bob();

    let bob = content(&world.bob, WINDOW).expect("bob content");
    assert!(bob[..27].iter().all(Option::is_none));
    world.open_chest();
    let alice = content(&world.alice, 2).expect("alice content");
    assert_eq!(alice[4], Some((item("minecraft:stone"), 1)));
}

#[test]
fn the_ender_chest_lid_and_sound_follow_its_viewers() {
    let mut world = world();
    world.place_storage("ender_chest");
    world.open_chest();
    assert_eq!(lid_events(&world.bob), vec![1]);
    assert_eq!(
        sounds_heard(&world.bob, "minecraft:block.ender_chest.open"),
        1
    );

    world.close(WINDOW);
    assert_eq!(lid_events(&world.bob), vec![1, 0]);
    assert_eq!(
        sounds_heard(&world.bob, "minecraft:block.ender_chest.close"),
        1
    );
}

#[test]
fn a_solid_block_above_an_ender_chest_keeps_it_shut() {
    let mut world = world();
    world.place_storage("ender_chest");
    world.give("stone");
    world.select(0);
    world.sneak(true);
    world.use_on(CHEST, Direction::Up);
    world.sneak(false);
    world.open_chest();

    assert_eq!(opened(&world.alice), None);
}

#[test]
fn an_ender_chest_placed_but_never_opened_is_in_the_chunk_of_a_late_joiner() {
    let mut world = world();
    world.place_storage("ender_chest");
    let mut carol = join(&mut world.server, "Carol");
    run_ticks(&mut world.server, &mut [&mut carol], 30);

    assert!(carries_a_block_entity(
        &carol,
        CHEST,
        "minecraft:ender_chest"
    ));
}

const COPPER_CHESTS: [&str; 8] = [
    "copper_chest",
    "exposed_copper_chest",
    "weathered_copper_chest",
    "oxidized_copper_chest",
    "waxed_copper_chest",
    "waxed_exposed_copper_chest",
    "waxed_weathered_copper_chest",
    "waxed_oxidized_copper_chest",
];

fn kind_at(player: &Player, pos: BlockPos) -> Option<BlockKind> {
    last_update(player, pos).and_then(BlockStateId::kind)
}

impl World {
    fn place_pair_of(&mut self, first: &str, second: &str) {
        self.give(first);
        self.give(second);
        self.select(0);
        self.use_on(FLOOR, Direction::Up);
        self.select(1);
        self.use_on(BlockPos::new(3, 63, 2), Direction::Up);
    }
}

#[test]
fn every_copper_chest_opens_a_three_row_window() {
    for item in COPPER_CHESTS {
        let mut world = world();
        world.place_storage(item);
        world.open_chest();

        assert_eq!(opened(&world.alice), Some((WINDOW, 2)), "{item}");
        let slots = content(&world.alice, WINDOW).unwrap_or_else(|| panic!("{item} content"));
        assert_eq!(slots.len(), 63, "{item}");
    }
}

#[test]
fn copper_chest_items_are_kept_in_the_chest_block_entity() {
    let mut world = world();
    world.place_storage("copper_chest");
    world.give("stone");
    world.open_chest();
    world.click(WINDOW, 27 + 27, 0, ClickInput::Pickup);
    world.click(WINDOW, 4, 0, ClickInput::Pickup);
    world.close(WINDOW);
    world.open_chest();

    let slots = content(&world.alice, 2).expect("second window");
    assert_eq!(slots[4], Some((item("minecraft:stone"), 1)));
    let mut carol = join(&mut world.server, "Carol");
    run_ticks(&mut world.server, &mut [&mut carol], 30);
    assert!(carries_a_block_entity(&carol, CHEST, "minecraft:chest"));
}

#[test]
fn the_copper_chest_hinge_sound_follows_the_oxidation() {
    let sounds = [
        ("copper_chest", "copper_chest"),
        ("exposed_copper_chest", "copper_chest"),
        ("weathered_copper_chest", "copper_chest_weathered"),
        ("oxidized_copper_chest", "copper_chest_oxidized"),
        ("waxed_copper_chest", "copper_chest"),
        ("waxed_exposed_copper_chest", "copper_chest"),
        ("waxed_weathered_copper_chest", "copper_chest_weathered"),
        ("waxed_oxidized_copper_chest", "copper_chest_oxidized"),
    ];
    for (item, sound) in sounds {
        let mut world = world();
        world.place_storage(item);
        world.open_chest();
        let open = format!("minecraft:block.{sound}.open");
        let close = format!("minecraft:block.{sound}.close");
        assert_eq!(sounds_heard(&world.bob, &open), 1, "{item}");
        assert_eq!(
            sounds_heard(&world.bob, "minecraft:block.chest.open"),
            0,
            "{item}"
        );
        assert_eq!(lid_events(&world.bob), vec![1], "{item}");

        world.close(WINDOW);
        assert_eq!(sounds_heard(&world.bob, &close), 1, "{item}");
        assert_eq!(lid_events(&world.bob), vec![1, 0], "{item}");
    }
}

#[test]
fn a_solid_block_above_a_copper_chest_keeps_it_shut() {
    let mut world = world();
    world.place_storage("copper_chest");
    world.give("stone");
    world.select(0);
    world.sneak(true);
    world.use_on(CHEST, Direction::Up);
    world.sneak(false);
    assert!(last_update(&world.alice, ABOVE).is_some());
    world.open_chest();

    assert_eq!(opened(&world.alice), None);
}

#[test]
fn two_copper_chests_of_different_oxidation_make_one_double_chest_of_the_least_oxidized() {
    let mut world = world();
    world.place_pair_of("weathered_copper_chest", "exposed_copper_chest");

    for player in [&world.alice, &world.bob] {
        assert_eq!(kind_at(player, CHEST), Some(BlockKind::ExposedCopperChest));
        assert_eq!(
            kind_at(player, RIGHT_HALF),
            Some(BlockKind::ExposedCopperChest)
        );
        assert_eq!(chest_type_at(player, CHEST).as_deref(), Some("left"));
        assert_eq!(chest_type_at(player, RIGHT_HALF).as_deref(), Some("right"));
    }
    world.open_at(CHEST);
    assert_eq!(opened(&world.alice), Some((WINDOW, 5)));
    assert_eq!(content(&world.alice, WINDOW).expect("content").len(), 90);
}

#[test]
fn a_waxed_copper_chest_loses_its_wax_when_it_pairs_with_an_unwaxed_one() {
    let mut world = world();
    world.place_pair_of("waxed_copper_chest", "exposed_copper_chest");

    assert_eq!(kind_at(&world.alice, CHEST), Some(BlockKind::CopperChest));
    assert_eq!(
        kind_at(&world.alice, RIGHT_HALF),
        Some(BlockKind::CopperChest)
    );
    assert_eq!(chest_type_at(&world.alice, CHEST).as_deref(), Some("left"));
}

#[test]
fn two_waxed_copper_chests_stay_waxed() {
    let mut world = world();
    world.place_pair_of(
        "waxed_oxidized_copper_chest",
        "waxed_weathered_copper_chest",
    );

    assert_eq!(
        kind_at(&world.alice, CHEST),
        Some(BlockKind::WaxedWeatheredCopperChest)
    );
    assert_eq!(
        kind_at(&world.alice, RIGHT_HALF),
        Some(BlockKind::WaxedWeatheredCopperChest)
    );
}

#[test]
fn a_copper_chest_next_to_a_plain_chest_stays_a_single_chest() {
    let mut world = world();
    world.place_pair_of("chest", "copper_chest");

    assert_eq!(kind_at(&world.alice, CHEST), Some(BlockKind::Chest));
    assert_eq!(
        chest_type_at(&world.alice, CHEST).as_deref(),
        Some("single")
    );
    assert_eq!(
        chest_type_at(&world.alice, RIGHT_HALF).as_deref(),
        Some("single")
    );
}

#[test]
fn breaking_one_copper_half_makes_the_other_a_single_copper_chest() {
    let mut world = world();
    world.place_pair_of("copper_chest", "exposed_copper_chest");
    world.open_at(CHEST);
    world.server.handle(Inbound::Input {
        player: world.alice.id,
        input: PlayerInput::Dig {
            action: DigAction::Start,
            pos: RIGHT_HALF,
            face: Direction::Up,
            sequence: 4,
        },
    });
    world.wait_to_finish();
    world.server.handle(Inbound::Input {
        player: world.alice.id,
        input: PlayerInput::Dig {
            action: DigAction::Finish,
            pos: RIGHT_HALF,
            face: Direction::Up,
            sequence: 4,
        },
    });
    world.settle();

    assert!(closed(&world.alice, WINDOW));
    assert_eq!(kind_at(&world.alice, CHEST), Some(BlockKind::CopperChest));
    assert_eq!(
        chest_type_at(&world.alice, CHEST).as_deref(),
        Some("single")
    );
}

fn titled(player: &Player, key: &str) -> bool {
    player.log.iter().any(|event| {
        matches!(event, ClientEvent::OpenScreen { title, .. }
            if format!("{title:?}").contains(key))
    })
}

#[test]
fn each_station_opens_its_own_menu_with_its_title_and_data_slots() {
    let stations = [
        ("anvil", 8, 39, "container.repair", 1),
        ("chipped_anvil", 8, 39, "container.repair", 1),
        ("damaged_anvil", 8, 39, "container.repair", 1),
        ("grindstone", 15, 39, "container.grindstone_title", 0),
        ("stonecutter", 24, 38, "container.stonecutter", 1),
        (
            "cartography_table",
            23,
            39,
            "container.cartography_table",
            0,
        ),
        ("loom", 18, 40, "container.loom", 1),
        ("smithing_table", 21, 40, "container.upgrade", 1),
    ];
    for (block, menu, slots, title, data) in stations {
        let mut world = world();
        world.place_storage(block);
        world.open_chest();

        assert_eq!(opened(&world.alice), Some((WINDOW, menu)), "{block}");
        let content = content(&world.alice, WINDOW).unwrap_or_else(|| panic!("{block} content"));
        assert_eq!(content.len(), slots, "{block}");
        assert!(titled(&world.alice, title), "{block}");
        assert_eq!(
            data_events(&world.alice, WINDOW),
            (0..data).map(|id| (id, 0)).collect::<Vec<_>>(),
            "{block}"
        );
    }
}

#[test]
fn stations_have_no_block_entity() {
    for block in [
        "anvil",
        "grindstone",
        "stonecutter",
        "cartography_table",
        "loom",
        "smithing_table",
    ] {
        let mut world = world();
        world.place_storage(block);
        let mut carol = join(&mut world.server, "Carol");
        run_ticks(&mut world.server, &mut [&mut carol], 30);

        assert!(
            !carries_a_block_entity(&carol, CHEST, &format!("minecraft:{block}")),
            "{block}"
        );
    }
}

#[test]
fn the_items_put_in_a_station_come_back_when_its_window_closes() {
    let stations = [
        ("anvil", "iron_pickaxe", 30),
        ("grindstone", "iron_pickaxe", 30),
        ("smithing_table", "netherite_upgrade_smithing_template", 31),
        ("loom", "white_banner", 31),
        ("stonecutter", "stone", 29),
    ];
    for (block, stack, hotbar_0) in stations {
        let mut world = world();
        world.place_storage(block);
        world.give(stack);
        world.open_chest();
        world.click(WINDOW, hotbar_0, 0, ClickInput::Pickup);
        world.click(WINDOW, 0, 0, ClickInput::Pickup);
        assert_eq!(
            slot_update(&world.alice, WINDOW, 0),
            Some(Some((item(&format!("minecraft:{stack}")), 1))),
            "{block}"
        );
        world.close(WINDOW);
        world.open_chest();

        let slots = content(&world.alice, 2).expect("second window");
        assert!(slots[..3].iter().all(Option::is_none), "{block}");
        let back = world.alice.log.iter().rev().any(|event| {
            matches!(
                event,
                ClientEvent::PlayerInventorySlot { stack: Some(held), .. }
                    if held.item.0 == item(&format!("minecraft:{stack}"))
            )
        });
        assert!(back, "{block}");
    }
}

#[test]
fn the_grindstone_refuses_an_item_that_is_neither_damageable_nor_enchanted() {
    let mut world = world();
    world.place_storage("grindstone");
    world.give("stick");
    world.open_chest();
    world.click(WINDOW, 30, 0, ClickInput::Pickup);
    world.click(WINDOW, 0, 0, ClickInput::Pickup);
    world.close(WINDOW);
    world.open_chest();

    let slots = content(&world.alice, 2).expect("second window");
    assert!(slots[..3].iter().all(Option::is_none));
    let kept = world.alice.log.iter().any(|event| {
        matches!(
            event,
            ClientEvent::PlayerInventorySlot { stack: Some(stack), .. }
                if stack.item.0 == item("minecraft:stick")
        )
    });
    assert!(kept);
}

#[test]
fn breaking_a_station_closes_its_window_and_returns_the_items() {
    let mut world = world();
    world.place_storage("grindstone");
    world.give("iron_pickaxe");
    world.open_chest();
    world.click(WINDOW, 30, 0, ClickInput::Pickup);
    world.click(WINDOW, 0, 0, ClickInput::Pickup);
    world.server.handle(Inbound::Input {
        player: world.alice.id,
        input: PlayerInput::Dig {
            action: DigAction::Start,
            pos: CHEST,
            face: Direction::Up,
            sequence: 4,
        },
    });
    world.wait_to_finish();
    world.server.handle(Inbound::Input {
        player: world.alice.id,
        input: PlayerInput::Dig {
            action: DigAction::Finish,
            pos: CHEST,
            face: Direction::Up,
            sequence: 4,
        },
    });
    world.settle();

    assert!(closed(&world.alice, WINDOW));
    let back = world.alice.log.iter().rev().any(|event| {
        matches!(
            event,
            ClientEvent::PlayerInventorySlot { stack: Some(stack), .. }
                if stack.item.0 == item("minecraft:iron_pickaxe")
        )
    });
    assert!(back);
}

fn on_the_ground(world: &World, name: &str) -> Vec<u32> {
    let wanted = item(name);
    world
        .server
        .ground_items()
        .into_iter()
        .filter(|(_, stack)| stack.item.0 == wanted)
        .map(|(_, stack)| u32::from(stack.count.get()))
        .collect()
}

const MINING_TICKS: usize = 360;

impl World {
    fn wait_to_finish(&mut self) {
        self.wait(MINING_TICKS);
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
        self.server.handle(Inbound::Input {
            player: self.alice.id,
            input: PlayerInput::Move {
                position: None,
                rotation: None,
                on_ground: true,
            },
        });
    }

    fn step_away(&mut self) {
        self.console("tp Alice 6 64 2");
    }

    fn break_at(&mut self, pos: BlockPos) {
        self.server.handle(Inbound::Input {
            player: self.alice.id,
            input: PlayerInput::Dig {
                action: DigAction::Start,
                pos,
                face: Direction::Up,
                sequence: 4,
            },
        });
        self.wait_to_finish();
        self.server.handle(Inbound::Input {
            player: self.alice.id,
            input: PlayerInput::Dig {
                action: DigAction::Finish,
                pos,
                face: Direction::Up,
                sequence: 4,
            },
        });
        self.settle();
    }

    fn stock_first_slot(&mut self, hotbar_0: i16, item_name: &str) {
        self.give(item_name);
        self.open_chest();
        self.click(WINDOW, hotbar_0, 0, ClickInput::Pickup);
        self.click(WINDOW, 0, 0, ClickInput::Pickup);
        self.close(WINDOW);
    }
}

#[test]
fn breaking_a_container_drops_what_it_holds() {
    let containers = [
        ("chest", 54),
        ("barrel", 54),
        ("hopper", 32),
        ("dispenser", 36),
        ("furnace", 30),
        ("shulker_box", 54),
    ];
    for (block, hotbar_0) in containers {
        let mut world = world();
        world.place_storage(block);
        world.stock_first_slot(hotbar_0, "stone");
        assert!(
            on_the_ground(&world, "minecraft:stone").is_empty(),
            "{block}"
        );
        world.step_away();
        world.break_at(CHEST);

        assert_eq!(on_the_ground(&world, "minecraft:stone"), vec![1], "{block}");
    }
}

#[test]
fn without_block_drops_a_broken_chest_drops_what_it_holds_but_not_itself() {
    for block_drops in [true, false] {
        let mut world = world_with(GameRules {
            block_drops,
            ..GameRules::default()
        });
        world.place_chest();
        world.stock_first_slot(HOTBAR_0, "stone");
        world.step_away();
        world.break_at(CHEST);

        assert_eq!(
            on_the_ground(&world, "minecraft:stone"),
            vec![1],
            "{block_drops}"
        );
        let chest = if block_drops { vec![1] } else { Vec::new() };
        assert_eq!(
            on_the_ground(&world, "minecraft:chest"),
            chest,
            "{block_drops}"
        );
    }
}

#[test]
fn a_big_stack_drops_every_item_when_its_container_breaks() {
    let mut world = world();
    world.place_chest();
    world.console("give Alice minecraft:stone 64");
    world.open_chest();
    world.click(WINDOW, HOTBAR_0, 0, ClickInput::Pickup);
    world.click(WINDOW, 0, 0, ClickInput::Pickup);
    world.close(WINDOW);
    world.step_away();
    world.break_at(CHEST);

    let pieces = on_the_ground(&world, "minecraft:stone");
    assert_eq!(pieces.iter().sum::<u32>(), 64);
}

#[test]
fn breaking_one_half_of_a_double_chest_drops_only_its_own_items() {
    let mut world = world();
    world.place_double_chest();
    world.give("stone");
    world.give("dirt");
    world.open_at(CHEST);
    world.click(WINDOW, 54 + 27, 0, ClickInput::Pickup);
    world.click(WINDOW, 0, 0, ClickInput::Pickup);
    world.click(WINDOW, 54 + 28, 0, ClickInput::Pickup);
    world.click(WINDOW, 27, 0, ClickInput::Pickup);
    world.close(WINDOW);
    world.step_away();
    world.break_at(RIGHT_HALF);

    assert_eq!(on_the_ground(&world, "minecraft:stone"), vec![1]);
    assert!(on_the_ground(&world, "minecraft:dirt").is_empty());
    world.console("tp Alice 2.5 64 4.5");
    world.open_chest();
    let slots = content(&world.alice, 2).expect("second window");
    assert_eq!(slots[0], Some((item("minecraft:dirt"), 1)));
}

#[test]
fn opening_a_loaded_barrel_does_not_drop_its_items() {
    let mut world = world();
    world.place_storage("barrel");
    world.stock_first_slot(54, "stone");
    world.open_chest();

    assert!(on_the_ground(&world, "minecraft:stone").is_empty());
    let slots = content(&world.alice, 2).expect("second window");
    assert_eq!(slots[0], Some((item("minecraft:stone"), 1)));
}

#[test]
fn pairing_a_second_chest_with_a_loaded_one_does_not_drop_its_items() {
    let mut world = world();
    world.place_chest();
    world.stock_first_slot(54, "stone");
    world.give("chest");
    world.select(0);
    world.use_on(BlockPos::new(3, 63, 2), Direction::Up);
    world.open_at(CHEST);

    assert!(on_the_ground(&world, "minecraft:stone").is_empty());
    let slots = content(&world.alice, 2).expect("double window");
    assert_eq!(slots.len(), 90);
    assert_eq!(slots[27], Some((item("minecraft:stone"), 1)));
}

impl World {
    fn open_lectern(&mut self) {
        self.select(8);
        self.use_on(LECTERN, Direction::Up);
    }

    fn press(&mut self, window: i32, button: i32) {
        self.server.handle(Inbound::Input {
            player: self.alice.id,
            input: PlayerInput::ContainerButtonClick { window, button },
        });
        self.settle();
    }

    fn open_lectern_as_bob(&mut self) {
        self.server.handle(Inbound::Input {
            player: self.bob.id,
            input: PlayerInput::UseItemOn {
                hand: Hand::Main,
                pos: LECTERN,
                face: Direction::Up,
                cursor: Vec3::new(0.5, 1.0, 0.5),
                inside_block: false,
                sequence: 9,
            },
        });
        self.settle();
    }
}

fn page_shown(player: &Player, window: i32) -> Option<i16> {
    data_events(player, window).last().map(|(_, value)| *value)
}

#[test]
fn a_lectern_with_a_book_opens_a_one_slot_window_with_the_book_and_the_page() {
    let mut world = world();
    world.open_lectern();

    assert_eq!(opened(&world.alice), Some((WINDOW, 17)));
    assert!(titled(&world.alice, "container.lectern"));
    let slots = content(&world.alice, WINDOW).expect("content");
    assert_eq!(slots, vec![Some((item("minecraft:written_book"), 1))]);
    assert_eq!(data_events(&world.alice, WINDOW), vec![(0, 0)]);
}

#[test]
fn the_page_buttons_move_through_the_book_and_stop_at_both_ends() {
    let mut world = world();
    world.open_lectern();
    world.press(WINDOW, 2);
    assert_eq!(page_shown(&world.alice, WINDOW), Some(1));
    world.press(WINDOW, 2);
    world.press(WINDOW, 2);
    assert_eq!(page_shown(&world.alice, WINDOW), Some(2));
    world.press(WINDOW, 1);
    assert_eq!(page_shown(&world.alice, WINDOW), Some(1));
    world.press(WINDOW, 1);
    world.press(WINDOW, 1);
    assert_eq!(page_shown(&world.alice, WINDOW), Some(0));
    world.press(WINDOW, 102);
    assert_eq!(page_shown(&world.alice, WINDOW), Some(2));
    world.press(WINDOW, 150);
    assert_eq!(page_shown(&world.alice, WINDOW), Some(2));
    world.press(WINDOW, 100);
    assert_eq!(page_shown(&world.alice, WINDOW), Some(0));
}

#[test]
fn a_button_that_means_nothing_changes_nothing() {
    let mut world = world();
    world.open_lectern();
    world.press(WINDOW, 2);
    let before = data_events(&world.alice, WINDOW);
    for button in [0, 4, 50, 99, -1] {
        world.press(WINDOW, button);
    }
    world.press(WINDOW + 5, 2);
    assert_eq!(data_events(&world.alice, WINDOW), before);
}

#[test]
fn a_page_turned_by_one_reader_reaches_the_other_one() {
    let mut world = world();
    world.open_lectern();
    world.open_lectern_as_bob();
    world.press(WINDOW, 2);

    assert_eq!(page_shown(&world.alice, WINDOW), Some(1));
    assert_eq!(page_shown(&world.bob, WINDOW), Some(1));
}

fn lectern_powered_updates(player: &Player) -> Vec<bool> {
    player
        .log
        .iter()
        .filter_map(|event| match event {
            ClientEvent::BlockUpdate { pos, state } if *pos == LECTERN => {
                Some(state.property("powered") == Some("true"))
            }
            _ => None,
        })
        .collect()
}

#[test]
fn turning_a_page_gives_the_lectern_a_two_tick_redstone_pulse() {
    let mut world = world();
    world.open_lectern();
    world.press(WINDOW, 2);
    world.press(WINDOW, 2);
    let updates = lectern_powered_updates(&world.alice);
    assert!(updates.contains(&true));
    assert_eq!(updates.last(), Some(&false));
    let before = updates.len();
    world.press(WINDOW, 150);
    world.press(WINDOW, 150);
    assert_eq!(lectern_powered_updates(&world.alice).len(), before);
}

#[test]
fn the_page_stays_when_the_lectern_is_closed_and_opened_again() {
    let mut world = world();
    world.open_lectern();
    world.press(WINDOW, 2);
    world.close(WINDOW);
    world.open_lectern();

    assert_eq!(data_events(&world.alice, 2), vec![(0, 1)]);
}

#[test]
fn taking_the_book_gives_it_back_and_empties_the_lectern() {
    let mut world = world();
    world.open_lectern();
    world.press(WINDOW, 3);

    assert!(closed(&world.alice, WINDOW));
    let given = world.alice.log.iter().any(|event| {
        matches!(
            event,
            ClientEvent::InventorySlot { stack: Some(stack), .. }
                if stack.item.0 == item("minecraft:written_book")
        )
    });
    assert!(given);
    assert!(on_the_ground(&world, "minecraft:written_book").is_empty());
    let state = last_update(&world.alice, LECTERN).expect("lectern update");
    assert_eq!(state.property("has_book"), Some("false"));
    world.open_lectern();
    assert_eq!(opened(&world.alice), Some((WINDOW, 17)));
}

#[test]
fn taking_the_book_closes_the_window_of_the_other_reader() {
    let mut world = world();
    world.open_lectern();
    world.open_lectern_as_bob();
    world.press(WINDOW, 3);

    assert!(closed(&world.bob, WINDOW));
}

#[test]
fn an_adventurer_cannot_take_the_book() {
    let mut world = world();
    world.console("gamemode adventure Alice");
    world.open_lectern();
    world.press(WINDOW, 3);

    assert!(!closed(&world.alice, WINDOW));
    let state = last_update(&world.alice, LECTERN);
    assert!(state.is_none_or(|state| state.property("has_book") == Some("true")));
}

#[test]
fn clicks_on_the_book_slot_do_not_take_the_book() {
    let mut world = world();
    world.open_lectern();
    world.click(WINDOW, 0, 0, ClickInput::Pickup);
    world.click(WINDOW, 0, 0, ClickInput::QuickMove);
    assert!(!closed(&world.alice, WINDOW));
    world.close(WINDOW);

    let book = item("minecraft:written_book");
    let handed_out = world.alice.log.iter().any(|event| match event {
        ClientEvent::CursorItem(Some(stack))
        | ClientEvent::InventorySlot {
            stack: Some(stack), ..
        }
        | ClientEvent::PlayerInventorySlot {
            stack: Some(stack), ..
        } => stack.item.0 == book,
        _ => false,
    });
    assert!(!handed_out);
    let state = last_update(&world.alice, LECTERN);
    assert!(state.is_none_or(|state| state.property("has_book") == Some("true")));
    world.open_lectern();
    assert_eq!(content(&world.alice, 2), Some(vec![Some((book, 1))]));
}

#[test]
fn walking_away_closes_the_lectern() {
    let mut world = world();
    world.open_lectern();
    world.console("tp Alice 40 64 40");

    assert!(closed(&world.alice, WINDOW));
}

fn place_lectern(world: &mut World) {
    world.place_storage("lectern");
}

#[test]
fn a_lectern_without_a_book_does_not_open() {
    let mut world = world();
    place_lectern(&mut world);
    world.open_chest();

    assert_eq!(opened(&world.alice), None);
}

#[test]
fn a_book_in_hand_is_put_on_an_empty_lectern() {
    for book in ["writable_book", "written_book"] {
        let mut world = world();
        place_lectern(&mut world);
        world.give(book);
        world.select(0);
        let before = world.alice.log.len();
        world.use_on(CHEST, Direction::Up);

        let state = last_update(&world.alice, CHEST).expect("lectern update");
        assert_eq!(state.property("has_book"), Some("true"), "{book}");
        assert_eq!(state.property("powered"), Some("false"), "{book}");
        assert_eq!(
            sounds_heard(&world.bob, "minecraft:item.book.put"),
            1,
            "{book}"
        );
        assert_eq!(opened(&world.alice), None, "{book}");
        let consumed = world.alice.log[before..].iter().any(|event| {
            matches!(
                event,
                ClientEvent::InventorySlot {
                    slot: 36,
                    stack: None,
                    ..
                }
            )
        });
        assert!(consumed, "{book}");

        world.open_chest();
        assert_eq!(opened(&world.alice), Some((WINDOW, 17)), "{book}");
        let slots = content(&world.alice, WINDOW).expect("content");
        assert_eq!(
            slots,
            vec![Some((item(&format!("minecraft:{book}")), 1))],
            "{book}"
        );
    }
}

#[test]
fn something_that_is_not_a_book_is_not_put_on_the_lectern() {
    let mut world = world();
    place_lectern(&mut world);
    world.give("stick");
    world.select(0);
    world.use_on(CHEST, Direction::Up);

    let state = last_update(&world.alice, CHEST).expect("lectern update");
    assert_eq!(state.property("has_book"), Some("false"));
    assert_eq!(sounds_heard(&world.bob, "minecraft:item.book.put"), 0);
}

#[test]
fn a_placed_lectern_has_its_block_entity_for_a_late_joiner() {
    let mut world = world();
    place_lectern(&mut world);
    let mut carol = join(&mut world.server, "Carol");
    run_ticks(&mut world.server, &mut [&mut carol], 30);

    assert!(carries_a_block_entity(&carol, CHEST, "minecraft:lectern"));
}

#[test]
fn breaking_a_lectern_pops_its_book() {
    let mut world = world();
    world.break_at(LECTERN);

    assert_eq!(on_the_ground(&world, "minecraft:written_book"), vec![1]);
    assert!(closed(&world.alice, WINDOW) || opened(&world.alice).is_none());
}

#[test]
fn breaking_an_empty_lectern_pops_nothing() {
    let mut world = world();
    place_lectern(&mut world);
    world.break_at(CHEST);

    assert!(on_the_ground(&world, "minecraft:written_book").is_empty());
    assert!(on_the_ground(&world, "minecraft:writable_book").is_empty());
}

fn effect_id(name: &str) -> i32 {
    i32::from(
        rspigot_domain::effect::EffectId::from_name(name)
            .expect(name)
            .0,
    )
}

fn last_data(player: &Player, window: i32, id: i16) -> Option<i16> {
    data_events(player, window)
        .into_iter()
        .rev()
        .find(|(at, _)| *at == id)
        .map(|(_, value)| value)
}

impl World {
    fn set_beacon(&mut self, primary: Option<&str>, secondary: Option<&str>) {
        self.server.handle(Inbound::Input {
            player: self.alice.id,
            input: PlayerInput::SetBeacon {
                primary: primary.map(effect_id),
                secondary: secondary.map(effect_id),
            },
        });
        self.settle();
    }

    fn pay_beacon(&mut self, beacon: BlockPos) {
        self.give("iron_ingot");
        self.open_at(beacon);
        self.click(WINDOW, 28, 0, ClickInput::Pickup);
        self.click(WINDOW, 0, 0, ClickInput::Pickup);
    }
}

#[test]
fn a_beacon_opens_with_a_payment_slot_its_levels_and_no_power_chosen() {
    let mut world = world();
    world.open_at(FULL_BEACON);

    assert_eq!(opened(&world.alice), Some((WINDOW, 9)));
    assert!(titled(&world.alice, "container.beacon"));
    assert_eq!(content(&world.alice, WINDOW).expect("content").len(), 37);
    assert_eq!(
        data_events(&world.alice, WINDOW),
        vec![(0, 4), (1, 0), (2, 0)]
    );
}

#[test]
fn the_levels_follow_the_pyramid_under_the_beacon() {
    let mut world = world();
    world.open_at(SMALL_BEACON);

    assert_eq!(last_data(&world.alice, WINDOW, 0), Some(1));
}

#[test]
fn the_payment_slot_takes_one_payment_item_at_a_time() {
    let mut world = world();
    world.console("give Alice minecraft:iron_ingot 5");
    world.give("stick");
    world.open_at(FULL_BEACON);
    world.click(WINDOW, 28, 0, ClickInput::Pickup);
    world.click(WINDOW, 0, 0, ClickInput::Pickup);

    assert_eq!(
        slot_update(&world.alice, WINDOW, 0),
        Some(Some((item("minecraft:iron_ingot"), 1)))
    );
    world.click(WINDOW, 0, 0, ClickInput::Pickup);
    world.click(WINDOW, 29, 0, ClickInput::Pickup);
    world.click(WINDOW, 0, 0, ClickInput::Pickup);
    assert_ne!(
        slot_update(&world.alice, WINDOW, 0),
        Some(Some((item("minecraft:stick"), 1)))
    );
}

#[test]
fn choosing_the_powers_uses_up_the_payment_and_keeps_the_choice() {
    let mut world = world();
    world.pay_beacon(FULL_BEACON);
    world.set_beacon(Some("minecraft:speed"), Some("minecraft:regeneration"));

    assert_eq!(
        last_data(&world.alice, WINDOW, 1),
        Some(effect_id("minecraft:speed") as i16 + 1)
    );
    assert_eq!(
        last_data(&world.alice, WINDOW, 2),
        Some(effect_id("minecraft:regeneration") as i16 + 1)
    );
    assert_eq!(slot_update(&world.alice, WINDOW, 0), Some(None));
    world.close(WINDOW);
    world.open_at(FULL_BEACON);
    assert_eq!(
        data_events(&world.alice, 2),
        vec![
            (0, 4),
            (1, effect_id("minecraft:speed") as i16 + 1),
            (2, effect_id("minecraft:regeneration") as i16 + 1),
        ]
    );
    assert!(on_the_ground(&world, "minecraft:iron_ingot").is_empty());
}

#[test]
fn the_secondary_power_may_repeat_the_primary_one() {
    let mut world = world();
    world.pay_beacon(FULL_BEACON);
    world.set_beacon(Some("minecraft:haste"), Some("minecraft:haste"));

    assert_eq!(
        last_data(&world.alice, WINDOW, 2),
        Some(effect_id("minecraft:haste") as i16 + 1)
    );
}

#[test]
fn a_choice_the_pyramid_is_too_small_for_is_refused_and_keeps_the_payment() {
    let refused: [(Option<&str>, Option<&str>); 4] = [
        (Some("minecraft:strength"), None),
        (Some("minecraft:resistance"), None),
        (Some("minecraft:speed"), Some("minecraft:speed")),
        (Some("minecraft:speed"), Some("minecraft:regeneration")),
    ];
    for (primary, secondary) in refused {
        let mut world = world();
        world.pay_beacon(SMALL_BEACON);
        world.set_beacon(primary, secondary);

        assert_eq!(
            last_data(&world.alice, WINDOW, 1),
            Some(0),
            "{primary:?} {secondary:?}"
        );
        assert_eq!(
            slot_update(&world.alice, WINDOW, 0),
            Some(Some((item("minecraft:iron_ingot"), 1))),
            "{primary:?} {secondary:?}"
        );
    }
}

#[test]
fn a_small_pyramid_accepts_a_first_tier_power() {
    let mut world = world();
    world.pay_beacon(SMALL_BEACON);
    world.set_beacon(Some("minecraft:haste"), None);

    assert_eq!(
        last_data(&world.alice, WINDOW, 1),
        Some(effect_id("minecraft:haste") as i16 + 1)
    );
    assert_eq!(last_data(&world.alice, WINDOW, 2), Some(0));
}

#[test]
fn the_beacon_refuses_choices_that_break_its_rules() {
    let refused: [(Option<&str>, Option<&str>); 4] = [
        (Some("minecraft:regeneration"), None),
        (Some("minecraft:speed"), Some("minecraft:haste")),
        (Some("minecraft:poison"), None),
        (Some("minecraft:speed"), Some("minecraft:poison")),
    ];
    for (primary, secondary) in refused {
        let mut world = world();
        world.pay_beacon(FULL_BEACON);
        world.set_beacon(primary, secondary);

        assert_eq!(
            last_data(&world.alice, WINDOW, 1),
            Some(0),
            "{primary:?} {secondary:?}"
        );
        assert_eq!(
            last_data(&world.alice, WINDOW, 2),
            Some(0),
            "{primary:?} {secondary:?}"
        );
        assert!(slot_update(&world.alice, WINDOW, 0).is_some_and(|slot| slot.is_some()));
    }
}

#[test]
fn an_effect_id_that_does_not_exist_is_refused() {
    for (primary, secondary) in [(Some(9999), None), (Some(-1), None), (None, Some(9999))] {
        let mut world = world();
        world.pay_beacon(FULL_BEACON);
        world.server.handle(Inbound::Input {
            player: world.alice.id,
            input: PlayerInput::SetBeacon { primary, secondary },
        });
        world.settle();

        assert_eq!(
            last_data(&world.alice, WINDOW, 1),
            Some(0),
            "{primary:?} {secondary:?}"
        );
        assert!(slot_update(&world.alice, WINDOW, 0).is_some_and(|slot| slot.is_some()));
    }
}

#[test]
fn nothing_is_chosen_without_a_payment() {
    let mut world = world();
    world.open_at(FULL_BEACON);
    world.set_beacon(Some("minecraft:speed"), None);

    assert_eq!(last_data(&world.alice, WINDOW, 1), Some(0));
}

#[test]
fn a_beacon_choice_without_an_open_beacon_does_nothing() {
    let mut world = world();
    world.set_beacon(Some("minecraft:speed"), None);
    world.place_chest();
    world.open_chest();
    world.set_beacon(Some("minecraft:speed"), None);

    let sent_data = world
        .alice
        .log
        .iter()
        .any(|event| matches!(event, ClientEvent::ContainerData { .. }));
    assert!(!sent_data);
}

#[test]
fn closing_the_beacon_throws_the_payment_on_the_ground() {
    let mut world = world();
    world.pay_beacon(FULL_BEACON);
    let before = world.alice.log.len();
    world.close(WINDOW);

    assert_eq!(on_the_ground(&world, "minecraft:iron_ingot"), vec![1]);
    let ingot = item("minecraft:iron_ingot");
    let given_back = world.alice.log[before..].iter().any(|event| {
        matches!(
            event,
            ClientEvent::InventorySlot { stack: Some(stack), .. }
            | ClientEvent::PlayerInventorySlot { stack: Some(stack), .. }
                if stack.item.0 == ingot
        )
    });
    assert!(!given_back);
}

#[test]
fn another_reader_sees_the_powers_that_were_chosen() {
    let mut world = world();
    world.open_at(FULL_BEACON);
    world.server.handle(Inbound::Input {
        player: world.bob.id,
        input: PlayerInput::UseItemOn {
            hand: Hand::Main,
            pos: FULL_BEACON,
            face: Direction::Up,
            cursor: Vec3::new(0.5, 1.0, 0.5),
            inside_block: false,
            sequence: 9,
        },
    });
    world.settle();
    world.give("iron_ingot");
    world.click(WINDOW, 28, 0, ClickInput::Pickup);
    world.click(WINDOW, 0, 0, ClickInput::Pickup);
    world.set_beacon(Some("minecraft:speed"), None);

    assert_eq!(
        last_data(&world.bob, WINDOW, 1),
        Some(effect_id("minecraft:speed") as i16 + 1)
    );
}

#[test]
fn the_levels_are_checked_again_every_eighty_ticks_while_the_window_is_open() {
    let mut world = world();
    world.open_at(FULL_BEACON);
    assert_eq!(last_data(&world.alice, WINDOW, 0), Some(4));
    world.console("tp Alice -4 64 6");
    world.break_at(BlockPos::new(-4, 63, 4));
    run_ticks(
        &mut world.server,
        &mut [&mut world.alice, &mut world.bob],
        85,
    );

    assert_eq!(last_data(&world.alice, WINDOW, 0), Some(0));
}

#[test]
fn a_placed_beacon_has_its_block_entity_for_a_late_joiner() {
    let mut world = world();
    world.place_storage("beacon");
    let mut carol = join(&mut world.server, "Carol");
    run_ticks(&mut world.server, &mut [&mut carol], 30);

    assert!(carries_a_block_entity(&carol, CHEST, "minecraft:beacon"));
}

const FURNACE_HOTBAR_0: i16 = 30;

impl World {
    fn wait(&mut self, ticks: usize) {
        self.server.deliver();
        run_ticks(
            &mut self.server,
            &mut [&mut self.alice, &mut self.bob],
            ticks,
        );
    }

    fn open_and_fill_furnace(&mut self, block: &str, input: &str, fuel: &str) {
        self.place_storage(block);
        self.give(input);
        self.give(fuel);
        self.open_chest();
        self.click(WINDOW, FURNACE_HOTBAR_0, 0, ClickInput::Pickup);
        self.click(WINDOW, 0, 0, ClickInput::Pickup);
        self.click(WINDOW, FURNACE_HOTBAR_0 + 1, 0, ClickInput::Pickup);
        self.click(WINDOW, 1, 0, ClickInput::Pickup);
    }

    fn furnace_is_lit(&self) -> bool {
        last_update(&self.alice, CHEST).is_some_and(|state| state.property("lit") == Some("true"))
    }
}

fn output_of_furnace(world: &World) -> Option<Option<(u16, u8)>> {
    slot_update(&world.alice, WINDOW, 2)
}

#[test]
fn a_furnace_cooks_an_item_with_its_fuel_and_lights_up() {
    let mut world = world();
    world.open_and_fill_furnace("furnace", "potato", "coal");
    assert_eq!(output_of_furnace(&world), None);

    world.wait(190);
    assert_eq!(output_of_furnace(&world), None);
    assert!(world.furnace_is_lit());

    world.wait(20);
    assert_eq!(
        output_of_furnace(&world),
        Some(Some((item("minecraft:baked_potato"), 1)))
    );
    assert_eq!(slot_update(&world.alice, WINDOW, 0), Some(None));
    assert!(world.furnace_is_lit());
}

#[test]
fn a_blast_furnace_cooks_in_half_the_time() {
    let mut world = world();
    world.open_and_fill_furnace("blast_furnace", "raw_iron", "coal");
    world.wait(90);
    assert_eq!(output_of_furnace(&world), None);
    world.wait(20);
    assert_eq!(
        output_of_furnace(&world),
        Some(Some((item("minecraft:iron_ingot"), 1)))
    );
}

#[test]
fn a_smoker_refuses_what_is_not_food_and_a_furnace_without_fuel_stays_cold() {
    let mut world = world();
    world.open_and_fill_furnace("smoker", "raw_iron", "coal");
    world.wait(220);
    assert_eq!(output_of_furnace(&world), None);
    assert!(!world.furnace_is_lit());

    let mut cold = self::world();
    cold.place_storage("furnace");
    cold.give("potato");
    cold.open_chest();
    cold.click(WINDOW, FURNACE_HOTBAR_0, 0, ClickInput::Pickup);
    cold.click(WINDOW, 0, 0, ClickInput::Pickup);
    cold.wait(60);
    assert!(!cold.furnace_is_lit());
    assert_eq!(output_of_furnace(&cold), None);
}

#[test]
fn the_menu_counts_down_the_fire_and_the_cooking_progress() {
    let mut world = world();
    world.open_and_fill_furnace("furnace", "potato", "coal");
    world.wait(20);
    let last = |id: i16| {
        data_events(&world.alice, WINDOW)
            .into_iter()
            .rev()
            .find_map(|(index, value)| (index == id).then_some(value))
    };
    let lit = last(0).expect("lit time");
    assert!((1570..1600).contains(&lit), "{lit}");
    assert_eq!(last(1), Some(1600));
    let progress = last(2).expect("progress");
    assert!((10..30).contains(&progress), "{progress}");
    assert_eq!(last(3), Some(200));
}

fn experience_of_everyone(world: &World) -> i32 {
    [world.alice.id, world.bob.id]
        .into_iter()
        .map(|id| world.server.player_experience(id).unwrap().2)
        .sum()
}

#[test]
fn taking_the_output_pays_the_experience_it_earned() {
    let mut world = world();
    world.open_and_fill_furnace("blast_furnace", "raw_gold", "coal");
    world.wait(110);
    assert_eq!(
        output_of_furnace(&world),
        Some(Some((item("minecraft:gold_ingot"), 1)))
    );
    let before = experience_of_everyone(&world);
    world.click(WINDOW, 2, 0, ClickInput::Pickup);
    world.wait(40);
    let after = experience_of_everyone(&world);
    assert_eq!(after - before, 1);
}

#[test]
fn the_experience_is_paid_once_per_item_cooked() {
    let mut world = world();
    world.place_storage("blast_furnace");
    world.console("give Alice minecraft:raw_gold 2");
    world.give("coal");
    world.open_chest();
    world.click(WINDOW, FURNACE_HOTBAR_0, 0, ClickInput::Pickup);
    world.click(WINDOW, 0, 0, ClickInput::Pickup);
    world.click(WINDOW, FURNACE_HOTBAR_0 + 1, 0, ClickInput::Pickup);
    world.click(WINDOW, 1, 0, ClickInput::Pickup);
    world.wait(105);
    let experience = experience_of_everyone;
    let start = experience(&world);
    world.click(WINDOW, 2, 0, ClickInput::Pickup);
    world.wait(40);
    let first = experience(&world);
    assert_eq!(first - start, 1);
    world.click(WINDOW, FURNACE_HOTBAR_0, 0, ClickInput::Pickup);
    world.wait(100);
    world.click(WINDOW, 2, 0, ClickInput::Pickup);
    world.wait(40);
    assert_eq!(experience(&world) - first, 1);
}

#[test]
fn breaking_a_furnace_drops_its_items_and_pops_the_experience_it_kept() {
    let mut world = world();
    world.open_and_fill_furnace("blast_furnace", "raw_gold", "coal");
    world.wait(110);
    world.close(WINDOW);
    let before = experience_of_everyone(&world);
    world.break_at(CHEST);
    world.wait(40);
    let after = experience_of_everyone(&world);
    assert_eq!(after - before, 1);
    let dropped = world.alice.log.iter().any(|event| {
        matches!(
            event,
            ClientEvent::SpawnEntity {
                kind: rspigot_core::events::EntityKind::Item,
                ..
            }
        )
    });
    assert!(dropped);
}

#[test]
fn replacing_the_input_restarts_the_cooking() {
    let mut world = world();
    world.open_and_fill_furnace("furnace", "potato", "coal");
    world.wait(100);
    world.click(WINDOW, 0, 0, ClickInput::Pickup);
    world.wait(2);
    let reset = data_events(&world.alice, WINDOW)
        .into_iter()
        .rev()
        .find_map(|(index, value)| (index == 2).then_some(value));
    assert_eq!(reset, Some(0));
}

#[test]
fn adding_more_of_the_same_input_keeps_the_progress() {
    let mut world = world();
    world.place_storage("furnace");
    world.console("give Alice minecraft:potato 2");
    world.give("coal");
    world.open_chest();
    world.click(WINDOW, FURNACE_HOTBAR_0, 0, ClickInput::Pickup);
    world.click(WINDOW, 0, 1, ClickInput::Pickup);
    world.click(WINDOW, FURNACE_HOTBAR_0, 0, ClickInput::Pickup);
    world.click(WINDOW, FURNACE_HOTBAR_0 + 1, 0, ClickInput::Pickup);
    world.click(WINDOW, 1, 0, ClickInput::Pickup);
    world.wait(100);
    world.click(WINDOW, FURNACE_HOTBAR_0, 0, ClickInput::Pickup);
    world.click(WINDOW, 0, 0, ClickInput::Pickup);
    world.wait(2);
    let progress = data_events(&world.alice, WINDOW)
        .into_iter()
        .rev()
        .find_map(|(index, value)| (index == 2).then_some(value))
        .unwrap();
    assert!(progress >= 95, "{progress}");
}

fn campfire_items(player: &Player) -> Option<Vec<(u16, u8)>> {
    player.log.iter().rev().find_map(|event| match event {
        ClientEvent::BlockEntityUpdate { pos, kind, data }
            if *pos == CHEST && *kind == "campfire" =>
        {
            Some(
                data.get_list("Items")
                    .unwrap_or_default()
                    .iter()
                    .filter_map(|entry| {
                        let entry = entry.as_compound()?;
                        let name = entry.get_str("id")?;
                        Some((item(name), u8::try_from(entry.get_i32("count")?).ok()?))
                    })
                    .collect(),
            )
        }
        _ => None,
    })
}

fn items_spawned(player: &Player) -> usize {
    player
        .log
        .iter()
        .filter(|event| {
            matches!(
                event,
                ClientEvent::SpawnEntity {
                    kind: rspigot_core::events::EntityKind::Item,
                    ..
                }
            )
        })
        .count()
}

impl World {
    fn campfire_with(&mut self, food: &str, count: u8) {
        self.place_storage("campfire");
        self.console(&format!("give Alice minecraft:{food} {count}"));
        self.select(0);
        self.use_on(CHEST, Direction::Up);
    }

    fn held_count(&self) -> Option<u8> {
        inventory_slot(&self.alice, 36).map(|(_, count)| count)
    }
}

#[test]
fn food_placed_on_a_campfire_is_shown_and_taken_one_at_a_time() {
    let mut world = world();
    world.campfire_with("beef", 3);
    assert_eq!(
        campfire_items(&world.alice),
        Some(vec![(item("minecraft:beef"), 1)])
    );
    assert_eq!(world.held_count(), Some(2));
    world.use_on(CHEST, Direction::Up);
    assert_eq!(
        campfire_items(&world.alice).map(|items| items.len()),
        Some(2)
    );
    assert_eq!(world.held_count(), Some(1));
}

#[test]
fn a_full_campfire_keeps_the_food_in_the_hand() {
    let mut world = world();
    world.campfire_with("beef", 6);
    for _ in 0..3 {
        world.use_on(CHEST, Direction::Up);
    }
    assert_eq!(
        campfire_items(&world.alice).map(|items| items.len()),
        Some(4)
    );
    assert_eq!(world.held_count(), Some(2));
    world.use_on(CHEST, Direction::Up);
    assert_eq!(world.held_count(), Some(2));
}

#[test]
fn a_creative_player_keeps_the_food_he_places() {
    let mut world = world();
    world.console("gamemode creative Alice");
    world.place_storage("campfire");
    world.console("give Alice minecraft:beef 3");
    world.select(1);
    world.use_on(CHEST, Direction::Up);
    assert_eq!(
        campfire_items(&world.alice).map(|items| items.len()),
        Some(1)
    );
    assert_eq!(
        inventory_slot(&world.alice, 37),
        Some((item("minecraft:beef"), 3))
    );
}

#[test]
fn food_cooks_for_600_ticks_then_drops_as_the_cooked_item() {
    let mut world = world();
    world.campfire_with("beef", 1);
    let before = items_spawned(&world.alice);
    world.wait(590);
    assert_eq!(items_spawned(&world.alice), before);
    assert_eq!(
        campfire_items(&world.alice).map(|items| items.len()),
        Some(1)
    );
    world.wait(20);
    assert_eq!(items_spawned(&world.alice), before + 1);
    assert_eq!(campfire_items(&world.alice), Some(Vec::new()));
}

#[test]
fn breaking_a_campfire_drops_the_food_on_it() {
    let mut world = world();
    world.campfire_with("beef", 2);
    world.use_on(CHEST, Direction::Up);
    let before = items_spawned(&world.alice);
    world.break_at(CHEST);
    assert!(items_spawned(&world.alice) >= before + 2);
}

const CUTTER_HOTBAR_0: i16 = 29;

impl World {
    fn open_stonecutter_with(&mut self, count: u8) {
        self.place_storage("stonecutter");
        self.console(&format!("give Alice minecraft:stone {count}"));
        self.open_chest();
        self.click(WINDOW, CUTTER_HOTBAR_0, 0, ClickInput::Pickup);
        self.click(WINDOW, 0, 0, ClickInput::Pickup);
    }
}

#[test]
fn the_client_is_told_every_recipe_it_needs_when_it_joins() {
    let world = world();
    assert!(
        world
            .alice
            .log
            .iter()
            .any(|event| matches!(event, ClientEvent::RecipeSync))
    );
}

#[test]
fn choosing_a_cut_shows_its_result_and_taking_it_uses_one_stone() {
    let mut world = world();
    world.open_stonecutter_with(3);
    assert_eq!(last_data(&world.alice, WINDOW, 0), Some(-1));

    world.press(WINDOW, 9);
    assert_eq!(
        slot_update(&world.alice, WINDOW, 1),
        Some(Some((item("minecraft:stone_slab"), 2)))
    );
    assert_eq!(last_data(&world.alice, WINDOW, 0), Some(9));

    world.click(WINDOW, 1, 0, ClickInput::Pickup);
    assert_eq!(
        slot_update(&world.alice, WINDOW, 0),
        Some(Some((item("minecraft:stone"), 2)))
    );
    assert_eq!(
        slot_update(&world.alice, WINDOW, 1),
        Some(Some((item("minecraft:stone_slab"), 2)))
    );
    assert_eq!(
        sounds_heard(&world.alice, "minecraft:ui.stonecutter.take_result"),
        1
    );
}

#[test]
fn a_button_for_another_window_or_a_missing_recipe_does_nothing() {
    let mut world = world();
    world.open_stonecutter_with(3);
    world.press(WINDOW + 1, 9);
    world.press(WINDOW, 99);
    assert_eq!(slot_update(&world.alice, WINDOW, 1), None);
    assert_eq!(last_data(&world.alice, WINDOW, 0), Some(-1));
}

#[test]
fn shift_click_on_the_result_cuts_the_whole_stack() {
    let mut world = world();
    world.open_stonecutter_with(3);
    world.press(WINDOW, 9);
    world.click(WINDOW, 1, 0, ClickInput::QuickMove);
    assert_eq!(slot_update(&world.alice, WINDOW, 0), Some(None));
    assert_eq!(slot_update(&world.alice, WINDOW, 1), Some(None));
    assert_eq!(last_data(&world.alice, WINDOW, 0), Some(-1));
    assert_eq!(
        slot_update(&world.alice, WINDOW, 37),
        Some(Some((item("minecraft:stone_slab"), 6)))
    );
}

#[test]
fn a_spectator_cannot_choose_a_cut() {
    let mut world = world();
    world.open_stonecutter_with(3);
    world.console("gamemode spectator Alice");
    world.press(WINDOW, 9);
    assert_eq!(slot_update(&world.alice, WINDOW, 1), None);
}

const SMITHING_HOTBAR_0: i16 = 31;

impl World {
    fn open_smithing_with(&mut self, items: &[(&str, u8)]) {
        self.place_storage("smithing_table");
        for (name, count) in items {
            self.console(&format!("give Alice minecraft:{name} {count}"));
        }
        self.open_chest();
        for (index, _) in items.iter().enumerate() {
            let hotbar = SMITHING_HOTBAR_0 + i16::try_from(index).unwrap();
            self.click(WINDOW, hotbar, 0, ClickInput::Pickup);
            self.click(WINDOW, i16::try_from(index).unwrap(), 0, ClickInput::Pickup);
        }
    }
}

#[test]
fn a_smithing_table_upgrades_diamond_gear_to_netherite() {
    let mut world = world();
    world.open_smithing_with(&[
        ("netherite_upgrade_smithing_template", 1),
        ("diamond_sword", 1),
        ("netherite_ingot", 2),
    ]);
    assert_eq!(
        slot_update(&world.alice, WINDOW, 3),
        Some(Some((item("minecraft:netherite_sword"), 1)))
    );
    assert_eq!(last_data(&world.alice, WINDOW, 0), Some(0));

    world.click(WINDOW, 3, 0, ClickInput::Pickup);
    assert_eq!(slot_update(&world.alice, WINDOW, 0), Some(None));
    assert_eq!(slot_update(&world.alice, WINDOW, 1), Some(None));
    assert_eq!(
        slot_update(&world.alice, WINDOW, 2),
        Some(Some((item("minecraft:netherite_ingot"), 1)))
    );
    assert_eq!(slot_update(&world.alice, WINDOW, 3), Some(None));
    assert_eq!(
        sounds_heard(&world.alice, "minecraft:block.smithing_table.use"),
        1
    );
}

#[test]
fn a_trim_is_applied_with_its_pattern_and_material() {
    let mut world = world();
    world.open_smithing_with(&[
        ("bolt_armor_trim_smithing_template", 1),
        ("iron_chestplate", 1),
        ("copper_ingot", 1),
    ]);
    let result = world
        .alice
        .log
        .iter()
        .rev()
        .find_map(|event| match event {
            ClientEvent::ContainerSlot {
                window: WINDOW,
                slot: 3,
                stack: Some(stack),
                ..
            } => Some(stack.clone()),
            _ => None,
        })
        .expect("a trimmed chestplate");
    assert_eq!(result.item.0, item("minecraft:iron_chestplate"));
    let trim = result.components.get_compound("minecraft:trim").unwrap();
    assert_eq!(trim.get_str("material"), Some("minecraft:copper"));
    assert_eq!(trim.get_str("pattern"), Some("minecraft:bolt"));
}

#[test]
fn inputs_that_do_not_fit_together_raise_the_error_flag() {
    let mut world = world();
    world.open_smithing_with(&[
        ("netherite_upgrade_smithing_template", 1),
        ("diamond_sword", 1),
        ("copper_ingot", 1),
    ]);
    assert_eq!(slot_update(&world.alice, WINDOW, 3), None);
    assert_eq!(last_data(&world.alice, WINDOW, 0), Some(1));
}

#[test]
fn the_smithing_slots_refuse_what_no_recipe_uses() {
    let mut world = world();
    world.place_storage("smithing_table");
    world.give("stick");
    world.open_chest();
    for slot in 0..3 {
        world.click(WINDOW, SMITHING_HOTBAR_0, 0, ClickInput::Pickup);
        world.click(WINDOW, slot, 0, ClickInput::Pickup);
        world.click(WINDOW, SMITHING_HOTBAR_0, 0, ClickInput::Pickup);
    }
    for slot in 0..3 {
        assert_eq!(slot_update(&world.alice, WINDOW, slot), None, "{slot}");
    }
}

#[test]
fn breaking_a_furnace_asks_block_exp_after_the_break_and_pays_the_settled_amount() {
    let mut world = world();
    let mailbox = common::Mailbox::default();
    world.server.handle(Inbound::PluginConnected {
        connection: 1,
        intro: PluginIntro {
            commands: Vec::new(),
            name: "tester".to_owned(),
            version: "1.0.0".to_owned(),
            handlers: vec![common::deciding(1, EventKind::BlockExp)],
        },
        outbox: Box::new(mailbox.clone()),
    });
    world.open_and_fill_furnace("blast_furnace", "raw_gold", "coal");
    world.wait(110);
    world.close(WINDOW);
    let before = experience_of_everyone(&world);
    for action in [DigAction::Start, DigAction::Finish] {
        if action == DigAction::Finish {
            world.wait_to_finish();
        }
        world.server.handle(Inbound::Input {
            player: world.alice.id,
            input: PlayerInput::Dig {
                action,
                pos: CHEST,
                face: Direction::Up,
                sequence: 4,
            },
        });
    }
    let broken = common::last_asked(&mailbox, EventKind::BlockBreak);
    world.server.handle(Inbound::PluginMessage {
        connection: 1,
        message: FromPlugin::Decision {
            event: broken.id,
            verdicts: Vec::new(),
        },
    });
    let asked = common::last_asked(&mailbox, EventKind::BlockExp);
    assert_eq!(
        asked.payload,
        EventPayload::BlockExp(BlockExp {
            block: CHEST,
            exp_to_drop: 1,
        })
    );
    world.server.handle(Inbound::PluginMessage {
        connection: 1,
        message: FromPlugin::Decision {
            event: asked.id,
            verdicts: vec![Verdict {
                handler: 1,
                cancel: false,
                patch: Some(EventPatch::BlockExp(BlockExpPatch {
                    exp_to_drop: Some(5),
                })),
            }],
        },
    });
    world.wait(40);
    assert_eq!(experience_of_everyone(&world) - before, 5);
}

const CRAFTER_HOTBAR_0: i16 = 36;
const CRAFTER_RESULT: i16 = 45;

impl World {
    fn set_crafter_slot(&mut self, window: i32, slot: i32, enabled: bool) {
        self.server.handle(Inbound::Input {
            player: self.alice.id,
            input: PlayerInput::ContainerSlotState {
                window,
                slot,
                enabled,
            },
        });
        self.settle();
    }
}

#[test]
fn the_crafter_opens_a_nine_slot_menu_with_ten_data_slots() {
    let mut world = world();
    world.place_storage("crafter");
    world.open_chest();

    assert_eq!(opened(&world.alice), Some((WINDOW, 7)));
    let slots = content(&world.alice, WINDOW).expect("content");
    assert_eq!(slots.len(), 46);
    assert_eq!(
        data_events(&world.alice, WINDOW),
        (0..10).map(|id| (id, 0)).collect::<Vec<_>>()
    );
}

#[test]
fn a_crafter_grid_shows_the_result_of_its_recipe_in_the_last_slot() {
    let mut world = world();
    world.place_storage("crafter");
    world.give("oak_log");
    world.open_chest();
    world.click(WINDOW, CRAFTER_HOTBAR_0, 0, ClickInput::Pickup);
    world.click(WINDOW, 4, 0, ClickInput::Pickup);

    assert_eq!(
        slot_update(&world.alice, WINDOW, CRAFTER_RESULT),
        Some(Some((item("minecraft:oak_planks"), 4)))
    );
    world.click(WINDOW, CRAFTER_RESULT, 0, ClickInput::Pickup);
    assert_eq!(
        slot_update(&world.alice, WINDOW, CRAFTER_RESULT),
        Some(Some((item("minecraft:oak_planks"), 4)))
    );
    assert!(world.alice.log.iter().all(|event| !matches!(
        event,
        ClientEvent::CursorItem(Some(stack)) if stack.item.0 == item("minecraft:oak_planks")
    )));
}

#[test]
fn the_crafter_keeps_its_items_when_the_window_closes() {
    let mut world = world();
    world.place_storage("crafter");
    world.give("oak_log");
    world.open_chest();
    world.click(WINDOW, CRAFTER_HOTBAR_0, 0, ClickInput::Pickup);
    world.click(WINDOW, 4, 0, ClickInput::Pickup);
    world.close(WINDOW);
    world.open_chest();

    let slots = content(&world.alice, 2).expect("second window");
    assert_eq!(slots[4], Some((item("minecraft:oak_log"), 1)));
    assert_eq!(slots[45], Some((item("minecraft:oak_planks"), 4)));
}

#[test]
fn a_disabled_crafter_slot_refuses_items_and_is_remembered() {
    let mut world = world();
    world.place_storage("crafter");
    world.give("stone");
    world.open_chest();
    world.set_crafter_slot(WINDOW, 2, false);

    assert_eq!(data_events(&world.alice, WINDOW).last(), Some(&(2, 1)));
    world.click(WINDOW, CRAFTER_HOTBAR_0, 0, ClickInput::Pickup);
    world.click(WINDOW, 2, 0, ClickInput::Pickup);
    assert_eq!(slot_update(&world.alice, WINDOW, 2), None);
    world.click(WINDOW, 3, 0, ClickInput::Pickup);
    assert_eq!(
        slot_update(&world.alice, WINDOW, 3),
        Some(Some((item("minecraft:stone"), 1)))
    );

    world.close(WINDOW);
    world.open_chest();
    assert!(data_events(&world.alice, 2).contains(&(2, 1)));
    world.set_crafter_slot(2, 2, true);
    assert_eq!(data_events(&world.alice, 2).last(), Some(&(2, 0)));
}

#[test]
fn a_slot_state_for_another_window_or_a_filled_slot_changes_nothing() {
    let mut world = world();
    world.place_storage("crafter");
    world.give("stone");
    world.open_chest();
    world.click(WINDOW, CRAFTER_HOTBAR_0, 0, ClickInput::Pickup);
    world.click(WINDOW, 3, 0, ClickInput::Pickup);
    world.set_crafter_slot(WINDOW + 5, 2, false);
    world.set_crafter_slot(WINDOW, 3, false);
    world.set_crafter_slot(WINDOW, 12, false);

    assert!(
        data_events(&world.alice, WINDOW)
            .iter()
            .all(|(_, v)| *v == 0)
    );
}
