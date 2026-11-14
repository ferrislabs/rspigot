mod common;

use common::*;
use rspigot_core::events::Inbound;
use rspigot_core::plugins::events::EquipmentSlot;
use rspigot_core::plugins::{
    Answer, EntityOrder, EventKind, EventPayload, ItemSpec, Order, PlayerOrder, Question,
};
use rspigot_domain::EntityId;
use rspigot_entities::EntityKind as Kind;

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn hit(s: &mut Scene, amount: f64) {
    let alice = s.players[0].clone();
    s.act(to(&alice, PlayerOrder::Damage(amount)));
}

fn health(s: &Scene) -> f32 {
    s.server
        .player_health(s.players[0].id)
        .expect("a health value")
}

fn held(s: &mut Scene) -> Option<ItemSpec> {
    let alice = s.players[0].clone();
    match s.ask(Question::Inventory(alice.id)) {
        Answer::Inventory(inventory) => inventory.slots[0].clone(),
        other => panic!("expected the inventory, got {other:?}"),
    }
}

fn zombie_with_totem(s: &mut Scene) -> EntityId {
    console(
        s,
        "summon zombie 1.5 64 0.5 {NoAI:1b,equipment:{mainhand:{id:\"minecraft:totem_of_undying\",count:1}}}",
    );
    s.server.entities_of(Kind::Zombie)[0].id
}

#[test]
fn a_lethal_hit_on_a_player_holding_a_totem_fires_the_event_and_resurrects() {
    let mut s = scene_with(&["Alice"], vec![observe(1, EventKind::EntityResurrect)]);
    console(&mut s, "give Alice totem_of_undying 1");
    hit(&mut s, 100.0);
    assert_eq!(health(&s), 1.0);
    assert_eq!(held(&mut s), None);
    let events = settled_for(&s.plugin, EventKind::EntityResurrect);
    let EventPayload::EntityResurrect(event) = &events[0].payload else {
        panic!("expected a resurrect event");
    };
    assert_eq!(event.hand, Some(EquipmentSlot::Hand));
    assert_eq!(event.entity_type, "minecraft:player");
    assert!(!events[0].cancelled);
}

#[test]
fn a_cancelled_resurrect_event_lets_the_player_die_and_keeps_the_totem_out_of_use() {
    let mut s = scene_with(&["Alice"], vec![deciding(1, EventKind::EntityResurrect)]);
    console(&mut s, "give Alice totem_of_undying 1");
    hit(&mut s, 100.0);
    let asked = last_asked(&s.plugin, EventKind::EntityResurrect);
    rule_on(&mut s.server, 1, asked.id, true, None);
    assert_eq!(health(&s), 0.0);
}

#[test]
fn an_allowed_resurrect_event_keeps_the_player_alive() {
    let mut s = scene_with(&["Alice"], vec![deciding(1, EventKind::EntityResurrect)]);
    console(&mut s, "give Alice totem_of_undying 1");
    hit(&mut s, 100.0);
    let asked = last_asked(&s.plugin, EventKind::EntityResurrect);
    rule_on(&mut s.server, 1, asked.id, false, None);
    assert_eq!(health(&s), 1.0);
    assert_eq!(held(&mut s), None);
}

#[test]
fn a_player_without_a_totem_dies_and_the_event_is_seen_cancelled_without_a_hand() {
    let mut s = scene_with(&["Alice"], vec![observe(1, EventKind::EntityResurrect)]);
    hit(&mut s, 100.0);
    assert_eq!(health(&s), 0.0);
    let events = settled_for(&s.plugin, EventKind::EntityResurrect);
    let EventPayload::EntityResurrect(event) = &events[0].payload else {
        panic!("expected a resurrect event");
    };
    assert_eq!(event.hand, None);
    assert!(events[0].cancelled);
}

#[test]
fn a_mob_holding_a_totem_survives_a_lethal_hit() {
    let mut s = scene_with(&["Alice"], vec![observe(1, EventKind::EntityResurrect)]);
    let zombie = zombie_with_totem(&mut s);
    s.act(Order::Entity {
        entity: zombie,
        order: EntityOrder::Damage(100.0),
    });
    assert_eq!(s.server.entities_of(Kind::Zombie)[0].health, Some(1.0));
    assert_eq!(settled_for(&s.plugin, EventKind::EntityResurrect).len(), 1);
}

#[test]
fn a_cancelled_event_lets_the_mob_holding_a_totem_die() {
    let mut s = scene_with(&["Alice"], vec![deciding(1, EventKind::EntityResurrect)]);
    let zombie = zombie_with_totem(&mut s);
    s.act(Order::Entity {
        entity: zombie,
        order: EntityOrder::Damage(100.0),
    });
    let asked = last_asked(&s.plugin, EventKind::EntityResurrect);
    rule_on(&mut s.server, 1, asked.id, true, None);
    assert!(s.server.mob_damage(zombie).is_some_and(|state| state.dead));
}
