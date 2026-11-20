mod fake_host;

use std::collections::HashSet;

use fake_host::*;
use rspigot_plugin::Invocation;
use rspigot_plugin::prelude::*;
use rspigot_plugin_protocol::v1::{self, argument_value, event, from_plugin, to_plugin};
use tokio::sync::mpsc;
use tonic::Status;

type Log = mpsc::UnboundedSender<String>;

fn said(log: &Log, line: &str) {
    let _ = log.send(line.to_owned());
}

struct Items {
    log: Log,
}

#[listeners]
impl Items {
    async fn on_enable(&self, _server: &Server) {
        tokio::task::yield_now().await;
        said(&self.log, "enable items");
    }

    async fn on_disable(&self) {
        said(&self.log, "disable items");
    }

    async fn on_player_join(&self, _event: &PlayerJoin) {
        said(&self.log, "join items");
    }

    #[command("faction create")]
    async fn create(&self, ctx: Context, name: String) {
        let _ = ctx;
        said(&self.log, &format!("create {name}"));
    }
}

struct Zones {
    log: Log,
}

#[listeners]
impl Zones {
    async fn on_enable(&self, _server: &Server) {
        said(&self.log, "enable zones");
    }

    async fn on_disable(&self) {
        said(&self.log, "disable zones");
    }

    async fn on_player_move(&self, event: &mut PlayerMove) {
        said(&self.log, "move zones");
        event.cancel();
    }

    #[command("faction claim")]
    async fn claim(&self, ctx: Context) {
        let _ = ctx;
        said(&self.log, "claim");
    }
}

struct Passive {
    log: Log,
}

#[listeners]
impl Passive {
    async fn on_enable(&self, _server: &Server) {
        said(&self.log, "enable passive");
    }

    async fn on_disable(&self) {
        said(&self.log, "disable passive");
    }

    async fn on_player_join(&self, _event: &PlayerJoin) {
        said(&self.log, "join passive");
    }
}

struct Aggressive {
    log: Log,
}

#[listeners]
impl Aggressive {
    async fn on_enable(&self, _server: &Server) {
        said(&self.log, "enable aggressive");
    }

    async fn on_disable(&self) {
        said(&self.log, "disable aggressive");
    }

    async fn on_player_join(&self, _event: &PlayerJoin) {
        said(&self.log, "join aggressive");
    }

    #[command("mob spawn")]
    async fn spawn(&self, ctx: Context, count: u32) {
        let _ = ctx;
        said(&self.log, &format!("spawn {count}"));
    }

    async fn on_command(&self, invocation: Invocation) {
        said(&self.log, &format!("dynamic {}", invocation.command()));
    }
}

#[derive(Module)]
#[module(own)]
struct Mobs {
    log: Log,
    #[module]
    passive: Passive,
    #[module]
    aggressive: Aggressive,
}

#[listeners]
impl Mobs {
    async fn on_enable(&self, _server: &Server) {
        said(&self.log, "enable mobs");
    }

    async fn on_disable(&self) {
        said(&self.log, "disable mobs");
    }

    async fn on_player_join(&self, _event: &PlayerJoin) {
        said(&self.log, "join mobs");
    }

    #[command("mob list")]
    async fn list(&self, ctx: Context) {
        let _ = ctx;
        said(&self.log, "list");
    }
}

#[derive(Plugin)]
#[plugin(name = "factions", version = "1.0.0")]
struct Factions {
    #[module]
    items: Items,
    #[module]
    mobs: Mobs,
    #[module]
    zones: Zones,
}

fn factions(log: &Log) -> Factions {
    Factions {
        items: Items { log: log.clone() },
        mobs: Mobs {
            log: log.clone(),
            passive: Passive { log: log.clone() },
            aggressive: Aggressive { log: log.clone() },
        },
        zones: Zones { log: log.clone() },
    }
}

