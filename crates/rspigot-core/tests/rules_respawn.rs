mod common;

use common::*;
use rspigot_core::events::{ClientEvent, Inbound, PlayerInput};
use rspigot_domain::{BlockPos, Vec3};

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn respawn_spots(radius: i32, deaths: usize) -> Vec<Vec3> {
    let mut s = scene(&["Alice"]);
    stream_chunk_of(&mut s, BlockPos::new(1, 64, 1));
    console(&mut s, &format!("gamerule respawn_radius {radius}"));
    let alice = s.players[0].clone();
    let mut spots = Vec::new();
    for _ in 0..deaths {
        console(&mut s, "kill Alice");
        s.tick(30);
        alice.link.take();
        send_input(&mut s.server, &alice, PlayerInput::Respawn);
        for event in alice.link.take() {
            if let ClientEvent::Teleport { id, position, .. } = event {
                spots.push(position);
                send_input(&mut s.server, &alice, PlayerInput::TeleportConfirm(id));
            }
        }
        send_input(&mut s.server, &alice, PlayerInput::Loaded);
        s.tick(2);
    }
    spots
}

#[test]
fn a_zero_radius_respawns_exactly_on_the_world_spawn() {
    let spots = respawn_spots(0, 3);
    assert_eq!(spots.len(), 3);
    assert!(spots.iter().all(|p| *p == Vec3::new(0.5, 64.0, 0.5)));
}

#[test]
fn a_radius_scatters_respawns_around_the_world_spawn_within_that_radius() {
    let spots = respawn_spots(6, 12);
    assert_eq!(spots.len(), 12);
    assert!(
        spots
            .iter()
            .all(|p| (p.x - 0.5).abs() <= 6.0 && (p.z - 0.5).abs() <= 6.0 && p.y == 64.0)
    );
    assert!(spots.iter().any(|p| *p != Vec3::new(0.5, 64.0, 0.5)));
}
