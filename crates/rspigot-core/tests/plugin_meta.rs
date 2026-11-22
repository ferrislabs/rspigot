mod common;

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use common::*;
use rspigot_core::admin::{AdminLists, ListEntry};
use rspigot_core::chunks::ChunkManagerConfig;
use rspigot_core::config::{GameConfig, LevelData};
use rspigot_core::events::{ClientEvent, Inbound, PlayerInput};
use rspigot_core::plugins::{
    ActionErrorKind, Answer, BlockOrder, EntityOrder, EventKind, EventPayload, FromPlugin,
    MetaChange, MetaHolder, MetaOrder, Order, PlayerOrder, Question, ToPlugin,
};
use rspigot_core::ports::{ChunkStore, EntityStore, StoreError};
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::meta::MetaValue;
use rspigot_domain::{BlockPos, ChunkPos, EntityId, Identifier, Vec3};
use rspigot_nbt::{Compound, Tag};
use rustc_hash::FxHashMap;

static NEXT_ACTION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(900_000);
const TAG: &str = "marker:kind";
const SPOT: BlockPos = BlockPos::new(2, 64, 2);
const FAR: BlockPos = BlockPos::new(100, 64, 100);

fn set(key: &str, value: MetaValue, volatile: bool) -> MetaOrder {
    MetaOrder {
        volatile,
        change: MetaChange::Set {
            key: key.to_owned(),
            value,
        },
    }
}

fn remove(key: &str, volatile: bool) -> MetaOrder {
    MetaOrder {
        volatile,
        change: MetaChange::Remove(key.to_owned()),
    }
}

fn clear(volatile: bool) -> MetaOrder {
    MetaOrder {
        volatile,
        change: MetaChange::Clear,
    }
}

fn on_entity(id: EntityId, order: MetaOrder) -> Order {
    Order::Entity {
        entity: id,
        order: EntityOrder::Meta(order),
    }
}

fn on_block(pos: BlockPos, order: MetaOrder) -> Order {
    Order::Block {
        pos,
        order: BlockOrder::Meta(order),
    }
}

fn put(s: &mut Scene, pos: BlockPos, kind: &str, properties: &[(&str, &str)]) {
    s.act(Order::Block {
        pos,
        order: BlockOrder::SetWith {
            kind: kind.to_owned(),
            properties: properties
                .iter()
                .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
                .collect(),
        },
    });
}

fn spawn(s: &mut Scene, kind: &str, at: Vec3) -> EntityId {
    let Answer::Spawned(view) = s.ask(Question::Spawn {
        kind: kind.into(),
        position: at,
    }) else {
        panic!("expected the spawned {kind}");
    };
    view.id
}

fn entries(
    s: &mut Scene,
    holder: MetaHolder,
    key: Option<&str>,
    volatile: bool,
) -> Vec<(String, MetaValue)> {
    match s.ask(Question::Meta {
        holder,
        key: key.map(str::to_owned),
        volatile,
    }) {
        Answer::MetaEntries(entries) => entries,
        other => panic!("expected meta entries, got {other:?}"),
    }
}

fn value_of(s: &mut Scene, holder: MetaHolder, key: &str, volatile: bool) -> Option<MetaValue> {
    entries(s, holder, Some(key), volatile)
        .into_iter()
        .next()
        .map(|(_, value)| value)
}

fn every_value() -> Vec<(&'static str, MetaValue)> {
    let mut inner = BTreeMap::new();
    inner.insert("a".to_owned(), MetaValue::Int(-3));
    inner.insert("b".to_owned(), MetaValue::List(vec![MetaValue::Bool(true)]));
    vec![
        ("t:bool", MetaValue::Bool(true)),
        ("t:int", MetaValue::Int(i64::MAX)),
        ("t:double", MetaValue::Double(2.5)),
        ("t:text", MetaValue::Text("hello".to_owned())),
        ("t:bytes", MetaValue::Bytes(vec![0, 7, 255])),
        (
            "t:list",
            MetaValue::List(vec![MetaValue::Int(1), MetaValue::Text("x".to_owned())]),
        ),
        ("t:map", MetaValue::Map(inner)),
    ]
}

