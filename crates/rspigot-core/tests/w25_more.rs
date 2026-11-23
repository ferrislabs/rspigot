mod common;
#[path = "common/screen.rs"]
mod screen;

use common::*;
use rspigot_core::config::PluginSettings;
use rspigot_core::events::{DigAction, Direction, Inbound, PlayerInput};
use rspigot_core::plugins::{Answer, Question};
use rspigot_domain::BlockPos;
use screen::use_block;

const AT: BlockPos = BlockPos::new(2, 64, 2);

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn ready() -> Scene {
    let mut s = scene_on(
        server(PluginSettings::default()).with_rng_seed(5),
        &["Alice"],
        Vec::new(),
    );
    stream_chunk_of(&mut s, AT);
    s.tick(2);
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Move {
            position: None,
            rotation: None,
            on_ground: true,
        },
    );
    s.tick(1);
    s
}

fn kind(s: &mut Scene, pos: BlockPos) -> String {
    match s.ask(Question::Block(pos)) {
        Answer::Block(state) => state.kind,
        other => panic!("expected a block, got {other:?}"),
    }
}

fn prop(s: &mut Scene, pos: BlockPos, name: &str) -> String {
    match s.ask(Question::Block(pos)) {
        Answer::Block(state) => state
            .properties
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.clone())
            .unwrap_or_default(),
        other => panic!("expected a block, got {other:?}"),
    }
}

fn mine(s: &mut Scene, pos: BlockPos) {
    let alice = s.players[0].clone();
    for (action, wait) in [(DigAction::Start, 40), (DigAction::Finish, 2)] {
        send_input(
            &mut s.server,
            &alice,
            PlayerInput::Dig {
                action,
                pos,
                face: Direction::Up,
                sequence: 9,
            },
        );
        s.tick(wait);
    }
}

#[test]
fn ice_mined_by_hand_over_a_solid_block_melts_into_water() {
    let mut s = ready();
    console(&mut s, "gamemode survival Alice");
    console(&mut s, "setblock 2 64 2 ice");
    mine(&mut s, AT);
    assert_eq!(kind(&mut s, AT), "minecraft:water");
}

#[test]
fn a_light_block_clicked_with_a_light_item_cycles_its_level() {
    let mut s = ready();
    console(&mut s, "gamemode creative Alice");
    console(&mut s, "setblock 2 64 2 light[level=3]");
    console(&mut s, "give Alice minecraft:light 1");
    s.tick(1);
    let alice = s.players[0].clone();
    use_block(&mut s, &alice, AT);
    assert_eq!(prop(&mut s, AT, "level"), "4");
}
