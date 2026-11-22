mod common;

use common::*;
use rspigot_core::events::{Inbound, PlayerInput};
use rspigot_domain::Vec3;

const ADVANCEMENT: &str = "minecraft:adventure/who_needs_rockets";

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn done(s: &Scene) -> bool {
    s.server
        .done_advancements(s.players[0].id)
        .contains(&ADVANCEMENT)
}

#[test]
fn a_wind_charge_launch_that_rises_seven_blocks_completes_who_needs_rockets() {
    let mut s = scene(&["Alice"]);
    s.tick(40);
    for _ in 0..6 {
        console(
            &mut s,
            "summon minecraft:wind_charge 0.5 64.3 0.5 {Motion:[0.0,-0.5,0.0]}",
        );
        s.tick(1);
    }
    s.tick(3);
    for y in [66.0, 70.0, 73.0, 72.0, 70.0, 66.0, 64.0] {
        let player = s.players[0].clone();
        send_input(
            &mut s.server,
            &player,
            PlayerInput::Move {
                position: Some(Vec3::new(0.5, y, 0.5)),
                rotation: None,
                on_ground: y == 64.0,
            },
        );
        s.tick(1);
    }
    assert!(done(&s));
}

#[test]
fn standing_still_does_not_complete_who_needs_rockets() {
    let mut s = scene(&["Alice"]);
    s.tick(100);
    assert!(!done(&s));
}
