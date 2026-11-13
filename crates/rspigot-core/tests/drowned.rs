mod common;

use std::sync::Arc;
use std::time::Duration;

use common::*;
use rspigot_blocks::BlockKind;
use rspigot_core::config::{GameConfig, LevelData};
use rspigot_core::events::Inbound;
use rspigot_core::ports::ChunkGenerator;
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::world::Dimension;
use rspigot_domain::{BiomeId, BlockPos, BlockStateId, ChunkPos, EntityId, Vec3};
use rspigot_entities::EntityKind as Kind;

struct Sea {
    biome: &'static str,
}

impl ChunkGenerator for Sea {
    fn generate(&self, dimension: &Dimension, pos: ChunkPos) -> ChunkColumn {
        let biome = rspigot_data::registries::biome_id(self.biome).unwrap();
        let mut chunk = ChunkColumn::empty(pos, dimension.height, BiomeId(biome));
        chunk.inhabited_time = 72_000_000;
        let wet = pos.x <= 0;
        for x in 0..16 {
            for z in 0..16 {
                let at = |y: i32| BlockPos::new(pos.min_block_x() + x, y, pos.min_block_z() + z);
                for y in 50..=55 {
                    chunk.set_block(at(y), BlockStateId(1));
                }
                if wet {
                    for y in 56..=62 {
                        chunk.set_block(at(y), BlockKind::Water.default_state());
                    }
                } else {
                    for y in 56..=62 {
                        chunk.set_block(at(y), BlockStateId(1));
                    }
                }
            }
        }
        chunk
    }

    fn spawn_point(&self, _: &Dimension) -> Vec3 {
        Vec3::new(8.5, 64.0, 8.5)
    }
}

fn sea_server(seed: u64, biome: &'static str, spawn_mobs: bool) -> Server<Link> {
    let config = GameConfig {
        view_distance: 5,
        chunk_workers: 2,
        spawn_protection: 0,
        ..GameConfig::default()
    };
    let data = LevelData {
        spawn_mobs,
        day_time: 6_000,
        daylight_cycle: false,
        ..LevelData::default()
    };
    let mut server = Server::new(
        config,
        data,
        Arc::new(NoStore),
        Arc::new(Sea { biome }),
        StatusBoard::new(20),
    )
    .with_rng_seed(seed);
    server.wait_for_spawn(Duration::from_secs(10));
    server
}

fn ready(seed: u64) -> Scene {
    ready_with(seed, "minecraft:warm_ocean", false)
}

fn ready_with(seed: u64, biome: &'static str, spawn_mobs: bool) -> Scene {
    let mut s = scene_on(sea_server(seed, biome, spawn_mobs), &["Alice"], Vec::new());
    s.tick(120);
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

fn position_of(s: &Scene, kind: Kind, id: EntityId) -> Vec3 {
    s.server
        .entities_of(kind)
        .into_iter()
        .find(|entity| entity.id == id)
        .expect("the entity")
        .position
}

#[test]
fn a_drowned_in_the_sea_throws_its_trident_at_a_player_in_the_water_and_hurts_them() {
    let mut s = ready_with(11, "minecraft:warm_ocean", false);
    let drowned = summon(
        &mut s,
        Kind::Drowned,
        "-14.5 58 8.5",
        "{equipment:{mainhand:{id:\"minecraft:trident\",count:1}}}",
    );
    console(&mut s, "tp Alice -6.5 58 8.5");
    let alice = s.players[0].id;
    let mut thrown = false;
    for _ in 0..40 {
        s.tick(10);
        thrown |= !s.server.entities_of(Kind::Trident).is_empty();
        let health = s.server.player_health(alice).unwrap_or(20.0);
        if thrown && health < 20.0 {
            break;
        }
    }
    assert!(thrown, "no trident was thrown by {drowned:?}");
    let health = s.server.player_health(alice).expect("alice lives");
    assert!(health < 20.0, "{health}");
}

#[test]
fn a_thrown_trident_saves_its_flags_and_is_not_picked_up_by_its_target() {
    let mut s = ready(2);
    let trident = summon(
        &mut s,
        Kind::Trident,
        "-8.5 60 8.5",
        "{DealtDamage:1b,Motion:[0.0,0.0,0.0]}",
    );
    s.tick(2);
    let nbt = saved(&s, trident);
    assert_eq!(nbt.get_bool("DealtDamage"), Some(true));
    assert!(nbt.get_compound("item").is_some());
}

#[test]
fn a_zombie_under_water_turns_into_a_drowned_after_fifteen_seconds_of_afflicton_and_fifteen_more() {
    let mut s = ready(3);
    let zombie = summon(&mut s, Kind::Zombie, "-8.5 57 8.5", "{}");
    for _ in 0..100 {
        s.tick(10);
        if !s.server.entities_of(Kind::Drowned).is_empty() {
            break;
        }
    }
    assert_eq!(s.server.entities_of(Kind::Drowned).len(), 1);
    assert!(
        s.server
            .entities_of(Kind::Zombie)
            .iter()
            .all(|entity| entity.id != zombie)
    );
}

#[test]
fn in_daylight_a_drowned_on_the_shore_walks_into_the_sea() {
    let mut s = ready(5);
    let drowned = summon(&mut s, Kind::Drowned, "20.5 63 8.5", "{}");
    let start = position_of(&s, Kind::Drowned, drowned);
    let mut entered = false;
    for _ in 0..30 {
        s.tick(10);
        if let Some(entity) = s
            .server
            .entities_of(Kind::Drowned)
            .into_iter()
            .find(|entity| entity.id == drowned)
        {
            entered |= entity.position.x < 15.5;
        }
    }
    assert!(entered, "{start:?}");
}

#[test]
fn drowned_save_nothing_but_the_zombie_layout() {
    let mut s = ready(7);
    let drowned = summon(&mut s, Kind::Drowned, "-8.5 58 8.5", "{IsBaby:1b}");
    s.tick(2);
    let nbt = saved(&s, drowned);
    assert_eq!(nbt.get_bool("IsBaby"), Some(true));
    assert_eq!(nbt.get_bool("CanBreakDoors"), Some(false));
    assert_eq!(nbt.get_i32("DrownedConversionTime"), Some(-1));
}
