mod common;
#[path = "common/screen.rs"]
mod screen;
#[path = "common/seeded.rs"]
mod seeded;

use common::*;
use rspigot_blocks::BlockKind;
use rspigot_core::events::{Direction, Hand, Inbound, PlayerInput};
use rspigot_core::plugins::events::CreatureSpawnReason;
use rspigot_core::plugins::{Answer, EventKind, EventPayload, Phase, Question};
use rspigot_domain::{BlockPos, Vec3};
use rspigot_entities::EntityKind as Kind;
use rspigot_nbt::Compound;
use seeded::{Seed, world};

const SPAWNER: BlockPos = BlockPos::new(4, 64, 4);

fn spawner(delay: i16, count: i16) -> Seed {
    Seed::block(SPAWNER, BlockKind::Spawner)
        .carrying("Delay", delay)
        .carrying("SpawnCount", count)
        .carrying("MaxNearbyEntities", 100i16)
        .carrying("MinSpawnDelay", 400i16)
        .carrying("MaxSpawnDelay", 400i16)
        .carrying(
            "SpawnData",
            Compound::new().with("entity", Compound::new().with("id", "minecraft:villager")),
        )
}

fn villagers(s: &Scene) -> usize {
    s.server.entities_of(Kind::Villager).len()
}

#[test]
fn a_spawner_near_a_player_asks_spawner_spawn_then_creature_spawn_and_places_the_mob() {
    let mut s = world(
        vec![spawner(0, 20)],
        vec![
            observe(1, EventKind::SpawnerSpawn),
            observe(2, EventKind::CreatureSpawn),
        ],
    );
    s.tick(3);
    let events = settled_for(&s.plugin, EventKind::SpawnerSpawn);
    assert!(!events.is_empty());
    let EventPayload::SpawnerSpawn(spawn) = &events[0].payload else {
        panic!("expected a spawner spawn");
    };
    assert_eq!(spawn.entity_type, "minecraft:villager");
    assert_eq!(spawn.entity.kind, "minecraft:villager");
    assert_eq!(spawn.spawner.pos, SPAWNER);
    assert_eq!(spawn.spawner.spawned.as_deref(), Some("minecraft:villager"));
    assert!((SPAWNER.y - 1..=SPAWNER.y + 1).contains(&(spawn.location.position.y.floor() as i32)));
    assert!(villagers(&s) >= 1);
    let order: Vec<_> = fired(&s.plugin)
        .into_iter()
        .filter(|event| event.phase == Phase::Settled)
        .map(|event| event.payload.kind())
        .collect();
    let first_spawn = order
        .iter()
        .position(|kind| *kind == EventKind::SpawnerSpawn)
        .expect("a spawner spawn");
    assert_eq!(order[first_spawn + 1], EventKind::CreatureSpawn);
    let creature = settled_for(&s.plugin, EventKind::CreatureSpawn);
    let EventPayload::CreatureSpawn(born) = &creature[0].payload else {
        panic!("expected a creature spawn");
    };
    assert_eq!(born.spawn_reason, CreatureSpawnReason::Spawner);
}

#[test]
fn cancelling_spawner_spawn_keeps_the_mob_out_and_the_round_goes_on() {
    let mut s = world(
        vec![spawner(0, 12)],
        vec![deciding(1, EventKind::SpawnerSpawn)],
    );
    let mut ruled: Vec<u64> = Vec::new();
    for _ in 0..4 {
        s.tick(1);
        while let Some(asked) = asked_for(&s.plugin, EventKind::SpawnerSpawn)
            .into_iter()
            .find(|asked| !ruled.contains(&asked.id))
        {
            rule_on(&mut s.server, 1, asked.id, true, None);
            ruled.push(asked.id);
        }
    }
    assert!(ruled.len() >= 2);
    assert_eq!(villagers(&s), 0);
}

#[test]
fn without_a_handler_the_spawner_places_its_mobs_and_waits_for_the_next_round() {
    let mut s = world(vec![spawner(0, 20)], Vec::new());
    s.tick(3);
    let placed = villagers(&s);
    assert!(placed >= 1);
    s.tick(20);
    assert_eq!(villagers(&s), placed);
}

#[test]
fn a_spawner_with_nobody_in_range_does_nothing() {
    let far = BlockPos::new(60, 64, 60);
    let mut s = world(
        vec![
            Seed::block(far, BlockKind::Spawner)
                .carrying("Delay", 0i16)
                .carrying(
                    "SpawnData",
                    Compound::new()
                        .with("entity", Compound::new().with("id", "minecraft:villager")),
                ),
        ],
        vec![observe(1, EventKind::SpawnerSpawn)],
    );
    s.tick(5);
    assert!(settled_for(&s.plugin, EventKind::SpawnerSpawn).is_empty());
    assert_eq!(villagers(&s), 0);
}

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

#[test]
fn a_spawn_egg_used_on_a_spawner_changes_its_mob_and_is_spent() {
    let mut s = world(
        vec![spawner(3, 20)],
        vec![observe(1, EventKind::SpawnerSpawn)],
    );
    let alice = s.players[0].clone();
    console(&mut s, "gamemode survival Alice");
    console(&mut s, "give Alice minecraft:wandering_trader_spawn_egg");
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::UseItemOn {
            hand: Hand::Main,
            pos: SPAWNER,
            face: Direction::Up,
            cursor: Vec3::new(0.5, 1.0, 0.5),
            inside_block: false,
            sequence: 1,
        },
    );
    s.tick(6);
    assert_eq!(villagers(&s), 0);
    assert!(!s.server.entities_of(Kind::WanderingTrader).is_empty());
    let Answer::Inventory(inventory) = s.ask(Question::Inventory(alice.id)) else {
        panic!("expected the inventory");
    };
    assert!(inventory.slots[0].is_none());
    let events = settled_for(&s.plugin, EventKind::SpawnerSpawn);
    assert!(!events.is_empty());
    let EventPayload::SpawnerSpawn(spawn) = &events[0].payload else {
        panic!("expected a spawner spawn");
    };
    assert_eq!(spawn.entity_type, "minecraft:wandering_trader");
    assert_eq!(
        spawn.spawner.spawned.as_deref(),
        Some("minecraft:wandering_trader")
    );
}
