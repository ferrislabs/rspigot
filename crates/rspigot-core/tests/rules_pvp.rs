mod common;

use common::*;
use rspigot_core::events::{DigAction, Direction, Hand, Inbound, PlayerInput};
use rspigot_domain::{BlockPos, Rotation, Vec3};

const AIM: Rotation = Rotation::new(0.0, 0.0);

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn duel(pvp: bool) -> Scene {
    let mut s = scene(&["Alice", "Bob"]);
    stream_chunk_of(&mut s, BlockPos::new(1, 64, 1));
    console(&mut s, &format!("gamerule pvp {pvp}"));
    let bob = s.players[1].clone();
    send_input(
        &mut s.server,
        &bob,
        PlayerInput::Move {
            position: Some(Vec3::new(0.5, 64.0, 6.5)),
            rotation: Some(Rotation::new(0.0, 0.0)),
            on_ground: true,
        },
    );
    s.tick(2);
    s
}

fn shoot(s: &mut Scene) {
    let alice = s.players[0].clone();
    console(s, "give Alice minecraft:bow");
    console(s, "give Alice minecraft:arrow 2");
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::UseItem {
            hand: Hand::Main,
            sequence: 1,
            rotation: AIM,
        },
    );
    s.tick(25);
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Dig {
            action: DigAction::ReleaseUseItem,
            pos: BlockPos::new(0, 0, 0),
            face: Direction::Down,
            sequence: 0,
        },
    );
    s.tick(15);
}

fn bob_health(s: &Scene) -> f32 {
    s.server.player_health(s.players[1].id).unwrap_or(-1.0)
}

#[test]
fn with_pvp_on_a_players_arrow_hurts_another_player() {
    let mut s = duel(true);
    shoot(&mut s);
    assert!(bob_health(&s) < 20.0);
}

#[test]
fn with_pvp_off_a_players_arrow_spares_another_player() {
    let mut s = duel(false);
    shoot(&mut s);
    assert_eq!(bob_health(&s), 20.0);
}
