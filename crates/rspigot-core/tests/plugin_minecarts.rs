mod common;

use common::*;
use rspigot_core::events::{ClientEvent, Direction, Hand, Inbound, PlayerInput};
use rspigot_core::plugins::events::{EquipmentSlot, Face, VehicleDamagePatch};
use rspigot_core::plugins::{
    Answer, BlockOrder, EventKind, EventPatch, EventPayload, ItemSpec, Order, Question,
};
use rspigot_domain::{BlockPos, EntityId, Vec3};
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

fn lay(s: &mut Scene, from: i32, to: i32, z: i32, shape: &str) {
    for x in from..=to {
        s.act(Order::Block {
            pos: BlockPos::new(x, 64, z),
            order: BlockOrder::SetWith {
                kind: "minecraft:rail".into(),
                properties: vec![("shape".into(), shape.into())],
            },
        });
    }
}

fn use_on(s: &mut Scene, alice: &Player, pos: BlockPos) {
    send_input(
        &mut s.server,
        alice,
        PlayerInput::UseItemOn {
            hand: Hand::Main,
            pos,
            face: Direction::Up,
            cursor: Vec3::new(0.5, 1.0, 0.5),
            inside_block: false,
            sequence: 1,
        },
    );
    s.tick(1);
}

fn cart(s: &mut Scene, kind: &str, x: f64, z: f64, nbt: &str) -> EntityId {
    console(s, &format!("summon {kind} {x} 64.0625 {z} {nbt}"));
    let kind = Kind::from_name(&format!("minecraft:{kind}")).unwrap();
    s.server.entities_of(kind).pop().unwrap().id
}

fn position_of(s: &Scene, kind: Kind) -> Vec3 {
    s.server.entities_of(kind)[0].position
}

#[test]
fn a_minecart_item_on_a_rail_places_a_cart_and_announces_it() {
    let (mut s, alice) = survival(
        vec![
            observe(1, EventKind::EntityPlace),
            observe(1, EventKind::VehicleCreate),
        ],
        "minecart 1",
    );
    lay(&mut s, 2, 2, 0, "east_west");
    use_on(&mut s, &alice, BlockPos::new(2, 64, 0));
    let placed = settled_for(&s.plugin, EventKind::EntityPlace);
    let EventPayload::EntityPlace(placed) = &placed[0].payload else {
        panic!("expected a placement");
    };
    assert_eq!(placed.block, BlockPos::new(2, 64, 0));
    assert_eq!(placed.block_face, Face::Up);
    assert_eq!(placed.entity_type, "minecraft:minecart");
    assert_eq!(placed.hand, EquipmentSlot::Hand);
    assert_eq!(settled_for(&s.plugin, EventKind::VehicleCreate).len(), 1);
    assert_eq!(
        position_of(&s, Kind::Minecart),
        Vec3::new(2.5, 64.0625, 0.5)
    );
    assert_eq!(held(&mut s, &alice), None);
}

#[test]
fn a_minecart_item_on_plain_ground_places_nothing() {
    let (mut s, alice) = survival(Vec::new(), "minecart 1");
    use_on(&mut s, &alice, FLOOR);
    assert!(s.server.entities_of(Kind::Minecart).is_empty());
    assert_eq!(
        held(&mut s, &alice),
        Some(ItemSpec::of("minecraft:minecart", 1))
    );
}

#[test]
fn a_cancelled_placement_keeps_the_minecart_item() {
    let (mut s, alice) = survival(
        vec![deciding(1, EventKind::VehicleCreate)],
        "chest_minecart 1",
    );
    lay(&mut s, 2, 2, 0, "east_west");
    use_on(&mut s, &alice, BlockPos::new(2, 64, 0));
    let asked = last_asked(&s.plugin, EventKind::VehicleCreate);
    rule_on(&mut s.server, 1, asked.id, true, None);
    s.tick(1);
    assert!(s.server.entities_of(Kind::ChestMinecart).is_empty());
    assert_eq!(
        held(&mut s, &alice),
        Some(ItemSpec::of("minecraft:chest_minecart", 1))
    );
}

