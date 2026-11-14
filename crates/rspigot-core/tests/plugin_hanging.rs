mod common;

use common::*;
use rspigot_core::events::{Direction, Hand, Inbound, PlayerInput};
use rspigot_core::plugins::events::{Face, HangingBreakRemoveCause};
use rspigot_core::plugins::{
    Answer, BlockOrder, EventKind, EventPayload, ItemSpec, Order, Question,
};
use rspigot_domain::{BlockPos, EntityId, Vec3};
use rspigot_entities::EntityKind as Kind;

const WALL: BlockPos = BlockPos::new(2, 64, 0);
const FLOOR: BlockPos = BlockPos::new(2, 63, 0);

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn set(s: &mut Scene, pos: BlockPos, kind: &str) {
    s.act(Order::Block {
        pos,
        order: BlockOrder::SetWith {
            kind: kind.to_owned(),
            properties: Vec::new(),
        },
    });
}

fn hanging_scene(handlers: Vec<rspigot_core::plugins::HandlerSpec>, item: &str) -> (Scene, Player) {
    let mut s = scene_with(&["Alice"], handlers);
    let alice = s.players[0].clone();
    console(&mut s, "gamemode survival Alice");
    console(&mut s, &format!("give Alice {item}"));
    stream_chunk_of(&mut s, FLOOR);
    set(&mut s, WALL, "minecraft:stone");
    alice.link.take();
    (s, alice)
}

fn place_on_wall(s: &mut Scene, alice: &Player) {
    send_input(
        &mut s.server,
        alice,
        PlayerInput::UseItemOn {
            hand: Hand::Main,
            pos: WALL,
            face: Direction::North,
            cursor: Vec3::new(0.5, 0.5, 0.0),
            inside_block: false,
            sequence: 1,
        },
    );
    s.tick(1);
}

fn held(s: &mut Scene, alice: &Player) -> Option<ItemSpec> {
    match s.ask(Question::Inventory(alice.id)) {
        Answer::Inventory(inventory) => inventory.slots[0].clone(),
        other => panic!("expected the inventory, got {other:?}"),
    }
}

fn frame_id(s: &Scene) -> EntityId {
    s.server.entities_of(Kind::ItemFrame)[0].id
}

#[test]
fn placing_an_item_frame_fires_hanging_place_with_its_fields_and_spends_the_item() {
    let (mut s, alice) = hanging_scene(vec![observe(1, EventKind::HangingPlace)], "item_frame 2");
    place_on_wall(&mut s, &alice);
    let events = settled_for(&s.plugin, EventKind::HangingPlace);
    assert_eq!(events.len(), 1);
    let EventPayload::HangingPlace(placed) = &events[0].payload else {
        panic!("expected a hanging place");
    };
    assert_eq!(placed.block, WALL);
    assert_eq!(placed.block_face, Face::North);
    assert_eq!(placed.entity.kind, "minecraft:item_frame");
    assert_eq!(
        placed.player.as_ref().map(|p| p.name.as_str()),
        Some("Alice")
    );
    assert_eq!(
        placed.item_stack.as_ref().map(|item| item.key.as_str()),
        Some("minecraft:item_frame")
    );
    assert_eq!(s.server.entities_of(Kind::ItemFrame).len(), 1);
    assert_eq!(
        held(&mut s, &alice),
        Some(ItemSpec::of("minecraft:item_frame", 1))
    );
}

#[test]
fn a_cancelled_hanging_place_keeps_the_item_and_spawns_nothing() {
    let (mut s, alice) = hanging_scene(vec![deciding(1, EventKind::HangingPlace)], "item_frame 2");
    place_on_wall(&mut s, &alice);
    let asked = last_asked(&s.plugin, EventKind::HangingPlace);
    rule_on(&mut s.server, 1, asked.id, true, None);
    s.tick(1);
    assert!(s.server.entities_of(Kind::ItemFrame).is_empty());
    assert_eq!(
        held(&mut s, &alice),
        Some(ItemSpec::of("minecraft:item_frame", 2))
    );
}

#[test]
fn without_a_handler_the_frame_is_placed_at_once() {
    let (mut s, alice) = hanging_scene(Vec::new(), "glow_item_frame 1");
    place_on_wall(&mut s, &alice);
    assert_eq!(s.server.entities_of(Kind::GlowItemFrame).len(), 1);
    assert_eq!(held(&mut s, &alice), None);
}

#[test]
fn a_frame_cannot_be_placed_where_it_has_no_support() {
    let (mut s, alice) = hanging_scene(vec![observe(1, EventKind::HangingPlace)], "item_frame 1");
    set(&mut s, WALL, "minecraft:air");
    place_on_wall(&mut s, &alice);
    assert!(settled_for(&s.plugin, EventKind::HangingPlace).is_empty());
    assert!(s.server.entities_of(Kind::ItemFrame).is_empty());
}

#[test]
fn placing_a_painting_picks_a_variant_that_fits_the_wall() {
    let (mut s, alice) = hanging_scene(vec![observe(1, EventKind::HangingPlace)], "painting 1");
    place_on_wall(&mut s, &alice);
    let paintings = s.server.entities_of(Kind::Painting);
    assert_eq!(paintings.len(), 1);
    let events = settled_for(&s.plugin, EventKind::HangingPlace);
    let EventPayload::HangingPlace(placed) = &events[0].payload else {
        panic!("expected a hanging place");
    };
    assert_eq!(placed.entity.kind, "minecraft:painting");
    assert_eq!(held(&mut s, &alice), None);
}

