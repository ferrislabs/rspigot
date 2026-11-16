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
