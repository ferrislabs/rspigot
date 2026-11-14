mod common;

use common::*;
use rspigot_core::events::{ClientEvent, Hand, InputState, MovementAction, PlayerInput};
use rspigot_core::plugins::EventKind;
use rspigot_domain::{EntityId, Vec3};
use rspigot_entities::EntityKind as Kind;

fn world() -> Scene {
    let mut s = scene(&["Alice"]);
    stream_chunk_of(&mut s, rspigot_domain::BlockPos::new(1, 64, 1));
    s
}

fn console(s: &mut Scene, line: &str) {
    s.server
        .handle(rspigot_core::events::Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn summon(s: &mut Scene, kind: &str, nbt: &str) -> EntityId {
    let id = format!("minecraft:{kind}");
    let before = s.server.entities_of(entity_kind(kind)).len();
    console(s, &format!("summon {id} 3.5 64 0.5 {nbt}"));
    let all = s.server.entities_of(entity_kind(kind));
    assert_eq!(all.len(), before + 1, "{kind} summoned");
    all.last().expect("the summoned mob").id
}

fn entity_kind(kind: &str) -> Kind {
    match kind {
        "horse" => Kind::Horse,
        "donkey" => Kind::Donkey,
        "mule" => Kind::Mule,
        "skeleton_horse" => Kind::SkeletonHorse,
        _ => Kind::ZombieHorse,
    }
}

fn interact(s: &mut Scene, target: EntityId, sneaking: bool) {
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Interact {
            target,
            hand: Hand::Main,
            sneaking,
            location: Vec3::new(0.0, 0.5, 0.0),
        },
    );
    s.tick(2);
}

fn saved(s: &Scene, id: EntityId) -> String {
    format!("{:?}", s.server.saved_entity(id).expect("a saved mob"))
}

const SADDLE: &str = "equipment:{saddle:{id:\"minecraft:saddle\",count:1}}";

#[test]
fn a_horse_summons_with_its_vanilla_state_and_persists_it() {
    let mut s = world();
    let horse = summon(
        &mut s,
        "horse",
        "{Tame:1b,Temper:42,Variant:260,Bred:1b,EatingHaystack:0b}",
    );
    let nbt = saved(&s, horse);
    assert!(nbt.contains("\"Tame\": Byte(1)"), "{nbt}");
    assert!(nbt.contains("\"Temper\": Int(42)"), "{nbt}");
    assert!(nbt.contains("\"Variant\": Int(260)"), "{nbt}");
    assert!(nbt.contains("Bukkit.MaxDomestication"), "{nbt}");
}

#[test]
fn feeding_a_hurt_horse_heals_it_and_a_golden_carrot_puts_a_tame_adult_in_love() {
    let mut s = world();
    let horse = summon(&mut s, "horse", "{Tame:1b,Health:10f}");
    console(&mut s, "give Alice minecraft:golden_carrot 2");
    interact(&mut s, horse, false);
    let nbt = saved(&s, horse);
    assert!(nbt.contains("Health"), "{nbt}");
    assert!(!nbt.contains("Float(10.0)"), "{nbt}");
    assert!(!nbt.contains("\"InLove\": Int(0)"), "{nbt}");
}

#[test]
fn an_untamed_horse_gets_angry_at_a_non_food_item_and_is_ridden_with_an_empty_hand() {
    let mut s = world();
    let horse = summon(&mut s, "horse", "{}");
    console(&mut s, "give Alice minecraft:stick 1");
    s.players[0].link.take();
    interact(&mut s, horse, false);
    assert!(
        !s.players[0]
            .link
            .take()
            .iter()
            .any(|event| matches!(event, ClientEvent::Passengers { .. })),
        "a stick does not mount"
    );
    console(&mut s, "clear Alice");
    interact(&mut s, horse, false);
    let events = s.players[0].link.take();
    assert!(
        events.iter().any(|event| matches!(event, ClientEvent::Passengers { vehicle, passengers } if *vehicle == horse && passengers.len() == 1)),
        "{events:?}"
    );
}

#[test]
fn a_rider_with_enough_temper_tames_the_horse_and_becomes_its_owner() {
    let mut s = world();
    let horse = summon(&mut s, "horse", "{Temper:99}");
    interact(&mut s, horse, false);
    s.tick(900);
    let nbt = saved(&s, horse);
    assert!(nbt.contains("\"Tame\": Byte(1)"), "{nbt}");
    assert!(nbt.contains("Owner"), "{nbt}");
}

#[test]
fn a_rider_with_no_temper_is_thrown_off_and_the_horse_never_tames() {
    let mut s = world();
    let horse = summon(&mut s, "horse", "{Temper:0}");
    interact(&mut s, horse, false);
    let early = s.players[0].link.take();
    s.tick(900);
    let events = [early, s.players[0].link.take()].concat();
    assert!(
        events.iter().any(|event| matches!(event, ClientEvent::Passengers { vehicle, passengers } if *vehicle == horse && passengers.is_empty())),
        "the rider was thrown"
    );
    assert!(saved(&s, horse).contains("\"Tame\": Byte(0)"));
}

#[test]
fn a_tame_horse_takes_a_saddle_and_a_horse_armor_from_the_hand_and_then_carries_its_rider() {
    let mut s = world();
    let horse = summon(&mut s, "horse", "{Tame:1b}");
    console(&mut s, "give Alice minecraft:saddle 1");
    s.players[0].link.take();
    interact(&mut s, horse, false);
    let events = s.players[0].link.take();
    assert!(
        events.iter().any(
            |event| matches!(event, ClientEvent::EntityEquipment { entity, .. } if *entity == horse)
        ),
        "{events:?}"
    );
    assert!(saved(&s, horse).contains("saddle"));
    console(&mut s, "give Alice minecraft:iron_horse_armor 1");
    interact(&mut s, horse, false);
    assert!(saved(&s, horse).contains("body"));
    s.tick(2);
    let attributes = s.players[0].link.take();
    assert!(
        attributes
            .iter()
            .any(|event| format!("{event:?}").contains("armor")),
        "the armor attribute follows the horse armor"
    );
    console(&mut s, "clear Alice");
    interact(&mut s, horse, false);
    assert!(
        s.players[0]
            .link
            .take()
            .iter()
            .any(|event| matches!(event, ClientEvent::Passengers { vehicle, passengers } if *vehicle == horse && passengers.len() == 1))
    );
}

#[test]
fn a_donkey_takes_a_chest_only_when_tame_and_a_mule_keeps_it_in_its_save() {
    let mut s = world();
    let donkey = summon(&mut s, "donkey", "{}");
    console(&mut s, "give Alice minecraft:chest 2");
    interact(&mut s, donkey, false);
    assert!(!saved(&s, donkey).contains("\"ChestedHorse\": Byte(1)"));
    console(&mut s, "give Alice minecraft:chest 2");
    let mule = summon(&mut s, "mule", "{Tame:1b}");
    interact(&mut s, mule, false);
    assert!(
        saved(&s, mule).contains("\"ChestedHorse\": Byte(1)"),
        "{}",
        saved(&s, mule)
    );
}

#[test]
fn a_saddled_rider_jumps_with_a_cancellable_horse_jump_event_and_the_horse_rears() {
    let mut s = scene_with(&["Alice"], vec![observe(1, EventKind::HorseJump)]);
    stream_chunk_of(&mut s, rspigot_domain::BlockPos::new(1, 64, 1));
    let horse = summon(&mut s, "horse", &format!("{{Tame:1b,NoAI:1b,{SADDLE}}}"));
    interact(&mut s, horse, false);
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Movement(MovementAction::StartRidingJump(45)),
    );
    s.tick(2);
    let jumps = settled_for(&s.plugin, EventKind::HorseJump);
    assert_eq!(jumps.len(), 1, "{:?}", fired_kinds(&s.plugin));
}

