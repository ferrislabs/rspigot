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

fn border_updates(rig: &Rig) -> Vec<ClientEvent> {
    rig.link
        .take()
        .into_iter()
        .filter(|event| {
            matches!(
                event,
                ClientEvent::Border(_)
                    | ClientEvent::BorderCenter { .. }
                    | ClientEvent::BorderSize(_)
                    | ClientEvent::BorderLerp { .. }
                    | ClientEvent::BorderWarningDelay(_)
                    | ClientEvent::BorderWarningDistance(_)
            )
        })
        .collect()
}

#[test]
fn border_changes_send_the_matching_packet_to_the_dimension() {
    let mut rig = rig();
    rig.run("worldborder set 100");
    assert_eq!(border_updates(&rig), [ClientEvent::BorderSize(100.0)]);
    rig.run("worldborder set 50 10s");
    assert_eq!(
        border_updates(&rig),
        [ClientEvent::BorderLerp {
            old_size: 100.0,
            new_size: 50.0,
            time: 200
        }]
    );
    rig.run("worldborder center 10.5 -20");
    assert_eq!(
        border_updates(&rig),
        [ClientEvent::BorderCenter { x: 10.5, z: -20.0 }]
    );
    rig.run("worldborder warning distance 9");
    assert_eq!(
        border_updates(&rig),
        [ClientEvent::BorderWarningDistance(9)]
    );
    rig.run("worldborder warning time 10s");
    assert_eq!(border_updates(&rig), [ClientEvent::BorderWarningDelay(200)]);
    rig.run("worldborder damage amount 1");
    assert!(border_updates(&rig).is_empty());
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
    assert_eq!(border_updates(&rig), [ClientEvent::BorderSize(100.0)]);
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

fn pig_positions(rig: &Rig) -> Vec<(f64, f64, f64)> {
    rig.server
        .select_entities(CommandSender::Player(rig.id), "@e[type=pig]")
        .expect("parses")
        .into_iter()
        .filter_map(|target| rig.server.target_position(target))
        .map(|at| (at.x, at.y, at.z))
        .collect()
}

fn floored(rig: &mut Rig) {
    assert_eq!(
        rig.run("fill -12 63 -12 12 63 12 minecraft:stone"),
        CommandOutcome::Success
    );
    rig.texts();
}

#[test]
fn spreadplayers_scatters_entities_over_the_floor_at_least_the_distance_apart() {
    let mut rig = rig();
    floored(&mut rig);
    for index in 0..3 {
        rig.run(&format!("summon minecraft:pig {index} 70 {index}"));
    }
    rig.texts();
    assert_eq!(
        rig.run("spreadplayers 0 0 6 10 under 70 false @e[type=pig]"),
        CommandOutcome::Success
    );
    let spots = pig_positions(&rig);
    assert_eq!(spots.len(), 3);
    for (index, (x, y, z)) in spots.iter().enumerate() {
        assert_eq!(*y, 64.0);
        assert!(x.abs() <= 10.5 && z.abs() <= 10.5, "{x} {z}");
        for (other_x, _, other_z) in &spots[index + 1..] {
            assert!((x - other_x).hypot(z - other_z) >= 4.0);
        }
    }
    let texts = rig.texts();
    assert!(
        texts[0].starts_with("commands.spreadplayers.success.entities 3 0.0 0.0 "),
        "{texts:?}"
    );
}

#[test]
fn spreadplayers_with_teams_puts_a_team_on_one_spot() {
    let mut rig = rig();
    floored(&mut rig);
    rig.run("team add red");
    rig.run("team join red Alice");
    rig.run("summon minecraft:pig 0 70 0");
    rig.texts();
    assert_eq!(
        rig.run("spreadplayers 0 0 6 10 under 70 true @e"),
        CommandOutcome::Success
    );
    let texts = rig.texts();
    assert!(
        texts[0].starts_with("commands.spreadplayers.success.teams 2 0.0 0.0 "),
        "{texts:?}"
    );
}

#[test]
fn spreadplayers_fails_when_there_is_no_room_or_the_height_is_below_the_world() {
    let mut rig = rig();
    floored(&mut rig);
    for index in 0..3 {
        rig.run(&format!("summon minecraft:pig {index} 70 {index}"));
    }
    rig.texts();
    assert_eq!(
        rig.run("spreadplayers 0 0 100 1 under 70 false @e[type=pig]"),
        CommandOutcome::Failure
    );
    assert!(rig.texts()[0].starts_with("commands.spreadplayers.failed.entities 3 0.0 0.0 "),);
    assert_eq!(
        rig.run("spreadplayers 0 0 1 10 under -100 false @e[type=pig]"),
        CommandOutcome::Failure
    );
    assert_eq!(
        rig.texts(),
        ["commands.spreadplayers.failed.invalid.height -100 -64"]
    );
    assert_eq!(
        rig.run("spreadplayers 0 0 1 0.5 false @e[type=pig]"),
        CommandOutcome::Failure
    );
}

fn biome_at(rig: &Rig, x: i32, y: i32, z: i32) -> u16 {
    let pos = rspigot_domain::BlockPos::new(x, y, z);
    let chunk = rig.server.world.chunks.get(pos.chunk()).expect("loaded");
    let min_y = chunk.height().min_y;
    let section = &chunk.sections()[((y - min_y) >> 4) as usize];
    section
        .biome(
            ((x & 15) >> 2) as usize,
            (((y - min_y) & 15) >> 2) as usize,
            ((z & 15) >> 2) as usize,
        )
        .0
}

#[test]
fn fillbiome_rewrites_the_quantized_cells_and_resends_the_chunk() {
    let mut rig = rig();
    for _ in 0..5 {
        rig.server.tick();
    }
    rig.link.take();
    let desert = rspigot_data::registries::biome_id("minecraft:desert").expect("desert");
    assert_ne!(biome_at(&rig, 1, 65, 1), desert);
    assert_eq!(
        rig.run("fillbiome 1 65 1 7 71 7 minecraft:desert"),
        CommandOutcome::Success
    );
    assert_eq!(biome_at(&rig, 1, 65, 1), desert);
    assert_eq!(biome_at(&rig, 5, 69, 5), desert);
    let events = rig.link.take();
    assert!(
        events
            .iter()
            .any(|event| matches!(event, ClientEvent::ChunksBiomes(chunks) if chunks.len() == 1))
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, ClientEvent::Chunk { .. }))
    );
    let texts: Vec<String> = events
        .iter()
        .filter_map(|event| match event {
            ClientEvent::SystemChat { message, .. } => Some(message.plain_text()),
            _ => None,
        })
        .collect();
    assert_eq!(texts, ["commands.fillbiome.success.count 8 0 64 0 4 68 4"]);
}

