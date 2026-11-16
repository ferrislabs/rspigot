mod common;

use std::cell::Cell;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use common::*;
use crossbeam_channel::{Receiver, unbounded};
use rspigot_core::admin::{AdminLists, Ban, BanEntry, IpBanEntry, ListEntry};
use rspigot_core::config::{GameConfig, LevelData, PluginSettings};
use rspigot_core::console::console_text;
use rspigot_core::events::{ClientEvent, Inbound, PlayerInput};
use rspigot_core::plugins::{Answer, Order, PlayerOrder, Question, ServerOrder};
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_domain::player::PlayerId;
use rspigot_domain::text::Content;

thread_local! {
    static NOW: Cell<u64> = const { Cell::new(1_793_534_400) };
}

fn clock() -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(NOW.with(Cell::get))
}

fn setup() -> (Scene, Receiver<AdminLists>) {
    let (sink, published) = unbounded();
    let mut server = server(PluginSettings::default())
        .with_clock(clock)
        .with_admin_sink(sink);
    let plugin = connect(&mut server, 1, "tester");
    let alice = join(&mut server, "Alice");
    let mut s = Scene {
        server,
        plugin,
        players: vec![alice],
    };
    s.act(Order::Server(ServerOrder::OperatorAdd("Alice".into())));
    (s, published)
}

fn far_future() -> SystemTime {
    UNIX_EPOCH + Duration::from_millis(i64::MAX.unsigned_abs())
}

fn far_past() -> SystemTime {
    UNIX_EPOCH - Duration::from_millis(i64::MAX.unsigned_abs())
}

fn server_with_bans(bans: Vec<BanEntry>) -> Server<Link> {
    let config = GameConfig {
        view_distance: 3,
        chunk_workers: 2,
        admin: AdminLists {
            bans,
            ..AdminLists::default()
        },
        ..GameConfig::default()
    };
    let mut server = Server::new(
        config,
        LevelData::default(),
        Arc::new(NoStore),
        Arc::new(Floor),
        StatusBoard::new(20),
    )
    .with_clock(clock);
    server.wait_for_spawn(Duration::from_secs(10));
    server
}

fn twins() -> (Scene, Player, Player) {
    let (mut s, _published) = setup();
    let upper = join(&mut s.server, "Steve");
    let lower = join(&mut s.server, "steve");
    assert_ne!(upper.id, lower.id);
    assert_eq!(s.server.players_online(), 3);
    (s, upper, lower)
}

fn said(player: &Player) -> Vec<String> {
    player
        .link
        .take()
        .into_iter()
        .filter_map(|event| match event {
            ClientEvent::SystemChat { message, .. } => Some(message),
            _ => None,
        })
        .filter(|message| {
            !matches!(&message.content, Content::Translatable { key, .. }
                if key == "multiplayer.player.joined" || key == "multiplayer.player.left")
        })
        .map(|message| console_text(&message))
        .collect()
}

fn command(s: &mut Scene, player: &Player, line: &str) -> Vec<String> {
    said(player);
    s.server.handle(Inbound::Input {
        player: player.id,
        input: PlayerInput::Command(line.to_owned()),
    });
    s.server.deliver();
    said(player)
}

fn refusal(player: &Player) -> String {
    console_text(
        &player
            .link
            .disconnected_with()
            .expect("the player was disconnected"),
    )
}

fn last_published(published: &Receiver<AdminLists>) -> AdminLists {
    published
        .try_iter()
        .last()
        .expect("the admin lists were published")
}

fn address(raw: &str) -> IpAddr {
    raw.parse().expect("a valid address")
}

fn bans(s: &mut Scene) -> Vec<BanEntry> {
    let Answer::Bans(bans) = s.ask(Question::Bans) else {
        panic!("expected the profile bans");
    };
    bans
}

fn ip_bans(s: &mut Scene) -> Vec<IpBanEntry> {
    let Answer::IpBans(bans) = s.ask(Question::IpBans) else {
        panic!("expected the IP bans");
    };
    bans
}