#[test]
fn a_tame_horse_opens_its_inventory_screen_to_a_sneaking_player() {
    let mut s = world();
    let horse = summon(&mut s, "horse", "{Tame:1b}");
    s.players[0].link.take();
    interact(&mut s, horse, true);
    let events = s.players[0].link.take();
    assert!(
        events.iter().any(|event| matches!(event, ClientEvent::OpenMountScreen { entity, columns: 0, .. } if *entity == horse)),
        "{events:?}"
    );
    let donkey = summon(&mut s, "donkey", "{Tame:1b,ChestedHorse:1b}");
    interact(&mut s, donkey, true);
    let events = s.players[0].link.take();
    assert!(
        events.iter().any(|event| matches!(event, ClientEvent::OpenMountScreen { entity, columns: 5, .. } if *entity == donkey)),
        "{events:?}"
    );
}

#[test]
fn a_horse_and_a_donkey_in_love_breed_a_mule_foal() {
    let mut s = world();
    summon(&mut s, "horse", "{Tame:1b,InLove:600}");
    summon(&mut s, "donkey", "{Tame:1b,InLove:600}");
    s.tick(400);
    let mules = s.server.entities_of(Kind::Mule);
    assert_eq!(mules.len(), 1, "a mule was born");
}

#[test]
fn a_trapped_skeleton_horse_springs_with_three_more_skeleton_riders_near_a_player() {
    let mut s = world();
    let horse = summon(&mut s, "skeleton_horse", "{SkeletonTrap:1b}");
    s.tick(10);
    assert!(s.server.entities_of(Kind::SkeletonHorse).len() >= 4);
    assert!(s.server.entities_of(Kind::Skeleton).len() >= 4);
    assert!(saved(&s, horse).contains("\"SkeletonTrap\": Byte(0)"));
}

