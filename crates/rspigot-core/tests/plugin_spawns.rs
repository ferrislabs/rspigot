mod common;

use std::sync::Arc;
use std::time::Duration;

use common::*;
use rspigot_blocks::{Block, GrassBlock, Stone};
use rspigot_core::config::{GameConfig, LevelData, PluginSettings, SpawningConfig};
use rspigot_core::events::{ClientEvent, Inbound, PlayerInput};
use rspigot_core::plugins::events::{CreatureSpawnReason, EntityRemoveCause};
use rspigot_core::plugins::{
    ActionError, ActionErrorKind, Answer, EntityOrder, EntityView, EventKind, EventPayload,
    FromPlugin, ItemSpec, Order, Phase, Question, ToPlugin,
};
use rspigot_core::ports::ChunkGenerator;
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::spawn::state::{CategorySettings, SpawnSettings};
use rspigot_domain::world::Dimension;
use rspigot_domain::{BiomeId, BlockPos, ChunkPos, EntityId, Vec3};

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn spawn(s: &mut Scene, kind: &str, position: Vec3) -> EntityView {
    let Answer::Spawned(view) = s.ask(Question::Spawn {
        kind: kind.into(),
        position,
    }) else {
        panic!("expected the spawned {kind}");
    };
    view
}

fn drop_item(s: &mut Scene, position: Vec3) -> EntityView {
    let Answer::Spawned(view) = s.ask(Question::DropItem {
        position,
        item: ItemSpec::of("minecraft:stick", 1),
    }) else {
        panic!("expected the dropped stick");
    };
    view
}

fn question(s: &mut Scene, request: u64, question: Question) {
    s.server.handle(Inbound::PluginMessage {
        connection: 1,
        message: Box::new(FromPlugin::Question { request, question }),
    });
    s.server.deliver();
}

fn answer(s: &Scene, request: u64) -> Option<Answer> {
    s.plugin
        .received()
        .into_iter()
        .find_map(|message| match message {
            ToPlugin::Answer {
                request: answered,
                answer,
            } if answered == request => Some(answer),
            _ => None,
        })
}

fn exists(s: &mut Scene, id: EntityId) -> bool {
    !matches!(s.ask(Question::Entity(id)), Answer::NotFound)
}

fn spawned_to(player: &Player, id: EntityId) -> bool {
    player
        .link
        .take()
        .iter()
        .any(|event| matches!(event, ClientEvent::SpawnEntity { entity, .. } if *entity == id))
}

fn reasons(s: &Scene) -> Vec<(Phase, CreatureSpawnReason)> {
    fired(&s.plugin)
        .into_iter()
        .filter_map(|event| match event.payload {
            EventPayload::CreatureSpawn(spawn) => Some((event.phase, spawn.spawn_reason)),
            _ => None,
        })
        .collect()
}

fn removals(s: &Scene) -> Vec<(EntityId, EntityRemoveCause)> {
    fired(&s.plugin)
        .into_iter()
        .filter_map(|event| match event.payload {
            EventPayload::EntityRemove(removed) => Some((removed.entity.id, removed.cause)),
            _ => None,
        })
        .collect()
}

