mod common;

use std::net::IpAddr;

use common::*;
use crossbeam_channel::bounded;
use rspigot_core::config::PluginSettings;
use rspigot_core::console::console_text;
use rspigot_core::events::{ClientEvent, Inbound, PlayerInput, PreLoginVerdict};
use rspigot_core::plugins::events::{
    AsyncPlayerPreLogin, AsyncPlayerPreLoginPatch, AsyncPlayerPreLoginResult, MainHand,
    PlayerChangedMainHand, PlayerCommandSend, PlayerCommandSendPatch, PlayerDeathPatch, PlayerJoin,
    PlayerJoinPatch, PlayerKick, PlayerKickPatch, PlayerLeave, PlayerLeavePatch,
    PlayerLocaleChange, PlayerLogin, PlayerLoginPatch, PlayerLoginResult,
};
use rspigot_core::plugins::{
    EventKind, EventPatch, EventPayload, HandlerSpec, Phase, PlayerOrder, Priority,
};
use rspigot_core::ports::ClientLink;
use rspigot_domain::player::{ClientSettings, MainHand as ClientHand, PlayerId};
use rspigot_domain::text::{Component, Content};

fn french_left_handed() -> ClientSettings {
    ClientSettings {
        locale: "fr_fr".to_owned(),
        main_hand: ClientHand::Left,
        ..ClientSettings::default()
    }
}

fn session_watch() -> Vec<rspigot_core::plugins::HandlerSpec> {
    [
        EventKind::PlayerLogin,
        EventKind::PlayerChangedMainHand,
        EventKind::PlayerLocaleChange,
        EventKind::PlayerCommandSend,
        EventKind::PlayerJoin,
        EventKind::PlayerKick,
        EventKind::PlayerLeave,
    ]
    .into_iter()
    .zip(1..)
    .map(|(kind, id)| observe(id, kind))
    .collect()
}

fn announced(player: &Player) -> Vec<String> {
    player
        .link
        .take()
        .into_iter()
        .filter_map(|event| match event {
            ClientEvent::SystemChat { message, .. } => Some(message),
            _ => None,
        })
        .map(|message| match &message.content {
            Content::Translatable { key, .. } => key.clone(),
            _ => console_text(&message),
        })
        .collect()
}

#[test]
fn a_joining_player_fires_the_pre_join_events_in_the_spigot_order() {
    let mut server = server(PluginSettings::default());
    let watch = connect_with(&mut server, 1, "watch", session_watch());
    let alice = join_with(&mut server, "Alice", french_left_handed());
    assert_eq!(
        fired_kinds(&watch),
        [
            (Phase::Settled, EventKind::PlayerLogin),
            (Phase::Settled, EventKind::PlayerChangedMainHand),
            (Phase::Settled, EventKind::PlayerLocaleChange),
            (Phase::Settled, EventKind::PlayerCommandSend),
            (Phase::Settled, EventKind::PlayerJoin),
        ]
    );
    let events = fired(&watch);
    let EventPayload::PlayerLogin(PlayerLogin {
        player,
        result,
        kick_message,
        address,
        ..
    }) = &events[0].payload
    else {
        panic!("expected the login first");
    };
    assert_eq!((player.id, player.name.as_str()), (alice.id, "Alice"));
    assert_eq!(*result, PlayerLoginResult::Allowed);
    assert_eq!(kick_message, "");
    assert_eq!(*address, IpAddr::from([127, 0, 0, 1]));
    assert_eq!(
        events[1].payload,
        EventPayload::PlayerChangedMainHand(PlayerChangedMainHand {
            main_hand: MainHand::Right,
            player: player_of(&events[4].payload),
        })
    );
    let EventPayload::PlayerLocaleChange(PlayerLocaleChange { locale, .. }) = &events[2].payload
    else {
        panic!("expected the locale");
    };
    assert_eq!(locale, "fr_fr");
}

fn player_of(payload: &EventPayload) -> rspigot_core::plugins::PlayerSnapshot {
    match payload {
        EventPayload::PlayerJoin(PlayerJoin { player, .. }) => player.clone(),
        other => panic!("expected a join, got {other:?}"),
    }
}

