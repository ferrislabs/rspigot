mod common;

use std::num::NonZeroU8;
use std::sync::Arc;
use std::time::Duration;

use common::*;
use rspigot_blocks::BlockKind;
use rspigot_core::config::{GameConfig, LevelData};
use rspigot_core::events::{ClientEvent, Hand, Inbound, MetadataValue, PlayerInput, Pose};
use rspigot_core::ports::ChunkGenerator;
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::item::ItemStack;
use rspigot_domain::world::Dimension;
use rspigot_domain::{BiomeId, BlockPos, BlockStateId, ChunkPos, EntityId, Vec3};
use rspigot_entities::EntityKind as Kind;
use rspigot_items::ItemKind;

const POSE_INDEX: u8 = 6;
const TONGUE_INDEX: u8 = 19;
const VARIANT_INDEX: u8 = 18;

struct Marsh;

fn biome_of(chunk: ChunkPos) -> &'static str {
    match chunk.x {
        1 => "minecraft:desert",
        -1 => "minecraft:snowy_plains",
        _ => "minecraft:plains",
    }
}

impl ChunkGenerator for Marsh {
    fn generate(&self, dimension: &Dimension, pos: ChunkPos) -> ChunkColumn {
        let biome = rspigot_data::registries::biome_id(biome_of(pos)).unwrap();
        let mut chunk = ChunkColumn::empty(pos, dimension.height, BiomeId(biome));
        for x in 0..16 {
            for z in 0..16 {
                let at = |y: i32| BlockPos::new(pos.min_block_x() + x, y, pos.min_block_z() + z);
                for y in 60..=62 {
                    chunk.set_block(at(y), BlockStateId(1));
                }
                chunk.set_block(at(63), BlockKind::GrassBlock.default_state());
            }
        }
        chunk
    }

    fn spawn_point(&self, _: &Dimension) -> Vec3 {
        Vec3::new(8.5, 64.0, 8.5)
    }
}

fn marsh_server(seed: u64) -> Server<Link> {
    let config = GameConfig {
        view_distance: 5,
        chunk_workers: 2,
        spawn_protection: 0,
        ..GameConfig::default()
    };
    let data = LevelData {
        spawn_mobs: false,
        day_time: 6_000,
        daylight_cycle: false,
        ..LevelData::default()
    };
    let mut server = Server::new(
        config,
        data,
        Arc::new(NoStore),
        Arc::new(Marsh),
        StatusBoard::new(20),
    )
    .with_rng_seed(seed);
    server.wait_for_spawn(Duration::from_secs(10));
    server
}

fn ready(seed: u64) -> Scene {
    let mut s = scene_on(marsh_server(seed), &["Alice"], Vec::new());
    stream_chunk_of(&mut s, BlockPos::new(8, 64, 8));
    s.tick(40);
    s
}

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn summon(s: &mut Scene, kind: Kind, at: &str, nbt: &str) -> EntityId {
    let known: Vec<EntityId> = s
        .server
        .entities_of(kind)
        .into_iter()
        .map(|entity| entity.id)
        .collect();
    console(s, &format!("summon {} {at} {nbt}", kind.name()));
    s.server
        .entities_of(kind)
        .into_iter()
        .map(|entity| entity.id)
        .find(|id| !known.contains(id))
        .expect("a new entity")
}

fn saved(s: &Scene, id: EntityId) -> rspigot_nbt::Compound {
    s.server.saved_entity(id).expect("a saved entity")
}

fn memories(s: &Scene, id: EntityId) -> rspigot_nbt::Compound {
    saved(s, id)
        .get_compound("Brain")
        .and_then(|brain| brain.get_compound("memories").cloned())
        .unwrap_or_default()
}

fn variant_of(s: &Scene, id: EntityId) -> String {
    saved(s, id)
        .get_str("variant")
        .map(str::to_owned)
        .expect("a variant")
}

fn exists(s: &Scene, kind: Kind, id: EntityId) -> bool {
    s.server
        .entities_of(kind)
        .iter()
        .any(|entity| entity.id == id)
}