async fn started() -> (Session, mpsc::UnboundedReceiver<String>, v1::Hello) {
    let mut host = Host::start().await;
    let (log, lines) = mpsc::unbounded_channel();
    tokio::spawn(run_with(factions(&log), host.url.clone(), None));
    let knock = host.knock().await;
    let hello = knock.hello.clone();
    (knock.accept(), lines, hello)
}

async fn next(lines: &mut mpsc::UnboundedReceiver<String>) -> String {
    within(lines.recv()).await.expect("the plugin stopped")
}

async fn expect(lines: &mut mpsc::UnboundedReceiver<String>, expected: &[&str]) {
    for line in expected {
        assert_eq!(&next(lines).await, line);
    }
}

fn ids_of(hello: &v1::Hello, kind: v1::EventKind) -> Vec<u32> {
    hello
        .handlers
        .iter()
        .filter(|handler| handler.event == kind as i32)
        .map(|handler| handler.id)
        .collect()
}

fn place() -> v1::Location {
    v1::Location {
        world: "minecraft:overworld".to_owned(),
        position: Some(v1::Vec3 {
            x: 1.0,
            y: 64.0,
            z: 2.0,
        }),
        rotation: Some(v1::Rotation {
            yaw: 0.0,
            pitch: 0.0,
        }),
    }
}

fn joined(id: u64, handlers: &[u32]) -> to_plugin::Body {
    event(
        id,
        handlers,
        v1::Phase::Settled,
        false,
        event::Payload::PlayerJoin(v1::PlayerJoin {
            player: Some(steve()),
            join_message: None,
        }),
    )
}

fn invocation(command: u32, arguments: Vec<argument_value::Value>) -> to_plugin::Body {
    to_plugin::Body::Invocation(v1::Invocation {
        command,
        sender: Some(steve()),
        arguments: arguments
            .into_iter()
            .map(|value| v1::ArgumentValue { value: Some(value) })
            .collect(),
    })
}

fn command_id(hello: &v1::Hello, name: &str, literal: &str) -> u32 {
    hello
        .commands
        .iter()
        .find(|command| {
            command.name == name
                && command
                    .arguments
                    .first()
                    .is_some_and(|argument| argument.name == literal)
        })
        .unwrap_or_else(|| panic!("no command {name} {literal}"))
        .id
}

#[tokio::test]
async fn the_hello_is_one_flat_list_of_every_handler_and_command() {
    let (_session, _lines, hello) = started().await;
    assert_eq!(hello.name, "factions");
    assert_eq!(hello.version, "1.0.0");
    assert_eq!(ids_of(&hello, v1::EventKind::PlayerJoin).len(), 4);
    assert_eq!(ids_of(&hello, v1::EventKind::PlayerMove).len(), 1);
    let names: Vec<&str> = hello
        .commands
        .iter()
        .map(|command| command.name.as_str())
        .collect();
    assert_eq!(names, ["faction", "mob", "mob", "faction"]);
}

#[tokio::test]
async fn ids_are_unique_across_the_tree() {
    let (_session, _lines, hello) = started().await;
    let handlers: HashSet<u32> = hello.handlers.iter().map(|handler| handler.id).collect();
    assert_eq!(handlers.len(), hello.handlers.len());
    let commands: HashSet<u32> = hello.commands.iter().map(|command| command.id).collect();
    assert_eq!(commands.len(), hello.commands.len());
}

#[tokio::test]
async fn at_equal_priority_the_first_declared_module_comes_first_and_a_node_before_its_children() {
    let (session, mut lines, hello) = started().await;
    let ids = ids_of(&hello, v1::EventKind::PlayerJoin);
    assert!(ids.windows(2).all(|pair| pair[0] < pair[1]), "{ids:?}");
    expect(
        &mut lines,
        &[
            "enable items",
            "enable mobs",
            "enable passive",
            "enable aggressive",
            "enable zones",
        ],
    )
    .await;
    session.say(joined(1, &ids));
    expect(
        &mut lines,
        &["join items", "join mobs", "join passive", "join aggressive"],
    )
    .await;
}

