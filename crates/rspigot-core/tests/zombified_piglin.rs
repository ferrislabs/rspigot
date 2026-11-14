mod common;

use common::*;
use rspigot_core::commands::Target;
use rspigot_core::config::PluginSettings;
use rspigot_core::events::{ClientEvent, Inbound};
use rspigot_core::plugins::events::PigZombieAngerPatch;
use rspigot_core::plugins::{EventKind, EventPatch, EventPayload};
use rspigot_domain::player::PlayerId;
use rspigot_domain::{BlockPos, EntityId};
use rspigot_entities::EntityKind as Kind;

const ANGRY_SOUND: &str = "minecraft:entity.zombified_piglin.angry";

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

fn uuid_ints(uuid: uuid::Uuid) -> String {
    let (high, low) = uuid.as_u64_pair();
    format!(
        "[I;{},{},{},{}]",
        (high >> 32) as i32,
        high as i32,
        (low >> 32) as i32,
        low as i32
    )
}

fn angry_at_alice() -> String {
    format!(
        "{{anger_end_time:9999999L,angry_at:{}}}",
        uuid_ints(PlayerId::offline("Alice").0)
    )
}

fn alice_entity(s: &Scene) -> EntityId {
    s.server.player_entity_id(s.players[0].id).unwrap()
}

fn ready(mut s: Scene) -> Scene {
    stream_chunk_of(&mut s, BlockPos::new(1, 64, 1));
    s
}

fn waiting_scene(handlers: Vec<rspigot_core::plugins::HandlerSpec>) -> Scene {
    let server = server(PluginSettings {
        decision_ticks: 1_000,
        ..PluginSettings::default()
    });
    ready(scene_on(server, &["Alice"], handlers))
}

#[test]
fn a_calm_piglin_leaves_a_nearby_player_alone() {
    let mut s = ready(scene(&["Alice"]));
    let piglin = summon(&mut s, Kind::ZombifiedPiglin, "5.5 64 0.5", "{}");
    s.tick(200);
    assert_eq!(s.server.mob_target(piglin), None);
}

#[test]
fn an_angry_piglin_hunts_its_player_with_one_loud_first_cry_and_a_speed_bonus() {
    let mut s = ready(scene(&["Alice"]));
    s.players[0].link.take();
    let piglin = summon(
        &mut s,
        Kind::ZombifiedPiglin,
        "7.5 64 0.5",
        &angry_at_alice(),
    );
    let mut events = Vec::new();
    for _ in 0..40 {
        s.tick(1);
        events.extend(s.players[0].link.take());
    }
    assert_eq!(s.server.mob_target(piglin), Some(alice_entity(&s)));
    let loud: Vec<f32> = events
        .iter()
        .filter_map(|event| match event {
            ClientEvent::Sound { sound, volume, .. } if *sound == ANGRY_SOUND && *volume > 1.5 => {
                Some(*volume)
            }
            _ => None,
        })
        .collect();
    assert_eq!(loud, vec![2.0]);
    let bonus = events.iter().any(|event| match event {
        ClientEvent::Attributes { entity, attributes } if *entity == piglin => attributes
            .iter()
            .flat_map(|attribute| &attribute.modifiers)
            .any(|modifier| modifier.id == "minecraft:attacking" && modifier.amount == 0.05),
        _ => false,
    });
    assert!(bonus, "no attacking modifier was sent");
    let saved = s.server.saved_entity(piglin).expect("saved");
    let modifiers = format!("{:?}", saved.get("attributes"));
    assert!(!modifiers.contains("attacking"), "{modifiers}");
    assert!(saved.get_i64("anger_end_time").unwrap() > 0);
}

#[test]
fn a_piglin_hurt_by_a_mob_fights_back_and_calls_its_neighbours() {
    let mut s = ready(scene(&["Alice"]));
    let one = summon(&mut s, Kind::ZombifiedPiglin, "5.5 64 5.5", "{}");
    let two = summon(&mut s, Kind::ZombifiedPiglin, "7.5 64 5.5", "{}");
    let zombie = summon(&mut s, Kind::Zombie, "5.5 64 7.5", "{NoAI:1b}");
    s.tick(2);
    assert!(s.server.do_hurt_target(zombie, Target::Entity(one)));
    s.tick(6);
    assert_eq!(s.server.mob_target(one), Some(zombie));
    assert_eq!(s.server.mob_target(two), Some(zombie));
}

#[test]
fn a_plugin_can_lengthen_the_anger_of_a_piglin() {
    let mut s = waiting_scene(vec![deciding(1, EventKind::PigZombieAnger)]);
    let piglin = summon(
        &mut s,
        Kind::ZombifiedPiglin,
        "7.5 64 0.5",
        &angry_at_alice(),
    );
    s.tick(3);
    let asked = asked_for(&s.plugin, EventKind::PigZombieAnger).remove(0);
    let EventPayload::PigZombieAnger(event) = &asked.payload else {
        panic!("a pig zombie anger");
    };
    assert_eq!(event.entity.id, piglin);
    assert!(
        (400..=780).contains(&event.new_anger),
        "{}",
        event.new_anger
    );
    assert_eq!(
        event.target.as_ref().map(|target| target.uuid),
        Some(PlayerId::offline("Alice").0)
    );
    rule_on(
        &mut s.server,
        1,
        asked.id,
        false,
        Some(EventPatch::PigZombieAnger(PigZombieAngerPatch {
            new_anger: Some(5000),
        })),
    );
    let end = s
        .server
        .saved_entity(piglin)
        .and_then(|nbt| nbt.get_i64("anger_end_time"))
        .unwrap();
    assert!((5000..5100).contains(&end), "{end}");
}

#[test]
fn a_plugin_that_cancels_the_anger_makes_the_piglin_forget_who_it_hated() {
    let mut s = waiting_scene(vec![deciding(1, EventKind::PigZombieAnger)]);
    let piglin = summon(
        &mut s,
        Kind::ZombifiedPiglin,
        "7.5 64 0.5",
        &angry_at_alice(),
    );
    s.tick(3);
    let asked = asked_for(&s.plugin, EventKind::PigZombieAnger).remove(0);
    rule_on(&mut s.server, 1, asked.id, true, None);
    let saved = s.server.saved_entity(piglin).unwrap();
    assert!(saved.get("angry_at").is_none());
}

#[test]
fn a_summoned_piglin_holds_a_golden_weapon_and_wears_no_armor() {
    let mut s = ready(scene(&["Alice"]));
    let piglin = summon(&mut s, Kind::ZombifiedPiglin, "1.5 64 1.5", "");
    let saved = s.server.saved_entity(piglin).expect("saved");
    let equipment = saved.get_compound("equipment").expect("equipment");
    let held = format!("{:?}", equipment.get("mainhand"));
    assert!(
        held.contains("minecraft:golden_sword") || held.contains("minecraft:golden_spear"),
        "{held}"
    );
    for slot in ["head", "chest", "legs", "feet"] {
        assert!(equipment.get(slot).is_none(), "{slot}");
    }
}
