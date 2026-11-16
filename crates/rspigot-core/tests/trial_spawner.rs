mod common;
#[path = "common/screen.rs"]
mod screen;
#[path = "common/seeded.rs"]
mod seeded;

use common::*;
use rspigot_blocks::BlockKind;
use rspigot_core::events::{ClientEvent, Inbound};
use rspigot_core::plugins::{Answer, EventKind, EventPayload, Question};
use rspigot_domain::BlockPos;
use rspigot_entities::EntityKind as Kind;
use rspigot_nbt::{Compound, Tag};
use seeded::{Seed, world_spawning};

const SPAWNER: BlockPos = BlockPos::new(4, 64, 4);

fn potential(id: &str) -> Tag {
    Tag::List(vec![Tag::Compound(
        Compound::new()
            .with(
                "data",
                Compound::new().with("entity", Compound::new().with("id", id)),
            )
            .with("weight", 1),
    )])
}

fn config(total: f32, simultaneous: f32) -> Compound {
    Compound::new()
        .with("spawn_potentials", potential("minecraft:villager"))
        .with("total_mobs", total)
        .with("simultaneous_mobs", simultaneous)
        .with("total_mobs_added_per_player", 2.0f32)
        .with("simultaneous_mobs_added_per_player", 1.0f32)
        .with("ticks_between_spawn", 10)
}

fn spawner(total: f32, simultaneous: f32) -> Seed {
    Seed::block(SPAWNER, BlockKind::TrialSpawner)
        .carrying("normal_config", config(total, simultaneous))
        .carrying("ominous_config", config(total, simultaneous))
        .carrying("target_cooldown_length", 100)
        .carrying("required_player_range", 14)
}

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn survival(handlers: Vec<rspigot_core::plugins::HandlerSpec>, seeds: Vec<Seed>) -> Scene {
    let mut s = world_spawning(seeds, handlers, true);
    console(&mut s, "gamemode survival Alice");
    s
}

fn prop(s: &mut Scene, pos: BlockPos, name: &str) -> String {
    match s.ask(Question::Block(pos)) {
        Answer::Block(state) => state
            .properties
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.clone())
            .unwrap_or_default(),
        other => panic!("expected a block, got {other:?}"),
    }
}

fn villagers(s: &Scene) -> usize {
    s.server.entities_of(Kind::Villager).len()
}

fn level_events(alice: &Player) -> Vec<(i32, i32)> {
    alice
        .link
        .take()
        .into_iter()
        .filter_map(|event| match event {
            ClientEvent::LevelEvent { event, data, .. } => Some((event, data)),
            _ => None,
        })
        .collect()
}

#[test]
fn a_spawner_with_a_mob_wakes_up_when_a_player_comes_near() {
    let mut s = survival(Vec::new(), vec![spawner(6.0, 2.0)]);
    s.tick(3);
    assert_eq!(
        prop(&mut s, SPAWNER, "trial_spawner_state"),
        "waiting_for_players"
    );
    s.tick(60);
    assert_eq!(prop(&mut s, SPAWNER, "trial_spawner_state"), "active");
}

#[test]
fn a_spawner_without_a_potential_stays_inactive() {
    let mut s = survival(
        Vec::new(),
        vec![Seed::block(SPAWNER, BlockKind::TrialSpawner)],
    );
    s.tick(80);
    assert_eq!(prop(&mut s, SPAWNER, "trial_spawner_state"), "inactive");
    assert_eq!(villagers(&s), 0);
}

#[test]
fn an_active_spawner_keeps_at_most_the_simultaneous_mobs_alive() {
    let mut s = survival(Vec::new(), vec![spawner(6.0, 2.0)]);
    s.tick(120);
    assert_eq!(villagers(&s), 2);
}

#[test]
fn a_creative_player_does_not_wake_the_spawner() {
    let mut s = survival(Vec::new(), vec![spawner(6.0, 2.0)]);
    console(&mut s, "gamemode creative Alice");
    s.tick(100);
    assert_eq!(
        prop(&mut s, SPAWNER, "trial_spawner_state"),
        "waiting_for_players"
    );
    assert_eq!(villagers(&s), 0);
}

#[test]
fn killing_every_mob_of_the_wave_ejects_the_reward_then_cools_down() {
    let mut s = survival(Vec::new(), vec![spawner(1.0, 1.0)]);
    s.tick(100);
    assert_eq!(villagers(&s), 1);
    console(&mut s, "kill @e[type=minecraft:villager]");
    s.tick(2);
    assert_eq!(
        prop(&mut s, SPAWNER, "trial_spawner_state"),
        "waiting_for_reward_ejection"
    );
    s.tick(93);
    assert!(!s.server.ground_items().is_empty());
    assert_eq!(prop(&mut s, SPAWNER, "trial_spawner_state"), "cooldown");
}