#[test]
fn a_painting_is_refused_on_the_floor() {
    let (mut s, alice) = hanging_scene(vec![observe(1, EventKind::HangingPlace)], "painting 1");
    send_input(
        &mut s.server,
        &alice,
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
    assert!(s.server.entities_of(Kind::Painting).is_empty());
}

fn placed_frame(handlers: Vec<rspigot_core::plugins::HandlerSpec>) -> (Scene, Player, EntityId) {
    let mut handlers = handlers;
    handlers.push(observe(9, EventKind::EntityDamage));
    let (mut s, alice) = hanging_scene(handlers, "item_frame 1");
    place_on_wall(&mut s, &alice);
    let id = frame_id(&s);
    (s, alice, id)
}

#[test]
fn losing_the_support_breaks_the_frame_with_a_physics_cause() {
    let (mut s, _alice, id) = placed_frame(vec![observe(1, EventKind::HangingBreak)]);
    set(&mut s, WALL, "minecraft:air");
    s.tick(101);
    let events = settled_for(&s.plugin, EventKind::HangingBreak);
    assert_eq!(events.len(), 1);
    let EventPayload::HangingBreak(broken) = &events[0].payload else {
        panic!("expected a hanging break");
    };
    assert_eq!(broken.cause, HangingBreakRemoveCause::Physics);
    assert_eq!(broken.entity.id, id);
    assert!(s.server.entities_of(Kind::ItemFrame).is_empty());
    assert!(
        s.server
            .entities_of(Kind::Item)
            .iter()
            .any(|item| item.position.z < 0.5)
    );
}

#[test]
fn a_cancelled_physics_break_keeps_the_frame() {
    let (mut s, _alice, _id) = placed_frame(vec![deciding(1, EventKind::HangingBreak)]);
    set(&mut s, WALL, "minecraft:air");
    s.tick(101);
    let asked = last_asked(&s.plugin, EventKind::HangingBreak);
    rule_on(&mut s.server, 1, asked.id, true, None);
    assert_eq!(s.server.entities_of(Kind::ItemFrame).len(), 1);
}

#[test]
fn a_player_hit_on_an_empty_frame_fires_break_by_entity_with_the_player_as_remover() {
    let (mut s, alice, id) = placed_frame(vec![observe(1, EventKind::HangingBreakByEntity)]);
    send_input(&mut s.server, &alice, PlayerInput::Attack(id));
    s.tick(2);
    let events = settled_for(&s.plugin, EventKind::HangingBreakByEntity);
    assert_eq!(events.len(), 1);
    let EventPayload::HangingBreakByEntity(broken) = &events[0].payload else {
        panic!("expected a break by entity");
    };
    assert_eq!(broken.cause, HangingBreakRemoveCause::Entity);
    assert_eq!(
        broken.remover.as_ref().map(|r| r.kind.as_str()),
        Some("minecraft:player")
    );
    assert!(s.server.entities_of(Kind::ItemFrame).is_empty());
}

#[test]
fn a_cancelled_break_by_entity_keeps_the_frame() {
    let (mut s, alice, id) = placed_frame(vec![deciding(1, EventKind::HangingBreakByEntity)]);
    send_input(&mut s.server, &alice, PlayerInput::Attack(id));
    s.tick(1);
    let asked = last_asked(&s.plugin, EventKind::HangingBreakByEntity);
    rule_on(&mut s.server, 1, asked.id, true, None);
    assert_eq!(s.server.entities_of(Kind::ItemFrame).len(), 1);
}

#[test]
fn a_hit_on_a_frame_holding_an_item_drops_the_item_and_leaves_the_frame() {
    let (mut s, alice, id) = placed_frame(vec![observe(1, EventKind::HangingBreak)]);
    console(&mut s, "give Alice stone 1");
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Interact {
            target: id,
            hand: Hand::Main,
            sneaking: false,
            location: Vec3::new(0.0, 0.0, 0.0),
        },
    );
    s.tick(1);
    send_input(&mut s.server, &alice, PlayerInput::Attack(id));
    s.tick(2);
    assert!(settled_for(&s.plugin, EventKind::HangingBreak).is_empty());
    assert_eq!(s.server.entities_of(Kind::ItemFrame).len(), 1);
    assert_eq!(s.server.entities_of(Kind::Item).len(), 1);
}

#[test]
fn clicking_a_frame_with_an_item_puts_it_in_then_clicks_rotate_it() {
    let (mut s, alice, id) = placed_frame(Vec::new());
    console(&mut s, "give Alice stone 1");
    let click = |s: &mut Scene| {
        send_input(
            &mut s.server,
            &alice,
            PlayerInput::Interact {
                target: id,
                hand: Hand::Main,
                sneaking: false,
                location: Vec3::new(0.0, 0.0, 0.0),
            },
        );
        s.tick(1);
    };
    click(&mut s);
    assert_eq!(held(&mut s, &alice), None);
    click(&mut s);
    click(&mut s);
    let nbt = s.server.saved_entity(id).expect("the frame can be saved");
    assert_eq!(nbt.get_i64("ItemRotation"), Some(2));
    assert!(nbt.get_compound("Item").is_some());
}

#[test]
fn a_placed_frame_is_announced_with_its_orientation_and_block_position() {
    let (mut s, alice) = hanging_scene(Vec::new(), "item_frame 1");
    place_on_wall(&mut s, &alice);
    s.tick(2);
    let spawned = alice.link.take().into_iter().find_map(|event| match event {
        rspigot_core::events::ClientEvent::SpawnEntity { data, position, .. } => {
            Some((data, position))
        }
        _ => None,
    });
    let (data, position) = spawned.expect("the frame is sent to the player");
    assert_eq!(data, 2);
    assert_eq!(position, Vec3::new(2.0, 64.0, -1.0));
}
