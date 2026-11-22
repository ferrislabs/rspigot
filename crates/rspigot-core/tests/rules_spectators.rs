mod common;

use std::time::Duration;

use common::*;
use rspigot_core::events::{ClientEvent, Inbound, PlayerInput};
use rspigot_domain::{BlockPos, ChunkPos};

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn chunks_after_teleport(rule: bool, mode: &str) -> Vec<ChunkPos> {
    let mut s = scene(&["Alice"]);
    stream_chunk_of(&mut s, BlockPos::new(1, 64, 1));
    console(
        &mut s,
        &format!("gamerule spectators_generate_chunks {rule}"),
    );
    console(&mut s, &format!("gamemode {mode} Alice"));
    s.tick(2);
    s.players[0].link.take();
    console(&mut s, "tp Alice 320 64 0");
    let mut seen = Vec::new();
    for _ in 0..60 {
        s.server.wait_for_spawn(Duration::from_millis(20));
        s.tick(1);
        let alice = s.players[0].clone();
        for event in alice.link.take() {
            match event {
                ClientEvent::Chunk { chunk, .. } => seen.push(chunk.pos()),
                ClientEvent::Teleport { id, .. } => {
                    send_input(&mut s.server, &alice, PlayerInput::TeleportConfirm(id));
                }
                ClientEvent::ChunkBatchFinished { .. } => {
                    send_input(
                        &mut s.server,
                        &alice,
                        PlayerInput::ChunkBatchReceived {
                            chunks_per_tick: 64.0,
                        },
                    );
                }
                _ => {}
            }
        }
    }
    seen
}

#[test]
fn a_survival_player_far_away_receives_new_chunks() {
    assert!(!chunks_after_teleport(false, "survival").is_empty());
}

#[test]
fn a_spectator_receives_new_chunks_when_the_rule_is_true() {
    assert!(!chunks_after_teleport(true, "spectator").is_empty());
}

#[test]
fn a_spectator_receives_no_new_chunks_when_the_rule_is_false() {
    assert!(chunks_after_teleport(false, "spectator").is_empty());
}
