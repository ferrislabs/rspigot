use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rspigot_core::admin::{AdminLists, ListEntry};
use rspigot_core::config::{GameConfig, LevelData};
use rspigot_core::events::{ClientEvent, DigAction, Direction, Inbound, PlayerInput};
use rspigot_core::ports::{ChunkGenerator, ChunkStore, ClientLink, EntityStore, StoreError};
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::player::{ClientSettings, GameMode, GameProfile, PlayerId};
use rspigot_domain::text::Component;
use rspigot_domain::world::Dimension;
use rspigot_domain::{BiomeId, BlockPos, BlockStateId, ChunkPos, Identifier, Vec3};

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
    ) -> Result<Option<Vec<rspigot_nbt::Compound>>, StoreError> {
        Ok(None)
    }

    fn save_entities(
        &self,
        _: &Identifier,
        _: ChunkPos,
        _: &[rspigot_nbt::Compound],
    ) -> Result<(), StoreError> {
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

fn profile(name: &str) -> GameProfile {
    GameProfile {
        id: PlayerId::offline(name),
        name: name.to_owned(),
        properties: Vec::new(),
    }
}

fn server(operators: Vec<String>) -> Server<Link> {
    server_with(operators, false)
}

fn server_with(operators: Vec<String>, whitelist_enabled: bool) -> Server<Link> {
    server_configured(
        operators,
        whitelist_enabled,
        GameConfig::default().item_despawn_ticks,
    )
}

fn server_configured(
    operators: Vec<String>,
    whitelist_enabled: bool,
    item_despawn_ticks: u32,
) -> Server<Link> {
    let config = GameConfig {
        view_distance: 3,
        chunk_workers: 2,
        item_despawn_ticks,
        admin: AdminLists {
            operators: operators
                .iter()
                .map(|name| ListEntry::named(name))
                .collect(),
            whitelist: Vec::new(),
            whitelist_enabled,
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
    );
    server.wait_for_spawn(Duration::from_secs(10));
    server
}

fn join(server: &mut Server<Link>, name: &str) -> (PlayerId, Link) {
    let link = Link::default();
    let profile = profile(name);
    let id = profile.id;
    server.handle(Inbound::Join {
        profile,
        settings: ClientSettings::default(),
        link: link.clone(),
        address: std::net::Ipv4Addr::LOCALHOST.into(),
        session: 0,
    });
    server.deliver();
    (id, link)
}

fn confirm_teleport(server: &mut Server<Link>, id: PlayerId, events: &[ClientEvent]) {
    for event in events {
        if let ClientEvent::Teleport { id: teleport, .. } = event {
            server.handle(Inbound::Input {
                player: id,
                input: PlayerInput::TeleportConfirm(*teleport),
            });
        }
    }
}

fn run_ticks(
    server: &mut Server<Link>,
    ticks: usize,
    links: &[(PlayerId, &Link)],
    log: &mut [Vec<ClientEvent>],
) {
    for _ in 0..ticks {
        server.wait_for_spawn(Duration::from_millis(20));
        server.tick();
        for (index, (id, link)) in links.iter().enumerate() {
            let events = link.take();
            let batches = events
                .iter()
                .filter(|e| matches!(e, ClientEvent::ChunkBatchFinished { .. }))
                .count();
            for _ in 0..batches {
                server.handle(Inbound::Input {
                    player: *id,
                    input: PlayerInput::ChunkBatchReceived {
                        chunks_per_tick: 64.0,
                    },
                });
            }
            log[index].extend(events);
        }
    }
}

fn chunks_received(events: &[ClientEvent]) -> Vec<ChunkPos> {
    events
        .iter()
        .filter_map(|e| match e {
            ClientEvent::Chunk { chunk, .. } => Some(chunk.pos()),
            _ => None,
        })
        .collect()
}

#[test]
fn joining_player_receives_join_then_chunks_nearest_first() {
    let mut server = server(Vec::new());
    let (id, link) = join(&mut server, "Alice");
    let first = link.take();
    assert!(matches!(first.first(), Some(ClientEvent::Join(_))));
    assert!(
        first
            .iter()
            .any(|e| matches!(e, ClientEvent::Teleport { .. }))
    );
    assert!(
        first
            .iter()
            .any(|e| matches!(e, ClientEvent::CenterChunk(p) if *p == ChunkPos::new(0, 0)))
    );
    let mut log = vec![Vec::new()];
    run_ticks(&mut server, 40, &[(id, &link)], &mut log);
    let chunks = chunks_received(&log[0]);
    assert_eq!(chunks.first(), Some(&ChunkPos::new(0, 0)));
    let unique: std::collections::HashSet<_> = chunks.iter().collect();
    assert_eq!(unique.len(), chunks.len());
    assert!(chunks.len() >= 25);
    assert_eq!(server.players_online(), 1);
}

#[test]
fn two_players_see_each_other_and_chat() {
    let mut server = server(Vec::new());
    let (alice, alice_link) = join(&mut server, "Alice");
    let (bob, bob_link) = join(&mut server, "Bob");
    let mut log = vec![Vec::new(), Vec::new()];
    confirm_teleport(&mut server, alice, &alice_link.take());
    confirm_teleport(&mut server, bob, &bob_link.take());
    run_ticks(
        &mut server,
        40,
        &[(alice, &alice_link), (bob, &bob_link)],
        &mut log,
    );
    let alice_spawns = log[0]
        .iter()
        .filter(|e| matches!(e, ClientEvent::SpawnEntity { .. }))
        .count();
    assert_eq!(alice_spawns, 1);
    server.handle(Inbound::Input {
        player: alice,
        input: PlayerInput::Move {
            position: Some(Vec3::new(2.5, 64.0, 0.5)),
            rotation: None,
            on_ground: true,
        },
    });
    server.handle(Inbound::Input {
        player: alice,
        input: PlayerInput::Chat("hello".to_owned()),
    });
    server.deliver();
    let bob_events = bob_link.take();
    assert!(
        bob_events
            .iter()
            .any(|e| matches!(e, ClientEvent::EntityMoved { to, .. } if to.x == 2.5))
    );
    assert!(
        bob_events
            .iter()
            .any(|e| matches!(e, ClientEvent::PlayerChat { message, .. } if message == "hello"))
    );
    server.handle(Inbound::Leave {
        player: bob,
        session: 0,
    });
    server.deliver();
    let alice_events = alice_link.take();
    assert!(
        alice_events
            .iter()
            .any(|e| matches!(e, ClientEvent::RemoveEntities(_)))
    );
    assert!(
        alice_events
            .iter()
            .any(|e| matches!(e, ClientEvent::PlayerInfoRemove(ids) if ids == &vec![bob]))
    );
}

#[test]
fn creative_operator_breaks_block_and_viewers_are_notified() {
    let mut server = server(vec!["alice".to_owned()]);
    let (alice, alice_link) = join(&mut server, "Alice");
    let (bob, bob_link) = join(&mut server, "Bob");
    let mut log = vec![Vec::new(), Vec::new()];
    confirm_teleport(&mut server, alice, &alice_link.take());
    confirm_teleport(&mut server, bob, &bob_link.take());
    run_ticks(
        &mut server,
        40,
        &[(alice, &alice_link), (bob, &bob_link)],
        &mut log,
    );
    server.handle(Inbound::Input {
        player: alice,
        input: PlayerInput::Command("gamemode creative".to_owned()),
    });
    let target = BlockPos::new(1, 63, 1);
    server.handle(Inbound::Input {
        player: alice,
        input: PlayerInput::Dig {
            action: DigAction::Start,
            pos: target,
            face: Direction::Up,
            sequence: 7,
        },
    });
    server.deliver();
    let alice_events = alice_link.take();
    assert!(alice_events.iter().any(|e| matches!(
        e,
        ClientEvent::GameEvent(rspigot_core::events::GameEvent::ChangeGameMode(
            GameMode::Creative
        ))
    )));
    assert!(
        alice_events
            .iter()
            .any(|e| matches!(e, ClientEvent::BlockChangedAck(7)))
    );
    let bob_events = bob_link.take();
    assert!(bob_events.iter().any(
        |e| matches!(e, ClientEvent::BlockUpdate { pos, state } if *pos == target && state.is_air())
    ));
}

#[test]
fn non_operator_survival_player_cannot_use_gamemode() {
    let mut server = server(Vec::new());
    let (alice, alice_link) = join(&mut server, "Alice");
    alice_link.take();
    server.handle(Inbound::Input {
        player: alice,
        input: PlayerInput::Command("gamemode creative".to_owned()),
    });
    server.deliver();
    let events = alice_link.take();
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, ClientEvent::GameEvent(_)))
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, ClientEvent::SystemChat { .. }))
    );
}

