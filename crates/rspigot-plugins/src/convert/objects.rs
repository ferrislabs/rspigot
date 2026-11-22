use std::collections::BTreeMap;

use rspigot_core::plugins::{
    Billboard, BlockDisplayData, DisplayBody, DisplayContext, DisplayProps, DisplaySpec,
    HangingView, InteractionData, ItemDisplayData, MapDecorationEdit, MapDecorationView, MapEdit,
    MapMarker, MapMarkerChange, MapView, ObjectAnswer, ObjectOrder, ObjectQuestion, PdcChange,
    PdcHolder, PdcValue, PlayerObjectOrder, Question, TextAlignment, TextData, Transformation,
    WorldObjectOrder,
};
use rspigot_domain::EntityId;
use rspigot_plugin_protocol::v1::{
    self, map_decoration_edit, map_edit, object_answer, object_entity_order, object_player_order,
    object_question, object_world_order, pdc_change, pdc_query, pdc_value,
};

use super::player;
use super::shared::{self, required};
use super::{Malformed, block, contract};

fn enum_error(field: &'static str, value: i32) -> Malformed {
    Malformed::Enum { field, value }
}

fn pdc_value(value: v1::PdcValue) -> Result<PdcValue, Malformed> {
    use pdc_value::Value;
    Ok(match required(value.value)? {
        Value::Text(text) => PdcValue::Text(text),
        Value::Byte(byte) => PdcValue::Byte(shared::byte(byte)?),
        Value::Int(int) => PdcValue::Int(int),
        Value::Long(long) => PdcValue::Long(long),
        Value::Double(double) => PdcValue::Double(double),
        Value::ByteArray(bytes) => PdcValue::Bytes(bytes),
        Value::Container(container) => PdcValue::Container(
            container
                .entries
                .into_iter()
                .map(|(key, value)| Ok((key, pdc_value(value)?)))
                .collect::<Result<_, Malformed>>()?,
        ),
        Value::List(list) => PdcValue::List(
            list.items
                .into_iter()
                .map(pdc_value)
                .collect::<Result<_, _>>()?,
        ),
    })
}

fn wire_pdc_value(value: PdcValue) -> v1::PdcValue {
    use pdc_value::Value;
    v1::PdcValue {
        value: Some(match value {
            PdcValue::Text(text) => Value::Text(text),
            PdcValue::Byte(byte) => Value::Byte(i32::from(byte)),
            PdcValue::Int(int) => Value::Int(int),
            PdcValue::Long(long) => Value::Long(long),
            PdcValue::Double(double) => Value::Double(double),
            PdcValue::Bytes(bytes) => Value::ByteArray(bytes),
            PdcValue::Container(entries) => Value::Container(wire_container(entries)),
            PdcValue::List(items) => Value::List(v1::PdcList {
                items: items.into_iter().map(wire_pdc_value).collect(),
            }),
        }),
    }
}

fn wire_container(entries: BTreeMap<String, PdcValue>) -> v1::PdcContainer {
    v1::PdcContainer {
        entries: entries
            .into_iter()
            .map(|(key, value)| (key, wire_pdc_value(value)))
            .collect(),
    }
}

pub(super) fn pdc_change(change: v1::PdcChange) -> Result<PdcChange, Malformed> {
    Ok(match required(change.change)? {
        pdc_change::Change::Set(set) => PdcChange::Set {
            key: set.key,
            value: pdc_value(required(set.value)?)?,
        },
        pdc_change::Change::Remove(key) => PdcChange::Remove(key),
        pdc_change::Change::Clear(_) => PdcChange::Clear,
    })
}

fn billboard(value: i32) -> Result<Billboard, Malformed> {
    match v1::Billboard::try_from(value) {
        Ok(v1::Billboard::Fixed) => Ok(Billboard::Fixed),
        Ok(v1::Billboard::Vertical) => Ok(Billboard::Vertical),
        Ok(v1::Billboard::Horizontal) => Ok(Billboard::Horizontal),
        Ok(v1::Billboard::Center) => Ok(Billboard::Center),
        Ok(v1::Billboard::Unspecified) | Err(_) => Err(enum_error("billboard", value)),
    }
}

