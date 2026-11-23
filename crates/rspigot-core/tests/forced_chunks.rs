mod common;

use std::time::Duration;

use common::*;
use rspigot_core::commands::{CommandOutcome, CommandSender, execute};
use rspigot_domain::BlockPos;
use rspigot_entities::EntityKind;

const FAR: BlockPos = BlockPos::new(408, 64, 408);

fn run(s: &mut Scene, line: &str) -> CommandOutcome {
    let outcome = execute(&mut s.server, CommandSender::Console, line);
    s.server.deliver();
    outcome
}

fn far_scene() -> Scene {
    let server = configured_level(
        |config| {
            config.simulation_distance = 1;
            config.rules.random_tick_speed = 1_000;
        },
        |_| {},
    );
    let mut s = scene_on(server, &["Alice"], Vec::new());
    assert_eq!(
        run(&mut s, "forceload add 400 400"),
        CommandOutcome::Success
    );
    s.server.wait_for_spawn(Duration::from_secs(10));
    assert!(
        s.server.block_state_at(FAR).is_some(),
        "the chunk never loaded"
    );
    s
}

#[test]
fn a_forced_chunk_far_from_players_simulates_its_entities() {
    let mut s = far_scene();
    run(
        &mut s,
        "summon minecraft:item 408.5 70 408.5 {Item:{id:\"minecraft:stone\",count:1}}",
    );
    let before = s.server.entities_of(EntityKind::Item)[0].position.y;
    s.tick(20);
    let after = s.server.entities_of(EntityKind::Item)[0].position.y;
    assert!(after < before - 0.5, "{before} -> {after}");
}

#[test]
fn a_forced_chunk_far_from_players_runs_random_ticks() {
    let mut s = far_scene();
    run(
        &mut s,
        "setblock 408 64 408 minecraft:oak_leaves[distance=7,persistent=false]",
    );
    for _ in 0..40 {
        s.tick(1);
        if s.server
            .block_state_at(FAR)
            .is_some_and(|state| state.is_air())
        {
            return;
        }
    }
    panic!("the leaves never decayed");
}
