use rspigot_domain::player::PlayerId;
use rspigot_domain::{BlockPos, Vec3};

use super::{CommandContext, CommandOutcome, CommandSender, execute};
use crate::end_tests::{Tap, arrive};
use crate::events::ClientEvent;
use crate::server::Server;

struct Rig {
    server: Server<Tap>,
    id: PlayerId,
    link: Tap,
}

fn rig() -> Rig {
    let (mut server, id, link) = arrive();
    if let Some(player) = server.players.get_mut(&id) {
        player.operator = true;
    }
    Rig { server, id, link }
}

impl Rig {
    fn run(&mut self, line: &str) -> CommandOutcome {
        let outcome = execute(&mut self.server, CommandSender::Player(self.id), line);
        self.server.deliver();
        outcome
    }

    fn texts(&self) -> Vec<String> {
        self.link
            .take()
            .into_iter()
            .filter_map(|event| match event {
                ClientEvent::SystemChat { message, .. } => Some(message.plain_text()),
                _ => None,
            })
            .collect()
    }

    fn count(&self, selector: &str) -> usize {
        self.server
            .select_entities(CommandSender::Player(self.id), selector)
            .expect("the selector parses")
            .len()
    }

    fn position(&self) -> Vec3 {
        self.server.players[&self.id].position
    }
}

#[test]
fn positioned_relative_coordinates_follow_the_new_source() {
    let mut rig = rig();
    let outcome = rig.run("execute positioned 10 70 10 run tp @s ~ ~2 ~");
    assert_eq!(outcome, CommandOutcome::Success);
    assert_eq!(rig.position(), Vec3::new(10.5, 72.0, 10.5));
}

#[test]
fn as_and_at_fork_once_per_target() {
    let mut rig = rig();
    rig.run("summon minecraft:cow 5 70 5");
    rig.run("summon minecraft:cow 9 70 9");
    assert_eq!(rig.count("@e[type=cow]"), 2);
    let outcome = rig.run("execute as @e[type=cow] at @s run summon minecraft:pig ~ ~ ~");
    assert_eq!(outcome, CommandOutcome::Success);
    assert_eq!(rig.count("@e[type=pig]"), 2);
    let near = rig.count("@e[type=pig,x=5,y=70,z=5,distance=..1]");
    assert_eq!(near, 1);
}

#[test]
fn as_with_nothing_selected_runs_nothing() {
    let mut rig = rig();
    let outcome = rig.run("execute as @e[type=creeper] run summon minecraft:pig 0 70 0");
    assert_eq!(outcome, CommandOutcome::Failure);
    assert_eq!(rig.count("@e[type=pig]"), 0);
}

#[test]
fn conditions_filter_and_report_when_last() {
    let mut rig = rig();
    rig.run("summon minecraft:cow 5 70 5");
    rig.texts();
    assert_eq!(
        rig.run("execute if entity @e[type=cow]"),
        CommandOutcome::Success
    );
    assert_eq!(rig.texts(), ["commands.execute.conditional.pass_count 1"]);
    assert_eq!(
        rig.run("execute unless entity @e[type=cow]"),
        CommandOutcome::Failure
    );
    assert_eq!(rig.texts(), ["commands.execute.conditional.fail_count 1"]);
    assert_eq!(
        rig.run("execute if entity @e[type=creeper]"),
        CommandOutcome::Failure
    );
    assert_eq!(rig.texts(), ["commands.execute.conditional.fail"]);
    assert_eq!(
        rig.run("execute unless entity @e[type=creeper] run summon minecraft:pig 0 70 0"),
        CommandOutcome::Success
    );
    assert_eq!(rig.count("@e[type=pig]"), 1);
    assert_eq!(
        rig.run("execute if entity @e[type=creeper] run summon minecraft:pig 0 70 0"),
        CommandOutcome::Failure
    );
    assert_eq!(rig.count("@e[type=pig]"), 1);
}

#[test]
fn block_and_loaded_conditions_read_the_world() {
    let mut rig = rig();
    let here = rig.position().block();
    let state = rig.server.block_at(here).expect("the chunk is loaded");
    let name = state.kind().expect("a block").name();
    let line = format!("execute if block {} {} {} {name}", here.x, here.y, here.z);
    assert_eq!(rig.run(&line), CommandOutcome::Success);
    let wrong = format!(
        "execute unless block {} {} {} {name}",
        here.x, here.y, here.z
    );
    assert_eq!(rig.run(&wrong), CommandOutcome::Failure);
    assert_eq!(rig.run("execute if loaded ~ ~ ~"), CommandOutcome::Success);
    assert_eq!(
        rig.run("execute if loaded 1000000 0 1000000"),
        CommandOutcome::Failure
    );
    rig.texts();
    assert_eq!(
        rig.run("execute if block 1000000 0 1000000 stone"),
        CommandOutcome::Failure
    );
    assert_eq!(rig.texts(), ["argument.pos.unloaded"]);
}

