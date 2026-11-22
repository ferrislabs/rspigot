mod common;

use common::*;
use rspigot_core::events::Inbound;
use rspigot_domain::{BlockPos, EntityId};
use rspigot_entities::EntityKind as Kind;
use rspigot_nbt::Compound;

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn ready() -> Scene {
    let mut s = scene(&["Alice"]);
    stream_chunk_of(&mut s, BlockPos::new(1, 64, 1));
    s
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

fn drop_item(s: &mut Scene, id: &str, at: &str) {
    console(
        s,
        &format!("summon minecraft:item {at} {{Item:{{id:\"{id}\",count:1}},PickupDelay:0s}}"),
    );
}

fn equipment(s: &Scene, mob: EntityId) -> Compound {
    s.server
        .saved_entity(mob)
        .and_then(|nbt| nbt.get_compound("equipment").cloned())
        .unwrap_or_default()
}

fn slot_id(equipment: &Compound, slot: &str) -> Option<String> {
    equipment
        .get_compound(slot)
        .and_then(|stack| stack.get_str("id"))
        .map(str::to_owned)
}

fn items_on_the_ground(s: &Scene) -> usize {
    s.server.entities_of(Kind::Item).len()
}

const CALM: &str = "{CanPickUpLoot:1b,Silent:1b,PersistenceRequired:1b,NoAI:1b}";

#[test]
fn a_zombie_that_can_pick_up_loot_wears_a_helmet_lying_beside_it() {
    let mut s = ready();
    let zombie = summon(&mut s, Kind::Zombie, "2.5 64 2.5", CALM);
    drop_item(&mut s, "minecraft:iron_helmet", "2.5 64 2.5");
    s.tick(5);
    assert_eq!(
        slot_id(&equipment(&s, zombie), "head").as_deref(),
        Some("minecraft:iron_helmet")
    );
    assert_eq!(items_on_the_ground(&s), 0);
}

#[test]
fn an_empty_handed_zombie_picks_up_a_sword() {
    let mut s = ready();
    let zombie = summon(&mut s, Kind::Zombie, "2.5 64 2.5", CALM);
    drop_item(&mut s, "minecraft:iron_sword", "2.5 64 2.5");
    s.tick(5);
    assert_eq!(
        slot_id(&equipment(&s, zombie), "mainhand").as_deref(),
        Some("minecraft:iron_sword")
    );
}

#[test]
fn a_mob_that_cannot_pick_up_loot_leaves_the_item_alone() {
    let mut s = ready();
    let zombie = summon(
        &mut s,
        Kind::Zombie,
        "2.5 64 2.5",
        "{CanPickUpLoot:0b,Silent:1b,PersistenceRequired:1b,NoAI:1b}",
    );
    drop_item(&mut s, "minecraft:iron_helmet", "2.5 64 2.5");
    s.tick(5);
    assert!(slot_id(&equipment(&s, zombie), "head").is_none());
    assert_eq!(items_on_the_ground(&s), 1);
}

#[test]
fn a_worse_helmet_is_left_on_the_ground() {
    let mut s = ready();
    let zombie = summon(
        &mut s,
        Kind::Zombie,
        "2.5 64 2.5",
        "{CanPickUpLoot:1b,Silent:1b,PersistenceRequired:1b,NoAI:1b,equipment:{head:{id:\"minecraft:diamond_helmet\",count:1},mainhand:{id:\"minecraft:stick\",count:1}}}",
    );
    drop_item(&mut s, "minecraft:leather_helmet", "2.5 64 2.5");
    s.tick(5);
    assert_eq!(
        slot_id(&equipment(&s, zombie), "head").as_deref(),
        Some("minecraft:diamond_helmet")
    );
    assert_eq!(items_on_the_ground(&s), 1);
}

#[test]
fn picked_up_gear_is_guaranteed_to_drop_and_keeps_the_mob_around() {
    let mut s = ready();
    let zombie = summon(
        &mut s,
        Kind::Zombie,
        "2.5 64 2.5",
        "{CanPickUpLoot:1b,Silent:1b,NoAI:1b}",
    );
    drop_item(&mut s, "minecraft:iron_helmet", "2.5 64 2.5");
    s.tick(5);
    let saved = s.server.saved_entity(zombie).expect("saved");
    let chances = saved.get_compound("drop_chances").expect("drop_chances");
    assert_eq!(chances.get_f64("head"), Some(2.0));
    assert_eq!(saved.get_bool("PersistenceRequired"), Some(true));
}

#[test]
fn an_item_out_of_reach_is_ignored() {
    let mut s = ready();
    let zombie = summon(&mut s, Kind::Zombie, "2.5 64 2.5", CALM);
    drop_item(&mut s, "minecraft:iron_helmet", "6.5 64 6.5");
    s.tick(5);
    assert!(slot_id(&equipment(&s, zombie), "head").is_none());
}

#[test]
fn a_better_sword_replaces_the_worn_one_which_drops_when_certain() {
    let mut s = ready();
    let zombie = summon(
        &mut s,
        Kind::Zombie,
        "2.5 64 2.5",
        "{CanPickUpLoot:1b,Silent:1b,PersistenceRequired:1b,NoAI:1b,equipment:{mainhand:{id:\"minecraft:wooden_sword\",count:1}},drop_chances:{mainhand:2.0f}}",
    );
    drop_item(&mut s, "minecraft:diamond_sword", "2.5 64 2.5");
    s.tick(5);
    assert_eq!(
        slot_id(&equipment(&s, zombie), "mainhand").as_deref(),
        Some("minecraft:diamond_sword")
    );
    assert_eq!(items_on_the_ground(&s), 1);
}

#[test]
fn a_skeleton_keeps_its_bow_instead_of_taking_a_sword() {
    let mut s = ready();
    let skeleton = summon(
        &mut s,
        Kind::Skeleton,
        "2.5 64 2.5",
        "{CanPickUpLoot:1b,Silent:1b,PersistenceRequired:1b,NoAI:1b,equipment:{mainhand:{id:\"minecraft:bow\",count:1}}}",
    );
    drop_item(&mut s, "minecraft:diamond_sword", "2.5 64 2.5");
    s.tick(5);
    assert_eq!(
        slot_id(&equipment(&s, skeleton), "mainhand").as_deref(),
        Some("minecraft:bow")
    );
    assert_eq!(items_on_the_ground(&s), 1);
}