#[test]
fn a_default_client_fires_neither_hand_nor_locale_at_join() {
    let mut server = server(PluginSettings::default());
    let watch = connect_with(&mut server, 1, "watch", session_watch());
    join(&mut server, "Alice");
    assert_eq!(
        fired_kinds(&watch),
        [
            (Phase::Settled, EventKind::PlayerLogin),
            (Phase::Settled, EventKind::PlayerCommandSend),
            (Phase::Settled, EventKind::PlayerJoin),
        ]
    );
}

#[test]
fn a_login_decider_can_refuse_a_player_with_a_literal_message() {
    let mut server = server(PluginSettings::default());
    let guard = connect_with(
        &mut server,
        1,
        "guard",
        vec![
            deciding(1, EventKind::PlayerLogin),
            observe(2, EventKind::PlayerJoin),
        ],
    );
    let alice = arrive(&mut server, "Alice");
    assert_eq!(server.players_online(), 0);
    let login = last_asked(&guard, EventKind::PlayerLogin);
    rule_on(
        &mut server,
        1,
        login.id,
        false,
        Some(EventPatch::PlayerLogin(PlayerLoginPatch {
            kick_message: Some("§cnot today".to_owned()),
            result: Some(PlayerLoginResult::KickOther),
        })),
    );
    assert_eq!(server.players_online(), 0);
    assert_eq!(
        alice.link.disconnected_with(),
        Some(Component::text("§cnot today"))
    );
    assert!(settled_for(&guard, EventKind::PlayerJoin).is_empty());
}

#[test]
fn a_login_decider_can_let_a_banned_player_in() {
    let mut server = server(PluginSettings::default());
    server.handle(Inbound::Console("ban Alice".to_owned()));
    let guard = connect_with(
        &mut server,
        1,
        "guard",
        vec![deciding(1, EventKind::PlayerLogin)],
    );
    let alice = arrive(&mut server, "Alice");
    let login = last_asked(&guard, EventKind::PlayerLogin);
    let EventPayload::PlayerLogin(PlayerLogin {
        result,
        kick_message,
        ..
    }) = &login.payload
    else {
        panic!("expected a login");
    };
    assert_eq!(*result, PlayerLoginResult::KickBanned);
    assert!(kick_message.contains("banned"), "{kick_message}");
    rule_on(
        &mut server,
        1,
        login.id,
        false,
        Some(EventPatch::PlayerLogin(PlayerLoginPatch {
            result: Some(PlayerLoginResult::Allowed),
            ..PlayerLoginPatch::default()
        })),
    );
    assert_eq!(server.players_online(), 1);
    assert!(alice.link.disconnected_with().is_none());
}

#[test]
fn a_banned_player_without_a_decider_is_refused_with_the_ban_reason() {
    let mut server = server(PluginSettings::default());
    server.handle(Inbound::Console("ban Alice".to_owned()));
    let watch = connect_with(&mut server, 1, "watch", session_watch());
    let alice = arrive(&mut server, "Alice");
    assert_eq!(server.players_online(), 0);
    let reason = alice.link.disconnected_with().expect("refused");
    assert!(
        matches!(&reason.content, Content::Translatable { key, .. } if key == "multiplayer.disconnect.banned.reason")
    );
    assert_eq!(
        fired_kinds(&watch),
        [(Phase::Settled, EventKind::PlayerLogin)]
    );
}

#[test]
fn a_disconnect_before_join_drops_the_pre_join_events() {
    let mut server = server(PluginSettings::default());
    let guard = connect_with(
        &mut server,
        1,
        "guard",
        [deciding(1, EventKind::PlayerLogin)]
            .into_iter()
            .chain(session_watch().into_iter().skip(1))
            .collect(),
    );
    let alice = arrive(&mut server, "Alice");
    let login = last_asked(&guard, EventKind::PlayerLogin);
    alice.link.disconnect(Component::text("gone"));
    server.handle(leave_of(&alice));
    rule_on(&mut server, 1, login.id, false, None);
    assert_eq!(server.players_online(), 0);
    assert_eq!(
        fired_kinds(&guard),
        [(Phase::Decide, EventKind::PlayerLogin)]
    );
}