#[test]
fn blocks_compares_two_regions() {
    let mut rig = rig();
    let at = rig.position().block();
    let line = format!(
        "execute if blocks {x} {y} {z} {x} {y} {z} {x} {y} {z} all",
        x = at.x,
        y = at.y,
        z = at.z
    );
    assert_eq!(rig.run(&line), CommandOutcome::Success);
    assert_eq!(rig.texts(), ["commands.execute.conditional.pass_count 1"]);
    let big = format!(
        "execute if blocks {} 0 {} {} 200 {} {} 0 {} all",
        at.x,
        at.z,
        at.x + 100,
        at.z + 100,
        at.x,
        at.z
    );
    assert_eq!(rig.run(&big), CommandOutcome::Failure);
    assert_eq!(rig.texts().len(), 1);
}

#[test]
fn align_anchored_rotated_and_facing_reshape_the_source() {
    let mut rig = rig();
    rig.run("execute positioned 1.7 2.9 3.1 align xz run tp @s ~ ~ ~");
    assert_eq!(rig.position(), Vec3::new(1.0, 2.9, 3.0));
    rig.run("execute positioned 0.0 64.0 0.0 rotated 90 0 run tp @s ^ ^ ^1");
    let p = rig.position();
    assert!(
        (p.x + 1.0).abs() < 1e-3 && (p.y - 64.0).abs() < 1e-3 && p.z.abs() < 1e-3,
        "{p:?}"
    );
    rig.run("execute positioned 0.0 64.0 0.0 facing 0.0 64.0 10.0 run tp @s ^ ^ ^1");
    let p = rig.position();
    assert!(p.x.abs() < 1e-3 && (p.z - 1.0).abs() < 1e-3, "{p:?}");
    rig.run("execute positioned 0.0 64.0 0.0 anchored eyes facing 0.0 65.62 10.0 run tp @s ^ ^ ^1");
    let p = rig.position();
    assert!(p.z > 0.9 && (p.y - 65.62).abs() < 1e-3, "{p:?}");
}

#[test]
fn store_writes_result_and_success_to_command_storage() {
    let mut rig = rig();
    assert_eq!(
        rig.run("execute store success storage test:s ok byte 1 run summon minecraft:pig 0 70 0"),
        CommandOutcome::Success
    );
    assert_eq!(
        rig.run("execute if data storage test:s {ok:1b}"),
        CommandOutcome::Success
    );
    assert_eq!(
        rig.run(
            "execute store success storage test:s ok byte 1 run summon minecraft:nothing 0 0 0"
        ),
        CommandOutcome::Failure
    );
    assert_eq!(
        rig.run("execute if data storage test:s {ok:0b}"),
        CommandOutcome::Success
    );
    rig.run("execute store result storage test:s n int 3 run time set 10");
    rig.texts();
    assert_eq!(
        rig.run("execute if data storage test:s {n:30}"),
        CommandOutcome::Success
    );
    assert_eq!(rig.texts(), ["commands.execute.conditional.pass_count 1"]);
}

#[test]
fn scoreboard_and_bossbar_targets_are_reported_as_unknown() {
    let mut rig = rig();
    rig.texts();
    assert_eq!(
        rig.run("execute store result score @s kills run time set 1"),
        CommandOutcome::Failure
    );
    assert_eq!(
        rig.run("execute if score @s kills matches 1.."),
        CommandOutcome::Failure
    );
    assert_eq!(
        rig.run("execute store result bossbar test:bar value run time set 1"),
        CommandOutcome::Success
    );
    let shown = rig.texts();
    assert_eq!(shown[0], "arguments.objective.notFound kills");
    assert_eq!(shown[1], "arguments.objective.notFound kills");
    assert_eq!(shown[3], "commands.bossbar.unknown test:bar");
}

#[test]
fn summon_makes_the_new_entity_the_executor() {
    let mut rig = rig();
    assert_eq!(
        rig.run("execute positioned 0 70 0 summon minecraft:cow run tp @s ~ ~ ~5"),
        CommandOutcome::Success
    );
    assert_eq!(rig.count("@e[type=cow,x=0,y=70,z=5,distance=..1]"), 1);
}

#[test]
fn on_follows_passenger_and_vehicle_relations() {
    let mut rig = rig();
    assert_eq!(
        rig.run("execute on vehicle run say riding"),
        CommandOutcome::Failure
    );
    assert_eq!(
        rig.run("execute on passengers run say carried"),
        CommandOutcome::Failure
    );
}

#[test]
fn syntax_problems_are_reported_once() {
    let mut rig = rig();
    rig.texts();
    assert_eq!(rig.run("execute"), CommandOutcome::Failure);
    assert_eq!(rig.run("execute as @s"), CommandOutcome::Failure);
    assert_eq!(
        rig.run("execute as @e[type=nope] run say x"),
        CommandOutcome::Failure
    );
    assert_eq!(
        rig.run("execute anchored hands run say x"),
        CommandOutcome::Failure
    );
    assert_eq!(
        rig.run("execute align w run say x"),
        CommandOutcome::Failure
    );
    let shown = rig.texts();
    assert_eq!(shown[0], "command.unknown.command");
    assert_eq!(shown[2], "command.unknown.command");
    assert_eq!(
        shown[4],
        "argument.entity.options.type.invalid minecraft:nope"
    );
    assert_eq!(shown[5], "argument.anchor.invalid hands");
    assert_eq!(shown[6], "arguments.swizzle.invalid");
}

#[test]
fn the_player_position_is_a_block_position_for_conditions() {
    let rig = rig();
    let at = rig.position().block();
    assert!(
        rig.server
            .is_position_loaded(BlockPos::new(at.x, at.y, at.z))
    );
}
