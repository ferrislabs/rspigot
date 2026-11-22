mod common;

use common::*;
use rspigot_core::events::ClientEvent;
use rspigot_core::plugins::{
    Answer, Billboard, BlockDisplayData, BlockState, DisplayBody, DisplayContext, DisplayProps,
    DisplaySpec, EntityOrder, InteractionData, ItemDisplayData, ItemSpec, ObjectAnswer,
    ObjectOrder, ObjectQuestion, Order, Question, TextAlignment, TextData, Transformation,
};
use rspigot_domain::{EntityId, Vec3};

fn settled() -> Scene {
    let mut s = scene(&["Alice"]);
    s.tick(60);
    s
}

fn spawn_display(s: &mut Scene, spec: DisplaySpec) -> Answer {
    s.ask(Question::Object(ObjectQuestion::SpawnDisplay {
        position: Vec3::new(2.5, 66.0, 2.5),
        spec,
    }))
}

fn spawned(answer: Answer) -> (EntityId, String) {
    match answer {
        Answer::Spawned(view) => (view.id, view.kind),
        other => panic!("expected a spawned display, got {other:?}"),
    }
}

fn display(s: &mut Scene, id: EntityId) -> DisplaySpec {
    match s.ask(Question::Object(ObjectQuestion::Display(id))) {
        Answer::Object(ObjectAnswer::Display(spec)) => *spec,
        other => panic!("expected a display, got {other:?}"),
    }
}

fn edit(s: &mut Scene, id: EntityId, spec: DisplaySpec) {
    s.act(Order::Entity {
        entity: id,
        order: EntityOrder::Object(ObjectOrder::EditDisplay(spec)),
    });
}

fn text(spec: TextData, props: DisplayProps) -> DisplaySpec {
    DisplaySpec {
        props,
        body: DisplayBody::Text(spec),
    }
}

fn text_body(spec: &DisplaySpec) -> &TextData {
    match &spec.body {
        DisplayBody::Text(text) => text,
        other => panic!("expected a text display, got {other:?}"),
    }
}

#[test]
fn a_text_display_spawns_with_its_text_and_options() {
    let mut s = settled();
    let props = DisplayProps {
        billboard: Some(Billboard::Center),
        brightness: Some((4, 11)),
        view_range: Some(2.5),
        glow_color: Some(0x00ff_0000),
        transformation: Some(Transformation {
            translation: Some([0.0, 1.0, 0.0]),
            scale: Some([2.0, 2.0, 2.0]),
            left_rotation: Some([0.0, 0.6, 0.0, 0.6]),
            right_rotation: None,
        }),
        ..DisplayProps::default()
    };
    let spec = text(
        TextData {
            text: Some("Hello hologram".to_owned()),
            line_width: Some(120),
            background: Some(0x4000_0000),
            opacity: Some(200),
            shadow: Some(true),
            see_through: Some(true),
            alignment: Some(TextAlignment::Left),
            ..TextData::default()
        },
        props,
    );

    let (id, kind) = spawned(spawn_display(&mut s, spec));

    assert_eq!(kind, "minecraft:text_display");
    let read = display(&mut s, id);
    let body = text_body(&read);
    assert_eq!(body.text.as_deref(), Some("Hello hologram"));
    assert_eq!(body.line_width, Some(120));
    assert_eq!(body.shadow, Some(true));
    assert_eq!(body.see_through, Some(true));
    assert_eq!(body.default_background, Some(false));
    assert_eq!(body.alignment, Some(TextAlignment::Left));
    assert_eq!(read.props.billboard, Some(Billboard::Center));
    assert_eq!(read.props.brightness, Some((4, 11)));
    assert_eq!(read.props.view_range, Some(2.5));
    assert_eq!(read.props.glow_color, Some(0x00ff_0000));
    let transformation = read.props.transformation.expect("a transformation");
    assert_eq!(transformation.translation, Some([0.0, 1.0, 0.0]));
    assert_eq!(transformation.scale, Some([2.0, 2.0, 2.0]));
    assert_eq!(transformation.left_rotation, Some([0.0, 0.6, 0.0, 0.6]));
}