#[test]
fn ban_writes_an_entry_kicks_and_refuses_the_next_login() {
    let (mut s, published) = setup();
    let alice = s.players[0].clone();
    let steve = join(&mut s.server, "Steve");
    assert_eq!(
        command(&mut s, &alice, "/ban Steve griefing"),
        ["Banned Steve: griefing"]
    );
    assert_eq!(refusal(&steve), "You are banned from this server");
    assert_eq!(s.server.players_online(), 1);
    assert_eq!(
        last_published(&published).bans,
        [BanEntry {
            target: ListEntry {
                name: "Steve".into(),
                uuid: Some(steve.id.0),
                level: None,
                bypasses_player_limit: false,
            },
            created: clock(),
            source: "Alice".into(),
            expires: None,
            reason: Some("griefing".into()),
        }]
    );
    let again = join(&mut s.server, "Steve");
    assert_eq!(
        refusal(&again),
        "You are banned from this server.\nReason: griefing"
    );
    assert_eq!(s.server.players_online(), 1);

    assert_eq!(command(&mut s, &alice, "/pardon Steve"), ["Unbanned Steve"]);
    let back = join(&mut s.server, "Steve");
    assert!(back.link.disconnected_with().is_none());
    assert_eq!(s.server.players_online(), 2);
}

#[test]
fn an_ip_ban_refuses_every_name_at_that_address() {
    let (mut s, _published) = setup();
    let alice = s.players[0].clone();
    let steve = join_from(&mut s.server, "Steve", address("203.0.113.7"));
    assert_eq!(
        command(&mut s, &alice, "/ban-ip Steve"),
        [
            "Banned IP 203.0.113.7: Banned by an operator.",
            "This ban affects 1 player(s): Steve"
        ]
    );
    assert_eq!(refusal(&steve), "You have been IP banned from this server");

    let refused = "Your IP address is banned from this server.\nReason: Banned by an operator.";
    let carol = join_from(&mut s.server, "Carol", address("203.0.113.7"));
    assert_eq!(refusal(&carol), refused);
    let dave = join_from(&mut s.server, "Dave", address("::ffff:203.0.113.7"));
    assert_eq!(refusal(&dave), refused);
    assert_eq!(s.server.players_online(), 1);

    assert_eq!(
        command(&mut s, &alice, "/pardon-ip 203.0.113.7"),
        ["Unbanned IP 203.0.113.7"]
    );
    let carol = join_from(&mut s.server, "Carol", address("203.0.113.7"));
    assert!(carol.link.disconnected_with().is_none());
    assert_eq!(s.server.players_online(), 2);
}

#[test]
fn an_expired_ban_no_longer_refuses() {
    let (mut s, published) = setup();
    s.act(Order::Server(ServerOrder::BanAdd {
        name: "Steve".into(),
        reason: None,
        expires: Some(clock() + Duration::from_secs(3600)),
    }));
    let steve = join(&mut s.server, "Steve");
    assert_eq!(
        refusal(&steve),
        "You are banned from this server.\nReason: Banned by an operator.\nYour ban will be removed on 2026-11-01 at 13:00:00 UTC"
    );
    last_published(&published);

    NOW.with(|now| now.set(now.get() + 3601));
    let steve = join(&mut s.server, "Steve");
    assert!(steve.link.disconnected_with().is_none());
    assert_eq!(s.server.players_online(), 2);
    assert!(last_published(&published).bans.is_empty());
}

#[test]
fn a_name_only_ban_gains_the_uuid_at_login() {
    let (mut s, published) = setup();
    s.act(Order::Server(ServerOrder::BanAdd {
        name: "Steve".into(),
        reason: None,
        expires: None,
    }));
    assert_eq!(last_published(&published).bans[0].target.uuid, None);
    let steve = join(&mut s.server, "steve");
    assert_eq!(
        refusal(&steve),
        "You are banned from this server.\nReason: Banned by an operator."
    );
    assert_eq!(
        last_published(&published).bans[0].target.uuid,
        Some(PlayerId::offline("steve").0)
    );
}

