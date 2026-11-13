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

struct Beach {
    blocks: Vec<(BlockPos, BlockStateId)>,
}

impl ChunkGenerator for Beach {
    fn generate(&self, dimension: &Dimension, pos: ChunkPos) -> ChunkColumn {
        let biome = rspigot_data::registries::biome_id("minecraft:beach").unwrap();
        let mut chunk = ChunkColumn::empty(pos, dimension.height, BiomeId(biome));
        chunk.inhabited_time = 72_000_000;
        for x in 0..16 {
            for z in 0..16 {
                let at = |y: i32| BlockPos::new(pos.min_block_x() + x, y, pos.min_block_z() + z);
                for y in 50..=55 {
                    chunk.set_block(at(y), BlockKind::Sand.default_state());
                }
            }
        }
        for (pos, state) in &self.blocks {
            if pos.chunk() == chunk.pos() {
                chunk.set_block(*pos, *state);
            }
        }
        chunk
    }

    fn spawn_point(&self, _: &Dimension) -> Vec3 {
        Vec3::new(8.5, 64.0, 8.5)
    }
}

fn beach_server(seed: u64, day_time: i64, blocks: Vec<(BlockPos, BlockStateId)>) -> Server<Link> {
    let config = GameConfig {
        view_distance: 4,
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
        Arc::new(Beach { blocks }),
        StatusBoard::new(20),
    )
    .with_rng_seed(seed);
    server.wait_for_spawn(Duration::from_secs(10));
    server
}

fn egg(hatch: i32, eggs: i32) -> BlockStateId {
    BlockKind::TurtleEgg
        .state_with(&[("hatch", &hatch.to_string()), ("eggs", &eggs.to_string())])
        .unwrap()
}

fn ready(seed: u64, day_time: i64) -> Scene {
    ready_with(seed, day_time, Vec::new())
}

