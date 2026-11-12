mod common;

use common::*;
use rspigot_core::commands::Target;
use rspigot_core::events::{ClientEvent, EntityStatus, Hand, Inbound, MetadataValue, PlayerInput};
use rspigot_core::plugins::{Answer, Question};
use rspigot_core::plugins::{BlockOrder, Order};
use rspigot_domain::poi::{Occupancy, PoiType};
use rspigot_domain::{BlockPos, EntityId, SectionPos, Vec3};
use rspigot_entities::EntityKind as Kind;

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn ready() -> Scene {
    let mut s = scene(&["Alice"]);
    stream_chunk_of(&mut s, BlockPos::new(1, 64, 1));
    s
}

fn summon(s: &mut Scene, kind: &str, nbt: &str, x: f64, z: f64) -> EntityId {
    let before = s.server.entities_of(Kind::from_name(kind).unwrap()).len();
    console(s, &format!("summon {kind} {x} 64 {z} {nbt}"));
    let found = s.server.entities_of(Kind::from_name(kind).unwrap());
    assert_eq!(found.len(), before + 1);
    found.last().unwrap().id
}

fn golem(s: &mut Scene, nbt: &str, x: f64, z: f64) -> EntityId {
    summon(s, "minecraft:iron_golem", nbt, x, z)
}

fn saved(s: &Scene, id: EntityId) -> rspigot_nbt::Compound {
    s.server.saved_entity(id).expect("the entity exists")
}

fn health(s: &Scene, kind: Kind, id: EntityId) -> f32 {
    s.server
        .entities_of(kind)
        .into_iter()
        .find(|entity| entity.id == id)
        .and_then(|entity| entity.health)
        .expect("a living entity")
}

fn velocity(s: &Scene, kind: Kind, id: EntityId) -> Vec3 {
    s.server
        .entities_of(kind)
        .into_iter()
        .find(|entity| entity.id == id)
        .map(|entity| entity.velocity)
        .expect("an entity")
}

fn set(s: &mut Scene, pos: BlockPos, kind: &str, properties: &[(&str, &str)]) {
    let order = if properties.is_empty() {
        BlockOrder::Set(kind.to_owned())
    } else {
        BlockOrder::SetWith {
            kind: kind.to_owned(),
            properties: properties
                .iter()
                .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
                .collect(),
        }
    };
    s.act(Order::Block { pos, order });
}

fn sounds(events: &[ClientEvent]) -> Vec<&'static str> {
    events
        .iter()
        .filter_map(|event| match event {
            ClientEvent::Sound { sound, .. } => Some(*sound),
            _ => None,
        })
        .collect()
}

fn entity_events(events: &[ClientEvent], id: EntityId) -> Vec<i8> {
    events
        .iter()
        .filter_map(|event| match event {
            ClientEvent::EntityStatus {
                entity,
                status: EntityStatus::Event(event),
            } if *entity == id => Some(*event),
            _ => None,
        })
        .collect()
}

#[test]
fn a_wild_golem_is_not_player_created_and_a_made_one_keeps_it_through_nbt() {
    let mut s = ready();
    let wild = golem(&mut s, "{NoAI:1b}", 1.5, 1.5);
    assert_eq!(saved(&s, wild).get_bool("PlayerCreated"), Some(false));
    let made = golem(&mut s, "{NoAI:1b,PlayerCreated:1b}", 3.5, 1.5);
    assert_eq!(saved(&s, made).get_bool("PlayerCreated"), Some(true));
}

#[test]
fn a_golem_keeps_its_anger_through_nbt() {
    let mut s = ready();
    let id = golem(&mut s, "{NoAI:1b,anger_end_time:5000L}", 1.5, 1.5);
    assert_eq!(saved(&s, id).get_i64("anger_end_time"), Some(5000));
}

#[test]
fn a_golem_syncs_the_player_created_flag_at_index_sixteen() {
    let mut s = ready();
    s.players[0].link.take();
    let id = golem(&mut s, "{NoAI:1b,PlayerCreated:1b}", 1.5, 1.5);
    s.tick(2);
    let entries: Vec<_> = s.players[0]
        .link
        .take()
        .into_iter()
        .filter_map(|event| match event {
            ClientEvent::EntityMetadata { entity, entries } if entity == id => Some(entries),
            _ => None,
        })
        .flatten()
        .collect();
    let flags = entries.iter().find(|entry| entry.index == 16);
    assert_eq!(
        flags.map(|entry| entry.value.clone()),
        Some(MetadataValue::Byte(1))
    );
}