#[test]
fn fillbiome_replace_only_touches_the_filtered_biome_and_reports_no_change() {
    let mut rig = rig();
    rig.run("fillbiome 0 64 0 7 71 7 minecraft:desert");
    rig.texts();
    assert_eq!(
        rig.run("fillbiome 0 64 0 7 71 7 minecraft:plains replace minecraft:desert"),
        CommandOutcome::Success
    );
    assert_eq!(
        rig.run("fillbiome 0 64 0 7 71 7 minecraft:plains replace minecraft:desert"),
        CommandOutcome::Failure
    );
    assert_eq!(rig.texts()[1], "commands.fillbiome.no_changes");
    assert_eq!(
        rig.run("fillbiome 0 64 0 7 71 7 minecraft:plains"),
        CommandOutcome::Failure
    );
}

#[test]
fn fillbiome_refuses_an_oversized_volume_an_unloaded_area_and_unknown_names() {
    let mut rig = rig();
    assert_eq!(
        rig.run("fillbiome 0 0 0 100 100 100 minecraft:desert"),
        CommandOutcome::Failure
    );
    assert_eq!(rig.texts(), ["commands.fillbiome.toobig 32768 1030301"]);
    assert_eq!(
        rig.run("fillbiome 10000 64 10000 10004 64 10004 minecraft:desert"),
        CommandOutcome::Failure
    );
    assert_eq!(rig.texts(), ["argument.pos.unloaded"]);
    assert_eq!(
        rig.run("fillbiome 0 64 0 4 64 4 minecraft:nowhere"),
        CommandOutcome::Failure
    );
}
