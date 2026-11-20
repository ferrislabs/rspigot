mod common;

use common::*;
use rspigot_core::events::{Direction, Hand, Inbound, PlayerInput};
use rspigot_core::plugins::events::{EquipmentSlot, Face};
use rspigot_core::plugins::{Answer, EventKind, EventPayload, ItemSpec, Question};
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

fn place(s: &mut Scene, alice: &Player) {
    send_input(
        &mut s.server,
        alice,
        PlayerInput::UseItemOn {
            hand: Hand::Main,
            pos: FLOOR,
            face: Direction::Up,
            cursor: Vec3::new(0.5, 1.0, 0.5),
            inside_block: false,
            sequence: 1,
        },
    );
    s.tick(1);
}

fn summon(s: &mut Scene, nbt: &str) -> EntityId {
    console(s, &format!("summon armor_stand 2.5 64 -2.5 {nbt}"));
    s.server.entities_of(Kind::ArmorStand)[0].id
}

fn click(s: &mut Scene, alice: &Player, id: EntityId, height: f64) {
    send_input(
        &mut s.server,
        alice,
        PlayerInput::Interact {
            target: id,
            hand: Hand::Main,
            sneaking: false,
            location: Vec3::new(0.0, height, 0.0),
        },
    );
    s.tick(1);
}

fn equipment(s: &Scene, id: EntityId, slot: &str) -> Option<String> {
    s.server
        .saved_entity(id)?
        .get_compound("equipment")?
        .get_compound(slot)?
        .get_str("id")
        .map(str::to_owned)
}

#[test]
fn placing_an_armor_stand_asks_entity_place_and_spends_the_item() {
    let (mut s, alice) = survival(vec![observe(1, EventKind::EntityPlace)], "armor_stand 2");
    place(&mut s, &alice);
    let events = settled_for(&s.plugin, EventKind::EntityPlace);
    assert_eq!(events.len(), 1);
    let EventPayload::EntityPlace(placed) = &events[0].payload else {
        panic!("expected a placement");
    };
    assert_eq!(placed.block, FLOOR);
    assert_eq!(placed.block_face, Face::Up);
    assert_eq!(placed.entity_type, "minecraft:armor_stand");
    assert_eq!(placed.hand, EquipmentSlot::Hand);
    let stands = s.server.entities_of(Kind::ArmorStand);
    assert_eq!(stands.len(), 1);
    assert_eq!(stands[0].yaw, -180.0);
    assert_eq!(
        held(&mut s, &alice),
        Some(ItemSpec::of("minecraft:armor_stand", 1))
    );
}

#[test]
fn a_cancelled_placement_keeps_the_item_and_spawns_nothing() {
    let (mut s, alice) = survival(vec![deciding(1, EventKind::EntityPlace)], "armor_stand 2");
    place(&mut s, &alice);
    let asked = last_asked(&s.plugin, EventKind::EntityPlace);
    rule_on(&mut s.server, 1, asked.id, true, None);
    s.tick(1);
    assert!(s.server.entities_of(Kind::ArmorStand).is_empty());
    assert_eq!(
        held(&mut s, &alice),
        Some(ItemSpec::of("minecraft:armor_stand", 2))
    );
}

#[test]
fn without_a_handler_the_stand_is_placed_at_once() {
    let (mut s, alice) = survival(Vec::new(), "armor_stand 1");
    place(&mut s, &alice);
    assert_eq!(s.server.entities_of(Kind::ArmorStand).len(), 1);
    assert_eq!(held(&mut s, &alice), None);
}

#[test]
fn equipping_a_piece_asks_the_manipulate_event_then_wears_it() {
    let (mut s, alice) = survival(
        vec![observe(1, EventKind::PlayerArmorStandManipulate)],
        "iron_helmet 1",
    );
    let id = summon(&mut s, "{}");
    click(&mut s, &alice, id, 1.0);
    let events = settled_for(&s.plugin, EventKind::PlayerArmorStandManipulate);
    assert_eq!(events.len(), 1);
    let EventPayload::PlayerArmorStandManipulate(manipulated) = &events[0].payload else {
        panic!("expected a manipulation");
    };
    assert_eq!(manipulated.slot, EquipmentSlot::Head);
    assert_eq!(manipulated.hand, EquipmentSlot::Hand);
    assert_eq!(manipulated.player_item.key, "minecraft:iron_helmet");
    assert_eq!(manipulated.armor_stand_item.key, "minecraft:air");
    assert_eq!(manipulated.right_clicked.id, id);
    assert_eq!(
        equipment(&s, id, "head").as_deref(),
        Some("minecraft:iron_helmet")
    );
    assert_eq!(held(&mut s, &alice), None);
}