#[tokio::test]
async fn an_event_reaches_only_the_handler_it_names() {
    let (session, mut lines, hello) = started().await;
    expect(
        &mut lines,
        &[
            "enable items",
            "enable mobs",
            "enable passive",
            "enable aggressive",
            "enable zones",
        ],
    )
    .await;
    let ids = ids_of(&hello, v1::EventKind::PlayerJoin);
    session.say(joined(1, &[ids[2]]));
    session.say(joined(2, &[ids[3], ids[0]]));
    expect(
        &mut lines,
        &["join passive", "join aggressive", "join items"],
    )
    .await;
}

#[tokio::test]
async fn a_deciding_handler_of_a_leaf_answers_under_its_tree_id() {
    let (mut session, _lines, hello) = started().await;
    let zones = ids_of(&hello, v1::EventKind::PlayerMove)[0];
    session.say(event(
        5,
        &[zones],
        v1::Phase::Decide,
        false,
        event::Payload::PlayerMove(v1::PlayerMove {
            player: Some(steve()),
            from: Some(place()),
            to: Some(place()),
        }),
    ));
    let from_plugin::Body::Decision(decision) = within(session.hear()).await else {
        panic!("a decision");
    };
    assert_eq!(decision.event, 5);
    assert_eq!(decision.verdicts.len(), 1);
    assert_eq!(decision.verdicts[0].handler, zones);
}

#[tokio::test]
async fn commands_reach_the_module_that_declares_them_at_every_depth() {
    let (session, mut lines, hello) = started().await;
    expect(
        &mut lines,
        &[
            "enable items",
            "enable mobs",
            "enable passive",
            "enable aggressive",
            "enable zones",
        ],
    )
    .await;
    session.say(invocation(
        command_id(&hello, "faction", "create"),
        vec![argument_value::Value::Text("wolves".to_owned())],
    ));
    session.say(invocation(command_id(&hello, "faction", "claim"), vec![]));
    session.say(invocation(command_id(&hello, "mob", "list"), vec![]));
    session.say(invocation(
        command_id(&hello, "mob", "spawn"),
        vec![argument_value::Value::Integer(3)],
    ));
    expect(&mut lines, &["create wolves", "claim", "list", "spawn 3"]).await;
}

#[tokio::test]
async fn the_same_command_root_in_several_modules_forms_one_tree() {
    let (_session, _lines, hello) = started().await;
    let faction: Vec<&str> = hello
        .commands
        .iter()
        .filter(|command| command.name == "faction")
        .map(|command| command.arguments[0].name.as_str())
        .collect();
    assert_eq!(faction, ["create", "claim"]);
    let mob: Vec<&str> = hello
        .commands
        .iter()
        .filter(|command| command.name == "mob")
        .map(|command| command.arguments[0].name.as_str())
        .collect();
    assert_eq!(mob, ["list", "spawn"]);
}

#[tokio::test]
async fn a_command_id_outside_every_module_reaches_the_dynamic_hooks() {
    let (session, mut lines, _) = started().await;
    expect(
        &mut lines,
        &[
            "enable items",
            "enable mobs",
            "enable passive",
            "enable aggressive",
            "enable zones",
        ],
    )
    .await;
    session.say(invocation(900, vec![]));
    assert_eq!(next(&mut lines).await, "dynamic 900");
}

#[tokio::test]
async fn enable_runs_depth_first_and_disable_in_reverse() {
    let (session, mut lines, _) = started().await;
    expect(
        &mut lines,
        &[
            "enable items",
            "enable mobs",
            "enable passive",
            "enable aggressive",
            "enable zones",
        ],
    )
    .await;
    session.end(Status::unavailable("gone"));
    expect(
        &mut lines,
        &[
            "disable zones",
            "disable aggressive",
            "disable passive",
            "disable mobs",
            "disable items",
        ],
    )
    .await;
}

struct Counter {
    name: &'static str,
    log: Log,
}

#[listeners]
impl Counter {
    async fn on_player_join(&self, _event: &PlayerJoin) {
        said(&self.log, self.name);
    }
}

