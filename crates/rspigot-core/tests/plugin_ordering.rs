mod common;

use common::*;
use rspigot_core::events::Inbound;
use rspigot_core::plugins::{Answer, FromPlugin, PlayerOrder, Question, ToPlugin};
use rspigot_domain::Vec3;

fn position_asked_after(s: &mut Scene, player: &Player, orders: &[PlayerOrder]) -> Vec3 {
    for order in orders {
        s.server.handle(Inbound::PluginMessage {
            connection: 1,
            message: Box::new(FromPlugin::Order(to(player, order.clone()))),
        });
    }
    let request = 9_000_001;
    s.server.handle(Inbound::PluginMessage {
        connection: 1,
        message: Box::new(FromPlugin::Question {
            request,
            question: Question::Player(player.id),
        }),
    });
    s.server.deliver();
    let answer = s
        .plugin
        .received()
        .into_iter()
        .find_map(|message| match message {
            ToPlugin::Answer {
                request: answered,
                answer: Answer::Player(state),
            } if answered == request => Some(state),
            _ => None,
        })
        .expect("the server answers the question");
    answer.player.position
}

#[test]
fn a_read_sees_the_effect_of_the_order_sent_before_it() {
    let mut s = scene(&["Alice", "Bob"]);
    let alice = s.players[0].clone();
    let there = Vec3::new(10.5, 64.0, 10.5);
    assert_eq!(
        position_asked_after(&mut s, &alice, &[PlayerOrder::Teleport(there)]),
        there
    );
}

#[test]
fn successive_orders_on_one_player_apply_in_the_order_received() {
    let mut s = scene(&["Alice", "Bob"]);
    let alice = s.players[0].clone();
    let first = Vec3::new(1.5, 64.0, 1.5);
    let second = Vec3::new(-7.5, 70.0, 3.5);
    let last = Vec3::new(20.5, 65.0, -4.5);
    let orders = [
        PlayerOrder::Teleport(first),
        PlayerOrder::Teleport(second),
        PlayerOrder::Teleport(last),
    ];
    assert_eq!(position_asked_after(&mut s, &alice, &orders), last);
    assert_eq!(s.server.player_location(alice.id), Some(last));
}