fn items_on_the_ground(s: &Scene) -> Vec<String> {
    s.server
        .entities_of(Kind::Item)
        .into_iter()
        .filter_map(|item| s.server.saved_entity(item.id))
        .filter_map(|nbt| {
            nbt.get_compound("Item")
                .and_then(|stack| stack.get_str("id").map(str::to_owned))
        })
        .collect()
}

#[derive(Default)]
struct Seen {
    poses: Vec<Pose>,
    tongue: Vec<Option<u32>>,
    sounds: Vec<&'static str>,
}

impl Seen {
    fn absorb(&mut self, events: Vec<ClientEvent>, frog: EntityId) {
        for event in events {
            match event {
                ClientEvent::EntityMetadata { entity, entries } if entity == frog => {
                    for entry in entries {
                        match (entry.index, entry.value) {
                            (POSE_INDEX, MetadataValue::Pose(pose)) => self.poses.push(pose),
                            (TONGUE_INDEX, MetadataValue::OptionalUnsignedInt(target)) => {
                                self.tongue.push(target);
                            }
                            _ => {}
                        }
                    }
                }
                ClientEvent::Sound { sound, .. } => self.sounds.push(sound),
                _ => {}
            }
        }
    }
}

#[test]
fn a_frog_takes_its_variant_from_the_biome_it_spawns_in() {
    let mut s = ready(3);
    let temperate = summon(&mut s, Kind::Frog, "8.5 64 8.5", "");
    let warm = summon(&mut s, Kind::Frog, "24.5 64 8.5", "");
    let cold = summon(&mut s, Kind::Frog, "-8.5 64 8.5", "");
    assert_eq!(variant_of(&s, temperate), "minecraft:temperate");
    assert_eq!(variant_of(&s, warm), "minecraft:warm");
    assert_eq!(variant_of(&s, cold), "minecraft:cold");
}

#[test]
fn a_frog_keeps_its_saved_variant_and_has_no_sound_variant() {
    let mut s = ready(3);
    let frog = summon(
        &mut s,
        Kind::Frog,
        "8.5 64 8.5",
        "{NoAI:1b,variant:\"minecraft:cold\"}",
    );
    let nbt = saved(&s, frog);
    assert_eq!(nbt.get_str("variant"), Some("minecraft:cold"));
    assert!(nbt.get_str("sound_variant").is_none());
}

#[test]
fn the_variant_is_announced_to_the_clients_in_the_frog_slot() {
    let mut s = ready(3);
    s.players[0].link.take();
    let frog = summon(
        &mut s,
        Kind::Frog,
        "8.5 64 8.5",
        "{NoAI:1b,variant:\"minecraft:warm\"}",
    );
    s.tick(2);
    let mut announced = Vec::new();
    for event in s.players[0].link.take() {
        if let ClientEvent::EntityMetadata { entity, entries } = event
            && entity == frog
        {
            announced.extend(
                entries
                    .into_iter()
                    .filter(|entry| entry.index == VARIANT_INDEX),
            );
        }
    }
    assert!(!announced.is_empty());
    assert!(
        announced
            .iter()
            .all(|entry| matches!(entry.value, MetadataValue::Variant(_, 2)))
    );
}

#[test]
fn a_new_frog_starts_with_a_long_jump_cooldown_of_one_hundred_to_one_hundred_forty_ticks() {
    let mut s = ready(5);
    let frog = summon(&mut s, Kind::Frog, "8.5 64 8.5", "");
    s.tick(1);
    let cooldown = memories(&s, frog)
        .get_compound("minecraft:long_jump_cooling_down")
        .and_then(|entry| entry.get_i32("value"))
        .expect("a cooldown");
    assert!((98..=140).contains(&cooldown), "{cooldown}");
}