#[test]
fn a_summoned_creature_reaches_the_entity_spawn_and_creature_spawn_handlers() {
    let mut s = scene_with(
        &["Alice"],
        vec![
            observe(1, EventKind::EntitySpawn),
            observe(2, EventKind::CreatureSpawn),
            observe(3, EventKind::ItemSpawn),
        ],
    );
    console(&mut s, "summon minecraft:cow 2.5 64 2.5");
    let cow = spawn(&mut s, "minecraft:cow", Vec3::new(4.5, 64.0, 4.5));
    assert_eq!(
        reasons(&s),
        [
            (Phase::Settled, CreatureSpawnReason::Command),
            (Phase::Settled, CreatureSpawnReason::Custom),
        ]
    );
    let heard = s
        .plugin
        .received()
        .into_iter()
        .filter_map(|message| match message {
            rspigot_core::plugins::ToPlugin::Event {
                handlers, payload, ..
            } => Some((payload.kind(), handlers)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        heard,
        [
            (EventKind::CreatureSpawn, vec![1, 2]),
            (EventKind::CreatureSpawn, vec![1, 2]),
        ]
    );
    let EventPayload::CreatureSpawn(spawn) =
        &settled_for(&s.plugin, EventKind::CreatureSpawn)[1].payload
    else {
        panic!("a creature spawn");
    };
    assert_eq!(spawn.entity, cow);
    assert_eq!(spawn.entity_type, "minecraft:cow");
    assert_eq!(spawn.location.position, cow.position);
    assert!(exists(&mut s, cow.id));
    assert!(s.warnings().is_empty());
}

#[test]
fn a_cancelled_creature_spawn_never_enters_the_world_nor_fires_a_removal() {
    let mut s = scene_with(
        &["Alice"],
        vec![
            deciding(1, EventKind::CreatureSpawn),
            observe(2, EventKind::EntityRemove),
            observe(3, EventKind::CreatureSpawn),
        ],
    );
    let alice = s.players[0].clone();
    let at = Vec3::new(4.5, 64.0, 4.5);
    stream_chunk_of(&mut s, at.block());
    question(
        &mut s,
        9_001,
        Question::Spawn {
            kind: "minecraft:zombie".into(),
            position: at,
        },
    );
    assert_eq!(answer(&s, 9_001), None);
    let asked = last_asked(&s.plugin, EventKind::CreatureSpawn);
    let EventPayload::CreatureSpawn(spawn) = &asked.payload else {
        panic!("a creature spawn");
    };
    let zombie = spawn.entity.id;
    assert!(!exists(&mut s, zombie));
    rule_on(&mut s.server, 1, asked.id, true, None);
    s.tick(2);
    assert!(matches!(
        answer(&s, 9_001),
        Some(Answer::Failed(ActionError {
            kind: ActionErrorKind::Cancelled,
            ..
        }))
    ));
    assert!(!exists(&mut s, zombie));
    assert!(!spawned_to(&alice, zombie));
    assert!(removals(&s).is_empty());
    let settled = settled_for(&s.plugin, EventKind::CreatureSpawn);
    assert_eq!(settled.len(), 1);
    assert!(settled[0].cancelled);

    question(
        &mut s,
        9_002,
        Question::Spawn {
            kind: "minecraft:cow".into(),
            position: at,
        },
    );
    assert_eq!(answer(&s, 9_002), None);
    let asked = last_asked(&s.plugin, EventKind::CreatureSpawn);
    rule_on(&mut s.server, 1, asked.id, false, None);
    s.tick(1);
    let Some(Answer::Spawned(cow)) = answer(&s, 9_002) else {
        panic!("expected the spawned cow");
    };
    assert!(exists(&mut s, cow.id));
    assert!(spawned_to(&alice, cow.id));
}

#[test]
fn an_entity_spawn_decider_decides_the_items_of_its_family() {
    let mut s = scene_with(&["Alice"], vec![deciding(1, EventKind::EntitySpawn)]);
    let at = Vec3::new(2.5, 64.0, 2.5);
    let stick = || Question::DropItem {
        position: at,
        item: ItemSpec::of("minecraft:stick", 1),
    };
    question(&mut s, 9_003, stick());
    assert_eq!(answer(&s, 9_003), None);
    let asked = last_asked(&s.plugin, EventKind::ItemSpawn);
    let EventPayload::ItemSpawn(spawn) = &asked.payload else {
        panic!("an item spawn");
    };
    let dropped = spawn.entity.id;
    assert!(!exists(&mut s, dropped));
    rule_on(&mut s.server, 1, asked.id, true, None);
    assert!(matches!(
        answer(&s, 9_003),
        Some(Answer::Failed(ActionError {
            kind: ActionErrorKind::Cancelled,
            ..
        }))
    ));
    assert!(!exists(&mut s, dropped));
    question(&mut s, 9_004, stick());
    let asked = last_asked(&s.plugin, EventKind::ItemSpawn);
    rule_on(&mut s.server, 1, asked.id, false, None);
    let Some(Answer::Spawned(kept)) = answer(&s, 9_004) else {
        panic!("expected the dropped stick");
    };
    assert!(exists(&mut s, kept.id));
}

#[test]
fn a_summon_waits_for_its_spawn_to_reply() {
    let mut s = scene_with(&["Alice"], vec![deciding(1, EventKind::CreatureSpawn)]);
    let alice = s.players[0].clone();
    console(&mut s, "op Alice");
    alice.link.take();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Command("summon minecraft:cow 2.5 64 2.5".to_owned()),
    );
    assert!(system_texts(&alice).is_empty());
    let asked = last_asked(&s.plugin, EventKind::CreatureSpawn);
    rule_on(&mut s.server, 1, asked.id, true, None);
    assert_eq!(
        system_texts(&alice),
        ["Unable to summon entity due to duplicate UUIDs"]
    );
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Command("summon minecraft:cow 2.5 64 2.5".to_owned()),
    );
    let asked = last_asked(&s.plugin, EventKind::CreatureSpawn);
    rule_on(&mut s.server, 1, asked.id, false, None);
    assert_eq!(
        system_texts(&alice),
        ["commands.summon.success entity.minecraft.cow"]
    );
}

