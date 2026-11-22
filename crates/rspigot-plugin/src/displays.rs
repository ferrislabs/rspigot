use rspigot_plugin_protocol::v1::{
    self, answer, display_spec, entity_order, from_plugin, object_answer, object_entity_order,
    object_question, question,
};

use crate::link::Link;
use crate::{BlockState, Error, ItemStack};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Billboard {
    Fixed,
    Vertical,
    Horizontal,
    Center,
}

impl Billboard {
    const fn to_wire(self) -> v1::Billboard {
        match self {
            Self::Fixed => v1::Billboard::Fixed,
            Self::Vertical => v1::Billboard::Vertical,
            Self::Horizontal => v1::Billboard::Horizontal,
            Self::Center => v1::Billboard::Center,
        }
    }

    fn from_wire(value: i32) -> Result<Self, Error> {
        match v1::Billboard::try_from(value) {
            Ok(v1::Billboard::Fixed) => Ok(Self::Fixed),
            Ok(v1::Billboard::Vertical) => Ok(Self::Vertical),
            Ok(v1::Billboard::Horizontal) => Ok(Self::Horizontal),
            Ok(v1::Billboard::Center) => Ok(Self::Center),
            Ok(v1::Billboard::Unspecified) | Err(_) => {
                Err(Error::Malformed(format!("an unknown billboard {value}")))
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextAlignment {
    Center,
    Left,
    Right,
}

impl TextAlignment {
    const fn to_wire(self) -> v1::TextAlignment {
        match self {
            Self::Center => v1::TextAlignment::Center,
            Self::Left => v1::TextAlignment::Left,
            Self::Right => v1::TextAlignment::Right,
        }
    }

    fn from_wire(value: i32) -> Result<Self, Error> {
        match v1::TextAlignment::try_from(value) {
            Ok(v1::TextAlignment::Center) => Ok(Self::Center),
            Ok(v1::TextAlignment::Left) => Ok(Self::Left),
            Ok(v1::TextAlignment::Right) => Ok(Self::Right),
            Ok(v1::TextAlignment::Unspecified) | Err(_) => {
                Err(Error::Malformed(format!("an unknown alignment {value}")))
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemDisplayContext {
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

impl ItemDisplayContext {
    const fn to_wire(self) -> v1::ItemDisplayContext {
        use v1::ItemDisplayContext as Wire;
        match self {
            Self::None => Wire::None,
            Self::ThirdPersonLeftHand => Wire::ThirdPersonLeftHand,
            Self::ThirdPersonRightHand => Wire::ThirdPersonRightHand,
            Self::FirstPersonLeftHand => Wire::FirstPersonLeftHand,
            Self::FirstPersonRightHand => Wire::FirstPersonRightHand,
            Self::Head => Wire::Head,
            Self::Gui => Wire::Gui,
            Self::Ground => Wire::Ground,
            Self::Fixed => Wire::Fixed,
        }
    }

    fn from_wire(value: i32) -> Result<Self, Error> {
        use v1::ItemDisplayContext as Wire;
        match Wire::try_from(value) {
            Ok(Wire::None) => Ok(Self::None),
            Ok(Wire::ThirdPersonLeftHand) => Ok(Self::ThirdPersonLeftHand),
            Ok(Wire::ThirdPersonRightHand) => Ok(Self::ThirdPersonRightHand),
            Ok(Wire::FirstPersonLeftHand) => Ok(Self::FirstPersonLeftHand),
            Ok(Wire::FirstPersonRightHand) => Ok(Self::FirstPersonRightHand),
            Ok(Wire::Head) => Ok(Self::Head),
            Ok(Wire::Gui) => Ok(Self::Gui),
            Ok(Wire::Ground) => Ok(Self::Ground),
            Ok(Wire::Fixed) => Ok(Self::Fixed),
            Ok(Wire::Unspecified) | Err(_) => Err(Error::Malformed(format!(
                "an unknown display context {value}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Transformation {
    pub translation: Option<[f32; 3]>,
    pub scale: Option<[f32; 3]>,
    pub left_rotation: Option<[f32; 4]>,
    pub right_rotation: Option<[f32; 4]>,
}

impl Transformation {
    pub fn translation(mut self, x: f32, y: f32, z: f32) -> Self {
        self.translation = Some([x, y, z]);
        self
    }

    pub fn scale(mut self, x: f32, y: f32, z: f32) -> Self {
        self.scale = Some([x, y, z]);
        self
    }

    pub fn left_rotation(mut self, x: f32, y: f32, z: f32, w: f32) -> Self {
        self.left_rotation = Some([x, y, z, w]);
        self
    }

    pub fn right_rotation(mut self, x: f32, y: f32, z: f32, w: f32) -> Self {
        self.right_rotation = Some([x, y, z, w]);
        self
    }

    fn to_wire(self) -> v1::DisplayTransformation {
        let vector = |value: [f32; 3]| v1::Vec3F {
            x: value[0],
            y: value[1],
            z: value[2],
        };
        let quaternion = |value: [f32; 4]| v1::Quat {
            x: value[0],
            y: value[1],
            z: value[2],
            w: value[3],
        };
        v1::DisplayTransformation {
            translation: self.translation.map(vector),
            scale: self.scale.map(vector),
            left_rotation: self.left_rotation.map(quaternion),
            right_rotation: self.right_rotation.map(quaternion),
        }
    }

    fn from_wire(value: v1::DisplayTransformation) -> Self {
        let vector = |value: v1::Vec3F| [value.x, value.y, value.z];
        let quaternion = |value: v1::Quat| [value.x, value.y, value.z, value.w];
        Self {
            translation: value.translation.map(vector),
            scale: value.scale.map(vector),
            left_rotation: value.left_rotation.map(quaternion),
            right_rotation: value.right_rotation.map(quaternion),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct DisplayProps {
    pub transformation: Option<Transformation>,
    pub billboard: Option<Billboard>,
    pub brightness: Option<(u8, u8)>,
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

impl DisplayProps {
    pub fn transformation(mut self, transformation: Transformation) -> Self {
        self.transformation = Some(transformation);
        self
    }

    pub fn billboard(mut self, billboard: Billboard) -> Self {
        self.billboard = Some(billboard);
        self
    }

    pub fn brightness(mut self, block: u8, sky: u8) -> Self {
        self.brightness = Some((block, sky));
        self
    }

    pub fn view_range(mut self, range: f32) -> Self {
        self.view_range = Some(range);
        self
    }

    pub fn glow_color(mut self, rgb: i32) -> Self {
        self.glow_color = Some(rgb);
        self
    }

    pub fn interpolation(mut self, delay: i32, duration: i32) -> Self {
        self.interpolation_delay = Some(delay);
        self.interpolation_duration = Some(duration);
        self
    }

    pub fn teleport_duration(mut self, ticks: i32) -> Self {
        self.teleport_duration = Some(ticks);
        self
    }

    pub fn shadow(mut self, radius: f32, strength: f32) -> Self {
        self.shadow_radius = Some(radius);
        self.shadow_strength = Some(strength);
        self
    }

    pub fn size(mut self, width: f32, height: f32) -> Self {
        self.width = Some(width);
        self.height = Some(height);
        self
    }

    fn to_wire(&self) -> v1::DisplayProps {
        v1::DisplayProps {
            transformation: self.transformation.map(Transformation::to_wire),
            billboard: self.billboard.map(|value| value.to_wire() as i32),
            brightness: self.brightness.map(|(block, sky)| v1::DisplayBrightness {
                block: i32::from(block),
                sky: i32::from(sky),
            }),
            view_range: self.view_range,
            glow_color: self.glow_color,
            interpolation_delay: self.interpolation_delay,
            interpolation_duration: self.interpolation_duration,
            teleport_duration: self.teleport_duration,
            shadow_radius: self.shadow_radius,
            shadow_strength: self.shadow_strength,
            width: self.width,
            height: self.height,
        }
    }

    fn from_wire(props: v1::DisplayProps) -> Result<Self, Error> {
        let light = |value: v1::DisplayBrightness| -> Result<(u8, u8), Error> {
            let part = |light: i32| {
                u8::try_from(light)
                    .map_err(|_| Error::Malformed(format!("a brightness of {light}")))
            };
            Ok((part(value.block)?, part(value.sky)?))
        };
        Ok(Self {
            transformation: props.transformation.map(Transformation::from_wire),
            billboard: props.billboard.map(Billboard::from_wire).transpose()?,
            brightness: props.brightness.map(light).transpose()?,
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
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct TextDisplay {
    pub props: DisplayProps,
    pub text: Option<String>,
    pub line_width: Option<i32>,
    pub background: Option<i32>,
    pub opacity: Option<i32>,
    pub shadow: Option<bool>,
    pub see_through: Option<bool>,
    pub default_background: Option<bool>,
    pub alignment: Option<TextAlignment>,
}

impl TextDisplay {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: Some(text.into()),
            ..Self::default()
        }
    }

    pub fn props(mut self, props: DisplayProps) -> Self {
        self.props = props;
        self
    }

    pub fn line_width(mut self, width: i32) -> Self {
        self.line_width = Some(width);
        self
    }

    pub fn background(mut self, argb: i32) -> Self {
        self.background = Some(argb);
        self
    }

    pub fn opacity(mut self, opacity: i32) -> Self {
        self.opacity = Some(opacity);
        self
    }

    pub fn shadow(mut self, shadow: bool) -> Self {
        self.shadow = Some(shadow);
        self
    }

    pub fn see_through(mut self, see_through: bool) -> Self {
        self.see_through = Some(see_through);
        self
    }

    pub fn default_background(mut self, default_background: bool) -> Self {
        self.default_background = Some(default_background);
        self
    }

    pub fn alignment(mut self, alignment: TextAlignment) -> Self {
        self.alignment = Some(alignment);
        self
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ItemDisplay {
    pub props: DisplayProps,
    pub item: Option<ItemStack>,
    pub context: Option<ItemDisplayContext>,
}

impl ItemDisplay {
    pub fn new(item: ItemStack) -> Self {
        Self {
            item: Some(item),
            ..Self::default()
        }
    }

    pub fn props(mut self, props: DisplayProps) -> Self {
        self.props = props;
        self
    }

    pub fn context(mut self, context: ItemDisplayContext) -> Self {
        self.context = Some(context);
        self
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct BlockDisplay {
    pub props: DisplayProps,
    pub state: Option<BlockState>,
}

impl BlockDisplay {
    pub fn new(state: BlockState) -> Self {
        Self {
            state: Some(state),
            ..Self::default()
        }
    }

    pub fn props(mut self, props: DisplayProps) -> Self {
        self.props = props;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct InteractionBox {
    pub width: Option<f32>,
    pub height: Option<f32>,
    pub response: Option<bool>,
}

impl InteractionBox {
    pub fn new(width: f32, height: f32) -> Self {
        Self {
            width: Some(width),
            height: Some(height),
            response: None,
        }
    }

    pub fn response(mut self, response: bool) -> Self {
        self.response = Some(response);
        self
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum DisplaySpec {
    Text(TextDisplay),
    Item(ItemDisplay),
    Block(BlockDisplay),
    Interaction(InteractionBox),
}

impl From<TextDisplay> for DisplaySpec {
    fn from(value: TextDisplay) -> Self {
        Self::Text(value)
    }
}

impl From<ItemDisplay> for DisplaySpec {
    fn from(value: ItemDisplay) -> Self {
        Self::Item(value)
    }
}

impl From<BlockDisplay> for DisplaySpec {
    fn from(value: BlockDisplay) -> Self {
        Self::Block(value)
    }
}

impl From<InteractionBox> for DisplaySpec {
    fn from(value: InteractionBox) -> Self {
        Self::Interaction(value)
    }
}

impl DisplaySpec {
    pub(crate) fn to_wire(&self) -> v1::DisplaySpec {
        use display_spec::Body;
        let (props, body) = match self {
            Self::Text(text) => (
                text.props.to_wire(),
                Body::Text(v1::TextDisplayData {
                    text: text.text.clone(),
                    line_width: text.line_width,
                    background: text.background,
                    opacity: text.opacity,
                    shadow: text.shadow,
                    see_through: text.see_through,
                    default_background: text.default_background,
                    alignment: text.alignment.map(|value| value.to_wire() as i32),
                }),
            ),
            Self::Item(item) => (
                item.props.to_wire(),
                Body::Item(v1::ItemDisplayData {
                    item: item.item.as_ref().map(ItemStack::to_wire),
                    context: item.context.map(|value| value.to_wire() as i32),
                }),
            ),
            Self::Block(block) => (
                block.props.to_wire(),
                Body::Block(v1::BlockDisplayData {
                    state: block.state.as_ref().map(BlockState::to_wire),
                }),
            ),
            Self::Interaction(interaction) => (
                v1::DisplayProps::default(),
                Body::Interaction(v1::InteractionData {
                    width: interaction.width,
                    height: interaction.height,
                    response: interaction.response,
                }),
            ),
        };
        v1::DisplaySpec {
            props: Some(props),
            body: Some(body),
        }
    }

    pub(crate) fn from_wire(spec: v1::DisplaySpec) -> Result<Self, Error> {
        use display_spec::Body;
        let props = DisplayProps::from_wire(spec.props.unwrap_or_default())?;
        let body = spec
            .body
            .ok_or_else(|| Error::Malformed("a display without its data".to_owned()))?;
        Ok(match body {
            Body::Text(text) => Self::Text(TextDisplay {
                props,
                text: text.text,
                line_width: text.line_width,
                background: text.background,
                opacity: text.opacity,
                shadow: text.shadow,
                see_through: text.see_through,
                default_background: text.default_background,
                alignment: text.alignment.map(TextAlignment::from_wire).transpose()?,
            }),
            Body::Item(item) => Self::Item(ItemDisplay {
                props,
                item: item.item.map(ItemStack::from_wire).transpose()?,
                context: item
                    .context
                    .map(ItemDisplayContext::from_wire)
                    .transpose()?,
            }),
            Body::Block(block) => Self::Block(BlockDisplay {
                props,
                state: block.state.map(BlockState::from_wire).transpose()?,
            }),
            Body::Interaction(interaction) => Self::Interaction(InteractionBox {
                width: interaction.width,
                height: interaction.height,
                response: interaction.response,
            }),
        })
    }
}

#[derive(Debug, Clone)]
pub struct DisplayHandle {
    entity: i32,
    link: Link,
}

impl DisplayHandle {
    pub(crate) fn new(entity: i32, link: Link) -> Self {
        Self { entity, link }
    }

    pub fn edit(&self, spec: impl Into<DisplaySpec>) {
        self.link.order(from_plugin::Body::Entity(v1::EntityOrder {
            entity: self.entity,
            order: Some(entity_order::Order::Object(v1::ObjectEntityOrder {
                order: Some(object_entity_order::Order::EditDisplay(
                    spec.into().to_wire(),
                )),
            })),
        }));
    }

    pub async fn get(&self) -> Result<DisplaySpec, Error> {
        let spec = self
            .link
            .ask(
                question::Question::Object(v1::ObjectQuestion {
                    query: Some(object_question::Query::Display(self.entity)),
                }),
                "a display",
                |body| match body {
                    answer::Body::Object(v1::ObjectAnswer {
                        answer: Some(object_answer::Answer::Display(spec)),
                    }) => Some(spec),
                    _ => None,
                },
            )
            .await?;
        DisplaySpec::from_wire(spec)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_text_display_survives_the_wire() {
        let spec = DisplaySpec::from(
            TextDisplay::new("hello")
                .props(
                    DisplayProps::default()
                        .billboard(Billboard::Center)
                        .brightness(3, 12)
                        .transformation(Transformation::default().scale(2.0, 2.0, 2.0)),
                )
                .alignment(TextAlignment::Left)
                .shadow(true),
        );
        assert_eq!(DisplaySpec::from_wire(spec.to_wire()).unwrap(), spec);
    }

    #[test]
    fn an_interaction_box_survives_the_wire() {
        let spec = DisplaySpec::from(InteractionBox::new(2.0, 3.0).response(true));
        assert_eq!(DisplaySpec::from_wire(spec.to_wire()).unwrap(), spec);
        assert!(DisplaySpec::from_wire(v1::DisplaySpec::default()).is_err());
    }
}
