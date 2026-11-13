mod common;

use std::sync::Arc;
use std::time::Duration;

use common::*;
use rspigot_core::config::PluginSettings;
use rspigot_core::config::{GameConfig, LevelData};
use rspigot_core::events::{ClientEvent, Inbound};
use rspigot_core::plugins::{EventKind, EventPayload, HandlerSpec};
use rspigot_core::ports::ChunkGenerator;
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::world::Dimension;
use rspigot_domain::{BiomeId, BlockStateId, ChunkPos, Vec3};
use rspigot_domain::{BlockPos, EntityId};
use rspigot_entities::EntityKind as Kind;
use rspigot_nbt::Tag;

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn seeded(seed: u64) -> Scene {
    scene_on(
        server(PluginSettings::default()).with_rng_seed(seed),
        &["Alice"],
        Vec::new(),
    )
}

fn ready(mut s: Scene) -> Scene {
    stream_chunk_of(&mut s, BlockPos::new(1, 64, 1));
    s
}

fn only(s: &Scene, kind: Kind) -> EntityId {
    let found = s.server.entities_of(kind);
    assert_eq!(found.len(), 1, "one {kind:?}");
    found[0].id
}

struct Pen;

impl ChunkGenerator for Pen {
    fn generate(&self, dimension: &Dimension, pos: ChunkPos) -> ChunkColumn {
        let mut chunk = ChunkColumn::empty(pos, dimension.height, BiomeId(0));
        for x in 0..16 {
            for z in 0..16 {
                let at = |y: i32| BlockPos::new(pos.min_block_x() + x, y, pos.min_block_z() + z);
                for y in 60..=63 {
                    chunk.set_block(at(y), BlockStateId(1));
                }
                let (bx, bz) = (pos.min_block_x() + x, pos.min_block_z() + z);
                let ring = (3..=9).contains(&bx)
                    && (3..=9).contains(&bz)
                    && !((4..=8).contains(&bx) && (4..=8).contains(&bz));
                if ring && pos == ChunkPos::new(0, 0) {
                    for y in 64..=67 {
                        chunk.set_block(at(y), BlockStateId(1));
                    }
                }
            }
        }
        chunk
    }

    fn spawn_point(&self, _: &Dimension) -> Vec3 {
        Vec3::new(0.5, 64.0, 0.5)
    }
}

fn penned(seed: u64) -> Scene {
    penned_with(seed, Vec::new())
}

