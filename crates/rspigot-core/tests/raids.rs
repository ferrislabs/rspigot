mod common;

use common::*;
use rspigot_core::config::PluginSettings;
use rspigot_core::events::{ClientEvent, Inbound, PlayerInput};
use rspigot_core::plugins::{
    Answer, BlockOrder, EntityOrder, EventKind, EventPayload, Order, Question,
};
use rspigot_domain::boss_event::BossUpdate;
use rspigot_domain::raid::RaidStatus;
use rspigot_domain::world::Difficulty;
use rspigot_domain::{BlockPos, Vec3};
use rspigot_entities::EntityKind as Kind;

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn set(s: &mut Scene, pos: BlockPos, kind: &str) {
    s.act(Order::Block {
        pos,
        order: BlockOrder::SetWith {
            kind: kind.to_owned(),
            properties: Vec::new(),
        },
    });
}

fn village_scene(difficulty: Difficulty) -> Scene {
    let server = configured_level(
        |config| config.difficulty = difficulty,
        |level| {
            level.day_time = 4_000;
            level.daylight_cycle = false;
        },
    )
    .with_rng_seed(11);
    let mut s = scene_on(server, &["Alice"], Vec::new());
    stream_chunk_of(&mut s, BlockPos::new(1, 64, 1));
    set(&mut s, BlockPos::new(8, 64, 8), "minecraft:lectern");
    console(
        &mut s,
        "summon minecraft:villager 7.5 64 8.5 {VillagerData:{type:\"minecraft:plains\",profession:\"minecraft:librarian\",level:2},Xp:20}",
    );
    s.tick(200);
    s
}

fn bar_events(s: &Scene) -> Vec<BossUpdate> {
    s.players
        .iter()
        .flat_map(|player| player.link.take())
        .filter_map(|event| match event {
            ClientEvent::BossEvent { update, .. } => Some(update),
            _ => None,
        })
        .collect()
}

#[test]
fn a_bad_omen_in_a_village_becomes_a_raid_that_spawns_its_first_wave() {
    let mut s = village_scene(Difficulty::Normal);
    console(&mut s, "effect give Alice minecraft:bad_omen 120 0");
    s.tick(5);
    assert!(s.server.raids().is_empty());
    s.tick(640);
    assert_eq!(s.server.raids().ids().len(), 1);
    let raid = s.server.raids().iter().next().unwrap();
    assert_eq!(raid.raid_omen_level(), 1);
    assert_eq!(raid.num_groups(), 5);
    assert_eq!(raid.groups_spawned(), 0);
    let shown = bar_events(&s);
    assert!(
        shown
            .iter()
            .any(|update| matches!(update, BossUpdate::Add { .. })),
        "{shown:?}"
    );
    s.tick(320);
    let raid = s.server.raids().iter().next().unwrap();
    assert!(raid.is_started());
    assert_eq!(raid.groups_spawned(), 1);
    let pillagers = s.server.entities_of(Kind::Pillager).len();
    assert!(pillagers >= 4, "{pillagers}");
    assert_eq!(
        raid.total_raiders_alive() as usize,
        s.server.entities_of(Kind::Pillager).len()
            + s.server.entities_of(Kind::Vindicator).len()
            + s.server.entities_of(Kind::Witch).len()
            + s.server.entities_of(Kind::Ravager).len()
    );
    assert_eq!(raid.status(), RaidStatus::Ongoing);
}

#[test]
fn raiders_of_a_wave_remember_their_raid_and_wave_in_the_saved_layout() {
    let mut s = village_scene(Difficulty::Normal);
    console(&mut s, "effect give Alice minecraft:bad_omen 120 0");
    s.tick(980);
    let pillager = s
        .server
        .entities_of(Kind::Pillager)
        .into_iter()
        .next()
        .expect("a raider");
    let saved = s.server.saved_entity(pillager.id).unwrap();
    assert_eq!(saved.get_i32("Wave"), Some(1));
    assert_eq!(saved.get_bool("CanJoinRaid"), Some(true));
    assert!(saved.get_i32("RaidId").is_some());
}

fn raiders(s: &Scene) -> Vec<rspigot_domain::EntityId> {
    [Kind::Pillager, Kind::Vindicator, Kind::Witch, Kind::Ravager]
        .into_iter()
        .flat_map(|kind| s.server.entities_of(kind))
        .map(|entity| entity.id)
        .collect()
}

fn kill_all(s: &mut Scene) {
    for id in raiders(s) {
        s.act(Order::Entity {
            entity: id,
            order: EntityOrder::Kill,
        });
    }
}

fn tick_until_wave(s: &mut Scene, wave: i32) {
    for _ in 0..800 {
        if s.server
            .raids()
            .iter()
            .next()
            .is_some_and(|raid| raid.groups_spawned() >= wave)
        {
            return;
        }
        s.tick(1);
    }
    panic!("wave {wave} never spawned");
}

fn observed_scene(difficulty: Difficulty) -> Scene {
    let server = configured_level(
        |config| config.difficulty = difficulty,
        |level| {
            level.day_time = 4_000;
            level.daylight_cycle = false;
        },
    )
    .with_rng_seed(11);
    let mut s = scene_on(
        server,
        &["Alice"],
        vec![
            observe(2, EventKind::RaidSpawnWave),
            observe(3, EventKind::RaidFinish),
            observe(4, EventKind::RaidStop),
            observe(5, EventKind::RaidTrigger),
        ],
    );
    stream_chunk_of(&mut s, BlockPos::new(1, 64, 1));
    set(&mut s, BlockPos::new(8, 64, 8), "minecraft:lectern");
    console(
        &mut s,
        "summon minecraft:villager 7.5 64 8.5 {VillagerData:{type:\"minecraft:plains\",profession:\"minecraft:librarian\",level:2},Xp:20}",
    );
    s.tick(200);
    s
}

