mod fake_host;

use std::time::{Duration, Instant};

use fake_host::*;
use rspigot_plugin::{
    AsyncPlayerChat, Error, GameMode, Location, Player, PlayerJoin, PlayerMove, PlayerState,
    Plugin, Position, PotionSplash, Rotation, listeners, run_with,
};
use rspigot_plugin_protocol::PROTOCOL_VERSION;
use rspigot_plugin_protocol::v1::{
    self, answer, event, from_plugin, patch, player_order, question, to_plugin,
};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tonic::{Code, Status};

#[derive(Debug, PartialEq)]
enum Mark {
    Decided {
        handler: u32,
        event: u64,
        height: f64,
    },
    Watched {
        event: u64,
        cancelled: bool,
        rolled_back: bool,
    },
}

#[derive(Debug)]
struct Joined {
    player: Player,
    state: Result<PlayerState, Error>,
    online: Result<Vec<Player>, Error>,
}

#[derive(Plugin)]
#[plugin(name = "guard", version = "1.2.3")]
struct Guard {
    marks: mpsc::UnboundedSender<Mark>,
    joins: mpsc::UnboundedSender<Joined>,
}

#[listeners]
impl Guard {
    async fn on_player_join(&self, event: &PlayerJoin) {
        let player = event.player();
        player.message.send(format!("Welcome, {}!", player.name));
        let state = player.get_state().await;
        let online = player.server().get_players().await;
        let _ = self.joins.send(Joined {
            player: player.clone(),
            state,
            online,
        });
    }

    #[priority(High)]
    async fn on_player_move(&self, event: &mut PlayerMove) {
        let Some(to) = event.to().cloned() else {
            return;
        };
        let _ = self.marks.send(Mark::Decided {
            handler: 2,
            event: event.id(),
            height: to.position.y,
        });
        if to.position.y > 100.0 {
            event.cancel();
        } else if to.position.y > 80.0 {
            event.set_to(Location {
                position: Position {
                    y: 80.0,
                    ..to.position
                },
                ..to
            });
        }
    }

    #[priority(Lowest)]
    #[ignore_cancelled]
    async fn on_player_move_seen(&self, event: &PlayerMove) {
        let _ = self.marks.send(Mark::Watched {
            event: event.id(),
            cancelled: event.is_cancelled(),
            rolled_back: event.is_rolled_back(),
        });
    }

    #[priority(Low)]
    async fn on_player_move_late(&self, event: &mut PlayerMove) {
        let _ = self.marks.send(Mark::Decided {
            handler: 4,
            event: event.id(),
            height: event.to().map_or(0.0, |to| to.position.y),
        });
        if event.to().is_some_and(|to| to.position.y > 100.0) {
            std::future::pending::<()>().await;
        }
    }
}

#[derive(Plugin)]
#[plugin(name = "splasher", version = "1.0.0")]
struct Splasher {
    read: mpsc::UnboundedSender<Vec<f64>>,
}

#[listeners]
impl Splasher {
    async fn on_potion_splash(&self, event: &mut PotionSplash) {
        let splashed = event.affected_entities().to_vec();
        let intensities = |event: &PotionSplash| -> Vec<f64> {
            splashed
                .iter()
                .map(|entity| event.intensity(entity))
                .collect()
        };
        let _ = self.read.send(intensities(event));
        event.set_intensity(&splashed[0], 0.5);
        event.set_intensity(&splashed[1], 1.0);
        event.set_intensity(&splashed[0], 0.25);
        let _ = self.read.send(intensities(event));
    }
}

#[derive(Plugin)]
#[plugin(name = "muter", version = "1.0.0")]
struct Muter {
    read: mpsc::UnboundedSender<Vec<String>>,
}

#[listeners]
impl Muter {
    async fn on_async_player_chat(&self, event: &mut AsyncPlayerChat) {
        event.set_recipients(Vec::new());
        let steve = event.player().clone();
        event.set_recipients(vec![steve.clone(), steve]);
        let names = event
            .recipients()
            .iter()
            .map(|player| player.name.clone())
            .collect();
        let _ = self.read.send(names);
    }
}