#[test]
fn a_parked_login_lets_the_rest_of_the_join_wait_in_order() {
    let mut server = server(PluginSettings::default());
    let guard = connect_with(
        &mut server,
        1,
        "guard",
        [deciding(1, EventKind::PlayerLogin)]
            .into_iter()
            .chain(session_watch().into_iter().skip(1))
            .collect(),
    );
    let alice = arrive(&mut server, "Alice");
    assert!(alice.link.take().is_empty());
    let login = last_asked(&guard, EventKind::PlayerLogin);
    rule_on(&mut server, 1, login.id, false, None);
    settle_arrival(&mut server, &alice);
    assert_eq!(server.players_online(), 1);
    assert_eq!(
        fired_kinds(&guard),
        [
            (Phase::Decide, EventKind::PlayerLogin),
            (Phase::Settled, EventKind::PlayerCommandSend),
            (Phase::Settled, EventKind::PlayerJoin),
        ]
    );
}

#[test]
fn a_join_decider_changes_or_removes_the_join_message() {
    let mut server = server(PluginSettings::default());
    let bob = join(&mut server, "Bob");
    let greeter = connect_with(
        &mut server,
        1,
        "greeter",
        vec![deciding(1, EventKind::PlayerJoin)],
    );
    bob.link.take();
    join(&mut server, "Alice");
    assert!(announced(&bob).is_empty());
    let asked = last_asked(&greeter, EventKind::PlayerJoin);
    let EventPayload::PlayerJoin(PlayerJoin { join_message, .. }) = &asked.payload else {
        panic!("expected a join");
    };
    assert_eq!(join_message.as_deref(), Some("§eAlice joined the game"));
    rule_on(
        &mut server,
        1,
        asked.id,
        false,
        Some(EventPatch::PlayerJoin(PlayerJoinPatch {
            join_message: Some(Some("§bWelcome Alice\nsecond line".to_owned())),
        })),
    );
    assert_eq!(announced(&bob), ["Welcome Alice", "second line"]);

    join(&mut server, "Carol");
    let asked = last_asked(&greeter, EventKind::PlayerJoin);
    rule_on(
        &mut server,
        1,
        asked.id,
        false,
        Some(EventPatch::PlayerJoin(PlayerJoinPatch {
            join_message: Some(None),
        })),
    );
    assert!(announced(&bob).is_empty());

    join(&mut server, "Dave");
    let asked = last_asked(&greeter, EventKind::PlayerJoin);
    rule_on(&mut server, 1, asked.id, false, None);
    assert_eq!(announced(&bob), ["multiplayer.player.joined"]);
}

#[test]
fn a_leave_decider_changes_the_quit_message_while_the_player_is_already_gone() {
    let mut s = scene_with(&["Alice", "Bob"], vec![deciding(1, EventKind::PlayerLeave)]);
    let bob = s.players[1].clone();
    let alice = s.players[0].clone();
    s.server.handle(leave_of(&alice));
    s.server.deliver();
    assert_eq!(s.server.players_online(), 1);
    assert!(announced(&bob).is_empty());
    let leave = last_asked(&s.plugin, EventKind::PlayerLeave);
    let EventPayload::PlayerLeave(PlayerLeave { quit_message, .. }) = &leave.payload else {
        panic!("expected a leave");
    };
    assert_eq!(quit_message.as_deref(), Some("§eAlice left the game"));
    rule_on(
        &mut s.server,
        1,
        leave.id,
        false,
        Some(EventPatch::PlayerLeave(PlayerLeavePatch {
            quit_message: Some(Some("§7Alice wandered off".to_owned())),
        })),
    );
    assert_eq!(announced(&bob), ["Alice wandered off"]);
}

#[test]
fn a_leave_without_a_change_is_announced_as_before() {
    let mut s = scene_with(&["Alice", "Bob"], vec![deciding(1, EventKind::PlayerLeave)]);
    let bob = s.players[1].clone();
    let alice = s.players[0].clone();
    s.server.handle(leave_of(&alice));
    let leave = last_asked(&s.plugin, EventKind::PlayerLeave);
    rule_on(&mut s.server, 1, leave.id, false, None);
    assert_eq!(announced(&bob), ["multiplayer.player.left"]);
    let mut s = scene_with(&["Alice", "Bob"], vec![deciding(1, EventKind::PlayerLeave)]);
    let bob = s.players[1].clone();
    let alice = s.players[0].clone();
    s.server.handle(leave_of(&alice));
    let leave = last_asked(&s.plugin, EventKind::PlayerLeave);
    rule_on(
        &mut s.server,
        1,
        leave.id,
        false,
        Some(EventPatch::PlayerLeave(PlayerLeavePatch {
            quit_message: Some(Some(String::new())),
        })),
    );
    assert!(announced(&bob).is_empty());
}