#[test]
fn a_shift_input_lets_the_rider_down_from_a_horse() {
    let mut s = world();
    let horse = summon(&mut s, "horse", "{Tame:1b,NoAI:1b}");
    interact(&mut s, horse, false);
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Input(InputState {
            shift: true,
            ..Default::default()
        }),
    );
    s.tick(2);
    let events = s.players[0].link.take();
    assert!(
        events.iter().any(|event| matches!(event, ClientEvent::Passengers { vehicle, passengers } if *vehicle == horse && passengers.is_empty())),
        "{events:?}"
    );
}

#[test]
fn a_zombie_horse_burns_in_daylight_unless_it_wears_body_armor() {
    let mut s = world();
    console(&mut s, "time set 1000");
    let bare = summon(&mut s, "zombie_horse", "{NoAI:1b,Tame:1b}");
    let armored = summon(
        &mut s,
        "zombie_horse",
        "{NoAI:1b,Tame:1b,equipment:{body:{id:\"minecraft:iron_horse_armor\",count:1}}}",
    );
    let mut burning = false;
    for _ in 0..300 {
        s.tick(1);
        if s.server
            .entity_fire_ticks(bare)
            .is_some_and(|ticks| ticks > 0)
        {
            burning = true;
            break;
        }
    }
    assert!(burning, "the zombie horse never caught fire");
    assert_eq!(s.server.entity_fire_ticks(armored), Some(0));
}

#[test]
fn a_dying_chested_donkey_drops_its_chest_and_its_stored_items() {
    let mut s = world();
    let donkey = summon(
        &mut s,
        "donkey",
        "{Tame:1b,ChestedHorse:1b,Items:[{Slot:2b,id:\"minecraft:apple\",count:3}]}",
    );
    assert!(saved(&s, donkey).contains("apple"));
    console(&mut s, "kill @e[type=donkey]");
    s.tick(2);
    let dropped: String = s
        .server
        .entities_of(Kind::Item)
        .iter()
        .filter_map(|item| s.server.saved_entity(item.id))
        .map(|nbt| format!("{nbt:?}"))
        .collect();
    assert!(dropped.contains("minecraft:chest"), "{dropped}");
    assert!(dropped.contains("minecraft:apple"), "{dropped}");
}

#[test]
fn shearing_a_horse_takes_its_body_armor_before_its_saddle() {
    let mut s = world();
    let horse = summon(
        &mut s,
        "horse",
        "{Tame:1b,equipment:{saddle:{id:\"minecraft:saddle\",count:1},body:{id:\"minecraft:iron_horse_armor\",count:1}}}",
    );
    console(&mut s, "give Alice minecraft:shears 1");
    interact(&mut s, horse, false);
    let nbt = saved(&s, horse);
    assert!(!nbt.contains("iron_horse_armor"), "{nbt}");
    assert!(nbt.contains("saddle"), "{nbt}");
    interact(&mut s, horse, false);
    assert!(!saved(&s, horse).contains("saddle"));
}
