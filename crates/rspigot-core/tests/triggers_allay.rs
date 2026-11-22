mod common;

use common::*;
use rspigot_core::events::{DigAction, Direction, Hand, Inbound, PlayerInput};
use rspigot_domain::{BlockPos, Vec3};
use rspigot_entities::EntityKind as Kind;

const NOTE: BlockPos = BlockPos::new(4, 64, 0);
const ADVANCEMENT: &str = "minecraft:husbandry/allay_deliver_cake_to_note_block";

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn done(s: &Scene) -> bool {
    s.server
        .done_advancements(s.players[0].id)
        .contains(&ADVANCEMENT)
}

fn play_note(s: &mut Scene) {
    let alice = s.players[0].clone();
    for action in [DigAction::Start, DigAction::Cancel] {
        send_input(
            &mut s.server,
            &alice,
            PlayerInput::Dig {
                action,
                pos: NOTE,
                face: Direction::Up,
                sequence: 1,
            },
        );
    }
}

fn scenario(give_cake: bool) -> Scene {
    let mut s = scene(&["Alice"]);
    s.tick(40);
    console(&mut s, "setblock 4 64 0 minecraft:note_block");
    console(&mut s, "summon minecraft:allay 1.5 65 0.5");
    let allay = s.server.entities_of(Kind::Allay)[0].id;
    if give_cake {
        console(&mut s, "give Alice minecraft:cake 1");
        let alice = s.players[0].clone();
        send_input(&mut s.server, &alice, PlayerInput::HeldSlot(0));
        s.tick(2);
        send_input(
            &mut s.server,
            &alice,
            PlayerInput::Interact {
                target: allay,
                hand: Hand::Main,
                sneaking: false,
                location: Vec3::new(0.0, 0.5, 0.0),
            },
        );
        s.tick(1);
        play_note(&mut s);
        s.tick(20);
        console(
            &mut s,
            "summon minecraft:item 1.5 65.2 0.5 {Item:{id:\"minecraft:cake\",count:1}}",
        );
        s.tick(30);
    }
    for _ in 0..40 {
        play_note(&mut s);
        s.tick(10);
        if done(&s) {
            break;
        }
    }
    s
}

#[test]
fn an_allay_carrying_cake_to_a_note_block_completes_the_advancement() {
    assert!(done(&scenario(true)));
}

#[test]
fn an_allay_holding_nothing_does_not() {
    assert!(!done(&scenario(false)));
}