struct Running {
    marks: mpsc::UnboundedReceiver<Mark>,
    joins: mpsc::UnboundedReceiver<Joined>,
    ended: JoinHandle<Result<(), Error>>,
}

impl Running {
    async fn joined(&mut self) -> Joined {
        within(self.joins.recv()).await.unwrap()
    }

    fn marks(&mut self) -> Vec<Mark> {
        std::iter::from_fn(|| self.marks.try_recv().ok()).collect()
    }
}

fn launch(host: &Host, token: Option<&str>) -> Running {
    let (marks, marked) = mpsc::unbounded_channel();
    let (joins, joined) = mpsc::unbounded_channel();
    let ended = tokio::spawn(run_with(
        Guard { marks, joins },
        host.url.clone(),
        token.map(str::to_owned),
    ));
    Running {
        marks: marked,
        joins: joined,
        ended,
    }
}

fn joining(player: v1::PlayerSnapshot) -> event::Payload {
    event::Payload::PlayerJoin(v1::PlayerJoin {
        player: Some(player),
        join_message: Some("§eSteve joined the game".to_owned()),
    })
}

fn at(height: f64) -> v1::Location {
    v1::Location {
        world: "minecraft:overworld".to_owned(),
        position: Some(v1::Vec3 {
            x: 1.0,
            y: height,
            z: -3.5,
        }),
        rotation: Some(v1::Rotation {
            yaw: 90.0,
            pitch: -30.0,
        }),
    }
}

fn moving_to(height: f64) -> event::Payload {
    event::Payload::PlayerMove(v1::PlayerMove {
        player: Some(steve()),
        from: Some(at(64.0)),
        to: Some(at(height)),
    })
}

fn decision(event: u64, verdicts: Vec<v1::Verdict>) -> from_plugin::Body {
    from_plugin::Body::Decision(v1::Decision { event, verdicts })
}

fn verdict(handler: u32, cancel: bool, height: Option<f64>) -> v1::Verdict {
    v1::Verdict {
        handler,
        cancel,
        patch: height.map(|y| v1::Patch {
            change: Some(patch::Change::PlayerMove(v1::PlayerMovePatch {
                to: Some(at(y)),
            })),
        }),
    }
}

fn decided(handler: u32, event: u64, height: f64) -> Mark {
    Mark::Decided {
        handler,
        event,
        height,
    }
}

fn message(player: &str, text: &str) -> from_plugin::Body {
    from_plugin::Body::Player(v1::PlayerOrder {
        player: player.to_owned(),
        order: Some(player_order::Order::SendMessage(text.to_owned())),
    })
}

fn resolving(body: from_plugin::Body) -> Option<(u64, String)> {
    match body {
        from_plugin::Body::Question(v1::Question {
            request,
            question: Some(question::Question::Player(player)),
        }) => Some((request, player)),
        _ => None,
    }
}

fn fetching(body: from_plugin::Body) -> Option<u64> {
    match body {
        from_plugin::Body::Question(v1::Question {
            request,
            question: Some(question::Question::Players(_)),
        }) => Some(request),
        _ => None,
    }
}

fn wire(
    id: u32,
    event: v1::EventKind,
    access: v1::Access,
    priority: v1::Priority,
    ignore_cancelled: bool,
) -> v1::Handler {
    v1::Handler {
        id,
        event: event as i32,
        access: access as i32,
        priority: priority as i32,
        ignore_cancelled,
    }
}

#[tokio::test]
async fn a_plugin_introduces_its_handlers() {
    let mut host = Host::start().await;
    let _plugin = launch(&host, Some("s3cret"));

    let hello = host.knock().await.hello;
    assert_eq!(hello.protocol_version, 5);
    assert_eq!(
        hello,
        v1::Hello {
            protocol_version: PROTOCOL_VERSION,
            name: "guard".to_owned(),
            version: "1.2.3".to_owned(),
            token: "s3cret".to_owned(),
            commands: Vec::new(),
            depends: Vec::new(),
            soft_depends: Vec::new(),
            load_before: Vec::new(),
            handlers: vec![
                wire(
                    1,
                    v1::EventKind::PlayerJoin,
                    v1::Access::Observe,
                    v1::Priority::Normal,
                    false
                ),
                wire(
                    2,
                    v1::EventKind::PlayerMove,
                    v1::Access::Decide,
                    v1::Priority::High,
                    false
                ),
                wire(
                    3,
                    v1::EventKind::PlayerMove,
                    v1::Access::Observe,
                    v1::Priority::Lowest,
                    true
                ),
                wire(
                    4,
                    v1::EventKind::PlayerMove,
                    v1::Access::Decide,
                    v1::Priority::Low,
                    false
                ),
            ],
        }
    );

    let mut open = Host::start().await;
    let _plugin = launch(&open, None);
    assert_eq!(open.knock().await.hello.token, "");
}

