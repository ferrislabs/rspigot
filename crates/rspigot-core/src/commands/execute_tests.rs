use rspigot_domain::Identifier;
use rspigot_domain::player::PlayerId;

use super::{CommandOutcome, CommandSender, execute};
use crate::end_tests::{Tap, arrive};
use crate::events::ClientEvent;
use crate::server::Server;

fn operator() -> (Server<Tap>, PlayerId, Tap) {
    let (mut server, id, link) = arrive();
    if let Some(player) = server.players.get_mut(&id) {
        player.operator = true;
    }
    (server, id, link)
}

fn respawns(events: &[ClientEvent]) -> usize {
    events
        .iter()
        .filter(|event| matches!(event, ClientEvent::Respawn { .. }))
        .count()
}

#[test]
fn execute_in_the_end_runs_tp_there() {
    let (mut server, id, link) = operator();
    let outcome = execute(
        &mut server,
        CommandSender::Player(id),
        "execute in minecraft:the_end run tp @s 0 80 0",
    );
    server.deliver();
    assert_eq!(outcome, CommandOutcome::Success);
    assert_eq!(
        server.player_dimension(id),
        Some(Identifier::minecraft("the_end"))
    );
    assert_eq!(respawns(&link.take()), 1);
    assert!(server.command_dimension.is_none());
}

#[test]
fn a_plain_tp_stays_in_the_players_world() {
    let (mut server, id, link) = operator();
    execute(&mut server, CommandSender::Player(id), "tp @s 3 80 3");
    server.deliver();
    assert_eq!(
        server.player_dimension(id),
        Some(Identifier::minecraft("overworld"))
    );
    assert_eq!(respawns(&link.take()), 0);
}

#[test]
fn an_unknown_dimension_is_refused() {
    let (mut server, id, link) = operator();
    let outcome = execute(
        &mut server,
        CommandSender::Player(id),
        "execute in minecraft:nowhere run tp @s 0 80 0",
    );
    server.deliver();
    assert_eq!(outcome, CommandOutcome::Failure);
    assert_eq!(
        server.player_dimension(id),
        Some(Identifier::minecraft("overworld"))
    );
    let shown = format!("{:?}", link.take());
    assert!(shown.contains("argument.dimension.invalid"), "{shown}");
}

#[test]
fn execute_in_runs_other_commands_in_that_world() {
    let (mut server, id, _) = operator();
    execute(&mut server, CommandSender::Player(id), "tp @s 0 80 0");
    server.deliver();
    let outcome = execute(
        &mut server,
        CommandSender::Player(id),
        "execute in minecraft:the_end run time set 1000",
    );
    assert_eq!(outcome, CommandOutcome::Success);
    assert_ne!(server.world.level.day_time, 1000);
    assert_eq!(
        server.dormant[&Identifier::minecraft("the_end")]
            .world
            .level
            .day_time,
        1000
    );
}

#[test]
fn execute_is_for_operators() {
    let (mut server, id, _) = arrive();
    let outcome = execute(
        &mut server,
        CommandSender::Player(id),
        "execute in minecraft:the_end run tp @s 0 80 0",
    );
    assert_eq!(outcome, CommandOutcome::Denied);
}

#[test]
fn dimension_keys_are_completed_after_in() {
    use crate::events::{Inbound, PlayerInput};
    let (mut server, id, link) = operator();
    server.handle(Inbound::Input {
        player: id,
        input: PlayerInput::CommandSuggestion {
            id: 7,
            text: "/execute in minecraft:the".to_owned(),
        },
    });
    server.deliver();
    let matches = link.take().into_iter().find_map(|event| match event {
        ClientEvent::CommandSuggestions { matches, .. } => Some(matches),
        _ => None,
    });
    assert_eq!(matches, Some(vec!["minecraft:the_end".to_owned()]));
}