fn act_result(s: &mut Scene, order: Order) -> Answer {
    let request = NEXT_ACTION.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    s.server.handle(Inbound::PluginMessage {
        connection: 1,
        message: Box::new(FromPlugin::Action { request, order }),
    });
    s.server.deliver();
    s.plugin
        .received()
        .into_iter()
        .rev()
        .find_map(|message| match message {
            ToPlugin::Answer {
                request: answered,
                answer,
            } if answered == request => Some(answer),
            _ => None,
        })
        .expect("the server answers the action")
}

#[test]
fn every_value_type_round_trips_on_an_entity() {
    let mut s = scene(&["Alice"]);
    stream_chunk_of(&mut s, SPOT);
    let zombie = spawn(&mut s, "minecraft:zombie", Vec3::new(4.5, 64.0, 4.5));
    for (key, value) in every_value() {
        s.act(on_entity(zombie, set(key, value.clone(), false)));
        assert_eq!(
            value_of(&mut s, MetaHolder::Entity(zombie), key, false),
            Some(value)
        );
    }
    assert_eq!(
        entries(&mut s, MetaHolder::Entity(zombie), None, false).len(),
        7
    );
    assert_eq!(
        entries(&mut s, MetaHolder::Entity(zombie), None, true).len(),
        0
    );
    s.act(on_entity(zombie, remove("t:bool", false)));
    assert_eq!(
        value_of(&mut s, MetaHolder::Entity(zombie), "t:bool", false),
        None
    );
    s.act(on_entity(zombie, clear(false)));
    assert!(entries(&mut s, MetaHolder::Entity(zombie), None, false).is_empty());
}

#[test]
fn every_value_type_round_trips_on_a_block_with_no_block_entity() {
    let mut s = scene(&["Alice"]);
    stream_chunk_of(&mut s, SPOT);
    put(&mut s, SPOT, "minecraft:stone", &[]);
    for (key, value) in every_value() {
        s.act(on_block(SPOT, set(key, value.clone(), false)));
        assert_eq!(
            value_of(&mut s, MetaHolder::Block(SPOT), key, false),
            Some(value)
        );
    }
    s.act(on_block(SPOT, remove("t:int", false)));
    assert_eq!(
        value_of(&mut s, MetaHolder::Block(SPOT), "t:int", false),
        None
    );
    s.act(on_block(SPOT, clear(false)));
    assert!(entries(&mut s, MetaHolder::Block(SPOT), None, false).is_empty());
}

#[test]
fn volatile_and_persistent_values_are_separate() {
    let mut s = scene(&["Alice"]);
    stream_chunk_of(&mut s, SPOT);
    put(&mut s, SPOT, "minecraft:stone", &[]);
    s.act(on_block(SPOT, set(TAG, MetaValue::Int(1), false)));
    s.act(on_block(SPOT, set(TAG, MetaValue::Int(2), true)));
    assert_eq!(
        value_of(&mut s, MetaHolder::Block(SPOT), TAG, false),
        Some(MetaValue::Int(1))
    );
    assert_eq!(
        value_of(&mut s, MetaHolder::Block(SPOT), TAG, true),
        Some(MetaValue::Int(2))
    );
}