#[tokio::test]
async fn a_deciding_handler_answers_its_event() {
    let mut host = Host::start().await;
    let mut plugin = launch(&host, None);
    let mut session = host.knock().await.accept();

    session.say(event(
        7,
        &[2, 4],
        v1::Phase::Decide,
        false,
        moving_to(200.0),
    ));
    assert_eq!(
        session.hear().await,
        decision(7, vec![verdict(2, true, None)])
    );
    session.say(event(8, &[2, 4], v1::Phase::Decide, false, moving_to(65.0)));
    assert_eq!(
        session.hear().await,
        decision(8, vec![verdict(2, false, None), verdict(4, false, None)])
    );
    let nowhere = event::Payload::PlayerMove(v1::PlayerMove {
        player: Some(steve()),
        from: None,
        to: None,
    });
    session.say(event(9, &[2, 4], v1::Phase::Decide, false, nowhere));
    assert_eq!(session.hear().await, decision(9, Vec::new()));
    let stranger = event::Payload::PlayerMove(v1::PlayerMove {
        player: Some(snapshot("not a uuid", "Steve", 12)),
        from: Some(at(64.0)),
        to: Some(at(200.0)),
    });
    session.say(event(10, &[2, 4], v1::Phase::Decide, false, stranger));
    assert_eq!(session.hear().await, decision(10, Vec::new()));

    assert_eq!(
        plugin.marks(),
        [
            decided(2, 7, 200.0),
            decided(2, 8, 65.0),
            decided(4, 8, 65.0),
        ]
    );
}

#[tokio::test]
async fn every_deciding_handler_runs_on_its_own_copy_and_the_plugin_answers_once() {
    let mut host = Host::start().await;
    let mut plugin = launch(&host, None);
    let mut session = host.knock().await.accept();

    session.say(event(5, &[2, 4], v1::Phase::Decide, false, moving_to(90.0)));
    assert_eq!(
        session.hear().await,
        decision(
            5,
            vec![verdict(2, false, Some(80.0)), verdict(4, false, None)]
        )
    );
    session.say(event(6, &[4, 2], v1::Phase::Decide, false, moving_to(90.0)));
    assert_eq!(
        session.hear().await,
        decision(
            6,
            vec![verdict(4, false, None), verdict(2, false, Some(80.0))]
        )
    );

    assert_eq!(
        plugin.marks(),
        [
            decided(2, 5, 90.0),
            decided(4, 5, 90.0),
            decided(4, 6, 90.0),
            decided(2, 6, 90.0),
        ]
    );
}

#[tokio::test]
async fn an_observer_answers_nothing() {
    let mut host = Host::start().await;
    let mut plugin = launch(&host, None);
    let mut session = host.knock().await.accept();

    session.say(event(9, &[3], v1::Phase::Settled, true, moving_to(200.0)));
    session.say(event(
        9,
        &[3],
        v1::Phase::RolledBack,
        true,
        moving_to(200.0),
    ));
    session.say(event(10, &[3], v1::Phase::Settled, false, moving_to(65.0)));
    session.say(event(11, &[2], v1::Phase::Decide, false, moving_to(65.0)));
    assert_eq!(
        session.hear().await,
        decision(11, vec![verdict(2, false, None)])
    );

    assert_eq!(
        plugin.marks(),
        [
            Mark::Watched {
                event: 9,
                cancelled: true,
                rolled_back: false
            },
            Mark::Watched {
                event: 9,
                cancelled: true,
                rolled_back: true
            },
            Mark::Watched {
                event: 10,
                cancelled: false,
                rolled_back: false
            },
            decided(2, 11, 65.0),
        ]
    );
}

