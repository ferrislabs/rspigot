mod common;

use std::time::Duration;

use common::*;
use crossbeam_channel::{RecvTimeoutError, Sender, bounded, unbounded};
use rspigot_core::config::{GameConfig, PluginSettings, PriorityOverride};
use rspigot_core::events::{ClientEvent, Inbound, PlayerInput};
use rspigot_core::plugins::events::{PlayerJoin, PlayerLeave, PlayerMove, PlayerMovePatch};
use rspigot_core::plugins::{
    Answer, EventKind, EventPatch, EventPayload, FromPlugin, HandlerSpec, Location, Phase,
    PlayerOrder, Priority, Question, ToPlugin, Verdict,
};
use rspigot_core::ports::PluginOutbox;
use rspigot_core::server::Server;
use rspigot_domain::player::PlayerId;
use rspigot_domain::{Rotation, Vec3};

#[test]
fn a_plugin_is_accepted_when_it_connects() {
    let mut server = server(PluginSettings::default());
    let welcome = connect(&mut server, 1, "welcome");
    assert_eq!(welcome.received(), [ToPlugin::Accepted]);
}

#[test]
fn the_plugins_command_lists_connected_plugins() {
    let mut server = server(PluginSettings::default());
    let alice = join(&mut server, "Alice");
    assert_eq!(plugins_line(&mut server, &alice), "Plugins (0): ");
    connect(&mut server, 1, "welcome");
    assert_eq!(plugins_line(&mut server, &alice), "Plugins (1): welcome");
}

#[test]
fn the_plugins_command_follows_the_configured_order() {
    let mut server = server(PluginSettings {
        order: vec!["guard".to_owned()],
        ..PluginSettings::default()
    });
    let alice = join(&mut server, "Alice");
    connect(&mut server, 1, "welcome");
    connect(&mut server, 2, "chat");
    connect(&mut server, 3, "guard");
    assert_eq!(
        plugins_line(&mut server, &alice),
        "Plugins (3): guard, chat, welcome"
    );
}

#[test]
fn a_plugin_that_takes_a_connected_name_replaces_the_old_connection() {
    let mut server = server(PluginSettings::default());
    let alice = join(&mut server, "Alice");
    let old = connect(&mut server, 1, "welcome");
    let new = connect(&mut server, 2, "welcome");
    assert_eq!(old.received(), [ToPlugin::Accepted, ToPlugin::Replaced]);
    assert_eq!(new.received(), [ToPlugin::Accepted]);
    server.handle(Inbound::PluginDisconnected { connection: 1 });
    assert_eq!(plugins_line(&mut server, &alice), "Plugins (1): welcome");
}

#[test]
fn a_disconnected_plugin_leaves_the_list() {
    let mut server = server(PluginSettings::default());
    let alice = join(&mut server, "Alice");
    connect(&mut server, 1, "welcome");
    server.handle(Inbound::PluginDisconnected { connection: 1 });
    assert_eq!(plugins_line(&mut server, &alice), "Plugins (0): ");
}

#[test]
fn observers_hear_a_player_join_and_leave() {
    let mut server = server(PluginSettings::default());
    let observer = connect_with(
        &mut server,
        1,
        "welcome",
        vec![
            observe(1, EventKind::PlayerJoin),
            observe(2, EventKind::PlayerLeave),
            observe(3, EventKind::PlayerJoin),
        ],
    );
    let alice = join(&mut server, "Alice");

    let joined = heard(&observer);
    assert_eq!(joined.len(), 1);
    let ToPlugin::Event {
        blocks: _,
        id,
        handlers,
        phase,
        cancelled,
        payload:
            EventPayload::PlayerJoin(PlayerJoin {
                player,
                join_message,
            }),
    } = &joined[0]
    else {
        panic!("expected a join event, got {:?}", joined[0]);
    };
    assert_eq!(
        (*id, handlers.as_slice(), *phase, *cancelled),
        (1, &[1, 3][..], Phase::Settled, false)
    );
    assert_eq!(player.id, alice.id);
    assert_eq!(player.name, "Alice");
    assert_eq!(player.world, "minecraft:overworld");
    assert_eq!(player.position, Vec3::new(0.5, 64.0, 0.5));
    assert_eq!(player.game_mode, GameConfig::default().default_game_mode);
    assert_eq!(join_message.as_deref(), Some("§eAlice joined the game"));

    server.handle(leave_of(&alice));

    let left = heard(&observer);
    assert_eq!(left.len(), 2);
    let ToPlugin::Event {
        blocks: _,
        id,
        handlers,
        phase,
        cancelled,
        payload:
            EventPayload::PlayerLeave(PlayerLeave {
                player,
                quit_message,
            }),
    } = &left[1]
    else {
        panic!("expected a leave event, got {:?}", left[1]);
    };
    assert_eq!(
        (*id, handlers.as_slice(), *phase, *cancelled),
        (2, &[2][..], Phase::Settled, false)
    );
    assert_eq!(player.id, alice.id);
    assert_eq!(player.name, "Alice");
    assert_eq!(player.position, Vec3::new(0.5, 64.0, 0.5));
    assert_eq!(quit_message.as_deref(), Some("§eAlice left the game"));
}

#[test]
fn a_plugin_that_does_not_listen_hears_nothing() {
    let mut server = server(PluginSettings::default());
    let quiet = connect(&mut server, 1, "quiet");
    let leaver = connect_with(
        &mut server,
        2,
        "leaver",
        vec![observe(1, EventKind::PlayerLeave)],
    );
    let alice = join(&mut server, "Alice");
    assert!(heard(&quiet).is_empty());
    assert!(heard(&leaver).is_empty());

    server.handle(leave_of(&alice));
    assert!(heard(&quiet).is_empty());
    assert_eq!(heard(&leaver).len(), 1);
}

#[test]
fn send_message_reaches_the_player() {
    let mut scene = scene(&["Alice"]);
    let alice = scene.players[0].clone();
    scene.act(to(&alice, PlayerOrder::SendMessage("hi".to_owned())));
    assert_eq!(texts(&alice), [("hi".to_owned(), false)]);
}

#[test]
fn resolve_answers_with_the_current_state_or_not_found() {
    let mut scene = scene(&["Alice"]);
    let alice = scene.players[0].clone();

    let Answer::Player(before) = scene.ask(Question::Player(alice.id)) else {
        panic!("expected the state of Alice");
    };
    assert_eq!(before.player.id, alice.id);
    assert_eq!(before.player.name, "Alice");
    assert_eq!(before.health, 20.0);
    assert_eq!(before.food, 20);
    assert_eq!(before.level, 0);

    scene.server.give_experience(alice.id, 7);
    send_move(
        &mut scene.server,
        &alice,
        None,
        Some(Rotation::new(90.0, -30.0)),
    );
    let Answer::Player(after) = scene.ask(Question::Player(alice.id)) else {
        panic!("expected the state of Alice");
    };
    assert_eq!(after.level, 1);
    assert_eq!(after.player.rotation, Rotation::new(90.0, -30.0));
    assert_eq!(
        scene.ask(Question::Player(PlayerId::offline("Nobody"))),
        Answer::NotFound
    );
}

#[test]
fn fetch_lists_the_online_players() {
    let mut server = server(PluginSettings::default());
    let welcome = connect(&mut server, 1, "welcome");
    order(&mut server, 1, players_question(8));
    join(&mut server, "Alice");
    join(&mut server, "Bob");
    order(&mut server, 1, players_question(9));

    let answers = heard(&welcome);
    assert_eq!(answers.len(), 2);
    assert_eq!(
        answers[0],
        ToPlugin::Answer {
            request: 8,
            answer: Answer::Players(Vec::new())
        }
    );
    let ToPlugin::Answer {
        request: 9,
        answer: Answer::Players(players),
    } = &answers[1]
    else {
        panic!("expected the online players, got {:?}", answers[1]);
    };
    let names: Vec<&str> = players.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["Alice", "Bob"]);
    assert_eq!(players[0].id, PlayerId::offline("Alice"));
    assert_eq!(players[1].id, PlayerId::offline("Bob"));
}

