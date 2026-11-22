use rspigot_domain::EntityId;
use rspigot_domain::entity::decor::{
    BLOCK_STATE, DISPLAY_BILLBOARD, DISPLAY_BRIGHTNESS, DISPLAY_GLOW_COLOR, DISPLAY_HEIGHT,
    DISPLAY_INTERPOLATION_DURATION, DISPLAY_LEFT_ROTATION, DISPLAY_RIGHT_ROTATION, DISPLAY_SCALE,
    DISPLAY_SHADOW_RADIUS, DISPLAY_SHADOW_STRENGTH, DISPLAY_START_INTERPOLATION,
    DISPLAY_TELEPORT_DURATION, DISPLAY_TRANSLATION, DISPLAY_VIEW_RANGE, DISPLAY_WIDTH,
    INTERACTION_HEIGHT, INTERACTION_RESPONSE, INTERACTION_WIDTH, ITEM_CONTEXT, ITEM_STACK,
    TEXT_BACKGROUND, TEXT_FLAGS, TEXT_LINE_WIDTH, TEXT_OPACITY, TEXT_TEXT, is_display,
};
use rspigot_domain::entity::{Entity, EntityBase, SyncedValue};
use rspigot_entities::EntityKind as Kind;

use crate::plugins::block::{block_state_of, data_of};
use crate::plugins::items::{spec_of, stack_of};
use crate::plugins::resolve;
use crate::plugins::{ActionError, ActionErrorKind, BlockState, ItemSpec};
use crate::ports::ClientLink;
use crate::server::Server;

const SHADOW: i8 = 1;
const SEE_THROUGH: i8 = 2;
const DEFAULT_BACKGROUND: i8 = 4;
const ALIGN_LEFT: i8 = 8;
const ALIGN_RIGHT: i8 = 16;
const ALIGNMENT_BITS: i8 = ALIGN_LEFT | ALIGN_RIGHT;
const LIGHT_MAX: i32 = 15;
const BLOCK_SHIFT: u32 = 4;
const SKY_SHIFT: u32 = 20;
const UNSET: i32 = -1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Billboard {
    Fixed,
    Vertical,
    Horizontal,
    Center,
}

impl Billboard {
    const ALL: [Self; 4] = [Self::Fixed, Self::Vertical, Self::Horizontal, Self::Center];

