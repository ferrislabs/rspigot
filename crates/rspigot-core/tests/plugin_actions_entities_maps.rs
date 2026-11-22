mod atlas;
mod common;

use atlas::*;
use common::*;
use rspigot_core::plugins::{
    Answer, MapDecorationEdit, MapEdit, MapMarker, MapMarkerChange, ObjectAnswer, ObjectQuestion,
    Order, PlayerObjectOrder, PlayerOrder, Question, WorldObjectOrder,
};
use rspigot_domain::map_data::{MAP_COLORS, MapData};

const MAP: i32 = 7;

fn stored_map() -> MapData {
    MapData::fresh(0.0, 0.0, 1, true, false, OVERWORLD)
}

fn edit(s: &mut Scene, edit: MapEdit) {
    s.act(Order::World(rspigot_core::plugins::WorldOrder::Object(
        WorldObjectOrder::EditMap { map: MAP, edit },
    )));
}

fn view(s: &mut Scene) -> rspigot_core::plugins::MapView {
    match s.ask(Question::Object(ObjectQuestion::Map(MAP))) {
        Answer::Object(ObjectAnswer::Map(view)) => *view,
        other => panic!("expected a map, got {other:?}"),
    }
}

fn saved_when(atlas: &Atlas, ready: impl Fn(&MapData) -> bool) -> MapData {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let data = atlas.map(MAP);
        if ready(&data) || std::time::Instant::now() > deadline {
            return data;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

fn map_scene() -> (Scene, std::sync::Arc<Atlas>) {
    let (mut s, atlas) = scene_with_atlas(Meadow::default());
    atlas.put_map(MAP, &stored_map());
    s.tick(5);
    (s, atlas)
}

#[test]
fn a_map_reports_its_center_scale_flags_and_canvas() {
    let (mut s, _atlas) = map_scene();

    let view = view(&mut s);

    assert_eq!(view.id, MAP);
    assert_eq!(view.scale, 1);
    assert!(!view.locked);
    assert!(view.tracking_position);
    assert_eq!(view.colors.len(), MAP_COLORS);
    assert_eq!(view.dimension, OVERWORLD);
}

#[test]
fn center_scale_and_flags_are_set_and_saved() {
    let (mut s, atlas) = map_scene();

    edit(&mut s, MapEdit::SetCenter { x: 256, z: -128 });
    edit(&mut s, MapEdit::SetScale(3));
    edit(&mut s, MapEdit::SetLocked(true));
    edit(&mut s, MapEdit::SetTrackingPosition(false));
    edit(&mut s, MapEdit::SetUnlimitedTracking(true));

    assert!(s.warnings().is_empty());
    let view = view(&mut s);
    assert_eq!((view.center_x, view.center_z), (256, -128));
    assert_eq!(view.scale, 3);
    assert!(view.locked);
    assert!(!view.tracking_position);
    assert!(view.unlimited_tracking);
    s.server.save_maps();
    let saved = saved_when(&atlas, |data| data.scale == 3);
    assert_eq!(
        (saved.center_x, saved.center_z, saved.scale),
        (256, -128, 3)
    );
    assert!(saved.locked);
}

#[test]
fn pixels_and_a_whole_canvas_are_written() {
    let (mut s, atlas) = map_scene();

    edit(
        &mut s,
        MapEdit::SetPixel {
            x: 3,
            y: 4,
            color: 44,
        },
    );
    assert_eq!(view(&mut s).colors[3 + 4 * 128], 44);

    let mut canvas = vec![0u8; MAP_COLORS];
    canvas[127 + 127 * 128] = 9;
    canvas[0] = 5;
    edit(&mut s, MapEdit::SetCanvas(canvas));

    assert!(s.warnings().is_empty());
    let colors = view(&mut s).colors;
    assert_eq!(colors[3 + 4 * 128], 0);
    assert_eq!((colors[0], colors[127 + 127 * 128]), (5, 9));
    s.server.save_maps();
    let saved = saved_when(&atlas, |data| data.colors()[127 + 127 * 128] == 9);
    assert_eq!(saved.colors()[127 + 127 * 128], 9);
}

#[test]
fn a_bad_scale_pixel_canvas_or_map_is_refused() {
    let (mut s, _atlas) = map_scene();

    edit(&mut s, MapEdit::SetScale(5));
    edit(
        &mut s,
        MapEdit::SetPixel {
            x: 128,
            y: 0,
            color: 1,
        },
    );
    edit(&mut s, MapEdit::SetCanvas(vec![0; 10]));

    assert_eq!(s.warnings().len(), 3);
    assert_eq!(view(&mut s).scale, 1);
    assert!(matches!(
        s.ask(Question::Object(ObjectQuestion::Map(99))),
        Answer::Failed(_)
    ));
}

#[test]
fn a_marker_is_added_to_and_removed_from_the_held_map() {
    let (mut s, _atlas) = map_scene();
    put_in_slot(&mut s, 36, filled_stack(MAP));
    let alice = s.players[0].clone();
    let marker = |change| {
        to(
            &alice,
            PlayerOrder::Object(PlayerObjectOrder::MapDecoration(MapDecorationEdit {
                slot: 0,
                key: "home".to_owned(),
                change,
            })),
        )
    };
    let markers = |s: &mut Scene| match s.ask(Question::Inventory(alice.id)) {
        Answer::Inventory(state) => {
            let spec = state.slots[0].clone().expect("the map is in the slot");
            let mut bytes = &spec.components[..];
            rspigot_nbt::read_network_compound(&mut bytes)
                .unwrap()
                .and_then(|components| {
                    components
                        .get_compound("minecraft:map_decorations")
                        .cloned()
                })
        }
        other => panic!("expected an inventory, got {other:?}"),
    };

    s.act(marker(MapMarkerChange::Set(MapMarker {
        kind: "minecraft:red_x".to_owned(),
        x: 12.0,
        z: -30.0,
        rotation: 90.0,
    })));

    assert!(s.warnings().is_empty());
    let held = markers(&mut s).expect("a marker is set");
    let home = held.get_compound("home").expect("the home marker");
    assert_eq!(home.get_str("type"), Some("minecraft:red_x"));
    assert_eq!(home.get_f64("x"), Some(12.0));

    s.act(marker(MapMarkerChange::Remove));
    assert!(markers(&mut s).is_none());

    s.act(marker(MapMarkerChange::Set(MapMarker {
        kind: "minecraft:nothing".to_owned(),
        x: 0.0,
        z: 0.0,
        rotation: 0.0,
    })));
    assert_eq!(s.warnings().len(), 1);
}

fn homes(listed: &[rspigot_core::plugins::MapDecorationView]) -> usize {
    listed
        .iter()
        .filter(|d| d.name.as_deref() == Some("Home"))
        .count()
}

fn marker_edit(s: &mut Scene, edit_order: MapEdit) {
    edit(s, edit_order);
}

#[test]
fn a_live_marker_is_listed_and_synced_to_a_viewer_until_removed() {
    let (mut s, _atlas) = map_scene();
    put_in_slot(&mut s, 36, filled_stack(MAP));
    let alice = s.players[0].clone();
    s.tick(12);
    alice.link.take();

    marker_edit(
        &mut s,
        MapEdit::AddMarker {
            key: "home".to_owned(),
            kind: "minecraft:red_x".to_owned(),
            x: 10.0,
            z: -6.0,
            rotation: 0.0,
            name: Some("Home".to_owned()),
        },
    );
    s.tick(12);

    assert!(s.warnings().is_empty());
    let listed = view(&mut s).decorations;
    assert_eq!(homes(&listed), 1);
    let synced = alice.link.take().into_iter().find_map(|event| match event {
        rspigot_core::events::ClientEvent::MapData(update) => update.decorations,
        _ => None,
    });
    let synced = synced.expect("the viewer is told");
    assert_eq!(
        synced
            .iter()
            .filter(|d| d.name.as_ref().map(|n| n.to_legacy()).as_deref() == Some("Home"))
            .count(),
        1
    );

    marker_edit(&mut s, MapEdit::RemoveMarker("home".to_owned()));
    s.tick(12);
    assert_eq!(homes(&view(&mut s).decorations), 0);
    let cleared = alice.link.take().into_iter().find_map(|event| match event {
        rspigot_core::events::ClientEvent::MapData(update) => update.decorations,
        _ => None,
    });
    assert!(
        cleared
            .expect("the viewer is told")
            .iter()
            .all(|d| d.name.is_none())
    );

    marker_edit(
        &mut s,
        MapEdit::AddMarker {
            key: "a".to_owned(),
            kind: "minecraft:nothing".to_owned(),
            x: 0.0,
            z: 0.0,
            rotation: 0.0,
            name: None,
        },
    );
    assert_eq!(s.warnings().len(), 1);
    marker_edit(&mut s, MapEdit::ClearMarkers);
    assert_eq!(homes(&view(&mut s).decorations), 0);
}
