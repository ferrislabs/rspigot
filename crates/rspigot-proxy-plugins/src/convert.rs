use rspigot_plugin_protocol::proxy::v1;
use rspigot_proxy_core::{ArgumentKind, CommandArgument, CommandSpec, PlayerView, Server, Verdict};
use rspigot_proxy_network::text::{Click, RichPart};

pub fn player(view: &PlayerView) -> v1::Player {
    v1::Player {
        id: view.id.to_string(),
        name: view.name.clone(),
        address: view.address.to_string(),
        server: view.server.clone(),
        properties: Vec::new(),
    }
}

pub fn server(server: &Server, players: usize) -> v1::Server {
    v1::Server {
        name: server.name.clone(),
        address: server.address.clone(),
        players: u32::try_from(players).unwrap_or(u32::MAX),
    }
}

pub fn verdict(decision: v1::decision::Verdict) -> Verdict {
    match decision {
        v1::decision::Verdict::Allow(_) => Verdict::Allow,
        v1::decision::Verdict::Deny(deny) => Verdict::Deny(deny.message),
        v1::decision::Verdict::Redirect(redirect) => Verdict::Redirect {
            server: redirect.server,
            message: redirect.message,
        },
    }
}

pub fn command(spec: v1::CommandSpec) -> CommandSpec {
    CommandSpec {
        name: spec.name,
        aliases: spec.aliases,
        permission: spec.permission,
        arguments: spec
            .arguments
            .into_iter()
            .map(|argument| CommandArgument {
                kind: match v1::ArgumentKind::try_from(argument.kind) {
                    Ok(v1::ArgumentKind::GreedyText) => ArgumentKind::GreedyText,
                    Ok(v1::ArgumentKind::Player) => ArgumentKind::Player,
                    Ok(v1::ArgumentKind::Server) => ArgumentKind::Server,
                    Ok(v1::ArgumentKind::Integer) => ArgumentKind::Integer,
                    Ok(v1::ArgumentKind::Choice) => ArgumentKind::Choice,
                    _ => ArgumentKind::Word,
                },
                name: argument.name,
                optional: argument.optional,
                choices: argument.choices,
            })
            .collect(),
    }
}

pub fn rich(message: v1::RichMessage) -> Vec<RichPart> {
    message
        .parts
        .into_iter()
        .map(|part| RichPart {
            text: part.text,
            hover: part.hover,
            click: part
                .click
                .and_then(|click| click.action)
                .map(|action| match action {
                    v1::rich_click::Action::RunCommand(command) => Click::RunCommand(command),
                    v1::rich_click::Action::SuggestCommand(command) => {
                        Click::SuggestCommand(command)
                    }
                    v1::rich_click::Action::OpenUrl(url) => Click::OpenUrl(url),
                    v1::rich_click::Action::CopyToClipboard(value) => Click::CopyToClipboard(value),
                }),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_commands_keep_their_arguments() {
        let spec = command(v1::CommandSpec {
            name: "server".to_owned(),
            aliases: vec!["sv".to_owned()],
            permission: Some("proxy.command.server".to_owned()),
            arguments: vec![v1::CommandArgument {
                name: "target".to_owned(),
                kind: v1::ArgumentKind::Server as i32,
                optional: true,
                choices: Vec::new(),
            }],
        });
        assert_eq!(spec.aliases, ["sv"]);
        assert_eq!(spec.arguments[0].kind, ArgumentKind::Server);
        assert!(spec.arguments[0].optional);
    }

    #[test]
    fn decisions_become_verdicts() {
        assert_eq!(
            verdict(v1::decision::Verdict::Redirect(v1::Redirect {
                server: "lobby".to_owned(),
                message: None,
            })),
            Verdict::Redirect {
                server: "lobby".to_owned(),
                message: None,
            }
        );
        assert_eq!(
            verdict(v1::decision::Verdict::Allow(v1::Allow {})),
            Verdict::Allow
        );
    }
}