#[tokio::test]
async fn a_handler_can_ask_a_question_while_handling_an_event() {
    let mut host = Host::start().await;
    let mut plugin = launch(&host, None);
    let mut session = host.knock().await.accept();

    session.say(event(1, &[1], v1::Phase::Settled, false, joining(steve())));
    assert_eq!(session.hear().await, message(STEVE, "Welcome, Steve!"));
    let Some((resolved, about)) = resolving(session.hear().await) else {
        panic!("expected a question about Steve");
    };
    assert_eq!(about, STEVE);
    session.say(answer(
        resolved,
        answer::Body::Player(v1::PlayerState {
            player: Some(steve()),
            health: 17.5,
            food: 12,
            level: 3,
            address: "127.0.0.1".to_owned(),
            ..v1::PlayerState::default()
        }),
    ));
    let Some(fetched) = fetching(session.hear().await) else {
        panic!("expected a question about the players");
    };
    assert_ne!(fetched, resolved);
    session.say(answer(
        fetched,
        answer::Body::Players(v1::PlayerList {
            players: vec![steve(), alex()],
        }),
    ));

    let joined = plugin.joined().await;
    assert_eq!(joined.player.uuid.to_string(), STEVE);
    assert_eq!(joined.player.name, "Steve");
    assert_eq!(joined.player.entity_id, 12);
    assert_eq!(joined.player.world.id(), "minecraft:overworld");
    assert_eq!(
        joined.player.position,
        Position {
            x: 1.0,
            y: 64.0,
            z: -3.5
        }
    );
    assert_eq!(
        joined.player.rotation,
        Rotation {
            yaw: 90.0,
            pitch: -30.0
        }
    );
    assert_eq!(joined.player.game_mode.value(), GameMode::Creative);
    let state = joined.state.unwrap();
    assert_eq!(state.player.name, "Steve");
    assert_eq!((state.health, state.food_level, state.level), (17.5, 12, 3));
    let online = joined.online.unwrap();
    let names: Vec<&str> = online.iter().map(|player| player.name.as_str()).collect();
    assert_eq!(names, ["Steve", "Alex"]);
    assert_eq!(online[1].uuid.to_string(), ALEX);

    session.say(event(2, &[1], v1::Phase::Settled, false, joining(alex())));
    assert_eq!(session.hear().await, message(ALEX, "Welcome, Alex!"));
    let Some((resolved, _)) = resolving(session.hear().await) else {
        panic!("expected a question about Alex");
    };
    session.say(answer(resolved, answer::Body::NotFound(v1::NotFound {})));
    let Some(fetched) = fetching(session.hear().await) else {
        panic!("expected a question about the players");
    };
    session.say(answer(
        fetched,
        answer::Body::Players(v1::PlayerList {
            players: vec![steve()],
        }),
    ));
    let joined = plugin.joined().await;
    assert!(matches!(joined.state, Err(Error::NotFound)));
    assert_eq!(joined.online.unwrap().len(), 1);
}

#[tokio::test]
async fn a_refused_introduction_stops_the_plugin() {
    for code in [
        Code::Unauthenticated,
        Code::InvalidArgument,
        Code::FailedPrecondition,
    ] {
        let mut host = Host::start().await;
        let plugin = launch(&host, Some("wrong"));
        host.knock().await.refuse(Status::new(code, "refused"));

        let ended = within(plugin.ended).await.unwrap();
        let Err(Error::Refused(status)) = ended else {
            panic!("expected a refusal, got {ended:?}");
        };
        assert_eq!(status.code(), code);
    }
}

#[tokio::test]
async fn a_replaced_plugin_stops() {
    let replaced = || Some(Status::already_exists("replaced by a newer connection"));
    for (told, ending) in [(true, replaced()), (true, None), (false, replaced())] {
        let mut host = Host::start().await;
        let plugin = launch(&host, None);
        let session = host.knock().await.accept();

        if told {
            session.say(to_plugin::Body::Replaced(v1::Replaced {}));
        }
        match ending {
            Some(status) => session.end(status),
            None => drop(session),
        }

        let ended = within(plugin.ended).await.unwrap();
        assert!(matches!(ended, Err(Error::Replaced)), "got {ended:?}");
        assert!(host.knocks.try_recv().is_err());
    }
}