#[test]
fn a_cancelled_raid_trigger_removes_the_omen_and_creates_no_raid() {
    let server = configured_level(
        |config| {
            config.difficulty = Difficulty::Normal;
            config.plugins = PluginSettings {
                decision_ticks: 1_000,
                ..PluginSettings::default()
            };
        },
        |level| {
            level.day_time = 4_000;
            level.daylight_cycle = false;
        },
    )
    .with_rng_seed(11);
    let mut s = scene_on(
        server,
        &["Alice"],
        vec![deciding(1, EventKind::RaidTrigger)],
    );
    stream_chunk_of(&mut s, BlockPos::new(1, 64, 1));
    set(&mut s, BlockPos::new(8, 64, 8), "minecraft:lectern");
    console(
        &mut s,
        "summon minecraft:villager 7.5 64 8.5 {VillagerData:{type:\"minecraft:plains\",profession:\"minecraft:librarian\",level:2},Xp:20}",
    );
    s.tick(200);
    console(&mut s, "effect give Alice minecraft:bad_omen 120 0");
    s.tick(640);
    let asked = last_asked(&s.plugin, EventKind::RaidTrigger);
    let EventPayload::RaidTrigger(trigger) = &asked.payload else {
        panic!("a raid trigger");
    };
    assert_eq!(trigger.world, "minecraft:overworld");
    rule_on(&mut s.server, 1, asked.id, true, None);
    s.tick(5);
    assert!(s.server.raids().is_empty());
}

#[test]
fn a_raid_runs_all_its_waves_then_ends_with_the_plugin_events_in_order() {
    let mut s = observed_scene(Difficulty::Easy);
    console(&mut s, "effect give Alice minecraft:bad_omen 120 0");
    s.tick(650);
    for wave in 1..=3 {
        tick_until_wave(&mut s, wave);
        s.tick(5);
        let raid = s.server.raids().iter().next().unwrap();
        assert_eq!(raid.total_raiders_alive() as usize, raiders(&s).len());
        if wave < 3 {
            kill_all(&mut s);
            s.tick(2);
            let raid = s.server.raids().iter().next().unwrap();
            assert_eq!(raid.total_raiders_alive(), 0);
            assert!(raid.is_between_waves() || raid.cooldown_ticks() == 300);
        }
    }
    let waves = settled_for(&s.plugin, EventKind::RaidSpawnWave);
    assert_eq!(waves.len(), 3);
    for event in &waves {
        let EventPayload::RaidSpawnWave(wave) = &event.payload else {
            panic!("a spawn wave");
        };
        assert!(wave.patrol_leader.is_some());
        assert!(!wave.raiders.is_empty());
        assert_eq!(wave.world, "minecraft:overworld");
    }
    kill_all(&mut s);
    s.tick(60);
    let raid = s.server.raids().iter().next().unwrap();
    assert_eq!(raid.status(), RaidStatus::Victory);
    assert_eq!(settled_for(&s.plugin, EventKind::RaidFinish).len(), 1);
    s.tick(620);
    assert!(s.server.raids().is_empty());
    let stops = settled_for(&s.plugin, EventKind::RaidStop);
    assert_eq!(stops.len(), 1);
    let EventPayload::RaidStop(stop) = &stops[0].payload else {
        panic!("a stop");
    };
    assert_eq!(
        stop.reason,
        rspigot_core::plugins::events::RaidStopReason::Finished
    );
    let triggers = settled_for(&s.plugin, EventKind::RaidTrigger);
    assert_eq!(triggers.len(), 1);
}

#[test]
fn killing_the_last_raider_with_a_weapon_makes_the_player_a_hero_of_the_village() {
    let mut s = observed_scene(Difficulty::Easy);
    console(&mut s, "effect give Alice minecraft:bad_omen 120 0");
    s.tick(650);
    for wave in 1..=3 {
        tick_until_wave(&mut s, wave);
        if wave < 3 {
            kill_all(&mut s);
            s.tick(2);
        }
    }
    let all = raiders(&s);
    let (last, rest) = all.split_last().unwrap();
    for id in rest {
        s.act(Order::Entity {
            entity: *id,
            order: EntityOrder::Kill,
        });
    }
    s.act(Order::Entity {
        entity: *last,
        order: EntityOrder::Teleport(Vec3::new(1.5, 64.0, 0.5)),
    });
    let health = [Kind::Pillager, Kind::Vindicator, Kind::Witch, Kind::Ravager]
        .into_iter()
        .flat_map(|kind| s.server.entities_of(kind))
        .find(|entity| entity.id == *last)
        .and_then(|entity| entity.health)
        .expect("a living raider");
    s.act(Order::Entity {
        entity: *last,
        order: EntityOrder::Damage(f64::from(health) - 0.01),
    });
    let alice = s.players[0].clone();
    s.tick(25);
    send_input(&mut s.server, &alice, PlayerInput::Attack(*last));
    s.tick(60);
    let raid = s.server.raids().iter().next().unwrap();
    assert_eq!(raid.status(), RaidStatus::Victory);
    let Answer::Player(state) = s.ask(Question::Player(alice.id)) else {
        panic!("expected Alice");
    };
    assert!(
        state
            .effects
            .iter()
            .any(|effect| effect.key == "minecraft:hero_of_the_village"),
        "{:?}",
        state.effects
    );
}