const fn wire_billboard(value: Billboard) -> v1::Billboard {
    match value {
        Billboard::Fixed => v1::Billboard::Fixed,
        Billboard::Vertical => v1::Billboard::Vertical,
        Billboard::Horizontal => v1::Billboard::Horizontal,
        Billboard::Center => v1::Billboard::Center,
    }
}

fn alignment(value: i32) -> Result<TextAlignment, Malformed> {
    match v1::TextAlignment::try_from(value) {
        Ok(v1::TextAlignment::Center) => Ok(TextAlignment::Center),
        Ok(v1::TextAlignment::Left) => Ok(TextAlignment::Left),
        Ok(v1::TextAlignment::Right) => Ok(TextAlignment::Right),
        Ok(v1::TextAlignment::Unspecified) | Err(_) => Err(enum_error("text alignment", value)),
    }
}

const fn wire_alignment(value: TextAlignment) -> v1::TextAlignment {
    match value {
        TextAlignment::Center => v1::TextAlignment::Center,
        TextAlignment::Left => v1::TextAlignment::Left,
        TextAlignment::Right => v1::TextAlignment::Right,
    }
}

fn context(value: i32) -> Result<DisplayContext, Malformed> {
    use v1::ItemDisplayContext as Wire;
    match Wire::try_from(value) {
        Ok(Wire::None) => Ok(DisplayContext::None),
        Ok(Wire::ThirdPersonLeftHand) => Ok(DisplayContext::ThirdPersonLeftHand),
        Ok(Wire::ThirdPersonRightHand) => Ok(DisplayContext::ThirdPersonRightHand),
        Ok(Wire::FirstPersonLeftHand) => Ok(DisplayContext::FirstPersonLeftHand),
        Ok(Wire::FirstPersonRightHand) => Ok(DisplayContext::FirstPersonRightHand),
        Ok(Wire::Head) => Ok(DisplayContext::Head),
        Ok(Wire::Gui) => Ok(DisplayContext::Gui),
        Ok(Wire::Ground) => Ok(DisplayContext::Ground),
        Ok(Wire::Fixed) => Ok(DisplayContext::Fixed),
        Ok(Wire::Unspecified) | Err(_) => Err(enum_error("item display context", value)),
    }
}

const fn wire_context(value: DisplayContext) -> v1::ItemDisplayContext {
    use v1::ItemDisplayContext as Wire;
    match value {
        DisplayContext::None => Wire::None,
        DisplayContext::ThirdPersonLeftHand => Wire::ThirdPersonLeftHand,
        DisplayContext::ThirdPersonRightHand => Wire::ThirdPersonRightHand,
        DisplayContext::FirstPersonLeftHand => Wire::FirstPersonLeftHand,
        DisplayContext::FirstPersonRightHand => Wire::FirstPersonRightHand,
        DisplayContext::Head => Wire::Head,
        DisplayContext::Gui => Wire::Gui,
        DisplayContext::Ground => Wire::Ground,
        DisplayContext::Fixed => Wire::Fixed,
    }
}

const fn vec3f(vec: v1::Vec3F) -> [f32; 3] {
    [vec.x, vec.y, vec.z]
}

const fn wire_vec3f(vec: [f32; 3]) -> v1::Vec3F {
    v1::Vec3F {
        x: vec[0],
        y: vec[1],
        z: vec[2],
    }
}

const fn quat(value: v1::Quat) -> [f32; 4] {
    [value.x, value.y, value.z, value.w]
}

const fn wire_quat(value: [f32; 4]) -> v1::Quat {
    v1::Quat {
        x: value[0],
        y: value[1],
        z: value[2],
        w: value[3],
    }
}