#[test]
fn an_answer_goes_only_to_the_connection_that_asked() {
    let mut server = server(PluginSettings::default());
    let alice = join(&mut server, "Alice");
    let asker = connect(&mut server, 1, "asker");
    let bystander = connect(&mut server, 2, "bystander");
    order(&mut server, 1, player_question(1, &alice));
    order(&mut server, 1, players_question(2));
    assert_eq!(heard(&asker).len(), 2);
    assert!(heard(&bystander).is_empty());
}

#[test]
fn messages_from_an_unknown_connection_are_ignored() {
    let mut server = server(PluginSettings::default());
    let alice = join(&mut server, "Alice");
    let old = connect(&mut server, 1, "welcome");
    let new = connect(&mut server, 2, "welcome");
    alice.link.take();
    for connection in [1, 99] {
        order(
            &mut server,
            connection,
            FromPlugin::Order(to(&alice, PlayerOrder::SendMessage("hi".to_owned()))),
        );
        order(&mut server, connection, player_question(1, &alice));
        order(&mut server, connection, players_question(2));
    }
    assert!(alice.link.take().is_empty());
    assert_eq!(heard(&old), [ToPlugin::Replaced]);
    assert!(heard(&new).is_empty());
}

fn player_question(request: u64, player: &Player) -> FromPlugin {
    FromPlugin::Question {
        request,
        question: Question::Player(player.id),
    }
}

fn players_question(request: u64) -> FromPlugin {
    FromPlugin::Question {
        request,
        question: Question::Players,
    }
}

const SPAWN: Vec3 = Vec3::new(0.5, 64.0, 0.5);

fn east(blocks: u32) -> Vec3 {
    Vec3::new(0.5 + f64::from(blocks), 64.0, 0.5)
}

fn send_move(
    server: &mut Server<Link>,
    player: &Player,
    position: Option<Vec3>,
    rotation: Option<Rotation>,
) {
    send_move_on(server, player, position, rotation, true);
}

fn send_move_on(
    server: &mut Server<Link>,
    player: &Player,
    position: Option<Vec3>,
    rotation: Option<Rotation>,
    on_ground: bool,
) {
    server.handle(Inbound::Input {
        player: player.id,
        input: PlayerInput::Move {
            position,
            rotation,
            on_ground,
        },
    });
    server.deliver();
}

fn step(server: &mut Server<Link>, player: &Player, to: Vec3) {
    send_move(server, player, Some(to), None);
}

fn answer(server: &mut Server<Link>, connection: u64, event: u64, cancel: bool) {
    let verdicts = vec![verdict(1, cancel, None)];
    order(server, connection, FromPlugin::Decision { event, verdicts });
}

fn verdict(handler: u32, cancel: bool, to: Option<Vec3>) -> Verdict {
    Verdict {
        handler,
        cancel,
        patch: to.map(|to| {
            EventPatch::PlayerMove(PlayerMovePatch {
                to: Some(Location {
                    world: "minecraft:overworld".to_owned(),
                    position: to,
                    rotation: Rotation::default(),
                }),
            })
        }),
    }
}

fn rule(server: &mut Server<Link>, connection: u64, event: u64, verdicts: Vec<Verdict>) {
    order(server, connection, FromPlugin::Decision { event, verdicts });
}

fn decider(id: u32, priority: Priority) -> HandlerSpec {
    HandlerSpec {
        priority,
        ..decide(id)
    }
}

fn ticks(server: &mut Server<Link>, count: usize) {
    for _ in 0..count {
        server.tick();
    }
}

fn wait_out_the_deadline(server: &mut Server<Link>) {
    ticks(server, 2);
}

fn kill(server: &mut Server<Link>, player: &Player) {
    server.handle(Inbound::Console("kill Alice".to_owned()));
    server.deliver();
    assert_eq!(server.player_is_dead(player.id), Some(true));
}

#[derive(Debug, PartialEq)]
struct Heard {
    id: u64,
    handlers: Vec<u32>,
    phase: Phase,
    cancelled: bool,
    from: Vec3,
    to: Vec3,
}

fn moves(mailbox: &Mailbox) -> Vec<Heard> {
    mailbox
        .received()
        .into_iter()
        .filter_map(|message| match message {
            ToPlugin::Event {
                blocks: _,
                id,
                handlers,
                phase,
                cancelled,
                payload: EventPayload::PlayerMove(PlayerMove { from, to, .. }),
            } => Some(Heard {
                id,
                handlers,
                phase,
                cancelled,
                from: from.position,
                to: to.map_or(from.position, |to| to.position),
            }),
            _ => None,
        })
        .collect()
}

fn heard_move(
    id: u64,
    handlers: &[u32],
    phase: Phase,
    cancelled: bool,
    from: Vec3,
    to: Vec3,
) -> Heard {
    Heard {
        id,
        handlers: handlers.to_vec(),
        phase,
        cancelled,
        from,
        to,
    }
}

fn warnings(mailbox: &Mailbox) -> Vec<String> {
    mailbox
        .received()
        .into_iter()
        .filter_map(|message| match message {
            ToPlugin::Warning(text) => Some(text),
            _ => None,
        })
        .collect()
}

fn too_late(event: u64) -> String {
    format!("cancel of event {event} arrived too late")
}

fn teleported_to(player: &Player) -> Vec<Vec3> {
    player
        .link
        .take()
        .into_iter()
        .filter_map(|event| match event {
            ClientEvent::Teleport { position, .. } => Some(position),
            _ => None,
        })
        .collect()
}

fn confirm_teleports(server: &mut Server<Link>, player: &Player) -> Vec<(Vec3, Rotation)> {
    let mut confirmed = Vec::new();
    for event in player.link.take() {
        if let ClientEvent::Teleport {
            id,
            position,
            rotation,
        } = event
        {
            server.handle(Inbound::Input {
                player: player.id,
                input: PlayerInput::TeleportConfirm(id),
            });
            confirmed.push((position, rotation));
        }
    }
    confirmed
}

fn asked(mailbox: &Mailbox) -> Vec<u64> {
    moves(mailbox)
        .into_iter()
        .filter(|heard| heard.phase == Phase::Decide)
        .map(|heard| heard.id)
        .collect()
}

#[test]
fn a_move_without_deciders_applies_at_once() {
    let mut server = server(PluginSettings::default());
    let joins = connect_with(
        &mut server,
        1,
        "joins",
        vec![observe(1, EventKind::PlayerJoin)],
    );
    let alice = join(&mut server, "Alice");
    let Some(ToPlugin::Event { id: joined, .. }) = heard(&joins).pop() else {
        panic!("expected the join of Alice");
    };
    step(&mut server, &alice, east(1));
    assert_eq!(server.player_location(alice.id), Some(east(1)));

    let watch = connect_with(
        &mut server,
        2,
        "watch",
        vec![observe(1, EventKind::PlayerMove)],
    );
    step(&mut server, &alice, east(2));
    assert_eq!(server.player_location(alice.id), Some(east(2)));
    assert_eq!(
        moves(&watch),
        [heard_move(
            joined + 1,
            &[1],
            Phase::Settled,
            false,
            east(1),
            east(2)
        )]
    );
    let Some(ToPlugin::Event {
        payload: EventPayload::PlayerMove(PlayerMove { player, .. }),
        ..
    }) = heard(&watch).pop()
    else {
        panic!("expected a move event");
    };
    assert_eq!(player.id, alice.id);
    assert_eq!(player.position, east(1));
}