#[test]
fn a_golem_has_the_vanilla_goals_and_targets() {
    let mut s = ready();
    let id = golem(&mut s, "{NoAI:1b}", 1.5, 1.5);
    let ai = s.server.mob_ai(id).expect("the golem has an AI");
    let goals: Vec<i32> = ai
        .goals()
        .goals()
        .iter()
        .map(rspigot_ai::WrappedGoal::priority)
        .collect();
    let targets: Vec<i32> = ai
        .targets()
        .goals()
        .iter()
        .map(rspigot_ai::WrappedGoal::priority)
        .collect();
    assert_eq!(goals, vec![1, 2, 2, 4, 5, 7, 8]);
    assert_eq!(targets, vec![1, 2, 3, 3, 4]);
}

#[test]
fn a_golem_hits_for_half_its_damage_plus_a_random_share_and_tosses_its_victim() {
    let mut s = ready();
    let attacker = golem(&mut s, "{NoAI:1b}", 1.5, 1.5);
    let husk = summon(&mut s, "minecraft:husk", "{NoAI:1b,OnGround:1b}", 2.5, 1.5);
    s.players[0].link.take();
    assert!(s.server.do_hurt_target(attacker, Target::Entity(husk)));
    s.server.deliver();
    let events = s.players[0].link.take();
    let damage = 20.0 - health(&s, Kind::Husk, husk);
    assert!((6.5..22.0).contains(&damage), "damage was {damage}");
    assert_eq!(entity_events(&events, attacker), vec![4]);
    assert!(sounds(&events).contains(&"minecraft:entity.iron_golem.attack"));
    let toss = velocity(&s, Kind::Husk, husk).y;
    assert!((toss - 0.8).abs() < 1.0e-6, "velocity was {toss}");
}

#[test]
fn the_golem_blow_rolls_seven_and_a_half_plus_zero_to_fourteen() {
    let mut s = ready();
    let attacker = golem(&mut s, "{NoAI:1b}", 1.5, 1.5);
    let mut seen = std::collections::BTreeSet::new();
    for round in 0..40 {
        let victim = summon(
            &mut s,
            "minecraft:ravager",
            "{NoAI:1b}",
            2.5,
            1.5 + f64::from(round % 8),
        );
        assert!(s.server.do_hurt_target(attacker, Target::Entity(victim)));
        let taken = 100.0 - health(&s, Kind::Ravager, victim);
        let extra = taken - 7.5;
        assert_eq!(extra.fract(), 0.0, "damage was {taken}");
        assert!((0.0..=14.0).contains(&extra), "damage was {taken}");
        seen.insert(extra as i32);
        console(&mut s, "kill @e[type=ravager]");
    }
    assert!(seen.len() > 5, "{seen:?}");
}

#[test]
fn a_golem_does_not_toss_a_victim_that_resists_knockback() {
    let mut s = ready();
    let attacker = golem(&mut s, "{NoAI:1b}", 1.5, 1.5);
    let other = golem(&mut s, "{NoAI:1b}", 3.5, 1.5);
    assert!(s.server.do_hurt_target(attacker, Target::Entity(other)));
    assert!(velocity(&s, Kind::IronGolem, other).y.abs() < 1.0e-6);
}

