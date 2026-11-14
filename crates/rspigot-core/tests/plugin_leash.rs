mod common;

use common::*;
use rspigot_core::events::{Hand, Inbound, PlayerInput};
use rspigot_core::plugins::events::EntityUnleashReason;
use rspigot_core::plugins::{Answer, EventKind, EventPayload, ItemSpec, Question};
use rspigot_domain::{EntityId, Vec3};
use rspigot_entities::EntityKind as Kind;

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn summoned(s: &mut Scene, line: &str, kind: Kind) -> EntityId {
    let before: Vec<EntityId> = s.server.entities_of(kind).iter().map(|e| e.id).collect();
    console(s, line);
    s.server
        .entities_of(kind)
        .iter()
        .map(|e| e.id)
        .find(|id| !before.contains(id))
        .expect("a new entity")
}

fn click(s: &mut Scene, player: &Player, target: EntityId) {
    send_input(
        &mut s.server,
        player,
        PlayerInput::Interact {
            target,
            hand: Hand::Main,
            sneaking: false,
            location: Vec3::new(0.0, 0.5, 0.0),
        },
    );
}

fn leashed(s: &Scene, id: EntityId) -> bool {
    s.server
        .saved_entity(id)
        .is_some_and(|nbt| nbt.get("leash").is_some())
}

fn slot_zero(s: &mut Scene, player: &Player) -> Option<ItemSpec> {
    match s.ask(Question::Inventory(player.id)) {
        Answer::Inventory(inventory) => inventory.slots[0].clone(),
        other => panic!("expected the inventory, got {other:?}"),
    }
}

#[test]
fn a_lead_on_a_mob_ties_it_to_the_player_and_fires_the_leash_event() {
    let mut s = scene_with(&["Alice"], vec![observe(1, EventKind::PlayerLeashEntity)]);
    let alice = s.players[0].clone();
    console(&mut s, "give Alice lead 2");
    let cow = summoned(&mut s, "summon cow 1.5 64 0.5 {NoAI:1b}", Kind::Cow);
    click(&mut s, &alice, cow);
    assert!(leashed(&s, cow));
    assert_eq!(
        slot_zero(&mut s, &alice),
        Some(ItemSpec::of("minecraft:lead", 1))
    );
    let asked = last_settled(&s);
    let EventPayload::PlayerLeashEntity(event) = &asked else {
        panic!("expected a leash event");
    };
    assert_eq!(event.entity.id, cow);
    assert_eq!(event.player.name, "Alice");
    assert_eq!(event.leash_holder.kind, "minecraft:player");
}

fn last_settled(s: &Scene) -> EventPayload {
    settled_for(&s.plugin, EventKind::PlayerLeashEntity)
        .pop()
        .expect("a settled event")
        .payload
}

#[test]
fn a_cancelled_leash_event_keeps_the_lead_and_leaves_the_mob_free() {
    let mut s = scene_with(&["Alice"], vec![deciding(1, EventKind::PlayerLeashEntity)]);
    let alice = s.players[0].clone();
    console(&mut s, "give Alice lead 1");
    let cow = summoned(&mut s, "summon cow 1.5 64 0.5 {NoAI:1b}", Kind::Cow);
    click(&mut s, &alice, cow);
    let asked = last_asked(&s.plugin, EventKind::PlayerLeashEntity);
    rule_on(&mut s.server, 1, asked.id, true, None);
    assert!(!leashed(&s, cow));
    assert_eq!(
        slot_zero(&mut s, &alice),
        Some(ItemSpec::of("minecraft:lead", 1))
    );
}

#[test]
fn an_allowed_leash_event_ties_the_mob() {
    let mut s = scene_with(&["Alice"], vec![deciding(1, EventKind::PlayerLeashEntity)]);
    let alice = s.players[0].clone();
    console(&mut s, "give Alice lead 1");
    let cow = summoned(&mut s, "summon cow 1.5 64 0.5 {NoAI:1b}", Kind::Cow);
    click(&mut s, &alice, cow);
    let asked = last_asked(&s.plugin, EventKind::PlayerLeashEntity);
    rule_on(&mut s.server, 1, asked.id, false, None);
    assert!(leashed(&s, cow));
    assert_eq!(slot_zero(&mut s, &alice), None);
}

#[test]
fn a_hostile_mob_cannot_be_leashed() {
    let mut s = scene_with(&["Alice"], vec![observe(1, EventKind::PlayerLeashEntity)]);
    let alice = s.players[0].clone();
    console(&mut s, "give Alice lead 1");
    let zombie = summoned(&mut s, "summon zombie 1.5 64 0.5 {NoAI:1b}", Kind::Zombie);
    click(&mut s, &alice, zombie);
    assert!(!leashed(&s, zombie));
    assert!(settled_for(&s.plugin, EventKind::PlayerLeashEntity).is_empty());
}

fn leash_cow(s: &mut Scene, player: &Player) -> EntityId {
    console(s, "give Alice lead 1");
    let cow = summoned(s, "summon cow 1.5 64 0.5 {NoAI:1b}", Kind::Cow);
    click(s, player, cow);
    assert!(leashed(s, cow));
    cow
}

#[test]
fn clicking_a_leashed_mob_unleashes_it_fires_the_event_and_drops_the_lead() {
    let mut s = scene_with(
        &["Alice"],
        vec![
            observe(1, EventKind::PlayerUnleashEntity),
            observe(2, EventKind::EntityUnleash),
        ],
    );
    let alice = s.players[0].clone();
    let cow = leash_cow(&mut s, &alice);
    let before = s.server.items_on_ground();
    click(&mut s, &alice, cow);
    assert!(!leashed(&s, cow));
    assert_eq!(s.server.items_on_ground(), before + 1);
    let events = settled_for(&s.plugin, EventKind::PlayerUnleashEntity);
    let EventPayload::PlayerUnleashEntity(event) = &events.last().expect("an event").payload else {
        panic!("expected an unleash event");
    };
    assert_eq!(event.entity.id, cow);
    assert_eq!(event.reason, EntityUnleashReason::PlayerUnleash);
    assert_eq!(event.player.name, "Alice");
}

#[test]
fn a_cancelled_unleash_event_keeps_the_mob_tied_and_an_allowed_one_frees_it() {
    let mut s = scene_with(
        &["Alice"],
        vec![deciding(1, EventKind::PlayerUnleashEntity)],
    );
    let alice = s.players[0].clone();
    let cow = leash_cow(&mut s, &alice);
    click(&mut s, &alice, cow);
    let asked = last_asked(&s.plugin, EventKind::PlayerUnleashEntity);
    rule_on(&mut s.server, 1, asked.id, true, None);
    assert!(leashed(&s, cow));
    click(&mut s, &alice, cow);
    let asked = last_asked(&s.plugin, EventKind::PlayerUnleashEntity);
    rule_on(&mut s.server, 1, asked.id, false, None);
    assert!(!leashed(&s, cow));
}

#[test]
fn a_lead_held_by_a_player_survives_the_ticks_while_the_player_stays_close() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    let cow = leash_cow(&mut s, &alice);
    s.tick(130);
    assert!(leashed(&s, cow));
}
