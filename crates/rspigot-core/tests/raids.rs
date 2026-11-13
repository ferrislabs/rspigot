mod common;

use common::*;
use rspigot_core::config::PluginSettings;
use rspigot_core::events::{ClientEvent, Hand, Inbound, PlayerInput};
use rspigot_core::plugins::{
    Answer, BlockOrder, EntityOrder, EventKind, EventPayload, Order, Question,
};
use rspigot_domain::boss_event::BossUpdate;
use rspigot_domain::raid::RaidStatus;
use rspigot_domain::world::Difficulty;
use rspigot_domain::{BlockPos, Rotation, Vec3};
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

fn bad_omen(s: &mut Scene) -> Option<(i32, i32)> {
    let Answer::Player(state) = s.ask(Question::Player(s.players[0].id)) else {
        panic!("expected Alice");
    };
    state
        .effects
        .iter()
        .find(|effect| effect.key == "minecraft:bad_omen")
        .map(|effect| (effect.duration, effect.amplifier))
}

fn drink_ominous_bottle() -> (Option<(i32, i32)>, usize, Vec<&'static str>) {
    let server = configured_level(
        |config| config.difficulty = Difficulty::Normal,
        |level| level.daylight_cycle = false,
    );
    let mut s = scene_on(server, &["Alice"], Vec::new());
    stream_chunk_of(&mut s, BlockPos::new(1, 64, 1));
    let alice = s.players[0].clone();
    console(&mut s, "give Alice minecraft:ominous_bottle");
    s.tick(2);
    alice.link.take();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::UseItem {
            hand: Hand::Main,
            sequence: 1,
            rotation: Rotation::new(0.0, 0.0),
        },
    );
    s.tick(40);
    let sounds = alice
        .link
        .take()
        .into_iter()
        .filter_map(|event| match event {
            ClientEvent::Sound { sound, .. } => Some(sound),
            _ => None,
        })
        .collect();
    let Answer::Inventory(inventory) = s.ask(Question::Inventory(alice.id)) else {
        panic!("expected the inventory");
    };
    let left = inventory
        .slots
        .iter()
        .flatten()
        .filter(|item| item.key == "minecraft:ominous_bottle")
        .count();
    (bad_omen(&mut s), left, sounds)
}

#[test]
fn drinking_an_ominous_bottle_gives_bad_omen_for_two_hours_and_uses_it_up() {
    let (effect, left, sounds) = drink_ominous_bottle();
    let (duration, amplifier) = effect.expect("bad omen");
    assert_eq!(amplifier, 0);
    assert!(duration > 119_000 && duration <= 120_000, "{duration}");
    assert_eq!(left, 0);
    assert!(sounds.contains(&"minecraft:item.ominous_bottle.dispose"));
}

fn plain_scene() -> Scene {
    let server = configured_level(
        |config| config.difficulty = Difficulty::Normal,
        |level| {
            level.day_time = 4_000;
            level.daylight_cycle = false;
        },
    )
    .with_rng_seed(3);
    let mut s = scene_on(server, &["Alice"], Vec::new());
    stream_chunk_of(&mut s, BlockPos::new(1, 64, 1));
    s
}

fn health_of(s: &mut Scene) -> f32 {
    let Answer::Player(state) = s.ask(Question::Player(s.players[0].id)) else {
        panic!("expected Alice");
    };
    state.health
}

#[test]
fn a_ravager_attack_arms_its_attack_timer_for_ten_ticks_and_hurts_the_target() {
    let mut s = plain_scene();
    console(&mut s, "summon minecraft:ravager 2.5 64 0.5 {}");
    let ravager = s.server.entities_of(Kind::Ravager)[0].id;
    let mut armed = false;
    for _ in 0..200 {
        s.tick(1);
        let saved = s.server.saved_entity(ravager).unwrap();
        if saved.get_i32("AttackTick").unwrap_or(0) == 10 {
            armed = true;
            break;
        }
    }
    assert!(armed, "the ravager never attacked");
    s.tick(10);
    let saved = s.server.saved_entity(ravager).unwrap();
    assert_eq!(saved.get_i32("AttackTick"), Some(0));
    assert!(health_of(&mut s) < 20.0);
}

#[test]
fn a_ravager_coming_out_of_a_stun_roars_ten_ticks_later_and_hurts_what_is_near() {
    let mut s = plain_scene();
    console(
        &mut s,
        "summon minecraft:ravager 2.5 64 0.5 {NoAI:1b,StunTick:2}",
    );
    let ravager = s.server.entities_of(Kind::Ravager)[0].id;
    assert_eq!(health_of(&mut s), 20.0);
    s.tick(2);
    let saved = s.server.saved_entity(ravager).unwrap();
    assert_eq!(saved.get_i32("StunTick"), Some(0));
    assert_eq!(saved.get_i32("RoarTick"), Some(20));
    s.tick(9);
    assert_eq!(health_of(&mut s), 20.0);
    s.tick(2);
    assert_eq!(health_of(&mut s), 14.0);
}