#[test]
fn a_golem_hits_players_too() {
    let mut s = ready();
    let attacker = golem(&mut s, "{NoAI:1b}", 1.5, 1.5);
    let alice = s.players[0].id;
    assert!(s.server.do_hurt_target(attacker, Target::Player(alice)));
    let health = s.server.player_health(alice).expect("alive");
    assert!(health <= 20.0 - 4.5, "health was {health}");
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

fn ingots(s: &mut Scene) -> i32 {
    let Answer::Inventory(state) = s.ask(Question::Inventory(s.players[0].id)) else {
        panic!("expected Alice's inventory");
    };
    state
        .slots
        .iter()
        .flatten()
        .filter(|item| item.key == "minecraft:iron_ingot")
        .map(|item| item.count)
        .sum()
}

#[test]
fn an_iron_ingot_repairs_twenty_five_health_and_is_consumed() {
    let mut s = ready();
    console(&mut s, "give Alice minecraft:iron_ingot 3");
    let id = golem(&mut s, "{NoAI:1b,Health:40f}", 1.5, 1.5);
    s.players[0].link.take();
    interact(&mut s, id);
    s.server.deliver();
    assert_eq!(health(&s, Kind::IronGolem, id), 65.0);
    assert_eq!(ingots(&mut s), 2);
    let events = s.players[0].link.take();
    assert!(sounds(&events).contains(&"minecraft:entity.iron_golem.repair"));
}

#[test]
fn the_repair_is_capped_at_the_maximum_health() {
    let mut s = ready();
    console(&mut s, "give Alice minecraft:iron_ingot 1");
    let id = golem(&mut s, "{NoAI:1b,Health:90f}", 1.5, 1.5);
    interact(&mut s, id);
    assert_eq!(health(&s, Kind::IronGolem, id), 100.0);
    assert_eq!(ingots(&mut s), 0);
}

#[test]
fn a_golem_at_full_health_refuses_the_ingot() {
    let mut s = ready();
    console(&mut s, "give Alice minecraft:iron_ingot 1");
    let id = golem(&mut s, "{NoAI:1b}", 1.5, 1.5);
    s.players[0].link.take();
    interact(&mut s, id);
    s.server.deliver();
    assert_eq!(health(&s, Kind::IronGolem, id), 100.0);
    assert_eq!(ingots(&mut s), 1);
    assert!(!sounds(&s.players[0].link.take()).contains(&"minecraft:entity.iron_golem.repair"));
}

#[test]
fn only_iron_ingots_repair() {
    let mut s = ready();
    console(&mut s, "give Alice minecraft:iron_block 1");
    let id = golem(&mut s, "{NoAI:1b,Health:40f}", 1.5, 1.5);
    interact(&mut s, id);
    assert_eq!(health(&s, Kind::IronGolem, id), 40.0);
}

#[test]
fn a_golem_plays_its_damage_sound_when_a_crack_appears() {
    let mut s = ready();
    let attacker = golem(&mut s, "{NoAI:1b}", 1.5, 1.5);
    let cracked = golem(&mut s, "{NoAI:1b,Health:76f}", 3.5, 1.5);
    s.players[0].link.take();
    assert!(s.server.do_hurt_target(attacker, Target::Entity(cracked)));
    s.server.deliver();
    let events = s.players[0].link.take();
    let heard = sounds(&events);
    assert!(heard.contains(&"minecraft:entity.iron_golem.hurt"));
    assert!(heard.contains(&"minecraft:entity.iron_golem.damage"));
}

#[test]
fn a_golem_stays_quiet_when_no_new_crack_appears() {
    let mut s = ready();
    let attacker = golem(&mut s, "{NoAI:1b}", 1.5, 1.5);
    let solid = golem(&mut s, "{NoAI:1b}", 3.5, 1.5);
    s.players[0].link.take();
    assert!(s.server.do_hurt_target(attacker, Target::Entity(solid)));
    s.server.deliver();
    let heard = sounds(&s.players[0].link.take());
    assert!(heard.contains(&"minecraft:entity.iron_golem.hurt"));
    assert!(!heard.contains(&"minecraft:entity.iron_golem.damage"));
}

#[test]
fn a_golem_touching_a_hostile_mob_targets_it_one_tick_in_twenty() {
    let mut s = ready();
    let id = golem(&mut s, "{NoAI:1b}", 1.5, 1.5);
    let zombie = summon(&mut s, "minecraft:zombie", "{NoAI:1b}", 1.5, 1.5);
    let mut found = None;
    for tick in 0..600 {
        s.tick(1);
        if s.server.mob_target(id) == Some(zombie) {
            found = Some(tick);
            break;
        }
    }
    assert!(found.is_some());
    assert_eq!(s.server.mob_target(id), Some(zombie));
}

#[test]
fn a_golem_does_not_target_what_it_only_stands_near() {
    let mut s = ready();
    let id = golem(&mut s, "{NoAI:1b}", 1.5, 1.5);
    summon(&mut s, "minecraft:zombie", "{NoAI:1b}", 6.5, 1.5);
    summon(&mut s, "minecraft:cow", "{NoAI:1b}", 1.5, 1.5);
    for _ in 0..300 {
        s.tick(1);
    }
    assert_eq!(s.server.mob_target(id), None);
}

#[test]
fn a_golem_never_targets_a_creeper_by_collision() {
    let mut s = ready();
    let id = golem(&mut s, "{NoAI:1b}", 1.5, 1.5);
    summon(&mut s, "minecraft:creeper", "{NoAI:1b}", 1.5, 1.5);
    for _ in 0..600 {
        s.tick(1);
        assert_eq!(s.server.mob_target(id), None);
    }
}

fn angry_scene() -> Scene {
    let server = configured(|config| config.rules.universal_anger = true);
    let mut s = scene_on(server, &["Alice"], Vec::new());
    stream_chunk_of(&mut s, BlockPos::new(1, 64, 1));
    s
}

#[test]
fn a_wild_golem_attacks_the_player_it_is_angry_at_but_a_made_one_never_does() {
    let mut s = angry_scene();
    let alice = s.server.player_entity_id(s.players[0].id).expect("Alice");
    let wild = golem(&mut s, "{anger_end_time:1000000L}", 1.5, 1.5);
    let made = golem(
        &mut s,
        "{PlayerCreated:1b,anger_end_time:1000000L}",
        9.5,
        9.5,
    );
    let mut wild_targeted = false;
    for _ in 0..300 {
        s.tick(1);
        wild_targeted |= s.server.mob_target(wild) == Some(alice);
        assert_ne!(s.server.mob_target(made), Some(alice));
    }
    assert!(wild_targeted);
}

#[test]
fn a_village_needs_an_occupied_poi_so_blocks_alone_are_not_enough() {
    let mut s = ready();
    set(&mut s, BlockPos::new(2, 64, 2), "minecraft:bell", &[]);
    set(
        &mut s,
        BlockPos::new(4, 64, 2),
        "minecraft:red_bed",
        &[("part", "head")],
    );
    s.tick(1);
    assert_eq!(
        s.server.poi().type_at(BlockPos::new(2, 64, 2)),
        Some(PoiType::Meeting)
    );
    assert_eq!(
        s.server.poi().type_at(BlockPos::new(4, 64, 2)),
        Some(PoiType::Home)
    );
    assert_eq!(
        s.server
            .poi()
            .sections_to_village(SectionPos::of(BlockPos::new(2, 64, 2))),
        7
    );
}

#[test]
fn a_claimed_bell_makes_the_surroundings_a_village() {
    let mut s = ready();
    set(&mut s, BlockPos::new(2, 64, 2), "minecraft:bell", &[]);
    s.tick(1);
    let bell = BlockPos::new(2, 64, 2);
    let taken = s
        .server
        .poi_mut()
        .take(|kind| kind == PoiType::Meeting, |_, _| true, bell, 4);
    assert_eq!(taken, Some(bell));
    let here = SectionPos::of(bell);
    assert_eq!(s.server.poi().sections_to_village(here), 0);
    assert_eq!(s.server.poi().sections_to_village(here.offset(1, 0, 1)), 1);
    assert_eq!(s.server.poi().sections_to_village(here.offset(7, 0, 0)), 7);
    let found = s
        .server
        .poi()
        .in_range(|_| true, bell, 8, Occupancy::IsOccupied);
    assert_eq!(found.len(), 1);
    set(&mut s, bell, "minecraft:air", &[]);
    s.tick(1);
    assert_eq!(s.server.poi().sections_to_village(here), 7);
    assert_eq!(s.server.poi().record_count(), 0);
}

#[test]
fn a_golem_outside_a_village_walks_back_towards_a_claimed_one() {
    let mut s = ready();
    let bell = BlockPos::new(-24, 64, 2);
    set(&mut s, bell, "minecraft:bell", &[]);
    s.tick(1);
    assert_eq!(s.server.poi().type_at(bell), Some(PoiType::Meeting));
    s.server.poi_mut().take(|_| true, |_, _| true, bell, 4);
    let id = golem(&mut s, "", 1.5, 1.5);
    let x_of = |s: &Scene| {
        s.server
            .entities_of(Kind::IronGolem)
            .iter()
            .find(|entity| entity.id == id)
            .map(|entity| entity.position.x)
            .expect("the golem")
    };
    let start = x_of(&s);
    let mut moved = false;
    for _ in 0..60 {
        s.tick(50);
        if x_of(&s) < start - 4.0 {
            moved = true;
            break;
        }
    }
    assert!(moved, "the golem stayed at {}", x_of(&s));
}