#[test]
fn limits_give_limit_reached_and_a_bad_key_gives_a_warning() {
    let mut s = scene(&["Alice"]);
    stream_chunk_of(&mut s, SPOT);
    let zombie = spawn(&mut s, "minecraft:zombie", Vec3::new(4.5, 64.0, 4.5));
    for index in 0..64 {
        s.act(on_entity(
            zombie,
            set(&format!("t:k{index}"), MetaValue::Int(1), false),
        ));
    }
    let Answer::Failed(error) = act_result(
        &mut s,
        on_entity(zombie, set("t:extra", MetaValue::Int(1), false)),
    ) else {
        panic!("the 65th key is refused");
    };
    assert_eq!(error.kind, ActionErrorKind::LimitReached);
    let Answer::Failed(error) = act_result(
        &mut s,
        on_entity(
            zombie,
            set("t:k0", MetaValue::Bytes(vec![0; 70_000]), false),
        ),
    ) else {
        panic!("a value over 64 KiB is refused");
    };
    assert_eq!(error.kind, ActionErrorKind::LimitReached);
    let mut deep = MetaValue::Int(0);
    for _ in 0..8 {
        deep = MetaValue::List(vec![deep]);
    }
    let Answer::Failed(error) = act_result(&mut s, on_entity(zombie, set("t:k0", deep, false)))
    else {
        panic!("a value nested 8 levels deep is refused");
    };
    assert_eq!(error.kind, ActionErrorKind::LimitReached);

    s.act(on_entity(
        zombie,
        set("NoNamespace", MetaValue::Int(1), false),
    ));
    assert!(
        s.warnings()
            .iter()
            .any(|warning| warning.contains("meta.set was not applied"))
    );
    let Answer::Failed(error) = act_result(
        &mut s,
        on_entity(zombie, set("Bad Key:x", MetaValue::Int(1), false)),
    ) else {
        panic!("a bad key is refused");
    };
    assert_eq!(error.kind, ActionErrorKind::InvalidArgument);
}

#[test]
fn a_block_loses_its_metadata_when_its_kind_changes_and_keeps_it_on_a_state_change() {
    let mut s = scene(&["Alice"]);
    stream_chunk_of(&mut s, SPOT);
    put(&mut s, SPOT, "minecraft:oak_door", &[("open", "false")]);
    s.act(on_block(SPOT, set(TAG, MetaValue::Int(1), false)));
    s.act(on_block(SPOT, set("t:tmp", MetaValue::Int(1), true)));
    put(&mut s, SPOT, "minecraft:oak_door", &[("open", "true")]);
    assert_eq!(
        value_of(&mut s, MetaHolder::Block(SPOT), TAG, false),
        Some(MetaValue::Int(1))
    );
    put(&mut s, SPOT, "minecraft:stone", &[]);
    assert_eq!(value_of(&mut s, MetaHolder::Block(SPOT), TAG, false), None);
    assert_eq!(
        value_of(&mut s, MetaHolder::Block(SPOT), "t:tmp", true),
        None
    );

    s.act(on_block(SPOT, set(TAG, MetaValue::Int(1), false)));
    put(&mut s, SPOT, "minecraft:air", &[]);
    assert_eq!(value_of(&mut s, MetaHolder::Block(SPOT), TAG, false), None);
}

#[test]
fn a_piston_moves_the_metadata_with_the_pushed_block() {
    let mut s = scene(&["Alice"]);
    stream_chunk_of(&mut s, SPOT);
    let piston = BlockPos::new(3, 64, 3);
    let front = BlockPos::new(4, 64, 3);
    let beyond = BlockPos::new(5, 64, 3);
    put(&mut s, front, "minecraft:stone", &[]);
    s.act(on_block(
        front,
        set(TAG, MetaValue::Text("a".to_owned()), false),
    ));
    s.act(on_block(front, set("t:tmp", MetaValue::Int(9), true)));
    put(
        &mut s,
        piston,
        "minecraft:piston",
        &[("facing", "east"), ("extended", "false")],
    );
    put(
        &mut s,
        BlockPos::new(3, 64, 2),
        "minecraft:redstone_block",
        &[],
    );
    s.tick(6);
    assert_eq!(value_of(&mut s, MetaHolder::Block(front), TAG, false), None);
    assert_eq!(
        value_of(&mut s, MetaHolder::Block(beyond), TAG, false),
        Some(MetaValue::Text("a".to_owned()))
    );
    assert_eq!(
        value_of(&mut s, MetaHolder::Block(beyond), "t:tmp", true),
        Some(MetaValue::Int(9))
    );
}

