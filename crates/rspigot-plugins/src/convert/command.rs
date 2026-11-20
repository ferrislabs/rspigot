use rspigot_core::plugins::{
    ArgumentKind, ArgumentValue, CommandArgument, CommandSpec, PlayerSnapshot,
};
use rspigot_plugin_protocol::v1::{self, argument_value};

use super::{Malformed, wire_player};

pub(super) fn spec(command: v1::CommandSpec) -> Result<CommandSpec, Malformed> {
    let arguments = command
        .arguments
        .into_iter()
        .map(|argument| {
            Ok(CommandArgument {
                kind: kind(argument.kind)?,
                name: argument.name,
                optional: argument.optional,
                suggested: argument.suggested,
                choices: argument.choices,
            })
        })
        .collect::<Result<_, Malformed>>()?;
    Ok(CommandSpec {
        id: command.id,
        name: command.name,
        aliases: command.aliases,
        permission: command.permission,
        arguments,
    })
}

fn kind(value: i32) -> Result<ArgumentKind, Malformed> {
    match v1::ArgumentKind::try_from(value) {
        Ok(v1::ArgumentKind::Player) => Ok(ArgumentKind::Player),
        Ok(v1::ArgumentKind::Item) => Ok(ArgumentKind::Item),
        Ok(v1::ArgumentKind::Integer) => Ok(ArgumentKind::Integer),
        Ok(v1::ArgumentKind::Text) => Ok(ArgumentKind::Text),
        Ok(v1::ArgumentKind::Boolean) => Ok(ArgumentKind::Boolean),
        Ok(v1::ArgumentKind::Literal) => Ok(ArgumentKind::Literal),
        Ok(v1::ArgumentKind::GreedyText) => Ok(ArgumentKind::GreedyText),
        Ok(v1::ArgumentKind::Double) => Ok(ArgumentKind::Double),
        Ok(v1::ArgumentKind::Float) => Ok(ArgumentKind::Float),
        Ok(v1::ArgumentKind::Entities) => Ok(ArgumentKind::Entities),
        Ok(v1::ArgumentKind::Location) => Ok(ArgumentKind::Location),
        Ok(v1::ArgumentKind::Choice) => Ok(ArgumentKind::Choice),
        Ok(v1::ArgumentKind::Unspecified) | Err(_) => Err(Malformed::Enum {
            field: "argument kind",
            value,
        }),
    }
}

pub(super) fn invocation(
    command: u32,
    sender: PlayerSnapshot,
    arguments: Vec<ArgumentValue>,
) -> v1::Invocation {
    v1::Invocation {
        command,
        sender: Some(wire_player(sender)),
        arguments: arguments.into_iter().map(value).collect(),
    }
}

fn value(argument: ArgumentValue) -> v1::ArgumentValue {
    v1::ArgumentValue {
        value: Some(match argument {
            ArgumentValue::Player(player) => argument_value::Value::Player(wire_player(player)),
            ArgumentValue::Item(key) => argument_value::Value::Item(key),
            ArgumentValue::Integer(number) => argument_value::Value::Integer(number),
            ArgumentValue::Text(text) => argument_value::Value::Text(text),
            ArgumentValue::Flag(flag) => argument_value::Value::Flag(flag),
            ArgumentValue::Decimal(number) => argument_value::Value::Decimal(number),
            ArgumentValue::Entities(views) => argument_value::Value::Entities(v1::EntityList {
                entities: views.into_iter().map(super::entity::wire_view).collect(),
            }),
            ArgumentValue::Location(spot) => {
                argument_value::Value::Location(super::shared::wire_vec(spot))
            }
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_declared_command_keeps_its_arguments() {
        let wire = v1::CommandSpec {
            id: 3,
            name: "give".to_owned(),
            permission: Some("kit.use".to_owned()),
            aliases: vec!["g".to_owned()],
            arguments: vec![v1::CommandArgument {
                name: "count".to_owned(),
                kind: v1::ArgumentKind::Integer as i32,
                optional: true,
                suggested: false,
                choices: Vec::new(),
            }],
        };
        let read = spec(wire).expect("a valid command");
        assert_eq!(read.id, 3);
        assert_eq!(read.permission.as_deref(), Some("kit.use"));
        assert_eq!(read.arguments[0].kind, ArgumentKind::Integer);
        assert!(read.arguments[0].optional);
        assert_eq!(read.aliases, ["g"]);
    }

    #[test]
    fn every_argument_kind_and_its_choices_cross_the_wire() {
        let wire = v1::CommandSpec {
            id: 1,
            name: "probe".to_owned(),
            permission: None,
            aliases: Vec::new(),
            arguments: [
                (v1::ArgumentKind::Literal, ArgumentKind::Literal),
                (v1::ArgumentKind::GreedyText, ArgumentKind::GreedyText),
                (v1::ArgumentKind::Double, ArgumentKind::Double),
                (v1::ArgumentKind::Float, ArgumentKind::Float),
                (v1::ArgumentKind::Entities, ArgumentKind::Entities),
                (v1::ArgumentKind::Location, ArgumentKind::Location),
                (v1::ArgumentKind::Choice, ArgumentKind::Choice),
            ]
            .iter()
            .map(|(kind, _)| v1::CommandArgument {
                name: "x".to_owned(),
                kind: *kind as i32,
                optional: false,
                suggested: false,
                choices: vec!["a".to_owned(), "b".to_owned()],
            })
            .collect(),
        };
        let read = spec(wire).expect("valid");
        let kinds: Vec<ArgumentKind> = read.arguments.iter().map(|a| a.kind).collect();
        assert_eq!(
            kinds,
            [
                ArgumentKind::Literal,
                ArgumentKind::GreedyText,
                ArgumentKind::Double,
                ArgumentKind::Float,
                ArgumentKind::Entities,
                ArgumentKind::Location,
                ArgumentKind::Choice
            ]
        );
        assert_eq!(read.arguments[6].choices, ["a", "b"]);
    }

    #[test]
    fn a_decimal_and_a_location_are_sent_back_as_values() {
        assert_eq!(
            value(ArgumentValue::Decimal(1.5)).value,
            Some(argument_value::Value::Decimal(1.5))
        );
        assert_eq!(
            value(ArgumentValue::Location(rspigot_domain::Vec3::new(
                1.0, 2.0, 3.0
            )))
            .value,
            Some(argument_value::Value::Location(v1::Vec3 {
                x: 1.0,
                y: 2.0,
                z: 3.0
            }))
        );
    }

    #[test]
    fn an_unknown_argument_kind_is_malformed() {
        let wire = v1::CommandSpec {
            id: 1,
            name: "x".to_owned(),
            permission: None,
            aliases: Vec::new(),
            arguments: vec![v1::CommandArgument {
                name: "a".to_owned(),
                kind: 99,
                optional: false,
                suggested: false,
                choices: Vec::new(),
            }],
        };
        assert!(spec(wire).is_err());
    }
}
