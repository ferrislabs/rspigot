mod common;

use common::*;
use rspigot_core::events::{ClientEvent, Hand, Inbound, PlayerInput};
use rspigot_core::plugins::events::VehicleDamagePatch;
use rspigot_core::plugins::{
    Answer, BlockOrder, EventKind, EventPatch, EventPayload, ItemSpec, Order, Question,
};
use rspigot_domain::{BlockPos, EntityId, Rotation, Vec3};
use rspigot_entities::EntityKind as Kind;

const FLOOR: BlockPos = BlockPos::new(2, 63, 0);

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn survival(handlers: Vec<rspigot_core::plugins::HandlerSpec>, item: &str) -> (Scene, Player) {
    let mut s = scene_with(&["Alice"], handlers);
    let alice = s.players[0].clone();
    console(&mut s, "gamemode survival Alice");
    if !item.is_empty() {
        console(&mut s, &format!("give Alice {item}"));
    }
    stream_chunk_of(&mut s, FLOOR);
    alice.link.take();
    (s, alice)
}

fn held(s: &mut Scene, alice: &Player) -> Option<ItemSpec> {
    match s.ask(Question::Inventory(alice.id)) {
        Answer::Inventory(inventory) => inventory.slots[0].clone(),
        other => panic!("expected the inventory, got {other:?}"),
    }
}

fn launch(s: &mut Scene, alice: &Player) {
    send_input(
        &mut s.server,
        alice,
        PlayerInput::UseItem {
            hand: Hand::Main,
            sequence: 1,
            rotation: Rotation::new(0.0, 90.0),
        },
    );
    s.tick(1);
}

fn summon(s: &mut Scene, kind: &str, x: f64, nbt: &str) -> EntityId {
    console(s, &format!("summon {kind} {x} 64 -2.5 {nbt}"));
    let kind = Kind::from_name(&format!("minecraft:{kind}")).unwrap();
    s.server.entities_of(kind).pop().unwrap().id
}

fn hit(s: &mut Scene, alice: &Player, id: EntityId) {
    send_input(&mut s.server, alice, PlayerInput::Attack(id));
    s.tick(1);
}

fn gather(s: &mut Scene, alice: &Player, id: EntityId) {
    console(s, "tp Alice 2.5 64 -1");
    alice.link.take();
    hit(s, alice, id);
}

fn dropped_boats(s: &Scene) -> usize {
    s.server.entities_of(Kind::Item).len()
}

#[test]
fn a_boat_item_places_a_boat_announces_it_and_spends_the_item() {
    let (mut s, alice) = survival(
        vec![
            observe(1, EventKind::EntityPlace),
            observe(1, EventKind::VehicleCreate),
        ],
        "oak_boat 1",
    );
    launch(&mut s, &alice);
    let placed = settled_for(&s.plugin, EventKind::EntityPlace);
    assert_eq!(placed.len(), 1);
    let EventPayload::EntityPlace(placed) = &placed[0].payload else {
        panic!("expected a placement");
    };
    assert_eq!(placed.block, BlockPos::new(0, 63, 0));
    assert_eq!(placed.entity_type, "minecraft:oak_boat");
    let created = settled_for(&s.plugin, EventKind::VehicleCreate);
    assert_eq!(created.len(), 1);
    let EventPayload::VehicleCreate(created) = &created[0].payload else {
        panic!("expected a creation");
    };
    assert_eq!(created.vehicle.kind, "minecraft:oak_boat");
    let boats = s.server.entities_of(Kind::OakBoat);
    assert_eq!(boats.len(), 1);
    assert_eq!(held(&mut s, &alice), None);
}

#[test]
fn a_cancelled_vehicle_create_keeps_the_item_and_spawns_nothing() {
    let (mut s, alice) = survival(vec![deciding(1, EventKind::VehicleCreate)], "oak_boat 1");
    launch(&mut s, &alice);
    let asked = last_asked(&s.plugin, EventKind::VehicleCreate);
    rule_on(&mut s.server, 1, asked.id, true, None);
    s.tick(1);
    assert!(s.server.entities_of(Kind::OakBoat).is_empty());
    assert_eq!(
        held(&mut s, &alice),
        Some(ItemSpec::of("minecraft:oak_boat", 1))
    );
}

#[test]
fn a_cancelled_entity_place_keeps_the_item_and_spawns_nothing() {
    let (mut s, alice) = survival(vec![deciding(1, EventKind::EntityPlace)], "oak_boat 1");
    launch(&mut s, &alice);
    let asked = last_asked(&s.plugin, EventKind::EntityPlace);
    rule_on(&mut s.server, 1, asked.id, true, None);
    s.tick(1);
    assert!(s.server.entities_of(Kind::OakBoat).is_empty());
    assert_eq!(
        held(&mut s, &alice),
        Some(ItemSpec::of("minecraft:oak_boat", 1))
    );
}