#[test]
fn a_cancelled_move_sends_the_player_back() {
    let mut server = server(PluginSettings::default());
    let alice = join(&mut server, "Alice");
    let guard = connect_with(&mut server, 1, "guard", vec![decide(1)]);
    let judge = connect_with(&mut server, 2, "judge", vec![decide(1)]);
    alice.link.take();
    step(&mut server, &alice, east(1));
    let heard = moves(&guard);
    assert_eq!(heard.len(), 1);
    assert_eq!(
        heard[0],
        heard_move(heard[0].id, &[1], Phase::Decide, false, SPAWN, east(1))
    );
    answer(&mut server, 1, heard[0].id, true);
    assert_eq!(teleported_to(&alice), [SPAWN]);
    assert_eq!(server.player_location(alice.id), Some(SPAWN));
    answer(&mut server, 2, heard[0].id, true);
    ticks(&mut server, 2);
    assert_eq!(server.player_location(alice.id), Some(SPAWN));
    assert!(teleported_to(&alice).is_empty());
    assert!(warnings(&guard).is_empty());
    assert!(warnings(&judge).is_empty());
}

#[test]
fn a_move_waits_for_its_deciders_then_applies() {
    let mut server = server(PluginSettings::default());
    let alice = join(&mut server, "Alice");
    connect_with(&mut server, 1, "guard", vec![decide(1)]);
    let judge = connect_with(&mut server, 2, "judge", vec![decide(1)]);
    alice.link.take();
    step(&mut server, &alice, east(1));
    assert_eq!(server.player_location(alice.id), Some(SPAWN));
    let event = asked(&judge)[0];
    answer(&mut server, 1, event, false);
    assert_eq!(server.player_location(alice.id), Some(SPAWN));
    answer(&mut server, 2, event, false);
    assert_eq!(server.player_location(alice.id), Some(east(1)));
    assert!(teleported_to(&alice).is_empty());
}

#[test]
fn a_silent_decider_lets_the_move_through_at_the_deadline() {
    let mut server = server(PluginSettings::default());
    let alice = join(&mut server, "Alice");
    connect_with(&mut server, 1, "guard", vec![decide(1)]);
    alice.link.take();
    step(&mut server, &alice, east(1));
    assert_eq!(server.player_location(alice.id), Some(SPAWN));
    ticks(&mut server, 1);
    assert_eq!(server.player_location(alice.id), Some(SPAWN));
    ticks(&mut server, 1);
    assert_eq!(server.player_location(alice.id), Some(east(1)));
    assert!(teleported_to(&alice).is_empty());
}

#[test]
fn a_late_cancel_rolls_the_move_back_and_tells_observers() {
    let mut server = server(PluginSettings::default());
    let alice = join(&mut server, "Alice");
    let guard = connect_with(&mut server, 1, "guard", vec![decide(1)]);
    let watch = connect_with(
        &mut server,
        2,
        "watch",
        vec![observe(1, EventKind::PlayerMove)],
    );
    alice.link.take();
    step(&mut server, &alice, east(1));
    wait_out_the_deadline(&mut server);
    assert_eq!(server.player_location(alice.id), Some(east(1)));
    let event = asked(&guard)[0];
    answer(&mut server, 1, event, true);
    assert_eq!(teleported_to(&alice), [SPAWN]);
    assert_eq!(server.player_location(alice.id), Some(SPAWN));
    assert_eq!(
        moves(&watch),
        [
            heard_move(event, &[1], Phase::Settled, false, SPAWN, east(1)),
            heard_move(event, &[1], Phase::RolledBack, true, SPAWN, east(1)),
        ]
    );
    assert!(warnings(&guard).is_empty());
}

#[test]
fn a_cancel_after_the_window_only_warns() {
    for decision_ticks in [1, 10] {
        let mut server = server(PluginSettings {
            decision_ticks,
            ..PluginSettings::default()
        });
        let alice = join(&mut server, "Alice");
        let guard = connect_with(&mut server, 1, "guard", vec![decide(1)]);
        alice.link.take();
        step(&mut server, &alice, east(1));
        ticks(&mut server, 21);
        let event = asked(&guard)[0];
        answer(&mut server, 1, event, true);
        assert_eq!(warnings(&guard), [too_late(event)]);
        assert!(teleported_to(&alice).is_empty());
        assert_eq!(server.player_location(alice.id), Some(east(1)));
    }
}

#[test]
fn a_patch_after_the_window_on_a_cancelled_move_is_ignored_silently() {
    let mut server = server(PluginSettings {
        decision_ticks: 10,
        late_cancel_ticks: 2,
        ..PluginSettings::default()
    });
    let alice = join(&mut server, "Alice");
    let guard = connect_with(&mut server, 1, "guard", vec![decide(1)]);
    let judge = connect_with(&mut server, 2, "judge", vec![decide(1)]);
    alice.link.take();
    step(&mut server, &alice, east(1));
    let event = asked(&guard)[0];
    answer(&mut server, 1, event, true);
    assert_eq!(server.player_location(alice.id), Some(SPAWN));
    ticks(&mut server, 5);
    rule(
        &mut server,
        2,
        event,
        vec![verdict(1, false, Some(south(3)))],
    );
    assert_eq!(server.player_location(alice.id), Some(SPAWN));
    assert!(warnings(&guard).is_empty());
    assert!(warnings(&judge).is_empty());
}

#[test]
fn ignore_cancelled_observers_only_hear_moves_that_happened() {
    let mut server = server(PluginSettings::default());
    let alice = join(&mut server, "Alice");
    let guard = connect_with(&mut server, 1, "guard", vec![decide(1)]);
    let mut real_moves = observe(1, EventKind::PlayerMove);
    real_moves.ignore_cancelled = true;
    let watch = connect_with(
        &mut server,
        2,
        "watch",
        vec![real_moves, observe(2, EventKind::PlayerMove)],
    );
    alice.link.take();

    step(&mut server, &alice, east(1));
    let cancelled = asked(&guard)[0];
    answer(&mut server, 1, cancelled, true);
    assert_eq!(confirm_teleports(&mut server, &alice).len(), 1);

    step(&mut server, &alice, east(1));
    let allowed = asked(&guard)[1];
    answer(&mut server, 1, allowed, false);

    step(&mut server, &alice, east(2));
    let rolled_back = asked(&guard)[2];
    wait_out_the_deadline(&mut server);
    answer(&mut server, 1, rolled_back, true);

    assert_eq!(
        moves(&watch),
        [
            heard_move(cancelled, &[2], Phase::Settled, true, SPAWN, east(1)),
            heard_move(allowed, &[1, 2], Phase::Settled, false, SPAWN, east(1)),
            heard_move(
                rolled_back,
                &[1, 2],
                Phase::Settled,
                false,
                east(1),
                east(2)
            ),
            heard_move(
                rolled_back,
                &[1, 2],
                Phase::RolledBack,
                true,
                east(1),
                east(2)
            ),
        ]
    );
}

