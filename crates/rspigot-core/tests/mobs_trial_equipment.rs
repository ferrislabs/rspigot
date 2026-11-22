mod common;
#[path = "common/screen.rs"]
mod screen;
#[path = "common/seeded.rs"]
mod seeded;

use common::*;
use rspigot_blocks::BlockKind;
use rspigot_core::events::Inbound;
use rspigot_domain::BlockPos;
use rspigot_entities::EntityKind as Kind;
use rspigot_nbt::{Compound, Tag};
use seeded::{Seed, world_spawning};

const SPAWNER: BlockPos = BlockPos::new(4, 64, 4);

fn potential(id: &str, table: Option<&str>) -> Tag {
    let mut data = Compound::new().with("entity", Compound::new().with("id", id));
    if let Some(table) = table {
        data.insert(
            "equipment",
            Compound::new()
                .with("loot_table", table)
                .with("slot_drop_chances", 0.0f32),
        );
    }
    Tag::List(vec![Tag::Compound(
        Compound::new().with("data", data).with("weight", 1),
    )])
}

fn config(id: &str, table: Option<&str>) -> Compound {
    Compound::new()
        .with("spawn_potentials", potential(id, table))
        .with("total_mobs", 6.0f32)
        .with("simultaneous_mobs", 2.0f32)
        .with("total_mobs_added_per_player", 2.0f32)
        .with("simultaneous_mobs_added_per_player", 1.0f32)
        .with("ticks_between_spawn", 10)
}

fn spawner(id: &str, table: Option<&str>) -> Seed {
    Seed::block(SPAWNER, BlockKind::TrialSpawner)
        .carrying("normal_config", config(id, table))
        .carrying("ominous_config", config(id, table))
        .carrying("target_cooldown_length", 100)
        .carrying("required_player_range", 14)
}

fn spawned_with(id: &str, kind: Kind, table: Option<&str>) -> Vec<Compound> {
    let mut s = world_spawning(vec![spawner(id, table)], Vec::new(), true);
    s.server
        .handle(Inbound::Console("gamemode survival Alice".to_owned()));
    s.server.deliver();
    s.tick(120);
    s.server
        .entities_of(kind)
        .into_iter()
        .filter_map(|entity| s.server.saved_entity(entity.id))
        .collect()
}

fn mainhand(saved: &Compound) -> Option<String> {
    saved
        .get_compound("equipment")?
        .get_compound("mainhand")?
        .get_str("id")
        .map(str::to_owned)
}

#[test]
fn a_melee_potential_keeps_its_equipment_table() {
    let mobs = spawned_with(
        "minecraft:zombie",
        Kind::Zombie,
        Some("minecraft:equipment/trial_chamber_melee"),
    );
    assert!(!mobs.is_empty());
    for mob in &mobs {
        let sword = mainhand(mob).expect("a sword");
        assert!(
            sword == "minecraft:iron_sword" || sword == "minecraft:diamond_sword",
            "{sword}"
        );
    }
}

#[test]
fn a_ranged_potential_keeps_its_equipment_table() {
    let mobs = spawned_with(
        "minecraft:skeleton",
        Kind::Skeleton,
        Some("minecraft:equipment/trial_chamber_ranged"),
    );
    assert!(!mobs.is_empty());
    for mob in &mobs {
        assert_eq!(mainhand(mob).as_deref(), Some("minecraft:bow"));
    }
}

#[test]
fn equipment_from_the_table_is_never_dropped() {
    let mobs = spawned_with(
        "minecraft:zombie",
        Kind::Zombie,
        Some("minecraft:equipment/trial_chamber_melee"),
    );
    assert!(!mobs.is_empty());
    let chances = mobs[0].get_compound("drop_chances").expect("drop_chances");
    assert_eq!(chances.get_f64("mainhand"), Some(0.0));
}

#[test]
fn a_potential_without_equipment_spawns_without_the_table_gear() {
    let mobs = spawned_with("minecraft:villager", Kind::Villager, None);
    assert!(!mobs.is_empty());
    assert!(mainhand(&mobs[0]).is_none());
}

#[test]
fn an_ominous_trial_spawner_creates_ominous_item_spawner_entities() {
    let mut s = world_spawning(vec![spawner("minecraft:villager", None)], Vec::new(), true);
    s.server
        .handle(Inbound::Console("gamemode survival Alice".to_owned()));
    s.server.handle(Inbound::Console(
        "effect give Alice minecraft:bad_omen 600 0".to_owned(),
    ));
    s.server.deliver();
    let mut seen = false;
    for _ in 0..400 {
        s.tick(1);
        if !s.server.entities_of(Kind::OminousItemSpawner).is_empty() {
            seen = true;
            break;
        }
    }
    assert!(seen);
}
