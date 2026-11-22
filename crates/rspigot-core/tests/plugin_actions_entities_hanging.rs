mod common;

use common::*;
use rspigot_core::plugins::{
    Answer, BlockOrder, EntityOrder, HangingView, ItemSpec, ObjectAnswer, ObjectOrder,
    ObjectQuestion, Order, Question,
};
use rspigot_domain::{BlockPos, EntityId};

const WALL: BlockPos = BlockPos::new(5, 64, 5);
const CELL: BlockPos = BlockPos::new(5, 64, 4);

fn walled() -> Scene {
    let mut s = scene(&["Alice"]);
    s.tick(60);
    for y in 64..68 {
        for x in 4..9 {
            s.act(Order::Block {
                pos: BlockPos::new(x, y, WALL.z),
                order: BlockOrder::Set("minecraft:stone".to_owned()),
            });
        }
    }
    s
}

fn place(
    s: &mut Scene,
    kind: &str,
    pos: BlockPos,
    face: &str,
    variant: Option<&str>,
    item: Option<ItemSpec>,
) -> Answer {
    s.ask(Question::Object(ObjectQuestion::PlaceHanging {
        kind: kind.to_owned(),
        pos,
        face: face.to_owned(),
        variant: variant.map(str::to_owned),
        item,
    }))
}

fn placed(answer: Answer) -> EntityId {
    match answer {
        Answer::Spawned(view) => view.id,
        other => panic!("expected a hanging entity, got {other:?}"),
    }
}

fn view(s: &mut Scene, id: EntityId) -> HangingView {
    match s.ask(Question::Object(ObjectQuestion::Hanging(id))) {
        Answer::Object(ObjectAnswer::Hanging(view)) => *view,
        other => panic!("expected a hanging view, got {other:?}"),
    }
}

fn order(s: &mut Scene, id: EntityId, order: ObjectOrder) {
    s.act(Order::Entity {
        entity: id,
        order: EntityOrder::Object(order),
    });
}

fn exists(s: &mut Scene, id: EntityId) -> bool {
    matches!(s.ask(Question::Entity(id)), Answer::Entity(_))
}

#[test]
fn an_item_frame_is_placed_on_a_wall_with_an_item() {
    let mut s = walled();

    let id = placed(place(
        &mut s,
        "minecraft:item_frame",
        CELL,
        "north",
        None,
        Some(ItemSpec::of("minecraft:diamond", 1)),
    ));

    let frame = view(&mut s, id);
    assert_eq!(frame.kind, "minecraft:item_frame");
    assert_eq!(frame.pos, CELL);
    assert_eq!(frame.facing, "north");
    assert_eq!(
        frame.item.map(|item| item.key),
        Some("minecraft:diamond".to_owned())
    );
    assert!(frame.visible);
}

#[test]
fn a_glow_frame_item_rotation_and_visibility_are_edited() {
    let mut s = walled();
    let id = placed(place(
        &mut s,
        "minecraft:glow_item_frame",
        CELL,
        "north",
        None,
        None,
    ));
    assert!(view(&mut s, id).item.is_none());

    order(
        &mut s,
        id,
        ObjectOrder::SetFrameItem(Some(ItemSpec::of("minecraft:compass", 1))),
    );
    order(&mut s, id, ObjectOrder::SetFrameRotation(11));
    order(&mut s, id, ObjectOrder::SetFrameVisible(false));

    assert!(s.warnings().is_empty());
    let frame = view(&mut s, id);
    assert_eq!(
        frame.item.map(|item| item.key),
        Some("minecraft:compass".to_owned())
    );
    assert_eq!(frame.rotation, 3);
    assert!(!frame.visible);

    order(&mut s, id, ObjectOrder::SetFrameItem(None));
    assert!(view(&mut s, id).item.is_none());
}

#[test]
fn a_painting_takes_a_named_variant_or_the_largest_that_fits() {
    let mut s = walled();

    let named = placed(place(
        &mut s,
        "minecraft:painting",
        BlockPos::new(8, 66, 4),
        "north",
        Some("minecraft:kebab"),
        None,
    ));
    let free = placed(place(
        &mut s,
        "minecraft:painting",
        BlockPos::new(5, 66, 4),
        "north",
        None,
        None,
    ));

    assert_eq!(
        view(&mut s, named).variant.as_deref(),
        Some("minecraft:kebab")
    );
    let chosen = view(&mut s, free);
    assert_eq!(chosen.facing, "north");
    assert!(chosen.variant.is_some());
}

#[test]
fn a_hanging_entity_is_removed_with_the_entity_order() {
    let mut s = walled();
    let id = placed(place(
        &mut s,
        "minecraft:item_frame",
        CELL,
        "north",
        None,
        None,
    ));
    assert!(exists(&mut s, id));

    s.act(Order::Entity {
        entity: id,
        order: EntityOrder::Remove,
    });
    s.tick(1);

    assert!(!exists(&mut s, id));
}

#[test]
fn a_spot_without_a_wall_or_a_bad_request_is_refused() {
    let mut s = walled();

    assert!(matches!(
        place(
            &mut s,
            "minecraft:item_frame",
            BlockPos::new(5, 70, 4),
            "north",
            None,
            None
        ),
        Answer::Failed(_)
    ));
    assert!(matches!(
        place(&mut s, "minecraft:painting", CELL, "up", None, None),
        Answer::Failed(_)
    ));
    assert!(matches!(
        place(&mut s, "minecraft:cow", CELL, "north", None, None),
        Answer::Failed(_)
    ));
    assert!(matches!(
        place(
            &mut s,
            "minecraft:painting",
            CELL,
            "north",
            Some("minecraft:nothing"),
            None
        ),
        Answer::Failed(_)
    ));
    let cow = {
        let Answer::Spawned(view) = s.ask(Question::Spawn {
            kind: "minecraft:cow".into(),
            position: rspigot_domain::Vec3::new(3.5, 64.0, 2.5),
        }) else {
            panic!("expected a cow");
        };
        view.id
    };
    order(&mut s, cow, ObjectOrder::SetFrameRotation(1));
    assert_eq!(s.warnings().len(), 1);
}
