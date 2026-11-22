mod common;

use common::*;
use rspigot_core::events::{Hand, Inbound, PlayerInput};
use rspigot_core::plugins::{Answer, Question};
use rspigot_domain::{BlockPos, EntityId, Vec3};
use rspigot_entities::EntityKind as Kind;

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

fn ready() -> Scene {
    let mut s = scene(&["Alice"]);
    stream_chunk_of(&mut s, BlockPos::new(1, 64, 1));
    s
}

fn interact(s: &mut Scene, target: EntityId) {
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Interact {
            target,
            hand: Hand::Main,
            sneaking: false,
            location: Vec3::new(0.0, 0.5, 0.0),
        },
    );
}

fn feed_pair(s: &mut Scene, kind: Kind, food: &str, nbt: &str) -> (EntityId, EntityId) {
    let first = summon(s, kind, "1.5 64 0.5", nbt);
    let second = summon(s, kind, "2.5 64 0.5", nbt);
    console(s, &format!("give Alice minecraft:{food} 4"));
    interact(s, first);
    interact(s, second);
    (first, second)
}

fn newborn(s: &Scene, kind: Kind, parents: [EntityId; 2]) -> Option<EntityId> {
    s.server
        .entities_of(kind)
        .into_iter()
        .map(|entity| entity.id)
        .find(|id| !parents.contains(id))
}

fn breed(s: &mut Scene, kind: Kind, food: &str, nbt: &str) -> (EntityId, EntityId, EntityId) {
    let (first, second) = feed_pair(s, kind, food, nbt);
    for _ in 0..400 {
        s.tick(1);
        if let Some(calf) = newborn(s, kind, [first, second]) {
            return (first, second, calf);
        }
    }
    panic!("no {kind:?} calf was born");
}

fn is_baby(s: &Scene, id: EntityId) -> bool {
    s.server
        .saved_entity(id)
        .and_then(|nbt| nbt.get_i32("Age"))
        .is_some_and(|age| age < 0)
}

#[test]
fn striders_fed_warped_fungus_breed_a_baby_strider() {
    let mut s = ready();
    let (_, _, calf) = breed(&mut s, Kind::Strider, "warped_fungus", "{}");
    assert!(is_baby(&s, calf));
}

#[test]
fn hoglins_fed_crimson_fungus_breed_a_baby_hoglin() {
    let mut s = ready();
    let (_, _, calf) = breed(
        &mut s,
        Kind::Hoglin,
        "crimson_fungus",
        "{IsImmuneToZombification:1b}",
    );
    assert!(is_baby(&s, calf));
}

#[test]
fn mooshrooms_fed_wheat_breed_a_baby_mooshroom() {
    let mut s = ready();
    let (_, _, calf) = breed(&mut s, Kind::Mooshroom, "wheat", "{}");
    assert!(is_baby(&s, calf));
}

fn count_of(s: &mut Scene, key: &str) -> i32 {
    let alice = s.players[0].clone();
    let Answer::Inventory(state) = s.ask(Question::Inventory(alice.id)) else {
        panic!("an inventory");
    };
    state
        .slots
        .into_iter()
        .flatten()
        .filter(|item| item.key == key)
        .map(|item| item.count)
        .sum()
}

fn dropped(s: &Scene, key: &str) -> i32 {
    s.server
        .entities_of(Kind::Item)
        .into_iter()
        .filter_map(|item| s.server.saved_entity(item.id))
        .filter_map(|saved| {
            let stack = saved.get_compound("Item")?.clone();
            (stack.get_str("id") == Some(key)).then(|| stack.get_i32("count").unwrap_or(0))
        })
        .sum()
}

fn mooshroom(s: &mut Scene, nbt: &str) -> EntityId {
    summon(s, Kind::Mooshroom, "1.5 64 0.5", nbt)
}

fn type_of(s: &Scene, id: EntityId) -> Option<String> {
    s.server
        .saved_entity(id)
        .and_then(|nbt| nbt.get_str("Type").map(str::to_owned))
}

#[test]
fn a_bowl_on_a_mooshroom_gives_mushroom_stew() {
    let mut s = ready();
    let cow = mooshroom(&mut s, "{NoAI:1b}");
    console(&mut s, "give Alice minecraft:bowl 2");
    interact(&mut s, cow);
    s.tick(1);
    assert_eq!(count_of(&mut s, "minecraft:mushroom_stew"), 1);
    assert_eq!(count_of(&mut s, "minecraft:bowl"), 1);
}

#[test]
fn a_bowl_on_a_baby_mooshroom_does_nothing() {
    let mut s = ready();
    let cow = mooshroom(&mut s, "{NoAI:1b,Age:-24000}");
    console(&mut s, "give Alice minecraft:bowl 1");
    interact(&mut s, cow);
    s.tick(1);
    assert_eq!(count_of(&mut s, "minecraft:mushroom_stew"), 0);
    assert_eq!(count_of(&mut s, "minecraft:bowl"), 1);
}