#[test]
fn a_cancelled_kick_keeps_the_player_online() {
    let mut s = scene_with(&["Alice", "Bob"], vec![deciding(1, EventKind::PlayerKick)]);
    let alice = s.players[0].clone();
    s.act(to(&alice, PlayerOrder::Kick("§cBye".into())));
    assert_eq!(s.server.players_online(), 2);
    let kick = last_asked(&s.plugin, EventKind::PlayerKick);
    assert_eq!(
        kick.payload,
        EventPayload::PlayerKick(PlayerKick {
            leave_message: "§eAlice left the game.".to_owned(),
            player: snapshot_of(&kick.payload),
            reason: "§cBye".to_owned(),
        })
    );
    rule_on(&mut s.server, 1, kick.id, true, None);
    assert_eq!(s.server.players_online(), 2);
    assert!(alice.link.disconnected_with().is_none());
}

fn snapshot_of(payload: &EventPayload) -> rspigot_core::plugins::PlayerSnapshot {
    match payload {
        EventPayload::PlayerKick(PlayerKick { player, .. }) => player.clone(),
        other => panic!("expected a kick, got {other:?}"),
    }
}

#[test]
fn a_kick_decider_changes_the_reason_and_the_leave_message() {
    let mut s = scene_with(
        &["Alice", "Bob"],
        vec![
            deciding(1, EventKind::PlayerKick),
            observe(2, EventKind::PlayerLeave),
        ],
    );
    let alice = s.players[0].clone();
    let bob = s.players[1].clone();
    s.act(to(&alice, PlayerOrder::Kick("Bye".into())));
    let kick = last_asked(&s.plugin, EventKind::PlayerKick);
    rule_on(
        &mut s.server,
        1,
        kick.id,
        false,
        Some(EventPatch::PlayerKick(PlayerKickPatch {
            leave_message: Some("§cAlice was thrown out".to_owned()),
            reason: Some("§4Out".to_owned()),
        })),
    );
    assert_eq!(s.server.players_online(), 1);
    assert_eq!(
        alice.link.disconnected_with(),
        Some(Component::from_section_codes("§4Out"))
    );
    let left = settled_for(&s.plugin, EventKind::PlayerLeave);
    let EventPayload::PlayerLeave(PlayerLeave { quit_message, .. }) = &left[0].payload else {
        panic!("expected a leave");
    };
    assert_eq!(quit_message.as_deref(), Some("§cAlice was thrown out"));
    assert_eq!(announced(&bob), ["Alice was thrown out"]);
}

#[test]
fn an_anti_cheat_disconnect_goes_through_the_kick() {
    let mut s = scene_with(&["Alice"], vec![deciding(1, EventKind::PlayerKick)]);
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Chat("bad\u{7}".to_owned()),
    );
    let kick = last_asked(&s.plugin, EventKind::PlayerKick);
    let EventPayload::PlayerKick(PlayerKick { reason, .. }) = &kick.payload else {
        panic!("expected a kick");
    };
    assert_eq!(reason, "Illegal characters in chat");
    rule_on(&mut s.server, 1, kick.id, true, None);
    assert!(alice.link.disconnected_with().is_none());
    assert_eq!(s.server.players_online(), 1);
}

#[test]
fn a_second_login_kicks_the_first_session_before_its_login_event() {
    let mut server = server(PluginSettings::default());
    let first = join(&mut server, "Alice");
    let watch = connect_with(&mut server, 1, "watch", session_watch());
    let second = join(&mut server, "Alice");
    assert_eq!(
        fired_kinds(&watch)[..4],
        [
            (Phase::Settled, EventKind::PlayerKick),
            (Phase::Settled, EventKind::PlayerLeave),
            (Phase::Settled, EventKind::PlayerLogin),
            (Phase::Settled, EventKind::PlayerCommandSend),
        ]
    );
    assert!(first.link.disconnected_with().is_some());
    assert!(second.link.disconnected_with().is_none());
    assert_eq!(server.players_online(), 1);
}

