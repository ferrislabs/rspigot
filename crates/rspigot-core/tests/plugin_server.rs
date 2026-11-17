mod common;

use std::net::IpAddr;
use std::time::{Duration, Instant};

use common::*;
use crossbeam_channel::unbounded;
use futures_channel::oneshot;
use rspigot_core::config::PluginSettings;
use rspigot_core::events::{ClientEvent, Inbound, PlayerInput};
use rspigot_core::plugins::events::{
    BroadcastMessage, BroadcastMessagePatch, PlayerCommandSend, PlayerCommandSendPatch,
    PluginDisable, PluginEnable, ServerCommand, ServerCommandPatch, ServerListPing,
    ServerListPingPatch, ServerLoad, ServerLoadType, TabComplete, TabCompletePatch,
};
use rspigot_core::plugins::{
    EventKind, EventPatch, EventPayload, FromPlugin, HandlerSpec, ListDiff, Order, PlayerSnapshot,
    PluginInfo, PluginIntro, Priority, SenderView, ServerOrder, ToPlugin,
};
use rspigot_core::server::Server;
use rspigot_core::status::{PingHook, PlayerSample, StatusBoard, StatusPing, StatusReply};

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn info(name: &str, version: &str) -> PluginInfo {
    PluginInfo {
        name: name.to_owned(),
        version: version.to_owned(),
    }
}

fn arrival(
    connection: u64,
    name: &str,
    version: &str,
    handlers: Vec<HandlerSpec>,
) -> (Inbound<Link>, Mailbox) {
    let mailbox = Mailbox::default();
    let message = Inbound::PluginConnected {
        connection,
        intro: PluginIntro {
            commands: Vec::new(),
            name: name.to_owned(),
            version: version.to_owned(),
            handlers,
        },
        outbox: Box::new(mailbox.clone()),
    };
    (message, mailbox)
}

fn payloads(mailbox: &Mailbox, kind: EventKind) -> Vec<EventPayload> {
    settled_for(mailbox, kind)
        .into_iter()
        .map(|event| event.payload)
        .collect()
}

fn enabled(name: &str, version: &str) -> EventPayload {
    EventPayload::PluginEnable(PluginEnable {
        plugin: info(name, version),
    })
}

fn disabled(name: &str, version: &str) -> EventPayload {
    EventPayload::PluginDisable(PluginDisable {
        plugin: info(name, version),
    })
}

fn lifecycle(mailbox: &Mailbox) -> Vec<EventPayload> {
    fired(mailbox)
        .into_iter()
        .map(|event| event.payload)
        .filter(|payload| {
            matches!(
                payload,
                EventPayload::PluginEnable(_) | EventPayload::PluginDisable(_)
            )
        })
        .collect()
}