#[test]
fn a_pushed_cart_rolls_along_the_track_and_reports_its_moves() {
    let (mut s, _alice) = survival(
        vec![
            observe(1, EventKind::VehicleUpdate),
            observe(1, EventKind::VehicleMove),
        ],
        "",
    );
    lay(&mut s, 0, 20, 3, "east_west");
    let id = cart(&mut s, "minecart", 2.5, 3.5, "{Motion:[0.3d,0.0d,0.0d]}");
    s.tick(10);
    let at = position_of(&s, Kind::Minecart);
    assert!(at.x > 4.5, "{at:?}");
    assert!((at.y - 64.0625).abs() < 1.0e-6);
    assert!((at.z - 3.5).abs() < 1.0e-9);
    let moves = settled_for(&s.plugin, EventKind::VehicleMove);
    assert!(moves.iter().any(|event| {
        matches!(&event.payload, EventPayload::VehicleMove(moved) if moved.vehicle.id == id && moved.to.position.x > moved.from.position.x)
    }));
    assert!(settled_for(&s.plugin, EventKind::VehicleUpdate).len() >= 10);
}

#[test]
fn a_cart_stopped_by_a_block_reports_the_block() {
    let (mut s, _alice) = survival(vec![observe(1, EventKind::VehicleBlockCollision)], "");
    lay(&mut s, 0, 5, 3, "east_west");
    s.act(Order::Block {
        pos: BlockPos::new(6, 64, 3),
        order: BlockOrder::Set("minecraft:stone".into()),
    });
    cart(&mut s, "minecart", 2.5, 3.5, "{Motion:[0.4d,0.0d,0.0d]}");
    s.tick(10);
    let events = settled_for(&s.plugin, EventKind::VehicleBlockCollision);
    assert!(!events.is_empty());
    let EventPayload::VehicleBlockCollision(collision) = &events[0].payload else {
        panic!("expected a collision");
    };
    assert_eq!(collision.block, BlockPos::new(6, 64, 3));
}

#[test]
fn a_cart_on_a_curve_follows_it() {
    let (mut s, _alice) = survival(Vec::new(), "");
    lay(&mut s, 2, 12, 5, "east_west");
    s.act(Order::Block {
        pos: BlockPos::new(1, 64, 5),
        order: BlockOrder::SetWith {
            kind: "minecraft:rail".into(),
            properties: vec![("shape".into(), "south_east".into())],
        },
    });
    for z in 6..14 {
        s.act(Order::Block {
            pos: BlockPos::new(1, 64, z),
            order: BlockOrder::SetWith {
                kind: "minecraft:rail".into(),
                properties: vec![("shape".into(), "north_south".into())],
            },
        });
    }
    cart(&mut s, "minecart", 6.5, 5.5, "{Motion:[-0.35d,0.0d,0.0d]}");
    s.tick(60);
    let at = position_of(&s, Kind::Minecart);
    assert!(at.z > 7.0, "{at:?}");
    assert!((at.x - 1.5).abs() < 0.2, "{at:?}");
}

