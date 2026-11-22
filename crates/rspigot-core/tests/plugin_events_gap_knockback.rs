mod common;

use common::*;
use rspigot_core::events::{Hand, Inbound, PlayerInput};
use rspigot_core::plugins::events::EntityKnockbackCause;
use rspigot_core::plugins::{EventKind, EventPayload};
use rspigot_domain::{Rotation, Vec3};

const AIM: Rotation = Rotation::new(0.0, 0.0);

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn stand(s: &mut Scene) {
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Move {
            position: Some(Vec3::new(0.5, 64.0, 0.5)),
            rotation: Some(AIM),
            on_ground: true,
        },
    );
}

fn causes(s: &Scene) -> Vec<EntityKnockbackCause> {
    fired(&s.plugin)
        .into_iter()
        .filter_map(|event| match event.payload {
            EventPayload::EntityKnockback(knock) => Some(knock.cause),
            EventPayload::EntityKnockbackByEntity(knock) => Some(knock.cause),
            _ => None,
        })
        .collect()
}

#[test]
fn a_shield_block_knocks_the_attacker_back_with_the_shield_block_cause() {
    let mut s = scene_with(&["Alice"], vec![observe(1, EventKind::EntityKnockback)]);
    let alice = s.players[0].clone();
    settle_arrival(&mut s.server, &alice);
    console(&mut s, "gamemode survival Alice");
    console(&mut s, "difficulty normal");
    console(&mut s, "time set 18000");
    console(&mut s, "give Alice minecraft:shield");
    s.tick(2);
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::UseItem {
            hand: Hand::Main,
            sequence: 1,
            rotation: AIM,
        },
    );
    for _ in 0..8 {
        stand(&mut s);
        s.tick(1);
    }
    console(&mut s, "summon minecraft:zombie 0.5 64 1.5");
    for _ in 0..300 {
        stand(&mut s);
        s.tick(1);
        if causes(&s).contains(&EntityKnockbackCause::ShieldBlock) {
            break;
        }
    }
    assert!(causes(&s).contains(&EntityKnockbackCause::ShieldBlock));
}