#[test]
fn an_event_about_a_tagged_block_or_entity_carries_its_meta_and_an_untagged_one_none() {
    let mut s = scene_with(
        &["Alice"],
        vec![
            observe(1, EventKind::EntityDamage),
            observe(2, EventKind::BlockBreak),
        ],
    );
    stream_chunk_of(&mut s, SPOT);
    let tagged = spawn(&mut s, "minecraft:zombie", Vec3::new(4.5, 64.0, 4.5));
    let plain = spawn(&mut s, "minecraft:zombie", Vec3::new(6.5, 64.0, 6.5));
    s.act(on_entity(
        tagged,
        set(TAG, MetaValue::Text("boss".to_owned()), false),
    ));
    s.act(on_entity(tagged, set("t:tmp", MetaValue::Int(1), true)));
    s.act(Order::Entity {
        entity: tagged,
        order: EntityOrder::Damage(1.0),
    });
    s.act(Order::Entity {
        entity: plain,
        order: EntityOrder::Damage(1.0),
    });
    let damaged: Vec<_> = fired(&s.plugin)
        .into_iter()
        .filter_map(|event| match event.payload {
            EventPayload::EntityDamage(damage) => Some(damage.entity),
            _ => None,
        })
        .collect();
    let tagged_view = damaged.iter().find(|view| view.id == tagged).unwrap();
    let plain_view = damaged.iter().find(|view| view.id == plain).unwrap();
    assert_eq!(
        tagged_view.meta.persistent(),
        [(TAG.to_owned(), MetaValue::Text("boss".to_owned()))]
    );
    assert_eq!(
        tagged_view.meta.volatile(),
        [("t:tmp".to_owned(), MetaValue::Int(1))]
    );
    assert!(plain_view.meta.is_empty());

    let alice = s.players[0].clone();
    put(&mut s, SPOT, "minecraft:dirt", &[]);
    put(&mut s, BlockPos::new(3, 63, 2), "minecraft:dirt", &[]);
    s.act(on_block(SPOT, set(TAG, MetaValue::Int(5), false)));
    s.server.handle(Inbound::Input {
        player: alice.id,
        input: rspigot_core::events::PlayerInput::Move {
            position: None,
            rotation: None,
            on_ground: true,
        },
    });
    for (pos, sequence) in [(SPOT, 1), (BlockPos::new(3, 63, 2), 3)] {
        for (action, seq) in [
            (rspigot_core::events::DigAction::Start, sequence),
            (rspigot_core::events::DigAction::Finish, sequence + 1),
        ] {
            if action == rspigot_core::events::DigAction::Finish {
                s.tick(12);
            }
            s.server.handle(Inbound::Input {
                player: alice.id,
                input: rspigot_core::events::PlayerInput::Dig {
                    action,
                    pos,
                    face: rspigot_core::events::Direction::Up,
                    sequence: seq,
                },
            });
            s.server.deliver();
        }
    }
    let carried: Vec<_> = s
        .plugin
        .received()
        .into_iter()
        .filter_map(|message| match message {
            ToPlugin::Event {
                payload: EventPayload::BlockBreak(broken),
                blocks,
                ..
            } => Some((broken.block, blocks)),
            _ => None,
        })
        .collect();
    let (_, with) = carried
        .iter()
        .find(|(pos, _)| *pos == SPOT)
        .expect("the tagged block broke");
    assert_eq!(with.len(), 1);
    assert_eq!(with[0].pos, SPOT);
    assert_eq!(
        with[0].meta.persistent(),
        [(TAG.to_owned(), MetaValue::Int(5))]
    );
    let (_, without) = carried
        .iter()
        .find(|(pos, _)| *pos == BlockPos::new(3, 63, 2))
        .expect("the plain block broke");
    assert!(without.is_empty());
}