#[test]
fn a_settings_packet_fires_the_main_hand_then_the_locale_with_the_old_hand() {
    let mut s = scene_with(
        &["Alice"],
        vec![
            observe(1, EventKind::PlayerChangedMainHand),
            observe(2, EventKind::PlayerLocaleChange),
        ],
    );
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Settings(french_left_handed()),
    );
    let events = fired(&s.plugin);
    assert_eq!(events.len(), 2);
    let EventPayload::PlayerChangedMainHand(PlayerChangedMainHand { main_hand, .. }) =
        &events[0].payload
    else {
        panic!("expected the hand first, got {:?}", events[0]);
    };
    assert_eq!(*main_hand, MainHand::Right);
    let EventPayload::PlayerLocaleChange(PlayerLocaleChange { locale, player }) =
        &events[1].payload
    else {
        panic!("expected the locale");
    };
    assert_eq!(locale, "fr_fr");
    assert_eq!(player.id, alice.id);
    let state = s.ask(rspigot_core::plugins::Question::Player(alice.id));
    let rspigot_core::plugins::Answer::Player(state) = state else {
        panic!("expected the state");
    };
    assert_eq!(state.locale, "fr_fr");
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Settings(french_left_handed()),
    );
    assert_eq!(fired(&s.plugin).len(), 2);
}

#[test]
fn a_pre_login_without_deciders_is_allowed_at_once() {
    let mut server = server(PluginSettings::default());
    let watch = connect_with(
        &mut server,
        1,
        "watch",
        vec![observe(1, EventKind::AsyncPlayerPreLogin)],
    );
    let (reply, verdict) = bounded(1);
    server.handle(Inbound::PreLogin {
        name: "Alice".to_owned(),
        id: PlayerId::offline("Alice"),
        address: IpAddr::from([10, 0, 0, 7]),
        transferred: true,
        reply,
    });
    assert_eq!(verdict.try_recv(), Ok(PreLoginVerdict::Allowed));
    assert_eq!(
        fired(&watch)[0].payload,
        EventPayload::AsyncPlayerPreLogin(AsyncPlayerPreLogin {
            address: IpAddr::from([10, 0, 0, 7]),
            kick_message: String::new(),
            login_result: AsyncPlayerPreLoginResult::Allowed,
            name: "Alice".to_owned(),
            transferred: true,
            unique_id: PlayerId::offline("Alice").0,
        })
    );
}

#[test]
fn a_pre_login_decider_refuses_with_its_message() {
    let mut server = server(PluginSettings::default());
    let guard = connect_with(
        &mut server,
        1,
        "guard",
        vec![deciding(1, EventKind::AsyncPlayerPreLogin)],
    );
    let (reply, verdict) = bounded(1);
    server.handle(Inbound::PreLogin {
        name: "Alice".to_owned(),
        id: PlayerId::offline("Alice"),
        address: IpAddr::from([127, 0, 0, 1]),
        transferred: false,
        reply,
    });
    assert!(verdict.try_recv().is_err());
    let asked = last_asked(&guard, EventKind::AsyncPlayerPreLogin);
    rule_on(
        &mut server,
        1,
        asked.id,
        false,
        Some(EventPatch::AsyncPlayerPreLogin(AsyncPlayerPreLoginPatch {
            kick_message: Some("full moon".to_owned()),
            login_result: Some(AsyncPlayerPreLoginResult::KickOther),
        })),
    );
    assert_eq!(
        verdict.try_recv(),
        Ok(PreLoginVerdict::Refused("full moon".to_owned()))
    );
}

#[test]
fn a_player_leaving_with_a_plugin_menu_open_closes_it_before_the_quit() {
    let mut s = scene_with(
        &["Alice"],
        vec![
            observe(1, EventKind::InventoryClose),
            observe(2, EventKind::PlayerLeave),
        ],
    );
    let alice = s.players[0].clone();
    s.act(rspigot_core::plugins::Order::Server(
        rspigot_core::plugins::ServerOrder::CreateInventory {
            id: 7,
            rows: 1,
            title: "Menu".to_owned(),
            locked: true,
        },
    ));
    s.act(to(&alice, PlayerOrder::OpenInventory(7)));
    s.server.handle(leave_of(&alice));
    assert_eq!(
        fired_kinds(&s.plugin),
        [
            (Phase::Settled, EventKind::InventoryClose),
            (Phase::Settled, EventKind::PlayerLeave),
        ]
    );
}

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn kill(s: &mut Scene, player: &Player) {
    s.act(to(player, PlayerOrder::SetHealth(0.0)));
}

