mod fake_host;

use fake_host::*;
use rspigot_plugin::prelude::*;
use rspigot_plugin::{ChoiceArgument, CommandArgument, GreedyText, Invocation};
use rspigot_plugin_protocol::v1::{
    self, argument_value, command_sender, event, from_plugin, patch, to_plugin,
};
use tokio::sync::mpsc;

#[derive(Debug, PartialEq, Eq)]
enum Kind {
    Food,
    Tool,
}

impl ChoiceArgument for Kind {
    const CHOICES: &'static [&'static str] = &["food", "tool"];

    fn from_choice(choice: &str) -> Option<Self> {
        match choice {
            "food" => Some(Self::Food),
            "tool" => Some(Self::Tool),
            _ => None,
        }
    }
}

#[derive(Debug, PartialEq)]
enum Seen {
    Buy(Material, u32),
    List(Option<Kind>),
    Probe(f64, f32, Vec<String>, Position, String),
    Late(u32),
}

#[derive(Plugin)]
#[plugin(name = "market", version = "1.0.0")]
struct Market {
    seen: mpsc::UnboundedSender<Seen>,
}

#[listeners]
impl Market {
    #[command("shop", aliases = ["s"])]
    async fn shop(&self, ctx: Context) {
        ctx.reply("usage");
    }

    #[command("shop buy", permission = "shop.buy")]
    async fn buy(&self, ctx: Context, #[suggest(Self::items)] item: Material, amount: u32) {
        let _ = ctx;
        let _ = self.seen.send(Seen::Buy(item, amount));
    }

    #[command("shop list")]
    async fn list(&self, ctx: Context, kind: Option<Kind>) {
        let _ = ctx;
        let _ = self.seen.send(Seen::List(kind));
    }

    #[command("probe")]
    async fn probe(
        &self,
        ctx: Context,
        a: f64,
        b: f32,
        who: Vec<Entity>,
        spot: Position,
        #[suggest(Self::items)] note: GreedyText,
    ) {
        let _ = ctx;
        let names = who
            .iter()
            .map(|entity| entity.entity_id.to_string())
            .collect();
        let _ = self.seen.send(Seen::Probe(a, b, names, spot, note.0));
    }

    async fn on_enable(&self, server: &Server) {
        server.command.register(&rspigot_plugin::CommandSpec {
            id: 900,
            name: "late",
            aliases: &[],
            permission: None,
            arguments: vec![CommandArgument {
                name: "n",
                kind: rspigot_plugin::ArgumentKind::Integer,
                optional: false,
                suggested: false,
                choices: &[],
            }],
        });
        server.command.unregister("old");
    }

    async fn on_command(&self, invocation: Invocation) {
        if invocation.command != 900 {
            return;
        }
        let Some((context, mut arguments)) = invocation.open() else {
            return;
        };
        if let Some(n) = arguments.read::<u32>(&context) {
            let _ = self.seen.send(Seen::Late(n));
        }
    }
}

impl Market {
    async fn items(&self, player: &Player, typed: &str) -> Vec<String> {
        vec![format!("{}:{typed}", player.name)]
    }
}

fn value(value: argument_value::Value) -> v1::ArgumentValue {
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
    tokio::spawn(run_with(Market { seen }, host.url.clone(), None));
    let knock = host.knock().await;
    let hello = knock.hello.clone();
    (knock.accept(), received, hello)
}

#[tokio::test]
async fn the_hello_declares_aliases_literals_and_choices() {
    let (_session, _seen, hello) = started().await;
    let shop = &hello.commands[0];
    assert_eq!(
        (shop.name.as_str(), shop.aliases.as_slice()),
        ("shop", ["s".to_owned()].as_slice())
    );
    let buy = &hello.commands[1];
    assert_eq!(buy.name, "shop");
    assert_eq!(buy.permission.as_deref(), Some("shop.buy"));
    let kinds: Vec<(String, i32)> = buy
        .arguments
        .iter()
        .map(|argument| (argument.name.clone(), argument.kind))
        .collect();
    assert_eq!(
        kinds,
        [
            ("buy".to_owned(), v1::ArgumentKind::Literal as i32),
            ("item".to_owned(), v1::ArgumentKind::Item as i32),
            ("amount".to_owned(), v1::ArgumentKind::Integer as i32),
        ]
    );
    assert!(buy.arguments[1].suggested);
    let list = &hello.commands[2];
    assert_eq!(list.arguments[1].kind, v1::ArgumentKind::Choice as i32);
    assert_eq!(list.arguments[1].choices, ["food", "tool"]);
    assert!(list.arguments[1].optional);
    let probe = &hello.commands[3];
    let probe_kinds: Vec<i32> = probe.arguments.iter().map(|a| a.kind).collect();
    assert_eq!(
        probe_kinds,
        [
            v1::ArgumentKind::Double as i32,
            v1::ArgumentKind::Float as i32,
            v1::ArgumentKind::Entities as i32,
            v1::ArgumentKind::Location as i32,
            v1::ArgumentKind::GreedyText as i32,
        ]
    );
}