#[test]
fn whitelist_rejects_unlisted_player_and_admits_after_add() {
    let mut server = server_with(vec!["alice".to_owned()], true);
    let (_, bob_link) = join(&mut server, "Bob");
    assert!(!bob_link.is_connected());
    assert_eq!(server.players_online(), 0);
    let (_, alice_link) = join(&mut server, "Alice");
    assert!(alice_link.is_connected());
    server.handle(Inbound::Console("whitelist add Bob".to_owned()));
    let (_, bob_again) = join(&mut server, "Bob");
    assert!(bob_again.is_connected());
    assert_eq!(server.players_online(), 2);
}

fn command(server: &mut Server<Link>, player: PlayerId, line: &str) {
    server.handle(Inbound::Input {
        player,
        input: PlayerInput::Command(line.to_owned()),
    });
    server.deliver();
}

fn effect_events(
    events: &[ClientEvent],
) -> (
    Vec<rspigot_domain::effect::EffectInstance>,
    Vec<rspigot_domain::effect::EffectId>,
) {
    let applied = events
        .iter()
        .filter_map(|e| match e {
            ClientEvent::EffectApplied { effect, .. } => Some(*effect),
            _ => None,
        })
        .collect();
    let removed = events
        .iter()
        .filter_map(|e| match e {
            ClientEvent::EffectRemoved { effect, .. } => Some(*effect),
            _ => None,
        })
        .collect();
    (applied, removed)
}