#[test]
fn a_ravager_roar_spares_illagers_and_pushes_mobs_without_ravager_peers() {
    let mut s = plain_scene();
    console(
        &mut s,
        "summon minecraft:ravager 2.5 64 0.5 {NoAI:1b,RoarTick:11}",
    );
    console(&mut s, "summon minecraft:vindicator 3.5 64 1.5 {NoAI:1b}");
    console(&mut s, "summon minecraft:cow 1.5 64 1.5 {NoAI:1b}");
    console(&mut s, "summon minecraft:ravager 4.5 64 0.5 {NoAI:1b}");
    s.tick(3);
    let health = |s: &Scene, kind: Kind| s.server.entities_of(kind)[0].health.unwrap();
    assert_eq!(health(&s, Kind::Vindicator), 24.0);
    assert!(health(&s, Kind::Cow) < 10.0);
    let peers: Vec<_> = s.server.entities_of(Kind::Ravager);
    assert!(peers.iter().all(|peer| peer.health == Some(100.0)));
}

#[test]
fn a_vindicator_called_johnny_attacks_whatever_is_near() {
    let mut s = plain_scene();
    console(
        &mut s,
        "summon minecraft:vindicator 8.5 64 8.5 {CustomName:\"Johnny\"}",
    );
    console(&mut s, "summon minecraft:cow 12.5 64 8.5 {}");
    let cow = s.server.entities_of(Kind::Cow)[0].id;
    let vindicator = s.server.entities_of(Kind::Vindicator)[0].id;
    s.tick(2);
    assert_eq!(
        s.server
            .saved_entity(vindicator)
            .unwrap()
            .get_bool("Johnny"),
        Some(true)
    );
    let mut killed = false;
    for _ in 0..400 {
        s.tick(1);
        if s.server.mob_damage(cow).is_some_and(|state| state.dead) {
            killed = true;
            break;
        }
    }
    assert!(killed, "Johnny left the cow alone");
}

#[test]
fn a_vindicator_that_is_not_johnny_ignores_a_cow() {
    let mut s = plain_scene();
    console(&mut s, "summon minecraft:vindicator 8.5 64 8.5 {}");
    console(&mut s, "summon minecraft:cow 12.5 64 8.5 {}");
    let cow = s.server.entities_of(Kind::Cow)[0].id;
    s.tick(300);
    assert!(s.server.mob_damage(cow).is_some_and(|state| !state.dead));
}

#[test]
fn the_second_wave_brings_vindicators_with_the_raid_axe() {
    let mut s = observed_scene(Difficulty::Normal);
    console(&mut s, "effect give Alice minecraft:bad_omen 120 0");
    s.tick(650);
    tick_until_wave(&mut s, 1);
    s.tick(5);
    kill_all(&mut s);
    s.tick(2);
    tick_until_wave(&mut s, 2);
    s.tick(5);
    let vindicators = s.server.entities_of(Kind::Vindicator);
    assert!(vindicators.len() >= 2, "{}", vindicators.len());
    for vindicator in vindicators {
        let saved = s.server.saved_entity(vindicator.id).unwrap();
        let weapon = saved
            .get_compound("equipment")
            .and_then(|equipment| equipment.get_compound("mainhand"))
            .and_then(|stack| stack.get_str("id"));
        assert_eq!(weapon, Some("minecraft:iron_axe"));
    }
}

#[test]
fn raids_saved_in_the_level_are_loaded_by_the_server() {
    let mut saved = rspigot_domain::raid::Raids::new();
    saved.create_or_extend(
        &rspigot_domain::raid::Omen {
            pos: BlockPos::new(8, 64, 8),
            village_pois: &[],
            difficulty: Difficulty::Normal,
            bar_id: uuid::Uuid::from_u128(5),
            seed: 1,
            amplifier: Some(2),
        },
        |_| true,
    );
    let server = configured_level(|_| {}, |level| level.raids = Some(saved.to_nbt()));
    let loaded = server.raids().get(2).expect("the saved raid");
    assert_eq!(loaded.center(), BlockPos::new(8, 64, 8));
    assert_eq!(loaded.raid_omen_level(), 3);
    assert_eq!(loaded.num_groups(), 5);
    assert_eq!(server.raids().next_id(), 2);
}