fn leave(s: &mut Scene, player: &Player) {
    s.server.handle(leave_of(player));
    s.server.deliver();
}

fn settled_kinds(mailbox: &Mailbox) -> Vec<EventKind> {
    fired_kinds(mailbox)
        .into_iter()
        .filter(|(phase, _)| *phase == Phase::Settled)
        .map(|(_, kind)| kind)
        .collect()
}

fn diamonds(count: u8) -> Vec<(String, u8)> {
    vec![("minecraft:diamond".to_owned(), count)]
}

#[test]
fn a_death_pending_when_its_player_leaves_settles_before_the_quit_and_the_save() {
    let (mut s, saves) = saving_scene(
        &["Alice"],
        vec![
            deciding(1, EventKind::PlayerDeath),
            observe(2, EventKind::PlayerLeave),
            observe(3, EventKind::PlayerDeath),
        ],
    );
    let _silent = connect_with(
        &mut s.server,
        2,
        "silent",
        vec![deciding(1, EventKind::PlayerDeath)],
    );
    let alice = s.players[0].clone();
    console(&mut s, "give Alice diamond 5");
    kill(&mut s, &alice);
    let death = last_asked(&s.plugin, EventKind::PlayerDeath);
    let EventPayload::PlayerDeath(dying) = &death.payload else {
        panic!("a player death");
    };
    rule_on(
        &mut s.server,
        1,
        death.id,
        false,
        Some(EventPatch::PlayerDeath(PlayerDeathPatch {
            keep_inventory: Some(true),
            drops: replacing(&dying.drops, Vec::new()),
            ..PlayerDeathPatch::default()
        })),
    );
    assert!(settled_for(&s.plugin, EventKind::PlayerDeath).is_empty());
    leave(&mut s, &alice);
    assert_eq!(
        settled_kinds(&s.plugin),
        [EventKind::PlayerDeath, EventKind::PlayerLeave]
    );
    assert_eq!(s.server.items_on_ground(), 0);
    assert_eq!(saves.items(alice.id), diamonds(5));
}

#[test]
fn a_drop_pending_when_its_player_leaves_is_thrown_before_the_save() {
    let (mut s, saves) = saving_scene(&["Alice"], vec![deciding(1, EventKind::PlayerDropItem)]);
    let alice = s.players[0].clone();
    console(&mut s, "give Alice diamond 3");
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Dig {
            action: rspigot_core::events::DigAction::DropItem,
            pos: rspigot_domain::BlockPos::new(0, 0, 0),
            face: rspigot_core::events::Direction::Up,
            sequence: 0,
        },
    );
    let asked = last_asked(&s.plugin, EventKind::PlayerDropItem);
    leave(&mut s, &alice);
    assert_eq!(s.server.items_on_ground(), 1);
    assert_eq!(saves.items(alice.id), diamonds(2));
    rule_on(&mut s.server, 1, asked.id, false, None);
    assert_eq!(s.server.items_on_ground(), 1);
}

#[test]
fn a_shutdown_settles_every_pending_event_before_the_players_are_saved() {
    let (mut s, saves) = saving_scene(
        &["Alice"],
        vec![
            deciding(1, EventKind::PlayerDeath),
            observe(2, EventKind::PlayerDeath),
        ],
    );
    let _silent = connect_with(
        &mut s.server,
        2,
        "silent",
        vec![deciding(1, EventKind::PlayerDeath)],
    );
    let alice = s.players[0].clone();
    console(&mut s, "give Alice diamond 5");
    kill(&mut s, &alice);
    let death = last_asked(&s.plugin, EventKind::PlayerDeath);
    let EventPayload::PlayerDeath(dying) = &death.payload else {
        panic!("a player death");
    };
    rule_on(
        &mut s.server,
        1,
        death.id,
        false,
        Some(EventPatch::PlayerDeath(PlayerDeathPatch {
            keep_inventory: Some(true),
            drops: replacing(&dying.drops, Vec::new()),
            ..PlayerDeathPatch::default()
        })),
    );
    let Scene { server, plugin, .. } = s;
    let (sender, receiver) = crossbeam_channel::unbounded();
    sender.send(Inbound::Shutdown).expect("open");
    server.run(&receiver);
    assert_eq!(settled_for(&plugin, EventKind::PlayerDeath).len(), 1);
    assert_eq!(saves.items(alice.id), diamonds(5));
}

