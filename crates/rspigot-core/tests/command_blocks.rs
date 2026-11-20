mod common;

use common::*;
use rspigot_core::events::{CommandBlockMode, Inbound, PlayerInput};
use rspigot_core::plugins::{Answer, Question};
use rspigot_domain::{BlockPos, EntityId};
use rspigot_entities::EntityKind;
use rspigot_nbt::Compound;

const BLOCK: BlockPos = BlockPos::new(3, 64, 3);

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn operator() -> Scene {
    let mut s = scene(&["Alice"]);
    console(&mut s, "op Alice");
    console(&mut s, "gamemode creative Alice");
    s
}

fn block_name(s: &mut Scene, pos: BlockPos) -> String {
    match s.ask(Question::Block(pos)) {
        Answer::Block(state) => state.kind,
        other => panic!("expected a block, got {other:?}"),
    }
}

fn data(s: &Scene, pos: BlockPos) -> Compound {
    s.server.block_entity_nbt(pos).expect("a block entity")
}

fn place(s: &mut Scene, pos: BlockPos, state: &str) {
    console(
        s,
        &format!("setblock {} {} {} {state}", pos.x, pos.y, pos.z),
    );
}

fn edit(
    s: &mut Scene,
    pos: BlockPos,
    command: &str,
    mode: CommandBlockMode,
    conditional: bool,
    automatic: bool,
) {
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::SetCommandBlock {
            pos,
            command: command.to_owned(),
            mode,
            track_output: true,
            conditional,
            automatic,
        },
    );
}

fn last_execution(s: &Scene, pos: BlockPos) -> i64 {
    data(s, pos).get_i64("LastExecution").unwrap()
}

fn impulse(s: &mut Scene, command: &str) {
    place(s, BLOCK, "minecraft:command_block[facing=east]");
    edit(s, BLOCK, command, CommandBlockMode::Redstone, false, false);
}

#[test]
fn an_impulse_block_runs_once_on_a_rising_edge() {
    let mut s = operator();
    impulse(&mut s, "setblock 3 64 8 minecraft:gold_block");
    s.tick(2);
    assert_eq!(block_name(&mut s, BlockPos::new(3, 64, 8)), "minecraft:air");
    place(&mut s, BlockPos::new(3, 64, 4), "minecraft:redstone_block");
    s.tick(2);
    assert_eq!(
        block_name(&mut s, BlockPos::new(3, 64, 8)),
        "minecraft:gold_block"
    );
    let first = last_execution(&s, BLOCK);
    assert!(first >= 0);
    s.tick(10);
    assert_eq!(last_execution(&s, BLOCK), first);
    place(&mut s, BlockPos::new(3, 64, 4), "minecraft:air");
    s.tick(3);
    place(&mut s, BlockPos::new(3, 64, 4), "minecraft:redstone_block");
    s.tick(3);
    assert!(last_execution(&s, BLOCK) > first);
}

#[test]
fn a_repeating_block_runs_every_tick_while_powered() {
    let mut s = operator();
    place(
        &mut s,
        BLOCK,
        "minecraft:repeating_command_block[facing=east]",
    );
    edit(&mut s, BLOCK, "seed", CommandBlockMode::Auto, false, false);
    s.tick(3);
    assert_eq!(last_execution(&s, BLOCK), -1);
    place(&mut s, BlockPos::new(3, 64, 4), "minecraft:redstone_block");
    s.tick(2);
    let mut previous = last_execution(&s, BLOCK);
    for _ in 0..5 {
        s.tick(1);
        let now = last_execution(&s, BLOCK);
        assert_eq!(now, previous + 1);
        previous = now;
    }
    place(&mut s, BlockPos::new(3, 64, 4), "minecraft:air");
    s.tick(3);
    let stopped = last_execution(&s, BLOCK);
    s.tick(3);
    assert_eq!(last_execution(&s, BLOCK), stopped);
}

#[test]
fn an_always_active_repeating_block_runs_without_power() {
    let mut s = operator();
    place(
        &mut s,
        BLOCK,
        "minecraft:repeating_command_block[facing=east]",
    );
    edit(&mut s, BLOCK, "seed", CommandBlockMode::Auto, false, true);
    s.tick(4);
    assert!(last_execution(&s, BLOCK) >= 0);
}

fn chain_at(s: &mut Scene, x: i32, command: &str, conditional: bool) {
    let pos = BlockPos::new(x, 64, 3);
    let state = if conditional {
        "minecraft:chain_command_block[facing=east,conditional=true]"
    } else {
        "minecraft:chain_command_block[facing=east]"
    };
    place(s, pos, state);
    edit(
        s,
        pos,
        command,
        CommandBlockMode::Sequence,
        conditional,
        true,
    );
}

#[test]
fn a_chain_runs_in_order_and_stops_at_a_failing_conditional() {
    let mut s = operator();
    impulse(&mut s, "setblock 3 64 8 minecraft:gold_block");
    chain_at(&mut s, 4, "setblock 3 64 9 minecraft:iron_block", false);
    chain_at(&mut s, 5, "nosuchcommand", true);
    chain_at(&mut s, 6, "setblock 3 64 10 minecraft:emerald_block", true);
    chain_at(&mut s, 7, "setblock 3 64 11 minecraft:lapis_block", false);
    place(&mut s, BlockPos::new(3, 64, 4), "minecraft:redstone_block");
    s.tick(3);
    assert_eq!(
        block_name(&mut s, BlockPos::new(3, 64, 8)),
        "minecraft:gold_block"
    );
    assert_eq!(
        block_name(&mut s, BlockPos::new(3, 64, 9)),
        "minecraft:iron_block"
    );
    assert_eq!(
        data(&s, BlockPos::new(5, 64, 3)).get_i32("SuccessCount"),
        Some(0)
    );
    assert_eq!(
        block_name(&mut s, BlockPos::new(3, 64, 10)),
        "minecraft:air"
    );
    assert_eq!(
        data(&s, BlockPos::new(6, 64, 3)).get_i32("SuccessCount"),
        Some(0)
    );
    assert_eq!(
        block_name(&mut s, BlockPos::new(3, 64, 11)),
        "minecraft:lapis_block"
    );
}

