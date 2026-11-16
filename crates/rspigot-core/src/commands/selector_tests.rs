use rspigot_domain::player::{GameMode, PlayerId};

use super::{CommandContext, CommandSender, Target};
use crate::end_tests::{Tap, arrive};
use crate::server::Server;

fn world() -> (Server<Tap>, PlayerId) {
    let (server, id, _) = arrive();
    (server, id)
}

fn count(server: &Server<Tap>, id: PlayerId, selector: &str) -> usize {
    server
        .select_entities(CommandSender::Player(id), selector)
        .expect("the selector parses")
        .len()
}

fn mutate(
    server: &mut Server<Tap>,
    id: PlayerId,
    change: impl FnOnce(&mut crate::player::Player<Tap>),
) {
    change(server.players.get_mut(&id).expect("the player is online"));
}

#[test]
fn gamemode_keeps_only_players_in_that_mode() {
    let (mut server, id) = world();
    mutate(&mut server, id, |p| p.game_mode = GameMode::Creative);
    assert_eq!(count(&server, id, "@a[gamemode=creative]"), 1);
    assert_eq!(count(&server, id, "@a[gamemode=survival]"), 0);
    assert_eq!(count(&server, id, "@a[gamemode=!survival]"), 1);
    assert_eq!(count(&server, id, "@e[gamemode=creative]"), 1);
}

#[test]
fn level_matches_the_experience_level_range() {
    let (mut server, id) = world();
    mutate(&mut server, id, |p| p.vitals.experience.level = 7);
    assert_eq!(count(&server, id, "@a[level=5..]"), 1);
    assert_eq!(count(&server, id, "@a[level=..6]"), 0);
    assert_eq!(count(&server, id, "@a[level=7]"), 1);
}

#[test]
fn rotation_ranges_wrap_around() {
    let (mut server, id) = world();
    mutate(&mut server, id, |p| {
        p.rotation.yaw = 170.0;
        p.rotation.pitch = 30.0;
    });
    assert_eq!(count(&server, id, "@a[y_rotation=160..-170]"), 1);
    assert_eq!(count(&server, id, "@a[y_rotation=-90..90]"), 0);
    assert_eq!(count(&server, id, "@a[x_rotation=0..45]"), 1);
    assert_eq!(count(&server, id, "@a[x_rotation=..10]"), 0);
}

#[test]
fn nbt_is_a_partial_match_on_the_saved_data() {
    let (mut server, id) = world();
    mutate(&mut server, id, |p| p.game_mode = GameMode::Creative);
    assert_eq!(count(&server, id, "@a[nbt={playerGameType:1}]"), 1);
    assert_eq!(count(&server, id, "@a[nbt={playerGameType:0}]"), 0);
    assert_eq!(count(&server, id, "@a[nbt=!{playerGameType:0}]"), 1);
}

#[test]
fn teams_scores_and_predicates_follow_an_empty_scoreboard() {
    let (server, id) = world();
    assert_eq!(count(&server, id, "@a[team=]"), 1);
    assert_eq!(count(&server, id, "@a[team=red]"), 0);
    assert_eq!(count(&server, id, "@a[team=!red]"), 1);
    assert_eq!(count(&server, id, "@a[scores={kills=1..}]"), 0);
    assert_eq!(count(&server, id, "@a[predicate=minecraft:nothing]"), 0);
    assert_eq!(count(&server, id, "@a[predicate=!minecraft:nothing]"), 1);
}

#[test]
fn advancements_match_done_state_and_criteria() {
    let (mut server, id) = world();
    let first = rspigot_data::advancements::all()[0].id;
    let selector = format!("@a[advancements={{{first}=true}}]");
    assert_eq!(count(&server, id, &selector), 0);
    assert!(server.award_advancement(id, first));
    assert_eq!(count(&server, id, &selector), 1);
    let negated = format!("@a[advancements={{{first}=false}}]");
    assert_eq!(count(&server, id, &negated), 0);
    assert_eq!(
        count(&server, id, "@a[advancements={minecraft:missing/one=true}]"),
        0
    );
}

#[test]
fn a_selector_target_is_the_player() {
    let (server, id) = world();
    let found = server
        .select_entities(CommandSender::Player(id), "@a[limit=1,sort=nearest]")
        .expect("the selector parses");
    assert_eq!(found, vec![Target::Player(id)]);
}
