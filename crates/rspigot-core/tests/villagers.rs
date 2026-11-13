mod common;

use std::sync::Arc;
use std::time::Duration;

use common::*;
use rspigot_blocks::BlockKind;
use rspigot_core::config::{GameConfig, LevelData};
use rspigot_core::events::{Inbound, PlayerInput};
use rspigot_core::ports::ChunkGenerator;
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::world::Dimension;
use rspigot_domain::{BiomeId, BlockPos, BlockStateId, ChunkPos, EntityId, Vec3};
use rspigot_entities::EntityKind as Kind;
use rspigot_nbt::Compound;

const LECTERN: BlockPos = BlockPos::new(12, 64, 10);
const BEDS: [i32; 3] = [14, 6, 3];

struct Village;

impl ChunkGenerator for Village {
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
        if pos == ChunkPos::new(0, 0) {
            chunk.set_block(LECTERN, BlockKind::Lectern.default_state());
            let bed = BlockKind::RedBed.default_state();
            let head = bed.with_property("part", "head").unwrap();
            let foot = bed.with_property("part", "foot").unwrap();
            for z in BEDS {
                chunk.set_block(
                    BlockPos::new(12, 64, z),
                    head.with_property("facing", "east").unwrap(),
                );
                chunk.set_block(
                    BlockPos::new(11, 64, z),
                    foot.with_property("facing", "east").unwrap(),
                );
            }
        }
        chunk
    }

    fn spawn_point(&self, _: &Dimension) -> Vec3 {
        Vec3::new(0.5, 64.0, 0.5)
    }
}

fn village_server(seed: u64, day_time: i64) -> Server<Link> {
    let config = GameConfig {
        view_distance: 3,
        chunk_workers: 2,
        spawn_protection: 0,
        ..GameConfig::default()
    };
    let data = LevelData {
        spawn_mobs: false,
        day_time,
        daylight_cycle: false,
        ..LevelData::default()
    };
    let mut server = Server::new(
        config,
        data,
        Arc::new(NoStore),
        Arc::new(Village),
        StatusBoard::new(20),
    )
    .with_rng_seed(seed);
    server.wait_for_spawn(Duration::from_secs(10));
    server
}

fn village(seed: u64) -> Scene {
    village_at(seed, 6_000)
}

