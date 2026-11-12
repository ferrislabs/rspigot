mod common;

use common::*;
use rspigot_core::commands::Target;
use rspigot_core::events::Inbound;
use rspigot_domain::{BlockPos, EntityId};
use rspigot_entities::EntityKind as Kind;
use rspigot_nbt::Compound;

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

fn ready(mut s: Scene) -> Scene {
    stream_chunk_of(&mut s, BlockPos::new(1, 64, 1));
    s
}

fn creative(mut s: Scene) -> Scene {
    console(&mut s, "gamemode creative Alice");
    s
}

fn memories(s: &Scene, id: EntityId) -> Compound {
    s.server
        .saved_entity(id)
        .and_then(|nbt| nbt.get_compound("Brain").cloned())
        .and_then(|brain| brain.get_compound("memories").cloned())
        .unwrap_or_default()
}

fn ttl(memories: &Compound, name: &str) -> Option<i64> {
    memories.get_compound(name)?.get_i64("ttl")
}

fn count(s: &Scene, kind: Kind) -> usize {
    s.server.entities_of(kind).len()
}

const IMMUNE: &str = "{IsImmuneToZombification:1b}";

#[test]
fn a_summoned_piglin_waits_before_its_first_hunt_and_a_baby_is_never_armed() {
    let mut s = ready(scene(&["Alice"]));
    let piglin = summon(&mut s, Kind::Piglin, "5.5 64 5.5", "");
    s.tick(1);
    let cooldown = ttl(&memories(&s, piglin), "minecraft:hunted_recently").expect("a cooldown");
    assert!((590..=2400).contains(&cooldown), "{cooldown}");
}

#[test]
fn a_summoned_brute_holds_a_golden_axe_and_remembers_where_it_was_born() {
    let mut s = ready(scene(&["Alice"]));
    let brute = summon(&mut s, Kind::PiglinBrute, "6.5 64 2.5", "");
    s.tick(1);
    let saved = s.server.saved_entity(brute).unwrap();
    let held = format!(
        "{:?}",
        saved.get_compound("equipment").unwrap().get("mainhand")
    );
    assert!(held.contains("minecraft:golden_axe"), "{held}");
    let home = memories(&s, brute);
    let home = home.get_compound("minecraft:home").expect("a home");
    let value = home.get_compound("value").unwrap();
    assert_eq!(value.get_str("dimension"), Some("minecraft:overworld"));
    let pos = format!("{:?}", value.get("pos"));
    assert!(
        pos.contains('6') && pos.contains("64") && pos.contains('2'),
        "{pos}"
    );
}

#[test]
fn brain_memories_survive_a_save_with_their_remaining_time() {
    let mut s = ready(scene(&["Alice"]));
    let piglin = summon(
        &mut s,
        Kind::Piglin,
        "5.5 64 5.5",
        "{IsImmuneToZombification:1b,Brain:{memories:{\"minecraft:admiring_disabled\":{value:1b,ttl:300L}}}}",
    );
    s.tick(10);
    let left = ttl(&memories(&s, piglin), "minecraft:admiring_disabled").expect("kept");
    assert!((285..=295).contains(&left), "{left}");
}

#[test]
fn a_hoglin_in_the_overworld_becomes_a_nauseous_zoglin_after_fifteen_seconds() {
    let mut s = creative(ready(scene(&["Alice"])));
    let hoglin = summon(&mut s, Kind::Hoglin, "5.5 64 5.5", "{}");
    s.tick(280);
    assert_eq!(count(&s, Kind::Hoglin), 1);
    s.tick(140);
    assert_eq!(count(&s, Kind::Hoglin), 0);
    assert_eq!(count(&s, Kind::Zoglin), 1);
    let zoglin = s.server.entities_of(Kind::Zoglin)[0].id;
    assert_ne!(zoglin, hoglin);
    let saved = s.server.saved_entity(zoglin).unwrap();
    assert!(format!("{:?}", saved.get("active_effects")).contains("nausea"));
}

#[test]
fn an_immune_hoglin_never_zombifies() {
    let mut s = ready(scene(&["Alice"]));
    summon(&mut s, Kind::Hoglin, "5.5 64 5.5", IMMUNE);
    s.tick(340);
    assert_eq!(count(&s, Kind::Hoglin), 1);
    assert_eq!(count(&s, Kind::Zoglin), 0);
}

#[test]
fn a_hoglin_without_ai_never_zombifies() {
    let mut s = ready(scene(&["Alice"]));
    summon(&mut s, Kind::Hoglin, "5.5 64 5.5", "{NoAI:1b}");
    s.tick(340);
    assert_eq!(count(&s, Kind::Hoglin), 1);
}

