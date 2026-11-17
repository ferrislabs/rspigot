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
            })
        })
        .collect::<Result<_, Malformed>>()?;
    Ok(CommandSpec {
        id: command.id,
        name: command.name,
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
            arguments: vec![v1::CommandArgument {
                name: "count".to_owned(),
                kind: v1::ArgumentKind::Integer as i32,
                optional: true,
                suggested: false,
            }],
        };
        let read = spec(wire).expect("a valid command");
        assert_eq!(read.id, 3);
        assert_eq!(read.permission.as_deref(), Some("kit.use"));
        assert_eq!(read.arguments[0].kind, ArgumentKind::Integer);
        assert!(read.arguments[0].optional);
    }

    #[test]
    fn an_unknown_argument_kind_is_malformed() {
        let wire = v1::CommandSpec {
            id: 1,
            name: "x".to_owned(),
            permission: None,
            arguments: vec![v1::CommandArgument {
                name: "a".to_owned(),
                kind: 99,
                optional: false,
                suggested: false,
            }],
        };
        assert!(spec(wire).is_err());
    }
}
