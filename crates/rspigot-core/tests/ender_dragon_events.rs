mod common;

use std::sync::Arc;
use std::time::Duration;

use common::*;
use rspigot_blocks::BlockKind;
use rspigot_core::config::{GameConfig, LevelData, PluginSettings};
use rspigot_core::events::{Inbound, PlayerInput};
use rspigot_core::plugins::events::{
    BlockIgniteCause, EnderDragonChangePhasePatch, EnderDragonPhase, EntityRegainHealthRegainReason,
};
use rspigot_core::plugins::{EventKind, EventPatch, EventPayload, HandlerSpec};
use rspigot_core::ports::ChunkGenerator;
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::dragon::phase::DragonPhase;
use rspigot_domain::world::Dimension;
use rspigot_domain::{BiomeId, BlockPos, BlockStateId, ChunkPos, EntityId, Vec3};
use rspigot_entities::EntityKind as Kind;

struct Island;

impl ChunkGenerator for Island {
    fn generate(&self, dimension: &Dimension, pos: ChunkPos) -> ChunkColumn {
        let biome = rspigot_data::registries::biome_id("minecraft:plains").unwrap();
        let mut chunk = ChunkColumn::empty(pos, dimension.height, BiomeId(biome));
        for x in 0..16 {
            for z in 0..16 {
                let at = |y: i32| BlockPos::new(pos.min_block_x() + x, y, pos.min_block_z() + z);
                for y in 40..=62 {
                    chunk.set_block(at(y), BlockStateId(1));
                }
                chunk.set_block(at(63), BlockKind::EndStone.default_state());
            }
        }
        chunk
    }

    fn spawn_point(&self, _: &Dimension) -> Vec3 {
        Vec3::new(8.5, 64.0, 8.5)
    }
}

