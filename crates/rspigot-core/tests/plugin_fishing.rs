mod common;

use std::sync::Arc;
use std::time::Duration;

use common::*;
use rspigot_core::config::{GameConfig, LevelData, PluginSettings};
use rspigot_core::events::{Hand, Inbound, PlayerInput};
use rspigot_core::plugins::events::{PlayerFishPatch, PlayerFishState};
use rspigot_core::plugins::{EventKind, EventPatch, EventPayload, HandlerSpec};
use rspigot_core::ports::ChunkGenerator;
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::world::Dimension;
use rspigot_domain::{BiomeId, BlockPos, BlockStateId, ChunkPos, Rotation, Vec3};
use rspigot_entities::EntityKind as Kind;

struct Lake;

impl ChunkGenerator for Lake {
    fn generate(&self, dimension: &Dimension, pos: ChunkPos) -> ChunkColumn {
        let biome = rspigot_data::registries::biome_id("minecraft:plains").unwrap();
        let mut chunk = ChunkColumn::empty(pos, dimension.height, BiomeId(biome));
        for x in 0..16 {
            for z in 0..16 {
                for y in 60..=61 {
                    chunk.set_block(
                        BlockPos::new(pos.min_block_x() + x, y, pos.min_block_z() + z),
                        BlockStateId(1),
                    );
                }
                let lake = pos.x == 0 && pos.z == 0 && z >= 4;
                for y in 62..=63 {
                    let state = if lake {
                        rspigot_blocks::BlockKind::Water.default_state()
                    } else {
                        BlockStateId(1)
                    };
                    chunk.set_block(
                        BlockPos::new(pos.min_block_x() + x, y, pos.min_block_z() + z),
                        state,
                    );
                }
            }
        }
        chunk
    }

    fn spawn_point(&self, _: &Dimension) -> Vec3 {
        Vec3::new(0.5, 64.0, 0.5)
    }
}

fn plains_server(adjust: impl FnOnce(&mut GameConfig)) -> Server<Link> {
    let mut config = GameConfig {
        view_distance: 3,
        chunk_workers: 2,
        spawn_protection: 0,
        plugins: PluginSettings {
            decision_ticks: 1_000,
            ..PluginSettings::default()
        },
        ..GameConfig::default()
    };
    adjust(&mut config);
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
        Arc::new(Lake),
        StatusBoard::new(20),
    )
    .with_rng_seed(7);
    server.wait_for_spawn(Duration::from_secs(10));
    server
}

fn ready(handlers: Vec<HandlerSpec>) -> Scene {
    ready_with(handlers, |_| {})
}

fn ready_with(handlers: Vec<HandlerSpec>, adjust: impl FnOnce(&mut GameConfig)) -> Scene {
    let mut s = scene_on(plains_server(adjust), &["Alice"], handlers);
    stream_chunk_of(&mut s, BlockPos::new(1, 64, 1));
    s
}

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn cast(s: &mut Scene, pitch: f32) {
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::UseItem {
            hand: Hand::Main,
            sequence: 1,
            rotation: Rotation::new(0.0, pitch),
        },
    );
    s.tick(1);
}

fn fishing_events(s: &Scene) -> Vec<(u64, PlayerFishState)> {
    asked_for(&s.plugin, EventKind::PlayerFish)
        .into_iter()
        .filter_map(|asked| match asked.payload {
            EventPayload::PlayerFish(event) => Some((asked.id, event.state)),
            _ => None,
        })
        .collect()
}

fn allow_until(s: &mut Scene, ticks: u32, wanted: PlayerFishState) -> bool {
    let mut ruled = std::collections::HashSet::new();
    for _ in 0..ticks {
        s.tick(1);
        for (id, state) in fishing_events(s) {
            if state == wanted {
                return true;
            }
            if ruled.insert(id) {
                rule_on(&mut s.server, 1, id, false, None);
            }
        }
    }
    false
}