fn props(props: v1::DisplayProps) -> Result<DisplayProps, Malformed> {
    Ok(DisplayProps {
        transformation: props.transformation.map(|value| Transformation {
            translation: value.translation.map(vec3f),
            scale: value.scale.map(vec3f),
            left_rotation: value.left_rotation.map(quat),
            right_rotation: value.right_rotation.map(quat),
        }),
        billboard: props.billboard.map(billboard).transpose()?,
        brightness: props.brightness.map(|value| (value.block, value.sky)),
        view_range: props.view_range,
        glow_color: props.glow_color,
        interpolation_delay: props.interpolation_delay,
        interpolation_duration: props.interpolation_duration,
        teleport_duration: props.teleport_duration,
        shadow_radius: props.shadow_radius,
        shadow_strength: props.shadow_strength,
        width: props.width,
        height: props.height,
    })
}

fn wire_props(props: DisplayProps) -> v1::DisplayProps {
    v1::DisplayProps {
        transformation: props.transformation.map(|value| v1::DisplayTransformation {
            translation: value.translation.map(wire_vec3f),
            scale: value.scale.map(wire_vec3f),
            left_rotation: value.left_rotation.map(wire_quat),
            right_rotation: value.right_rotation.map(wire_quat),
        }),
        billboard: props.billboard.map(|value| wire_billboard(value) as i32),
        brightness: props
            .brightness
            .map(|(block, sky)| v1::DisplayBrightness { block, sky }),
        view_range: props.view_range,
        glow_color: props.glow_color,
        interpolation_delay: props.interpolation_delay,
        interpolation_duration: props.interpolation_duration,
        teleport_duration: props.teleport_duration,
        shadow_radius: props.shadow_radius,
        shadow_strength: props.shadow_strength,
        width: props.width,
        height: props.height,
    }
}

pub(super) fn display_spec(spec: v1::DisplaySpec) -> Result<DisplaySpec, Malformed> {
    use v1::display_spec::Body;
    let body = match required(spec.body)? {
        Body::Text(text) => DisplayBody::Text(TextData {
            text: text.text,
            line_width: text.line_width,
            background: text.background,
            opacity: text.opacity,
            shadow: text.shadow,
            see_through: text.see_through,
            default_background: text.default_background,
            alignment: text.alignment.map(alignment).transpose()?,
        }),
        Body::Item(item) => DisplayBody::Item(ItemDisplayData {
            item: item.item.map(shared::item),
            context: item.context.map(context).transpose()?,
        }),
        Body::Block(block) => DisplayBody::Block(BlockDisplayData {
            state: block.state.map(block::state),
        }),
        Body::Interaction(data) => DisplayBody::Interaction(InteractionData {
            width: data.width,
            height: data.height,
            response: data.response,
        }),
    };
    Ok(DisplaySpec {
        props: props(spec.props.unwrap_or_default())?,
        body,
    })
}

fn wire_display_spec(spec: DisplaySpec) -> v1::DisplaySpec {
    use v1::display_spec::Body;
    let body = match spec.body {
        DisplayBody::Text(text) => Body::Text(v1::TextDisplayData {
            text: text.text,
            line_width: text.line_width,
            background: text.background,
            opacity: text.opacity,
            shadow: text.shadow,
            see_through: text.see_through,
            default_background: text.default_background,
            alignment: text.alignment.map(|value| wire_alignment(value) as i32),
        }),
        DisplayBody::Item(item) => Body::Item(v1::ItemDisplayData {
            item: item.item.map(shared::wire_item),
            context: item.context.map(|value| wire_context(value) as i32),
        }),
        DisplayBody::Block(block) => Body::Block(v1::BlockDisplayData {
            state: block.state.map(block::wire_state),
        }),
        DisplayBody::Interaction(data) => Body::Interaction(v1::InteractionData {
            width: data.width,
            height: data.height,
            response: data.response,
        }),
    };
    v1::DisplaySpec {
        props: Some(wire_props(spec.props)),
        body: Some(body),
    }
}