#[tokio::test]
async fn a_url_without_a_scheme_is_refused_at_once() {
    for url in ["127.0.0.1:25580", "rspigot:25580", "ftp://127.0.0.1:25580"] {
        let (marks, _marked) = mpsc::unbounded_channel();
        let (joins, _joined) = mpsc::unbounded_channel();
        let started = Instant::now();

        let ended = within(run_with(Guard { marks, joins }, url.to_owned(), None)).await;

        let Err(Error::Url(refused)) = ended else {
            panic!("expected {url} to be refused, got {ended:?}");
        };
        assert_eq!(refused, url);
        assert!(started.elapsed() < Duration::from_millis(900));
    }
}

#[tokio::test]
async fn a_lost_connection_is_retried() {
    let mut host = Host::start().await;
    let mut plugin = launch(&host, None);
    let knock = host.knock().await;
    let hello = knock.hello.clone();
    let mut first = knock.accept();

    first.say(event(1, &[1], v1::Phase::Settled, false, joining(steve())));
    assert_eq!(first.hear().await, message(STEVE, "Welcome, Steve!"));
    assert!(
        resolving(first.hear().await).is_some(),
        "expected a question about Steve"
    );
    let lost = Instant::now();
    drop(first);

    let joined = plugin.joined().await;
    assert!(matches!(joined.state, Err(Error::Disconnected)));
    assert!(matches!(joined.online, Err(Error::Disconnected)));
    let steve = joined.player;
    assert!(matches!(steve.get_state().await, Err(Error::Disconnected)));
    steve.message.send("nobody hears this");

    let knock = host.knock().await;
    assert!(lost.elapsed() >= Duration::from_millis(900));
    assert_eq!(knock.hello, hello);
    let mut second = knock.accept();
    second.say(event(2, &[2], v1::Phase::Decide, false, moving_to(65.0)));
    assert_eq!(
        second.hear().await,
        decision(2, vec![verdict(2, false, None)])
    );
    steve.message.send("back again");
    assert_eq!(second.hear().await, message(STEVE, "back again"));

    second.end(Status::unavailable("the server is stopping"));
    let stopped = Instant::now();
    let knock = host.knock().await;
    let waited = stopped.elapsed();
    assert!(waited >= Duration::from_millis(900));
    assert!(waited < Duration::from_secs(2));
    assert_eq!(knock.hello, hello);
    let mut third = knock.accept();
    third.say(event(3, &[2], v1::Phase::Decide, false, moving_to(200.0)));
    assert_eq!(
        third.hear().await,
        decision(3, vec![verdict(2, true, None)])
    );
}

#[tokio::test]
async fn an_old_event_is_not_decided_after_a_reconnect() {
    let mut host = Host::start().await;
    let mut plugin = launch(&host, None);
    let mut first = host.knock().await.accept();

    first.say(event(1, &[1], v1::Phase::Settled, false, joining(steve())));
    assert_eq!(first.hear().await, message(STEVE, "Welcome, Steve!"));
    assert!(
        resolving(first.hear().await).is_some(),
        "expected a question about Steve"
    );
    first.say(event(2, &[2], v1::Phase::Decide, false, moving_to(200.0)));
    let Session {
        says,
        hears: _still_open,
    } = first;
    drop(says);

    let joined = plugin.joined().await;
    assert!(matches!(joined.state, Err(Error::Disconnected)));
    let mut second = host.knock().await.accept();
    second.say(event(3, &[2], v1::Phase::Decide, false, moving_to(65.0)));
    assert_eq!(
        second.hear().await,
        decision(3, vec![verdict(2, false, None)])
    );
    assert_eq!(plugin.marks(), [decided(2, 3, 65.0)]);
}