#[test]
fn metadata_can_be_searched() {
    let mut s = scene(&["Alice"]);
    stream_chunk_of(&mut s, SPOT);
    let a = BlockPos::new(2, 64, 2);
    let b = BlockPos::new(3, 64, 2);
    let c = BlockPos::new(4, 64, 2);
    for pos in [a, b, c] {
        put(&mut s, pos, "minecraft:stone", &[]);
    }
    s.act(on_block(a, set(TAG, MetaValue::Int(1), false)));
    s.act(on_block(b, set(TAG, MetaValue::Int(2), false)));
    s.act(on_block(c, set("t:other", MetaValue::Int(1), false)));
    s.act(on_block(c, set(TAG, MetaValue::Int(1), true)));
    let find = |s: &mut Scene, value: Option<MetaValue>, volatile: bool| match s.ask(
        Question::FindBlocks {
            key: TAG.to_owned(),
            value,
            volatile,
        },
    ) {
        Answer::BlockPositions(found) => found,
        other => panic!("expected positions, got {other:?}"),
    };
    assert_eq!(find(&mut s, None, false), [a, b]);
    assert_eq!(find(&mut s, Some(MetaValue::Int(2)), false), [b]);
    assert_eq!(find(&mut s, None, true), [c]);

    let zombie = spawn(&mut s, "minecraft:zombie", Vec3::new(4.5, 64.0, 4.5));
    let _other = spawn(&mut s, "minecraft:zombie", Vec3::new(6.5, 64.0, 6.5));
    s.act(on_entity(zombie, set(TAG, MetaValue::Bool(true), false)));
    let Answer::Entities(found) = s.ask(Question::EntitiesWithMeta {
        key: TAG.to_owned(),
        volatile: false,
    }) else {
        panic!("expected entities");
    };
    assert_eq!(
        found.iter().map(|view| view.id).collect::<Vec<_>>(),
        [zombie]
    );
}

#[derive(Default)]
struct Memory {
    chunks: Mutex<FxHashMap<ChunkPos, ChunkColumn>>,
    entities: Mutex<FxHashMap<ChunkPos, Vec<Compound>>>,
}

impl ChunkStore for Memory {
    fn load(&self, _: &Identifier, pos: ChunkPos) -> Result<Option<ChunkColumn>, StoreError> {
        Ok(self.chunks.lock().unwrap().get(&pos).cloned())
    }

    fn save(&self, _: &Identifier, chunk: &ChunkColumn) -> Result<(), StoreError> {
        self.chunks
            .lock()
            .unwrap()
            .insert(chunk.pos(), chunk.clone());
        Ok(())
    }

    fn flush(&self) -> Result<(), StoreError> {
        Ok(())
    }
}

impl EntityStore for Memory {
    fn load_entities(
        &self,
        _: &Identifier,
        pos: ChunkPos,
    ) -> Result<Option<Vec<Compound>>, StoreError> {
        Ok(self.entities.lock().unwrap().get(&pos).cloned())
    }

    fn save_entities(
        &self,
        _: &Identifier,
        pos: ChunkPos,
        entities: &[Compound],
    ) -> Result<(), StoreError> {
        let mut map = self.entities.lock().unwrap();
        if entities.is_empty() {
            map.remove(&pos);
        } else {
            map.insert(pos, entities.to_vec());
        }
        Ok(())
    }
}

fn persistent_scene(store: &Arc<Memory>) -> Scene {
    let config = GameConfig {
        view_distance: 3,
        chunk_workers: 2,
        chunks: ChunkManagerConfig {
            unload_delay_ticks: 5,
            autosave_interval_ticks: 0,
        },
        admin: AdminLists {
            operators: vec![ListEntry::named("Alice")],
            ..AdminLists::default()
        },
        ..GameConfig::default()
    };
    let level = LevelData {
        spawn_mobs: false,
        ..LevelData::default()
    };
    let mut server = Server::new(
        config,
        level,
        Arc::clone(store),
        Arc::new(Floor),
        StatusBoard::new(20),
    );
    server.wait_for_spawn(Duration::from_secs(10));
    scene_on(server, &["Alice"], Vec::new())
}