#[test]
fn the_end_of_a_refused_second_session_leaves_the_first_one_online() {
    let mut s = scene_with(&["Alice"], vec![deciding(1, EventKind::PlayerKick)]);
    let first = s.players[0].clone();
    let second = arrive(&mut s.server, "Alice");
    let kick = last_asked(&s.plugin, EventKind::PlayerKick);
    rule_on(&mut s.server, 1, kick.id, true, None);
    assert!(second.link.disconnected_with().is_some());
    assert!(first.link.is_connected());
    s.server.handle(leave_of(&second));
    s.server.tick();
    assert_eq!(s.server.players_online(), 1);
    assert!(first.link.is_connected());
    s.server.handle(leave_of(&first));
    assert_eq!(s.server.players_online(), 0);
}

#[test]
fn repeated_anti_cheat_triggers_ask_one_kick_at_a_time() {
    let mut s = scene_with(&["Alice"], vec![deciding(1, EventKind::PlayerKick)]);
    let alice = s.players[0].clone();
    for _ in 0..3 {
        send_input(
            &mut s.server,
            &alice,
            PlayerInput::Chat("bad\u{7}".to_owned()),
        );
    }
    assert_eq!(asked_for(&s.plugin, EventKind::PlayerKick).len(), 1);
    let kick = last_asked(&s.plugin, EventKind::PlayerKick);
    rule_on(&mut s.server, 1, kick.id, true, None);
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Chat("bad\u{7}".to_owned()),
    );
    assert_eq!(asked_for(&s.plugin, EventKind::PlayerKick).len(), 2);
}

