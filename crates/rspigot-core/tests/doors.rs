mod common;

use std::sync::Arc;
use std::time::Duration;

use common::*;
use rspigot_blocks::BlockKind;
use rspigot_core::config::{GameConfig, LevelData};
use rspigot_core::events::{ClientEvent, Direction, Hand, Inbound, PlayerInput};
use rspigot_core::plugins::{Answer, EventKind, EventPayload, Question};
use rspigot_core::ports::ChunkGenerator;
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::world::Dimension;
use rspigot_domain::{BiomeId, BlockPos, BlockStateId, ChunkPos, EntityId, Vec3};
use rspigot_entities::EntityKind as Kind;

const WALL_X: i32 = 10;
const DOOR_Z: i32 = 8;
const LOWER: BlockPos = BlockPos::new(WALL_X, 64, DOOR_Z);

struct Walled;

fn door(half: &str) -> BlockStateId {
    BlockKind::OakDoor
        .default_state()
        .with_property("half", half)
        .unwrap()
        .with_property("facing", "east")
        .unwrap()
}

impl ChunkGenerator for Walled {
    fn generate(&self, dimension: &Dimension, pos: ChunkPos) -> ChunkColumn {
        let biome = rspigot_data::registries::biome_id("minecraft:plains").unwrap();
        let mut chunk = ChunkColumn::empty(pos, dimension.height, BiomeId(biome));
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
        if pos.x == 0 {
            for z in 0..16 {
                let at = pos.min_block_z() + z;
                for y in 64..=66 {
                    chunk.set_block(BlockPos::new(WALL_X, y, at), BlockStateId(1));
                }
            }
        }
        if pos == ChunkPos::new(0, 0) {
            chunk.set_block(LOWER, door("lower"));
            chunk.set_block(LOWER.offset(0, 1, 0), door("upper"));
            let bed = BlockKind::RedBed.default_state();
            chunk.set_block(
                BlockPos::new(13, 64, DOOR_Z),
                bed.with_property("part", "head")
                    .unwrap()
                    .with_property("facing", "east")
                    .unwrap(),
            );
            chunk.set_block(
                BlockPos::new(12, 64, DOOR_Z),
                bed.with_property("part", "foot")
                    .unwrap()
                    .with_property("facing", "east")
                    .unwrap(),
            );
        }
        chunk
    }

    fn spawn_point(&self, _: &Dimension) -> Vec3 {
        Vec3::new(0.5, 64.0, 0.5)
    }
}

fn walled(seed: u64, handlers: Vec<rspigot_core::plugins::HandlerSpec>) -> Scene {
    let config = GameConfig {
        view_distance: 3,
        chunk_workers: 2,
        spawn_protection: 0,
        ..GameConfig::default()
    };
    let data = LevelData {
        spawn_mobs: false,
        day_time: 13_000,
        daylight_cycle: false,
        ..LevelData::default()
    };
    let mut server = Server::new(
        config,
        data,
        Arc::new(NoStore),
        Arc::new(Walled),
        StatusBoard::new(20),
    )
    .with_rng_seed(seed);
    server.wait_for_spawn(Duration::from_secs(10));
    let mut s = scene_on(server, &["Alice", "Bob"], handlers);
    stream_chunk_of(&mut s, BlockPos::new(1, 64, 1));
    s
}

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn summon(s: &mut Scene, kind: Kind, at: &str, nbt: &str) -> EntityId {
    console(s, &format!("summon {} {at} {nbt}", kind.name()));
    s.server
        .entities_of(kind)
        .into_iter()
        .map(|entity| entity.id)
        .next_back()
        .expect("a new entity")
}

const HOME: &str = "{Brain:{memories:{\"minecraft:home\":{value:{dimension:\"minecraft:overworld\",pos:[I;13,64,8]}}}}}";

fn door_open(events: &[ClientEvent]) -> Vec<bool> {
    events
        .iter()
        .filter_map(|event| match event {
            ClientEvent::BlockUpdate { pos, state } if *pos == LOWER => {
                Some(state.property("open") == Some("true"))
            }
            _ => None,
        })
        .collect()
}