fn movement_speed(
    events: &[ClientEvent],
) -> Option<Vec<(f64, rspigot_domain::effect::AttributeOperation)>> {
    events.iter().rev().find_map(|e| match e {
        ClientEvent::Attributes { attributes, .. } => attributes
            .iter()
            .find(|a| a.attribute == "minecraft:movement_speed")
            .map(|a| {
                a.modifiers
                    .iter()
                    .map(|m| (m.amount, m.operation))
                    .collect()
            }),
        _ => None,
    })
}

#[test]
fn effect_give_speed_250_reaches_the_client_with_attributes() {
    let mut server = server(vec!["alice".to_owned()]);
    let (alice, link) = join(&mut server, "Alice");
    link.take();
    command(&mut server, alice, "effect give @s speed 99999 250 false");
    let events = link.take();
    let (applied, _) = effect_events(&events);
    assert_eq!(applied.len(), 1);
    assert_eq!(applied[0].amplifier, 250);
    assert_eq!(
        applied[0].duration,
        rspigot_domain::effect::EffectDuration::Ticks(1_999_980)
    );
    assert!(applied[0].visible);
    let modifiers = movement_speed(&events).unwrap_or_default();
    assert_eq!(modifiers.len(), 1);
    assert!((modifiers[0].0 - 50.2).abs() < 1e-5);
    assert_eq!(
        modifiers[0].1,
        rspigot_domain::effect::AttributeOperation::AddMultipliedTotal
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, ClientEvent::SystemChat { .. }))
    );
}

#[test]
fn weaker_effect_is_refused_and_clear_resets_attributes() {
    let mut server = server(vec!["alice".to_owned()]);
    let (alice, link) = join(&mut server, "Alice");
    command(&mut server, alice, "effect give @s speed 60 3");
    link.take();
    command(&mut server, alice, "effect give @s speed 600 1");
    let refused = link.take();
    assert!(effect_events(&refused).0.is_empty());
    assert!(refused.iter().any(|e| matches!(e, ClientEvent::SystemChat { message, .. } if message.plain_text().contains("commands.effect.give.failed"))));
    command(&mut server, alice, "effect clear @s speed");
    let cleared = link.take();
    assert_eq!(effect_events(&cleared).1.len(), 1);
    assert_eq!(movement_speed(&cleared), Some(Vec::new()));
}

#[test]
fn effect_expires_after_its_duration() {
    let mut server = server(vec!["alice".to_owned()]);
    let (alice, link) = join(&mut server, "Alice");
    command(&mut server, alice, "effect give @s night_vision 1");
    link.take();
    for _ in 0..19 {
        server.tick();
    }
    assert!(effect_events(&link.take()).1.is_empty());
    server.tick();
    assert_eq!(effect_events(&link.take()).1.len(), 1);
}

#[test]
fn invisibility_sets_the_shared_flag_for_viewers() {
    let mut server = server(vec!["alice".to_owned()]);
    let (alice, alice_link) = join(&mut server, "Alice");
    let (bob, bob_link) = join(&mut server, "Bob");
    let mut log = vec![Vec::new(), Vec::new()];
    confirm_teleport(&mut server, alice, &alice_link.take());
    confirm_teleport(&mut server, bob, &bob_link.take());
    run_ticks(
        &mut server,
        30,
        &[(alice, &alice_link), (bob, &bob_link)],
        &mut log,
    );
    command(&mut server, alice, "effect give Alice invisibility 30");
    let flags = bob_link.take().iter().find_map(|e| match e {
        ClientEvent::EntityMetadata { entries, .. } => entries
            .iter()
            .find(|m| m.index == 0)
            .map(|m| m.value.clone()),
        _ => None,
    });
    assert_eq!(flags, Some(rspigot_core::events::MetadataValue::Byte(0x20)));
}