#[test]
fn a_login_waiting_behind_a_duplicate_kick_computes_its_refusal_once_the_kick_applies() {
    let config = rspigot_core::config::GameConfig {
        view_distance: 3,
        chunk_workers: 2,
        spawn_protection: 0,
        max_players: 1,
        ..rspigot_core::config::GameConfig::default()
    };
    let mut server = rspigot_core::server::Server::new(
        config,
        rspigot_core::config::LevelData::default(),
        std::sync::Arc::new(NoStore),
        std::sync::Arc::new(Floor),
        rspigot_core::status::StatusBoard::new(1),
    );
    server.wait_for_spawn(std::time::Duration::from_secs(10));
    let first = join(&mut server, "Alice");
    let plugin = connect_with(
        &mut server,
        1,
        "tester",
        vec![
            deciding(1, EventKind::PlayerKick),
            observe(2, EventKind::PlayerLogin),
        ],
    );
    let second = arrive(&mut server, "Alice");
    let kick = last_asked(&plugin, EventKind::PlayerKick);
    rule_on(&mut server, 1, kick.id, false, None);
    let login = settled_for(&plugin, EventKind::PlayerLogin);
    let EventPayload::PlayerLogin(PlayerLogin { result, .. }) = &login[0].payload else {
        panic!("expected a login");
    };
    assert_eq!(*result, PlayerLoginResult::Allowed);
    assert!(first.link.disconnected_with().is_some());
    assert!(second.link.is_connected());
    assert_eq!(server.players_online(), 1);
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

#[test]
fn a_command_send_decider_prunes_the_roots_and_fires_again_at_operator_changes_and_respawn() {
    let mut s = scene_with(&[], vec![deciding(1, EventKind::PlayerCommandSend)]);
    let alice = join(&mut s.server, "Alice");
    let asked = last_asked(&s.plugin, EventKind::PlayerCommandSend);
    let EventPayload::PlayerCommandSend(PlayerCommandSend { commands, .. }) = &asked.payload else {
        panic!("expected a command send");
    };
    assert!(commands.contains(&"help".to_owned()));
    assert!(commands.len() > 1);
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
    let fires = |s: &Scene| asked_for(&s.plugin, EventKind::PlayerCommandSend).len();
    for (line, expected) in [("op Alice", 2), ("deop Alice", 3)] {
        s.server.handle(Inbound::Console(line.to_owned()));
        s.server.deliver();
        assert_eq!(fires(&s), expected, "{line}");
        let asked = last_asked(&s.plugin, EventKind::PlayerCommandSend);
        rule_on(&mut s.server, 1, asked.id, false, None);
    }
    s.act(to(&alice, PlayerOrder::SetHealth(0.0)));
    assert_eq!(s.server.player_is_dead(alice.id), Some(true));
    send_input(&mut s.server, &alice, PlayerInput::Respawn);
    assert_eq!(s.server.player_is_dead(alice.id), Some(false));
    assert_eq!(fires(&s), 4);
}

#[test]
fn a_kick_settled_after_its_client_went_away_does_not_disconnect_twice() {
    let mut s = scene_with(
        &["Alice", "Bob"],
        vec![
            deciding(1, EventKind::PlayerKick),
            observe(2, EventKind::PlayerLeave),
        ],
    );
    let alice = s.players[0].clone();
    s.act(to(&alice, PlayerOrder::Kick("Bye".into())));
    let kick = last_asked(&s.plugin, EventKind::PlayerKick);
    alice.link.disconnect(Component::text("gone"));
    s.tick(1);
    assert_eq!(s.server.players_online(), 1);
    assert_eq!(
        alice.link.disconnected_with(),
        Some(Component::text("gone"))
    );
    assert_eq!(settled_for(&s.plugin, EventKind::PlayerLeave).len(), 1);
    rule_on(&mut s.server, 1, kick.id, false, None);
    assert_eq!(
        alice.link.disconnected_with(),
        Some(Component::text("gone"))
    );
    assert_eq!(settled_for(&s.plugin, EventKind::PlayerLeave).len(), 1);
}

#[test]
fn two_plugins_that_each_remove_a_command_both_prune_it() {
    let sending = |priority| HandlerSpec {
        priority,
        ..deciding(1, EventKind::PlayerCommandSend)
    };
    for later_answers_first in [false, true] {
        let mut s = scene_with(&[], vec![sending(Priority::Low)]);
        connect_with(&mut s.server, 2, "second", vec![sending(Priority::High)]);
        let alice = join(&mut s.server, "Alice");
        let asked = last_asked(&s.plugin, EventKind::PlayerCommandSend);
        let EventPayload::PlayerCommandSend(PlayerCommandSend { commands, .. }) = &asked.payload
        else {
            panic!("expected a command send");
        };
        let [first, second] = ["help", "me"].map(|gone| {
            assert!(commands.iter().any(|name| name == gone));
            Some(EventPatch::PlayerCommandSend(PlayerCommandSendPatch {
                commands: removing(commands, |name| name == gone),
            }))
        });
        let mut answers = vec![(1, first), (2, second)];
        if later_answers_first {
            answers.reverse();
        }
        alice.link.take();
        for (connection, patch) in answers {
            rule_on(&mut s.server, connection, asked.id, false, patch);
        }
        let sent = command_roots(&alice);
        assert_eq!(sent.len(), 1);
        let expected: Vec<String> = commands
            .iter()
            .filter(|name| *name != "help" && *name != "me")
            .cloned()
            .collect();
        assert_eq!(sent[0], expected, "{later_answers_first}");
    }
}

#[test]
fn a_command_send_settled_after_an_op_prunes_the_operator_tree() {
    let mut s = scene_with(&[], vec![deciding(1, EventKind::PlayerCommandSend)]);
    let alice = join(&mut s.server, "Alice");
    let asked = last_asked(&s.plugin, EventKind::PlayerCommandSend);
    let EventPayload::PlayerCommandSend(PlayerCommandSend { commands, .. }) = &asked.payload else {
        panic!("expected a command send");
    };
    assert!(commands.iter().any(|name| name == "help"));
    s.server.handle(Inbound::Console("op Alice".to_owned()));
    s.server.deliver();
    alice.link.take();
    rule_on(
        &mut s.server,
        1,
        asked.id,
        false,
        Some(EventPatch::PlayerCommandSend(PlayerCommandSendPatch {
            commands: removing(commands, |name| name == "help"),
        })),
    );
    let sent = command_roots(&alice);
    assert_eq!(sent.len(), 1);
    assert!(!sent[0].iter().any(|name| name == "help"));
    assert!(
        sent[0].iter().any(|name| !commands.contains(name)),
        "the operator commands are sent: {:?}",
        sent[0]
    );
}