#[test]
fn without_a_handler_every_boat_kind_is_placed_at_once() {
    for (item, kind) in [
        ("bamboo_raft", Kind::BambooRaft),
        ("cherry_chest_boat", Kind::CherryChestBoat),
    ] {
        let (mut s, alice) = survival(Vec::new(), &format!("{item} 1"));
        launch(&mut s, &alice);
        assert_eq!(s.server.entities_of(kind).len(), 1, "{item}");
        assert_eq!(held(&mut s, &alice), None);
    }
}

#[test]
fn hitting_a_boat_asks_vehicle_damage_with_the_attacker() {
    let (mut s, alice) = survival(vec![observe(1, EventKind::VehicleDamage)], "");
    let id = summon(&mut s, "oak_boat", 2.5, "{}");
    gather(&mut s, &alice, id);
    let events = settled_for(&s.plugin, EventKind::VehicleDamage);
    assert_eq!(events.len(), 1);
    let EventPayload::VehicleDamage(damage) = &events[0].payload else {
        panic!("expected a damage");
    };
    assert_eq!(damage.vehicle.id, id);
    assert_eq!(
        damage.attacker.as_ref().map(|a| a.kind.as_str()),
        Some("minecraft:player")
    );
    assert!(damage.damage > 0.0);
    assert_eq!(s.server.entities_of(Kind::OakBoat).len(), 1);
}

#[test]
fn a_cancelled_vehicle_damage_leaves_the_boat_whole() {
    let (mut s, alice) = survival(
        vec![
            deciding(1, EventKind::VehicleDamage),
            observe(1, EventKind::VehicleDestroy),
        ],
        "netherite_sword 1",
    );
    let id = summon(&mut s, "oak_boat", 2.5, "{}");
    s.tick(25);
    gather(&mut s, &alice, id);
    let asked = last_asked(&s.plugin, EventKind::VehicleDamage);
    rule_on(&mut s.server, 1, asked.id, true, None);
    s.tick(1);
    assert_eq!(s.server.entities_of(Kind::OakBoat).len(), 1);
    assert!(settled_for(&s.plugin, EventKind::VehicleDestroy).is_empty());
    assert_eq!(dropped_boats(&s), 0);
}

#[test]
fn a_patched_damage_over_forty_destroys_the_boat_and_drops_it() {
    let (mut s, alice) = survival(
        vec![
            deciding(1, EventKind::VehicleDamage),
            observe(1, EventKind::VehicleDestroy),
        ],
        "",
    );
    let id = summon(&mut s, "oak_boat", 2.5, "{}");
    gather(&mut s, &alice, id);
    let asked = last_asked(&s.plugin, EventKind::VehicleDamage);
    rule_on(
        &mut s.server,
        1,
        asked.id,
        false,
        Some(EventPatch::VehicleDamage(VehicleDamagePatch {
            damage: Some(50.0),
        })),
    );
    s.tick(1);
    let destroyed = settled_for(&s.plugin, EventKind::VehicleDestroy);
    assert_eq!(destroyed.len(), 1);
    let EventPayload::VehicleDestroy(destroyed) = &destroyed[0].payload else {
        panic!("expected a destruction");
    };
    assert_eq!(destroyed.vehicle.id, id);
    assert!(destroyed.attacker.is_some());
    assert!(s.server.entities_of(Kind::OakBoat).is_empty());
    assert_eq!(dropped_boats(&s), 1);
}

#[test]
fn a_cancelled_vehicle_destroy_keeps_the_boat() {
    let (mut s, alice) = survival(
        vec![
            deciding(1, EventKind::VehicleDamage),
            deciding(1, EventKind::VehicleDestroy),
        ],
        "",
    );
    let id = summon(&mut s, "oak_boat", 2.5, "{}");
    gather(&mut s, &alice, id);
    let asked = last_asked(&s.plugin, EventKind::VehicleDamage);
    rule_on(
        &mut s.server,
        1,
        asked.id,
        false,
        Some(EventPatch::VehicleDamage(VehicleDamagePatch {
            damage: Some(50.0),
        })),
    );
    s.tick(1);
    let asked = last_asked(&s.plugin, EventKind::VehicleDestroy);
    rule_on(&mut s.server, 1, asked.id, true, None);
    s.tick(1);
    assert_eq!(s.server.entities_of(Kind::OakBoat).len(), 1);
    assert_eq!(dropped_boats(&s), 0);
}

#[test]
fn without_a_handler_a_heavy_blow_breaks_the_boat_into_an_item() {
    let (mut s, alice) = survival(Vec::new(), "netherite_sword 1");
    let id = summon(&mut s, "oak_boat", 2.5, "{}");
    s.tick(25);
    gather(&mut s, &alice, id);
    assert!(s.server.entities_of(Kind::OakBoat).is_empty());
    assert_eq!(dropped_boats(&s), 1);
}

