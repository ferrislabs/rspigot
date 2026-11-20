mod common;

use std::sync::Arc;
use std::time::Duration;

use common::*;
use rspigot_core::config::{GameConfig, LevelData, PluginSettings};
use rspigot_core::events::ClientEvent;
use rspigot_core::events::{Hand, Inbound, PlayerInput};
use rspigot_core::plugins::events::PlayerTeleportCause;
use rspigot_core::plugins::{
    Answer, EventKind, EventPayload, HandlerSpec, ItemSpec, PlayerState, Question,
};
use rspigot_core::ports::ChunkGenerator;
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::world::Dimension;
use rspigot_domain::{BiomeId, BlockPos, BlockStateId, ChunkPos, Rotation, Vec3};
use rspigot_entities::EntityKind as Kind;

struct Plains;

impl ChunkGenerator for Plains {
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
        Arc::new(Plains),
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

fn state(s: &mut Scene) -> PlayerState {
    let Answer::Player(state) = s.ask(Question::Player(s.players[0].id)) else {
        panic!("expected Alice");
    };
    state
}

fn slot_zero(s: &mut Scene) -> Option<ItemSpec> {
    let Answer::Inventory(inventory) = s.ask(Question::Inventory(s.players[0].id)) else {
        panic!("expected Alice's inventory");
    };
    inventory.slots[0].clone()
}

fn armed(handlers: Vec<HandlerSpec>, mode: &str) -> Scene {
    let mut s = ready(handlers);
    console(&mut s, &format!("gamemode {mode} Alice"));
    console(&mut s, "give Alice ender_pearl 5");
    s.players[0].link.take();
    s
}

fn throw(s: &mut Scene) {
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::UseItem {
            hand: Hand::Main,
            sequence: 1,
            rotation: Rotation::new(0.0, 60.0),
        },
    );
}

fn cooldowns(s: &Scene) -> Vec<(String, i32)> {
    s.players[0]
        .link
        .take()
        .into_iter()
        .filter_map(|event| match event {
            ClientEvent::Cooldown { group, ticks } => Some((group, ticks)),
            _ => None,
        })
        .collect()
}

#[test]
fn a_thrown_pearl_spends_one_item_and_starts_a_twenty_tick_cooldown() {
    let mut s = armed(Vec::new(), "survival");
    throw(&mut s);
    s.tick(1);
    assert_eq!(s.server.entities_of(Kind::EnderPearl).len(), 1);
    assert_eq!(
        slot_zero(&mut s),
        Some(ItemSpec::of("minecraft:ender_pearl", 4))
    );
    assert_eq!(
        cooldowns(&s),
        vec![("minecraft:ender_pearl".to_owned(), 20)]
    );
}

#[test]
fn a_pearl_on_cooldown_does_nothing_and_the_cooldown_ends_after_twenty_ticks() {
    let mut s = armed(Vec::new(), "survival");
    throw(&mut s);
    s.tick(1);
    cooldowns(&s);
    throw(&mut s);
    s.tick(1);
    assert_eq!(
        slot_zero(&mut s),
        Some(ItemSpec::of("minecraft:ender_pearl", 4))
    );
    assert!(cooldowns(&s).is_empty());
    s.tick(25);
    assert!(cooldowns(&s).contains(&("minecraft:ender_pearl".to_owned(), 0)));
    throw(&mut s);
    s.tick(1);
    assert_eq!(
        slot_zero(&mut s),
        Some(ItemSpec::of("minecraft:ender_pearl", 3))
    );
}

#[test]
fn creative_players_keep_their_pearls_but_still_wait() {
    let mut s = armed(Vec::new(), "creative");
    throw(&mut s);
    s.tick(1);
    assert_eq!(
        slot_zero(&mut s),
        Some(ItemSpec::of("minecraft:ender_pearl", 5))
    );
    assert_eq!(
        cooldowns(&s),
        vec![("minecraft:ender_pearl".to_owned(), 20)]
    );
}

