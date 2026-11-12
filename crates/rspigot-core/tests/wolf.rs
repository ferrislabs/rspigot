mod common;

use common::*;
use rspigot_core::config::PluginSettings;
use rspigot_core::events::{ClientEvent, EntityStatus, Hand, Inbound, PlayerInput};
use rspigot_core::plugins::{Answer, EventKind, HandlerSpec, Question};
use rspigot_domain::{BlockPos, EntityId, Vec3};
use rspigot_entities::EntityKind as Kind;

fn waiting_scene(handlers: Vec<HandlerSpec>) -> Scene {
    let mut server = server(PluginSettings {
        decision_ticks: 1_000,
        ..PluginSettings::default()
    });
    let plugin = connect_with(&mut server, 1, "tester", handlers);
    let alice = join(&mut server, "Alice");
    server.deliver();
    alice.link.take();
    let mut scene = Scene {
        server,
        plugin,
        players: vec![alice],
    };
    stream_chunk_of(&mut scene, BlockPos::new(1, 64, 1));
    scene
}

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn summon(s: &mut Scene, line: &str) -> EntityId {
    let before = s.server.entities_of(Kind::Wolf).len();
    console(s, line);
    let found = s.server.entities_of(Kind::Wolf);
    assert_eq!(found.len(), before + 1, "{line}");
    found.last().map(|entity| entity.id).unwrap()
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

fn attack(s: &mut Scene, target: EntityId) {
    let alice = s.players[0].clone();
    send_input(&mut s.server, &alice, PlayerInput::Attack(target));
}

fn owner_of_alice(s: &Scene) -> String {
    let id = s.players[0].id.0.as_u128();
    format!(
        "[I;{},{},{},{}]",
        (id >> 96) as u32 as i32,
        (id >> 64) as u32 as i32,
        (id >> 32) as u32 as i32,
        id as u32 as i32
    )
}

fn tame_wolf(s: &mut Scene, extra: &str) -> EntityId {
    let owner = owner_of_alice(s);
    let line = format!(
        "summon minecraft:wolf 1.5 64 1.5 {{NoAI:1b,Owner:{owner},attributes:[{{id:\"minecraft:max_health\",base:40d}}],Health:40f{extra}}}"
    );
    summon(s, &line)
}

fn wild_wolf(s: &mut Scene, extra: &str) -> EntityId {
    summon(
        s,
        &format!("summon minecraft:wolf 1.5 64 1.5 {{NoAI:1b{extra}}}"),
    )
}

fn nbt(s: &Scene, id: EntityId) -> rspigot_nbt::Compound {
    s.server.saved_entity(id).expect("the wolf is saved")
}

fn held_count(s: &mut Scene, key: &str) -> i32 {
    let Answer::Inventory(state) = s.ask(Question::Inventory(s.players[0].id)) else {
        panic!("expected Alice's inventory");
    };
    state
        .slots
        .iter()
        .flatten()
        .filter(|item| item.key == key)
        .map(|item| item.count)
        .sum()
}

fn statuses(events: &[ClientEvent], entity: EntityId) -> Vec<EntityStatus> {
    events
        .iter()
        .filter_map(|event| match event {
            ClientEvent::EntityStatus { entity: e, status } if *e == entity => Some(*status),
            _ => None,
        })
        .collect()
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

fn is_tame(s: &Scene, id: EntityId) -> bool {
    nbt(s, id).contains_key("Owner")
}

fn body_id(s: &Scene, id: EntityId) -> Option<String> {
    nbt(s, id)
        .get_compound("equipment")
        .and_then(|equipment| equipment.get_compound("body"))
        .and_then(|body| body.get_str("id"))
        .map(str::to_owned)
}

fn armor_damage(s: &Scene, id: EntityId) -> i32 {
    nbt(s, id)
        .get_compound("equipment")
        .and_then(|equipment| equipment.get_compound("body"))
        .and_then(|body| body.get_compound("components"))
        .and_then(|components| components.get_i32("minecraft:damage"))
        .unwrap_or(0)
}

const WORN_ARMOR: &str = ",equipment:{body:{id:\"minecraft:wolf_armor\",count:1}}";

#[test]
fn a_bone_tames_a_wild_wolf_after_a_few_tries_and_sits_it_down() {
    let mut s = waiting_scene(vec![]);
    console(&mut s, "give Alice minecraft:bone 60");
    let wolf = wild_wolf(&mut s, "");
    s.players[0].link.take();
    let mut tries = 0;
    while !is_tame(&s, wolf) {
        interact(&mut s, wolf);
        tries += 1;
        assert!(tries < 60, "never tamed");
    }
    assert_eq!(held_count(&mut s, "minecraft:bone"), 60 - tries);
    let saved = nbt(&s, wolf);
    assert_eq!(saved.get_bool("Sitting"), Some(true));
    assert_eq!(saved.get_f64("Health"), Some(40.0));
    let events = s.players[0].link.take();
    let seen = statuses(&events, wolf);
    assert!(seen.contains(&EntityStatus::Event(7)), "{seen:?}");
    assert_eq!(
        seen.iter()
            .filter(|status| **status == EntityStatus::Event(6))
            .count() as i32,
        tries - 1
    );
}

#[test]
fn a_cancelled_tame_keeps_the_wolf_wild_and_smokes() {
    let mut s = waiting_scene(vec![deciding(1, EventKind::EntityTame)]);
    console(&mut s, "give Alice minecraft:bone 60");
    let wolf = wild_wolf(&mut s, "");
    let mut asked = None;
    for _ in 0..60 {
        interact(&mut s, wolf);
        let found = asked_for(&s.plugin, EventKind::EntityTame);
        if let Some(event) = found.last() {
            asked = Some(event.id);
            break;
        }
    }
    let event = asked.expect("a tame is asked one time in three");
    s.players[0].link.take();
    rule_on(&mut s.server, 1, event, true, None);
    assert!(!is_tame(&s, wolf));
    let seen = statuses(&s.players[0].link.take(), wolf);
    assert!(seen.contains(&EntityStatus::Event(6)), "{seen:?}");
}

#[test]
fn an_angry_wolf_does_not_take_the_bone() {
    let mut s = waiting_scene(vec![]);
    console(&mut s, "give Alice minecraft:bone 5");
    let wolf = wild_wolf(&mut s, ",anger_end_time:99999999L");
    for _ in 0..5 {
        interact(&mut s, wolf);
    }
    assert_eq!(held_count(&mut s, "minecraft:bone"), 5);
    assert!(!is_tame(&s, wolf));
}

#[test]
fn the_owner_toggles_sitting_with_an_empty_hand_and_strangers_cannot() {
    let mut s = waiting_scene(vec![]);
    let wolf = tame_wolf(&mut s, "");
    assert_eq!(nbt(&s, wolf).get_bool("Sitting"), Some(false));
    interact(&mut s, wolf);
    assert_eq!(nbt(&s, wolf).get_bool("Sitting"), Some(true));
    interact(&mut s, wolf);
    assert_eq!(nbt(&s, wolf).get_bool("Sitting"), Some(false));
    let stranger = summon(
        &mut s,
        "summon minecraft:wolf 1.5 64 1.5 {NoAI:1b,Owner:[I;1,2,3,4]}",
    );
    interact(&mut s, stranger);
    assert_eq!(nbt(&s, stranger).get_bool("Sitting"), Some(false));
}

#[test]
fn the_owner_dyes_the_collar_but_a_stranger_cannot() {
    let mut s = waiting_scene(vec![]);
    console(&mut s, "give Alice minecraft:blue_dye 3");
    let wolf = tame_wolf(&mut s, "");
    assert_eq!(nbt(&s, wolf).get_i64("CollarColor"), Some(14));
    interact(&mut s, wolf);
    assert_eq!(nbt(&s, wolf).get_i64("CollarColor"), Some(11));
    assert_eq!(held_count(&mut s, "minecraft:blue_dye"), 2);
    interact(&mut s, wolf);
    assert_eq!(held_count(&mut s, "minecraft:blue_dye"), 2);
    assert_eq!(nbt(&s, wolf).get_bool("Sitting"), Some(false));
    let stranger = summon(
        &mut s,
        "summon minecraft:wolf 1.5 64 1.5 {NoAI:1b,Owner:[I;1,2,3,4]}",
    );
    interact(&mut s, stranger);
    assert_eq!(nbt(&s, stranger).get_i64("CollarColor"), Some(14));
    assert_eq!(held_count(&mut s, "minecraft:blue_dye"), 2);
}

#[test]
fn food_heals_a_hurt_tame_wolf_by_twice_the_nutrition() {
    let mut s = waiting_scene(vec![]);
    console(&mut s, "give Alice minecraft:cooked_beef 3");
    let owner = owner_of_alice(&s);
    let wolf = summon(
        &mut s,
        &format!(
            "summon minecraft:wolf 1.5 64 1.5 {{NoAI:1b,Owner:{owner},attributes:[{{id:\"minecraft:max_health\",base:40d}}],Health:10f}}"
        ),
    );
    interact(&mut s, wolf);
    assert_eq!(nbt(&s, wolf).get_f64("Health"), Some(26.0));
    assert_eq!(held_count(&mut s, "minecraft:cooked_beef"), 2);
    interact(&mut s, wolf);
    interact(&mut s, wolf);
    assert_eq!(nbt(&s, wolf).get_f64("Health"), Some(40.0));
    assert_eq!(held_count(&mut s, "minecraft:cooked_beef"), 0);
}

#[test]
fn a_cancelled_regain_still_spends_the_food() {
    let mut s = waiting_scene(vec![deciding(1, EventKind::EntityRegainHealth)]);
    console(&mut s, "give Alice minecraft:cooked_beef 2");
    let owner = owner_of_alice(&s);
    let wolf = summon(
        &mut s,
        &format!(
            "summon minecraft:wolf 1.5 64 1.5 {{NoAI:1b,Owner:{owner},attributes:[{{id:\"minecraft:max_health\",base:40d}}],Health:10f}}"
        ),
    );
    interact(&mut s, wolf);
    let asked = last_asked(&s.plugin, EventKind::EntityRegainHealth);
    assert_eq!(held_count(&mut s, "minecraft:cooked_beef"), 1);
    rule_on(&mut s.server, 1, asked.id, true, None);
    assert_eq!(nbt(&s, wolf).get_f64("Health"), Some(10.0));
}

#[test]
fn rabbit_stew_leaves_a_bowl() {
    let mut s = waiting_scene(vec![]);
    console(&mut s, "give Alice minecraft:rabbit_stew 1");
    let owner = owner_of_alice(&s);
    let wolf = summon(
        &mut s,
        &format!(
            "summon minecraft:wolf 1.5 64 1.5 {{NoAI:1b,Owner:{owner},attributes:[{{id:\"minecraft:max_health\",base:40d}}],Health:10f}}"
        ),
    );
    interact(&mut s, wolf);
    assert_eq!(nbt(&s, wolf).get_f64("Health"), Some(30.0));
    assert_eq!(held_count(&mut s, "minecraft:rabbit_stew"), 0);
    assert_eq!(held_count(&mut s, "minecraft:bowl"), 1);
}

#[test]
fn food_on_a_healthy_wolf_starts_love_mode_instead() {
    let mut s = waiting_scene(vec![]);
    console(&mut s, "give Alice minecraft:beef 2");
    let wolf = tame_wolf(&mut s, "");
    interact(&mut s, wolf);
    assert_eq!(nbt(&s, wolf).get_i32("InLove"), Some(600));
    assert_eq!(held_count(&mut s, "minecraft:beef"), 1);
    assert_eq!(nbt(&s, wolf).get_bool("Sitting"), Some(false));
}

#[test]
fn the_owner_puts_wolf_armor_on_an_adult_wolf_only() {
    let mut s = waiting_scene(vec![]);
    console(&mut s, "give Alice minecraft:wolf_armor 3");
    let pup = tame_wolf(&mut s, ",Age:-24000");
    interact(&mut s, pup);
    assert_eq!(body_id(&s, pup), None);
    assert_eq!(held_count(&mut s, "minecraft:wolf_armor"), 3);
    let stranger = summon(
        &mut s,
        "summon minecraft:wolf 1.5 64 1.5 {NoAI:1b,Owner:[I;1,2,3,4]}",
    );
    interact(&mut s, stranger);
    assert_eq!(body_id(&s, stranger), None);
    let wolf = tame_wolf(&mut s, "");
    interact(&mut s, wolf);
    assert_eq!(body_id(&s, wolf).as_deref(), Some("minecraft:wolf_armor"));
    assert_eq!(held_count(&mut s, "minecraft:wolf_armor"), 2);
    interact(&mut s, wolf);
    assert_eq!(held_count(&mut s, "minecraft:wolf_armor"), 2);
}

#[test]
fn armadillo_scutes_repair_the_armor_of_a_sitting_wolf() {
    let mut s = waiting_scene(vec![]);
    console(&mut s, "give Alice minecraft:armadillo_scute 5");
    let wolf = tame_wolf(
        &mut s,
        ",Sitting:1b,equipment:{body:{id:\"minecraft:wolf_armor\",count:1,components:{\"minecraft:damage\":40}}}",
    );
    interact(&mut s, wolf);
    assert_eq!(armor_damage(&s, wolf), 32);
    assert_eq!(held_count(&mut s, "minecraft:armadillo_scute"), 4);
    for _ in 0..5 {
        interact(&mut s, wolf);
    }
    assert_eq!(armor_damage(&s, wolf), 0);
    assert_eq!(held_count(&mut s, "minecraft:armadillo_scute"), 0);
}

#[test]
fn a_standing_wolf_does_not_get_its_armor_repaired() {
    let mut s = waiting_scene(vec![]);
    console(&mut s, "give Alice minecraft:armadillo_scute 4");
    let wolf = tame_wolf(
        &mut s,
        ",equipment:{body:{id:\"minecraft:wolf_armor\",count:1,components:{\"minecraft:damage\":40}}}",
    );
    interact(&mut s, wolf);
    assert_eq!(armor_damage(&s, wolf), 40);
    assert_eq!(held_count(&mut s, "minecraft:armadillo_scute"), 4);
}

#[test]
fn shears_take_the_armor_off_for_the_owner() {
    let mut s = waiting_scene(vec![]);
    console(&mut s, "give Alice minecraft:shears 1");
    let wolf = tame_wolf(&mut s, WORN_ARMOR);
    interact(&mut s, wolf);
    assert_eq!(body_id(&s, wolf), None);
    assert!(
        s.server
            .ground_items()
            .iter()
            .any(|(_, stack)| stack.item == rspigot_items::ItemKind::WolfArmor.id())
    );
}

#[test]
fn worn_armor_absorbs_every_hit_and_wears_down_instead_of_the_wolf() {
    let mut s = waiting_scene(vec![]);
    let wolf = tame_wolf(&mut s, &format!(",Sitting:1b{WORN_ARMOR}"));
    s.players[0].link.take();
    attack(&mut s, wolf);
    assert_eq!(nbt(&s, wolf).get_f64("Health"), Some(40.0));
    assert!(armor_damage(&s, wolf) >= 1);
    assert_eq!(nbt(&s, wolf).get_bool("Sitting"), Some(true));
    let events = s.players[0].link.take();
    assert!(
        !sounds(&events)
            .iter()
            .any(|sound| sound.contains("wolf.hurt")),
        "{:?}",
        sounds(&events)
    );
}

#[test]
fn a_nearly_broken_armor_cracks_with_a_sound_then_breaks() {
    let mut s = waiting_scene(vec![]);
    let wolf = tame_wolf(
        &mut s,
        ",equipment:{body:{id:\"minecraft:wolf_armor\",count:1,components:{\"minecraft:damage\":63}}}",
    );
    s.players[0].link.take();
    attack(&mut s, wolf);
    let events = s.players[0].link.take();
    assert!(
        sounds(&events).contains(&"minecraft:item.wolf_armor.crack"),
        "{:?}",
        sounds(&events)
    );
    assert!(armor_damage(&s, wolf) >= 63 || body_id(&s, wolf).is_none());
}

#[test]
fn an_unarmored_wolf_stands_up_when_hurt_and_uses_its_sound_variant() {
    let mut s = waiting_scene(vec![]);
    let wolf = tame_wolf(&mut s, ",Sitting:1b,sound_variant:\"minecraft:grumpy\"");
    s.players[0].link.take();
    attack(&mut s, wolf);
    assert_eq!(nbt(&s, wolf).get_bool("Sitting"), Some(false));
    let events = s.players[0].link.take();
    assert!(
        sounds(&events).contains(&"minecraft:entity.wolf_grumpy.hurt"),
        "{:?}",
        sounds(&events)
    );
}

#[test]
fn two_tamed_wolves_of_one_owner_breed_a_tamed_pup_with_forty_health() {
    let mut s = waiting_scene(vec![]);
    let owner = owner_of_alice(&s);
    for x in ["1.5", "2.5"] {
        let line = format!(
            "summon minecraft:wolf {x} 64 1.5 {{Owner:{owner},InLove:600,CollarColor:3b,attributes:[{{id:\"minecraft:max_health\",base:40d}}],Health:40f}}"
        );
        summon(&mut s, &line);
    }
    s.tick(200);
    let wolves = s.server.entities_of(Kind::Wolf);
    assert_eq!(wolves.len(), 3);
    let pup = wolves
        .iter()
        .map(|wolf| (wolf.id, nbt(&s, wolf.id)))
        .find(|(_, saved)| saved.get_i32("Age").is_some_and(|age| age < 0))
        .expect("a pup was born");
    assert!(pup.1.contains_key("Owner"));
    assert_eq!(pup.1.get_f64("Health"), Some(40.0));
    assert_eq!(pup.1.get_i64("CollarColor"), Some(3));
    assert_eq!(pup.1.get_bool("Sitting"), Some(false));
}

#[test]
fn a_wild_wolf_is_never_a_mate() {
    let mut s = waiting_scene(vec![]);
    for x in ["1.5", "2.5"] {
        wild_wolf_at(&mut s, x);
    }
    s.tick(200);
    assert_eq!(s.server.entities_of(Kind::Wolf).len(), 2);
}

fn wild_wolf_at(s: &mut Scene, x: &str) -> EntityId {
    summon(
        s,
        &format!("summon minecraft:wolf {x} 64 1.5 {{InLove:600}}"),
    )
}

#[test]
fn an_angry_wolf_goes_for_the_player_it_is_angry_at_until_they_turn_creative() {
    let mut s = waiting_scene(vec![]);
    let owner = owner_of_alice(&s);
    let wolf = summon(
        &mut s,
        &format!("summon minecraft:wolf 3.5 64 3.5 {{anger_end_time:99999999L,angry_at:{owner}}}"),
    );
    s.tick(60);
    let alice = s.server.player_entity_id(s.players[0].id).unwrap();
    assert_eq!(s.server.mob_target(wolf), Some(alice));
    console(&mut s, "gamemode creative Alice");
    s.tick(5);
    assert_eq!(s.server.mob_target(wolf), None);
    assert_eq!(nbt(&s, wolf).get_i64("anger_end_time"), Some(-1));
    assert!(!nbt(&s, wolf).contains_key("angry_at"));
}