fn door_sounds(events: &[ClientEvent]) -> Vec<&'static str> {
    events
        .iter()
        .filter_map(|event| match event {
            ClientEvent::Sound { sound, .. } if sound.contains("_door.") => Some(*sound),
            _ => None,
        })
        .collect()
}

#[test]
fn a_villager_walking_to_bed_opens_the_closed_door_in_its_way_and_closes_it_behind() {
    let mut s = walled(3, Vec::new());
    s.players[0].link.take();
    let villager = summon(&mut s, Kind::Villager, "7.5 64 8.5", HOME);
    let mut events = Vec::new();
    for _ in 0..500 {
        s.tick(1);
        events.extend(s.players[0].link.take());
    }
    let states = door_open(&events);
    assert!(states.contains(&true), "the door was opened: {states:?}");
    assert_eq!(states.last(), Some(&false), "the door was closed again");
    let sounds = door_sounds(&events);
    assert!(
        sounds.contains(&"minecraft:block.wooden_door.open")
            && sounds.contains(&"minecraft:block.wooden_door.close"),
        "{sounds:?}"
    );
    let at = s
        .server
        .entities_of(Kind::Villager)
        .into_iter()
        .find(|entity| entity.id == villager)
        .expect("the villager")
        .position;
    assert!(at.x > f64::from(WALL_X), "{at:?}");
}

fn ask_for_the_door(s: &mut Scene) {
    for _ in 0..800 {
        s.tick(1);
        if !asked_for(&s.plugin, EventKind::EntityInteract).is_empty() {
            return;
        }
    }
    panic!("the villager never asked to interact with the door");
}

fn door_is_open(s: &mut Scene) -> bool {
    match s.ask(Question::Block(LOWER)) {
        Answer::Block(state) => state
            .properties
            .iter()
            .any(|(name, value)| name == "open" && value == "true"),
        _ => false,
    }
}

#[test]
fn a_plugin_is_asked_before_a_villager_opens_a_door_and_can_keep_it_shut() {
    let mut s = walled(3, vec![deciding(1, EventKind::EntityInteract)]);
    summon(&mut s, Kind::Villager, "7.5 64 8.5", HOME);
    ask_for_the_door(&mut s);
    let asked = last_asked(&s.plugin, EventKind::EntityInteract);
    let EventPayload::EntityInteract(interact) = &asked.payload else {
        panic!("an entity interact");
    };
    assert_eq!(interact.block, LOWER);
    assert_eq!(interact.entity_type, "minecraft:villager");
    assert!(!door_is_open(&mut s));
    rule_on(&mut s.server, 1, asked.id, true, None);
    assert!(!door_is_open(&mut s));
    for _ in 0..40 {
        s.tick(1);
        if last_asked(&s.plugin, EventKind::EntityInteract).id != asked.id {
            break;
        }
    }
    let again = last_asked(&s.plugin, EventKind::EntityInteract);
    assert_ne!(again.id, asked.id);
    rule_on(&mut s.server, 1, again.id, false, None);
    assert!(door_is_open(&mut s));
}

#[test]
fn a_player_toggling_a_door_is_heard_by_the_others_but_not_by_themselves() {
    let mut s = walled(3, Vec::new());
    console(&mut s, "tp Alice 8.5 64 8.5");
    s.tick(2);
    let alice = s.players[0].clone();
    let bob = s.players[1].clone();
    alice.link.take();
    bob.link.take();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::UseItemOn {
            hand: Hand::Main,
            pos: LOWER,
            face: Direction::West,
            cursor: Vec3::new(0.5, 0.5, 0.5),
            inside_block: false,
            sequence: 9,
        },
    );
    s.tick(1);
    let own = alice.link.take();
    assert!(door_open(&own).contains(&true));
    assert!(door_sounds(&own).is_empty());
    assert_eq!(
        door_sounds(&bob.link.take()),
        vec!["minecraft:block.wooden_door.open"]
    );
}