#[test]
fn editing_a_display_changes_only_the_given_fields_and_syncs_them() {
    let mut s = settled();
    let alice = s.players[0].clone();
    let (id, _) = spawned(spawn_display(
        &mut s,
        text(
            TextData {
                text: Some("first".to_owned()),
                shadow: Some(true),
                ..TextData::default()
            },
            DisplayProps {
                billboard: Some(Billboard::Vertical),
                ..DisplayProps::default()
            },
        ),
    ));
    s.tick(2);
    alice.link.take();

    edit(
        &mut s,
        id,
        text(
            TextData {
                text: Some("second".to_owned()),
                ..TextData::default()
            },
            DisplayProps {
                interpolation_duration: Some(20),
                interpolation_delay: Some(-1),
                ..DisplayProps::default()
            },
        ),
    );
    s.tick(2);

    assert!(s.warnings().is_empty());
    let read = display(&mut s, id);
    let body = text_body(&read);
    assert_eq!(body.text.as_deref(), Some("second"));
    assert_eq!(body.shadow, Some(true));
    assert_eq!(read.props.billboard, Some(Billboard::Vertical));
    assert_eq!(read.props.interpolation_duration, Some(20));
    assert!(
        alice.link.take().iter().any(
            |event| matches!(event, ClientEvent::EntityMetadata { entity, .. } if *entity == id)
        )
    );
}

#[test]
fn an_item_display_holds_an_item_and_a_context() {
    let mut s = settled();
    let spec = DisplaySpec {
        props: DisplayProps::default(),
        body: DisplayBody::Item(ItemDisplayData {
            item: Some(ItemSpec::of("minecraft:diamond_sword", 1)),
            context: Some(DisplayContext::Head),
        }),
    };

    let (id, kind) = spawned(spawn_display(&mut s, spec));

    assert_eq!(kind, "minecraft:item_display");
    let read = display(&mut s, id);
    let DisplayBody::Item(item) = read.body else {
        panic!("expected an item display");
    };
    assert_eq!(
        item.item.map(|item| item.key),
        Some("minecraft:diamond_sword".to_owned())
    );
    assert_eq!(item.context, Some(DisplayContext::Head));
}

#[test]
fn a_block_display_holds_a_block_state() {
    let mut s = settled();
    let spec = DisplaySpec {
        props: DisplayProps::default(),
        body: DisplayBody::Block(BlockDisplayData {
            state: Some(BlockState {
                kind: "minecraft:oak_stairs".to_owned(),
                properties: vec![("facing".to_owned(), "west".to_owned())],
            }),
        }),
    };

    let (id, kind) = spawned(spawn_display(&mut s, spec));

    assert_eq!(kind, "minecraft:block_display");
    let read = display(&mut s, id);
    let DisplayBody::Block(block) = read.body else {
        panic!("expected a block display");
    };
    let state = block.state.expect("a block state");
    assert_eq!(state.kind, "minecraft:oak_stairs");
    assert!(
        state
            .properties
            .contains(&("facing".to_owned(), "west".to_owned()))
    );
}

#[test]
fn an_interaction_takes_a_box_and_a_response_flag() {
    let mut s = settled();
    let spec = DisplaySpec {
        props: DisplayProps::default(),
        body: DisplayBody::Interaction(InteractionData {
            width: Some(2.0),
            height: Some(3.0),
            response: Some(true),
        }),
    };

    let (id, kind) = spawned(spawn_display(&mut s, spec));

    assert_eq!(kind, "minecraft:interaction");
    let read = display(&mut s, id);
    let DisplayBody::Interaction(data) = read.body else {
        panic!("expected an interaction");
    };
    assert_eq!(
        (data.width, data.height, data.response),
        (Some(2.0), Some(3.0), Some(true))
    );
}

#[test]
fn bad_values_and_the_wrong_kind_are_refused() {
    let mut s = settled();
    let (id, _) = spawned(spawn_display(
        &mut s,
        text(TextData::default(), DisplayProps::default()),
    ));

    edit(
        &mut s,
        id,
        text(
            TextData::default(),
            DisplayProps {
                brightness: Some((16, 0)),
                ..DisplayProps::default()
            },
        ),
    );
    edit(
        &mut s,
        id,
        DisplaySpec {
            props: DisplayProps::default(),
            body: DisplayBody::Block(BlockDisplayData::default()),
        },
    );
    edit(
        &mut s,
        id,
        text(
            TextData::default(),
            DisplayProps {
                view_range: Some(f32::NAN),
                ..DisplayProps::default()
            },
        ),
    );

    assert_eq!(s.warnings().len(), 3);
    assert!(matches!(
        spawn_display(
            &mut s,
            DisplaySpec {
                props: DisplayProps {
                    width: Some(-1.0),
                    ..DisplayProps::default()
                },
                body: DisplayBody::Text(TextData::default()),
            }
        ),
        Answer::Failed(_)
    ));
}