#[test]
fn ban_commands_reply_like_vanilla() {
    let (mut s, _published) = setup();
    let alice = s.players[0].clone();
    assert_eq!(
        command(&mut s, &alice, "/ban Steve"),
        ["Banned Steve: Banned by an operator."]
    );
    assert_eq!(
        command(&mut s, &alice, "/ban Steve"),
        ["Nothing changed. The player is already banned"]
    );
    assert_eq!(
        command(&mut s, &alice, "/pardon Nobody"),
        ["Nothing changed. The player isn't banned"]
    );
    assert_eq!(
        command(&mut s, &alice, "/ban-ip not-an-ip"),
        ["Invalid IP address or unknown player"]
    );
    assert_eq!(
        command(&mut s, &alice, "/pardon-ip 1.2.3.4"),
        ["Nothing changed. That IP isn't banned"]
    );
    assert_eq!(
        command(&mut s, &alice, "/pardon-ip nope"),
        ["Invalid IP address"]
    );
    assert_eq!(
        command(&mut s, &alice, "/ban-ip 203.0.113.7"),
        ["Banned IP 203.0.113.7: Banned by an operator."]
    );
    assert_eq!(
        command(&mut s, &alice, "/ban-ip 203.0.113.7"),
        ["Nothing changed. That IP is already banned"]
    );

    let (mut s, _published) = setup();
    let alice = s.players[0].clone();
    assert_eq!(command(&mut s, &alice, "/banlist"), ["There are no bans"]);
    command(&mut s, &alice, "/ban Steve griefing");
    command(&mut s, &alice, "/ban-ip 203.0.113.7");
    assert_eq!(
        command(&mut s, &alice, "/banlist"),
        [
            "There are 2 ban(s):",
            "Steve was banned by Alice: griefing",
            "203.0.113.7 was banned by Alice: Banned by an operator."
        ]
    );
    assert_eq!(
        command(&mut s, &alice, "/banlist ips"),
        [
            "There are 1 ban(s):",
            "203.0.113.7 was banned by Alice: Banned by an operator."
        ]
    );
    let bob = join(&mut s.server, "Bob");
    assert_eq!(
        command(&mut s, &bob, "/ban Alice"),
        [
            "Unknown or incomplete command, see below for error",
            "ban Alice<--[HERE]"
        ]
    );
    assert!(alice.link.disconnected_with().is_none());
}

#[test]
fn plugin_bans_and_their_lists() {
    let (mut s, _published) = setup();
    let alice = s.players[0].clone();
    let steve = join(&mut s.server, "Steve");
    s.act(Order::Player {
        player: steve.id,
        order: PlayerOrder::Ban {
            reason: Some("cheating".into()),
            expires: None,
        },
    });
    assert_eq!(refusal(&steve), "You are banned from this server");
    assert_eq!(
        bans(&mut s),
        [BanEntry {
            target: ListEntry {
                name: "Steve".into(),
                uuid: Some(steve.id.0),
                level: None,
                bypasses_player_limit: false,
            },
            created: clock(),
            source: "tester".into(),
            expires: None,
            reason: Some("cheating".into()),
        }]
    );

    s.act(Order::Server(ServerOrder::IpBanAdd {
        address: "203.0.113.9".into(),
        reason: None,
        expires: None,
    }));
    assert_eq!(
        ip_bans(&mut s),
        [Ban {
            target: address("203.0.113.9"),
            created: clock(),
            source: "tester".into(),
            expires: None,
            reason: None,
        }]
    );
    s.act(Order::Server(ServerOrder::IpBanAdd {
        address: "not an ip".into(),
        reason: None,
        expires: None,
    }));
    assert_eq!(
        s.warnings(),
        ["server.ip_ban_add was not applied: not an ip is not an IP address"]
    );

    s.act(Order::Server(ServerOrder::BanRemove("Steve".into())));
    s.act(Order::Server(ServerOrder::IpBanRemove(
        "203.0.113.9".into(),
    )));
    assert_eq!(bans(&mut s), []);
    assert_eq!(ip_bans(&mut s), []);
    assert_eq!(s.warnings().len(), 1);

    let Answer::Player(state) = s.ask(Question::Player(alice.id)) else {
        panic!("expected the state of Alice");
    };
    assert_eq!(state.address, IpAddr::from([127, 0, 0, 1]));
}