#[test]
fn non_operator_cannot_give_effects() {
    let mut server = server(Vec::new());
    let (alice, link) = join(&mut server, "Alice");
    link.take();
    command(&mut server, alice, "effect give @s speed");
    assert!(effect_events(&link.take()).0.is_empty());
}

#[test]
fn door_item_places_both_halves_and_use_opens_both() {
    use rspigot_blocks::{Block, BlockKind, OakDoor};
    use rspigot_core::events::{Hand, PlayerInput};
    use rspigot_domain::item::ItemStack;
    use std::num::NonZeroU8;

    let mut server = server(vec!["alice".to_owned()]);
    let (alice, alice_link) = join(&mut server, "Alice");
    let mut log = vec![Vec::new()];
    confirm_teleport(&mut server, alice, &alice_link.take());
    run_ticks(&mut server, 30, &[(alice, &alice_link)], &mut log);
    command(&mut server, alice, "gamemode creative");
    let door_item = OakDoor::ITEM.unwrap_or_default();
    server.handle(Inbound::Input {
        player: alice,
        input: PlayerInput::CreativeSlot {
            slot: 36,
            stack: Some(ItemStack::new(
                rspigot_domain::ItemId(door_item),
                NonZeroU8::MIN,
            )),
        },
    });
    let ground = BlockPos::new(2, 63, 0);
    let use_on = |pos: BlockPos, sequence: i32| Inbound::Input {
        player: alice,
        input: PlayerInput::UseItemOn {
            hand: Hand::Main,
            pos,
            face: Direction::Up,
            cursor: Vec3::new(0.5, 1.0, 0.5),
            inside_block: false,
            sequence,
        },
    };
    alice_link.take();
    server.handle(use_on(ground, 1));
    server.deliver();
    let placed: Vec<(BlockPos, BlockStateId)> = alice_link
        .take()
        .iter()
        .filter_map(|e| match e {
            ClientEvent::BlockUpdate { pos, state } => Some((*pos, *state)),
            _ => None,
        })
        .collect();
    assert_eq!(placed.len(), 2);
    assert!(
        placed
            .iter()
            .all(|(_, s)| s.kind() == BlockKind::from_name("oak_door"))
    );
    assert_eq!(placed[0].1.property("half"), Some("lower"));
    assert_eq!(placed[1].0, BlockPos::new(2, 65, 0));
    assert_eq!(placed[1].1.property("half"), Some("upper"));
    server.handle(use_on(BlockPos::new(2, 64, 0), 2));
    server.deliver();
    let opened: Vec<Option<&str>> = alice_link
        .take()
        .iter()
        .filter_map(|e| match e {
            ClientEvent::BlockUpdate { state, .. } => Some(state.property("open")),
            _ => None,
        })
        .collect();
    assert_eq!(opened, vec![Some("true"), Some("true")]);
}

const MINING_TICKS: usize = 40;

fn dig(server: &mut Server<Link>, player: PlayerId, pos: BlockPos, action: DigAction) {
    if action == DigAction::Finish {
        server.handle(Inbound::Input {
            player,
            input: PlayerInput::Move {
                position: None,
                rotation: None,
                on_ground: true,
            },
        });
        for _ in 0..MINING_TICKS {
            server.tick();
        }
    }
    server.handle(Inbound::Input {
        player,
        input: PlayerInput::Dig {
            action,
            pos,
            face: Direction::Up,
            sequence: 1,
        },
    });
    server.deliver();
}

fn spawned_items(events: &[ClientEvent]) -> Vec<rspigot_domain::EntityId> {
    events
        .iter()
        .filter_map(|e| match e {
            ClientEvent::SpawnEntity {
                entity,
                kind: rspigot_core::events::EntityKind::Item,
                ..
            } => Some(*entity),
            _ => None,
        })
        .collect()
}

fn dropped_item_name(events: &[ClientEvent]) -> Option<&'static str> {
    events.iter().find_map(|e| match e {
        ClientEvent::EntityMetadata { entries, .. } => {
            entries.iter().find_map(|m| match &m.value {
                rspigot_core::events::MetadataValue::Item(Some(stack)) => {
                    stack.item.kind().map(rspigot_items::ItemKind::name)
                }
                _ => None,
            })
        }
        _ => None,
    })
}

