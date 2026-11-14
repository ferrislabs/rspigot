use std::num::NonZeroU8;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rspigot_blocks::{Block, Stone};
use rspigot_core::admin::{AdminLists, ListEntry};
use rspigot_core::chunks::ChunkManagerConfig;
use rspigot_core::config::{GameConfig, LevelData};
use rspigot_core::events::{
    ClickInput, ClientEvent, ContainerClick, DigAction, Direction, EntityKind, HashedSlot, Inbound,
    MetadataValue, PlayerInput,
};
use rspigot_core::ports::{ChunkGenerator, ChunkStore, ClientLink, EntityStore, StoreError};
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::item::{ItemStack, Slot};
use rspigot_domain::player::{ClientSettings, GameMode, GameProfile, PlayerId};
use rspigot_domain::text::Component;
use rspigot_domain::world::Dimension;
use rspigot_domain::{BiomeId, BlockPos, ChunkPos, EntityId, Identifier, Vec3};
use rspigot_entities::EntityKind as Kind;
use rspigot_items::ItemKind;
use rspigot_nbt::{Compound, Tag, read_named, write_named};
use rustc_hash::FxHashMap;

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

#[derive(Default)]
struct MemoryStore {
    entities: Mutex<FxHashMap<ChunkPos, Vec<u8>>>,
}

impl MemoryStore {
    fn stored(&self, pos: ChunkPos) -> Option<Vec<u8>> {
        self.entities.lock().ok()?.get(&pos).cloned()
    }
}

impl ChunkStore for MemoryStore {
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

impl EntityStore for MemoryStore {
    fn load_entities(
        &self,
        _: &Identifier,
        _: ChunkPos,
    ) -> Result<Option<Vec<Compound>>, StoreError> {
        Ok(None)
    }