#[test]
fn every_removal_carries_its_bukkit_cause() {
    let mut s = scene_with(&["Alice"], vec![observe(1, EventKind::EntityRemove)]);
    let at = Vec3::new(4.5, 64.0, 4.5);
    stream_chunk_of(&mut s, at.block());
    let cow = spawn(&mut s, "minecraft:cow", at);
    s.act(Order::Entity {
        entity: cow.id,
        order: EntityOrder::Remove,
    });
    let far = Vec3::new(9.5, 64.0, 9.5);
    let stick = drop_item(&mut s, far);
    s.act(Order::Entity {
        entity: stick.id,
        order: EntityOrder::Kill,
    });
    let near = drop_item(&mut s, Vec3::new(0.5, 64.0, 0.5));
    s.tick(20);
    assert_eq!(
        removals(&s),
        [
            (cow.id, EntityRemoveCause::Plugin),
            (stick.id, EntityRemoveCause::Death),
            (near.id, EntityRemoveCause::Pickup),
        ]
    );
    assert!(s.warnings().is_empty());
}

struct Meadow;

impl ChunkGenerator for Meadow {
    fn generate(&self, dimension: &Dimension, pos: ChunkPos) -> ChunkColumn {
        let plains = BiomeId(rspigot_data::registries::biome_id("minecraft:plains").unwrap_or(0));
        let mut chunk = ChunkColumn::empty(pos, dimension.height, plains);
        for x in 0..16 {
            for z in 0..16 {
                let (x, z) = (pos.min_block_x() + x, pos.min_block_z() + z);
                for y in 60..63 {
                    chunk.set_block(BlockPos::new(x, y, z), Stone::DEFAULT_STATE);
                }
                chunk.set_block(BlockPos::new(x, 63, z), GrassBlock::DEFAULT_STATE);
            }
        }
        chunk
    }

    fn spawn_point(&self, _: &Dimension) -> Vec3 {
        Vec3::new(8.5, 64.0, 8.5)
    }
}

fn meadow(handlers: Vec<rspigot_core::plugins::HandlerSpec>) -> Scene {
    let every_tick = |limit| CategorySettings {
        limit,
        ticks_per: 1,
    };
    let config = GameConfig {
        view_distance: 3,
        simulation_distance: 3,
        chunk_workers: 2,
        spawn_protection: 0,
        plugins: PluginSettings {
            decision_ticks: 10_000,
            ..PluginSettings::default()
        },
        spawning: SpawningConfig {
            categories: SpawnSettings {
                monster: every_tick(1),
                creature: every_tick(0),
                ambient: every_tick(0),
                axolotls: every_tick(0),
                underground_water_creature: every_tick(0),
                water_creature: every_tick(0),
                water_ambient: every_tick(0),
            },
            ..SpawningConfig::default()
        },
        ..GameConfig::default()
    };
    let level = LevelData {
        day_time: 18000,
        daylight_cycle: false,
        ..LevelData::default()
    };
    let mut server: Server<Link> = Server::new(
        config,
        level,
        Arc::new(NoStore),
        Arc::new(Meadow),
        StatusBoard::new(20),
    );
    server.wait_for_spawn(Duration::from_secs(10));
    let plugin = connect_with(&mut server, 1, "tester", handlers);
    let alice = join(&mut server, "Alice");
    Scene {
        server,
        plugin,
        players: vec![alice],
    }
}

fn natural_asks(s: &Scene) -> Vec<u64> {
    asked_for(&s.plugin, EventKind::CreatureSpawn)
        .into_iter()
        .filter(|event| {
            matches!(&event.payload, EventPayload::CreatureSpawn(spawn)
                if spawn.spawn_reason == CreatureSpawnReason::Natural)
        })
        .map(|event| event.id)
        .collect()
}

