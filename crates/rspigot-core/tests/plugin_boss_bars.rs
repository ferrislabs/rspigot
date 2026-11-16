mod common;

use common::*;
use rspigot_core::events::ClientEvent;
use rspigot_core::plugins::{Answer, BossBarOrder, BossBarSpec, BossBarView, Order, Question};
use rspigot_domain::boss_event::BossUpdate;

fn spec(key: Option<&str>) -> BossBarSpec {
    BossBarSpec {
        title: "§cBoss".to_owned(),
        color: "red".to_owned(),
        style: "notched_10".to_owned(),
        flags: vec!["darken_screen".to_owned()],
        key: key.map(str::to_owned),
    }
}

fn create(s: &mut Scene, key: Option<&str>) -> u64 {
    let Answer::BossBarCreated(bar) = s.ask(Question::CreateBossBar(spec(key))) else {
        panic!("expected a boss bar");
    };
    bar
}

fn order(bar: u64, order: BossBarOrder) -> Order {
    Order::BossBar { bar, order }
}

fn state(s: &mut Scene, bar: u64) -> BossBarView {
    let Answer::BossBar(view) = s.ask(Question::BossBar(bar)) else {
        panic!("expected a boss bar state");
    };
    view
}

fn updates(player: &Player) -> Vec<BossUpdate> {
    player
        .link
        .take()
        .into_iter()
        .filter_map(|event| match event {
            ClientEvent::BossEvent { update, .. } => Some(update),
            _ => None,
        })
        .collect()
}

#[test]
fn a_created_bar_starts_full_with_the_requested_look_and_nobody_watching() {
    let mut s = scene(&["Alice"]);
    let bar = create(&mut s, None);
    let view = state(&mut s, bar);
    assert_eq!(view.title, "§cBoss");
    assert_eq!(view.color, "red");
    assert_eq!(view.style, "notched_10");
    assert_eq!(view.progress, 1.0);
    assert_eq!(view.flags, ["darken_screen"]);
    assert!(view.visible && view.players.is_empty() && view.key.is_none());
    assert!(updates(&s.players[0]).is_empty());
}

#[test]
fn players_see_the_bar_and_every_change() {
    let mut s = scene(&["Alice", "Bob"]);
    let bar = create(&mut s, None);
    let alice = s.players[0].id.0.to_string();
    s.act(order(bar, BossBarOrder::AddPlayer(alice)));
    let seen = updates(&s.players[0]);
    assert!(matches!(
        &seen[0],
        BossUpdate::Add { color, flags, .. }
            if *color == rspigot_domain::boss_event::BossColor::Red && flags.darken_screen
    ));
    assert!(updates(&s.players[1]).is_empty());
    s.act(order(bar, BossBarOrder::SetProgress(0.25)));
    s.act(order(bar, BossBarOrder::SetTitle("§aNew".to_owned())));
    s.act(order(bar, BossBarOrder::SetColor("green".to_owned())));
    s.act(order(bar, BossBarOrder::SetStyle("progress".to_owned())));
    s.act(order(
        bar,
        BossBarOrder::SetFlag {
            flag: "play_boss_music".to_owned(),
            enabled: true,
        },
    ));
    let seen = updates(&s.players[0]);
    assert_eq!(seen[0], BossUpdate::Progress(0.25));
    assert!(matches!(seen[1], BossUpdate::Name(_)));
    assert!(matches!(seen[2], BossUpdate::Style { .. }));
    assert!(matches!(seen[3], BossUpdate::Style { .. }));
    assert!(matches!(seen[4], BossUpdate::Properties(f) if f.play_music && f.darken_screen));
    assert!(updates(&s.players[1]).is_empty());
}

#[test]
fn visibility_removing_players_and_removing_the_bar_send_the_remove_packet() {
    let mut s = scene(&["Alice", "Bob"]);
    let bar = create(&mut s, None);
    for player in [0, 1] {
        let id = s.players[player].id.0.to_string();
        s.act(order(bar, BossBarOrder::AddPlayer(id)));
    }
    updates(&s.players[0]);
    updates(&s.players[1]);
    s.act(order(bar, BossBarOrder::SetVisible(false)));
    assert_eq!(updates(&s.players[0]), vec![BossUpdate::Remove]);
    s.act(order(bar, BossBarOrder::SetVisible(true)));
    assert!(matches!(updates(&s.players[0])[0], BossUpdate::Add { .. }));
    updates(&s.players[1]);
    let alice = s.players[0].id.0.to_string();
    s.act(order(bar, BossBarOrder::RemovePlayer(alice)));
    assert_eq!(updates(&s.players[0]), vec![BossUpdate::Remove]);
    assert!(updates(&s.players[1]).is_empty());
    s.act(order(bar, BossBarOrder::Remove));
    assert_eq!(updates(&s.players[1]), vec![BossUpdate::Remove]);
    assert!(matches!(s.ask(Question::BossBar(bar)), Answer::NotFound));
}

#[test]
fn removing_all_players_empties_the_bar() {
    let mut s = scene(&["Alice", "Bob"]);
    let bar = create(&mut s, None);
    for player in [0, 1] {
        let id = s.players[player].id.0.to_string();
        s.act(order(bar, BossBarOrder::AddPlayer(id)));
    }
    s.act(order(bar, BossBarOrder::RemoveAllPlayers));
    assert!(state(&mut s, bar).players.is_empty());
    assert_eq!(updates(&s.players[0]).last(), Some(&BossUpdate::Remove));
}

#[test]
fn keyed_bars_are_listed_and_keys_are_unique() {
    let mut s = scene(&["Alice"]);
    create(&mut s, None);
    let keyed = create(&mut s, Some("test:raid"));
    let Answer::BossBars(listed) = s.ask(Question::BossBars) else {
        panic!("expected the list");
    };
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].bar, keyed);
    assert_eq!(listed[0].key.as_deref(), Some("test:raid"));
    assert!(matches!(
        s.ask(Question::CreateBossBar(spec(Some("test:raid")))),
        Answer::Refused(_)
    ));
    assert!(matches!(
        s.ask(Question::CreateBossBar(spec(Some("Bad Key")))),
        Answer::Refused(_)
    ));
}

#[test]
fn a_keyed_bars_progress_can_be_read_back() {
    let mut s = scene(&["Alice"]);
    let bar = create(&mut s, Some("test:raid"));
    s.act(order(bar, BossBarOrder::SetProgress(0.5)));
    let view = state(&mut s, bar);
    assert_eq!(view.progress, 0.5);
}

#[test]
fn invalid_values_are_refused_without_changing_the_bar() {
    let mut s = scene(&["Alice"]);
    let bar = create(&mut s, None);
    for bad in [
        BossBarOrder::SetColor("teal".to_owned()),
        BossBarOrder::SetStyle("round".to_owned()),
        BossBarOrder::SetProgress(1.5),
        BossBarOrder::SetFlag {
            flag: "shake".to_owned(),
            enabled: true,
        },
        BossBarOrder::AddPlayer("nobody".to_owned()),
    ] {
        s.act(order(bar, bad));
    }
    let view = state(&mut s, bar);
    assert_eq!(view.color, "red");
    assert_eq!(view.style, "notched_10");
    assert_eq!(view.progress, 1.0);
    assert!(view.players.is_empty());
    s.act(order(999, BossBarOrder::Remove));
}