#[test]
fn a_creative_punch_discards_the_boat_without_a_drop() {
    let (mut s, alice) = survival(Vec::new(), "");
    console(&mut s, "gamemode creative Alice");
    let id = summon(&mut s, "oak_boat", 2.5, "{}");
    gather(&mut s, &alice, id);
    assert!(s.server.entities_of(Kind::OakBoat).is_empty());
    assert_eq!(dropped_boats(&s), 0);
}

#[test]
fn a_boat_in_the_world_updates_every_tick() {
    let (mut s, _alice) = survival(vec![observe(1, EventKind::VehicleUpdate)], "");
    let id = summon(&mut s, "oak_boat", 2.5, "{}");
    s.tick(5);
    let updates = settled_for(&s.plugin, EventKind::VehicleUpdate);
    let mine = updates
        .iter()
        .filter(|event| {
            matches!(&event.payload, EventPayload::VehicleUpdate(update) if update.vehicle.id == id)
        })
        .count();
    assert!(mine >= 5, "{mine} updates");
}

#[test]
fn a_boat_hitting_a_wall_reports_the_block() {
    let (mut s, _alice) = survival(vec![observe(1, EventKind::VehicleBlockCollision)], "");
    for z in [1, 2, 3] {
        s.act(Order::Block {
            pos: BlockPos::new(4, 64, z),
            order: BlockOrder::Set("minecraft:stone".into()),
        });
    }
    console(
        &mut s,
        "summon oak_boat 2.5 64 2.5 {Motion:[0.9d,0.0d,0.0d]}",
    );
    let id = s.server.entities_of(Kind::OakBoat)[0].id;
    s.tick(6);
    let events = settled_for(&s.plugin, EventKind::VehicleBlockCollision);
    assert!(!events.is_empty());
    let EventPayload::VehicleBlockCollision(collision) = &events[0].payload else {
        panic!("expected a collision");
    };
    assert_eq!(collision.vehicle.id, id);
    assert_eq!(collision.block, BlockPos::new(4, 64, 2));
}

#[test]
fn two_overlapping_boats_collide_and_drift_apart() {
    let (mut s, _alice) = survival(vec![observe(1, EventKind::VehicleEntityCollision)], "");
    summon(&mut s, "oak_boat", 2.5, "{}");
    summon(&mut s, "birch_boat", 2.9, "{}");
    s.tick(3);
    let events = settled_for(&s.plugin, EventKind::VehicleEntityCollision);
    assert!(!events.is_empty());
    let EventPayload::VehicleEntityCollision(collision) = &events[0].payload else {
        panic!("expected a collision");
    };
    assert!(!collision.collision_cancelled && !collision.pickup_cancelled);
    let oak = s.server.entities_of(Kind::OakBoat)[0].position.x;
    let birch = s.server.entities_of(Kind::BirchBoat)[0].position.x;
    assert!(birch - oak > 0.4, "{oak} {birch}");
}

#[test]
fn a_cancelled_collision_leaves_the_boats_where_they_are() {
    let (mut s, _alice) = survival(vec![deciding(1, EventKind::VehicleEntityCollision)], "");
    summon(&mut s, "oak_boat", 2.5, "{}");
    summon(&mut s, "birch_boat", 2.9, "{}");
    for _ in 0..3 {
        s.tick(1);
        for asked in asked_for(&s.plugin, EventKind::VehicleEntityCollision) {
            rule_on(&mut s.server, 1, asked.id, true, None);
        }
    }
    let oak = s.server.entities_of(Kind::OakBoat)[0].position.x;
    let birch = s.server.entities_of(Kind::BirchBoat)[0].position.x;
    assert!((birch - oak - 0.4).abs() < 1.0e-6, "{oak} {birch}");
}

#[test]
fn clicking_a_boat_seats_the_player_after_vehicle_enter() {
    let (mut s, alice) = survival(vec![observe(1, EventKind::VehicleEnter)], "");
    let id = summon(&mut s, "oak_boat", 2.5, "{}");
    console(&mut s, "tp Alice 2.5 64 -1");
    alice.link.take();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Interact {
            target: id,
            hand: Hand::Main,
            sneaking: false,
            location: Vec3::new(0.0, 0.5, 0.0),
        },
    );
    s.tick(1);
    assert_eq!(settled_for(&s.plugin, EventKind::VehicleEnter).len(), 1);
    assert!(alice.link.take().iter().any(|event| {
        matches!(event, ClientEvent::Passengers { vehicle, passengers } if *vehicle == id && passengers.len() == 1)
    }));
}

#[test]
fn a_boat_is_saved_with_its_chest() {
    let (mut s, _alice) = survival(Vec::new(), "");
    let id = summon(
        &mut s,
        "oak_chest_boat",
        2.5,
        "{Items:[{Slot:3b,id:\"minecraft:stick\",count:2}]}",
    );
    let saved = s.server.saved_entity(id).unwrap();
    let items = saved.get_list("Items").unwrap();
    assert_eq!(items.len(), 1);
}