#[test]
fn a_cancelled_manipulation_changes_nothing() {
    let (mut s, alice) = survival(
        vec![deciding(1, EventKind::PlayerArmorStandManipulate)],
        "iron_helmet 1",
    );
    let id = summon(&mut s, "{}");
    click(&mut s, &alice, id, 1.0);
    let asked = last_asked(&s.plugin, EventKind::PlayerArmorStandManipulate);
    rule_on(&mut s.server, 1, asked.id, true, None);
    assert_eq!(equipment(&s, id, "head"), None);
    assert_eq!(
        held(&mut s, &alice),
        Some(ItemSpec::of("minecraft:iron_helmet", 1))
    );
}

#[test]
fn an_empty_hand_takes_the_clicked_piece_back() {
    let (mut s, alice) = survival(vec![observe(1, EventKind::PlayerArmorStandManipulate)], "");
    let id = summon(
        &mut s,
        "{equipment:{chest:{id:\"minecraft:iron_chestplate\",count:1}}}",
    );
    click(&mut s, &alice, id, 1.2);
    let events = settled_for(&s.plugin, EventKind::PlayerArmorStandManipulate);
    let EventPayload::PlayerArmorStandManipulate(manipulated) = &events[0].payload else {
        panic!("expected a manipulation");
    };
    assert_eq!(manipulated.slot, EquipmentSlot::Chest);
    assert_eq!(
        manipulated.armor_stand_item.key,
        "minecraft:iron_chestplate"
    );
    assert_eq!(equipment(&s, id, "chest"), None);
    assert_eq!(
        held(&mut s, &alice),
        Some(ItemSpec::of("minecraft:iron_chestplate", 1))
    );
}

#[test]
fn a_disabled_slot_fires_nothing_and_keeps_the_item() {
    let (mut s, alice) = survival(
        vec![observe(1, EventKind::PlayerArmorStandManipulate)],
        "iron_helmet 1",
    );
    let id = summon(&mut s, "{DisabledSlots:16}");
    click(&mut s, &alice, id, 1.0);
    assert!(settled_for(&s.plugin, EventKind::PlayerArmorStandManipulate).is_empty());
    assert_eq!(equipment(&s, id, "head"), None);
}

#[test]
fn two_quick_hits_break_the_stand_and_drop_what_it_wore() {
    let (mut s, alice) = survival(Vec::new(), "");
    let id = summon(
        &mut s,
        "{equipment:{head:{id:\"minecraft:iron_helmet\",count:1}}}",
    );
    send_input(&mut s.server, &alice, PlayerInput::Attack(id));
    s.tick(1);
    assert_eq!(s.server.entities_of(Kind::ArmorStand).len(), 1);
    send_input(&mut s.server, &alice, PlayerInput::Attack(id));
    s.tick(1);
    assert!(s.server.entities_of(Kind::ArmorStand).is_empty());
    assert_eq!(s.server.entities_of(Kind::Item).len(), 2);
}

#[test]
fn the_flags_and_poses_survive_a_save() {
    let (mut s, _alice) = survival(Vec::new(), "");
    let id = summon(
        &mut s,
        "{Small:1b,ShowArms:1b,NoBasePlate:1b,Invisible:1b,Pose:{Head:[10f,20f,30f]}}",
    );
    let nbt = s.server.saved_entity(id).expect("a saved stand");
    assert_eq!(nbt.get_bool("Small"), Some(true));
    assert_eq!(nbt.get_bool("ShowArms"), Some(true));
    assert_eq!(nbt.get_bool("NoBasePlate"), Some(true));
    assert_eq!(nbt.get_bool("Invisible"), Some(true));
    assert!(nbt.get_compound("Pose").unwrap().contains_key("Head"));
}

#[test]
fn the_entity_drops_rule_off_makes_a_broken_stand_drop_nothing() {
    let (mut s, alice) = survival(Vec::new(), "");
    console(&mut s, "gamerule entity_drops false");
    let id = summon(
        &mut s,
        "{equipment:{head:{id:\"minecraft:iron_helmet\",count:1}}}",
    );
    send_input(&mut s.server, &alice, PlayerInput::Attack(id));
    s.tick(1);
    send_input(&mut s.server, &alice, PlayerInput::Attack(id));
    s.tick(1);
    assert!(s.server.entities_of(Kind::ArmorStand).is_empty());
    assert!(s.server.entities_of(Kind::Item).is_empty());
}