#[test]
fn queued_moves_settle_in_order_at_the_deadline() {
    let mut server = server(PluginSettings::default());
    let alice = join(&mut server, "Alice");
    let guard = connect_with(&mut server, 1, "guard", vec![decide(1)]);
    let watch = connect_with(
        &mut server,
        2,
        "watch",
        vec![observe(1, EventKind::PlayerMove)],
    );
    alice.link.take();
    for blocks in 1..=5 {
        step(&mut server, &alice, east(blocks));
    }
    assert_eq!(server.player_location(alice.id), Some(SPAWN));
    let questions = moves(&guard);
    assert_eq!(questions.len(), 5);
    for (blocks, question) in (1..=5).zip(&questions) {
        assert_eq!(
            *question,
            heard_move(
                question.id,
                &[1],
                Phase::Decide,
                false,
                east(blocks - 1),
                east(blocks)
            )
        );
    }
    assert!(moves(&watch).is_empty());

    ticks(&mut server, 1);
    assert_eq!(server.player_location(alice.id), Some(SPAWN));
    ticks(&mut server, 1);
    assert_eq!(server.player_location(alice.id), Some(east(5)));
    assert!(teleported_to(&alice).is_empty());
    let settled: Vec<Heard> = (1..=5)
        .zip(&questions)
        .map(|(blocks, question)| {
            heard_move(
                question.id,
                &[1],
                Phase::Settled,
                false,
                east(blocks - 1),
                east(blocks),
            )
        })
        .collect();
    assert_eq!(moves(&watch), settled);
}

#[test]
fn a_rollback_also_undoes_the_moves_after_it() {
    let mut server = server(PluginSettings::default());
    let alice = join(&mut server, "Alice");
    let guard = connect_with(&mut server, 1, "guard", vec![decide(1)]);
    let watch = connect_with(
        &mut server,
        2,
        "watch",
        vec![observe(1, EventKind::PlayerMove)],
    );
    alice.link.take();
    for blocks in 1..=3 {
        step(&mut server, &alice, east(blocks));
        wait_out_the_deadline(&mut server);
    }
    assert_eq!(server.player_location(alice.id), Some(east(3)));
    step(&mut server, &alice, east(4));
    let events = asked(&guard);
    assert_eq!(events.len(), 4);

    answer(&mut server, 1, events[0], true);
    assert_eq!(teleported_to(&alice), [SPAWN]);
    assert_eq!(server.player_location(alice.id), Some(SPAWN));
    let applied = (0..3).map(|n| {
        heard_move(
            events[n],
            &[1],
            Phase::Settled,
            false,
            east(n as u32),
            east(n as u32 + 1),
        )
    });
    let undone = (0..3).map(|n| {
        heard_move(
            events[n],
            &[1],
            Phase::RolledBack,
            true,
            east(n as u32),
            east(n as u32 + 1),
        )
    });
    let dropped = heard_move(events[3], &[1], Phase::Settled, true, east(3), east(4));
    let expected: Vec<Heard> = applied
        .chain(undone)
        .chain(std::iter::once(dropped))
        .collect();
    assert_eq!(moves(&watch), expected);

    answer(&mut server, 1, events[2], true);
    answer(&mut server, 1, events[3], true);
    assert!(warnings(&guard).is_empty());
    assert!(teleported_to(&alice).is_empty());
    assert_eq!(server.player_location(alice.id), Some(SPAWN));
    assert_eq!(moves(&watch), expected);
}

#[test]
fn standing_still_and_turning_are_not_events() {
    let mut server = server(PluginSettings::default());
    let alice = join(&mut server, "Alice");
    let guard = connect_with(&mut server, 1, "guard", vec![decide(1)]);
    alice.link.take();

    send_move(&mut server, &alice, Some(SPAWN), None);
    let turned = Rotation::new(90.0, 10.0);
    send_move(&mut server, &alice, None, Some(turned));
    assert!(heard(&guard).is_empty());
    assert_eq!(server.player_location(alice.id), Some(SPAWN));

    send_move(&mut server, &alice, Some(Vec3::new(500.5, 64.0, 0.5)), None);
    assert!(heard(&guard).is_empty());
    assert_eq!(confirm_teleports(&mut server, &alice), [(SPAWN, turned)]);

    step(&mut server, &alice, east(1));
    step(&mut server, &alice, east(1));
    assert_eq!(asked(&guard).len(), 1);
    assert_eq!(server.player_location(alice.id), Some(SPAWN));
}

#[test]
fn a_plugin_leaving_releases_the_moves_waiting_on_it() {
    let mut server = server(PluginSettings::default());
    let alice = join(&mut server, "Alice");
    connect_with(&mut server, 1, "guard", vec![decide(1)]);
    let judge = connect_with(&mut server, 2, "judge", vec![decide(1)]);
    alice.link.take();
    step(&mut server, &alice, east(1));
    step(&mut server, &alice, east(2));
    for event in asked(&judge) {
        answer(&mut server, 2, event, false);
    }
    assert_eq!(server.player_location(alice.id), Some(SPAWN));
    server.handle(Inbound::PluginDisconnected { connection: 1 });
    assert_eq!(server.player_location(alice.id), Some(east(2)));
    assert!(teleported_to(&alice).is_empty());
}

#[test]
fn a_player_leaving_drops_their_pending_moves() {
    let mut server = server(PluginSettings::default());
    let alice = join(&mut server, "Alice");
    let guard = connect_with(&mut server, 1, "guard", vec![decide(1)]);
    let watch = connect_with(
        &mut server,
        2,
        "watch",
        vec![observe(1, EventKind::PlayerMove)],
    );
    step(&mut server, &alice, east(1));
    wait_out_the_deadline(&mut server);
    step(&mut server, &alice, east(2));
    server.handle(leave_of(&alice));
    ticks(&mut server, 2);
    assert_eq!(server.players_online(), 0);
    let events = asked(&guard);
    assert_eq!(events.len(), 2);
    for event in &events {
        answer(&mut server, 1, *event, true);
    }
    assert_eq!(warnings(&guard), [too_late(events[0])]);
    assert_eq!(
        moves(&watch),
        [
            heard_move(events[0], &[1], Phase::Settled, false, SPAWN, east(1)),
            heard_move(events[1], &[1], Phase::Settled, true, east(1), east(2)),
        ]
    );
}

#[test]
fn a_decision_from_a_replaced_connection_is_ignored() {
    let mut server = server(PluginSettings::default());
    let alice = join(&mut server, "Alice");
    let old = connect_with(&mut server, 1, "guard", vec![decide(1)]);
    alice.link.take();
    step(&mut server, &alice, east(1));
    wait_out_the_deadline(&mut server);
    step(&mut server, &alice, east(2));
    assert_eq!(server.player_location(alice.id), Some(east(1)));

    let new = connect_with(
        &mut server,
        2,
        "guard",
        vec![decide(1), observe(2, EventKind::PlayerMove)],
    );
    assert_eq!(server.player_location(alice.id), Some(east(2)));
    let released = asked(&old)[1];
    assert_eq!(new.received()[0], ToPlugin::Accepted);
    assert_eq!(
        moves(&new),
        [heard_move(
            released,
            &[2],
            Phase::Settled,
            false,
            east(1),
            east(2)
        )]
    );

    for event in asked(&old) {
        answer(&mut server, 1, event, true);
    }
    assert_eq!(server.player_location(alice.id), Some(east(2)));
    assert!(teleported_to(&alice).is_empty());
    assert!(warnings(&old).is_empty());
    assert_eq!(old.received().last(), Some(&ToPlugin::Replaced));
    assert_eq!(heard(&new).len(), 1);
}

#[test]
fn a_server_teleport_forgets_the_moves_in_flight() {
    let mut server = server(PluginSettings::default());
    let alice = join(&mut server, "Alice");
    let guard = connect_with(&mut server, 1, "guard", vec![decide(1)]);
    step(&mut server, &alice, east(1));
    wait_out_the_deadline(&mut server);
    step(&mut server, &alice, east(2));
    let far = Vec3::new(40.5, 64.0, 0.5);
    server.handle(Inbound::Console("tp Alice 40.5 64 0.5".to_owned()));
    ticks(&mut server, 2);
    assert_eq!(server.player_location(alice.id), Some(far));
    let events = asked(&guard);
    for event in &events {
        answer(&mut server, 1, *event, true);
    }
    assert_eq!(server.player_location(alice.id), Some(far));
    assert_eq!(warnings(&guard), [too_late(events[0])]);
}