#[test]
fn clicking_a_minecart_seats_the_player_after_vehicle_enter() {
    let (mut s, alice) = survival(vec![observe(1, EventKind::VehicleEnter)], "");
    lay(&mut s, 2, 2, -3, "east_west");
    let id = cart(&mut s, "minecart", 2.5, -2.5, "{}");
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
fn a_chest_minecart_opens_a_three_row_window_with_its_items() {
    let (mut s, alice) = survival(Vec::new(), "");
    let id = cart(
        &mut s,
        "chest_minecart",
        2.5,
        -2.5,
        "{Items:[{Slot:0b,id:\"minecraft:stick\",count:3}]}",
    );
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
    let events = alice.link.take();
    assert!(
        events
            .iter()
            .any(|event| matches!(event, ClientEvent::OpenScreen { menu, .. } if *menu == 2))
    );
    let slots = events
        .iter()
        .find_map(|event| match event {
            ClientEvent::ContainerContent { slots, .. } => Some(slots.clone()),
            _ => None,
        })
        .expect("the content");
    assert_eq!(slots.iter().flatten().count(), 1);
}

#[test]
fn hitting_a_cart_asks_vehicle_damage_and_a_heavy_blow_destroys_it() {
    let (mut s, alice) = survival(
        vec![
            deciding(1, EventKind::VehicleDamage),
            observe(1, EventKind::VehicleDestroy),
        ],
        "",
    );
    let id = cart(&mut s, "minecart", 2.5, -2.5, "{}");
    console(&mut s, "tp Alice 2.5 64 -1");
    send_input(&mut s.server, &alice, PlayerInput::Attack(id));
    s.tick(1);
    let asked = last_asked(&s.plugin, EventKind::VehicleDamage);
    let EventPayload::VehicleDamage(damage) = &asked.payload else {
        panic!("expected a damage");
    };
    assert_eq!(damage.vehicle.id, id);
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
    assert_eq!(settled_for(&s.plugin, EventKind::VehicleDestroy).len(), 1);
    assert!(s.server.entities_of(Kind::Minecart).is_empty());
    assert_eq!(s.server.entities_of(Kind::Item).len(), 1);
}

#[test]
fn a_chest_cart_spills_its_contents_when_it_breaks() {
    let (mut s, alice) = survival(vec![deciding(1, EventKind::VehicleDamage)], "");
    let id = cart(
        &mut s,
        "chest_minecart",
        2.5,
        -2.5,
        "{Items:[{Slot:0b,id:\"minecraft:stick\",count:3}]}",
    );
    console(&mut s, "tp Alice 2.5 64 -1");
    send_input(&mut s.server, &alice, PlayerInput::Attack(id));
    s.tick(1);
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
    assert_eq!(s.server.entities_of(Kind::Item).len(), 2);
}

#[test]
fn two_carts_touching_collide() {
    let (mut s, _alice) = survival(vec![observe(1, EventKind::VehicleEntityCollision)], "");
    lay(&mut s, 0, 10, 3, "east_west");
    cart(&mut s, "minecart", 4.5, 3.5, "{}");
    cart(&mut s, "chest_minecart", 5.0, 3.5, "{}");
    s.tick(2);
    let events = settled_for(&s.plugin, EventKind::VehicleEntityCollision);
    assert!(!events.is_empty());
}

#[test]
fn a_fast_cart_picks_up_the_mob_in_its_way() {
    let (mut s, _alice) = survival(vec![observe(1, EventKind::VehicleEntityCollision)], "");
    lay(&mut s, 0, 20, 3, "east_west");
    let id = cart(&mut s, "minecart", 2.5, 3.5, "{Motion:[0.3d,0.0d,0.0d]}");
    console(&mut s, "summon pig 3.4 64 3.5 {NoAI:1b}");
    s.tick(4);
    let events = settled_for(&s.plugin, EventKind::VehicleEntityCollision);
    assert!(!events.is_empty());
    let pig = s.server.entities_of(Kind::Pig)[0].id;
    let EventPayload::VehicleEntityCollision(collision) = &events[0].payload else {
        panic!("expected a collision");
    };
    assert_eq!(collision.vehicle.id, id);
    assert_eq!(collision.entity.id, pig);
}

#[test]
fn a_cart_is_saved_with_its_flip_and_cargo() {
    let (mut s, _alice) = survival(Vec::new(), "");
    let id = cart(
        &mut s,
        "chest_minecart",
        2.5,
        -2.5,
        "{FlippedRotation:1b,Items:[{Slot:2b,id:\"minecraft:apple\",count:1}]}",
    );
    let saved = s.server.saved_entity(id).unwrap();
    assert_eq!(saved.get_bool("FlippedRotation"), Some(true));
    assert_eq!(saved.get_list("Items").map(|items| items.len()), Some(1));
}