pub(super) fn entity_order(order: v1::ObjectEntityOrder) -> Result<ObjectOrder, Malformed> {
    use object_entity_order::Order;
    Ok(match required(order.order)? {
        Order::SetTrades(list) => ObjectOrder::SetTrades(
            list.trades
                .into_iter()
                .map(contract::trade)
                .collect::<Result<_, _>>()?,
        ),
        Order::EditDisplay(spec) => ObjectOrder::EditDisplay(display_spec(spec)?),
        Order::DetonateFirework(_) => ObjectOrder::DetonateFirework,
        Order::SetVehicleVelocity(velocity) => {
            ObjectOrder::SetVehicleVelocity(shared::vec3(velocity))
        }
        Order::SetFrameItem(frame) => ObjectOrder::SetFrameItem(frame.item.map(shared::item)),
        Order::SetFrameRotation(rotation) => ObjectOrder::SetFrameRotation(rotation),
        Order::Pdc(change) => ObjectOrder::Pdc(pdc_change(change)?),
        Order::SetFrameVisible(visible) => ObjectOrder::SetFrameVisible(visible),
    })
}

fn map_marker_edit(edit: v1::MapDecorationEdit) -> Result<MapDecorationEdit, Malformed> {
    let change = match required(edit.change)? {
        map_decoration_edit::Change::Set(marker) => MapMarkerChange::Set(MapMarker {
            kind: marker.kind,
            x: marker.x,
            z: marker.z,
            rotation: marker.rotation,
        }),
        map_decoration_edit::Change::Remove(_) => MapMarkerChange::Remove,
    };
    Ok(MapDecorationEdit {
        slot: edit.slot,
        key: edit.key,
        change,
    })
}

pub(super) fn player_order(order: v1::ObjectPlayerOrder) -> Result<PlayerObjectOrder, Malformed> {
    use object_player_order::Order;
    Ok(match required(order.order)? {
        Order::OpenMerchant(entity) => PlayerObjectOrder::OpenMerchant(EntityId(entity)),
        Order::ItemPdc(pdc) => PlayerObjectOrder::ItemPdc {
            slot: pdc.slot,
            change: pdc_change(required(pdc.change)?)?,
        },
        Order::MapDecoration(edit) => PlayerObjectOrder::MapDecoration(map_marker_edit(edit)?),
    })
}

pub(super) fn world_order(order: v1::ObjectWorldOrder) -> Result<WorldObjectOrder, Malformed> {
    use object_world_order::Order;
    let Order::EditMap(edit) = required(order.order)?;
    let change = match required(edit.edit)? {
        map_edit::Edit::SetCenter(center) => MapEdit::SetCenter {
            x: center.x,
            z: center.z,
        },
        map_edit::Edit::SetScale(scale) => {
            MapEdit::SetScale(u8::try_from(scale).map_err(|_| Malformed::OutOfRange {
                field: "map scale",
                value: i64::from(scale),
            })?)
        }
        map_edit::Edit::SetLocked(locked) => MapEdit::SetLocked(locked),
        map_edit::Edit::SetTrackingPosition(tracking) => MapEdit::SetTrackingPosition(tracking),
        map_edit::Edit::SetUnlimitedTracking(unlimited) => MapEdit::SetUnlimitedTracking(unlimited),
        map_edit::Edit::SetPixel(pixel) => MapEdit::SetPixel {
            x: pixel.x,
            y: pixel.y,
            color: u8::try_from(pixel.color).map_err(|_| Malformed::OutOfRange {
                field: "map color",
                value: i64::from(pixel.color),
            })?,
        },
        map_edit::Edit::SetCanvas(colors) => MapEdit::SetCanvas(colors),
        map_edit::Edit::AddMarker(marker) => MapEdit::AddMarker {
            key: marker.key,
            kind: marker.kind,
            x: marker.x,
            z: marker.z,
            rotation: marker.rotation,
            name: marker.name,
        },
        map_edit::Edit::RemoveMarker(key) => MapEdit::RemoveMarker(key),
        map_edit::Edit::ClearMarkers(_) => MapEdit::ClearMarkers,
    };
    Ok(WorldObjectOrder::EditMap {
        map: edit.map,
        edit: change,
    })
}

