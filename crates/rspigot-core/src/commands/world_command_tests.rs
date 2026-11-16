use rspigot_domain::player::PlayerId;

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
        player.op_level = 4;
    }
    server.deliver();
    link.take();
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
}

#[test]
fn forceload_add_marks_the_chunks_of_a_column_range_and_names_them() {
    let mut rig = rig();
    assert_eq!(rig.run("forceload add 20 20"), CommandOutcome::Success);
    assert_eq!(
        rig.texts(),
        ["commands.forceload.added.single [1, 1] minecraft:overworld"]
    );
    assert!(rig.server.is_chunk_forced(1, 1));
    assert_eq!(rig.run("forceload add 0 0 40 20"), CommandOutcome::Success);
    assert_eq!(
        rig.texts(),
        ["commands.forceload.added.multiple 5 minecraft:overworld [0, 0] [2, 1]"]
    );
    assert_eq!(rig.run("forceload add 0 0"), CommandOutcome::Failure);
    assert_eq!(rig.texts(), ["commands.forceload.added.failure"]);
}

#[test]
fn forceload_remove_and_remove_all_release_the_chunks() {
    let mut rig = rig();
    rig.run("forceload add 0 0 31 31");
    rig.texts();
    assert_eq!(rig.run("forceload remove 0 0"), CommandOutcome::Success);
    assert_eq!(
        rig.texts(),
        ["commands.forceload.removed.single [0, 0] minecraft:overworld"]
    );
    assert_eq!(rig.run("forceload remove 0 0"), CommandOutcome::Failure);
    assert_eq!(rig.run("forceload remove all"), CommandOutcome::Success);
    assert_eq!(
        rig.texts()[1],
        "commands.forceload.removed.all minecraft:overworld"
    );
    assert!(rig.server.forced_chunks().is_empty());
}

#[test]
fn forceload_query_lists_or_answers_for_one_column() {
    let mut rig = rig();
    assert_eq!(rig.run("forceload query"), CommandOutcome::Failure);
    assert_eq!(
        rig.texts(),
        ["commands.forceload.added.none minecraft:overworld"]
    );
    rig.run("forceload add 0 0");
    rig.run("forceload add 16 0");
    rig.texts();
    assert_eq!(rig.run("forceload query"), CommandOutcome::Success);
    assert_eq!(
        rig.texts(),
        ["commands.forceload.list.multiple 2 minecraft:overworld [0, 0], [1, 0]"]
    );
    assert_eq!(rig.run("forceload query 3 3"), CommandOutcome::Success);
    assert_eq!(
        rig.texts(),
        ["commands.forceload.query.success [0, 0] minecraft:overworld"]
    );
    assert_eq!(rig.run("forceload query 100 100"), CommandOutcome::Failure);
    assert_eq!(
        rig.texts(),
        ["commands.forceload.query.failure [6, 6] minecraft:overworld"]
    );
}

#[test]
fn forceload_refuses_a_range_beyond_256_chunks_and_a_far_column() {
    let mut rig = rig();
    assert_eq!(
        rig.run("forceload add 0 0 256 256"),
        CommandOutcome::Failure
    );
    assert_eq!(rig.texts(), ["commands.forceload.toobig 256 289"]);
    assert_eq!(rig.run("forceload add 30000000 0"), CommandOutcome::Failure);
    assert_eq!(rig.run("forceload sideways"), CommandOutcome::Failure);
}

#[test]
fn forced_chunks_survive_a_restart_through_the_level() {
    let mut rig = rig();
    rig.run("forceload add -20 5");
    rig.server.save_forced_chunks();
    let level = crate::config::LevelData {
        forced_chunks: rig.server.world.level.forced_chunks.clone(),
        ..crate::config::LevelData::default()
    };
    let restored = Server::<Tap>::new(
        crate::config::GameConfig {
            chunk_workers: 1,
            spawn_chunk_radius: 0,
            ..crate::config::GameConfig::default()
        },
        level,
        std::sync::Arc::new(crate::world_state::tests::Quiet),
        std::sync::Arc::new(crate::world_state::tests::Quiet),
        crate::status::StatusBoard::new(1),
    );
    assert_eq!(restored.forced_chunks(), vec![(-2, 0)]);
}

fn border_events(rig: &Rig) -> Vec<rspigot_domain::world_border::BorderView> {
    rig.link
        .take()
        .into_iter()
        .filter_map(|event| match event {
            ClientEvent::Border(view) => Some(view),
            _ => None,
        })
        .collect()
}

#[test]
fn worldborder_get_reports_the_default_size() {
    let mut rig = rig();
    assert_eq!(rig.run("worldborder get"), CommandOutcome::Success);
    assert_eq!(rig.texts(), ["commands.worldborder.get 59999968"]);
}

#[test]
fn worldborder_set_changes_the_size_at_once_and_tells_the_clients() {
    let mut rig = rig();
    assert_eq!(rig.run("worldborder set 100"), CommandOutcome::Success);
    assert_eq!(rig.server.world.border.size(0), 100.0);
    let views = border_events(&rig);
    assert_eq!(views.last().map(|view| view.new_size), Some(100.0));
    assert_eq!(rig.run("worldborder set 100"), CommandOutcome::Failure);
    assert_eq!(rig.run("worldborder set 0.5"), CommandOutcome::Failure);
    assert_eq!(rig.run("worldborder set 70000000"), CommandOutcome::Failure);
}