#[test]
fn a_turn_after_a_pending_move_is_kept() {
    let mut server = server(PluginSettings::default());
    let alice = join(&mut server, "Alice");
    let guard = connect_with(&mut server, 1, "guard", vec![decide(1)]);
    alice.link.take();
    let first = Rotation::new(45.0, 0.0);
    let second = Rotation::new(90.0, 10.0);
    send_move(&mut server, &alice, Some(east(1)), Some(first));
    send_move(&mut server, &alice, None, Some(second));
    let events = asked(&guard);
    assert_eq!(events.len(), 1);
    answer(&mut server, 1, events[0], false);
    assert_eq!(server.player_location(alice.id), Some(east(1)));

    send_move(&mut server, &alice, Some(Vec3::new(200.5, 64.0, 0.5)), None);
    assert_eq!(confirm_teleports(&mut server, &alice), [(east(1), second)]);
}

#[test]
fn two_silent_deciders_cancelling_late_get_no_warning() {
    let mut server = server(PluginSettings::default());
    let alice = join(&mut server, "Alice");
    let guard = connect_with(&mut server, 1, "guard", vec![decide(1)]);
    let judge = connect_with(&mut server, 2, "judge", vec![decide(1)]);
    let watch = connect_with(
        &mut server,
        3,
        "watch",
        vec![observe(1, EventKind::PlayerMove)],
    );
    alice.link.take();
    step(&mut server, &alice, east(1));
    wait_out_the_deadline(&mut server);
    let event = asked(&guard)[0];
    answer(&mut server, 1, event, true);
    answer(&mut server, 2, event, true);
    assert_eq!(teleported_to(&alice), [SPAWN]);
    assert_eq!(server.player_location(alice.id), Some(SPAWN));
    assert!(warnings(&guard).is_empty());
    assert!(warnings(&judge).is_empty());
    assert_eq!(
        moves(&watch),
        [
            heard_move(event, &[1], Phase::Settled, false, SPAWN, east(1)),
            heard_move(event, &[1], Phase::RolledBack, true, SPAWN, east(1)),
        ]
    );
}

#[test]
fn cancelling_a_dropped_move_gets_no_warning() {
    let mut server = server(PluginSettings::default());
    let alice = join(&mut server, "Alice");
    let guard = connect_with(&mut server, 1, "guard", vec![decide(1)]);
    let watch = connect_with(
        &mut server,
        2,
        "watch",
        vec![observe(1, EventKind::PlayerMove)],
    );
    alice.link.take();
    step(&mut server, &alice, east(1));
    step(&mut server, &alice, east(2));
    let events = asked(&guard);
    assert_eq!(events.len(), 2);
    answer(&mut server, 1, events[0], true);
    answer(&mut server, 1, events[1], true);
    wait_out_the_deadline(&mut server);
    assert_eq!(teleported_to(&alice), [SPAWN]);
    assert_eq!(server.player_location(alice.id), Some(SPAWN));
    assert!(warnings(&guard).is_empty());
    assert_eq!(
        moves(&watch),
        [
            heard_move(events[0], &[1], Phase::Settled, true, SPAWN, east(1)),
            heard_move(events[1], &[1], Phase::Settled, true, east(1), east(2)),
        ]
    );
}

#[test]
fn a_player_dying_drops_their_moves() {
    let mut server = server(PluginSettings::default());
    let alice = join(&mut server, "Alice");
    let guard = connect_with(&mut server, 1, "guard", vec![decide(1)]);
    step(&mut server, &alice, east(1));
    wait_out_the_deadline(&mut server);
    step(&mut server, &alice, east(2));
    kill(&mut server, &alice);
    alice.link.take();
    wait_out_the_deadline(&mut server);
    assert_eq!(server.player_location(alice.id), Some(east(1)));
    let events = asked(&guard);
    assert_eq!(events.len(), 2);
    for event in &events {
        answer(&mut server, 1, *event, true);
    }
    assert!(teleported_to(&alice).is_empty());
    assert_eq!(server.player_location(alice.id), Some(east(1)));
    assert_eq!(warnings(&guard), [too_late(events[0])]);
}

#[test]
fn two_deciders_settle_the_same_whatever_the_answer_order() {
    let guard_cancels = (1, true);
    let judge_allows = (2, false);
    type Answers = Vec<(u64, bool)>;
    let scenarios: [(Answers, Answers); 5] = [
        (vec![guard_cancels, judge_allows], Vec::new()),
        (vec![judge_allows, guard_cancels], Vec::new()),
        (Vec::new(), vec![guard_cancels, judge_allows]),
        (Vec::new(), vec![judge_allows, guard_cancels]),
        (vec![judge_allows], vec![guard_cancels]),
    ];
    for (before, after) in scenarios {
        let mut server = server(PluginSettings::default());
        let alice = join(&mut server, "Alice");
        let guard = connect_with(&mut server, 1, "guard", vec![decide(1)]);
        let judge = connect_with(&mut server, 2, "judge", vec![decide(1)]);
        let watch = connect_with(
            &mut server,
            3,
            "watch",
            vec![observe(1, EventKind::PlayerMove)],
        );
        alice.link.take();
        step(&mut server, &alice, east(1));
        let event = asked(&guard)[0];
        for (connection, cancel) in before {
            answer(&mut server, connection, event, cancel);
        }
        wait_out_the_deadline(&mut server);
        for (connection, cancel) in after {
            answer(&mut server, connection, event, cancel);
        }
        assert_eq!(server.player_location(alice.id), Some(SPAWN));
        assert_eq!(teleported_to(&alice), [SPAWN]);
        assert!(warnings(&guard).is_empty());
        assert!(warnings(&judge).is_empty());
        let last = moves(&watch).pop().expect("the watch hears the move");
        assert_eq!((last.id, last.cancelled), (event, true));
    }
}

#[test]
fn the_speed_check_measures_from_the_last_pending_move() {
    let mut server = server(PluginSettings::default());
    let alice = join(&mut server, "Alice");
    let guard = connect_with(&mut server, 1, "guard", vec![decide(1)]);
    alice.link.take();
    let stride = |n: u32| Vec3::new(0.5 + 10.0 * f64::from(n), 64.0, 0.5);
    for n in 1..=11 {
        step(&mut server, &alice, stride(n));
    }
    assert_eq!(asked(&guard).len(), 11);
    assert!(teleported_to(&alice).is_empty());

    step(&mut server, &alice, Vec3::new(260.5, 64.0, 0.5));
    assert_eq!(asked(&guard).len(), 11);
    assert_eq!(teleported_to(&alice), [stride(11)]);
    wait_out_the_deadline(&mut server);
    assert_eq!(server.player_location(alice.id), Some(stride(11)));
}