#[test]
fn an_expiry_the_ban_files_cannot_hold_is_refused_with_one_warning() {
    let (mut s, _published) = setup();
    let steve = join(&mut s.server, "Steve");
    s.act(Order::Server(ServerOrder::BanAdd {
        name: "Carol".into(),
        reason: None,
        expires: Some(far_future()),
    }));
    s.act(Order::Server(ServerOrder::IpBanAdd {
        address: "203.0.113.9".into(),
        reason: None,
        expires: Some(far_past()),
    }));
    s.act(Order::Player {
        player: steve.id,
        order: PlayerOrder::Ban {
            reason: None,
            expires: Some(far_future()),
        },
    });
    let reason = "expiry is outside the dates a ban can store";
    assert_eq!(
        s.warnings(),
        [
            format!("server.ban_add was not applied: {reason}"),
            format!("server.ip_ban_add was not applied: {reason}"),
            format!("player.ban was not applied: {reason}"),
        ]
    );
    assert_eq!(bans(&mut s), []);
    assert_eq!(ip_bans(&mut s), []);
    assert!(steve.link.disconnected_with().is_none());
}

#[test]
fn a_date_outside_the_range_never_breaks_the_kick_text() {
    let mut server = server_with_bans(vec![Ban {
        target: ListEntry::named("Steve"),
        created: far_past(),
        source: "Server".into(),
        expires: Some(far_future()),
        reason: None,
    }]);
    let steve = join(&mut server, "Steve");
    assert_eq!(
        refusal(&steve),
        "You are banned from this server.\nReason: Banned by an operator."
    );
}

#[test]
fn a_plugin_ban_hits_the_player_it_names_not_a_name_twin() {
    for banned_lower in [true, false] {
        let (mut s, upper, lower) = twins();
        let (banned, spared) = if banned_lower {
            (lower, upper)
        } else {
            (upper, lower)
        };
        s.act(Order::Player {
            player: banned.id,
            order: PlayerOrder::Ban {
                reason: None,
                expires: None,
            },
        });
        assert_eq!(refusal(&banned), "You are banned from this server");
        assert!(spared.link.disconnected_with().is_none());
        assert_eq!(bans(&mut s)[0].target.uuid, Some(banned.id.0));
    }
}

#[test]
fn a_selector_ban_hits_the_selected_player_not_a_name_twin() {
    for name in ["steve", "Steve"] {
        let (mut s, upper, lower) = twins();
        let alice = s.players[0].clone();
        let (banned, spared) = if name == "steve" {
            (lower, upper)
        } else {
            (upper, lower)
        };
        assert_eq!(
            command(&mut s, &alice, &format!("/ban @a[name={name}]")),
            [format!("Banned {name}: Banned by an operator.")]
        );
        assert_eq!(refusal(&banned), "You are banned from this server");
        assert!(spared.link.disconnected_with().is_none());
        assert_eq!(bans(&mut s)[0].target.uuid, Some(banned.id.0));
    }
}

#[test]
fn a_ban_of_another_uuid_with_the_same_name_is_a_new_ban() {
    let (mut s, _published) = setup();
    let alice = s.players[0].clone();
    let upper = join(&mut s.server, "Steve");
    command(&mut s, &alice, "/ban Steve");
    assert!(upper.link.disconnected_with().is_some());
    let lower = join(&mut s.server, "steve");
    assert!(lower.link.disconnected_with().is_none());
    assert_eq!(
        command(&mut s, &alice, "/ban steve"),
        ["Banned steve: Banned by an operator."]
    );
    assert_eq!(refusal(&lower), "You are banned from this server");
    let uuids: Vec<_> = bans(&mut s).iter().map(|ban| ban.target.uuid).collect();
    assert_eq!(uuids, [Some(upper.id.0), Some(lower.id.0)]);
}
