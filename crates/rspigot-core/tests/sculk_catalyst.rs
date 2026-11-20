mod common;

use std::sync::Arc;
use std::time::Duration;

use common::*;
use rspigot_blocks::BlockKind;
use rspigot_core::config::{GameConfig, LevelData};
use rspigot_core::events::{ClientEvent, Inbound, InputState, PlayerInput};
use rspigot_core::ports::ChunkGenerator;
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_domain::chunk::{BlockEntity, ChunkColumn};
use rspigot_domain::world::Dimension;
use rspigot_domain::{BiomeId, BlockPos, ChunkPos, Identifier, Vec3};
use rspigot_entities::EntityKind as Kind;
use rspigot_nbt::Compound;

const CATALYST: BlockPos = BlockPos::new(8, 64, 8);
const SENSOR: BlockPos = BlockPos::new(2, 64, 12);

struct Meadow;

impl ChunkGenerator for Meadow {
    fn generate(&self, dimension: &Dimension, pos: ChunkPos) -> ChunkColumn {
        let biome = rspigot_data::registries::biome_id("minecraft:plains").unwrap();
        let mut chunk = ChunkColumn::empty(pos, dimension.height, BiomeId(biome));
        for x in 0..16 {
            for z in 0..16 {
                for y in 60..=63 {
                    chunk.set_block(
                        BlockPos::new(pos.min_block_x() + x, y, pos.min_block_z() + z),
                        BlockKind::Stone.default_state(),
                    );
                }
            }
        }
        if CATALYST.chunk() == pos {
            chunk.set_block(CATALYST, BlockKind::SculkCatalyst.default_state());
            chunk.set_block(SENSOR, BlockKind::SculkSensor.default_state());
            chunk.set_block_entity(BlockEntity {
                pos: SENSOR,
                kind: Identifier::minecraft("sculk_sensor"),
                data: Compound::new(),
            });
        }
        chunk
    }

    fn spawn_point(&self, _: &Dimension) -> Vec3 {
        Vec3::new(8.5, 64.0, 12.5)
    }
}

fn meadow() -> Scene {
    let config = GameConfig {
        view_distance: 3,
        chunk_workers: 2,
        spawn_protection: 0,
        ..GameConfig::default()
    };
    let data = LevelData {
        spawn_mobs: false,
        day_time: 18_000,
        daylight_cycle: false,
        ..LevelData::default()
    };
    let mut server = Server::new(
        config,
        data,
        Arc::new(NoStore),
        Arc::new(Meadow),
        StatusBoard::new(20),
    )
    .with_rng_seed(7);
    server.wait_for_spawn(Duration::from_secs(10));
    let mut s = scene_on(server, &["Alice"], Vec::new());
    s.tick(30);
    s
}

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn sculk_blocks(s: &Scene) -> usize {
    let mut count = 0;
    for x in -4..=20 {
        for z in -4..=20 {
            for y in 60..=66 {
                let at = BlockPos::new(x, y, z);
                if s.server.block_state_at(at).and_then(|state| state.kind())
                    == Some(BlockKind::Sculk)
                {
                    count += 1;
                }
            }
        }
    }
    count
}

fn kill_a_zombie_by_hand(s: &mut Scene) {
    console(
        s,
        "summon minecraft:zombie 8.5 64 10.5 {NoAI:1b,Silent:1b,Health:0.5f,PersistenceRequired:1b}",
    );
    s.tick(2);
    let alice = s.players[0].clone();
    let target = s.server.entities_of(Kind::Zombie)[0].id;
    send_input(&mut s.server, &alice, PlayerInput::Attack(target));
    s.tick(1);
}

#[test]
fn a_mob_dying_near_a_catalyst_spreads_sculk_over_the_floor() {
    let mut s = meadow();
    assert_eq!(sculk_blocks(&s), 0);
    kill_a_zombie_by_hand(&mut s);
    s.tick(120);
    assert!(sculk_blocks(&s) > 0);
}

#[test]
fn the_catalyst_blooms_on_the_kill_then_settles() {
    let mut s = meadow();
    kill_a_zombie_by_hand(&mut s);
    let bloom = |s: &Scene| {
        s.server
            .block_state_at(CATALYST)
            .and_then(|state| state.property("bloom").map(str::to_owned))
    };
    assert_eq!(bloom(&s).as_deref(), Some("true"));
    s.tick(10);
    assert_eq!(bloom(&s).as_deref(), Some("false"));
}

#[test]
fn killing_a_mob_near_a_catalyst_completes_the_advancement() {
    let mut s = meadow();
    kill_a_zombie_by_hand(&mut s);
    assert!(
        s.server
            .done_advancements(s.players[0].id)
            .contains(&"minecraft:adventure/kill_mob_near_sculk_catalyst")
    );
}

#[test]
fn a_death_out_of_the_catalysts_range_spreads_nothing() {
    let mut s = meadow();
    console(
        &mut s,
        "summon minecraft:zombie 8.5 64 28.5 {NoAI:1b,Silent:1b,Health:0.5f,PersistenceRequired:1b}",
    );
    s.tick(2);
    console(&mut s, "kill @e[type=zombie]");
    s.tick(60);
    assert_eq!(sculk_blocks(&s), 0);
}

fn walk_past_the_sensor(s: &mut Scene, sneaking: bool) -> bool {
    let alice = s.players[0].clone();
    if sneaking {
        send_input(
            &mut s.server,
            &alice,
            PlayerInput::Input(InputState {
                shift: true,
                ..InputState::default()
            }),
        );
    }
    s.players[0].link.take();
    let mut at = Vec3::new(5.5, 64.0, 12.5);
    for _ in 0..12 {
        at = Vec3::new(at.x - 0.3, at.y, at.z);
        send_input(
            &mut s.server,
            &alice,
            PlayerInput::Move {
                position: Some(at),
                rotation: None,
                on_ground: true,
            },
        );
        s.tick(1);
    }
    s.tick(10);
    s.players[0].link.take().iter().any(|event| {
        matches!(
            event,
            ClientEvent::BlockUpdate { pos, state }
                if *pos == SENSOR && state.property("sculk_sensor_phase") == Some("active")
        )
    })
}

#[test]
fn a_sneaking_player_passing_a_sensor_earns_avoid_vibration() {
    let mut s = meadow();
    assert!(!walk_past_the_sensor(&mut s, true));
    assert!(
        s.server
            .done_advancements(s.players[0].id)
            .contains(&"minecraft:adventure/avoid_vibration")
    );
}

#[test]
fn a_walking_player_sets_the_sensor_off_and_earns_nothing() {
    let mut s = meadow();
    assert!(walk_past_the_sensor(&mut s, false));
    assert!(
        !s.server
            .done_advancements(s.players[0].id)
            .contains(&"minecraft:adventure/avoid_vibration")
    );
}
