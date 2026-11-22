mod common;

use common::*;
use rspigot_core::events::{DigAction, Direction, Inbound, PlayerInput};
use rspigot_core::plugins::events::TNTPrimeCause;
use rspigot_core::plugins::{Answer, BlockOrder, EventKind, EventPayload, Order, Question};
use rspigot_domain::BlockPos;

const TNT: BlockPos = BlockPos::new(10, 64, 10);
const NEAR: BlockPos = BlockPos::new(2, 64, 2);

fn shoot_burning_arrow(s: &mut Scene, burning: bool) {
    s.act(Order::Block {
        pos: TNT,
        order: BlockOrder::Set("minecraft:tnt".to_owned()),
    });
    let fire = if burning { "Fire:200s," } else { "" };
    s.server.handle(Inbound::Console(format!(
        "summon minecraft:arrow 10.5 64.2 9.2 {{{fire}Motion:[0.0d,0.0d,1.0d]}}"
    )));
    for _ in 0..20 {
        if !asked_for(&s.plugin, EventKind::TNTPrime).is_empty() {
            break;
        }
        s.tick(1);
    }
}

fn kind_at(s: &mut Scene, pos: BlockPos) -> String {
    match s.ask(Question::Block(pos)) {
        Answer::Block(state) => state.kind,
        other => panic!("expected a block, got {other:?}"),
    }
}

#[test]
fn a_burning_arrow_primes_a_tnt_block_with_the_projectile_cause() {
    let mut s = scene_with(&["Alice"], vec![observe(1, EventKind::TNTPrime)]);
    shoot_burning_arrow(&mut s, true);
    let events = settled_for(&s.plugin, EventKind::TNTPrime);
    assert_eq!(events.len(), 1);
    let EventPayload::TNTPrime(prime) = &events[0].payload else {
        panic!("expected a TNTPrime");
    };
    assert_eq!(prime.cause, TNTPrimeCause::Projectile);
    assert_eq!(prime.block, TNT);
    assert!(prime.priming_entity.is_some());
    assert_eq!(kind_at(&mut s, TNT), "minecraft:air");
}

#[test]
fn a_plain_arrow_leaves_a_tnt_block_alone() {
    let mut s = scene_with(&["Alice"], vec![observe(1, EventKind::TNTPrime)]);
    shoot_burning_arrow(&mut s, false);
    assert!(settled_for(&s.plugin, EventKind::TNTPrime).is_empty());
    assert_eq!(kind_at(&mut s, TNT), "minecraft:tnt");
}

#[test]
fn a_cancelled_projectile_prime_keeps_the_tnt_block() {
    let mut s = scene_with(&["Alice"], vec![deciding(1, EventKind::TNTPrime)]);
    shoot_burning_arrow(&mut s, true);
    let asked = last_asked(&s.plugin, EventKind::TNTPrime);
    judge(&mut s.server, 1, asked.id, 1, true, None);
    assert_eq!(kind_at(&mut s, TNT), "minecraft:tnt");
}

fn unstable_tnt_scene(mode: &str) -> (Scene, Player) {
    let mut s = scene_with(&["Alice"], vec![observe(1, EventKind::TNTPrime)]);
    let alice = s.players[0].clone();
    s.server
        .handle(Inbound::Console(format!("gamemode {mode} Alice")));
    s.act(Order::Block {
        pos: NEAR,
        order: BlockOrder::SetWith {
            kind: "minecraft:tnt".to_owned(),
            properties: vec![("unstable".to_owned(), "true".to_owned())],
        },
    });
    stream_chunk_of(&mut s, NEAR);
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Move {
            position: None,
            rotation: None,
            on_ground: true,
        },
    );
    (s, alice)
}

fn break_tnt(s: &mut Scene, alice: &Player) {
    for (action, sequence) in [(DigAction::Start, 1), (DigAction::Finish, 2)] {
        send_input(
            &mut s.server,
            alice,
            PlayerInput::Dig {
                action,
                pos: NEAR,
                face: Direction::Up,
                sequence,
            },
        );
        s.tick(2);
    }
}

#[test]
fn a_survival_player_breaking_an_unstable_tnt_primes_it_with_the_block_break_cause() {
    let (mut s, alice) = unstable_tnt_scene("survival");
    break_tnt(&mut s, &alice);
    let events = settled_for(&s.plugin, EventKind::TNTPrime);
    assert_eq!(events.len(), 1);
    let EventPayload::TNTPrime(prime) = &events[0].payload else {
        panic!("expected a NEARPrime");
    };
    assert_eq!(prime.cause, TNTPrimeCause::BlockBreak);
    assert!(prime.priming_entity.is_some());
    assert_eq!(kind_at(&mut s, NEAR), "minecraft:air");
}

#[test]
fn a_creative_player_breaking_an_unstable_tnt_primes_nothing() {
    let (mut s, alice) = unstable_tnt_scene("creative");
    break_tnt(&mut s, &alice);
    assert!(settled_for(&s.plugin, EventKind::TNTPrime).is_empty());
    assert_eq!(kind_at(&mut s, NEAR), "minecraft:air");
}