fn tick_until_asked(s: &mut Scene, count: usize) {
    for _ in 0..1200 {
        if natural_asks(s).len() >= count {
            return;
        }
        s.server.wait_for_spawn(Duration::from_millis(5));
        s.tick(1);
        for event in s.players[0].link.take() {
            if let ClientEvent::ChunkBatchFinished { .. } = event {
                send_input(
                    &mut s.server,
                    &s.players[0].clone(),
                    PlayerInput::ChunkBatchReceived {
                        chunks_per_tick: 64.0,
                    },
                );
            }
        }
    }
    panic!("the spawner asked {count} natural spawns");
}

#[test]
fn a_waiting_natural_spawn_holds_its_slot_and_a_cancelled_one_frees_it() {
    let mut s = meadow(vec![deciding(1, EventKind::CreatureSpawn)]);
    tick_until_asked(&mut s, 1);
    let pack = natural_asks(&s);
    s.tick(40);
    assert_eq!(
        natural_asks(&s),
        pack,
        "the waiting pack holds the only slot"
    );
    for event in &pack {
        rule_on(&mut s.server, 1, *event, true, None);
    }
    tick_until_asked(&mut s, pack.len() + 1);
    let second = natural_asks(&s)[pack.len()..].to_vec();
    for event in &second {
        rule_on(&mut s.server, 1, *event, false, None);
    }
    s.tick(40);
    assert_eq!(
        natural_asks(&s).len(),
        pack.len() + second.len(),
        "the spawned creatures fill the slot"
    );
}

fn drowning_zombie() -> (Scene, EntityId, Fired) {
    let mut server = server(PluginSettings {
        decision_ticks: 1_000,
        ..PluginSettings::default()
    });
    let plugin = connect_with(
        &mut server,
        1,
        "tester",
        vec![
            deciding(1, EventKind::CreatureSpawn),
            observe(2, EventKind::EntityRemove),
        ],
    );
    let alice = join(&mut server, "Alice");
    let mut s = Scene {
        server,
        plugin,
        players: vec![alice],
    };
    let water = rspigot_domain::BlockPos::new(4, 64, 4);
    stream_chunk_of(&mut s, water);
    for y in 64..=66 {
        s.act(Order::Block {
            pos: rspigot_domain::BlockPos::new(4, y, 4),
            order: rspigot_core::plugins::BlockOrder::Set("minecraft:water".into()),
        });
    }
    console(&mut s, "time set 18000");
    console(&mut s, "gamemode creative Alice");
    console(
        &mut s,
        "summon minecraft:zombie 4.5 64 4.5 {NoAI:0b,DrownedConversionTime:2}",
    );
    let summoned = last_asked(&s.plugin, EventKind::CreatureSpawn);
    rule_on(&mut s.server, 1, summoned.id, false, None);
    let zombie = s.server.entities_of(rspigot_entities::EntityKind::Zombie)[0].id;
    for _ in 0..40 {
        s.tick(1);
        if reasons(&s).contains(&(Phase::Decide, CreatureSpawnReason::Drowned)) {
            break;
        }
    }
    let asked = asked_for(&s.plugin, EventKind::CreatureSpawn)
        .into_iter()
        .find(|event| {
            matches!(&event.payload, EventPayload::CreatureSpawn(spawn)
                if spawn.spawn_reason == CreatureSpawnReason::Drowned)
        })
        .expect("the conversion asks");
    (s, zombie, asked)
}

#[test]
fn a_cancelled_conversion_still_removes_the_old_mob_with_transformation() {
    let (mut s, zombie, asked) = drowning_zombie();
    let EventPayload::CreatureSpawn(spawn) = &asked.payload else {
        panic!("a creature spawn");
    };
    assert_eq!(spawn.entity_type, "minecraft:drowned");
    assert!(
        exists(&mut s, zombie),
        "the zombie waits for its conversion"
    );
    s.tick(5);
    assert_eq!(
        reasons(&s)
            .iter()
            .filter(|reason| **reason == (Phase::Decide, CreatureSpawnReason::Drowned))
            .count(),
        1
    );
    rule_on(&mut s.server, 1, asked.id, true, None);
    assert!(!exists(&mut s, zombie));
    assert!(
        s.server
            .entities_of(rspigot_entities::EntityKind::Drowned)
            .is_empty()
    );
    assert_eq!(removals(&s), [(zombie, EntityRemoveCause::Transformation)]);
}