#[tokio::test]
async fn a_sub_command_invocation_reaches_its_method_without_the_literal() {
    let (session, mut seen, _) = started().await;
    session.say(invocation(
        2,
        vec![
            value(argument_value::Value::Item("minecraft:bread".to_owned())),
            value(argument_value::Value::Integer(4)),
        ],
    ));
    assert_eq!(
        within(seen.recv()).await,
        Some(Seen::Buy(Material::Bread, 4))
    );
    session.say(invocation(
        3,
        vec![value(argument_value::Value::Text("tool".to_owned()))],
    ));
    assert_eq!(
        within(seen.recv()).await,
        Some(Seen::List(Some(Kind::Tool)))
    );
    session.say(invocation(3, Vec::new()));
    assert_eq!(within(seen.recv()).await, Some(Seen::List(None)));
}

#[tokio::test]
async fn decimals_entities_a_location_and_greedy_text_are_read() {
    let (session, mut seen, _) = started().await;
    session.say(invocation(
        4,
        vec![
            value(argument_value::Value::Decimal(1.5)),
            value(argument_value::Value::Decimal(2.5)),
            value(argument_value::Value::Entities(v1::EntityList {
                entities: vec![v1::EntitySnapshot {
                    entity_id: 77,
                    uuid: ALEX.to_owned(),
                    kind: "minecraft:cow".to_owned(),
                    position: Some(v1::Vec3 {
                        x: 0.0,
                        y: 0.0,
                        z: 0.0,
                    }),
                    ..Default::default()
                }],
            })),
            value(argument_value::Value::Location(v1::Vec3 {
                x: 1.0,
                y: 2.0,
                z: 3.0,
            })),
            value(argument_value::Value::Text("a b".to_owned())),
        ],
    ));
    assert_eq!(
        within(seen.recv()).await,
        Some(Seen::Probe(
            1.5,
            2.5,
            vec!["77".to_owned()],
            Position {
                x: 1.0,
                y: 2.0,
                z: 3.0
            },
            "a b".to_owned()
        ))
    );
}

#[tokio::test]
async fn a_dynamic_command_is_registered_after_hello_and_invokes_the_fallback() {
    let (mut session, mut seen, _) = started().await;
    let from_plugin::Body::RegisterCommand(spec) = within(session.hear()).await else {
        panic!("a registration");
    };
    assert_eq!((spec.id, spec.name.as_str()), (900, "late"));
    assert_eq!(
        within(session.hear()).await,
        from_plugin::Body::UnregisterCommand("old".to_owned())
    );
    session.say(invocation(
        900,
        vec![value(argument_value::Value::Integer(5))],
    ));
    assert_eq!(within(seen.recv()).await, Some(Seen::Late(5)));
}

#[tokio::test]
async fn a_suggestion_follows_the_literals_and_the_widths_before_its_argument() {
    let (mut session, _seen, _) = started().await;
    let _ = within(session.hear()).await;
    let _ = within(session.hear()).await;
    let ask = |id: u64, buffer: &str| {
        event(
            id,
            &[u32::MAX],
            v1::Phase::Decide,
            false,
            event::Payload::TabComplete(v1::TabComplete {
                buffer: buffer.to_owned(),
                completions: Vec::new(),
                sender: Some(v1::CommandSender {
                    sender: Some(command_sender::Sender::Player(steve())),
                }),
            }),
        )
    };
    let answer = async |session: &mut Session, id: u64, buffer: &str| {
        session.say(ask(id, buffer));
        let from_plugin::Body::Decision(decision) = within(session.hear()).await else {
            panic!("a decision");
        };
        decision.verdicts[0]
            .patch
            .clone()
            .and_then(|patch| patch.change)
            .and_then(|change| match change {
                patch::Change::TabComplete(change) => change.completions.map(|c| c.added),
                _ => None,
            })
    };
    assert_eq!(
        answer(&mut session, 1, "/shop buy di").await,
        Some(vec!["Steve:di".to_owned()])
    );
    assert_eq!(
        answer(&mut session, 2, "/s buy di").await,
        Some(vec!["Steve:di".to_owned()])
    );
    assert_eq!(answer(&mut session, 3, "/shop buy diamond 3").await, None);
    assert_eq!(answer(&mut session, 4, "/shop sell di").await, None);
    assert_eq!(
        answer(&mut session, 5, "/probe 1 2 @a 1 2 3 hel").await,
        Some(vec!["Steve:hel".to_owned()])
    );
    assert_eq!(
        answer(&mut session, 6, "/probe 1 2 @a 1 2 3 hello wor").await,
        Some(vec!["Steve:wor".to_owned()])
    );
    assert_eq!(answer(&mut session, 7, "/probe 1 2 @a 1 2").await, None);
}
