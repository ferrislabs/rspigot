mod fake_host;

use fake_host::*;
use rspigot_plugin::{BarColor, BarFlag, BarStyle, Error};
use rspigot_plugin_protocol::v1::{self, answer, boss_bar_order, from_plugin, question};

fn ordered(bar: u64, order: boss_bar_order::Order) -> from_plugin::Body {
    from_plugin::Body::BossBar(v1::BossBarOrder {
        bar,
        order: Some(order),
    })
}

fn state() -> v1::BossBarState {
    v1::BossBarState {
        bar: 5,
        key: Some("test:raid".to_owned()),
        title: "Raid".to_owned(),
        color: "purple".to_owned(),
        style: "notched_12".to_owned(),
        progress: 0.75,
        flags: vec!["play_boss_music".to_owned(), "create_world_fog".to_owned()],
        visible: false,
        players: vec![STEVE.to_owned()],
    }
}

#[tokio::test]
async fn creating_a_bar_asks_the_server_and_orders_go_on_the_wire() {
    let mut called = calling(|player| {
        Box::pin(async move {
            let bars = player.server().boss_bars;
            let bar = bars
                .create(
                    "Boss",
                    BarColor::Red,
                    BarStyle::Segmented10,
                    &[BarFlag::DarkenSky],
                )
                .await
                .unwrap();
            assert_eq!(bar.id(), 7);
            assert_eq!(bar.key(), None);
            bar.set_title("New");
            bar.set_color(BarColor::Green);
            bar.set_style(BarStyle::Solid);
            bar.set_progress(0.4);
            bar.add_flag(BarFlag::CreateFog);
            bar.remove_flag(BarFlag::DarkenSky);
            bar.add_player(&player);
            bar.remove_player(&player);
            bar.remove_all_players();
            bar.set_visible(false);
            bar.remove();
        })
    })
    .await;
    let (request, asked) = called.session.asked().await;
    assert_eq!(
        asked,
        question::Question::CreateBossBar(v1::BossBarCreate {
            title: "Boss".to_owned(),
            color: "red".to_owned(),
            style: "notched_10".to_owned(),
            flags: vec!["darken_screen".to_owned()],
            key: None,
        })
    );
    called
        .session
        .say(answer(request, answer::Body::BossBarCreated(7)));
    use boss_bar_order::Order;
    let expected = [
        Order::SetTitle("New".to_owned()),
        Order::SetColor("green".to_owned()),
        Order::SetStyle("progress".to_owned()),
        Order::SetProgress(0.4),
        Order::SetFlag(v1::BossFlagOrder {
            flag: "create_world_fog".to_owned(),
            enabled: true,
        }),
        Order::SetFlag(v1::BossFlagOrder {
            flag: "darken_screen".to_owned(),
            enabled: false,
        }),
        Order::AddPlayer(STEVE.to_owned()),
        Order::RemovePlayer(STEVE.to_owned()),
        Order::RemoveAllPlayers(v1::Empty {}),
        Order::SetVisible(false),
        Order::Remove(v1::Empty {}),
    ];
    for order in expected {
        assert_eq!(called.session.hear().await, ordered(7, order));
    }
    called.finished().await;
}

#[tokio::test]
async fn keyed_bars_are_created_listed_and_found_by_key() {
    let mut called = calling(|player| {
        Box::pin(async move {
            let bars = player.server().boss_bars;
            let made = bars
                .create_keyed("test:raid", "Raid", BarColor::Pink, BarStyle::Solid, &[])
                .await
                .unwrap();
            assert_eq!(made.key(), Some("test:raid"));
            let listed = bars.all().await.unwrap();
            assert_eq!(listed.len(), 1);
            assert_eq!(listed[0], made);
            assert!(bars.get("test:raid").await.unwrap().is_some());
            assert!(bars.get("test:other").await.unwrap().is_none());
        })
    })
    .await;
    let (request, asked) = called.session.asked().await;
    assert!(matches!(
        asked,
        question::Question::CreateBossBar(v1::BossBarCreate { key: Some(_), .. })
    ));
    called
        .session
        .say(answer(request, answer::Body::BossBarCreated(5)));
    for _ in 0..3 {
        let (request, asked) = called.session.asked().await;
        assert_eq!(asked, question::Question::BossBars(v1::Empty {}));
        called.session.say(answer(
            request,
            answer::Body::BossBars(v1::BossBarList {
                bars: vec![state()],
            }),
        ));
    }
    called.finished().await;
}

#[tokio::test]
async fn the_state_of_a_bar_is_read_back_as_typed_values() {
    let mut called = calling(|player| {
        Box::pin(async move {
            let bar = player.server().boss_bars.get("test:raid").await.unwrap();
            let bar = bar.unwrap();
            let read = bar.state().await.unwrap();
            assert_eq!(read.color, BarColor::Purple);
            assert_eq!(read.style, BarStyle::Segmented12);
            assert_eq!(read.progress, 0.75);
            assert_eq!(read.flags, [BarFlag::PlayBossMusic, BarFlag::CreateFog]);
            assert!(!read.visible);
            assert_eq!(read.players.len(), 1);
            assert!(matches!(bar.state().await, Err(Error::NotFound)));
        })
    })
    .await;
    let (request, _) = called.session.asked().await;
    called.session.say(answer(
        request,
        answer::Body::BossBars(v1::BossBarList {
            bars: vec![state()],
        }),
    ));
    let (request, asked) = called.session.asked().await;
    assert_eq!(asked, question::Question::BossBar(5));
    called
        .session
        .say(answer(request, answer::Body::BossBar(state())));
    let (request, _) = called.session.asked().await;
    called
        .session
        .say(answer(request, answer::Body::NotFound(v1::NotFound {})));
    called.finished().await;
}

#[tokio::test]
async fn a_server_refusal_surfaces_as_a_rejection() {
    let mut called = calling(|player| {
        Box::pin(async move {
            let made = player
                .server()
                .boss_bars
                .create_keyed("test:raid", "x", BarColor::Pink, BarStyle::Solid, &[])
                .await;
            assert!(matches!(made, Err(Error::Rejected(_))));
        })
    })
    .await;
    let (request, _) = called.session.asked().await;
    called.session.say(answer(
        request,
        answer::Body::Refused(v1::Refused {
            reason: "taken".to_owned(),
        }),
    ));
    called.finished().await;
}