    const fn index(self) -> i8 {
        match self {
            Self::Fixed => 0,
            Self::Vertical => 1,
            Self::Horizontal => 2,
            Self::Center => 3,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextAlignment {
    Center,
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayContext {
    None,
    ThirdPersonLeftHand,
    ThirdPersonRightHand,
    FirstPersonLeftHand,
    FirstPersonRightHand,
    Head,
    Gui,
    Ground,
    Fixed,
}

impl DisplayContext {
    const ALL: [Self; 9] = [
        Self::None,
        Self::ThirdPersonLeftHand,
        Self::ThirdPersonRightHand,
        Self::FirstPersonLeftHand,
        Self::FirstPersonRightHand,
        Self::Head,
        Self::Gui,
        Self::Ground,
        Self::Fixed,
    ];

    fn index(self) -> i8 {
        Self::ALL.iter().position(|c| *c == self).unwrap_or(0) as i8
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Transformation {
    pub translation: Option<[f32; 3]>,
    pub scale: Option<[f32; 3]>,
    pub left_rotation: Option<[f32; 4]>,
    pub right_rotation: Option<[f32; 4]>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct DisplayProps {
    pub transformation: Option<Transformation>,
    pub billboard: Option<Billboard>,
    pub brightness: Option<(i32, i32)>,
    pub view_range: Option<f32>,
    pub glow_color: Option<i32>,
    pub interpolation_delay: Option<i32>,
    pub interpolation_duration: Option<i32>,
    pub teleport_duration: Option<i32>,
    pub shadow_radius: Option<f32>,
    pub shadow_strength: Option<f32>,
    pub width: Option<f32>,
    pub height: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct TextData {
    pub text: Option<String>,
    pub line_width: Option<i32>,
    pub background: Option<i32>,
    pub opacity: Option<i32>,
    pub shadow: Option<bool>,
    pub see_through: Option<bool>,
    pub default_background: Option<bool>,
    pub alignment: Option<TextAlignment>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ItemDisplayData {
    pub item: Option<ItemSpec>,
    pub context: Option<DisplayContext>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct BlockDisplayData {
    pub state: Option<BlockState>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct InteractionData {
    pub width: Option<f32>,
    pub height: Option<f32>,
    pub response: Option<bool>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DisplayBody {
    Text(TextData),
    Item(ItemDisplayData),
    Block(BlockDisplayData),
    Interaction(InteractionData),
}

#[derive(Debug, Clone, PartialEq)]
pub struct DisplaySpec {
    pub props: DisplayProps,
    pub body: DisplayBody,
}

impl DisplaySpec {
    pub const fn kind(&self) -> Kind {
        match self.body {
            DisplayBody::Text(_) => Kind::TextDisplay,
            DisplayBody::Item(_) => Kind::ItemDisplay,
            DisplayBody::Block(_) => Kind::BlockDisplay,
            DisplayBody::Interaction(_) => Kind::Interaction,
        }
    }
}

fn invalid(reason: impl Into<String>) -> ActionError {
    ActionError::new(ActionErrorKind::InvalidArgument, reason)
}

fn finite(what: &str, values: &[f32]) -> Result<(), ActionError> {
    if values.iter().all(|value| value.is_finite()) {
        Ok(())
    } else {
        Err(invalid(format!("{what} is not finite")))
    }
}

fn not_negative(what: &str, value: f32) -> Result<f32, ActionError> {
    if value.is_finite() && value >= 0.0 {
        Ok(value)
    } else {
        Err(invalid(format!("{what} {value} is negative or not finite")))
    }
}

fn pack_brightness((block, sky): (i32, i32)) -> Result<i32, ActionError> {
    if !(0..=LIGHT_MAX).contains(&block) || !(0..=LIGHT_MAX).contains(&sky) {
        return Err(invalid(format!(
            "brightness {block}/{sky} is outside 0..={LIGHT_MAX}"
        )));
    }
    Ok(block << BLOCK_SHIFT | sky << SKY_SHIFT)
}

fn unpack_brightness(packed: i32) -> Option<(i32, i32)> {
    (packed != UNSET).then_some((
        (packed >> BLOCK_SHIFT) & LIGHT_MAX,
        (packed >> SKY_SHIFT) & LIGHT_MAX,
    ))
}

fn set_transformation(base: &mut EntityBase, value: &Transformation) -> Result<(), ActionError> {
    if let Some(translation) = value.translation {
        finite("a translation", &translation)?;
        base.data
            .set(DISPLAY_TRANSLATION, SyncedValue::Vector3(translation));
    }
    if let Some(scale) = value.scale {
        finite("a scale", &scale)?;
        base.data.set(DISPLAY_SCALE, SyncedValue::Vector3(scale));
    }
    if let Some(rotation) = value.left_rotation {
        finite("a rotation", &rotation)?;
        base.data
            .set(DISPLAY_LEFT_ROTATION, SyncedValue::Quaternion(rotation));
    }
    if let Some(rotation) = value.right_rotation {
        finite("a rotation", &rotation)?;
        base.data
            .set(DISPLAY_RIGHT_ROTATION, SyncedValue::Quaternion(rotation));
    }
    Ok(())
}

fn set_props(base: &mut EntityBase, props: &DisplayProps) -> Result<(), ActionError> {
    if let Some(value) = &props.transformation {
        set_transformation(base, value)?;
    }
    if let Some(billboard) = props.billboard {
        base.data
            .set(DISPLAY_BILLBOARD, SyncedValue::Byte(billboard.index()));
    }
    if let Some(brightness) = props.brightness {
        base.data.set(
            DISPLAY_BRIGHTNESS,
            SyncedValue::Int(pack_brightness(brightness)?),
        );
    }
    for (index, value, what) in [
        (DISPLAY_VIEW_RANGE, props.view_range, "view range"),
        (DISPLAY_SHADOW_RADIUS, props.shadow_radius, "shadow radius"),
        (
            DISPLAY_SHADOW_STRENGTH,
            props.shadow_strength,
            "shadow strength",
        ),
        (DISPLAY_WIDTH, props.width, "width"),
        (DISPLAY_HEIGHT, props.height, "height"),
    ] {
        if let Some(value) = value {
            base.data
                .set(index, SyncedValue::Float(not_negative(what, value)?));
        }
    }
    for (index, value) in [
        (DISPLAY_GLOW_COLOR, props.glow_color),
        (DISPLAY_START_INTERPOLATION, props.interpolation_delay),
        (DISPLAY_INTERPOLATION_DURATION, props.interpolation_duration),
        (DISPLAY_TELEPORT_DURATION, props.teleport_duration),
    ] {
        if let Some(value) = value {
            base.data.set(index, SyncedValue::Int(value));
        }
    }
    Ok(())
}

fn set_flag(flags: &mut i8, bit: i8, value: Option<bool>) {
    match value {
        Some(true) => *flags |= bit,
        Some(false) => *flags &= !bit,
        None => {}
    }
}

fn set_text(base: &mut EntityBase, text: &TextData) {
    if let Some(value) = &text.text {
        base.data
            .set(TEXT_TEXT, SyncedValue::Component(resolve::text(value)));
    }
    if let Some(value) = text.line_width {
        base.data.set(TEXT_LINE_WIDTH, SyncedValue::Int(value));
    }
    if let Some(value) = text.background {
        base.data.set(TEXT_BACKGROUND, SyncedValue::Int(value));
    }
    if let Some(value) = text.opacity {
        base.data
            .set(TEXT_OPACITY, SyncedValue::Byte(value.clamp(-1, 255) as i8));
    }
    let mut flags = base.data.byte(TEXT_FLAGS);
    set_flag(&mut flags, SHADOW, text.shadow);
    set_flag(&mut flags, SEE_THROUGH, text.see_through);
    set_flag(&mut flags, DEFAULT_BACKGROUND, text.default_background);
    if let Some(alignment) = text.alignment {
        flags &= !ALIGNMENT_BITS;
        flags |= match alignment {
            TextAlignment::Center => 0,
            TextAlignment::Left => ALIGN_LEFT,
            TextAlignment::Right => ALIGN_RIGHT,
        };
    }
    base.data.set(TEXT_FLAGS, SyncedValue::Byte(flags));
}

fn set_item(base: &mut EntityBase, item: &ItemDisplayData) -> Result<(), ActionError> {
    if let Some(spec) = &item.item {
        let stack = stack_of(spec).map_err(invalid)?;
        base.data.set(ITEM_STACK, SyncedValue::Item(stack));
    }
    if let Some(context) = item.context {
        base.data
            .set(ITEM_CONTEXT, SyncedValue::Byte(context.index()));
    }
    Ok(())
}

fn set_block(base: &mut EntityBase, block: &BlockDisplayData) -> Result<(), ActionError> {
    if let Some(state) = &block.state {
        let id = block_state_of(state)
            .ok_or_else(|| invalid(format!("{} is not a block state", state.kind)))?;
        base.data.set(BLOCK_STATE, SyncedValue::BlockState(id));
    }
    Ok(())
}

fn set_interaction(base: &mut EntityBase, data: &InteractionData) -> Result<(), ActionError> {
    if let Some(width) = data.width {
        let width = not_negative("width", width)?;
        base.data.set(INTERACTION_WIDTH, SyncedValue::Float(width));
        base.dimensions.width = width;
    }
    if let Some(height) = data.height {
        let height = not_negative("height", height)?;
        base.data
            .set(INTERACTION_HEIGHT, SyncedValue::Float(height));
        base.dimensions.height = height;
    }
    if let Some(response) = data.response {
        base.data
            .set(INTERACTION_RESPONSE, SyncedValue::Boolean(response));
    }
    base.needs_sync = true;
    Ok(())
}

pub(crate) fn apply_display(base: &mut EntityBase, spec: &DisplaySpec) -> Result<(), ActionError> {
    if base.kind != spec.kind() {
        return Err(invalid(format!(
            "the entity is a {}, not a {}",
            base.kind.name(),
            spec.kind().name()
        )));
    }
    if is_display(base.kind) {
        set_props(base, &spec.props)?;
    }
    match &spec.body {
        DisplayBody::Text(text) => set_text(base, text),
        DisplayBody::Item(item) => set_item(base, item)?,
        DisplayBody::Block(block) => set_block(base, block)?,
        DisplayBody::Interaction(data) => set_interaction(base, data)?,
    }
    Ok(())
}

fn float(base: &EntityBase, index: u8) -> f32 {
    base.data.float(index)
}

fn vector(base: &EntityBase, index: u8) -> Option<[f32; 3]> {
    match base.data.get(index) {
        Some(SyncedValue::Vector3(value)) => Some(*value),
        _ => None,
    }
}

fn quaternion(base: &EntityBase, index: u8) -> Option<[f32; 4]> {
    match base.data.get(index) {
        Some(SyncedValue::Quaternion(value)) => Some(*value),
        _ => None,
    }
}

fn read_props(base: &EntityBase) -> DisplayProps {
    DisplayProps {
        transformation: Some(Transformation {
            translation: vector(base, DISPLAY_TRANSLATION),
            scale: vector(base, DISPLAY_SCALE),
            left_rotation: quaternion(base, DISPLAY_LEFT_ROTATION),
            right_rotation: quaternion(base, DISPLAY_RIGHT_ROTATION),
        }),
        billboard: Billboard::ALL
            .get(base.data.byte(DISPLAY_BILLBOARD).max(0) as usize)
            .copied(),
        brightness: unpack_brightness(base.data.int(DISPLAY_BRIGHTNESS)),
        view_range: Some(float(base, DISPLAY_VIEW_RANGE)),
        glow_color: Some(base.data.int(DISPLAY_GLOW_COLOR)),
        interpolation_delay: Some(base.data.int(DISPLAY_START_INTERPOLATION)),
        interpolation_duration: Some(base.data.int(DISPLAY_INTERPOLATION_DURATION)),
        teleport_duration: Some(base.data.int(DISPLAY_TELEPORT_DURATION)),
        shadow_radius: Some(float(base, DISPLAY_SHADOW_RADIUS)),
        shadow_strength: Some(float(base, DISPLAY_SHADOW_STRENGTH)),
        width: Some(float(base, DISPLAY_WIDTH)),
        height: Some(float(base, DISPLAY_HEIGHT)),
    }
}

fn read_text(base: &EntityBase) -> TextData {
    let flags = base.data.byte(TEXT_FLAGS);
    let alignment = if flags & ALIGN_LEFT != 0 {
        TextAlignment::Left
    } else if flags & ALIGN_RIGHT != 0 {
        TextAlignment::Right
    } else {
        TextAlignment::Center
    };
    let text = match base.data.get(TEXT_TEXT) {
        Some(SyncedValue::Component(text)) => Some(text.to_legacy()),
        _ => None,
    };
    TextData {
        text,
        line_width: Some(base.data.int(TEXT_LINE_WIDTH)),
        background: Some(base.data.int(TEXT_BACKGROUND)),
        opacity: Some(i32::from(base.data.byte(TEXT_OPACITY))),
        shadow: Some(flags & SHADOW != 0),
        see_through: Some(flags & SEE_THROUGH != 0),
        default_background: Some(flags & DEFAULT_BACKGROUND != 0),
        alignment: Some(alignment),
    }
}

pub(crate) fn read_display(entity: &Entity) -> Option<DisplaySpec> {
    let base = &entity.base;
    let none = DisplayProps::default();
    Some(match base.kind {
        Kind::TextDisplay => DisplaySpec {
            props: read_props(base),
            body: DisplayBody::Text(read_text(base)),
        },
        Kind::ItemDisplay => DisplaySpec {
            props: read_props(base),
            body: DisplayBody::Item(ItemDisplayData {
                item: match base.data.get(ITEM_STACK) {
                    Some(SyncedValue::Item(Some(stack))) => Some(spec_of(stack)),
                    _ => None,
                },
                context: DisplayContext::ALL
                    .get(base.data.byte(ITEM_CONTEXT).max(0) as usize)
                    .copied(),
            }),
        },
        Kind::BlockDisplay => DisplaySpec {
            props: read_props(base),
            body: DisplayBody::Block(BlockDisplayData {
                state: match base.data.get(BLOCK_STATE) {
                    Some(SyncedValue::BlockState(id)) => Some(data_of(*id)),
                    _ => None,
                },
            }),
        },
        Kind::Interaction => DisplaySpec {
            props: none,
            body: DisplayBody::Interaction(InteractionData {
                width: Some(float(base, INTERACTION_WIDTH)),
                height: Some(float(base, INTERACTION_HEIGHT)),
                response: Some(base.data.boolean(INTERACTION_RESPONSE)),
            }),
        },
        _ => return None,
    })
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn edit_display(
        &mut self,
        id: EntityId,
        spec: &DisplaySpec,
    ) -> Result<(), ActionError> {
        let entity =
            self.entities.store.get_mut(id).ok_or_else(|| {
                ActionError::new(ActionErrorKind::TargetGone, "the entity is gone")
            })?;
        let mut staged = entity.base.clone();
        apply_display(&mut staged, spec)?;
        entity.base = staged;
        entity.base.needs_sync = true;
        Ok(())
    }

    pub(crate) fn display_of(&self, id: EntityId) -> Result<DisplaySpec, ActionError> {
        let entity =
            self.entities.store.get(id).ok_or_else(|| {
                ActionError::new(ActionErrorKind::TargetGone, "the entity is gone")
            })?;
        read_display(entity).ok_or_else(|| invalid("the entity is not a display or an interaction"))
    }
}

#[cfg(test)]
mod tests {
    use rspigot_domain::entity::EntityBase;
    use rspigot_domain::entity::decor;

    use super::*;

    fn text_base() -> EntityBase {
        let mut base = EntityBase::new(
            rspigot_domain::EntityId(1),
            uuid::Uuid::from_u128(1),
            Kind::TextDisplay,
        );
        decor::define(&mut base);
        base
    }

    #[test]
    fn brightness_packs_and_unpacks() {
        assert_eq!(pack_brightness((3, 12)).unwrap(), 3 << 4 | 12 << 20);
        assert_eq!(unpack_brightness(3 << 4 | 12 << 20), Some((3, 12)));
        assert_eq!(unpack_brightness(-1), None);
        assert!(pack_brightness((16, 0)).is_err());
    }

    #[test]
    fn text_flags_change_one_bit_at_a_time() {
        let mut base = text_base();
        set_text(
            &mut base,
            &TextData {
                shadow: Some(true),
                alignment: Some(TextAlignment::Left),
                ..TextData::default()
            },
        );
        set_text(
            &mut base,
            &TextData {
                see_through: Some(true),
                ..TextData::default()
            },
        );
        let flags = base.data.byte(TEXT_FLAGS);
        assert_eq!(flags, SHADOW | SEE_THROUGH | ALIGN_LEFT);
        set_text(
            &mut base,
            &TextData {
                alignment: Some(TextAlignment::Right),
                shadow: Some(false),
                ..TextData::default()
            },
        );
        assert_eq!(base.data.byte(TEXT_FLAGS), SEE_THROUGH | ALIGN_RIGHT);
    }

    #[test]
    fn a_body_of_another_kind_is_refused() {
        let mut base = text_base();
        let spec = DisplaySpec {
            props: DisplayProps::default(),
            body: DisplayBody::Block(BlockDisplayData::default()),
        };
        assert!(apply_display(&mut base, &spec).is_err());
    }
}