#[test]
fn the_spawn_asks_trial_spawner_spawn_before_the_mob_appears() {
    let mut s = survival(
        vec![
            observe(1, EventKind::TrialSpawnerSpawn),
            observe(2, EventKind::CreatureSpawn),
        ],
        vec![spawner(6.0, 2.0)],
    );
    s.tick(100);
    let events = settled_for(&s.plugin, EventKind::TrialSpawnerSpawn);
    assert!(!events.is_empty());
    let EventPayload::TrialSpawnerSpawn(spawn) = &events[0].payload else {
        panic!("expected a trial spawner spawn");
    };
    assert_eq!(spawn.entity_type, "minecraft:villager");
    assert_eq!(spawn.trial_spawner.pos, SPAWNER);
    assert!(!spawn.trial_spawner.ominous);
}

#[test]
fn cancelling_the_trial_spawner_spawn_keeps_the_mob_out() {
    let mut s = survival(
        vec![deciding(1, EventKind::TrialSpawnerSpawn)],
        vec![spawner(6.0, 2.0)],
    );
    let mut ruled: Vec<u64> = Vec::new();
    for _ in 0..120 {
        s.tick(1);
        while let Some(asked) = asked_for(&s.plugin, EventKind::TrialSpawnerSpawn)
            .into_iter()
            .find(|asked| !ruled.contains(&asked.id))
        {
            rule_on(&mut s.server, 1, asked.id, true, None);
            ruled.push(asked.id);
        }
    }
    assert!(!ruled.is_empty());
    assert_eq!(villagers(&s), 0);
}

#[test]
fn the_reward_ejection_asks_block_dispense_loot() {
    let mut s = survival(
        vec![observe(1, EventKind::BlockDispenseLoot)],
        vec![spawner(1.0, 1.0)],
    );
    s.tick(100);
    console(&mut s, "kill @e[type=minecraft:villager]");
    s.tick(150);
    let events = settled_for(&s.plugin, EventKind::BlockDispenseLoot);
    assert!(!events.is_empty());
    let EventPayload::BlockDispenseLoot(loot) = &events[0].payload else {
        panic!("expected a dispense loot");
    };
    assert_eq!(loot.block, SPAWNER);
    assert!(!loot.dispensed_loot.is_empty());
    assert!(loot.player.is_none());
}

#[test]
fn a_cancelled_dispense_loot_ejects_nothing() {
    let mut s = survival(
        vec![deciding(1, EventKind::BlockDispenseLoot)],
        vec![spawner(1.0, 1.0)],
    );
    s.tick(100);
    console(&mut s, "kill @e[type=minecraft:villager]");
    let mut ruled: Vec<u64> = Vec::new();
    for _ in 0..150 {
        s.tick(1);
        while let Some(asked) = asked_for(&s.plugin, EventKind::BlockDispenseLoot)
            .into_iter()
            .find(|asked| !ruled.contains(&asked.id))
        {
            rule_on(&mut s.server, 1, asked.id, true, None);
            ruled.push(asked.id);
        }
    }
    assert!(!ruled.is_empty());
    assert!(s.server.ground_items().is_empty());
}

#[test]
fn a_player_with_bad_omen_makes_the_spawner_ominous_and_gets_trial_omen() {
    let mut s = survival(Vec::new(), vec![spawner(6.0, 2.0)]);
    console(&mut s, "effect give Alice minecraft:bad_omen 600 0");
    let alice = s.players[0].clone();
    s.tick(60);
    assert_eq!(prop(&mut s, SPAWNER, "ominous"), "true");
    assert!(level_events(&alice).iter().any(|(event, _)| *event == 3020));
    let Answer::Player(state) = s.ask(Question::Player(alice.id)) else {
        panic!("expected the state of Alice");
    };
    let effects = state.effects;
    assert!(
        effects
            .iter()
            .any(|effect| effect.key == "minecraft:trial_omen")
    );
    assert!(
        !effects
            .iter()
            .any(|effect| effect.key == "minecraft:bad_omen")
    );
}

#[test]
fn an_ominous_spawner_drops_an_item_above_a_player_after_a_delay() {
    let mut s = survival(Vec::new(), vec![spawner(6.0, 2.0)]);
    console(&mut s, "effect give Alice minecraft:bad_omen 600 0");
    let alice = s.players[0].clone();
    let mut seen = Vec::new();
    for _ in 0..400 {
        s.tick(1);
        seen.extend(level_events(&alice));
    }
    assert!(
        seen.iter()
            .any(|(event, data)| *event == 3021 && *data == 1)
    );
}

#[test]
fn a_config_named_in_the_registry_spawns_its_mob() {
    let seed = Seed::block(SPAWNER, BlockKind::TrialSpawner)
        .carrying("normal_config", "trial_chamber/breeze/normal");
    let mut s = survival(Vec::new(), vec![seed]);
    s.tick(160);
    assert!(!s.server.entities_of(Kind::Breeze).is_empty());
}

#[test]
fn a_spawn_egg_sets_the_mob_of_a_trial_spawner_and_resets_it() {
    let mut s = survival(
        Vec::new(),
        vec![Seed::block(SPAWNER, BlockKind::TrialSpawner)],
    );
    let alice = s.players[0].clone();
    console(&mut s, "give Alice minecraft:villager_spawn_egg");
    screen::use_block(&mut s, &alice, SPAWNER);
    s.tick(3);
    assert_eq!(
        prop(&mut s, SPAWNER, "trial_spawner_state"),
        "waiting_for_players"
    );
    s.tick(100);
    assert!(villagers(&s) >= 1);
}