fn ready_with(seed: u64, day_time: i64, eggs: Vec<(BlockPos, BlockStateId)>) -> Scene {
    let mut s = scene_on(beach_server(seed, day_time, eggs), &["Alice"], Vec::new());
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

fn egg_at(s: &Scene, pos: BlockPos) -> Option<BlockStateId> {
    s.server
        .block_state_at(pos)
        .filter(|state| state.kind() == Some(BlockKind::TurtleEgg))
}

fn eggs_of(state: BlockStateId) -> i32 {
    state.property("eggs").unwrap().parse().unwrap()
}

#[test]
fn a_turtle_keeps_its_home_and_egg_across_a_save_and_starts_at_its_spawn_block() {
    let mut s = ready(1, 6_000);
    let turtle = summon(
        &mut s,
        Kind::Turtle,
        "3.5 56 4.5",
        "{home_pos:[I;10,56,12],has_egg:1b}",
    );
    s.tick(2);
    let nbt = saved(&s, turtle);
    assert_eq!(nbt.get_int_array("home_pos"), Some(&[10, 56, 12][..]));
    assert_eq!(nbt.get_bool("has_egg"), Some(true));
    let fresh = summon(&mut s, Kind::Turtle, "6.5 56 7.5", "");
    s.tick(1);
    assert_eq!(
        saved(&s, fresh).get_int_array("home_pos"),
        Some(&[6, 56, 7][..])
    );
}

#[test]
fn a_turtle_with_an_egg_digs_in_the_sand_and_buries_a_fresh_egg() {
    let mut s = ready(2, 6_000);
    let turtle = summon(
        &mut s,
        Kind::Turtle,
        "5.5 56 5.5",
        "{home_pos:[I;5,56,5],has_egg:1b}",
    );
    let mut found = None;
    for _ in 0..40 {
        s.tick(20);
        for x in -10..=20 {
            for z in -10..=20 {
                let pos = BlockPos::new(x, 56, z);
                if egg_at(&s, pos).is_some() {
                    found = Some(pos);
                }
            }
        }
        if found.is_some() {
            break;
        }
    }
    let pos = found.expect("an egg on the sand");
    let state = egg_at(&s, pos).unwrap();
    assert!((1..=4).contains(&eggs_of(state)));
    assert_eq!(state.property("hatch"), Some("0"));
    let nbt = saved(&s, turtle);
    assert_eq!(nbt.get_bool("has_egg"), Some(false));
}

#[test]
fn an_egg_cracks_in_three_stages_then_hatches_baby_turtles_homed_on_the_nest() {
    let nest = BlockPos::new(2, 56, 2);
    let mut s = ready_with(4, 21_500, vec![(nest, egg(0, 2))]);
    let mut stages = Vec::new();
    for _ in 0..600 {
        s.tick(20);
        match egg_at(&s, nest) {
            Some(state) => {
                let hatch = state.property("hatch").unwrap().to_owned();
                if stages.last() != Some(&hatch) {
                    stages.push(hatch);
                }
            }
            None => break,
        }
    }
    assert_eq!(stages, vec!["0", "1", "2"]);
    s.tick(2);
    let turtles = s.server.entities_of(Kind::Turtle);
    assert_eq!(turtles.len(), 2);
    for turtle in turtles {
        let nbt = saved(&s, turtle.id);
        assert_eq!(nbt.get_int_array("home_pos"), Some(&[2, 56, 2][..]));
        assert!(nbt.get_i32("Age").unwrap() < 0);
    }
}

#[test]
fn eggs_never_hatch_outside_the_dawn_window_in_the_time_it_takes_one_stage() {
    let nest = BlockPos::new(2, 56, 2);
    let mut s = ready_with(6, 6_000, vec![(nest, egg(0, 1))]);
    s.tick(300);
    assert_eq!(egg_at(&s, nest).unwrap().property("hatch"), Some("0"));
}

#[test]
fn cows_that_land_on_nests_trample_the_eggs() {
    let nests: Vec<BlockPos> = (0..6).map(|i| BlockPos::new(3 + 2 * i, 56, 3)).collect();
    let blocks: Vec<(BlockPos, BlockStateId)> =
        nests.iter().map(|nest| (*nest, egg(0, 4))).collect();
    let mut s = ready_with(8, 6_000, blocks);
    for nest in &nests {
        summon(
            &mut s,
            Kind::Cow,
            &format!("{}.5 58 {}.5", nest.x, nest.z),
            "",
        );
    }
    s.tick(400);
    let trampled = nests
        .iter()
        .filter(|nest| egg_at(&s, **nest).is_none_or(|state| eggs_of(state) < 4))
        .count();
    assert!(trampled >= 1, "{trampled}");
}

#[test]
fn a_turtle_never_tramples_an_egg() {
    let nest = BlockPos::new(3, 56, 3);
    let mut s = ready_with(9, 6_000, vec![(nest, egg(0, 4))]);
    summon(&mut s, Kind::Turtle, "3.5 57 3.5", "{NoAI:1b}");
    s.tick(600);
    assert_eq!(eggs_of(egg_at(&s, nest).unwrap()), 4);
}

#[test]
fn a_turtle_that_breeds_carries_an_egg_and_both_parents_wait_five_minutes() {
    let mut s = ready(10, 6_000);
    let first = summon(
        &mut s,
        Kind::Turtle,
        "4.5 56 4.5",
        "{InLove:600,home_pos:[I;4,56,4]}",
    );
    let second = summon(
        &mut s,
        Kind::Turtle,
        "5.2 56 4.5",
        "{InLove:600,home_pos:[I;4,56,4]}",
    );
    let mut bred = false;
    for _ in 0..40 {
        s.tick(10);
        let one = saved(&s, first);
        let two = saved(&s, second);
        if one.get_bool("has_egg") == Some(true) || two.get_bool("has_egg") == Some(true) {
            assert!(one.get_i32("Age").unwrap() >= 6_000 - 20);
            assert!(two.get_i32("Age").unwrap() >= 6_000 - 20);
            assert_eq!(one.get_i32("InLove"), Some(0));
            bred = true;
            break;
        }
    }
    assert!(bred);
}
