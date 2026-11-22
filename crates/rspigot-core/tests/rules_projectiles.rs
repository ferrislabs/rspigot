mod common;

use common::*;
use rspigot_blocks::BlockKind;
use rspigot_core::events::{DigAction, Direction, Hand, Inbound, PlayerInput};
use rspigot_domain::{BlockPos, Rotation};

const AIM: Rotation = Rotation::new(0.0, 0.0);
const TARGET: BlockPos = BlockPos::new(0, 65, 6);

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn shoot_at(block: &str, rule: bool) -> Option<BlockKind> {
    let mut s = scene(&["Alice"]);
    stream_chunk_of(&mut s, BlockPos::new(1, 64, 1));
    console(
        &mut s,
        &format!("gamerule projectiles_can_break_blocks {rule}"),
    );
    console(&mut s, "setblock 0 64 6 minecraft:end_stone");
    console(&mut s, &format!("setblock 0 65 6 minecraft:{block}"));
    let alice = s.players[0].clone();
    console(&mut s, "give Alice minecraft:bow");
    console(&mut s, "give Alice minecraft:arrow 2");
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
    s.server.block_at(TARGET).and_then(|state| state.kind())
}

#[test]
fn an_arrow_breaks_a_chorus_flower_when_the_rule_is_true() {
    assert_ne!(
        shoot_at("chorus_flower", true),
        Some(BlockKind::ChorusFlower)
    );
}

#[test]
fn an_arrow_spares_a_chorus_flower_when_the_rule_is_false() {
    assert_eq!(
        shoot_at("chorus_flower", false),
        Some(BlockKind::ChorusFlower)
    );
}

#[test]
fn an_arrow_breaks_a_decorated_pot_when_the_rule_is_true() {
    assert_ne!(
        shoot_at("decorated_pot", true),
        Some(BlockKind::DecoratedPot)
    );
}

#[test]
fn an_arrow_spares_a_decorated_pot_when_the_rule_is_false() {
    assert_eq!(
        shoot_at("decorated_pot", false),
        Some(BlockKind::DecoratedPot)
    );
}