#[test]
fn a_rollback_reports_the_answered_moves_it_undoes() {
    let mut server = server(PluginSettings::default());
    let alice = join(&mut server, "Alice");
    let guard = connect_with(&mut server, 1, "guard", vec![decide(1)]);
    let watch = connect_with(
        &mut server,
        2,
        "watch",
        vec![observe(1, EventKind::PlayerMove)],
    );
    alice.link.take();
    step(&mut server, &alice, east(1));
    step(&mut server, &alice, east(2));
    let events = asked(&guard);
    assert_eq!(events.len(), 2);
    answer(&mut server, 1, events[1], false);
    assert_eq!(server.player_location(alice.id), Some(SPAWN));
    wait_out_the_deadline(&mut server);
    assert_eq!(server.player_location(alice.id), Some(east(2)));

    answer(&mut server, 1, events[1], true);
    let answered_twice = [format!(
        "event {} was already answered by this plugin",
        events[1]
    )];
    assert!(teleported_to(&alice).is_empty());
    assert_eq!(warnings(&guard), answered_twice);
    assert_eq!(server.player_location(alice.id), Some(east(2)));

    answer(&mut server, 1, events[0], true);
    assert_eq!(teleported_to(&alice), [SPAWN]);
    assert_eq!(server.player_location(alice.id), Some(SPAWN));
    assert_eq!(warnings(&guard), answered_twice);
    assert_eq!(
        moves(&watch),
        [
            heard_move(events[0], &[1], Phase::Settled, false, SPAWN, east(1)),
            heard_move(events[1], &[1], Phase::Settled, false, east(1), east(2)),
            heard_move(events[0], &[1], Phase::RolledBack, true, SPAWN, east(1)),
            heard_move(events[1], &[1], Phase::RolledBack, true, east(1), east(2)),
        ]
    );
}

#[test]
fn a_landing_after_a_pending_move_hurts_once() {
    let mut server = server(PluginSettings::default());
    let alice = join(&mut server, "Alice");
    let above = |y: f64| Vec3::new(0.5, y, 0.5);
    send_move_on(&mut server, &alice, Some(above(74.0)), None, false);
    let guard = connect_with(&mut server, 1, "guard", vec![decide(1)]);
    for y in [70.0, 66.0] {
        send_move_on(&mut server, &alice, Some(above(y)), None, false);
        let event = *asked(&guard).last().expect("the guard is asked");
        answer(&mut server, 1, event, false);
    }
    assert_eq!(server.player_location(alice.id), Some(above(66.0)));
    alice.link.take();

    send_move_on(&mut server, &alice, Some(SPAWN), None, true);
    let landing = *asked(&guard).last().expect("the guard is asked");
    send_move_on(
        &mut server,
        &alice,
        None,
        Some(Rotation::new(90.0, 0.0)),
        true,
    );
    assert_eq!(server.player_health(alice.id), Some(20.0));
    answer(&mut server, 1, landing, false);
    assert_eq!(server.player_location(alice.id), Some(SPAWN));
    assert_eq!(server.player_health(alice.id), Some(13.0));
    let hurt = alice
        .link
        .take()
        .into_iter()
        .filter(|event| matches!(event, ClientEvent::DamageEvent { .. }))
        .count();
    assert_eq!(hurt, 1);
}

struct CancelThenStall {
    connection: u64,
    inbound: Sender<Inbound<Link>>,
    stall: Duration,
}

impl PluginOutbox for CancelThenStall {
    fn send(&self, message: ToPlugin) {
        if let ToPlugin::Event {
            id,
            phase: Phase::Decide,
            ..
        } = message
        {
            let _ = self.inbound.send(Inbound::PluginMessage {
                connection: self.connection,
                message: FromPlugin::Decision {
                    event: id,
                    verdicts: vec![verdict(1, true, None)],
                },
            });
            std::thread::sleep(self.stall);
        }
    }
}

struct StopOnceSettled {
    mailbox: Mailbox,
    inbound: Sender<Inbound<Link>>,
}

impl PluginOutbox for StopOnceSettled {
    fn send(&self, message: ToPlugin) {
        let settled = matches!(
            message,
            ToPlugin::Event {
                phase: Phase::Settled | Phase::RolledBack,
                ..
            }
        );
        self.mailbox.send(message);
        if settled {
            let _ = self.inbound.send(Inbound::Shutdown);
        }
    }
}

#[test]
fn a_cancel_sent_at_once_stays_on_time_when_the_server_falls_behind() {
    let mut server = server(PluginSettings::default());
    let alice = join(&mut server, "Alice");
    let (inbound, receiver) = unbounded();
    plug(
        &mut server,
        1,
        "guard",
        vec![decide(1)],
        Box::new(CancelThenStall {
            connection: 1,
            inbound: inbound.clone(),
            stall: Duration::from_millis(300),
        }),
    );
    let watch = Mailbox::default();
    plug(
        &mut server,
        2,
        "watch",
        vec![observe(1, EventKind::PlayerMove)],
        Box::new(StopOnceSettled {
            mailbox: watch.clone(),
            inbound: inbound.clone(),
        }),
    );
    inbound
        .send(Inbound::Input {
            player: alice.id,
            input: PlayerInput::Move {
                position: Some(east(1)),
                rotation: None,
                on_ground: true,
            },
        })
        .expect("the server listens");
    let (finished, stop) = bounded::<()>(0);
    let watchdog = std::thread::spawn(move || {
        if stop.recv_timeout(Duration::from_secs(10)) == Err(RecvTimeoutError::Timeout) {
            let _ = inbound.send(Inbound::Shutdown);
        }
    });

    server.run(&receiver);
    drop(finished);
    let _ = watchdog.join();

    let heard = moves(&watch);
    assert_eq!(
        heard,
        [heard_move(
            heard.first().map_or(0, |heard| heard.id),
            &[1],
            Phase::Settled,
            true,
            SPAWN,
            east(1)
        )]
    );
}

fn south(blocks: u32) -> Vec3 {
    Vec3::new(0.5, 64.0, 0.5 + f64::from(blocks))
}

fn late_patch(event: u64) -> String {
    format!("patch of event {event} arrived too late")
}

fn permutations(count: usize) -> Vec<Vec<usize>> {
    (0..count).fold(vec![Vec::new()], |known, next| {
        known
            .into_iter()
            .flat_map(|order: Vec<usize>| {
                (0..=order.len()).map(move |at| {
                    let mut longer = order.clone();
                    longer.insert(at, next);
                    longer
                })
            })
            .collect()
    })
}

#[test]
fn two_plugins_that_disagree_on_the_destination_settle_on_the_higher_priority() {
    for low_first in [true, false] {
        let mut server = server(PluginSettings::default());
        let alice = join(&mut server, "Alice");
        let lower = connect_with(&mut server, 1, "lower", vec![decider(1, Priority::Low)]);
        connect_with(&mut server, 2, "higher", vec![decider(1, Priority::High)]);
        alice.link.take();
        step(&mut server, &alice, east(1));
        let event = asked(&lower)[0];
        let mut rulings = vec![(1, south(2)), (2, south(4))];
        if !low_first {
            rulings.reverse();
        }
        for (connection, to) in rulings {
            rule(
                &mut server,
                connection,
                event,
                vec![verdict(1, false, Some(to))],
            );
        }
        assert_eq!(server.player_location(alice.id), Some(south(4)));
        assert_eq!(teleported_to(&alice), [south(4)]);
    }
}

#[derive(Debug, PartialEq)]
struct Settled {
    location: Option<Vec3>,
    cancelled: bool,
    to: Vec3,
}