#[test]
fn worldborder_set_with_a_time_lerps_and_names_the_direction() {
    let mut rig = rig();
    rig.run("worldborder set 100");
    rig.texts();
    assert_eq!(rig.run("worldborder set 50 10s"), CommandOutcome::Success);
    assert_eq!(rig.texts(), ["commands.worldborder.set.shrink 50.0 10.00"]);
    let now = rig.server.world.level.game_time;
    assert_eq!(rig.server.world.border.lerp_time(now), 200);
    assert_eq!(rig.server.world.border.lerp_target(), 50.0);
    assert_eq!(rig.run("worldborder set 400 1s"), CommandOutcome::Success);
    assert_eq!(rig.texts(), ["commands.worldborder.set.grow 400.0 1.00"]);
}

#[test]
fn worldborder_add_grows_from_the_current_size() {
    let mut rig = rig();
    rig.run("worldborder set 100");
    assert_eq!(rig.run("worldborder add 25"), CommandOutcome::Success);
    assert_eq!(rig.server.world.border.size(0), 125.0);
    assert_eq!(rig.run("worldborder add -200"), CommandOutcome::Failure);
}

#[test]
fn worldborder_center_is_checked_and_can_be_relative() {
    let mut rig = rig();
    assert_eq!(
        rig.run("worldborder center 10.5 -20"),
        CommandOutcome::Success
    );
    assert_eq!(
        rig.texts(),
        ["commands.worldborder.center.success 10.50 -20.00"]
    );
    assert_eq!(
        rig.run("worldborder center 10.5 -20"),
        CommandOutcome::Failure
    );
    assert_eq!(
        rig.run("worldborder center 30000000 0"),
        CommandOutcome::Failure
    );
    assert_eq!(rig.run("worldborder center ~ ~"), CommandOutcome::Success);
}

#[test]
fn worldborder_damage_and_warning_settings_refuse_no_ops() {
    let mut rig = rig();
    assert_eq!(
        rig.run("worldborder damage amount 1"),
        CommandOutcome::Success
    );
    assert_eq!(
        rig.texts(),
        ["commands.worldborder.damage.amount.success 1.00"]
    );
    assert_eq!(
        rig.run("worldborder damage amount 1"),
        CommandOutcome::Failure
    );
    assert_eq!(
        rig.run("worldborder damage buffer 2"),
        CommandOutcome::Success
    );
    assert_eq!(
        rig.run("worldborder damage buffer 2"),
        CommandOutcome::Failure
    );
    assert_eq!(
        rig.run("worldborder warning distance 9"),
        CommandOutcome::Success
    );
    assert_eq!(
        rig.run("worldborder warning distance 9"),
        CommandOutcome::Failure
    );
    rig.texts();
    assert_eq!(
        rig.run("worldborder warning time 10s"),
        CommandOutcome::Success
    );
    assert_eq!(
        rig.texts(),
        ["commands.worldborder.warning.time.success 10.00"]
    );
    assert_eq!(
        rig.run("worldborder warning time 10s"),
        CommandOutcome::Failure
    );
    assert_eq!(
        rig.run("worldborder warning distance -1"),
        CommandOutcome::Failure
    );
}

#[test]
fn a_player_beyond_the_buffer_takes_border_damage() {
    let mut rig = rig();
    rig.run("worldborder center 500 500");
    rig.run("worldborder set 10");
    rig.run("worldborder damage amount 0.01");
    rig.run("worldborder damage buffer 0");
    let health = |rig: &Rig| {
        rig.server
            .players
            .get(&rig.id)
            .expect("player")
            .vitals
            .health
    };
    let before = health(&rig);
    for _ in 0..200 {
        rig.server.tick();
        if health(&rig) < before {
            break;
        }
    }
    assert!(health(&rig) < before);
}

#[test]
fn a_player_inside_the_border_takes_no_damage() {
    let mut rig = rig();
    rig.run("worldborder set 1000");
    let health = |rig: &Rig| {
        rig.server
            .players
            .get(&rig.id)
            .expect("player")
            .vitals
            .health
    };
    let before = health(&rig);
    for _ in 0..3 {
        rig.server.tick();
    }
    assert_eq!(health(&rig), before);
}

#[test]
fn the_border_survives_a_restart_through_the_level() {
    let mut rig = rig();
    rig.run("worldborder set 300");
    rig.run("worldborder center 5 6");
    rig.server.save_world_border();
    let level = crate::config::LevelData {
        world_border: rig.server.world.level.world_border.clone(),
        ..crate::config::LevelData::default()
    };
    let restored = Server::<Tap>::new(
        crate::config::GameConfig {
            chunk_workers: 1,
            spawn_chunk_radius: 0,
            ..crate::config::GameConfig::default()
        },
        level,
        std::sync::Arc::new(crate::world_state::tests::Quiet),
        std::sync::Arc::new(crate::world_state::tests::Quiet),
        crate::status::StatusBoard::new(1),
    );
    assert_eq!(restored.world.border.size(0), 300.0);
    assert_eq!(restored.world.border.center(), (5.0, 6.0));
}