fn wait_until(what: &str, done: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !done() {
        assert!(Instant::now() < deadline, "gave up waiting until {what}");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn the_server_load_reaches_the_plugins_connected_when_the_server_runs() {
    let mut server = server(PluginSettings::default());
    let early = connect_with(
        &mut server,
        1,
        "early",
        vec![observe(1, EventKind::ServerLoad)],
    );
    let (sender, inbox) = unbounded();
    let (late_arrival, late) = arrival(2, "late", "1.0.0", vec![observe(1, EventKind::ServerLoad)]);
    sender.send(late_arrival).expect("open");
    sender.send(Inbound::Shutdown).expect("open");
    server.run(&inbox);
    let startup = EventPayload::ServerLoad(ServerLoad {
        load_type: ServerLoadType::Startup,
    });
    for mailbox in [&early, &late] {
        assert_eq!(
            payloads(mailbox, EventKind::ServerLoad),
            std::slice::from_ref(&startup)
        );
    }
}

#[test]
fn the_server_load_is_not_replayed_to_a_plugin_that_connects_later() {
    let mut server = server(PluginSettings::default());
    let early = connect_with(
        &mut server,
        1,
        "early",
        vec![observe(1, EventKind::ServerLoad)],
    );
    let (sender, inbox) = unbounded();
    let running = std::thread::spawn(move || server.run(&inbox));
    wait_until("the server loaded", || {
        settled_for(&early, EventKind::ServerLoad).len() == 1
    });
    let (late_arrival, late) = arrival(2, "late", "1.0.0", vec![observe(1, EventKind::ServerLoad)]);
    sender.send(late_arrival).expect("open");
    wait_until("the late plugin is accepted", || {
        late.received().contains(&ToPlugin::Accepted)
    });
    sender.send(Inbound::Shutdown).expect("open");
    running.join().expect("the server stops");
    assert!(settled_for(&late, EventKind::ServerLoad).is_empty());
    assert_eq!(settled_for(&early, EventKind::ServerLoad).len(), 1);
}

#[test]
fn connecting_a_plugin_fires_plugin_enable_for_the_other_plugins_only() {
    let mut server = server(PluginSettings::default());
    let watch = vec![observe(1, EventKind::PluginEnable)];
    let alpha = connect_with(&mut server, 1, "alpha", watch.clone());
    let beta = connect_with(&mut server, 2, "beta", watch);
    assert_eq!(lifecycle(&alpha), [enabled("beta", "1.0.0")]);
    assert!(fired(&beta).is_empty());
    assert_eq!(beta.received().first(), Some(&ToPlugin::Accepted));
}

#[test]
fn disconnecting_a_plugin_fires_plugin_disable_for_the_plugins_left() {
    let mut server = server(PluginSettings::default());
    let watch = vec![observe(1, EventKind::PluginDisable)];
    let alpha = connect_with(&mut server, 1, "alpha", watch.clone());
    let beta = connect_with(&mut server, 2, "beta", watch);
    server.handle(Inbound::PluginDisconnected { connection: 2 });
    server.handle(Inbound::PluginDisconnected { connection: 2 });
    assert_eq!(lifecycle(&alpha), [disabled("beta", "1.0.0")]);
    assert!(fired(&beta).is_empty());
}

#[test]
fn a_plugin_that_reconnects_under_its_name_is_disabled_then_enabled_for_the_others() {
    let mut server = server(PluginSettings::default());
    let watch = vec![
        observe(1, EventKind::PluginEnable),
        observe(2, EventKind::PluginDisable),
    ];
    let alpha = connect_with(&mut server, 1, "alpha", watch.clone());
    let (first, old) = arrival(2, "beta", "1.0.0", watch.clone());
    server.handle(first);
    let (second, new) = arrival(3, "beta", "2.0.0", watch);
    server.handle(second);
    server.handle(Inbound::PluginDisconnected { connection: 2 });
    assert_eq!(
        lifecycle(&alpha),
        [
            enabled("beta", "1.0.0"),
            disabled("beta", "1.0.0"),
            enabled("beta", "2.0.0"),
        ]
    );
    assert_eq!(old.received(), [ToPlugin::Accepted, ToPlugin::Replaced]);
    assert_eq!(new.received(), [ToPlugin::Accepted]);
}

#[test]
fn at_shutdown_every_plugin_hears_the_plugin_disable_of_the_others_last() {
    let mut server = server(PluginSettings::default());
    let watch = vec![
        observe(1, EventKind::PluginDisable),
        observe(2, EventKind::WorldSave),
    ];
    let names = ["alpha", "beta", "gamma"];
    let mailboxes: Vec<Mailbox> = (1..)
        .zip(names)
        .map(|(connection, name)| connect_with(&mut server, connection, name, watch.clone()))
        .collect();
    let (sender, inbox) = unbounded();
    sender.send(Inbound::Shutdown).expect("open");
    server.run(&inbox);
    for (mailbox, own) in mailboxes.iter().zip(names) {
        let others: Vec<EventPayload> = names
            .iter()
            .rev()
            .filter(|name| **name != own)
            .map(|name| disabled(name, "1.0.0"))
            .collect();
        let kinds: Vec<EventKind> = fired(mailbox)
            .iter()
            .map(|event| event.payload.kind())
            .collect();
        assert_eq!(
            kinds,
            [
                EventKind::WorldSave,
                EventKind::PluginDisable,
                EventKind::PluginDisable
            ],
            "{own}"
        );
        assert_eq!(lifecycle(mailbox), others, "{own}");
    }
}

#[test]
fn a_console_line_fires_server_command_then_runs() {
    let mut s = scene_with(&["Alice"], vec![observe(1, EventKind::ServerCommand)]);
    let alice = s.players[0].clone();
    console(&mut s, "say hi");
    assert_eq!(
        payloads(&s.plugin, EventKind::ServerCommand),
        [EventPayload::ServerCommand(ServerCommand {
            command: "say hi".to_owned(),
            sender: SenderView::Console,
        })]
    );
    assert_eq!(system_texts(&alice), ["[Server] hi"]);
}

#[test]
fn a_cancelled_server_command_runs_nothing() {
    let mut s = scene_with(&["Alice"], vec![deciding(1, EventKind::ServerCommand)]);
    let alice = s.players[0].clone();
    console(&mut s, "say hi");
    let asked = last_asked(&s.plugin, EventKind::ServerCommand);
    assert!(system_texts(&alice).is_empty());
    rule_on(&mut s.server, 1, asked.id, true, None);
    assert!(system_texts(&alice).is_empty());
    s.tick(5);
    assert!(system_texts(&alice).is_empty());
}

#[test]
fn a_changed_server_command_runs_the_new_line() {
    let mut s = scene_with(&["Alice"], vec![deciding(1, EventKind::ServerCommand)]);
    let alice = s.players[0].clone();
    console(&mut s, "say hi");
    let asked = last_asked(&s.plugin, EventKind::ServerCommand);
    rule_on(
        &mut s.server,
        1,
        asked.id,
        false,
        Some(EventPatch::ServerCommand(ServerCommandPatch {
            command: Some("say bye".to_owned()),
        })),
    );
    assert_eq!(system_texts(&alice), ["[Server] bye"]);
}

#[test]
fn console_lines_run_in_the_order_they_were_typed() {
    let mut s = scene_with(&["Alice"], vec![deciding(1, EventKind::ServerCommand)]);
    let alice = s.players[0].clone();
    console(&mut s, "say one");
    console(&mut s, "say two");
    let asked = asked_for(&s.plugin, EventKind::ServerCommand);
    assert_eq!(asked.len(), 2);
    rule_on(&mut s.server, 1, asked[1].id, false, None);
    assert!(system_texts(&alice).is_empty());
    rule_on(&mut s.server, 1, asked[0].id, false, None);
    assert_eq!(system_texts(&alice), ["[Server] one", "[Server] two"]);
}

#[test]
fn a_console_line_parked_behind_stop_does_not_run() {
    let mut s = scene_with(&["Alice"], vec![deciding(1, EventKind::ServerCommand)]);
    let alice = s.players[0].clone();
    console(&mut s, "stop");
    console(&mut s, "say late");
    let asked = asked_for(&s.plugin, EventKind::ServerCommand);
    assert_eq!(asked.len(), 2);
    rule_on(&mut s.server, 1, asked[1].id, false, None);
    rule_on(&mut s.server, 1, asked[0].id, false, None);
    assert!(!s.server.is_running());
    assert!(system_texts(&alice).is_empty());
}

#[test]
fn neither_a_dispatched_command_nor_a_player_command_fires_server_command() {
    let mut s = scene_with(&["Alice"], vec![observe(1, EventKind::ServerCommand)]);
    let alice = s.players[0].clone();
    s.act(Order::Server(ServerOrder::DispatchCommand(
        "say from a plugin".to_owned(),
    )));
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Command("help".to_owned()),
    );
    assert!(fired(&s.plugin).is_empty());
    assert!(system_texts(&alice).contains(&"[Server] from a plugin".to_owned()));
}

fn suggest(s: &mut Scene, player: &Player, id: i32, text: &str) {
    send_input(
        &mut s.server,
        player,
        PlayerInput::CommandSuggestion {
            id,
            text: text.to_owned(),
        },
    );
}

fn suggestions(player: &Player) -> Vec<(i32, i32, i32, Vec<String>)> {
    player
        .link
        .take()
        .into_iter()
        .filter_map(|event| match event {
            ClientEvent::CommandSuggestions {
                id,
                start,
                length,
                matches,
            } => Some((id, start, length, matches)),
            _ => None,
        })
        .collect()
}

fn snapshot(s: &mut Scene, player: &Player) -> PlayerSnapshot {
    let asked = s.ask(rspigot_core::plugins::Question::Player(player.id));
    match asked {
        rspigot_core::plugins::Answer::Player(state) => state.player,
        other => panic!("expected a player, got {other:?}"),
    }
}

#[test]
fn a_tab_completion_fires_tab_complete_and_answers_once_settled() {
    let mut s = scene_with(&["Alice"], vec![deciding(1, EventKind::TabComplete)]);
    let alice = s.players[0].clone();
    let me = snapshot(&mut s, &alice);
    suggest(&mut s, &alice, 7, "/he");
    let asked = last_asked(&s.plugin, EventKind::TabComplete);
    assert_eq!(
        asked.payload,
        EventPayload::TabComplete(TabComplete {
            buffer: "/he".to_owned(),
            completions: vec!["help".to_owned()],
            sender: SenderView::Player(me),
        })
    );
    assert!(suggestions(&alice).is_empty());
    rule_on(&mut s.server, 1, asked.id, false, None);
    assert_eq!(suggestions(&alice), [(7, 1, 2, vec!["help".to_owned()])]);
}

#[test]
fn tab_complete_fires_for_vanilla_commands_too() {
    let mut s = scene_with(&["Alice", "Bob"], vec![observe(1, EventKind::TabComplete)]);
    let alice = s.players[0].clone();
    suggest(&mut s, &alice, 3, "/gamemode creative B");
    let heard = payloads(&s.plugin, EventKind::TabComplete);
    let [
        EventPayload::TabComplete(TabComplete {
            buffer,
            completions,
            ..
        }),
    ] = heard.as_slice()
    else {
        panic!("expected one tab completion, got {heard:?}");
    };
    assert_eq!(buffer, "/gamemode creative B");
    assert_eq!(completions, &["Bob".to_owned()]);
    assert_eq!(suggestions(&alice), [(3, 19, 1, vec!["Bob".to_owned()])]);
}

#[test]
fn a_cancelled_tab_complete_sends_no_suggestions() {
    let mut s = scene_with(&["Alice"], vec![deciding(1, EventKind::TabComplete)]);
    let alice = s.players[0].clone();
    suggest(&mut s, &alice, 7, "/he");
    let asked = last_asked(&s.plugin, EventKind::TabComplete);
    rule_on(&mut s.server, 1, asked.id, true, None);
    assert!(suggestions(&alice).is_empty());
}

#[test]
fn set_completions_replaces_the_suggestions_of_the_last_word() {
    let mut s = scene_with(&["Alice"], vec![deciding(1, EventKind::TabComplete)]);
    let alice = s.players[0].clone();
    suggest(&mut s, &alice, 9, "/tp Al");
    let asked = last_asked(&s.plugin, EventKind::TabComplete);
    let EventPayload::TabComplete(TabComplete { completions, .. }) = &asked.payload else {
        panic!("expected a tab completion");
    };
    let chosen = vec!["Zed".to_owned(), "Amy".to_owned()];
    rule_on(
        &mut s.server,
        1,
        asked.id,
        false,
        Some(EventPatch::TabComplete(TabCompletePatch {
            completions: replacing(completions, chosen.clone()),
        })),
    );
    assert_eq!(suggestions(&alice), [(9, 4, 2, chosen)]);
}

fn broadcast(s: &mut Scene, text: &str) {
    s.act(Order::Server(ServerOrder::Broadcast(text.to_owned())));
}

#[test]
fn a_plugin_broadcast_fires_broadcast_message_for_the_console_and_every_player() {
    let mut s = scene_with(
        &["Alice", "Bob"],
        vec![observe(1, EventKind::BroadcastMessage)],
    );
    let (alice, bob) = (s.players[0].clone(), s.players[1].clone());
    let (alice_now, bob_now) = (snapshot(&mut s, &alice), snapshot(&mut s, &bob));
    broadcast(&mut s, "§aHello");
    assert_eq!(
        payloads(&s.plugin, EventKind::BroadcastMessage),
        [EventPayload::BroadcastMessage(BroadcastMessage {
            message: "§aHello".to_owned(),
            recipients: vec![
                SenderView::Console,
                SenderView::Player(alice_now),
                SenderView::Player(bob_now),
            ],
        })]
    );
    assert_eq!(system_texts(&alice), ["Hello"]);
    assert_eq!(system_texts(&bob), ["Hello"]);
}

#[test]
fn a_cancelled_broadcast_reaches_nobody() {
    let mut s = scene_with(&["Alice"], vec![deciding(1, EventKind::BroadcastMessage)]);
    let alice = s.players[0].clone();
    broadcast(&mut s, "Hello");
    let asked = last_asked(&s.plugin, EventKind::BroadcastMessage);
    assert!(system_texts(&alice).is_empty());
    rule_on(&mut s.server, 1, asked.id, true, None);
    assert!(system_texts(&alice).is_empty());
}

#[test]
fn a_changed_broadcast_reaches_the_settled_recipients_with_the_settled_message() {
    let mut s = scene_with(
        &["Alice", "Bob"],
        vec![deciding(1, EventKind::BroadcastMessage)],
    );
    let (alice, bob) = (s.players[0].clone(), s.players[1].clone());
    let alice_now = snapshot(&mut s, &alice);
    broadcast(&mut s, "Hello");
    let asked = last_asked(&s.plugin, EventKind::BroadcastMessage);
    let EventPayload::BroadcastMessage(BroadcastMessage { recipients, .. }) = &asked.payload else {
        panic!("expected a broadcast");
    };
    rule_on(
        &mut s.server,
        1,
        asked.id,
        false,
        Some(EventPatch::BroadcastMessage(BroadcastMessagePatch {
            message: Some("§cBye".to_owned()),
            recipients: removing(recipients, |sender| {
                *sender != SenderView::Player(alice_now.clone())
            }),
        })),
    );
    assert_eq!(system_texts(&alice), ["Bye"]);
    assert!(system_texts(&bob).is_empty());
}

#[test]
fn say_and_me_fire_no_broadcast_message() {
    let mut s = scene_with(&["Alice"], vec![observe(1, EventKind::BroadcastMessage)]);
    let alice = s.players[0].clone();
    console(&mut s, "say hi");
    console(&mut s, "me waves");
    assert!(fired(&s.plugin).is_empty());
    assert_eq!(system_texts(&alice), ["[Server] hi", "* Server waves"]);
}

fn plugin_broadcast(text: &str) -> Order {
    Order::Server(ServerOrder::Broadcast(text.to_owned()))
}

fn react(s: &mut Scene, connection: u64, event: u64, text: &str) {
    order(
        &mut s.server,
        connection,
        FromPlugin::Reaction {
            event,
            order: plugin_broadcast(text),
        },
    );
}

#[test]
fn a_broadcast_a_deciding_handler_sends_while_it_runs_goes_out_without_firing_again() {
    let mut s = scene_with(&["Alice"], vec![deciding(1, EventKind::BroadcastMessage)]);
    let alice = s.players[0].clone();
    broadcast(&mut s, "outer");
    let asked = last_asked(&s.plugin, EventKind::BroadcastMessage);
    react(&mut s, 1, asked.id, "inner");
    assert_eq!(system_texts(&alice), ["inner"]);
    assert_eq!(asked_for(&s.plugin, EventKind::BroadcastMessage).len(), 1);
    rule_on(&mut s.server, 1, asked.id, false, None);
    assert_eq!(system_texts(&alice), ["outer"]);
}

#[test]
fn a_deciding_handler_that_broadcasts_and_answers_after_the_deadline_does_not_loop() {
    let mut s = scene_with(&["Alice"], vec![deciding(1, EventKind::BroadcastMessage)]);
    let alice = s.players[0].clone();
    broadcast(&mut s, "outer");
    let asked = last_asked(&s.plugin, EventKind::BroadcastMessage);
    s.tick(3);
    assert_eq!(system_texts(&alice), ["outer"]);
    react(&mut s, 1, asked.id, "late");
    rule_on(&mut s.server, 1, asked.id, false, None);
    assert_eq!(system_texts(&alice), ["late"]);
    assert_eq!(asked_for(&s.plugin, EventKind::BroadcastMessage).len(), 1);
}

#[test]
fn an_observer_that_rebroadcasts_does_not_loop() {
    let mut s = scene_with(&["Alice"], vec![observe(1, EventKind::BroadcastMessage)]);
    let alice = s.players[0].clone();
    broadcast(&mut s, "outer");
    let heard = settled_for(&s.plugin, EventKind::BroadcastMessage);
    assert_eq!(heard.len(), 1);
    react(&mut s, 1, heard[0].id, "echo");
    assert_eq!(system_texts(&alice), ["outer", "echo"]);
    assert_eq!(settled_for(&s.plugin, EventKind::BroadcastMessage).len(), 1);
}

#[test]
fn an_untagged_broadcast_from_a_plugin_owing_a_verdict_fires_and_waits_behind_the_parked_one() {
    let mut s = scene_with(&["Alice"], vec![deciding(1, EventKind::BroadcastMessage)]);
    let alice = s.players[0].clone();
    let other = connect(&mut s.server, 2, "other");
    broadcast(&mut s, "outer");
    order(
        &mut s.server,
        1,
        FromPlugin::Order(plugin_broadcast("inner")),
    );
    let asked = asked_for(&s.plugin, EventKind::BroadcastMessage);
    assert_eq!(asked.len(), 2);
    rule_on(&mut s.server, 1, asked[1].id, false, None);
    assert!(system_texts(&alice).is_empty());
    rule_on(&mut s.server, 1, asked[0].id, false, None);
    assert_eq!(system_texts(&alice), ["outer", "inner"]);
    react(&mut s, 2, 999, "unknown");
    assert_eq!(asked_for(&s.plugin, EventKind::BroadcastMessage).len(), 3);
    assert!(heard(&other).is_empty());
}

fn ping_from(legacy: bool) -> StatusPing {
    StatusPing {
        address: IpAddr::from([10, 0, 0, 5]),
        hostname: if legacy {
            String::new()
        } else {
            "play.example:25565".to_owned()
        },
        motd: "A Minecraft Server".to_owned(),
        legacy,
    }
}

fn ping(s: &mut Scene, legacy: bool) -> oneshot::Receiver<StatusReply> {
    let (reply, answer) = oneshot::channel();
    s.server.handle(Inbound::StatusPing {
        ping: ping_from(legacy),
        reply: Some(reply),
    });
    s.server.deliver();
    answer
}

fn answered(answer: &mut oneshot::Receiver<StatusReply>) -> Option<StatusReply> {
    answer.try_recv().ok().flatten()
}

fn icon() -> Vec<u8> {
    let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
    png.extend_from_slice(&64u32.to_be_bytes());
    png.extend_from_slice(&64u32.to_be_bytes());
    png.extend_from_slice(&[8, 6, 0, 0, 0]);
    png
}

fn sample(names: &[&str]) -> Vec<PlayerSample> {
    names
        .iter()
        .map(|name| PlayerSample {
            id: rspigot_domain::player::PlayerId::offline(name),
            name: (*name).to_owned(),
        })
        .collect()
}

fn defaults(online: i32, names: &[&str]) -> StatusReply {
    StatusReply {
        motd: "A Minecraft Server".to_owned(),
        max_players: 20,
        online,
        sample: sample(names),
        icon: None,
    }
}

fn sorted(mut reply: StatusReply) -> StatusReply {
    reply.sample.sort_by(|a, b| a.name.cmp(&b.name));
    reply
}

#[test]
fn a_status_ping_waits_for_the_deciding_handlers_and_applies_every_field_of_their_patch() {
    let mut s = scene_with(
        &["Alice", "Bob"],
        vec![deciding(1, EventKind::ServerListPing)],
    );
    let (alice, bob) = (s.players[0].clone(), s.players[1].clone());
    let players = vec![snapshot(&mut s, &alice), snapshot(&mut s, &bob)];
    let mut answer = ping(&mut s, false);
    let asked = last_asked(&s.plugin, EventKind::ServerListPing);
    assert_eq!(
        asked.payload,
        EventPayload::ServerListPing(ServerListPing {
            address: IpAddr::from([10, 0, 0, 5]),
            hostname: "play.example:25565".to_owned(),
            players,
            max_players: 20,
            motd: "A Minecraft Server".to_owned(),
            num_players: 2,
            hidden_players: Vec::new(),
        })
    );
    assert_eq!(answered(&mut answer), None);
    rule_on(
        &mut s.server,
        1,
        asked.id,
        false,
        Some(EventPatch::ServerListPing(ServerListPingPatch {
            max_players: Some(5),
            motd: Some("§bPatched".to_owned()),
            server_icon: Some(icon()),
            hidden_players: ListDiff {
                removed: Vec::new(),
                replaced: Vec::new(),
                added: vec![alice.id.0],
            },
        })),
    );
    assert_eq!(
        answered(&mut answer),
        Some(StatusReply {
            motd: "§bPatched".to_owned(),
            max_players: 5,
            online: 1,
            sample: sample(&["Bob"]),
            icon: Some(icon()),
        })
    );
}

#[test]
fn a_ping_without_a_patch_answers_with_the_configuration() {
    let mut s = scene_with(
        &["Alice", "Bob"],
        vec![deciding(1, EventKind::ServerListPing)],
    );
    let mut answer = ping(&mut s, false);
    let asked = last_asked(&s.plugin, EventKind::ServerListPing);
    rule_on(&mut s.server, 1, asked.id, false, None);
    let reply = answered(&mut answer).expect("the ping is answered");
    assert_eq!(sorted(reply), defaults(2, &["Alice", "Bob"]));
}

#[test]
fn a_ping_whose_decider_stays_silent_is_answered_at_the_deadline() {
    let mut s = scene_with(&[], vec![deciding(1, EventKind::ServerListPing)]);
    let mut answer = ping(&mut s, false);
    s.tick(1);
    assert_eq!(answered(&mut answer), None);
    s.tick(1);
    assert_eq!(answered(&mut answer), Some(defaults(0, &[])));
}

#[test]
fn a_server_icon_that_is_not_a_64_pixel_png_is_refused_with_a_warning() {
    let mut s = scene_with(&[], vec![deciding(1, EventKind::ServerListPing)]);
    let mut answer = ping(&mut s, false);
    let asked = last_asked(&s.plugin, EventKind::ServerListPing);
    let mut wide = icon();
    wide[16..20].copy_from_slice(&128u32.to_be_bytes());
    rule_on(
        &mut s.server,
        1,
        asked.id,
        false,
        Some(EventPatch::ServerListPing(ServerListPingPatch {
            motd: Some("Patched".to_owned()),
            server_icon: Some(wide),
            ..ServerListPingPatch::default()
        })),
    );
    assert_eq!(
        s.warnings(),
        [format!(
            "patch of event {} was refused: server_icon is a 128x64 image, not 64x64",
            asked.id
        )]
    );
    assert_eq!(answered(&mut answer), Some(defaults(0, &[])));
}

#[test]
fn a_legacy_ping_shows_no_player_and_ignores_the_icon_and_the_hidden_players() {
    let mut s = scene_with(&["Alice"], vec![deciding(1, EventKind::ServerListPing)]);
    let alice = s.players[0].clone();
    let mut answer = ping(&mut s, true);
    let asked = last_asked(&s.plugin, EventKind::ServerListPing);
    let EventPayload::ServerListPing(ServerListPing {
        hostname,
        players,
        num_players,
        ..
    }) = &asked.payload
    else {
        panic!("expected a ping");
    };
    assert_eq!((hostname.as_str(), players.len(), *num_players), ("", 0, 1));
    rule_on(
        &mut s.server,
        1,
        asked.id,
        false,
        Some(EventPatch::ServerListPing(ServerListPingPatch {
            motd: Some("Old".to_owned()),
            server_icon: Some(icon()),
            hidden_players: ListDiff {
                removed: Vec::new(),
                replaced: Vec::new(),
                added: vec![alice.id.0],
            },
            ..ServerListPingPatch::default()
        })),
    );
    assert_eq!(
        answered(&mut answer),
        Some(StatusReply {
            motd: "Old".to_owned(),
            ..defaults(1, &["Alice"])
        })
    );
}

#[test]
fn a_ping_waiting_at_shutdown_is_still_answered() {
    let mut s = scene_with(&[], vec![deciding(1, EventKind::ServerListPing)]);
    let mut answer = ping(&mut s, false);
    let (sender, inbox) = unbounded();
    sender.send(Inbound::Shutdown).expect("open");
    let Scene { server, .. } = s;
    server.run(&inbox);
    assert_eq!(answered(&mut answer), Some(defaults(0, &[])));
}

#[test]
fn a_ping_still_in_the_inbox_at_shutdown_is_dropped_so_the_network_answers_alone() {
    let s = scene_with(&[], vec![deciding(1, EventKind::ServerListPing)]);
    let (reply, mut answer) = oneshot::channel();
    let (sender, inbox) = unbounded();
    sender.send(Inbound::Shutdown).expect("open");
    sender
        .send(Inbound::StatusPing {
            ping: ping_from(false),
            reply: Some(reply),
        })
        .expect("open");
    let Scene { server, .. } = s;
    server.run(&inbox);
    assert_eq!(answer.try_recv(), Err(oneshot::Canceled));
    assert!(inbox.is_empty());
}

#[test]
fn the_status_board_tells_the_network_whether_a_ping_is_announced_or_asked() {
    let status = StatusBoard::new(20);
    let mut server = Server::new(
        rspigot_core::config::GameConfig {
            chunk_workers: 1,
            spawn_chunk_radius: 0,
            ..rspigot_core::config::GameConfig::default()
        },
        rspigot_core::config::LevelData::default(),
        std::sync::Arc::new(NoStore),
        std::sync::Arc::new(Floor),
        status.clone(),
    );
    let hook = || status.snapshot().ping;
    assert_eq!(hook(), PingHook::Off);
    let watcher = connect_with(
        &mut server,
        1,
        "watcher",
        vec![observe(1, EventKind::ServerListPing)],
    );
    assert_eq!(hook(), PingHook::Notify);
    connect_with(
        &mut server,
        2,
        "decider",
        vec![deciding(1, EventKind::ServerListPing)],
    );
    assert_eq!(hook(), PingHook::Ask);
    server.handle(Inbound::PluginDisconnected { connection: 2 });
    assert_eq!(hook(), PingHook::Notify);
    server.handle(Inbound::StatusPing {
        ping: ping_from(false),
        reply: None,
    });
    assert_eq!(settled_for(&watcher, EventKind::ServerListPing).len(), 1);
    server.handle(Inbound::PluginDisconnected { connection: 1 });
    assert_eq!(hook(), PingHook::Off);
    connect_with(
        &mut server,
        3,
        "decider",
        vec![deciding(1, EventKind::ServerListPing)],
    );
    let (sender, inbox) = unbounded();
    sender.send(Inbound::Shutdown).expect("open");
    server.run(&inbox);
    assert_eq!(hook(), PingHook::Off);
}

#[test]
fn a_command_send_patch_prunes_a_copy_for_that_player_only() {
    let mut s = scene_with(&[], vec![deciding(1, EventKind::PlayerCommandSend)]);
    let alice = join(&mut s.server, "Alice");
    let asked = last_asked(&s.plugin, EventKind::PlayerCommandSend);
    let EventPayload::PlayerCommandSend(PlayerCommandSend { commands, .. }) = &asked.payload else {
        panic!("expected a command send");
    };
    alice.link.take();
    rule_on(
        &mut s.server,
        1,
        asked.id,
        false,
        Some(EventPatch::PlayerCommandSend(PlayerCommandSendPatch {
            commands: removing(commands, |name| name != "help"),
        })),
    );
    assert_eq!(command_roots(&alice), [vec!["help".to_owned()]]);
    let bob = join(&mut s.server, "Bob");
    let asked = last_asked(&s.plugin, EventKind::PlayerCommandSend);
    bob.link.take();
    rule_on(&mut s.server, 1, asked.id, false, None);
    let roots = command_roots(&bob);
    assert_eq!(roots.len(), 1);
    assert!(roots[0].len() > 1);
    assert!(roots[0].contains(&"help".to_owned()));
}

fn command_roots(player: &Player) -> Vec<Vec<String>> {
    player
        .link
        .take()
        .into_iter()
        .filter_map(|event| match event {
            ClientEvent::Commands(tree) => Some(
                tree.nodes[tree.root]
                    .children
                    .iter()
                    .filter_map(|child| match &tree.nodes[*child].kind {
                        rspigot_core::commands::CommandNodeKind::Literal(name) => {
                            Some(name.clone())
                        }
                        _ => None,
                    })
                    .collect(),
            ),
            _ => None,
        })
        .collect()
}

fn two_deciders(names: &[&str], event: EventKind) -> Scene {
    let ranked = |priority| HandlerSpec {
        priority,
        ..deciding(1, event)
    };
    let mut s = scene_with(names, vec![ranked(Priority::Low)]);
    connect_with(&mut s.server, 2, "second", vec![ranked(Priority::High)]);
    s
}

#[test]
fn two_plugins_that_each_hide_a_player_from_the_ping_both_keep_them_hidden() {
    for later_answers_first in [false, true] {
        let mut s = two_deciders(&["Alice", "Bob", "Carol"], EventKind::ServerListPing);
        let (alice, bob) = (s.players[0].clone(), s.players[1].clone());
        let mut answer = ping(&mut s, false);
        let asked = last_asked(&s.plugin, EventKind::ServerListPing);
        let hiding = |player: &Player| {
            EventPatch::ServerListPing(ServerListPingPatch {
                hidden_players: ListDiff {
                    removed: Vec::new(),
                    replaced: Vec::new(),
                    added: vec![player.id.0],
                },
                ..ServerListPingPatch::default()
            })
        };
        let mut answers = vec![(1, hiding(&alice)), (2, hiding(&bob))];
        if later_answers_first {
            answers.reverse();
        }
        for (connection, patch) in answers {
            rule_on(&mut s.server, connection, asked.id, false, Some(patch));
        }
        assert_eq!(
            answered(&mut answer).map(sorted),
            Some(defaults(1, &["Carol"])),
            "{later_answers_first}"
        );
    }
}

#[test]
fn two_plugins_that_each_remove_a_broadcast_recipient_both_keep_it_from_them() {
    for later_answers_first in [false, true] {
        let mut s = two_deciders(&["Alice", "Bob", "Carol"], EventKind::BroadcastMessage);
        let [alice, bob, carol] = [0, 1, 2].map(|index| s.players[index].clone());
        broadcast(&mut s, "Hello");
        let asked = last_asked(&s.plugin, EventKind::BroadcastMessage);
        let EventPayload::BroadcastMessage(BroadcastMessage { recipients, .. }) = &asked.payload
        else {
            panic!("expected a broadcast");
        };
        let without = |player: &Player| {
            EventPatch::BroadcastMessage(BroadcastMessagePatch {
                recipients: removing(
                    recipients,
                    |sender| matches!(sender, SenderView::Player(snapshot) if snapshot.id == player.id),
                ),
                ..BroadcastMessagePatch::default()
            })
        };
        let mut answers = vec![(1, without(&bob)), (2, without(&carol))];
        if later_answers_first {
            answers.reverse();
        }
        for (connection, patch) in answers {
            rule_on(&mut s.server, connection, asked.id, false, Some(patch));
        }
        assert_eq!(system_texts(&alice), ["Hello"]);
        assert!(system_texts(&bob).is_empty(), "{later_answers_first}");
        assert!(system_texts(&carol).is_empty(), "{later_answers_first}");
    }
}
