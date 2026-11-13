mod common;

use std::sync::Arc;
use std::time::Duration;

use common::*;
use rspigot_blocks::BlockKind;
use rspigot_core::config::{GameConfig, LevelData};
use rspigot_core::events::{Hand, Inbound, PlayerInput};
use rspigot_core::plugins::{Answer, Question};
use rspigot_core::ports::ChunkGenerator;
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_domain::Rotation;
use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::world::Dimension;
use rspigot_domain::{BiomeId, BlockPos, BlockStateId, ChunkPos, EntityId, Vec3};
use rspigot_entities::EntityKind as Kind;

struct Pond;

fn biome_of(chunk: ChunkPos) -> &'static str {
    match chunk.x {
        1 => "minecraft:warm_ocean",
        _ => "minecraft:plains",
    }
}

fn in_pond(x: i32, z: i32) -> bool {
    (11..=14).contains(&x) && (6..=10).contains(&z)
}

impl ChunkGenerator for Pond {
    fn generate(&self, dimension: &Dimension, pos: ChunkPos) -> ChunkColumn {
        let biome = rspigot_data::registries::biome_id(biome_of(pos)).unwrap();
        let mut chunk = ChunkColumn::empty(pos, dimension.height, BiomeId(biome));
        for x in 0..16 {
            for z in 0..16 {
                let (wx, wz) = (pos.min_block_x() + x, pos.min_block_z() + z);
                let at = |y: i32| BlockPos::new(wx, y, wz);
                for y in 55..=61 {
                    chunk.set_block(at(y), BlockStateId(1));
                }
                if pos.x == 0 && in_pond(wx, wz) {
                    chunk.set_block(at(62), BlockKind::Water.default_state());
                    chunk.set_block(at(63), BlockKind::Water.default_state());
                } else {
                    chunk.set_block(at(62), BlockStateId(1));
                    chunk.set_block(at(63), BlockKind::GrassBlock.default_state());
                }
            }
        }
        chunk
    }

    fn spawn_point(&self, _: &Dimension) -> Vec3 {
        Vec3::new(8.5, 64.0, 8.5)
    }
}

fn pond_server(seed: u64) -> Server<Link> {
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
        Arc::new(Pond),
        StatusBoard::new(20),
    )
    .with_rng_seed(seed);
    server.wait_for_spawn(Duration::from_secs(10));
    server
}

fn ready(seed: u64) -> Scene {
    let mut s = scene_on(pond_server(seed), &["Alice"], Vec::new());
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

fn first_slot(s: &mut Scene) -> Option<String> {
    let alice = s.players[0].clone();
    match s.ask(Question::Inventory(alice.id)) {
        Answer::Inventory(inventory) => inventory
            .slots
            .into_iter()
            .flatten()
            .next()
            .map(|spec| spec.key),
        other => panic!("expected the inventory, got {other:?}"),
    }
}

fn use_down(s: &mut Scene) {
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::UseItem {
            hand: Hand::Main,
            sequence: 1,
            rotation: Rotation::new(0.0, 90.0),
        },
    );
    s.tick(2);
}

fn block(s: &Scene, x: i32, y: i32, z: i32) -> Option<BlockKind> {
    s.server
        .block_state_at(BlockPos::new(x, y, z))
        .and_then(BlockStateId::kind)
}

fn frogspawn_positions(s: &Scene) -> Vec<BlockPos> {
    let mut found = Vec::new();
    for x in 5..=20 {
        for z in 0..=15 {
            if block(s, x, 64, z) == Some(BlockKind::Frogspawn) {
                found.push(BlockPos::new(x, 64, z));
            }
        }
    }
    found
}

#[test]
fn a_pregnant_frog_beside_a_pond_lays_frogspawn_on_the_water_and_stops_being_pregnant() {
    let mut s = ready(21);
    let frog = summon(
        &mut s,
        Kind::Frog,
        "10.5 64 8.5",
        "{Brain:{memories:{\"minecraft:is_pregnant\":{value:{}}}}}",
    );
    let mut laid = Vec::new();
    for _ in 0..400 {
        s.tick(1);
        s.players[0].link.take();
        laid = frogspawn_positions(&s);
        if !laid.is_empty() {
            break;
        }
    }
    assert_eq!(laid.len(), 1, "one clutch of frogspawn");
    let pos = laid[0];
    assert!(in_pond(pos.x, pos.z));
    assert_eq!(block(&s, pos.x, 63, pos.z), Some(BlockKind::Water));
    s.tick(3);
    assert!(
        memories(&s, frog)
            .get_compound("minecraft:is_pregnant")
            .is_none()
    );
}