fn ready_player(server: &mut Server<Link>, name: &str) -> (PlayerId, Link) {
    let (id, link) = join(server, name);
    let mut log = vec![Vec::new()];
    confirm_teleport(server, id, &link.take());
    run_ticks(server, 30, &[(id, &link)], &mut log);
    (id, link)
}

#[test]
fn stone_mined_with_a_pickaxe_drops_cobblestone_seen_by_others() {
    let mut server = server(vec!["alice".to_owned()]);
    let (alice, alice_link) = ready_player(&mut server, "Alice");
    let (bob, bob_link) = join(&mut server, "Bob");
    let mut log = vec![Vec::new(), Vec::new()];
    confirm_teleport(&mut server, bob, &bob_link.take());
    run_ticks(
        &mut server,
        30,
        &[(alice, &alice_link), (bob, &bob_link)],
        &mut log,
    );
    command(&mut server, alice, "give Alice stone_pickaxe");
    dig(
        &mut server,
        alice,
        BlockPos::new(3, 63, 3),
        DigAction::Start,
    );
    dig(
        &mut server,
        alice,
        BlockPos::new(3, 63, 3),
        DigAction::Finish,
    );
    assert_eq!(server.items_on_ground(), 1);
    server.tick();
    let seen = bob_link.take();
    assert_eq!(spawned_items(&seen).len(), 1);
    assert_eq!(dropped_item_name(&seen), Some("minecraft:cobblestone"));
}

#[test]
fn stone_mined_by_hand_drops_nothing_and_creative_never_drops() {
    let mut server = server(vec!["alice".to_owned()]);
    let (alice, _link) = ready_player(&mut server, "Alice");
    dig(
        &mut server,
        alice,
        BlockPos::new(3, 63, 3),
        DigAction::Start,
    );
    dig(
        &mut server,
        alice,
        BlockPos::new(3, 63, 3),
        DigAction::Finish,
    );
    assert_eq!(server.items_on_ground(), 0);
    command(&mut server, alice, "give Alice stone_pickaxe");
    command(&mut server, alice, "gamemode creative");
    dig(
        &mut server,
        alice,
        BlockPos::new(4, 63, 4),
        DigAction::Start,
    );
    assert_eq!(server.items_on_ground(), 0);
}

#[test]
fn nearby_player_picks_up_the_drop_after_the_delay() {
    let mut server = server(vec!["alice".to_owned()]);
    let (alice, link) = ready_player(&mut server, "Alice");
    command(&mut server, alice, "give Alice stone_pickaxe");
    dig(
        &mut server,
        alice,
        BlockPos::new(1, 63, 1),
        DigAction::Start,
    );
    dig(
        &mut server,
        alice,
        BlockPos::new(1, 63, 1),
        DigAction::Finish,
    );
    server.handle(Inbound::Input {
        player: alice,
        input: PlayerInput::Move {
            position: Some(Vec3::new(1.5, 63.0, 1.5)),
            rotation: None,
            on_ground: true,
        },
    });
    link.take();
    for _ in 0..5 {
        server.tick();
    }
    assert_eq!(server.items_on_ground(), 1);
    for _ in 0..20 {
        server.tick();
    }
    assert_eq!(server.items_on_ground(), 0);
    let events = link.take();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, ClientEvent::ItemPickup { amount: 1, .. }))
    );
    let cobblestone = rspigot_items::ItemKind::Cobblestone.id();
    assert!(events.iter().any(
        |e| matches!(e, ClientEvent::InventorySlot { stack: Some(s), .. } if s.item == cobblestone)
    ));
}

#[test]
fn unpicked_drop_despawns() {
    let mut server = server_configured(vec!["alice".to_owned()], false, 40);
    let (alice, link) = ready_player(&mut server, "Alice");
    command(&mut server, alice, "give Alice stone_pickaxe");
    command(&mut server, alice, "tp 0.5 64 40.5");
    dig(
        &mut server,
        alice,
        BlockPos::new(0, 63, 38),
        DigAction::Start,
    );
    dig(
        &mut server,
        alice,
        BlockPos::new(0, 63, 38),
        DigAction::Finish,
    );
    link.take();
    assert_eq!(server.items_on_ground(), 1);
    for _ in 0..45 {
        server.tick();
    }
    assert_eq!(server.items_on_ground(), 0);
    assert!(
        link.take()
            .iter()
            .any(|e| matches!(e, ClientEvent::RemoveEntities(_)))
    );
}