#[test]
fn casting_a_rod_asks_for_fishing_and_a_cancel_throws_no_hook() {
    let mut s = ready(vec![deciding(1, EventKind::PlayerFish)]);
    console(&mut s, "give Alice fishing_rod");
    cast(&mut s, -30.0);
    let asked = last_asked(&s.plugin, EventKind::PlayerFish);
    let EventPayload::PlayerFish(event) = &asked.payload else {
        panic!("expected a fish event");
    };
    assert_eq!(event.state, PlayerFishState::Fishing);
    assert_eq!(event.player.name, "Alice");
    assert!(event.caught.is_none());
    assert!(event.hand.is_some());
    assert_eq!(event.hook.kind, "minecraft:fishing_bobber");
    assert!(s.server.entities_of(Kind::FishingBobber).is_empty());
    rule_on(&mut s.server, 1, asked.id, true, None);
    s.tick(2);
    assert!(s.server.entities_of(Kind::FishingBobber).is_empty());

    cast(&mut s, -30.0);
    let asked = last_asked(&s.plugin, EventKind::PlayerFish);
    rule_on(&mut s.server, 1, asked.id, false, None);
    s.tick(2);
    assert_eq!(s.server.entities_of(Kind::FishingBobber).len(), 1);
}

#[test]
fn without_a_handler_a_cast_throws_a_hook_and_a_second_use_reels_it_in() {
    let mut s = ready(Vec::new());
    console(&mut s, "give Alice fishing_rod");
    cast(&mut s, -30.0);
    s.tick(2);
    assert_eq!(s.server.entities_of(Kind::FishingBobber).len(), 1);
    cast(&mut s, -30.0);
    s.tick(1);
    assert!(s.server.entities_of(Kind::FishingBobber).is_empty());
}

#[test]
fn reeling_in_a_flying_hook_asks_for_reel_in_and_a_cancel_keeps_it() {
    let mut s = ready(vec![deciding(1, EventKind::PlayerFish)]);
    console(&mut s, "give Alice fishing_rod");
    cast(&mut s, -60.0);
    let first = last_asked(&s.plugin, EventKind::PlayerFish);
    rule_on(&mut s.server, 1, first.id, false, None);
    s.tick(1);
    cast(&mut s, -60.0);
    let asked = last_asked(&s.plugin, EventKind::PlayerFish);
    let EventPayload::PlayerFish(event) = &asked.payload else {
        panic!("expected a fish event");
    };
    assert_eq!(event.state, PlayerFishState::ReelIn);
    assert!(event.hand.is_none());
    rule_on(&mut s.server, 1, asked.id, true, None);
    s.tick(1);
    assert_eq!(s.server.entities_of(Kind::FishingBobber).len(), 1);
    cast(&mut s, -60.0);
    let asked = last_asked(&s.plugin, EventKind::PlayerFish);
    rule_on(&mut s.server, 1, asked.id, false, None);
    s.tick(1);
    assert!(s.server.entities_of(Kind::FishingBobber).is_empty());
}

#[test]
fn a_hook_on_the_ground_asks_for_in_ground_when_reeled() {
    let mut s = ready(vec![deciding(1, EventKind::PlayerFish)]);
    console(&mut s, "give Alice fishing_rod");
    cast(&mut s, 80.0);
    let first = last_asked(&s.plugin, EventKind::PlayerFish);
    rule_on(&mut s.server, 1, first.id, false, None);
    s.tick(30);
    for _ in 0..4 {
        if s.server.entities_of(Kind::FishingBobber)[0].on_ground {
            break;
        }
        s.tick(1);
    }
    assert!(s.server.entities_of(Kind::FishingBobber)[0].on_ground);
    cast(&mut s, 80.0);
    let asked = last_asked(&s.plugin, EventKind::PlayerFish);
    let EventPayload::PlayerFish(event) = &asked.payload else {
        panic!("expected a fish event");
    };
    assert_eq!(event.state, PlayerFishState::InGround);
    rule_on(&mut s.server, 1, asked.id, false, None);
    s.tick(1);
    assert!(s.server.entities_of(Kind::FishingBobber).is_empty());
}

