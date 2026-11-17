mod fake_host;

use fake_host::*;
use rspigot_plugin::prelude::*;
use rspigot_plugin_protocol::v1::{
    self, argument_value, command_sender, event, from_plugin, patch, to_plugin,
};
use tokio::sync::mpsc;

type Seen = (String, Material, Option<u32>);

#[derive(Plugin)]
#[plugin(name = "shop", version = "1.0.0")]
struct Shop {
    seen: mpsc::UnboundedSender<Seen>,
}

#[listeners]
impl Shop {
    #[command("kit", permission = "kit.use")]
    async fn kit(&self, ctx: Context, target: Player, item: Material, count: Option<u32>) {
        let _ = self.seen.send((target.name, item, count));
        ctx.reply("done");
    }

    #[command("warp")]
    async fn warp(&self, ctx: Context, #[suggest(Self::places)] place: String) {
        let _ = (ctx, place);
    }
}

impl Shop {
    async fn places(&self, player: &Player, typed: &str) -> Vec<String> {
        vec![format!("{}-{typed}", player.name)]
    }
}

fn argument(value: argument_value::Value) -> v1::ArgumentValue {
    v1::ArgumentValue { value: Some(value) }
}

fn invocation(command: u32, arguments: Vec<v1::ArgumentValue>) -> to_plugin::Body {
    to_plugin::Body::Invocation(v1::Invocation {
        command,
        sender: Some(steve()),
        arguments,
    })
}

async fn started() -> (Session, mpsc::UnboundedReceiver<Seen>, v1::Hello) {
    let mut host = Host::start().await;
    let (seen, received) = mpsc::unbounded_channel();
    tokio::spawn(run_with(Shop { seen }, host.url.clone(), None));
    let knock = host.knock().await;
    let hello = knock.hello.clone();
    (knock.accept(), received, hello)
}

#[tokio::test]
async fn the_hello_carries_the_declared_commands_and_the_suggestion_handler() {
    let (_session, _seen, hello) = started().await;
    let names: Vec<&str> = hello
        .commands
        .iter()
        .map(|command| command.name.as_str())
        .collect();
    assert_eq!(names, ["kit", "warp"]);
    let kit = &hello.commands[0];
    assert_eq!(kit.permission.as_deref(), Some("kit.use"));
    assert_eq!(kit.arguments[0].kind, v1::ArgumentKind::Player as i32);
    assert!(kit.arguments[2].optional);
    assert!(hello.commands[1].arguments[0].suggested);
    assert!(hello.handlers.iter().any(|handler| {
        handler.event == v1::EventKind::TabComplete as i32
            && handler.access == v1::Access::Decide as i32
    }));
}

#[tokio::test]
async fn an_invocation_reaches_the_command_with_typed_arguments() {
    let (session, mut seen, _) = started().await;
    session.say(invocation(
        1,
        vec![
            argument(argument_value::Value::Player(alex())),
            argument(argument_value::Value::Item("minecraft:diamond".to_owned())),
            argument(argument_value::Value::Integer(3)),
        ],
    ));
    let (target, item, count) = within(seen.recv()).await.expect("the command ran");
    assert_eq!(
        (target.as_str(), item, count),
        ("Alex", Material::Diamond, Some(3))
    );

    session.say(invocation(
        1,
        vec![
            argument(argument_value::Value::Player(alex())),
            argument(argument_value::Value::Item("minecraft:stone".to_owned())),
        ],
    ));
    let (_, item, count) = within(seen.recv()).await.expect("the command ran");
    assert_eq!((item, count), (Material::Stone, None));
}

#[tokio::test]
async fn a_dynamic_suggestion_answers_the_tab_complete_with_the_typed_text() {
    let (session, _seen, _) = started().await;
    let mut session = session;
    session.say(event(
        7,
        &[u32::MAX],
        v1::Phase::Decide,
        false,
        event::Payload::TabComplete(v1::TabComplete {
            buffer: "/warp ho".to_owned(),
            completions: Vec::new(),
            sender: Some(v1::CommandSender {
                sender: Some(command_sender::Sender::Player(steve())),
            }),
        }),
    ));
    let from_plugin::Body::Decision(decision) = within(session.hear()).await else {
        panic!("a decision");
    };
    assert_eq!(decision.event, 7);
    let Some(patch::Change::TabComplete(change)) = decision.verdicts[0]
        .patch
        .clone()
        .and_then(|patch| patch.change)
    else {
        panic!("a tab complete patch");
    };
    assert_eq!(change.completions.expect("completions").added, ["Steve-ho"]);
}
