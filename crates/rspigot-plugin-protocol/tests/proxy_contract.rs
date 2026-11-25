use prost::Message;
use rspigot_plugin_protocol::proxy::v1::{
    ArgumentKind, CommandArgument, CommandSpec, Decision, FromPlugin, Hello, Hook, Redirect,
    Subscription, decision, from_plugin,
};

#[test]
fn a_proxy_hello_round_trips() {
    let hello = FromPlugin {
        body: Some(from_plugin::Body::Hello(Hello {
            protocol_version: rspigot_plugin_protocol::proxy::PROTOCOL_VERSION,
            name: "network".to_owned(),
            version: "1.0.0".to_owned(),
            token: "t".to_owned(),
            subscriptions: vec![Subscription {
                hook: Hook::PreConnect as i32,
                decide: true,
            }],
            commands: vec![CommandSpec {
                name: "hub".to_owned(),
                aliases: vec!["lobby".to_owned()],
                permission: None,
                arguments: vec![CommandArgument {
                    name: "server".to_owned(),
                    kind: ArgumentKind::Server as i32,
                    optional: true,
                    choices: Vec::new(),
                }],
            }],
        })),
        request: None,
    };
    let decoded = FromPlugin::decode(hello.encode_to_vec().as_slice()).unwrap();
    assert_eq!(decoded, hello);
}

#[test]
fn a_redirect_decision_names_the_target_server() {
    let decision = FromPlugin {
        body: Some(from_plugin::Body::Decision(Decision {
            event: 42,
            verdict: Some(decision::Verdict::Redirect(Redirect {
                server: "lobby-2".to_owned(),
                message: Some("§eLobby plein".to_owned()),
            })),
        })),
        request: None,
    };
    let decoded = FromPlugin::decode(decision.encode_to_vec().as_slice()).unwrap();
    let Some(from_plugin::Body::Decision(Decision {
        event,
        verdict: Some(decision::Verdict::Redirect(redirect)),
    })) = decoded.body
    else {
        panic!("not a redirect");
    };
    assert_eq!(event, 42);
    assert_eq!(redirect.server, "lobby-2");
}