fn optional_entity(id: Option<i32>) -> Option<EntityId> {
    id.map(EntityId)
}

pub(super) fn question(question: v1::ObjectQuestion) -> Result<Question, Malformed> {
    use object_question::Query;
    Ok(Question::Object(match required(question.query)? {
        Query::LaunchProjectile(launch) => ObjectQuestion::LaunchProjectile {
            kind: launch.kind,
            position: shared::vec3(required(launch.position)?),
            velocity: shared::vec3(required(launch.velocity)?),
            shooter: optional_entity(launch.shooter),
        },
        Query::LaunchFirework(launch) => ObjectQuestion::LaunchFirework {
            position: shared::vec3(required(launch.position)?),
            item: shared::item(required(launch.item)?),
            shooter: optional_entity(launch.shooter),
            attached: optional_entity(launch.attached),
            angled: launch.angled,
            velocity: launch.velocity.map(shared::vec3),
        },
        Query::SpawnVehicle(spawn) => ObjectQuestion::SpawnVehicle {
            kind: spawn.kind,
            position: shared::vec3(required(spawn.position)?),
            velocity: spawn.velocity.map(shared::vec3),
        },
        Query::PlaceHanging(place) => ObjectQuestion::PlaceHanging {
            kind: place.kind,
            pos: shared::block_pos(required(place.pos)?),
            face: place.face,
            variant: place.variant,
            item: place.item.map(shared::item),
        },
        Query::SpawnDisplay(spawn) => ObjectQuestion::SpawnDisplay {
            position: shared::vec3(required(spawn.position)?),
            spec: display_spec(required(spawn.spec)?)?,
        },
        Query::Trades(entity) => ObjectQuestion::Trades(EntityId(entity)),
        Query::Display(entity) => ObjectQuestion::Display(EntityId(entity)),
        Query::Map(map) => ObjectQuestion::Map(map),
        Query::Hanging(entity) => ObjectQuestion::Hanging(EntityId(entity)),
        Query::Pdc(query) => ObjectQuestion::Pdc(match required(query.holder)? {
            pdc_query::Holder::Entity(entity) => PdcHolder::Entity(EntityId(entity)),
            pdc_query::Holder::Block(pos) => PdcHolder::Block(shared::block_pos(pos)),
            pdc_query::Holder::Item(item) => PdcHolder::Item {
                player: player::id(item.player)?,
                slot: item.slot,
            },
        }),
    }))
}

fn wire_map(view: MapView) -> v1::MapView {
    v1::MapView {
        id: view.id,
        dimension: view.dimension,
        center_x: view.center_x,
        center_z: view.center_z,
        scale: u32::from(view.scale),
        locked: view.locked,
        tracking_position: view.tracking_position,
        unlimited_tracking: view.unlimited_tracking,
        colors: view.colors,
        decorations: view.decorations.into_iter().map(wire_decoration).collect(),
    }
}

fn wire_decoration(decoration: MapDecorationView) -> v1::MapDecorationInfo {
    v1::MapDecorationInfo {
        kind: u32::from(decoration.kind),
        x: i32::from(decoration.x),
        y: i32::from(decoration.y),
        rotation: i32::from(decoration.rotation),
        name: decoration.name,
    }
}

fn wire_hanging(view: HangingView) -> v1::HangingView {
    v1::HangingView {
        kind: view.kind,
        pos: Some(shared::wire_block_pos(view.pos)),
        facing: view.facing,
        item: view.item.map(shared::wire_item),
        rotation: view.rotation,
        visible: view.visible,
        variant: view.variant,
    }
}