fn ready(seed: u64, handlers: Vec<HandlerSpec>) -> Scene {
    let config = GameConfig {
        view_distance: 8,
        chunk_workers: 2,
        spawn_protection: 0,
        plugins: PluginSettings {
            decision_ticks: 1_000,
            ..PluginSettings::default()
        },
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
        Arc::new(Island),
        StatusBoard::new(20),
    )
    .with_rng_seed(seed);
    server.wait_for_spawn(Duration::from_secs(10));
    let plugin = connect_with(&mut server, 1, "tester", handlers);
    let alice = join_with(
        &mut server,
        "Alice",
        rspigot_domain::player::ClientSettings {
            view_distance: 8,
            ..Default::default()
        },
    );
    server.deliver();
    alice.link.take();
    let mut s = Scene {
        server,
        plugin,
        players: vec![alice],
    };
    stream_chunk_of(&mut s, BlockPos::new(8, 64, 8));
    s.server.wait_for_spawn(Duration::from_secs(60));
    s.tick(20);
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

fn phase_events(s: &Scene) -> Vec<(Option<EnderDragonPhase>, EnderDragonPhase)> {
    settled_for(&s.plugin, EventKind::EnderDragonChangePhase)
        .into_iter()
        .filter_map(|event| match event.payload {
            EventPayload::EnderDragonChangePhase(change) => {
                Some((change.current_phase, change.new_phase))
            }
            _ => None,
        })
        .collect()
}

#[test]
fn every_phase_change_reaches_plugins_with_the_old_and_the_new_phase() {
    let mut s = ready(1, vec![observe(1, EventKind::EnderDragonChangePhase)]);
    summon(&mut s, Kind::EnderDragon, "8.5 100 8.5", "{DragonPhase:6}");
    s.tick(400);
    let changes = phase_events(&s);
    assert!(!changes.is_empty());
    assert_eq!(
        changes[0],
        (
            Some(EnderDragonPhase::SearchForBreathAttackTarget),
            EnderDragonPhase::LeavePortal
        ),
        "{changes:?}"
    );
    assert!(
        changes
            .iter()
            .any(|(from, to)| *from == Some(EnderDragonPhase::LeavePortal)
                && *to == EnderDragonPhase::ChargePlayer),
        "{changes:?}"
    );
}

#[test]
fn a_cancelled_phase_change_keeps_the_dragon_in_its_phase() {
    let mut s = ready(1, vec![deciding(1, EventKind::EnderDragonChangePhase)]);
    let dragon = summon(&mut s, Kind::EnderDragon, "8.5 100 8.5", "{DragonPhase:6}");
    s.tick(105);
    let asked = asked_for(&s.plugin, EventKind::EnderDragonChangePhase);
    assert_eq!(asked.len(), 1, "one decision at a time");
    rule_on(&mut s.server, 1, asked[0].id, true, None);
    s.tick(2);
    assert_eq!(
        s.server.dragon_phase_of(dragon),
        Some(DragonPhase::SittingScanning)
    );
}

#[test]
fn a_plugin_can_replace_the_new_phase() {
    let mut s = ready(1, vec![deciding(1, EventKind::EnderDragonChangePhase)]);
    let dragon = summon(&mut s, Kind::EnderDragon, "8.5 100 8.5", "{DragonPhase:6}");
    s.tick(105);
    let asked = asked_for(&s.plugin, EventKind::EnderDragonChangePhase);
    rule_on(
        &mut s.server,
        1,
        asked[0].id,
        false,
        Some(EventPatch::EnderDragonChangePhase(
            EnderDragonChangePhasePatch {
                new_phase: Some(EnderDragonPhase::Hover),
            },
        )),
    );
    s.tick(2);
    assert_eq!(
        s.server.dragon_phase_of(dragon),
        Some(DragonPhase::Hovering)
    );
}

#[test]
fn the_dragon_breaking_blocks_fires_entity_explode_with_a_zero_yield() {
    let mut s = ready(2, vec![observe(1, EventKind::EntityExplode)]);
    summon(&mut s, Kind::EnderDragon, "8.5 62 8.5", "{DragonPhase:10}");
    s.tick(4);
    let events = settled_for(&s.plugin, EventKind::EntityExplode);
    assert!(!events.is_empty());
    let EventPayload::EntityExplode(blast) = &events[0].payload else {
        panic!("an explode");
    };
    assert_eq!(blast.explosion_yield, 0.0);
    assert!(!blast.block_list.is_empty());
    assert_eq!(blast.entity_type, "minecraft:ender_dragon");
    assert_eq!(
        s.server
            .block_state_at(BlockPos::new(8, 62, 8))
            .and_then(|state| state.kind()),
        Some(BlockKind::Air)
    );
}

#[test]
fn cancelling_the_wall_break_keeps_the_blocks() {
    let mut s = ready(2, vec![deciding(1, EventKind::EntityExplode)]);
    summon(&mut s, Kind::EnderDragon, "8.5 62 8.5", "{DragonPhase:10}");
    s.tick(3);
    let asked = asked_for(&s.plugin, EventKind::EntityExplode);
    assert!(!asked.is_empty());
    rule_on(&mut s.server, 1, asked[0].id, true, None);
    s.tick(1);
    assert_eq!(
        s.server
            .block_state_at(BlockPos::new(8, 62, 8))
            .and_then(|state| state.kind()),
        Some(BlockKind::Stone)
    );
}

#[test]
fn a_crystal_in_range_heals_with_the_ender_crystal_regain_reason() {
    let mut s = ready(3, vec![observe(1, EventKind::EntityRegainHealth)]);
    summon(&mut s, Kind::EnderDragon, "8.5 100 8.5", "{Health:100f}");
    summon(&mut s, Kind::EndCrystal, "8.5 70 8.5", "{}");
    s.tick(300);
    let heals: Vec<_> = settled_for(&s.plugin, EventKind::EntityRegainHealth)
        .into_iter()
        .filter_map(|event| match event.payload {
            EventPayload::EntityRegainHealth(heal) => Some(heal),
            _ => None,
        })
        .collect();
    assert!(!heals.is_empty());
    assert!(heals.iter().all(|heal| heal.regain_reason
        == EntityRegainHealthRegainReason::EnderCrystal
        && (heal.amount - 1.0).abs() < 1e-9));
}

#[test]
fn a_crystal_blast_goes_through_explosion_prime_and_can_be_cancelled() {
    let mut s = ready(4, vec![deciding(1, EventKind::ExplosionPrime)]);
    console(&mut s, "gamemode creative Alice");
    console(&mut s, "give Alice minecraft:diamond_sword");
    s.tick(60);
    summon(&mut s, Kind::EndCrystal, "20.5 64 8.5", "{}");
    let crystal = s.server.entities_of(Kind::EndCrystal)[0].id;
    console(&mut s, "tp Alice 20.5 64 10.5");
    let alice = s.players[0].clone();
    send_input(&mut s.server, &alice, PlayerInput::Attack(crystal));
    s.tick(1);
    let asked = asked_for(&s.plugin, EventKind::ExplosionPrime);
    assert_eq!(asked.len(), 1);
    let EventPayload::ExplosionPrime(prime) = &asked[0].payload else {
        panic!("a prime");
    };
    assert_eq!(prime.radius, 6.0);
    assert!(!prime.fire);
    assert_eq!(prime.entity_type, "minecraft:end_crystal");
    rule_on(&mut s.server, 1, asked[0].id, true, None);
    s.tick(2);
    assert_eq!(s.server.entities_of(Kind::EndCrystal).len(), 1);
}

#[test]
fn crystals_igniting_their_block_fire_block_ignite_with_the_ender_crystal_cause() {
    let mut s = ready(5, vec![observe(1, EventKind::BlockIgnite)]);
    summon(&mut s, Kind::EnderDragon, "8.5 100 8.5", "{}");
    summon(&mut s, Kind::EndCrystal, "30.5 64 8.5", "{}");
    s.tick(3);
    let events = settled_for(&s.plugin, EventKind::BlockIgnite);
    let EventPayload::BlockIgnite(ignite) = &events[0].payload else {
        panic!("an ignite");
    };
    assert_eq!(ignite.cause, BlockIgniteCause::EnderCrystal);
    assert_eq!(ignite.block, BlockPos::new(30, 64, 8));
}

#[test]
fn the_dragon_death_reports_the_first_kill_experience() {
    let mut s = ready(6, vec![observe(1, EventKind::EntityDeath)]);
    console(&mut s, "gamemode creative Alice");
    console(&mut s, "give Alice minecraft:diamond_sword");
    s.tick(60);
    let dragon = summon(
        &mut s,
        Kind::EnderDragon,
        "8.5 100 8.5",
        "{DragonPhase:6,Health:5f}",
    );
    s.tick(5);
    s.tick(15);
    let at = s.server.dragon_parts(dragon).unwrap()[0];
    console(
        &mut s,
        &format!("tp Alice {} {} {}", at.x, at.y, at.z + 1.5),
    );
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Attack(EntityId(dragon.0 + 1)),
    );
    s.tick(2);
    let deaths = settled_for(&s.plugin, EventKind::EntityDeath);
    let EventPayload::EntityDeath(death) = &deaths[0].payload else {
        panic!("a death");
    };
    assert_eq!(death.entity_type, "minecraft:ender_dragon");
    assert_eq!(death.dropped_exp, 12_000);
}