#[test]
fn a_conversion_whose_old_mob_died_meanwhile_brings_no_new_mob() {
    let (mut s, zombie, asked) = drowning_zombie();
    s.act(Order::Entity {
        entity: zombie,
        order: EntityOrder::Kill,
    });
    rule_on(&mut s.server, 1, asked.id, false, None);
    assert!(
        s.server
            .entities_of(rspigot_entities::EntityKind::Drowned)
            .is_empty()
    );
    assert!(
        !removals(&s)
            .iter()
            .any(|(_, cause)| *cause == EntityRemoveCause::Transformation)
    );
    s.tick(25);
    assert_eq!(removals(&s), [(zombie, EntityRemoveCause::Death)]);
}

fn held_count(s: &mut Scene, key: &str) -> i32 {
    let Answer::Inventory(state) = s.ask(Question::Inventory(s.players[0].id)) else {
        panic!("Alice's inventory");
    };
    state
        .slots
        .iter()
        .flatten()
        .filter(|item| item.key == key)
        .map(|item| item.count)
        .sum()
}

fn eggs(s: &mut Scene) -> i32 {
    held_count(s, "minecraft:cow_spawn_egg")
}

#[test]
fn a_spawn_egg_is_spent_only_when_its_baby_enters_the_world() {
    let mut s = scene_with(&["Alice"], vec![deciding(1, EventKind::CreatureSpawn)]);
    let alice = s.players[0].clone();
    console(&mut s, "give Alice minecraft:cow_spawn_egg 2");
    question(
        &mut s,
        9_005,
        Question::Spawn {
            kind: "minecraft:cow".into(),
            position: Vec3::new(1.5, 64.0, 1.5),
        },
    );
    let asked = last_asked(&s.plugin, EventKind::CreatureSpawn);
    rule_on(&mut s.server, 1, asked.id, false, None);
    let Some(Answer::Spawned(cow)) = answer(&s, 9_005) else {
        panic!("expected the spawned cow");
    };
    let use_egg = |s: &mut Scene| {
        send_input(
            &mut s.server,
            &alice,
            PlayerInput::Interact {
                target: cow.id,
                hand: rspigot_core::events::Hand::Main,
                sneaking: false,
                location: Vec3::new(0.0, 0.5, 0.0),
            },
        );
        last_asked(&s.plugin, EventKind::CreatureSpawn)
    };
    let egg = use_egg(&mut s);
    assert_eq!(
        reasons(&s).last(),
        Some(&(Phase::Decide, CreatureSpawnReason::SpawnerEgg))
    );
    assert_eq!(eggs(&mut s), 2);
    rule_on(&mut s.server, 1, egg.id, true, None);
    assert_eq!(eggs(&mut s), 2);
    let egg = use_egg(&mut s);
    rule_on(&mut s.server, 1, egg.id, false, None);
    assert_eq!(eggs(&mut s), 1);
    assert_eq!(
        s.server
            .entities_of(rspigot_entities::EntityKind::Cow)
            .len(),
        2
    );
}

#[test]
fn a_spawn_egg_moved_out_of_its_slot_while_its_baby_waits_is_not_spent() {
    let mut s = scene_with(&["Alice"], vec![deciding(1, EventKind::CreatureSpawn)]);
    let alice = s.players[0].clone();
    console(&mut s, "give Alice minecraft:cow_spawn_egg 1");
    question(
        &mut s,
        9_006,
        Question::Spawn {
            kind: "minecraft:cow".into(),
            position: Vec3::new(1.5, 64.0, 1.5),
        },
    );
    let asked = last_asked(&s.plugin, EventKind::CreatureSpawn);
    rule_on(&mut s.server, 1, asked.id, false, None);
    let Some(Answer::Spawned(cow)) = answer(&s, 9_006) else {
        panic!("expected the spawned cow");
    };
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Interact {
            target: cow.id,
            hand: rspigot_core::events::Hand::Main,
            sneaking: false,
            location: Vec3::new(0.0, 0.5, 0.0),
        },
    );
    let egg = last_asked(&s.plugin, EventKind::CreatureSpawn);
    assert_ne!(egg.id, asked.id);
    console(&mut s, "clear Alice minecraft:cow_spawn_egg");
    console(&mut s, "give Alice minecraft:stick 1");
    console(&mut s, "give Alice minecraft:cow_spawn_egg 1");
    rule_on(&mut s.server, 1, egg.id, false, None);
    assert_eq!(held_count(&mut s, "minecraft:stick"), 1);
    assert_eq!(eggs(&mut s), 1);
    assert_eq!(
        s.server
            .entities_of(rspigot_entities::EntityKind::Cow)
            .len(),
        2
    );
}