#[test]
fn a_hook_in_the_lake_bites_then_a_reel_catches_a_fish() {
    let mut s = ready(vec![deciding(1, EventKind::PlayerFish)]);
    console(&mut s, "give Alice fishing_rod");
    cast(&mut s, 15.0);
    assert!(!allow_until(&mut s, 5, PlayerFishState::Bite));
    s.tick(40);
    let hook = s.server.entities_of(Kind::FishingBobber);
    assert_eq!(hook.len(), 1);
    assert!(hook[0].position.y < 64.5, "{:?}", hook[0].position);
    assert!(allow_until(&mut s, 900, PlayerFishState::Bite));
    let bite = last_asked(&s.plugin, EventKind::PlayerFish);
    rule_on(&mut s.server, 1, bite.id, false, None);
    s.tick(1);
    cast(&mut s, 15.0);
    let asked = last_asked(&s.plugin, EventKind::PlayerFish);
    let EventPayload::PlayerFish(event) = &asked.payload else {
        panic!("expected a fish event");
    };
    assert_eq!(event.state, PlayerFishState::CaughtFish);
    let caught = event.caught.clone().expect("the caught item");
    assert_eq!(caught.kind, "minecraft:item");
    assert!((1..=6).contains(&event.exp_to_drop));
    rule_on(
        &mut s.server,
        1,
        asked.id,
        false,
        Some(EventPatch::PlayerFish(PlayerFishPatch {
            exp_to_drop: Some(4),
        })),
    );
    assert_eq!(s.server.entities_of(Kind::ExperienceOrb).len(), 1);
    assert_eq!(s.server.entities_of(Kind::Item).len(), 1);
    assert!(s.server.entities_of(Kind::FishingBobber).is_empty());
}

#[test]
fn a_bite_that_is_cancelled_lets_the_fish_get_away() {
    let mut s = ready(vec![deciding(1, EventKind::PlayerFish)]);
    console(&mut s, "give Alice fishing_rod");
    cast(&mut s, 15.0);
    assert!(allow_until(&mut s, 900, PlayerFishState::Bite));
    let bite = last_asked(&s.plugin, EventKind::PlayerFish);
    rule_on(&mut s.server, 1, bite.id, true, None);
    s.tick(2);
    let seen = fishing_events(&s)
        .into_iter()
        .filter(|(_, state)| *state == PlayerFishState::Bite)
        .count();
    assert_eq!(seen, 1);
}

#[test]
fn a_bite_nobody_reels_in_ends_as_a_failed_attempt() {
    let mut s = ready(vec![observe(1, EventKind::PlayerFish)]);
    console(&mut s, "give Alice fishing_rod");
    cast(&mut s, 15.0);
    s.tick(1_200);
    let states: Vec<PlayerFishState> = settled_for(&s.plugin, EventKind::PlayerFish)
        .into_iter()
        .filter_map(|fired| match fired.payload {
            EventPayload::PlayerFish(event) => Some(event.state),
            _ => None,
        })
        .collect();
    assert_eq!(states[0], PlayerFishState::Fishing);
    let bite = states
        .iter()
        .position(|state| *state == PlayerFishState::Bite)
        .expect("a bite");
    assert_eq!(states[bite + 1], PlayerFishState::FailedAttempt);
}

#[test]
fn a_hook_stuck_in_a_mob_asks_for_caught_entity_and_pulls_it() {
    let mut s = ready(vec![deciding(1, EventKind::PlayerFish)]);
    console(&mut s, "give Alice fishing_rod");
    console(
        &mut s,
        "summon minecraft:cow 0.5 64 3.5 {NoAI:1b,Silent:1b}",
    );
    let cow = s.server.entities_of(Kind::Cow)[0].id;
    cast(&mut s, 12.0);
    let first = last_asked(&s.plugin, EventKind::PlayerFish);
    rule_on(&mut s.server, 1, first.id, false, None);
    s.tick(20);
    cast(&mut s, 12.0);
    let asked = last_asked(&s.plugin, EventKind::PlayerFish);
    let EventPayload::PlayerFish(event) = &asked.payload else {
        panic!("expected a fish event");
    };
    assert_eq!(event.state, PlayerFishState::CaughtEntity);
    assert_eq!(event.caught.as_ref().map(|view| view.id), Some(cow));
    rule_on(&mut s.server, 1, asked.id, false, None);
    s.tick(1);
    assert!(s.server.entities_of(Kind::FishingBobber).is_empty());
    let cow = &s.server.entities_of(Kind::Cow)[0];
    assert!(cow.velocity.z < 0.0, "{:?}", cow.velocity);
}