fn settle_four_plugins(rulings: &[(u64, Vec<Verdict>)], arrival: &[usize]) -> Settled {
    let mut server = server(PluginSettings {
        order: vec!["bravo".to_owned(), "alpha".to_owned()],
        overrides: vec![PriorityOverride {
            plugin: "delta".to_owned(),
            event: Some(EventKind::PlayerMove),
            priority: Priority::Low,
        }],
        ..PluginSettings::default()
    });
    let alice = join(&mut server, "Alice");
    let plugins = [
        connect_with(
            &mut server,
            1,
            "alpha",
            vec![decider(1, Priority::Lowest), decider(2, Priority::High)],
        ),
        connect_with(
            &mut server,
            2,
            "bravo",
            vec![decider(1, Priority::High), decider(2, Priority::Normal)],
        ),
        connect_with(
            &mut server,
            3,
            "charlie",
            vec![decider(1, Priority::Normal)],
        ),
        connect_with(&mut server, 4, "delta", vec![decider(1, Priority::Highest)]),
    ];
    let watch = connect_with(
        &mut server,
        5,
        "watch",
        vec![observe(1, EventKind::PlayerMove)],
    );
    alice.link.take();
    step(&mut server, &alice, east(1));
    let event = asked(&plugins[0])[0];
    for (count, index) in arrival.iter().enumerate() {
        if count == 2 {
            ticks(&mut server, 1);
        }
        let (connection, verdicts) = &rulings[*index];
        rule(&mut server, *connection, event, verdicts.clone());
    }
    for plugin in &plugins {
        assert!(warnings(plugin).is_empty());
    }
    let last = moves(&watch)
        .pop()
        .expect("the watch hears the settled move");
    assert_eq!((last.id, last.phase), (event, Phase::Settled));
    Settled {
        location: server.player_location(alice.id),
        cancelled: last.cancelled,
        to: last.to,
    }
}

#[test]
fn four_plugins_settle_the_same_for_every_arrival_order() {
    let patches = vec![
        (
            1,
            vec![
                verdict(2, false, Some(south(2))),
                verdict(1, false, Some(south(1))),
            ],
        ),
        (
            2,
            vec![verdict(1, false, Some(south(3))), verdict(2, false, None)],
        ),
        (3, vec![verdict(1, false, Some(south(4)))]),
        (4, vec![verdict(1, false, Some(south(5)))]),
    ];
    let mut with_a_cancel = patches.clone();
    with_a_cancel[2].1 = vec![verdict(1, true, Some(south(4)))];
    let orders = permutations(patches.len());
    assert_eq!(orders.len(), 24);
    for (rulings, expected) in [
        (
            &patches,
            Settled {
                location: Some(south(2)),
                cancelled: false,
                to: south(2),
            },
        ),
        (
            &with_a_cancel,
            Settled {
                location: Some(SPAWN),
                cancelled: true,
                to: east(1),
            },
        ),
    ] {
        for arrival in &orders {
            assert_eq!(
                settle_four_plugins(rulings, arrival),
                expected,
                "arrival order {arrival:?}"
            );
        }
    }
}

#[test]
fn a_cancel_at_lowest_wins_over_a_patch_at_highest() {
    for cancel_first in [true, false] {
        let mut server = server(PluginSettings::default());
        let alice = join(&mut server, "Alice");
        let guard = connect_with(&mut server, 1, "guard", vec![decider(1, Priority::Lowest)]);
        let mover = connect_with(&mut server, 2, "mover", vec![decider(1, Priority::Highest)]);
        let watch = connect_with(
            &mut server,
            3,
            "watch",
            vec![observe(1, EventKind::PlayerMove)],
        );
        alice.link.take();
        step(&mut server, &alice, east(1));
        let event = asked(&guard)[0];
        let mut rulings = vec![
            (1, verdict(1, true, None)),
            (2, verdict(1, false, Some(south(3)))),
        ];
        if !cancel_first {
            rulings.reverse();
        }
        for (connection, ruling) in rulings {
            rule(&mut server, connection, event, vec![ruling]);
        }
        assert_eq!(server.player_location(alice.id), Some(SPAWN));
        assert_eq!(teleported_to(&alice), [SPAWN]);
        assert_eq!(
            moves(&watch),
            [heard_move(
                event,
                &[1],
                Phase::Settled,
                true,
                SPAWN,
                east(1)
            )]
        );
        assert!(warnings(&guard).is_empty());
        assert!(warnings(&mover).is_empty());
    }
}

#[test]
fn a_patch_after_the_deadline_changes_nothing_and_warns() {
    let mut server = server(PluginSettings::default());
    let alice = join(&mut server, "Alice");
    let guard = connect_with(&mut server, 1, "guard", vec![decide(1)]);
    let watch = connect_with(
        &mut server,
        2,
        "watch",
        vec![observe(1, EventKind::PlayerMove)],
    );
    alice.link.take();
    step(&mut server, &alice, east(1));
    wait_out_the_deadline(&mut server);
    let within = asked(&guard)[0];
    rule(
        &mut server,
        1,
        within,
        vec![verdict(1, false, Some(south(3)))],
    );
    assert_eq!(server.player_location(alice.id), Some(east(1)));

    step(&mut server, &alice, east(2));
    ticks(&mut server, 21);
    let after = asked(&guard)[1];
    rule(
        &mut server,
        1,
        after,
        vec![verdict(1, false, Some(south(3)))],
    );

    assert_eq!(server.player_location(alice.id), Some(east(2)));
    assert!(teleported_to(&alice).is_empty());
    assert_eq!(warnings(&guard), [late_patch(within), late_patch(after)]);
    assert_eq!(
        moves(&watch),
        [
            heard_move(within, &[1], Phase::Settled, false, SPAWN, east(1)),
            heard_move(after, &[1], Phase::Settled, false, east(1), east(2)),
        ]
    );
}

#[test]
fn a_late_cancel_that_also_patches_still_rolls_back() {
    let mut server = server(PluginSettings::default());
    let alice = join(&mut server, "Alice");
    let guard = connect_with(&mut server, 1, "guard", vec![decide(1)]);
    let watch = connect_with(
        &mut server,
        2,
        "watch",
        vec![observe(1, EventKind::PlayerMove)],
    );
    alice.link.take();
    step(&mut server, &alice, east(1));
    wait_out_the_deadline(&mut server);
    let event = asked(&guard)[0];
    rule(
        &mut server,
        1,
        event,
        vec![verdict(1, true, Some(south(3)))],
    );
    assert_eq!(teleported_to(&alice), [SPAWN]);
    assert_eq!(server.player_location(alice.id), Some(SPAWN));
    assert!(warnings(&guard).is_empty());
    assert_eq!(
        moves(&watch),
        [
            heard_move(event, &[1], Phase::Settled, false, SPAWN, east(1)),
            heard_move(event, &[1], Phase::RolledBack, true, SPAWN, east(1)),
        ]
    );
}

#[test]
fn observers_hear_the_settled_move_with_the_changed_destination() {
    let mut server = server(PluginSettings::default());
    let alice = join(&mut server, "Alice");
    let guard = connect_with(&mut server, 1, "guard", vec![decide(1)]);
    let watch = connect_with(
        &mut server,
        2,
        "watch",
        vec![observe(1, EventKind::PlayerMove)],
    );
    alice.link.take();
    step(&mut server, &alice, east(1));
    let event = asked(&guard)[0];
    rule(
        &mut server,
        1,
        event,
        vec![verdict(1, false, Some(south(3)))],
    );
    assert_eq!(server.player_location(alice.id), Some(south(3)));
    assert_eq!(teleported_to(&alice), [south(3)]);
    assert_eq!(
        moves(&watch),
        [heard_move(
            event,
            &[1],
            Phase::Settled,
            false,
            SPAWN,
            south(3)
        )]
    );
    assert!(warnings(&guard).is_empty());
}