    fn save_entities(
        &self,
        _: &Identifier,
        pos: ChunkPos,
        entities: &[Compound],
    ) -> Result<(), StoreError> {
        if let Ok(mut map) = self.entities.lock() {
            let root = Compound::new().with(
                "Entities",
                Tag::List(entities.iter().cloned().map(Tag::Compound).collect()),
            );
            let mut out = Vec::new();
            write_named(&mut out, "", &root);
            map.insert(pos, out);
        }
        Ok(())
    }
}

struct Flat;

impl ChunkGenerator for Flat {
    fn generate(&self, dimension: &Dimension, pos: ChunkPos) -> ChunkColumn {
        let mut chunk = ChunkColumn::empty(pos, dimension.height, BiomeId(0));
        for x in 0..16 {
            for z in 0..16 {
                for y in 60..=63 {
                    chunk.set_block(
                        BlockPos::new(pos.min_block_x() + x, y, pos.min_block_z() + z),
                        Stone::DEFAULT_STATE,
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

fn server(store: Arc<MemoryStore>) -> Server<Link> {
    let config = GameConfig {
        view_distance: 3,
        chunk_workers: 2,
        chunks: ChunkManagerConfig {
            unload_delay_ticks: 5,
            autosave_interval_ticks: 0,
        },
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
        store,
        Arc::new(Flat),
        StatusBoard::new(20),
    );
    server.wait_for_spawn(Duration::from_secs(10));
    server
}

struct Client {
    id: PlayerId,
    link: Link,
    events: Vec<ClientEvent>,
    slots: Vec<Slot>,
    carried: Slot,
    state_id: i32,
}

impl Client {
    fn absorb(&mut self, events: &[ClientEvent]) {
        for event in events {
            match event {
                ClientEvent::InventoryContent {
                    state_id,
                    slots,
                    carried,
                } => {
                    self.state_id = *state_id;
                    self.slots = slots.clone();
                    self.carried = carried.clone();
                }
                ClientEvent::InventorySlot {
                    state_id,
                    slot,
                    stack,
                } => {
                    self.state_id = *state_id;
                    if let Ok(index) = usize::try_from(*slot) {
                        self.slots[index] = stack.clone();
                    }
                }
                ClientEvent::PlayerInventorySlot { slot, stack } => {
                    let menu = match *slot {
                        0..=8 => 36 + *slot as usize,
                        40 => 45,
                        other => other as usize,
                    };
                    self.slots[menu] = stack.clone();
                }
                ClientEvent::CursorItem(stack) => self.carried = stack.clone(),
                _ => {}
            }
        }
    }

    fn drain(&mut self) -> Vec<ClientEvent> {
        let fresh = self.link.take();
        self.absorb(&fresh);
        self.events.extend(fresh);
        std::mem::take(&mut self.events)
    }
}

fn input(server: &mut Server<Link>, player: PlayerId, input: PlayerInput) {
    server.handle(Inbound::Input { player, input });
    server.deliver();
}

fn pump(server: &mut Server<Link>, clients: &mut [&mut Client], ticks: usize) {
    for _ in 0..ticks {
        server.wait_for_spawn(Duration::from_millis(20));
        server.tick();
        for client in clients.iter_mut() {
            let events = client.link.take();
            for event in &events {
                match event {
                    ClientEvent::ChunkBatchFinished { .. } => input(
                        server,
                        client.id,
                        PlayerInput::ChunkBatchReceived {
                            chunks_per_tick: 64.0,
                        },
                    ),
                    ClientEvent::Teleport { id, .. } => {
                        input(server, client.id, PlayerInput::TeleportConfirm(*id))
                    }
                    _ => {}
                }
            }
            client.absorb(&events);
            client.events.extend(events);
        }
    }
}

fn join(server: &mut Server<Link>, name: &str) -> Client {
    let link = Link::default();
    let profile = GameProfile {
        id: PlayerId::offline(name),
        name: name.to_owned(),
        properties: Vec::new(),
    };
    let id = profile.id;
    server.handle(Inbound::Join {
        profile,
        settings: ClientSettings::default(),
        link: link.clone(),
        address: std::net::Ipv4Addr::LOCALHOST.into(),
        session: 0,
    });
    server.deliver();
    let mut client = Client {
        id,
        link,
        events: Vec::new(),
        slots: vec![None; 46],
        carried: None,
        state_id: 0,
    };
    let first = client.link.take();
    for event in &first {
        if let ClientEvent::Teleport { id: teleport, .. } = event {
            input(server, id, PlayerInput::TeleportConfirm(*teleport));
        }
    }
    client.absorb(&first);
    input(server, id, PlayerInput::Loaded);
    pump(server, &mut [&mut client], 30);
    client.drain();
    client
}

fn command(server: &mut Server<Link>, client: &mut Client, line: &str) {
    input(server, client.id, PlayerInput::Command(line.to_owned()));
    client.drain();
}

fn stack(kind: ItemKind, count: u8) -> ItemStack {
    ItemStack::new(kind.id(), NonZeroU8::new(count).unwrap())
}

fn dig(server: &mut Server<Link>, client: &Client, action: DigAction) {
    input(
        server,
        client.id,
        PlayerInput::Dig {
            action,
            pos: BlockPos::new(0, 0, 0),
            face: Direction::Down,
            sequence: 0,
        },
    );
}

fn click(
    server: &mut Server<Link>,
    client: &mut Client,
    slot: i16,
    button: i8,
    input_kind: ClickInput,
    changed: Vec<(i16, Slot)>,
    carried: Slot,
) {
    for (index, stack) in &changed {
        if let Ok(index) = usize::try_from(*index) {
            client.slots[index] = stack.clone();
        }
    }
    client.carried = carried.clone();
    let packet = ContainerClick {
        container: 0,
        state_id: client.state_id,
        slot,
        button,
        input: input_kind,
        changed: changed
            .iter()
            .map(|(index, stack)| (*index, HashedSlot::of(stack)))
            .collect(),
        carried: HashedSlot::of(&carried),
    };
    input(server, client.id, PlayerInput::ContainerClick(packet));
}

fn item_spawns(events: &[ClientEvent]) -> Vec<(EntityId, Vec3, Vec3)> {
    events
        .iter()
        .filter_map(|e| match e {
            ClientEvent::SpawnEntity {
                entity,
                kind: EntityKind::Item,
                position,
                velocity,
                ..
            } => Some((*entity, *position, *velocity)),
            _ => None,
        })
        .collect()
}

fn item_metadata(events: &[ClientEvent], id: EntityId) -> Option<ItemStack> {
    events.iter().find_map(|e| match e {
        ClientEvent::EntityMetadata { entity, entries } if *entity == id => {
            entries.iter().find_map(|entry| match &entry.value {
                MetadataValue::Item(stack) if entry.index == 8 => stack.clone(),
                _ => None,
            })
        }
        _ => None,
    })
}

fn dropped(events: &[ClientEvent]) -> Vec<ItemStack> {
    item_spawns(events)
        .into_iter()
        .filter_map(|(id, _, _)| item_metadata(events, id))
        .collect()
}

fn inventory_resyncs(events: &[ClientEvent]) -> Vec<&ClientEvent> {
    events
        .iter()
        .filter(|e| {
            matches!(
                e,
                ClientEvent::InventoryContent { .. }
                    | ClientEvent::InventorySlot { .. }
                    | ClientEvent::CursorItem(_)
                    | ClientEvent::PlayerInventorySlot { .. }
            )
        })
        .collect()
}

fn eye_drop_height(feet: f64) -> f64 {
    feet + f64::from(1.62f32) - f64::from(0.3f32)
}

#[test]
fn q_drops_one_item_from_the_selected_slot_without_a_resync() {
    let mut server = server(Arc::default());
    let mut alice = join(&mut server, "Alice");
    command(&mut server, &mut alice, "give Alice stone 5");
    assert_eq!(alice.slots[36], Some(stack(ItemKind::Stone, 5)));
    dig(&mut server, &alice, DigAction::DropItem);
    let events = alice.drain();
    assert_eq!(dropped(&events), vec![stack(ItemKind::Stone, 1)]);
    let (_, position, velocity) = item_spawns(&events)[0];
    assert_eq!(position, Vec3::new(0.5, eye_drop_height(64.0), 0.5));
    assert!(velocity.x.abs() <= 0.021, "{velocity:?}");
    assert!((0.27..=0.33).contains(&velocity.z), "{velocity:?}");
    assert!((-0.01..=0.21).contains(&velocity.y), "{velocity:?}");
    assert!(inventory_resyncs(&events).is_empty(), "{events:?}");
    assert_eq!(server.items_on_ground(), 1);
    pump(&mut server, &mut [&mut alice], 2);
    assert!(inventory_resyncs(&alice.drain()).is_empty());
    dig(&mut server, &alice, DigAction::DropStack);
    assert_eq!(dropped(&alice.drain()), vec![stack(ItemKind::Stone, 4)]);
    dig(&mut server, &alice, DigAction::DropItem);
    assert!(dropped(&alice.drain()).is_empty());
    assert_eq!(server.items_on_ground(), 2);
}

#[test]
fn ctrl_q_drops_the_whole_selected_stack_in_creative_too() {
    let mut server = server(Arc::default());
    let mut alice = join(&mut server, "Alice");
    input(
        &mut server,
        alice.id,
        PlayerInput::ChangeGameMode(GameMode::Creative),
    );
    command(&mut server, &mut alice, "give Alice dirt 12");
    dig(&mut server, &alice, DigAction::DropStack);
    let events = alice.drain();
    assert_eq!(dropped(&events), vec![stack(ItemKind::Dirt, 12)]);
    assert_eq!(server.items_on_ground(), 1);
}

#[test]
fn items_dropped_too_fast_disconnect_after_twenty_in_one_tick() {
    let mut server = server(Arc::default());
    let mut alice = join(&mut server, "Alice");
    command(&mut server, &mut alice, "give Alice stone 64");
    for _ in 0..20 {
        dig(&mut server, &alice, DigAction::DropItem);
    }
    assert!(alice.link.is_connected());
    assert_eq!(server.items_on_ground(), 20);
    dig(&mut server, &alice, DigAction::DropItem);
    assert!(!alice.link.is_connected());
    assert_eq!(server.items_on_ground(), 20);
}

#[test]
fn creative_drag_out_drops_the_stack_and_is_throttled() {
    let mut server = server(Arc::default());
    let mut alice = join(&mut server, "Alice");
    input(
        &mut server,
        alice.id,
        PlayerInput::ChangeGameMode(GameMode::Creative),
    );
    alice.drain();
    let diamonds = stack(ItemKind::Diamond, 3);
    input(
        &mut server,
        alice.id,
        PlayerInput::CreativeSlot {
            slot: -1,
            stack: Some(diamonds.clone()),
        },
    );
    let events = alice.drain();
    assert_eq!(dropped(&events), vec![diamonds.clone()]);
    assert!(inventory_resyncs(&events).is_empty());
    for _ in 0..80 {
        input(
            &mut server,
            alice.id,
            PlayerInput::CreativeSlot {
                slot: -1,
                stack: Some(diamonds.clone()),
            },
        );
    }
    assert_eq!(dropped(&alice.drain()).len(), 73);
    pump(&mut server, &mut [&mut alice], 20);
    alice.drain();
    for _ in 0..3 {
        input(
            &mut server,
            alice.id,
            PlayerInput::CreativeSlot {
                slot: -1,
                stack: Some(diamonds.clone()),
            },
        );
    }
    assert_eq!(dropped(&alice.drain()).len(), 1);
}

#[test]
fn creative_slot_packets_from_survival_are_ignored() {
    let mut server = server(Arc::default());
    let mut alice = join(&mut server, "Alice");
    input(
        &mut server,
        alice.id,
        PlayerInput::CreativeSlot {
            slot: -1,
            stack: Some(stack(ItemKind::Diamond, 1)),
        },
    );
    input(
        &mut server,
        alice.id,
        PlayerInput::CreativeSlot {
            slot: 36,
            stack: Some(stack(ItemKind::Diamond, 1)),
        },
    );
    let events = alice.drain();
    assert!(dropped(&events).is_empty());
    assert!(inventory_resyncs(&events).is_empty());
    assert_eq!(server.items_on_ground(), 0);
}

#[test]
fn inventory_throw_drops_one_then_the_rest_of_the_slot() {
    let mut server = server(Arc::default());
    let mut alice = join(&mut server, "Alice");
    command(&mut server, &mut alice, "give Alice stone 5");
    click(
        &mut server,
        &mut alice,
        36,
        0,
        ClickInput::Throw,
        vec![(36, Some(stack(ItemKind::Stone, 4)))],
        None,
    );
    let events = alice.drain();
    assert_eq!(dropped(&events), vec![stack(ItemKind::Stone, 1)]);
    assert!(inventory_resyncs(&events).is_empty(), "{events:?}");
    click(
        &mut server,
        &mut alice,
        36,
        1,
        ClickInput::Throw,
        vec![(36, None)],
        None,
    );
    let events = alice.drain();
    assert_eq!(dropped(&events), vec![stack(ItemKind::Stone, 4)]);
    assert!(inventory_resyncs(&events).is_empty(), "{events:?}");
    pump(&mut server, &mut [&mut alice], 2);
    assert!(inventory_resyncs(&alice.drain()).is_empty());
    let ground = server.ground_items();
    assert_eq!(ground.len(), 1);
    assert_eq!(ground[0].1.count.get(), 5);
}

#[test]
fn clicking_outside_the_window_drops_the_carried_stack() {
    let mut server = server(Arc::default());
    let mut alice = join(&mut server, "Alice");
    command(&mut server, &mut alice, "give Alice stone 5");
    click(
        &mut server,
        &mut alice,
        36,
        0,
        ClickInput::Pickup,
        vec![(36, None)],
        Some(stack(ItemKind::Stone, 5)),
    );
    let events = alice.drain();
    assert!(dropped(&events).is_empty());
    assert!(inventory_resyncs(&events).is_empty(), "{events:?}");
    click(
        &mut server,
        &mut alice,
        -999,
        1,
        ClickInput::Pickup,
        Vec::new(),
        Some(stack(ItemKind::Stone, 4)),
    );
    assert_eq!(dropped(&alice.drain()), vec![stack(ItemKind::Stone, 1)]);
    click(
        &mut server,
        &mut alice,
        -999,
        0,
        ClickInput::Pickup,
        Vec::new(),
        None,
    );
    let events = alice.drain();
    assert_eq!(dropped(&events), vec![stack(ItemKind::Stone, 4)]);
    assert!(inventory_resyncs(&events).is_empty(), "{events:?}");
    assert_eq!(server.items_on_ground(), 2);
}

#[test]
fn closing_the_inventory_returns_the_carried_item_and_the_crafting_grid() {
    let mut server = server(Arc::default());
    let mut alice = join(&mut server, "Alice");
    command(&mut server, &mut alice, "give Alice stone 5");
    command(&mut server, &mut alice, "give Alice dirt 2");
    assert_eq!(alice.slots[37], Some(stack(ItemKind::Dirt, 2)));
    click(
        &mut server,
        &mut alice,
        37,
        0,
        ClickInput::Pickup,
        vec![(37, None)],
        Some(stack(ItemKind::Dirt, 2)),
    );
    click(
        &mut server,
        &mut alice,
        1,
        0,
        ClickInput::Pickup,
        vec![(1, Some(stack(ItemKind::Dirt, 2)))],
        None,
    );
    let events = alice.drain();
    assert!(
        events.iter().any(|e| matches!(
            e,
            ClientEvent::InventorySlot {
                slot: 0,
                stack: None,
                ..
            }
        )),
        "{events:?}"
    );
    click(
        &mut server,
        &mut alice,
        36,
        0,
        ClickInput::Pickup,
        vec![(36, None)],
        Some(stack(ItemKind::Stone, 5)),
    );
    alice.drain();
    input(&mut server, alice.id, PlayerInput::ContainerClose(0));
    let events = alice.drain();
    assert!(events.contains(&ClientEvent::PlayerInventorySlot {
        slot: 0,
        stack: Some(stack(ItemKind::Stone, 5)),
    }));
    assert!(events.contains(&ClientEvent::PlayerInventorySlot {
        slot: 1,
        stack: Some(stack(ItemKind::Dirt, 2)),
    }));
    pump(&mut server, &mut [&mut alice], 1);
    let events = alice.drain();
    assert!(
        events.contains(&ClientEvent::CursorItem(None)),
        "{events:?}"
    );
    assert_eq!(server.items_on_ground(), 0);
    assert_eq!(alice.slots[36], Some(stack(ItemKind::Stone, 5)));
    assert_eq!(alice.slots[37], Some(stack(ItemKind::Dirt, 2)));
    assert_eq!(alice.slots[1], None);
    assert_eq!(alice.carried, None);
}

#[test]
fn shift_click_and_hotbar_swap_move_items_without_a_resync() {
    let mut server = server(Arc::default());
    let mut alice = join(&mut server, "Alice");
    command(&mut server, &mut alice, "give Alice stone 5");
    click(
        &mut server,
        &mut alice,
        36,
        0,
        ClickInput::QuickMove,
        vec![(9, Some(stack(ItemKind::Stone, 5))), (36, None)],
        None,
    );
    click(
        &mut server,
        &mut alice,
        9,
        4,
        ClickInput::Swap,
        vec![(9, None), (40, Some(stack(ItemKind::Stone, 5)))],
        None,
    );
    pump(&mut server, &mut [&mut alice], 1);
    let events = alice.drain();
    assert!(inventory_resyncs(&events).is_empty(), "{events:?}");
    input(&mut server, alice.id, PlayerInput::HeldSlot(4));
    dig(&mut server, &alice, DigAction::DropStack);
    assert_eq!(dropped(&alice.drain()), vec![stack(ItemKind::Stone, 5)]);
}

#[test]
fn mispredicted_click_is_corrected_and_stale_state_resends_everything() {
    let mut server = server(Arc::default());
    let mut alice = join(&mut server, "Alice");
    command(&mut server, &mut alice, "give Alice stone 5");
    click(
        &mut server,
        &mut alice,
        36,
        0,
        ClickInput::Pickup,
        vec![(36, Some(stack(ItemKind::Stone, 2)))],
        Some(stack(ItemKind::Stone, 3)),
    );
    let events = alice.drain();
    assert!(events.iter().any(|e| matches!(
        e,
        ClientEvent::InventorySlot {
            slot: 36,
            stack: None,
            ..
        }
    )));
    assert!(events.contains(&ClientEvent::CursorItem(Some(stack(ItemKind::Stone, 5)))));
    alice.state_id = alice.state_id.wrapping_add(7);
    click(
        &mut server,
        &mut alice,
        36,
        0,
        ClickInput::Pickup,
        vec![(36, Some(stack(ItemKind::Stone, 5)))],
        None,
    );
    let events = alice.drain();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, ClientEvent::InventoryContent { .. })),
        "{events:?}"
    );
    assert_eq!(alice.slots[36], Some(stack(ItemKind::Stone, 5)));
    assert_eq!(alice.carried, None);
}

#[test]
fn dropper_and_second_player_both_receive_the_item_pairing() {
    let mut server = server(Arc::default());
    let mut alice = join(&mut server, "Alice");
    let mut bob = join(&mut server, "Bob");
    pump(&mut server, &mut [&mut alice, &mut bob], 5);
    command(&mut server, &mut alice, "give Alice stone 5");
    alice.drain();
    bob.drain();
    dig(&mut server, &alice, DigAction::DropItem);
    let alice_events = alice.drain();
    let bob_events = bob.drain();
    let alice_spawns = item_spawns(&alice_events);
    let bob_spawns = item_spawns(&bob_events);
    assert_eq!(alice_spawns.len(), 1);
    assert_eq!(bob_spawns.len(), 1);
    assert_eq!(alice_spawns[0].0, bob_spawns[0].0);
    let id = alice_spawns[0].0;
    assert_eq!(
        item_metadata(&alice_events, id),
        Some(stack(ItemKind::Stone, 1))
    );
    assert_eq!(
        item_metadata(&bob_events, id),
        Some(stack(ItemKind::Stone, 1))
    );
}

#[test]
fn dropped_item_is_picked_up_only_after_its_forty_tick_delay() {
    let mut server = server(Arc::default());
    let mut alice = join(&mut server, "Alice");
    command(&mut server, &mut alice, "give Alice stone 5");
    dig(&mut server, &alice, DigAction::DropItem);
    alice.drain();
    for tick in 1..=40 {
        let item = server.entities_of(Kind::Item)[0];
        input(
            &mut server,
            alice.id,
            PlayerInput::Move {
                position: Some(Vec3::new(item.position.x, 64.0, item.position.z)),
                rotation: None,
                on_ground: true,
            },
        );
        pump(&mut server, &mut [&mut alice], 1);
        if tick < 40 {
            assert_eq!(server.items_on_ground(), 1, "picked up at tick {tick}");
        }
    }
    assert_eq!(server.items_on_ground(), 0);
    let events = alice.drain();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, ClientEvent::ItemPickup { amount: 1, .. }))
    );
    assert_eq!(alice.slots[36], Some(stack(ItemKind::Stone, 5)));
}

fn thrower_of(compound: &Compound) -> Option<u128> {
    let ints = compound.get_int_array("Thrower")?;
    Some(
        ints.iter()
            .fold(0u128, |acc, part| (acc << 32) | u128::from(*part as u32)),
    )
}

#[test]
fn thrown_item_keeps_its_thrower_when_saved() {
    let store = Arc::new(MemoryStore::default());
    let mut server = server(Arc::clone(&store));
    let mut alice = join(&mut server, "Alice");
    server.handle(Inbound::Console("tp Alice 100 64 104".to_owned()));
    pump(&mut server, &mut [&mut alice], 40);
    command(&mut server, &mut alice, "give Alice stone 5");
    dig(&mut server, &alice, DigAction::DropItem);
    pump(&mut server, &mut [&mut alice], 2);
    let item = server.entities_of(Kind::Item)[0];
    let chunk = item.position.chunk();
    server.handle(Inbound::Console("tp Alice 2000 64 2000".to_owned()));
    pump(&mut server, &mut [&mut alice], 40);
    assert_eq!(server.items_on_ground(), 0);
    server.wait_for_entity_saves();
    let saved = store.stored(chunk).expect("item chunk saved on unload");
    let mut bytes = saved.as_slice();
    let (_, root) = read_named(&mut bytes).unwrap();
    let entities = root.get_list("Entities").unwrap();
    assert_eq!(entities.len(), 1);
    let saved_item = entities[0].as_compound().unwrap();
    assert_eq!(saved_item.get_str("id"), Some("minecraft:item"));
    assert_eq!(thrower_of(saved_item), Some(alice.id.0.as_u128()));
    assert_eq!(saved_item.get_i64("PickupDelay"), Some(38));
}

#[test]
fn creative_drop_above_the_max_stack_size_is_refused() {
    let mut server = server(Arc::default());
    let mut alice = join(&mut server, "Alice");
    input(
        &mut server,
        alice.id,
        PlayerInput::ChangeGameMode(GameMode::Creative),
    );
    input(
        &mut server,
        alice.id,
        PlayerInput::CreativeSlot {
            slot: -1,
            stack: Some(ItemStack::new(
                ItemKind::Stone.id(),
                NonZeroU8::new(99).unwrap(),
            )),
        },
    );
    assert!(dropped(&alice.drain()).is_empty());
    assert_eq!(server.items_on_ground(), 0);
}