#[test]
fn a_conditional_block_does_not_run_after_a_failure() {
    let mut s = operator();
    impulse(&mut s, "nosuchcommand");
    chain_at(&mut s, 4, "setblock 3 64 9 minecraft:iron_block", true);
    place(&mut s, BlockPos::new(3, 64, 4), "minecraft:redstone_block");
    s.tick(3);
    assert_eq!(block_name(&mut s, BlockPos::new(3, 64, 9)), "minecraft:air");
    assert_eq!(
        data(&s, BlockPos::new(4, 64, 3)).get_i32("SuccessCount"),
        Some(0)
    );
}

#[test]
fn the_success_count_and_last_output_are_stored() {
    let mut s = operator();
    impulse(&mut s, "seed");
    place(&mut s, BlockPos::new(3, 64, 4), "minecraft:redstone_block");
    s.tick(3);
    let stored = data(&s, BLOCK);
    assert_eq!(stored.get_i32("SuccessCount"), Some(1));
    assert!(stored.contains_key("LastOutput"));
    assert_eq!(stored.get_str("Command"), Some("seed"));
    assert_eq!(stored.get_bool("TrackOutput"), Some(true));
    assert_eq!(stored.get_bool("powered"), Some(true));
}

#[test]
fn a_non_operator_cannot_edit_a_command_block() {
    let mut s = operator();
    place(&mut s, BLOCK, "minecraft:command_block[facing=east]");
    console(&mut s, "deop Alice");
    edit(
        &mut s,
        BLOCK,
        "seed",
        CommandBlockMode::Redstone,
        false,
        false,
    );
    assert_eq!(data(&s, BLOCK).get_str("Command"), Some(""));
    let alice = s.players[0].clone();
    assert!(
        system_texts(&alice)
            .iter()
            .any(|text| text.contains("not allowed") || text.contains("advMode.notAllowed"))
    );
}

#[test]
fn a_survival_operator_cannot_edit_a_command_block() {
    let mut s = operator();
    place(&mut s, BLOCK, "minecraft:command_block[facing=east]");
    console(&mut s, "gamemode survival Alice");
    edit(
        &mut s,
        BLOCK,
        "seed",
        CommandBlockMode::Redstone,
        false,
        false,
    );
    assert_eq!(data(&s, BLOCK).get_str("Command"), Some(""));
}

#[test]
fn an_edit_changes_the_block_type_and_the_conditional_state() {
    let mut s = operator();
    place(&mut s, BLOCK, "minecraft:command_block[facing=east]");
    edit(
        &mut s,
        BLOCK,
        "seed",
        CommandBlockMode::Sequence,
        true,
        false,
    );
    assert_eq!(block_name(&mut s, BLOCK), "minecraft:chain_command_block");
    let alice = s.players[0].clone();
    assert!(
        system_texts(&alice)
            .iter()
            .any(|text| text.contains("seed"))
    );
    edit(&mut s, BLOCK, "seed", CommandBlockMode::Auto, false, false);
    assert_eq!(
        block_name(&mut s, BLOCK),
        "minecraft:repeating_command_block"
    );
    assert_eq!(data(&s, BLOCK).get_str("Command"), Some("seed"));
}

#[test]
fn command_blocks_do_nothing_when_the_game_rule_is_off() {
    let mut s = operator();
    impulse(&mut s, "setblock 3 64 8 minecraft:gold_block");
    console(&mut s, "gamerule command_blocks_work false");
    place(&mut s, BlockPos::new(3, 64, 4), "minecraft:redstone_block");
    s.tick(3);
    assert_eq!(block_name(&mut s, BlockPos::new(3, 64, 8)), "minecraft:air");
    assert_eq!(data(&s, BLOCK).get_i32("SuccessCount"), Some(0));
}

#[test]
fn a_command_minecart_on_a_powered_activator_rail_runs_every_four_ticks() {
    let mut s = operator();
    place(&mut s, BlockPos::new(3, 64, 3), "minecraft:activator_rail");
    place(&mut s, BlockPos::new(3, 63, 3), "minecraft:redstone_block");
    s.tick(2);
    console(
        &mut s,
        "summon minecraft:command_block_minecart 3.5 64.1 3.5",
    );
    s.tick(1);
    let id: EntityId = s.server.entities_of(EntityKind::CommandBlockMinecart)[0].id;
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::SetCommandMinecart {
            entity: id,
            command: "seed".to_owned(),
            track_output: true,
        },
    );
    let mut runs = Vec::new();
    let mut last = -1;
    for _ in 0..24 {
        s.tick(1);
        let now = s
            .server
            .saved_entity(id)
            .and_then(|nbt| nbt.get_i64("LastExecution"))
            .unwrap_or(-1);
        if now != last {
            runs.push(now);
            last = now;
        }
    }
    assert!(runs.len() >= 4, "{runs:?}");
    for pair in runs.windows(2).skip(1) {
        assert_eq!(pair[1] - pair[0], 4, "{runs:?}");
    }
    let saved = s.server.saved_entity(id).unwrap();
    assert_eq!(saved.get_str("Command"), Some("seed"));
    assert_eq!(saved.get_i32("SuccessCount"), Some(1));
}