fn pump(s: &mut Scene, ticks: u32) {
    let alice = s.players[0].clone();
    for _ in 0..ticks {
        s.server.wait_for_spawn(Duration::from_millis(20));
        s.tick(1);
        for event in alice.link.take() {
            let input = match event {
                ClientEvent::ChunkBatchFinished { .. } => PlayerInput::ChunkBatchReceived {
                    chunks_per_tick: 64.0,
                },
                ClientEvent::Teleport { id, .. } => PlayerInput::TeleportConfirm(id),
                _ => continue,
            };
            s.server.handle(Inbound::Input {
                player: alice.id,
                input,
            });
        }
    }
}

fn go_far(s: &mut Scene) {
    s.server
        .handle(Inbound::Console("tp Alice 100 64 104".to_owned()));
    pump(s, 40);
}

fn away_and_back(s: &mut Scene) {
    s.server
        .handle(Inbound::Console("tp Alice 2000 64 2000".to_owned()));
    pump(s, 60);
    s.server.wait_for_entity_saves();
    s.server
        .handle(Inbound::Console("tp Alice 100 64 104".to_owned()));
    pump(s, 60);
}

#[test]
fn entity_and_block_metadata_survive_a_chunk_unload_and_volatile_does_not() {
    let store = Arc::new(Memory::default());
    let mut s = persistent_scene(&store);
    go_far(&mut s);
    let zombie = spawn(&mut s, "minecraft:zombie", Vec3::new(100.5, 64.0, 100.5));
    s.act(on_entity(
        zombie,
        set("marker:kind", MetaValue::Text("boss".to_owned()), false),
    ));
    s.act(on_entity(zombie, set("t:tmp", MetaValue::Int(1), true)));
    put(&mut s, FAR, "minecraft:stone", &[]);
    s.act(on_block(FAR, set("marker:kind", MetaValue::Int(7), false)));
    s.act(on_block(FAR, set("t:tmp", MetaValue::Int(1), true)));

    away_and_back(&mut s);

    let Answer::Entities(found) = s.ask(Question::EntitiesWithMeta {
        key: "marker:kind".to_owned(),
        volatile: false,
    }) else {
        panic!("expected entities");
    };
    assert_eq!(found.len(), 1);
    let back = found[0].id;
    assert_eq!(
        value_of(&mut s, MetaHolder::Entity(back), "marker:kind", false),
        Some(MetaValue::Text("boss".to_owned()))
    );
    assert_eq!(
        value_of(&mut s, MetaHolder::Entity(back), "t:tmp", true),
        None
    );
    assert_eq!(
        value_of(&mut s, MetaHolder::Block(FAR), "marker:kind", false),
        Some(MetaValue::Int(7))
    );
    assert_eq!(
        value_of(&mut s, MetaHolder::Block(FAR), "t:tmp", true),
        None
    );

    let saved = store.chunks.lock().unwrap();
    let chunk = saved.get(&FAR.chunk()).expect("the chunk was saved");
    assert!(chunk.block_meta.persistent(FAR).is_some());
    drop(saved);
    let entities = store.entities.lock().unwrap();
    let written = entities
        .values()
        .flatten()
        .find_map(|saved| saved.get_compound("BukkitValues"))
        .expect("the entity wrote its BukkitValues");
    assert!(written.contains_key("marker:kind"));
    assert!(!written.contains_key("t:tmp"));
}