fn village_at(seed: u64, day_time: i64) -> Scene {
    let mut s = scene_on(village_server(seed, day_time), &["Alice"], Vec::new());
    stream_chunk_of(&mut s, BlockPos::new(1, 64, 1));
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

fn ready(mut s: Scene) -> Scene {
    stream_chunk_of(&mut s, BlockPos::new(1, 64, 1));
    s
}

fn saved(s: &Scene, id: EntityId) -> Compound {
    s.server.saved_entity(id).expect("a saved entity")
}

fn memories(s: &Scene, id: EntityId) -> Compound {
    saved(s, id)
        .get_compound("Brain")
        .and_then(|brain| brain.get_compound("memories").cloned())
        .unwrap_or_default()
}

fn profession(s: &Scene, id: EntityId) -> String {
    saved(s, id)
        .get_compound("VillagerData")
        .and_then(|data| data.get_str("profession").map(str::to_owned))
        .expect("a profession")
}

#[test]
fn a_summoned_villager_keeps_its_data_food_experience_and_gossip() {
    let mut s = ready(scene(&["Alice"]));
    let villager = summon(
        &mut s,
        Kind::Villager,
        "5.5 64 5.5",
        "{VillagerData:{type:\"minecraft:desert\",profession:\"minecraft:farmer\",level:3},FoodLevel:7b,Xp:90,LastRestock:15L,RestocksToday:1,Gossips:[{Target:[I;1,2,3,4],Type:\"major_positive\",Value:10}]}",
    );
    s.tick(2);
    let nbt = saved(&s, villager);
    let data = nbt.get_compound("VillagerData").unwrap();
    assert_eq!(data.get_str("type"), Some("minecraft:desert"));
    assert_eq!(data.get_str("profession"), Some("minecraft:farmer"));
    assert_eq!(data.get_i32("level"), Some(3));
    assert_eq!(nbt.get_i32("Xp"), Some(90));
    assert_eq!(nbt.get_i64("LastRestock"), Some(15));
    assert_eq!(nbt.get_i32("RestocksToday"), Some(1));
    assert!(nbt.get_list("Gossips").is_some_and(|list| list.len() == 1));
}

#[test]
fn a_jobless_villager_claims_a_nearby_lectern_and_becomes_a_librarian() {
    let mut s = village(11);
    let villager = summon(&mut s, Kind::Villager, "8.5 64 10.5", "{}");
    assert_eq!(profession(&s, villager), "minecraft:none");
    s.tick(900);
    assert_eq!(profession(&s, villager), "minecraft:librarian");
    let memories = memories(&s, villager);
    assert!(memories.contains_key("minecraft:job_site"), "{memories:?}");
}

#[test]
fn a_villager_claims_a_free_bed_as_its_home() {
    let mut s = village(5);
    let villager = summon(&mut s, Kind::Villager, "8.5 64 10.5", "{}");
    s.tick(900);
    assert!(memories(&s, villager).contains_key("minecraft:home"));
}

#[test]
fn only_one_of_two_villagers_keeps_a_single_workstation() {
    let mut s = village(21);
    let first = summon(&mut s, Kind::Villager, "8.5 64 10.5", "{}");
    let second = summon(&mut s, Kind::Villager, "8.5 64 12.5", "{}");
    s.tick(1200);
    let librarians = [first, second]
        .into_iter()
        .filter(|id| profession(&s, *id) == "minecraft:librarian")
        .count();
    assert_eq!(librarians, 1);
}

#[test]
fn a_hurt_villager_panics_and_remembers_who_hurt_it() {
    let mut s = village(2);
    let villager = summon(&mut s, Kind::Villager, "1.5 64 1.5", "{NoAI:1b}");
    let before = saved(&s, villager).get_f64("Health").unwrap();
    let alice = s.players[0].clone();
    send_input(&mut s.server, &alice, PlayerInput::Attack(villager));
    s.tick(5);
    let after = saved(&s, villager).get_f64("Health").unwrap();
    assert!(after < before, "{after} {before}");
    let gossips = saved(&s, villager);
    assert!(
        gossips
            .get_list("Gossips")
            .is_some_and(|list| !list.is_empty())
    );
}

#[test]
fn a_trader_despawns_when_its_delay_runs_out() {
    let mut s = ready(scene(&["Alice"]));
    let trader = summon(
        &mut s,
        Kind::WanderingTrader,
        "5.5 64 5.5",
        "{DespawnDelay:3}",
    );
    s.tick(10);
    assert!(s.server.saved_entity(trader).is_none());
}

#[test]
fn a_trader_keeps_its_delay_and_wander_target_in_the_save() {
    let mut s = ready(scene(&["Alice"]));
    let trader = summon(
        &mut s,
        Kind::WanderingTrader,
        "5.5 64 5.5",
        "{DespawnDelay:5000,wander_target:[I;10,64,10]}",
    );
    s.tick(3);
    let nbt = saved(&s, trader);
    assert!(
        nbt.get_i32("DespawnDelay")
            .is_some_and(|delay| delay < 5000)
    );
    assert!(nbt.contains_key("wander_target"));
}

const HOME: &str = "{Brain:{memories:{\"minecraft:home\":{value:{dimension:\"minecraft:overworld\",pos:[I;12,64,14]}}}}}";

#[test]
fn a_villager_with_a_home_goes_to_bed_at_night() {
    let mut s = village_at(3, 13_000);
    let villager = summon(&mut s, Kind::Villager, "9.5 64 14.5", HOME);
    s.tick(400);
    let nbt = saved(&s, villager);
    let slept = nbt
        .get_compound("Brain")
        .and_then(|brain| brain.get_compound("memories").cloned())
        .is_some_and(|memories| memories.contains_key("minecraft:last_slept"));
    assert!(slept, "{nbt:?}");
}

#[test]
fn two_fed_villagers_breed_a_baby_into_a_free_bed() {
    let mut s = village_at(8, 6_000);
    let nbt = "{FoodLevel:12b}";
    summon(&mut s, Kind::Villager, "8.5 64 12.5", nbt);
    summon(&mut s, Kind::Villager, "8.5 64 14.5", nbt);
    s.tick(1500);
    let villagers = s.server.entities_of(Kind::Villager);
    assert!(villagers.len() >= 3, "{}", villagers.len());
}