pub(super) fn wire_answer(answer: ObjectAnswer) -> v1::ObjectAnswer {
    use object_answer::Answer;
    v1::ObjectAnswer {
        answer: Some(match answer {
            ObjectAnswer::Trades(trades) => Answer::Trades(v1::TradeList {
                trades: trades.into_iter().map(contract::wire_trade).collect(),
            }),
            ObjectAnswer::Display(spec) => Answer::Display(wire_display_spec(*spec)),
            ObjectAnswer::Map(view) => Answer::Map(wire_map(*view)),
            ObjectAnswer::Pdc(entries) => Answer::Pdc(wire_container(entries)),
            ObjectAnswer::Hanging(view) => Answer::Hanging(wire_hanging(*view)),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wire(value: pdc_value::Value) -> v1::PdcValue {
        v1::PdcValue { value: Some(value) }
    }

    #[test]
    fn a_pdc_value_survives_the_wire_both_ways() {
        let nested = PdcValue::Container(BTreeMap::from([
            ("n".to_owned(), PdcValue::Byte(-2)),
            (
                "l".to_owned(),
                PdcValue::List(vec![PdcValue::Long(1), PdcValue::Long(2)]),
            ),
            ("b".to_owned(), PdcValue::Bytes(vec![1, 2])),
        ]));
        assert_eq!(pdc_value(wire_pdc_value(nested.clone())).unwrap(), nested);
        assert_eq!(
            pdc_value(wire(pdc_value::Value::Double(1.5))).unwrap(),
            PdcValue::Double(1.5)
        );
    }

    #[test]
    fn a_byte_outside_its_range_an_empty_value_or_an_unknown_enum_is_malformed() {
        assert!(pdc_value(wire(pdc_value::Value::Byte(300))).is_err());
        assert!(pdc_value(v1::PdcValue { value: None }).is_err());
        assert!(
            props(v1::DisplayProps {
                billboard: Some(0),
                ..v1::DisplayProps::default()
            })
            .is_err()
        );
        assert!(display_spec(v1::DisplaySpec::default()).is_err());
    }

    #[test]
    fn a_display_spec_survives_the_wire_both_ways() {
        let spec = DisplaySpec {
            props: DisplayProps {
                billboard: Some(Billboard::Center),
                brightness: Some((3, 9)),
                transformation: Some(Transformation {
                    translation: Some([1.0, 2.0, 3.0]),
                    scale: None,
                    left_rotation: Some([0.0, 0.0, 0.0, 1.0]),
                    right_rotation: None,
                }),
                ..DisplayProps::default()
            },
            body: DisplayBody::Text(TextData {
                text: Some("hi".to_owned()),
                alignment: Some(TextAlignment::Right),
                ..TextData::default()
            }),
        };
        assert_eq!(display_spec(wire_display_spec(spec.clone())).unwrap(), spec);
    }

    #[test]
    fn a_launch_question_reaches_the_core_with_its_shooter() {
        let asked = question(v1::ObjectQuestion {
            query: Some(object_question::Query::LaunchProjectile(
                v1::LaunchProjectile {
                    kind: "minecraft:arrow".to_owned(),
                    position: Some(v1::Vec3::default()),
                    velocity: Some(v1::Vec3 {
                        x: 0.0,
                        y: 1.0,
                        z: 0.0,
                    }),
                    shooter: Some(5),
                },
            )),
        })
        .unwrap();
        assert!(matches!(
            asked,
            Question::Object(ObjectQuestion::LaunchProjectile {
                shooter: Some(EntityId(5)),
                ..
            })
        ));
        assert!(
            question(v1::ObjectQuestion {
                query: Some(object_question::Query::LaunchProjectile(
                    v1::LaunchProjectile::default()
                )),
            })
            .is_err()
        );
    }

    #[test]
    fn a_map_edit_with_a_color_outside_a_byte_is_malformed() {
        let edit = v1::ObjectWorldOrder {
            order: Some(object_world_order::Order::EditMap(v1::MapEdit {
                map: 1,
                edit: Some(map_edit::Edit::SetPixel(v1::MapPixel {
                    x: 0,
                    y: 0,
                    color: 256,
                })),
            })),
        };
        assert!(world_order(edit).is_err());
    }
}