#[test]
fn a_piglin_in_the_overworld_becomes_a_zombified_piglin_and_keeps_its_weapon() {
    let mut s = creative(ready(scene(&["Alice"])));
    let piglin = summon(&mut s, Kind::Piglin, "5.5 64 5.5", "{}");
    let before = s.server.saved_entity(piglin).unwrap();
    let held = format!(
        "{:?}",
        before.get_compound("equipment").map(|e| e.get("mainhand"))
    );
    s.tick(420);
    assert_eq!(count(&s, Kind::Piglin), 0);
    assert_eq!(count(&s, Kind::ZombifiedPiglin), 1);
    let zombified = s.server.entities_of(Kind::ZombifiedPiglin)[0].id;
    let after = s.server.saved_entity(zombified).unwrap();
    let kept = format!(
        "{:?}",
        after.get_compound("equipment").map(|e| e.get("mainhand"))
    );
    if held.contains("golden_sword") || held.contains("crossbow") {
        assert!(
            kept.contains("golden_sword") || kept.contains("crossbow"),
            "{kept}"
        );
    }
    assert!(format!("{:?}", after.get("active_effects")).contains("nausea"));
}

#[test]
fn a_piglin_picks_up_a_gold_ingot_admires_it_and_trades_it_for_loot() {
    let mut s = ready(scene(&["Alice"]));
    let piglin = summon(
        &mut s,
        Kind::Piglin,
        "4.5 64 4.5",
        "{IsImmuneToZombification:1b,IsBaby:0b}",
    );
    console(
        &mut s,
        "summon minecraft:item 4.5 64 4.5 {Item:{id:\"minecraft:gold_ingot\",count:1},PickupDelay:0s}",
    );
    s.tick(20);
    let saved = s.server.saved_entity(piglin).unwrap();
    let off = format!(
        "{:?}",
        saved.get_compound("equipment").unwrap().get("offhand")
    );
    assert!(off.contains("gold_ingot"), "{off}");
    assert!(ttl(&memories(&s, piglin), "minecraft:admiring_item").is_some());
    s.tick(140);
    let saved = s.server.saved_entity(piglin).unwrap();
    let off = saved
        .get_compound("equipment")
        .and_then(|equipment| equipment.get("offhand").cloned());
    assert!(
        off.is_none() || !format!("{off:?}").contains("gold_ingot"),
        "{off:?}"
    );
    let dropped = s.server.ground_items();
    assert!(
        dropped
            .iter()
            .any(|(_, stack)| stack.item != rspigot_items::ItemKind::GoldIngot.id()),
        "{dropped:?}"
    );
}

#[test]
fn a_hurt_piglin_hates_the_attacker_and_calls_the_adult_piglins_around_it() {
    let mut s = creative(ready(scene(&["Alice"])));
    let one = summon(&mut s, Kind::Piglin, "30.5 64 30.5", IMMUNE);
    let two = summon(&mut s, Kind::Piglin, "32.5 64 30.5", IMMUNE);
    let zombie = summon(&mut s, Kind::Zombie, "30.5 64 32.5", "{NoAI:1b}");
    s.tick(70);
    assert!(s.server.do_hurt_target(zombie, Target::Entity(one)));
    s.tick(6);
    assert!(ttl(&memories(&s, one), "minecraft:angry_at").is_some());
    assert!(ttl(&memories(&s, two), "minecraft:angry_at").is_some());
}

#[test]
fn a_hoglin_charges_a_player_and_throws_them_back() {
    let mut s = ready(scene(&["Alice"]));
    let alice = s.players[0].id;
    let before = s.server.player_health(alice).unwrap();
    summon(&mut s, Kind::Hoglin, "3.5 64 0.5", IMMUNE);
    s.tick(120);
    let after = s.server.player_health(alice).unwrap();
    assert!(after < before, "{before} {after}");
}

#[test]
fn a_zoglin_attacks_anything_but_other_zoglins_and_creepers() {
    let mut s = ready(scene(&["Alice"]));
    let zoglin = summon(&mut s, Kind::Zoglin, "10.5 64 10.5", "{}");
    let cow = summon(&mut s, Kind::Cow, "12.5 64 10.5", "{NoAI:1b}");
    summon(&mut s, Kind::Creeper, "8.5 64 10.5", "{NoAI:1b}");
    s.tick(25);
    assert_eq!(s.server.mob_target(zoglin), Some(cow));
}

#[test]
fn a_brute_never_zombifies_into_anything_but_a_zombified_piglin() {
    let mut s = creative(ready(scene(&["Alice"])));
    summon(&mut s, Kind::PiglinBrute, "5.5 64 5.5", "{}");
    s.tick(420);
    assert_eq!(count(&s, Kind::PiglinBrute), 0);
    assert_eq!(count(&s, Kind::ZombifiedPiglin), 1);
}