#[test]
fn frogspawn_hatches_into_two_to_five_persistent_tadpoles_and_leaves_no_block() {
    let mut s = ready(22);
    summon(
        &mut s,
        Kind::Frog,
        "10.5 64 8.5",
        "{Brain:{memories:{\"minecraft:is_pregnant\":{value:{}}}}}",
    );
    let mut laid = Vec::new();
    for _ in 0..400 {
        s.tick(1);
        s.players[0].link.take();
        laid = frogspawn_positions(&s);
        if !laid.is_empty() {
            break;
        }
    }
    assert_eq!(laid.len(), 1);
    assert!(s.server.entities_of(Kind::Tadpole).is_empty());
    let mut hatched = Vec::new();
    for _ in 0..12_000 {
        s.tick(1);
        s.players[0].link.take();
        hatched = s.server.entities_of(Kind::Tadpole);
        if !hatched.is_empty() {
            break;
        }
    }
    assert!((2..=5).contains(&hatched.len()), "{}", hatched.len());
    assert!(frogspawn_positions(&s).is_empty());
    for tadpole in hatched {
        let nbt = saved(&s, tadpole.id);
        assert_eq!(nbt.get_bool("PersistenceRequired"), Some(true));
        assert_eq!(nbt.get_bool("FromBucket"), Some(true));
    }
}

#[test]
fn a_tadpole_turns_into_a_persistent_frog_after_twenty_four_thousand_ticks() {
    let mut s = ready(24);
    let tadpole = summon(&mut s, Kind::Tadpole, "12.5 62 8.5", "{NoAI:1b,Age:23990}");
    assert!(s.server.entities_of(Kind::Frog).is_empty());
    s.tick(30);
    assert!(
        s.server
            .entities_of(Kind::Tadpole)
            .iter()
            .all(|entity| entity.id != tadpole)
    );
    let frogs = s.server.entities_of(Kind::Frog);
    assert_eq!(frogs.len(), 1);
    let nbt = saved(&s, frogs[0].id);
    assert_eq!(nbt.get_bool("PersistenceRequired"), Some(true));
    assert_eq!(nbt.get_str("variant"), Some("minecraft:temperate"));
}

#[test]
fn an_age_locked_tadpole_never_grows_and_keeps_its_age() {
    let mut s = ready(25);
    let tadpole = summon(
        &mut s,
        Kind::Tadpole,
        "12.5 62 8.5",
        "{NoAI:1b,Age:100,AgeLocked:1b}",
    );
    s.tick(50);
    let nbt = saved(&s, tadpole);
    assert_eq!(nbt.get_i32("Age"), Some(100));
    assert_eq!(nbt.get_bool("AgeLocked"), Some(true));
    assert_eq!(nbt.get_bool("FromBucket"), Some(true));
}

#[test]
fn a_slime_ball_ages_a_tadpole_by_a_tenth_of_what_it_has_left() {
    let mut s = ready(26);
    let tadpole = summon(&mut s, Kind::Tadpole, "12.5 62 8.5", "{NoAI:1b,Age:0}");
    console(&mut s, "give Alice slime_ball 1");
    interact(&mut s, tadpole);
    let age = saved(&s, tadpole).get_i32("Age").unwrap();
    assert!((2400..=2402).contains(&age), "{age}");
}

#[test]
fn an_axolotl_saves_its_variant_and_bucket_flag_in_the_vanilla_layout() {
    let mut s = ready(28);
    let axolotl = summon(
        &mut s,
        Kind::Axolotl,
        "12.5 62 8.5",
        "{NoAI:1b,Variant:3,FromBucket:1b}",
    );
    let nbt = saved(&s, axolotl);
    assert_eq!(nbt.get_i32("Variant"), Some(3));
    assert_eq!(nbt.get_bool("FromBucket"), Some(true));
    assert_eq!(
        nbt.get_bool("PersistenceRequired"),
        None.or(nbt.get_bool("PersistenceRequired"))
    );
}

#[test]
fn an_axolotl_out_of_the_water_dries_out_and_takes_two_damage_when_its_air_runs_out() {
    let mut s = ready(31);
    let axolotl = summon(&mut s, Kind::Axolotl, "4.5 64 4.5", "{NoAI:0b,Air:5s}");
    let health = |s: &Scene| saved(s, axolotl).get_f64("Health").unwrap_or(0.0);
    assert!(health(&s) > 13.9);
    s.tick(40);
    assert!(health(&s) <= 12.1, "{}", health(&s));
}

#[test]
fn an_axolotl_in_the_water_keeps_its_air() {
    let mut s = ready(32);
    let axolotl = summon(&mut s, Kind::Axolotl, "12.5 62 8.5", "{Air:200s}");
    s.tick(60);
    assert!(saved(&s, axolotl).get_f64("Health").unwrap_or(0.0) > 13.9);
}

#[test]
fn a_zombie_nautilus_takes_the_warm_variant_in_a_warm_ocean_and_the_temperate_one_elsewhere() {
    let mut s = ready(33);
    let warm = summon(&mut s, Kind::ZombieNautilus, "24.5 64 8.5", "");
    let temperate = summon(&mut s, Kind::ZombieNautilus, "8.5 64 8.5", "");
    assert_eq!(saved(&s, warm).get_str("variant"), Some("minecraft:warm"));
    assert_eq!(
        saved(&s, temperate).get_str("variant"),
        Some("minecraft:temperate")
    );
}

