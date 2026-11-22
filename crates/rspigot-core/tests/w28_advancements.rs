mod common;

use std::sync::Arc;
use std::time::Duration;

use common::*;
use rspigot_blocks::BlockKind;
use rspigot_core::config::{GameConfig, LevelData};
use rspigot_core::events::{Direction, Hand, Inbound, PlayerInput};
use rspigot_core::ports::ChunkGenerator;
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_domain::chunk::{BlockEntity, ChunkColumn};
use rspigot_domain::world::Dimension;
use rspigot_domain::{BiomeId, BlockPos, BlockStateId, ChunkPos, Identifier, Vec3};
use rspigot_nbt::{Compound, Tag};

const HIVE: BlockPos = BlockPos::new(6, 66, 6);

struct Site {
    blocks: Vec<(BlockPos, BlockStateId)>,
    structure: Option<&'static str>,
    biome: &'static str,
}

fn packed(pos: ChunkPos) -> i64 {
    i64::from(pos.x as u32) | (i64::from(pos.z) << 32)
}

fn structure_extra(name: &str) -> Compound {
    let piece = Compound::new().with("BB", Tag::IntArray(vec![0, 50, 0, 15, 90, 15]));
    let start = Compound::new()
        .with("ChunkX", 0)
        .with("ChunkZ", 0)
        .with("Children", Tag::List(vec![Tag::Compound(piece)]));
    Compound::new().with(
        "structures",
        Compound::new()
            .with("starts", Compound::new().with(name, Tag::Compound(start)))
            .with(
                "References",
                Compound::new().with(name, Tag::LongArray(vec![packed(ChunkPos::new(0, 0))])),
            ),
    )
}

impl ChunkGenerator for Site {
    fn generate(&self, dimension: &Dimension, pos: ChunkPos) -> ChunkColumn {
        let biome = rspigot_data::registries::biome_id(self.biome).unwrap();
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
        if pos == ChunkPos::new(0, 0) {
            for (at, block) in &self.blocks {
                chunk.set_block(*at, *block);
            }
            if self.blocks.iter().any(|(at, _)| *at == HIVE) {
                chunk.set_block_entity(BlockEntity {
                    pos: HIVE,
                    kind: Identifier::minecraft("beehive"),
                    data: Compound::new().with("bees", Tag::List(Vec::new())),
                });
            }
            if let Some(name) = self.structure {
                chunk.extra = structure_extra(name);
            }
        }
        chunk
    }

    fn spawn_point(&self, _: &Dimension) -> Vec3 {
        Vec3::new(8.5, 64.0, 8.5)
    }
}

fn scene_of(site: Site) -> Scene {
    let config = GameConfig {
        view_distance: 4,
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
        Arc::new(site),
        StatusBoard::new(20),
    )
    .with_rng_seed(3);
    server.wait_for_spawn(Duration::from_secs(10));
    let mut s = scene_on(server, &["Alice"], Vec::new());
    stream_chunk_of(&mut s, BlockPos::new(8, 64, 8));
    s
}

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn state(kind: BlockKind, properties: &[(&str, &str)]) -> BlockStateId {
    kind.state_with(properties).expect("a block state")
}

#[test]
fn standing_in_a_fortress_piece_grants_find_a_fortress() {
    let mut s = scene_of(Site {
        blocks: Vec::new(),
        structure: Some("minecraft:fortress"),
        biome: "minecraft:plains",
    });
    s.tick(25);
    let alice = s.players[0].id;
    assert!(
        s.server
            .done_advancements(alice)
            .contains(&"minecraft:nether/find_fortress")
    );
}

#[test]
fn standing_outside_every_structure_grants_nothing() {
    let mut s = scene_of(Site {
        blocks: Vec::new(),
        structure: None,
        biome: "minecraft:plains",
    });
    s.tick(25);
    let alice = s.players[0].id;
    assert!(
        !s.server
            .done_advancements(alice)
            .contains(&"minecraft:nether/find_fortress")
    );
}

fn harvest_honey(smoke: bool) -> bool {
    let mut blocks = vec![(
        HIVE,
        state(
            BlockKind::Beehive,
            &[("facing", "south"), ("honey_level", "5")],
        ),
    )];
    if smoke {
        blocks.push((
            BlockPos::new(6, 64, 6),
            state(BlockKind::Campfire, &[("lit", "true")]),
        ));
    }
    let mut s = scene_of(Site {
        blocks,
        structure: None,
        biome: "minecraft:plains",
    });
    s.tick(20);
    console(&mut s, "give Alice minecraft:glass_bottle 1");
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::UseItemOn {
            hand: Hand::Main,
            pos: HIVE,
            face: Direction::Up,
            cursor: Vec3::new(0.5, 1.0, 0.5),
            inside_block: false,
            sequence: 5,
        },
    );
    s.tick(2);
    s.server
        .done_advancements(alice.id)
        .contains(&"minecraft:husbandry/safely_harvest_honey")
}

#[test]
fn harvesting_honey_over_a_campfire_grants_safely_harvest_honey() {
    assert!(harvest_honey(true));
}

#[test]
fn harvesting_honey_without_smoke_grants_nothing() {
    assert!(!harvest_honey(false));
}

#[test]
fn standing_in_a_bastion_piece_grants_find_a_bastion() {
    let mut s = scene_of(Site {
        blocks: Vec::new(),
        structure: Some("minecraft:bastion_remnant"),
        biome: "minecraft:plains",
    });
    s.tick(25);
    let alice = s.players[0].id;
    assert!(
        s.server
            .done_advancements(alice)
            .contains(&"minecraft:nether/find_bastion")
    );
}

const JUKEBOX: BlockPos = BlockPos::new(6, 64, 6);

fn play_disc(biome: &'static str) -> bool {
    let mut s = scene_of(Site {
        blocks: vec![(
            JUKEBOX,
            state(BlockKind::Jukebox, &[("has_record", "false")]),
        )],
        structure: None,
        biome,
    });
    s.tick(20);
    console(&mut s, "give Alice minecraft:music_disc_cat 1");
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::UseItemOn {
            hand: Hand::Main,
            pos: JUKEBOX,
            face: Direction::Up,
            cursor: Vec3::new(0.5, 1.0, 0.5),
            inside_block: false,
            sequence: 5,
        },
    );
    s.tick(2);
    s.server
        .done_advancements(alice.id)
        .contains(&"minecraft:adventure/play_jukebox_in_meadows")
}

#[test]
fn a_disc_in_a_jukebox_in_a_meadow_grants_play_jukebox_in_meadows() {
    assert!(play_disc("minecraft:meadow"));
}

#[test]
fn a_disc_in_a_jukebox_outside_a_meadow_grants_nothing() {
    assert!(!play_disc("minecraft:plains"));
}