#[test]
fn a_frog_eats_a_small_slime_with_its_tongue() {
    let mut s = ready(7);
    let frog = summon(&mut s, Kind::Frog, "8.5 64 8.5", "");
    let slime = summon(&mut s, Kind::Slime, "13.5 64 8.5", "{NoAI:1b,Size:0}");
    let mut seen = Seen::default();
    let mut eaten = false;
    for _ in 0..600 {
        s.tick(1);
        seen.absorb(s.players[0].link.take(), frog);
        if !exists(&s, Kind::Slime, slime) {
            eaten = true;
            break;
        }
    }
    assert!(eaten, "the slime is still there");
    assert!(seen.poses.contains(&Pose::UsingTongue), "{:?}", seen.poses);
    assert!(seen.sounds.contains(&"minecraft:entity.frog.tongue"));
    assert!(seen.sounds.contains(&"minecraft:entity.frog.eat"));
    assert!(seen.tongue.iter().any(Option::is_some));
    s.tick(60);
    seen.absorb(s.players[0].link.take(), frog);
    assert_eq!(seen.tongue.last(), Some(&None));
    let shot = seen
        .poses
        .iter()
        .rposition(|pose| *pose == Pose::UsingTongue);
    assert_eq!(
        shot.and_then(|at| seen.poses.get(at + 1)),
        Some(&Pose::Standing)
    );
}

#[test]
fn a_frog_ignores_slimes_that_are_not_the_smallest_size() {
    let mut s = ready(7);
    summon(&mut s, Kind::Frog, "8.5 64 8.5", "");
    let big = summon(&mut s, Kind::Slime, "13.5 64 8.5", "{NoAI:1b,Size:1}");
    let mut seen = Seen::default();
    for _ in 0..800 {
        s.tick(1);
        seen.absorb(s.players[0].link.take(), EntityId(-1));
    }
    assert!(exists(&s, Kind::Slime, big));
    assert!(!seen.sounds.contains(&"minecraft:entity.frog.tongue"));
}

#[test]
fn a_warm_frog_that_eats_a_magma_cube_leaves_a_pearlescent_froglight() {
    let mut s = ready(9);
    summon(&mut s, Kind::Frog, "24.5 64 8.5", "");
    let cube = summon(&mut s, Kind::MagmaCube, "29.5 64 8.5", "{NoAI:1b,Size:0}");
    for _ in 0..900 {
        s.tick(1);
        s.players[0].link.take();
        if !exists(&s, Kind::MagmaCube, cube) {
            break;
        }
    }
    assert!(!exists(&s, Kind::MagmaCube, cube));
    s.tick(5);
    assert!(
        items_on_the_ground(&s)
            .iter()
            .any(|id| id == "minecraft:pearlescent_froglight"),
        "{:?}",
        items_on_the_ground(&s)
    );
}

#[test]
fn a_cold_frog_leaves_a_verdant_froglight_and_a_temperate_one_an_ochre_froglight() {
    for (at, expected) in [
        ("-8.5 64 8.5", "minecraft:verdant_froglight"),
        ("8.5 64 8.5", "minecraft:ochre_froglight"),
    ] {
        let mut s = ready(9);
        let x: f64 = at.split(' ').next().unwrap().parse().unwrap();
        summon(&mut s, Kind::Frog, at, "");
        let cube = summon(
            &mut s,
            Kind::MagmaCube,
            &format!("{} 64 8.5", x + 5.0),
            "{NoAI:1b,Size:0}",
        );
        for _ in 0..900 {
            s.tick(1);
            s.players[0].link.take();
            if !exists(&s, Kind::MagmaCube, cube) {
                break;
            }
        }
        s.tick(5);
        assert!(
            items_on_the_ground(&s).iter().any(|id| id == expected),
            "{expected} in {:?}",
            items_on_the_ground(&s)
        );
    }
}