#[test]
fn a_landing_pearl_takes_the_owner_there_for_five_damage() {
    let mut s = armed(Vec::new(), "survival");
    let before = state(&mut s);
    throw(&mut s);
    for _ in 0..40 {
        s.tick(1);
        if s.server.entities_of(Kind::EnderPearl).is_empty() {
            break;
        }
    }
    let after = state(&mut s);
    assert!(after.player.position.z > before.player.position.z + 1.0);
    assert!((after.player.position.y - 64.0).abs() < 1.0);
    assert_eq!(after.health, before.health - 5.0);
    assert!(s.server.entities_of(Kind::EnderPearl).is_empty());
}

#[test]
fn a_landing_pearl_plays_the_player_teleport_sound() {
    let mut s = armed(Vec::new(), "survival");
    throw(&mut s);
    s.tick(40);
    let sounds: Vec<&str> = s.players[0]
        .link
        .take()
        .into_iter()
        .filter_map(|event| match event {
            ClientEvent::Sound { sound, .. } => Some(sound),
            _ => None,
        })
        .collect();
    assert!(
        sounds.contains(&"minecraft:entity.player.teleport"),
        "{sounds:?}"
    );
    assert!(!sounds.contains(&"minecraft:entity.enderman.teleport"));
}

#[test]
fn a_landing_pearl_does_not_hurt_a_creative_owner() {
    let mut s = armed(Vec::new(), "creative");
    let before = state(&mut s);
    throw(&mut s);
    s.tick(40);
    let after = state(&mut s);
    assert!(after.player.position.z > before.player.position.z + 1.0);
    assert_eq!(after.health, before.health);
}

#[test]
fn a_plugin_can_cancel_the_pearl_teleport() {
    let mut s = armed(vec![deciding(1, EventKind::PlayerTeleport)], "survival");
    let before = state(&mut s);
    throw(&mut s);
    s.tick(40);
    let asked = last_asked(&s.plugin, EventKind::PlayerTeleport);
    let EventPayload::PlayerTeleport(event) = &asked.payload else {
        panic!("expected a teleport event");
    };
    assert_eq!(event.cause, PlayerTeleportCause::EnderPearl);
    rule_on(&mut s.server, 1, asked.id, true, None);
    s.tick(2);
    let after = state(&mut s);
    assert_eq!(after.player.position, before.player.position);
    assert_eq!(after.health, before.health);
}

#[test]
fn a_pearl_vanishes_when_its_owner_leaves() {
    let mut s = scene_on(plains_server(|_| {}), &["Alice", "Bob"], Vec::new());
    stream_chunk_of(&mut s, BlockPos::new(1, 64, 1));
    console(&mut s, "gamemode survival Alice");
    console(&mut s, "give Alice ender_pearl 5");
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::UseItem {
            hand: Hand::Main,
            sequence: 1,
            rotation: Rotation::new(0.0, -90.0),
        },
    );
    s.tick(1);
    assert_eq!(s.server.entities_of(Kind::EnderPearl).len(), 1);
    s.server.handle(leave_of(&alice));
    s.server.deliver();
    s.tick(2);
    assert!(s.server.entities_of(Kind::EnderPearl).is_empty());
}

fn pearls_after_the_owner_dies(vanish: bool) -> usize {
    let mut s = ready_with(Vec::new(), |config| {
        config.rules.ender_pearls_vanish_on_death = vanish;
    });
    console(&mut s, "gamemode survival Alice");
    console(&mut s, "give Alice ender_pearl 5");
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::UseItem {
            hand: Hand::Main,
            sequence: 1,
            rotation: Rotation::new(0.0, -90.0),
        },
    );
    s.tick(1);
    assert_eq!(s.server.entities_of(Kind::EnderPearl).len(), 1);
    console(&mut s, "kill Alice");
    s.tick(2);
    s.server.entities_of(Kind::EnderPearl).len()
}

#[test]
fn the_vanish_on_death_rule_decides_whether_a_pearl_outlives_a_dead_owner() {
    assert_eq!(pearls_after_the_owner_dies(true), 0);
    assert_eq!(pearls_after_the_owner_dies(false), 1);
}
