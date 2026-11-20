use std::collections::VecDeque;

use rspigot_plugin_protocol::v1::{self, argument_value};

use crate::link::Link;
use crate::{CommandSender, Entity, Error, Material, Player, Position, ServerCommandHandle};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArgumentKind {
    Player,
    Item,
    Integer,
    Text,
    Boolean,
    Literal,
    GreedyText,
    Double,
    Float,
    Entities,
    Location,
    Choice,
}

impl ArgumentKind {
    const fn to_wire(self) -> v1::ArgumentKind {
        match self {
            Self::Player => v1::ArgumentKind::Player,
            Self::Item => v1::ArgumentKind::Item,
            Self::Integer => v1::ArgumentKind::Integer,
            Self::Text => v1::ArgumentKind::Text,
            Self::Boolean => v1::ArgumentKind::Boolean,
            Self::Literal => v1::ArgumentKind::Literal,
            Self::GreedyText => v1::ArgumentKind::GreedyText,
            Self::Double => v1::ArgumentKind::Double,
            Self::Float => v1::ArgumentKind::Float,
            Self::Entities => v1::ArgumentKind::Entities,
            Self::Location => v1::ArgumentKind::Location,
            Self::Choice => v1::ArgumentKind::Choice,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandArgument {
    pub name: &'static str,
    pub kind: ArgumentKind,
    pub optional: bool,
    pub suggested: bool,
    pub choices: &'static [&'static str],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandSpec {
    pub id: u32,
    pub name: &'static str,
    pub aliases: &'static [&'static str],
    pub permission: Option<&'static str>,
    pub arguments: Vec<CommandArgument>,
}

impl CommandSpec {
    pub(crate) fn to_wire(&self) -> v1::CommandSpec {
        v1::CommandSpec {
            id: self.id,
            name: self.name.to_owned(),
            aliases: self
                .aliases
                .iter()
                .map(|alias| (*alias).to_owned())
                .collect(),
            permission: self.permission.map(str::to_owned),
            arguments: self
                .arguments
                .iter()
                .map(|argument| v1::CommandArgument {
                    name: argument.name.to_owned(),
                    kind: argument.kind.to_wire() as i32,
                    optional: argument.optional,
                    suggested: argument.suggested,
                    choices: argument
                        .choices
                        .iter()
                        .map(|choice| (*choice).to_owned())
                        .collect(),
                })
                .collect(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Context {
    sender: Player,
}

impl Context {
    pub const fn sender(&self) -> &Player {
        &self.sender
    }

    pub fn reply(&self, text: impl Into<String>) {
        self.sender.message.send(text);
    }
}

#[derive(Debug)]
pub struct Arguments {
    values: VecDeque<v1::ArgumentValue>,
    link: Link,
}

impl Arguments {
    fn next(&mut self) -> Option<argument_value::Value> {
        self.values.pop_front().and_then(|value| value.value)
    }

    pub fn read<T: Argument>(&mut self, context: &Context) -> Option<T> {
        match T::read(self) {
            Ok(value) => Some(value),
            Err(error) => {
                tracing::warn!("A command argument could not be read: {error}");
                context.reply(format!("Invalid argument: {error}"));
                None
            }
        }
    }
}

pub trait Argument: Sized {
    const KIND: ArgumentKind;
    const OPTIONAL: bool = false;
    const WIDTH: usize = 1;
    const CHOICES: &'static [&'static str] = &[];

    fn read(arguments: &mut Arguments) -> Result<Self, Error>;
}

fn missing<T>() -> Result<T, Error> {
    Err(Error::Malformed(
        "an argument of another kind than declared".to_owned(),
    ))
}

impl Argument for Player {
    const KIND: ArgumentKind = ArgumentKind::Player;

    fn read(arguments: &mut Arguments) -> Result<Self, Error> {
        match arguments.next() {
            Some(argument_value::Value::Player(snapshot)) => {
                Self::from_snapshot(snapshot, &arguments.link)
            }
            _ => missing(),
        }
    }
}

impl Argument for Material {
    const KIND: ArgumentKind = ArgumentKind::Item;

    fn read(arguments: &mut Arguments) -> Result<Self, Error> {
        match arguments.next() {
            Some(argument_value::Value::Item(key)) => Self::from_key(&key)
                .ok_or_else(|| Error::Malformed(format!("`{key}` is not an item"))),
            _ => missing(),
        }
    }
}

impl Argument for String {
    const KIND: ArgumentKind = ArgumentKind::Text;

    fn read(arguments: &mut Arguments) -> Result<Self, Error> {
        match arguments.next() {
            Some(argument_value::Value::Text(text)) => Ok(text),
            _ => missing(),
        }
    }
}

impl Argument for bool {
    const KIND: ArgumentKind = ArgumentKind::Boolean;

    fn read(arguments: &mut Arguments) -> Result<Self, Error> {
        match arguments.next() {
            Some(argument_value::Value::Flag(flag)) => Ok(flag),
            _ => missing(),
        }
    }
}

macro_rules! integer_argument {
    ($($number:ty),*) => {$(
        impl Argument for $number {
            const KIND: ArgumentKind = ArgumentKind::Integer;

            fn read(arguments: &mut Arguments) -> Result<Self, Error> {
                match arguments.next() {
                    Some(argument_value::Value::Integer(number)) => {
                        Self::try_from(number).map_err(|_| {
                            Error::Malformed(format!(
                                "{number} is out of range for {}",
                                stringify!($number)
                            ))
                        })
                    }
                    _ => missing(),
                }
            }
        }
    )*};
}

integer_argument!(u8, u16, u32, u64, usize, i8, i16, i32, i64);

impl Argument for f64 {
    const KIND: ArgumentKind = ArgumentKind::Double;

    fn read(arguments: &mut Arguments) -> Result<Self, Error> {
        match arguments.next() {
            Some(argument_value::Value::Decimal(number)) => Ok(number),
            _ => missing(),
        }
    }
}

impl Argument for f32 {
    const KIND: ArgumentKind = ArgumentKind::Float;

    fn read(arguments: &mut Arguments) -> Result<Self, Error> {
        match arguments.next() {
            Some(argument_value::Value::Decimal(number)) => Ok(number as Self),
            _ => missing(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GreedyText(pub String);

impl Argument for GreedyText {
    const KIND: ArgumentKind = ArgumentKind::GreedyText;

    fn read(arguments: &mut Arguments) -> Result<Self, Error> {
        match arguments.next() {
            Some(argument_value::Value::Text(text)) => Ok(Self(text)),
            _ => missing(),
        }
    }
}

impl Argument for Vec<Entity> {
    const KIND: ArgumentKind = ArgumentKind::Entities;

    fn read(arguments: &mut Arguments) -> Result<Self, Error> {
        match arguments.next() {
            Some(argument_value::Value::Entities(list)) => list
                .entities
                .into_iter()
                .map(|entity| Entity::from_wire(entity, &arguments.link))
                .collect(),
            _ => missing(),
        }
    }
}

impl Argument for Position {
    const KIND: ArgumentKind = ArgumentKind::Location;
    const WIDTH: usize = 3;

    fn read(arguments: &mut Arguments) -> Result<Self, Error> {
        match arguments.next() {
            Some(argument_value::Value::Location(spot)) => Self::from_wire(Some(spot), "location"),
            _ => missing(),
        }
    }
}

pub trait ChoiceArgument: Sized {
    const CHOICES: &'static [&'static str];

    fn from_choice(choice: &str) -> Option<Self>;
}

impl<T: ChoiceArgument> Argument for T {
    const KIND: ArgumentKind = ArgumentKind::Choice;
    const CHOICES: &'static [&'static str] = <T as ChoiceArgument>::CHOICES;

    fn read(arguments: &mut Arguments) -> Result<Self, Error> {
        match arguments.next() {
            Some(argument_value::Value::Text(text)) => T::from_choice(&text)
                .ok_or_else(|| Error::Malformed(format!("`{text}` is not one of the choices"))),
            _ => missing(),
        }
    }
}

impl<T: Argument> Argument for Option<T> {
    const KIND: ArgumentKind = T::KIND;
    const OPTIONAL: bool = true;
    const WIDTH: usize = T::WIDTH;
    const CHOICES: &'static [&'static str] = T::CHOICES;

    fn read(arguments: &mut Arguments) -> Result<Self, Error> {
        if arguments.values.is_empty() {
            Ok(None)
        } else {
            T::read(arguments).map(Some)
        }
    }
}

impl ServerCommandHandle {
    pub fn register(&self, spec: &CommandSpec) {
        self.link
            .order(rspigot_plugin_protocol::v1::from_plugin::Body::RegisterCommand(spec.to_wire()));
    }

    pub fn unregister(&self, name: impl Into<String>) {
        self.link
            .order(rspigot_plugin_protocol::v1::from_plugin::Body::UnregisterCommand(name.into()));
    }
}

#[derive(Debug)]
pub struct Invocation {
    command: u32,
    sender: Option<v1::PlayerSnapshot>,
    arguments: Vec<v1::ArgumentValue>,
    link: Link,
}

impl Invocation {
    pub(crate) fn from_wire(invocation: v1::Invocation, link: &Link) -> Self {
        Self {
            command: invocation.command,
            sender: invocation.sender,
            arguments: invocation.arguments,
            link: link.clone(),
        }
    }

    pub const fn command(&self) -> u32 {
        self.command
    }

    pub fn open(self) -> Option<(Context, Arguments)> {
        let sender = match Player::from_wire(self.sender, &self.link) {
            Ok(sender) => sender,
            Err(error) => {
                tracing::warn!("Skipped command {}: {error}", self.command);
                return None;
            }
        };
        Some((
            Context { sender },
            Arguments {
                values: self.arguments.into(),
                link: self.link,
            },
        ))
    }
}

#[derive(Debug)]
pub struct Typing {
    command: String,
    path: Vec<String>,
    position: usize,
    text: String,
    player: Player,
}

impl Typing {
    pub fn of(buffer: &str, sender: &CommandSender) -> Option<Self> {
        let CommandSender::Player(player) = sender else {
            return None;
        };
        let mut words: Vec<&str> = buffer.trim_start_matches('/').split(' ').collect();
        let text = words.pop()?.to_owned();
        let command = words.first()?.to_ascii_lowercase();
        let path = words[1..]
            .iter()
            .map(|word| word.to_ascii_lowercase())
            .collect();
        Some(Self {
            command,
            path,
            position: words.len() - 1,
            text,
            player: player.clone(),
        })
    }

    pub fn command(&self) -> &str {
        &self.command
    }

    pub const fn position(&self) -> usize {
        self.position
    }

    pub fn path(&self) -> &[String] {
        &self.path
    }

    pub fn is_for(
        &self,
        labels: &[&str],
        literals: &[&str],
        start: usize,
        width: usize,
        open: bool,
    ) -> bool {
        labels.iter().any(|label| *label == self.command)
            && self.path.len() >= literals.len()
            && literals
                .iter()
                .zip(&self.path)
                .all(|(literal, typed)| typed == literal)
            && self.position >= start
            && (open || self.position < start + width)
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub const fn player(&self) -> &Player {
        &self.player
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arguments(values: Vec<argument_value::Value>) -> Arguments {
        Arguments {
            values: values
                .into_iter()
                .map(|value| v1::ArgumentValue { value: Some(value) })
                .collect(),
            link: Link::default(),
        }
    }

    #[test]
    fn an_integer_argument_keeps_its_range() {
        let mut given = arguments(vec![
            argument_value::Value::Integer(3),
            argument_value::Value::Integer(-1),
        ]);
        assert_eq!(u32::read(&mut given).expect("in range"), 3);
        assert!(u32::read(&mut given).is_err());
    }

    #[test]
    fn an_optional_argument_is_none_when_absent_and_some_when_given() {
        let mut absent = arguments(Vec::new());
        assert_eq!(Option::<u32>::read(&mut absent).expect("absent"), None);
        let mut given = arguments(vec![argument_value::Value::Integer(7)]);
        assert_eq!(Option::<u32>::read(&mut given).expect("given"), Some(7));
        const { assert!(<Option<u32> as Argument>::OPTIONAL) };
        assert_eq!(<Option<u32> as Argument>::KIND, ArgumentKind::Integer);
    }

    #[test]
    fn an_item_argument_names_a_material() {
        let mut given = arguments(vec![argument_value::Value::Item(
            "minecraft:diamond".to_owned(),
        )]);
        assert_eq!(
            Material::read(&mut given).expect("an item"),
            Material::Diamond
        );
        let mut unknown = arguments(vec![argument_value::Value::Item(
            "minecraft:nope".to_owned(),
        )]);
        assert!(Material::read(&mut unknown).is_err());
    }
}