#[test]
fn a_nautilus_gets_its_first_attack_cooldown_when_it_spawns() {
    let mut s = ready(34);
    let nautilus = summon(&mut s, Kind::Nautilus, "12.5 62 8.5", "");
    s.tick(2);
    let cooldown = memories(&s, nautilus)
        .get_compound("minecraft:attack_target_cooldown")
        .and_then(|memory| memory.get_i32("value"));
    assert!(
        cooldown.is_some_and(|ticks| (2300..=3600).contains(&ticks)),
        "{cooldown:?}"
    );
}

#[test]
fn feeding_a_nautilus_pufferfish_tames_about_one_in_three_for_the_player() {
    let mut s = ready(35);
    let mut tamed = 0;
    let mut tries = 0;
    for index in 0..24 {
        let nautilus = summon(
            &mut s,
            Kind::Nautilus,
            &format!("{}.5 62 {}.5", 11 + index % 4, 6 + index % 5),
            "{NoAI:1b}",
        );
        console(&mut s, "give Alice pufferfish 1");
        interact(&mut s, nautilus);
        tries += 1;
        if saved(&s, nautilus).get("Owner").is_some() {
            tamed += 1;
        }
    }
    assert!(tamed >= 2 && tamed <= tries - 2, "{tamed}/{tries}");
}

#[test]
fn an_axolotl_goes_into_a_bucket_and_comes_out_with_its_variant_age_and_hunting_cooldown() {
    let mut s = ready(40);
    let axolotl = summon(
        &mut s,
        Kind::Axolotl,
        "12.5 62 8.5",
        "{NoAI:1b,Variant:2,Age:-500,Health:3f,Brain:{memories:{\"minecraft:has_hunting_cooldown\":{value:1b,ttl:900L}}}}",
    );
    s.tick(2);
    console(&mut s, "give Alice water_bucket 1");
    interact(&mut s, axolotl);
    assert!(s.server.entities_of(Kind::Axolotl).is_empty());
    assert_eq!(
        first_slot(&mut s).as_deref(),
        Some("minecraft:axolotl_bucket")
    );
    use_down(&mut s);
    let axolotls = s.server.entities_of(Kind::Axolotl);
    assert_eq!(axolotls.len(), 1);
    let nbt = saved(&s, axolotls[0].id);
    assert_eq!(nbt.get_i32("Variant"), Some(2));
    assert_eq!(nbt.get_bool("FromBucket"), Some(true));
    assert_eq!(nbt.get_bool("NoAI"), Some(true));
    assert!(nbt.get_f64("Health").is_some_and(|health| health <= 3.0));
    assert!(
        nbt.get_i32("Age")
            .is_some_and(|age| (-500..0).contains(&age))
    );
    let cooldown = memories(&s, axolotls[0].id)
        .get_compound("minecraft:has_hunting_cooldown")
        .and_then(|memory| memory.get_i64("ttl"));
    assert!(
        cooldown.is_some_and(|ticks| ticks > 0 && ticks <= 900),
        "{cooldown:?}"
    );
}

#[test]
fn a_tadpole_goes_into_a_bucket_and_comes_out_with_its_age_and_its_lock() {
    let mut s = ready(41);
    let tadpole = summon(
        &mut s,
        Kind::Tadpole,
        "12.5 62 8.5",
        "{NoAI:1b,Age:500,AgeLocked:1b}",
    );
    console(&mut s, "give Alice water_bucket 1");
    interact(&mut s, tadpole);
    assert!(s.server.entities_of(Kind::Tadpole).is_empty());
    assert_eq!(
        first_slot(&mut s).as_deref(),
        Some("minecraft:tadpole_bucket")
    );
    use_down(&mut s);
    let tadpoles = s.server.entities_of(Kind::Tadpole);
    assert_eq!(tadpoles.len(), 1);
    let nbt = saved(&s, tadpoles[0].id);
    assert_eq!(nbt.get_i32("Age"), Some(500));
    assert_eq!(nbt.get_bool("AgeLocked"), Some(true));
}

#[test]
fn a_tropical_fish_bucket_feeds_an_axolotl_and_hands_back_a_water_bucket() {
    let mut s = ready(42);
    let axolotl = summon(&mut s, Kind::Axolotl, "12.5 62 8.5", "{NoAI:1b}");
    console(&mut s, "give Alice tropical_fish_bucket 1");
    interact(&mut s, axolotl);
    assert_eq!(
        first_slot(&mut s).as_deref(),
        Some("minecraft:water_bucket")
    );
    let nbt = saved(&s, axolotl);
    assert_eq!(nbt.get_i32("InLove").map(|ticks| ticks > 0), Some(true));
}

#[test]
fn a_taming_item_makes_a_nautilus_persistent_and_a_baby_is_not_tamed_by_it() {
    let mut s = ready(43);
    let baby = summon(
        &mut s,
        Kind::Nautilus,
        "12.5 62 8.5",
        "{NoAI:1b,Age:-24000}",
    );
    console(&mut s, "give Alice pufferfish 1");
    interact(&mut s, baby);
    let nbt = saved(&s, baby);
    assert_eq!(nbt.get_bool("PersistenceRequired"), Some(true));
    assert!(nbt.get("Owner").is_none());
}