fn penned_with(seed: u64, handlers: Vec<HandlerSpec>) -> Scene {
    let config = GameConfig {
        view_distance: 3,
        chunk_workers: 2,
        spawn_protection: 0,
        plugins: PluginSettings {
            decision_ticks: 1_000,
            ..PluginSettings::default()
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
        Arc::new(NoStore),
        Arc::new(Pen),
        StatusBoard::new(20),
    )
    .with_rng_seed(seed);
    server.wait_for_spawn(Duration::from_secs(10));
    scene_on(server, &["Alice"], handlers)
}

const JOCKEY: &str = "{Passengers:[{id:\"minecraft:zombie\",IsBaby:1b}]}";

fn passenger_lists(events: &[ClientEvent]) -> Vec<(EntityId, Vec<EntityId>)> {
    events
        .iter()
        .filter_map(|event| match event {
            ClientEvent::Passengers {
                vehicle,
                passengers,
            } => Some((*vehicle, passengers.clone())),
            _ => None,
        })
        .collect()
}

#[test]
fn a_summoned_stack_loads_its_passengers_onto_the_vehicle_and_tells_the_clients() {
    let mut s = ready(scene(&["Alice"]));
    s.players[0].link.take();
    console(
        &mut s,
        &format!("summon minecraft:chicken 5.5 64 5.5 {JOCKEY}"),
    );
    let chicken = only(&s, Kind::Chicken);
    let zombie = only(&s, Kind::Zombie);
    s.tick(1);
    let lists = passenger_lists(&s.players[0].link.take());
    assert!(lists.contains(&(chicken, vec![zombie])), "{lists:?}");
    let chicken_at = s.server.entities_of(Kind::Chicken)[0].position;
    let zombie_at = s.server.entities_of(Kind::Zombie)[0].position;
    assert!((zombie_at.x - chicken_at.x).abs() < 1e-6);
    assert!(
        (zombie_at.y - (chicken_at.y + 0.7 - 0.1875)).abs() < 1e-3,
        "{zombie_at:?} {chicken_at:?}"
    );
    assert!(
        (zombie_at.z - (chicken_at.z - 0.1)).abs() < 1e-3,
        "{zombie_at:?} {chicken_at:?}"
    );
}

#[test]
fn a_rider_follows_its_vehicle_and_is_saved_inside_it() {
    let mut s = ready(scene(&["Alice"]));
    console(
        &mut s,
        &format!("summon minecraft:chicken 5.5 64 5.5 {JOCKEY}"),
    );
    let chicken = only(&s, Kind::Chicken);
    let zombie = only(&s, Kind::Zombie);
    console(&mut s, "tp @e[type=chicken] 8.5 64 8.5");
    s.tick(2);
    let chicken_at = s.server.entities_of(Kind::Chicken)[0].position;
    let zombie_at = s.server.entities_of(Kind::Zombie)[0].position;
    assert!((zombie_at.x - chicken_at.x).abs() < 1e-6);
    let saved = s.server.saved_entity(chicken).unwrap();
    let Some(Tag::List(riders)) = saved.get("Passengers") else {
        panic!("passengers are saved in the vehicle");
    };
    assert_eq!(riders.len(), 1);
    assert!(format!("{riders:?}").contains("minecraft:zombie"));
    let alone = s.server.saved_entity(zombie).unwrap();
    assert!(alone.get("Passengers").is_none());
}

#[test]
fn killing_the_vehicle_drops_the_rider_beside_it_with_a_boarding_cooldown() {
    let mut s = ready(scene(&["Alice"]));
    s.players[0].link.take();
    console(
        &mut s,
        &format!("summon minecraft:chicken 5.5 64 5.5 {JOCKEY}"),
    );
    let chicken = only(&s, Kind::Chicken);
    let zombie = only(&s, Kind::Zombie);
    s.tick(1);
    s.players[0].link.take();
    console(&mut s, "kill @e[type=chicken]");
    s.tick(25);
    assert!(s.server.entities_of(Kind::Chicken).is_empty());
    let zombie_at = s.server.entities_of(Kind::Zombie)[0].position;
    assert!(zombie_at.y >= 63.9 && zombie_at.y < 66.0, "{zombie_at:?}");
    let lists = passenger_lists(&s.players[0].link.take());
    assert!(lists.contains(&(chicken, Vec::new())), "{lists:?}");
    assert_eq!(s.server.entities_of(Kind::Zombie)[0].id, zombie);
}

#[test]
fn some_spiders_spawn_with_a_skeleton_riding_them() {
    let mut s = ready(seeded(11));
    for _ in 0..700 {
        console(&mut s, "summon minecraft:spider 5.5 64 5.5");
    }
    s.tick(2);
    let skeletons = s.server.entities_of(Kind::Skeleton);
    assert!(!skeletons.is_empty(), "a skeleton jockey spawned");
    let carried = s
        .server
        .entities_of(Kind::Spider)
        .into_iter()
        .filter(|spider| {
            s.server
                .saved_entity(spider.id)
                .is_some_and(|nbt| nbt.get("Passengers").is_some())
        })
        .count();
    assert_eq!(carried, skeletons.len());
}

#[test]
fn some_baby_zombies_spawn_as_chicken_jockeys() {
    let mut s = ready(seeded(5));
    for _ in 0..1800 {
        console(&mut s, "summon minecraft:zombie 5.5 64 5.5");
    }
    s.tick(2);
    let chickens = s.server.entities_of(Kind::Chicken);
    assert!(!chickens.is_empty(), "a chicken jockey spawned");
    let jockeys = chickens
        .iter()
        .filter(|chicken| {
            s.server
                .saved_entity(chicken.id)
                .is_some_and(|nbt| nbt.get("Passengers").is_some())
        })
        .count();
    assert!(jockeys > 0, "a zombie rides a chicken");
}

fn riders(s: &Scene, vehicle: EntityId) -> Vec<String> {
    let Some(saved) = s.server.saved_entity(vehicle) else {
        return Vec::new();
    };
    let Some(Tag::List(riders)) = saved.get("Passengers") else {
        return Vec::new();
    };
    riders
        .iter()
        .filter_map(|tag| match tag {
            Tag::Compound(nbt) => nbt.get_str("id").map(str::to_owned),
            _ => None,
        })
        .collect()
}

#[test]
fn a_baby_piglin_walks_to_a_baby_hoglin_and_rides_it_for_a_while() {
    let mut s = ready(penned(3));
    console(
        &mut s,
        "summon minecraft:hoglin 5.5 64 5.5 {Age:-24000,IsImmuneToZombification:1b,NoAI:1b}",
    );
    console(
        &mut s,
        "summon minecraft:piglin 6.5 64 5.5 {IsBaby:1b,IsImmuneToZombification:1b}",
    );
    let hoglin = only(&s, Kind::Hoglin);
    let piglin = only(&s, Kind::Piglin);
    let mut mounted_at = None;
    for tick in 0..1600 {
        s.tick(1);
        if !riders(&s, hoglin).is_empty() {
            mounted_at = Some(tick);
            break;
        }
    }
    assert!(mounted_at.is_some(), "the baby piglin rode the baby hoglin");
    assert_eq!(riders(&s, hoglin), vec!["minecraft:piglin".to_owned()]);
    s.tick(2);
    let piglin_at = s
        .server
        .entities_of(Kind::Piglin)
        .iter()
        .find(|e| e.id == piglin)
        .map(|e| e.position)
        .unwrap();
    let hoglin_at = s.server.entities_of(Kind::Hoglin)[0].position;
    assert!((piglin_at.x - hoglin_at.x).abs() < 1e-6, "{piglin_at:?}");
    s.tick(700);
    assert!(
        riders(&s, hoglin).is_empty(),
        "the ride lasts at most thirty seconds"
    );
}

fn pen_pair(s: &mut Scene) -> (EntityId, EntityId) {
    console(
        s,
        "summon minecraft:hoglin 5.5 64 5.5 {Age:-24000,IsImmuneToZombification:1b,NoAI:1b}",
    );
    console(
        s,
        "summon minecraft:piglin 6.5 64 5.5 {IsBaby:1b,IsImmuneToZombification:1b}",
    );
    (only(s, Kind::Hoglin), only(s, Kind::Piglin))
}

fn tick_until_asked(s: &mut Scene, kind: EventKind) -> Fired {
    for _ in 0..1600 {
        s.tick(1);
        if !asked_for(&s.plugin, kind).is_empty() {
            return last_asked(&s.plugin, kind);
        }
    }
    panic!("a {kind:?} was asked");
}

#[test]
fn a_plugin_can_cancel_the_mount_of_a_piglin_on_a_hoglin() {
    let mut s = ready(penned_with(3, vec![deciding(1, EventKind::EntityMount)]));
    let (hoglin, piglin) = pen_pair(&mut s);
    let asked = tick_until_asked(&mut s, EventKind::EntityMount);
    let EventPayload::EntityMount(mount) = &asked.payload else {
        panic!("an entity mount");
    };
    assert_eq!(mount.entity.id, piglin);
    assert_eq!(mount.mount.id, hoglin);
    assert!(riders(&s, hoglin).is_empty());
    rule_on(&mut s.server, 1, asked.id, true, None);
    s.tick(5);
    assert!(riders(&s, hoglin).is_empty(), "the mount was cancelled");
}

#[test]
fn a_plugin_sees_a_piglin_mount_then_dismount_the_hoglin() {
    let mut s = ready(penned_with(
        3,
        vec![
            observe(1, EventKind::EntityMount),
            observe(2, EventKind::EntityDismount),
        ],
    ));
    let (hoglin, _) = pen_pair(&mut s);
    for _ in 0..1600 {
        s.tick(1);
        if !riders(&s, hoglin).is_empty() {
            break;
        }
    }
    assert!(!settled_for(&s.plugin, EventKind::EntityMount).is_empty());
    for _ in 0..900 {
        s.tick(1);
        if riders(&s, hoglin).is_empty() {
            break;
        }
    }
    assert!(riders(&s, hoglin).is_empty());
    let dismounts = settled_for(&s.plugin, EventKind::EntityDismount);
    assert!(!dismounts.is_empty(), "{:?}", fired_kinds(&s.plugin));
}

#[test]
fn a_zombie_riding_a_chicken_steers_it_toward_the_player() {
    let mut s = ready(scene(&["Alice"]));
    console(
        &mut s,
        "summon minecraft:chicken 8.5 64 0.5 {Passengers:[{id:\"minecraft:zombie\",IsBaby:1b}]}",
    );
    let chicken = only(&s, Kind::Chicken);
    let before = s.server.entities_of(Kind::Chicken)[0].position;
    s.tick(80);
    let after = s
        .server
        .entities_of(Kind::Chicken)
        .iter()
        .find(|e| e.id == chicken)
        .map(|e| e.position)
        .unwrap();
    assert!(after.x < before.x - 1.0, "{before:?} {after:?}");
    let zombie = s.server.entities_of(Kind::Zombie)[0].position;
    assert!((zombie.x - after.x).abs() < 0.2, "{zombie:?} {after:?}");
}