#[test]
fn a_spigot_pdc_value_in_bukkit_values_is_readable_and_survives_a_rewrite() {
    let store = Arc::new(Memory::default());
    let chunk = Vec3::new(100.5, 64.0, 100.5).chunk();
    let spigot = Compound::new()
        .with("id", "minecraft:zombie")
        .with(
            "Pos",
            Tag::List(vec![
                Tag::Double(100.5),
                Tag::Double(64.0),
                Tag::Double(100.5),
            ]),
        )
        .with("UUID", Tag::IntArray(vec![1, 2, 3, 4]))
        .with(
            "BukkitValues",
            Compound::new()
                .with("plugin:count", Tag::Int(7))
                .with("plugin:ints", Tag::IntArray(vec![1, 2])),
        );
    store.entities.lock().unwrap().insert(chunk, vec![spigot]);
    let mut s = persistent_scene(&store);
    go_far(&mut s);
    let Answer::Entities(zombies) = s.ask(Question::Entities {
        near: None,
        kind: Some("minecraft:zombie".to_owned()),
    }) else {
        panic!("expected entities");
    };
    assert_eq!(zombies.len(), 1);
    let id = zombies[0].id;
    assert_eq!(
        value_of(&mut s, MetaHolder::Entity(id), "plugin:count", false),
        Some(MetaValue::Int(7))
    );
    s.act(on_entity(
        id,
        set("marker:kind", MetaValue::Bool(true), false),
    ));

    away_and_back(&mut s);

    let entities = store.entities.lock().unwrap();
    let written = entities
        .values()
        .flatten()
        .find_map(|saved| saved.get_compound("BukkitValues"))
        .expect("BukkitValues was written back");
    assert_eq!(written.get("plugin:count"), Some(&Tag::Int(7)));
    assert_eq!(written.get("plugin:ints"), Some(&Tag::IntArray(vec![1, 2])));
    assert!(written.contains_key("marker:kind"));
}

fn on_player(player: &Player, order: MetaOrder) -> Order {
    to(player, PlayerOrder::Meta(order))
}

#[test]
fn every_value_type_round_trips_on_a_player_and_limits_and_keys_are_checked() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    let holder = MetaHolder::Player(alice.id);
    for (key, value) in every_value() {
        s.act(on_player(&alice, set(key, value.clone(), false)));
        assert_eq!(value_of(&mut s, holder.clone(), key, false), Some(value));
    }
    s.act(on_player(&alice, set("t:tmp", MetaValue::Int(1), true)));
    assert_eq!(
        value_of(&mut s, holder.clone(), "t:tmp", true),
        Some(MetaValue::Int(1))
    );
    assert_eq!(value_of(&mut s, holder.clone(), "t:tmp", false), None);
    s.act(on_player(&alice, remove("t:bool", false)));
    assert_eq!(value_of(&mut s, holder.clone(), "t:bool", false), None);
    s.act(on_player(&alice, clear(false)));
    assert!(entries(&mut s, holder.clone(), None, false).is_empty());

    for index in 0..64 {
        s.act(on_player(
            &alice,
            set(&format!("t:k{index}"), MetaValue::Int(1), false),
        ));
    }
    let Answer::Failed(error) = act_result(
        &mut s,
        on_player(&alice, set("t:extra", MetaValue::Int(1), false)),
    ) else {
        panic!("the 65th key is refused");
    };
    assert_eq!(error.kind, ActionErrorKind::LimitReached);
    s.act(on_player(
        &alice,
        set("NoNamespace", MetaValue::Int(1), false),
    ));
    assert!(
        s.warnings()
            .iter()
            .any(|warning| warning.contains("meta.set was not applied"))
    );
    let Answer::Failed(error) = act_result(
        &mut s,
        on_player(&alice, set("Bad Key:x", MetaValue::Int(1), false)),
    ) else {
        panic!("a bad key is refused");
    };
    assert_eq!(error.kind, ActionErrorKind::InvalidArgument);
}