#[test]
fn a_changed_destination_drops_the_queued_moves_as_a_cancel_does() {
    let mut server = server(PluginSettings::default());
    let alice = join(&mut server, "Alice");
    let guard = connect_with(&mut server, 1, "guard", vec![decide(1)]);
    let watch = connect_with(
        &mut server,
        2,
        "watch",
        vec![observe(1, EventKind::PlayerMove)],
    );
    alice.link.take();
    step(&mut server, &alice, east(1));
    send_move(&mut server, &alice, None, Some(Rotation::new(90.0, 0.0)));
    step(&mut server, &alice, east(2));
    let events = asked(&guard);
    assert_eq!(events.len(), 2);
    rule(
        &mut server,
        1,
        events[0],
        vec![verdict(1, false, Some(south(3)))],
    );
    assert_eq!(server.player_location(alice.id), Some(south(3)));
    assert_eq!(
        confirm_teleports(&mut server, &alice),
        [(south(3), Rotation::default())]
    );
    answer(&mut server, 1, events[1], false);
    wait_out_the_deadline(&mut server);
    assert_eq!(server.player_location(alice.id), Some(south(3)));
    assert!(warnings(&guard).is_empty());
    assert_eq!(
        moves(&watch),
        [
            heard_move(events[0], &[1], Phase::Settled, false, SPAWN, south(3)),
            heard_move(events[1], &[1], Phase::Settled, true, east(1), east(2)),
        ]
    );
}

#[test]
fn a_destination_set_to_where_the_player_was_going_is_a_plain_move() {
    let mut server = server(PluginSettings::default());
    let alice = join(&mut server, "Alice");
    let guard = connect_with(&mut server, 1, "guard", vec![decide(1)]);
    alice.link.take();
    step(&mut server, &alice, east(1));
    step(&mut server, &alice, east(2));
    let events = asked(&guard);
    rule(
        &mut server,
        1,
        events[0],
        vec![verdict(1, false, Some(east(1)))],
    );
    assert_eq!(server.player_location(alice.id), Some(east(1)));
    answer(&mut server, 1, events[1], false);
    assert_eq!(server.player_location(alice.id), Some(east(2)));
    assert!(teleported_to(&alice).is_empty());
}

#[test]
fn a_destination_outside_the_world_is_dropped_and_the_move_applies() {
    let mut server = server(PluginSettings::default());
    let alice = join(&mut server, "Alice");
    let guard = connect_with(&mut server, 1, "guard", vec![decide(1)]);
    let watch = connect_with(
        &mut server,
        2,
        "watch",
        vec![observe(1, EventKind::PlayerMove)],
    );
    alice.link.take();
    step(&mut server, &alice, east(1));
    let event = asked(&guard)[0];
    let nowhere = Vec3::new(f64::NAN, 64.0, 0.5);
    rule(
        &mut server,
        1,
        event,
        vec![verdict(1, false, Some(nowhere))],
    );
    assert_eq!(server.player_location(alice.id), Some(east(1)));
    assert!(teleported_to(&alice).is_empty());
    assert_eq!(
        warnings(&guard),
        [format!(
            "patch of event {event} was refused: position NaN 64 0.5 is outside the world"
        )]
    );
    assert_eq!(
        moves(&watch),
        [heard_move(
            event,
            &[1],
            Phase::Settled,
            false,
            SPAWN,
            east(1)
        )]
    );
}

#[test]
fn a_refused_destination_leaves_the_last_valid_one_to_win() {
    for refused_first in [true, false] {
        let mut server = server(PluginSettings::default());
        let alice = join(&mut server, "Alice");
        let lowest = connect_with(&mut server, 1, "lowest", vec![decider(1, Priority::Lowest)]);
        let highest = connect_with(
            &mut server,
            2,
            "highest",
            vec![decider(1, Priority::Highest)],
        );
        let watch = connect_with(
            &mut server,
            3,
            "watch",
            vec![observe(1, EventKind::PlayerMove)],
        );
        alice.link.take();
        step(&mut server, &alice, east(1));
        let event = asked(&lowest)[0];
        let nowhere = Vec3::new(f64::NAN, 64.0, 0.0);
        let elsewhere = Vec3::new(10.0, 64.0, 0.0);
        let mut rulings = vec![
            (1, verdict(1, false, Some(nowhere))),
            (2, verdict(1, false, Some(elsewhere))),
        ];
        if !refused_first {
            rulings.reverse();
        }
        for (connection, ruling) in rulings {
            rule(&mut server, connection, event, vec![ruling]);
        }
        assert_eq!(server.player_location(alice.id), Some(elsewhere));
        assert_eq!(teleported_to(&alice), [elsewhere]);
        assert_eq!(
            warnings(&lowest),
            [format!(
                "patch of event {event} was refused: position NaN 64 0 is outside the world"
            )]
        );
        assert!(warnings(&highest).is_empty());
        assert_eq!(
            moves(&watch),
            [heard_move(
                event,
                &[1],
                Phase::Settled,
                false,
                SPAWN,
                elsewhere
            )]
        );
    }
}

#[test]
fn verdicts_fold_by_handler_priority_across_plugins() {
    for (highest, expected) in [(Some(south(5)), south(5)), (None, south(4))] {
        let mut server = server(PluginSettings::default());
        let alice = join(&mut server, "Alice");
        let zeta = connect_with(
            &mut server,
            1,
            "zeta",
            vec![decider(1, Priority::Lowest), decider(2, Priority::Highest)],
        );
        let alpha = connect_with(&mut server, 2, "alpha", vec![decider(1, Priority::Normal)]);
        alice.link.take();
        step(&mut server, &alice, east(1));
        let decide_events: Vec<Heard> = moves(&zeta);
        assert_eq!(decide_events[0].handlers, [1, 2]);
        let event = decide_events[0].id;
        rule(
            &mut server,
            1,
            event,
            vec![
                verdict(1, false, Some(south(3))),
                verdict(2, false, highest),
            ],
        );
        rule(
            &mut server,
            2,
            event,
            vec![verdict(1, false, Some(south(4)))],
        );
        assert_eq!(server.player_location(alice.id), Some(expected));
        assert!(warnings(&zeta).is_empty());
        assert!(warnings(&alpha).is_empty());
    }
}

#[test]
fn a_verdict_for_a_handler_that_was_not_asked_is_ignored_with_a_warning() {
    let mut server = server(PluginSettings::default());
    let alice = join(&mut server, "Alice");
    let guard = connect_with(&mut server, 1, "guard", vec![decide(1)]);
    alice.link.take();
    step(&mut server, &alice, east(1));
    let event = asked(&guard)[0];
    rule(
        &mut server,
        1,
        event,
        vec![
            verdict(9, true, None),
            verdict(1, false, None),
            verdict(1, true, None),
        ],
    );
    assert_eq!(server.player_location(alice.id), Some(east(1)));
    assert!(teleported_to(&alice).is_empty());
    assert_eq!(
        warnings(&guard),
        [
            format!("handler 9 was not asked to decide event {event}"),
            format!("handler 1 answered event {event} more than once"),
        ]
    );
}

#[test]
fn a_teleport_drops_the_pending_moves_and_tells_their_observers() {
    let mut server = server(PluginSettings::default());
    let alice = join(&mut server, "Alice");
    let guard = connect_with(&mut server, 1, "guard", vec![decide(1)]);
    let watch = connect_with(
        &mut server,
        2,
        "watch",
        vec![observe(1, EventKind::PlayerMove)],
    );
    step(&mut server, &alice, east(1));
    server.handle(Inbound::Console("tp Alice 10.5 64 0.5".to_owned()));
    server.deliver();
    let events = asked(&guard);
    assert_eq!(events.len(), 1);
    assert_eq!(
        moves(&watch),
        [heard_move(
            events[0],
            &[1],
            Phase::Settled,
            true,
            SPAWN,
            east(1)
        )]
    );
    answer(&mut server, 1, events[0], true);
    assert!(warnings(&guard).is_empty());
    assert_eq!(server.player_location(alice.id), Some(east(10)));
}