#[derive(Module)]
struct Pair {
    #[module]
    left: Counter,
    #[module]
    right: Counter,
}

#[derive(Plugin)]
#[plugin(name = "pair", version = "1.0.0")]
struct Twin {
    #[module]
    pair: Pair,
}

#[tokio::test]
async fn two_fields_of_one_module_type_are_distinct_instances() {
    let mut host = Host::start().await;
    let (log, mut lines) = mpsc::unbounded_channel();
    let twin = Twin {
        pair: Pair {
            left: Counter {
                name: "left",
                log: log.clone(),
            },
            right: Counter { name: "right", log },
        },
    };
    tokio::spawn(run_with(twin, host.url.clone(), None));
    let knock = host.knock().await;
    let ids = ids_of(&knock.hello, v1::EventKind::PlayerJoin);
    assert_eq!(ids.len(), 2);
    let session = knock.accept();
    session.say(joined(1, &[ids[1]]));
    session.say(joined(2, &[ids[0]]));
    expect(&mut lines, &["right", "left"]).await;
}

struct Quiet;

#[listeners]
impl Quiet {
    #[command("faction create")]
    async fn create(&self, ctx: Context) {
        let _ = ctx;
    }
}

#[derive(Plugin)]
#[plugin(name = "clash", version = "1.0.0")]
struct Clash {
    #[module]
    items: Items,
    #[module]
    quiet: Quiet,
}

#[tokio::test]
async fn the_same_full_command_path_in_two_modules_is_refused_before_connecting() {
    let host = Host::start().await;
    let (log, _lines) = mpsc::unbounded_channel();
    let clash = Clash {
        items: Items { log },
        quiet: Quiet,
    };
    let error = run_with(clash, host.url.clone(), None)
        .await
        .expect_err("a clash");
    let message = error.to_string();
    assert!(message.contains("/faction create"), "{message}");
    assert!(message.contains("items.Items"), "{message}");
    assert!(message.contains("quiet.Quiet"), "{message}");
}

struct Warps;

#[listeners]
impl Warps {
    #[command("warp")]
    async fn warp(&self, ctx: Context, #[suggest(Self::places)] place: String) {
        let _ = (ctx, place);
    }
}

impl Warps {
    async fn places(&self, player: &Player, typed: &str) -> Vec<String> {
        vec![format!("{}-{typed}", player.name)]
    }
}

#[derive(Plugin)]
#[plugin(name = "traveler", version = "1.0.0")]
struct Traveler {
    #[module]
    counter: Counter,
    #[module]
    warps: Warps,
}

#[tokio::test]
async fn a_suggestion_inside_a_tree_gets_its_own_tab_complete_id() {
    let mut host = Host::start().await;
    let (log, _lines) = mpsc::unbounded_channel();
    let traveler = Traveler {
        counter: Counter { name: "c", log },
        warps: Warps,
    };
    tokio::spawn(run_with(traveler, host.url.clone(), None));
    let knock = host.knock().await;
    let tab = ids_of(&knock.hello, v1::EventKind::TabComplete);
    assert_eq!(tab.len(), 1);
    assert_ne!(tab[0], u32::MAX);
    let mut session = knock.accept();
    session.say(event(
        7,
        &tab,
        v1::Phase::Decide,
        false,
        event::Payload::TabComplete(v1::TabComplete {
            buffer: "/warp ho".to_owned(),
            completions: Vec::new(),
            sender: Some(v1::CommandSender {
                sender: Some(v1::command_sender::Sender::Player(steve())),
            }),
        }),
    ));
    let from_plugin::Body::Decision(decision) = within(session.hear()).await else {
        panic!("a decision");
    };
    assert_eq!(decision.verdicts[0].handler, tab[0]);
    let Some(v1::patch::Change::TabComplete(change)) = decision.verdicts[0]
        .patch
        .clone()
        .and_then(|patch| patch.change)
    else {
        panic!("a tab complete patch");
    };
    assert_eq!(change.completions.expect("completions").added, ["Steve-ho"]);
}
