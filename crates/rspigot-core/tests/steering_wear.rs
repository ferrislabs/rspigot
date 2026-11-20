mod common;

use std::sync::Arc;
use std::time::Duration;

use common::*;
use rspigot_blocks::BlockKind;
use rspigot_core::config::{GameConfig, LevelData, PluginSettings};
use rspigot_core::events::{ClientEvent, Hand, Inbound, PlayerInput};
use rspigot_core::ports::ChunkGenerator;
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::world::Dimension;
use rspigot_domain::{BiomeId, BlockPos, ChunkPos, EntityId, Vec3};
use rspigot_entities::EntityKind as Kind;

struct Lake;

impl ChunkGenerator for Lake {
    fn generate(&self, dimension: &Dimension, pos: ChunkPos) -> ChunkColumn {
        let mut chunk = ChunkColumn::empty(pos, dimension.height, BiomeId(0));
        for x in 0..16 {
            for z in 0..16 {
                let (bx, bz) = (pos.min_block_x() + x, pos.min_block_z() + z);
                let at = |y: i32| BlockPos::new(bx, y, bz);
                for y in 55..=62 {
                    chunk.set_block(at(y), BlockKind::Netherrack.default_state());
                }
                if bx >= 4 {
                    chunk.set_block(at(63), BlockKind::Lava.default_state());
                } else {
                    chunk.set_block(at(63), BlockKind::Netherrack.default_state());
                }
            }
        }
        chunk
    }

    fn spawn_point(&self, _: &Dimension) -> Vec3 {
        Vec3::new(0.5, 64.0, 0.5)
    }
}

fn lake(seed: u64) -> Scene {
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
        Arc::new(Lake),
        StatusBoard::new(20),
    )
    .with_rng_seed(seed);
    server.wait_for_spawn(Duration::from_secs(10));
    let mut s = scene_on(server, &["Alice"], Vec::new());
    stream_chunk_of(&mut s, BlockPos::new(1, 64, 1));
    s
}

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn summon(s: &mut Scene, at: &str, nbt: &str) -> EntityId {
    console(s, &format!("summon minecraft:strider {at} {nbt}"));
    s.server.entities_of(Kind::Strider)[0].id
}

fn damage_in_slot(player: &Player, hotbar: usize) -> Option<i32> {
    player
        .link
        .take()
        .iter()
        .rev()
        .find_map(|event| match event {
            ClientEvent::InventorySlot { slot, stack, .. }
                if usize::from(*slot as u16) == 36 + hotbar =>
            {
                Some(stack.as_ref().map_or(i32::MAX, |stack| {
                    stack.components.get_i32("minecraft:damage").unwrap_or(0)
                }))
            }
            _ => None,
        })
}

fn boost(s: &mut Scene) {
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::UseItem {
            hand: Hand::Main,
            sequence: 1,
            rotation: rspigot_domain::Rotation::new(0.0, 0.0),
        },
    );
    s.tick(2);
}

#[test]
fn each_boost_wears_the_fungus_stick_by_one() {
    let mut s = lake(1);
    console(&mut s, "give Alice minecraft:warped_fungus_on_a_stick 1");
    let strider = summon(
        &mut s,
        "1.5 64 0.5",
        "{NoAI:1b,equipment:{saddle:{id:\"minecraft:saddle\",count:1}}}",
    );
    s.tick(2);
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Interact {
            target: strider,
            hand: Hand::Main,
            sneaking: false,
            location: Vec3::new(0.0, 0.5, 0.0),
        },
    );
    s.tick(2);
    s.players[0].link.take();
    boost(&mut s);
    assert_eq!(damage_in_slot(&s.players[0], 0), Some(1));
    s.tick(1000);
    s.players[0].link.take();
    boost(&mut s);
    assert_eq!(damage_in_slot(&s.players[0], 0), Some(2));
}