#[test]
fn two_frogs_in_love_become_pregnant_instead_of_having_a_tadpole_child() {
    let mut s = ready(13);
    let first = summon(&mut s, Kind::Frog, "22.5 64 8.5", "{InLove:600}");
    let second = summon(&mut s, Kind::Frog, "23.5 64 8.5", "{InLove:600}");
    let pregnant = |s: &Scene| {
        [first, second]
            .into_iter()
            .filter(|frog| {
                memories(s, *frog)
                    .get_compound("minecraft:is_pregnant")
                    .is_some()
            })
            .count()
    };
    let mut orbs = 0;
    for _ in 0..600 {
        s.tick(1);
        s.players[0].link.take();
        orbs = orbs.max(s.server.entities_of(Kind::ExperienceOrb).len());
        if pregnant(&s) > 0 {
            break;
        }
    }
    for _ in 0..3 {
        s.tick(1);
        orbs = orbs.max(s.server.entities_of(Kind::ExperienceOrb).len());
    }
    assert_eq!(pregnant(&s), 1);
    assert_eq!(s.server.entities_of(Kind::Frog).len(), 2);
    assert_eq!(s.server.entities_of(Kind::Tadpole).len(), 0);
    for frog in [first, second] {
        let nbt = saved(&s, frog);
        assert_eq!(nbt.get_i32("InLove"), Some(0));
        let age = nbt.get_i32("Age").unwrap_or(0);
        assert!((5_900..=6_000).contains(&age), "{age}");
    }
    assert!(orbs >= 1);
}

#[test]
fn a_pregnant_frog_stays_pregnant_across_a_save_and_load() {
    let mut s = ready(13);
    let frog = summon(
        &mut s,
        Kind::Frog,
        "8.5 64 8.5",
        "{NoAI:1b,Brain:{memories:{\"minecraft:is_pregnant\":{value:{}}}}}",
    );
    s.tick(2);
    assert!(
        memories(&s, frog)
            .get_compound("minecraft:is_pregnant")
            .is_some()
    );
}

fn put_in_hand(s: &mut Scene, item: ItemKind) {
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::CreativeSlot {
            slot: 36,
            stack: Some(ItemStack::new(item.id(), NonZeroU8::MIN)),
        },
    );
}

fn interact(s: &mut Scene, target: EntityId) {
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Interact {
            target,
            hand: Hand::Main,
            sneaking: false,
            location: Vec3::new(0.0, 0.5, 0.0),
        },
    );
    s.tick(1);
}

#[test]
fn slime_balls_feed_a_frog_with_the_loud_eating_sound_and_nothing_else_does() {
    let mut s = ready(17);
    console(&mut s, "gamemode creative Alice");
    let frog = summon(&mut s, Kind::Frog, "9.5 64 9.5", "{NoAI:1b}");
    put_in_hand(&mut s, ItemKind::Wheat);
    interact(&mut s, frog);
    assert_eq!(saved(&s, frog).get_i32("InLove").unwrap_or(0), 0);
    s.players[0].link.take();
    put_in_hand(&mut s, ItemKind::SlimeBall);
    interact(&mut s, frog);
    assert!(saved(&s, frog).get_i32("InLove").unwrap_or(0) > 580);
    let heard: Vec<&'static str> = s.players[0]
        .link
        .take()
        .into_iter()
        .filter_map(|event| match event {
            ClientEvent::Sound { sound, .. } => Some(sound),
            _ => None,
        })
        .collect();
    assert!(heard.contains(&"minecraft:entity.frog.eat"), "{heard:?}");
}

#[test]
fn an_idle_frog_croaks_on_its_own() {
    let mut s = ready(19);
    let frog = summon(&mut s, Kind::Frog, "8.5 64 8.5", "");
    let mut seen = Seen::default();
    for _ in 0..1_500 {
        s.tick(1);
        seen.absorb(s.players[0].link.take(), frog);
    }
    assert!(seen.poses.contains(&Pose::Croaking), "{:?}", seen.poses);
}

#[test]
fn a_frog_takes_five_points_less_fall_damage() {
    let mut s = ready(23);
    let frog = summon(&mut s, Kind::Frog, "8.5 75 8.5", "");
    let cow = summon(&mut s, Kind::Cow, "10.5 75 8.5", "");
    s.tick(60);
    let health = |id: EntityId| saved(&s, id).get_f64("Health").unwrap();
    let lost_cow = 10.0 - health(cow);
    let lost_frog = 10.0 - health(frog);
    assert!(lost_cow > 0.0, "{lost_cow}");
    assert!(
        (lost_cow - lost_frog - 5.0).abs() < 1.0e-6 || lost_frog == 0.0,
        "{lost_cow} {lost_frog}"
    );
}
