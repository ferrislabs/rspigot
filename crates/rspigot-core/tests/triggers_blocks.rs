mod common;

use common::*;
use rspigot_core::events::{DigAction, Direction, Inbound, PlayerInput};
use rspigot_domain::BlockPos;

const NEST: BlockPos = BlockPos::new(2, 64, 0);
const ADVANCEMENT: &str = "minecraft:husbandry/silk_touch_nest";

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn done(s: &Scene) -> bool {
    s.server
        .done_advancements(s.players[0].id)
        .contains(&ADVANCEMENT)
}

fn dig(s: &mut Scene, action: DigAction) {
    let player = s.players[0].clone();
    send_input(
        &mut s.server,
        &player,
        PlayerInput::Dig {
            action,
            pos: NEST,
            face: Direction::Up,
            sequence: 4,
        },
    );
}

fn nest_with(bees: usize) -> String {
    let bee = "{entity_data:{id:\"minecraft:bee\"},min_ticks_in_hive:600,ticks_in_hive:0}";
    let list = vec![bee; bees].join(",");
    format!("setblock 2 64 0 minecraft:bee_nest{{bees:[{list}]}}")
}

fn mine(silk: bool, bees: usize) -> Scene {
    let mut s = scene(&["Alice"]);
    s.tick(40);
    console(&mut s, "give Alice diamond_pickaxe 1");
    if silk {
        console(&mut s, "enchant Alice minecraft:silk_touch 1");
    }
    console(&mut s, &nest_with(bees));
    s.tick(2);
    let player = s.players[0].clone();
    send_input(&mut s.server, &player, PlayerInput::HeldSlot(0));
    dig(&mut s, DigAction::Start);
    s.tick(60);
    dig(&mut s, DigAction::Finish);
    s.tick(2);
    s
}

#[test]
fn mining_a_full_nest_with_silk_touch_completes_silk_touch_nest() {
    let s = mine(true, 3);
    assert!(done(&s));
}

#[test]
fn mining_a_full_nest_without_silk_touch_does_not() {
    let s = mine(false, 3);
    assert!(!done(&s));
}

#[test]
fn mining_a_nest_with_too_few_bees_does_not() {
    let s = mine(true, 2);
    assert!(!done(&s));
}