#[test]
fn player_metadata_persists_across_a_leave_and_rejoin_and_volatile_does_not() {
    let (mut s, saves) = saving_scene(&["Alice"], Vec::new());
    let alice = s.players[0].clone();
    s.act(on_player(
        &alice,
        set(TAG, MetaValue::Text("vip".to_owned()), false),
    ));
    s.act(on_player(&alice, set("t:tmp", MetaValue::Int(1), true)));

    s.server.handle(leave_of(&alice));
    s.server.deliver();
    s.tick(2);
    let mut saved = None;
    for _ in 0..100 {
        saved = rspigot_core::ports::PlayerStore::load_player(&*saves, alice.id).unwrap();
        if saved.is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let written = saved
        .as_ref()
        .and_then(|data| data.get_compound("BukkitValues"))
        .expect("the player data holds BukkitValues");
    assert!(written.contains_key(TAG));
    assert!(!written.contains_key("t:tmp"));

    let back = join(&mut s.server, "Alice");
    s.server.deliver();
    let holder = MetaHolder::Player(back.id);
    assert_eq!(
        value_of(&mut s, holder.clone(), TAG, false),
        Some(MetaValue::Text("vip".to_owned()))
    );
    assert_eq!(value_of(&mut s, holder, "t:tmp", true), None);
    let Answer::Players(found) = s.ask(Question::PlayersWithMeta {
        key: TAG.to_owned(),
        volatile: false,
    }) else {
        panic!("expected players");
    };
    assert_eq!(found.len(), 1);
}

#[test]
fn an_event_about_a_tagged_player_carries_its_meta() {
    let mut s = scene_with(&["Alice"], vec![observe(1, EventKind::PlayerMove)]);
    let alice = s.players[0].clone();
    s.act(on_player(
        &alice,
        set(TAG, MetaValue::Text("vip".to_owned()), false),
    ));
    s.act(on_player(&alice, set("t:tmp", MetaValue::Int(2), true)));
    send_input(
        &mut s.server,
        &alice,
        rspigot_core::events::PlayerInput::Move {
            position: Some(Vec3::new(3.5, 64.0, 3.5)),
            rotation: None,
            on_ground: true,
        },
    );
    s.server.deliver();
    let moved: Vec<_> = fired(&s.plugin)
        .into_iter()
        .filter_map(|event| match event.payload {
            EventPayload::PlayerMove(moved) => Some(moved.player),
            _ => None,
        })
        .collect();
    let player = moved.last().expect("the move was heard");
    assert_eq!(
        player.meta.persistent(),
        [(TAG.to_owned(), MetaValue::Text("vip".to_owned()))]
    );
    assert_eq!(
        player.meta.volatile(),
        [("t:tmp".to_owned(), MetaValue::Int(2))]
    );
}

#[test]
fn a_falling_block_carries_its_metadata_and_gives_it_back_when_it_lands() {
    let mut s = scene(&["Alice"]);
    stream_chunk_of(&mut s, SPOT);
    let high = BlockPos::new(8, 70, 8);
    let ground = BlockPos::new(8, 64, 8);
    put(&mut s, high, "minecraft:sand", &[]);
    s.act(on_block(high, set(TAG, MetaValue::Int(3), false)));
    s.act(on_block(high, set("t:tmp", MetaValue::Int(4), true)));
    s.tick(60);
    assert_eq!(value_of(&mut s, MetaHolder::Block(high), TAG, false), None);
    assert_eq!(
        value_of(&mut s, MetaHolder::Block(ground), TAG, false),
        Some(MetaValue::Int(3))
    );
    assert_eq!(
        value_of(&mut s, MetaHolder::Block(ground), "t:tmp", true),
        Some(MetaValue::Int(4))
    );
}

#[test]
fn a_sticky_piston_pulls_the_metadata_back_with_the_block() {
    let mut s = scene(&["Alice"]);
    stream_chunk_of(&mut s, SPOT);
    let piston = BlockPos::new(3, 64, 3);
    let front = BlockPos::new(4, 64, 3);
    let beyond = BlockPos::new(5, 64, 3);
    let power = BlockPos::new(3, 64, 2);
    put(&mut s, front, "minecraft:stone", &[]);
    s.act(on_block(front, set(TAG, MetaValue::Int(8), false)));
    put(
        &mut s,
        piston,
        "minecraft:sticky_piston",
        &[("facing", "east"), ("extended", "false")],
    );
    put(&mut s, power, "minecraft:redstone_block", &[]);
    s.tick(5);
    assert_eq!(
        value_of(&mut s, MetaHolder::Block(beyond), TAG, false),
        Some(MetaValue::Int(8))
    );
    put(&mut s, power, "minecraft:air", &[]);
    s.tick(6);
    assert_eq!(
        value_of(&mut s, MetaHolder::Block(front), TAG, false),
        Some(MetaValue::Int(8))
    );
    assert_eq!(
        value_of(&mut s, MetaHolder::Block(beyond), TAG, false),
        None
    );
}
