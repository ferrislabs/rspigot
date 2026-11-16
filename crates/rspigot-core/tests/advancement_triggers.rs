mod common;

use std::sync::Arc;
use std::time::Duration;

use common::*;
use rspigot_blocks::BlockKind;
use rspigot_core::config::{GameConfig, LevelData};
use rspigot_core::events::{Hand, Inbound, PlayerInput};
use rspigot_core::plugins::PlayerOrder;
use rspigot_core::plugins::events::PlayerTeleportCause;
use rspigot_core::ports::ChunkGenerator;
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_domain::Identifier;
use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::world::Dimension;
use rspigot_domain::{BiomeId, BlockPos, BlockStateId, ChunkPos, Vec3};
use rspigot_entities::EntityKind as Kind;

struct Meadow {
    blocks: Vec<(BlockPos, BlockStateId)>,
}

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
        for (at, state) in &self.blocks {
            if at.chunk() == pos {
                chunk.set_block(*at, *state);
            }
        }
        chunk
    }

    fn spawn_point(&self, _: &Dimension) -> Vec3 {
        Vec3::new(8.5, 64.0, 8.5)
    }
}

fn meadow(blocks: Vec<(BlockPos, BlockStateId)>) -> Scene {
    let config = GameConfig {
        view_distance: 3,
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
        Arc::new(Meadow { blocks }),
        StatusBoard::new(20),
    )
    .with_rng_seed(7);
    server.wait_for_spawn(Duration::from_secs(10));
    scene_on(server, &["Alice"], Vec::new())
}

#[test]
fn standing_in_a_biome_ticks_its_adventuring_time_criterion() {
    let mut s = meadow(Vec::new());
    let alice = s.players[0].id;
    s.tick(25);
    let obtained = s
        .server
        .obtained_criteria(alice, "minecraft:adventure/adventuring_time");
    assert_eq!(obtained, vec!["minecraft:plains"]);
}

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

#[test]
fn spending_a_totem_completes_its_advancement() {
    let mut s = meadow(Vec::new());
    let alice = s.players[0].clone();
    console(&mut s, "give Alice totem_of_undying 1");
    s.act(to(&alice, PlayerOrder::Damage(100.0)));
    s.tick(2);
    assert!(
        s.server
            .done_advancements(alice.id)
            .contains(&"minecraft:adventure/totem_of_undying")
    );
}

#[test]
fn breeding_two_cows_completes_breed_an_animal_for_the_feeder() {
    let mut s = meadow(Vec::new());
    let alice = s.players[0].clone();
    stream_chunk_of(&mut s, BlockPos::new(9, 64, 9));
    console(&mut s, "give Alice wheat 5");
    console(&mut s, "summon cow 9.5 64 9.5");
    console(&mut s, "summon cow 10.5 64 9.5");
    for cow in s.server.entities_of(Kind::Cow) {
        send_input(
            &mut s.server,
            &alice,
            PlayerInput::Interact {
                target: cow.id,
                hand: Hand::Main,
                sneaking: false,
                location: Vec3::new(0.0, 0.5, 0.0),
            },
        );
    }
    for _ in 0..400 {
        s.tick(1);
        if s.server.entities_of(Kind::Cow).len() > 2 {
            break;
        }
    }
    assert_eq!(s.server.entities_of(Kind::Cow).len(), 3);
    assert!(
        s.server
            .done_advancements(alice.id)
            .contains(&"minecraft:husbandry/breed_an_animal")
    );
}

#[test]
fn entering_the_nether_completes_its_advancement() {
    let mut server = configured(|_| {});
    server.load_dimension(
        Identifier::minecraft("the_nether"),
        Dimension::nether(),
        Arc::new(Floor),
        Arc::new(NoStore),
    );
    let mut s = scene_on(server, &["Alice"], Vec::new());
    let alice = s.players[0].clone();
    s.server.change_dimension(
        alice.id,
        Identifier::minecraft("the_nether"),
        Vec3::new(8.5, 70.0, 8.5),
        None,
        PlayerTeleportCause::NetherPortal,
    );
    s.server.deliver();
    s.tick(3);
    assert!(
        s.server
            .done_advancements(alice.id)
            .contains(&"minecraft:story/enter_the_nether")
    );
}