#[tokio::test]
async fn an_event_the_plugin_cannot_read_is_skipped() {
    let mut host = Host::start().await;
    let _plugin = launch(&host, None);
    let mut session = host.knock().await.accept();

    let closed = event::Payload::InventoryClose(v1::InventoryClose {
        by_player: true,
        ..v1::InventoryClose::default()
    });
    session.say(event(1, &[1], v1::Phase::Settled, false, closed));
    session.say(event(2, &[1], v1::Phase::Settled, false, joining(steve())));
    assert_eq!(session.hear().await, message(STEVE, "Welcome, Steve!"));
}

fn splashed(entity_id: i32, kind: &str) -> v1::EntitySnapshot {
    v1::EntitySnapshot {
        entity_id,
        uuid: format!("00000000-0000-0000-0000-{entity_id:012}"),
        kind: kind.to_owned(),
        position: Some(v1::Vec3 {
            x: 0.0,
            y: 64.0,
            z: 0.0,
        }),
        player: None,
        ..Default::default()
    }
}

fn intensity(entity_id: i32, value: f64) -> v1::EntitySnapshotDoubleEntry {
    v1::EntitySnapshotDoubleEntry {
        key: Some(splashed(entity_id, "minecraft:zombie")),
        value,
    }
}

#[tokio::test]
async fn a_splash_intensity_is_patched_once_per_entity_with_the_last_value() {
    let mut host = Host::start().await;
    let (read, mut heard) = mpsc::unbounded_channel();
    let _plugin = tokio::spawn(run_with(Splasher { read }, host.url.clone(), None));
    let mut session = host.knock().await.accept();

    let potion = splashed(3, "minecraft:splash_potion");
    session.say(event(
        4,
        &[1],
        v1::Phase::Decide,
        false,
        event::Payload::PotionSplash(v1::PotionSplash {
            affected_entities: vec![
                splashed(4, "minecraft:zombie"),
                splashed(5, "minecraft:zombie"),
            ],
            entity: Some(potion.clone()),
            entity_type: "minecraft:splash_potion".to_owned(),
            intensities: vec![intensity(4, 0.75)],
            hit_block: None,
            hit_block_face: None,
            hit_entity: None,
            potion: Some(potion),
        }),
    ));
    assert_eq!(
        session.hear().await,
        decision(
            4,
            vec![v1::Verdict {
                handler: 1,
                cancel: false,
                patch: Some(v1::Patch {
                    change: Some(patch::Change::PotionSplash(v1::PotionSplashPatch {
                        intensities: vec![intensity(4, 0.25), intensity(5, 1.0)],
                    })),
                }),
            }]
        )
    );
    assert_eq!(within(heard.recv()).await, Some(vec![0.75, 0.0]));
    assert_eq!(within(heard.recv()).await, Some(vec![0.25, 1.0]));
}

#[tokio::test]
async fn a_list_setter_sends_the_diff_from_the_snapshot_it_received() {
    let mut host = Host::start().await;
    let (read, mut heard) = mpsc::unbounded_channel();
    let _plugin = tokio::spawn(run_with(Muter { read }, host.url.clone(), None));
    let mut session = host.knock().await.accept();

    session.say(event(
        5,
        &[1],
        v1::Phase::Decide,
        false,
        event::Payload::AsyncPlayerChat(v1::AsyncPlayerChat {
            format: "<%1$s> %2$s".to_owned(),
            message: "psst".to_owned(),
            player: Some(steve()),
            recipients: vec![steve(), alex()],
        }),
    ));
    assert_eq!(
        session.hear().await,
        decision(
            5,
            vec![v1::Verdict {
                handler: 1,
                cancel: false,
                patch: Some(v1::Patch {
                    change: Some(patch::Change::AsyncPlayerChat(v1::AsyncPlayerChatPatch {
                        recipients: Some(v1::PlayerSnapshotListDiff {
                            removed: vec![1],
                            replaced: Vec::new(),
                            added: vec![steve()],
                        }),
                        ..v1::AsyncPlayerChatPatch::default()
                    })),
                }),
            }]
        )
    );
    assert_eq!(
        within(heard.recv()).await,
        Some(vec!["Steve".to_owned(), "Steve".to_owned()])
    );
}