#[test]
fn a_bucket_on_a_mooshroom_gives_milk() {
    let mut s = ready();
    let cow = mooshroom(&mut s, "{NoAI:1b}");
    console(&mut s, "give Alice minecraft:bucket 1");
    interact(&mut s, cow);
    s.tick(1);
    assert_eq!(count_of(&mut s, "minecraft:milk_bucket"), 1);
}

#[test]
fn a_brown_mooshroom_fed_a_flower_gives_suspicious_stew() {
    let mut s = ready();
    let cow = mooshroom(&mut s, "{NoAI:1b,Type:\"brown\"}");
    console(&mut s, "give Alice minecraft:dandelion 1");
    interact(&mut s, cow);
    s.tick(1);
    assert_eq!(count_of(&mut s, "minecraft:dandelion"), 0);
    let saved = s.server.saved_entity(cow).expect("saved");
    assert!(saved.get_list("stew_effects").is_some_and(|l| l.len() == 1));
    console(
        &mut s,
        "item replace entity Alice weapon.mainhand with minecraft:bowl",
    );
    interact(&mut s, cow);
    s.tick(1);
    assert_eq!(count_of(&mut s, "minecraft:suspicious_stew"), 1);
    assert!(
        s.server
            .saved_entity(cow)
            .is_some_and(|nbt| nbt.get_list("stew_effects").is_none())
    );
}

#[test]
fn a_red_mooshroom_refuses_flowers() {
    let mut s = ready();
    let cow = mooshroom(&mut s, "{NoAI:1b}");
    console(&mut s, "give Alice minecraft:dandelion 1");
    interact(&mut s, cow);
    s.tick(1);
    assert_eq!(count_of(&mut s, "minecraft:dandelion"), 1);
    assert!(
        s.server
            .saved_entity(cow)
            .is_some_and(|nbt| nbt.get_list("stew_effects").is_none())
    );
}

#[test]
fn shears_turn_a_mooshroom_into_a_cow_and_drop_five_mushrooms() {
    let mut s = ready();
    let cow = mooshroom(&mut s, "{NoAI:1b}");
    console(&mut s, "give Alice minecraft:shears 1");
    interact(&mut s, cow);
    s.tick(2);
    assert!(s.server.entities_of(Kind::Mooshroom).is_empty());
    assert_eq!(s.server.entities_of(Kind::Cow).len(), 1);
    assert_eq!(dropped(&s, "minecraft:red_mushroom"), 5);
}

#[test]
fn a_brown_mooshroom_drops_brown_mushrooms() {
    let mut s = ready();
    let cow = mooshroom(&mut s, "{NoAI:1b,Type:\"brown\"}");
    console(&mut s, "give Alice minecraft:shears 1");
    interact(&mut s, cow);
    s.tick(2);
    assert_eq!(dropped(&s, "minecraft:brown_mushroom"), 5);
    assert_eq!(dropped(&s, "minecraft:red_mushroom"), 0);
}

#[test]
fn the_mooshroom_type_is_saved_and_defaults_to_red() {
    let mut s = ready();
    let red = mooshroom(&mut s, "{NoAI:1b}");
    let brown = mooshroom(&mut s, "{NoAI:1b,Type:\"brown\"}");
    assert_eq!(type_of(&s, red).as_deref(), Some("red"));
    assert_eq!(type_of(&s, brown).as_deref(), Some("brown"));
}

#[test]
fn brown_parents_may_have_a_brown_calf() {
    let mut brown_calves = 0;
    for _ in 0..8 {
        let mut s = ready();
        let (_, _, calf) = breed(&mut s, Kind::Mooshroom, "wheat", "{Type:\"brown\"}");
        if type_of(&s, calf).as_deref() == Some("brown") {
            brown_calves += 1;
        }
    }
    assert!(brown_calves > 0);
}

#[test]
fn a_giant_stands_still_and_never_attacks() {
    let mut s = ready();
    let giant = summon(&mut s, Kind::Giant, "3.5 64 0.5", "{}");
    let start = s
        .server
        .entities_of(Kind::Giant)
        .into_iter()
        .find(|e| e.id == giant)
        .map(|e| e.position)
        .expect("giant");
    s.tick(200);
    let end = s
        .server
        .entities_of(Kind::Giant)
        .into_iter()
        .find(|e| e.id == giant)
        .map(|e| e.position)
        .expect("giant");
    assert!((end.x - start.x).abs() < 1.0e-6 && (end.z - start.z).abs() < 1.0e-6);
    let saved = s.server.saved_entity(giant).expect("saved");
    assert_eq!(saved.get_f64("Health"), Some(100.0));
}

#[test]
fn lightning_toggles_a_mooshroom_colour_once_per_bolt() {
    let mut s = ready();
    let cow = mooshroom(&mut s, "{NoAI:1b}");
    console(&mut s, "summon minecraft:lightning_bolt 1.5 64 0.5");
    s.tick(3);
    assert_eq!(type_of(&s, cow).as_deref(), Some("brown"));
    s.tick(20);
    assert_eq!(type_of(&s, cow).as_deref(), Some("brown"));
}
