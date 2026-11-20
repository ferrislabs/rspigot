mod common;

use common::*;
use rspigot_core::plugins::{Answer, PlayerOrder, Question};

const ROOT: &str = "minecraft:story/root";
const MINE_STONE: &str = "minecraft:story/mine_stone";

fn progress(
    s: &mut Scene,
    player: &Player,
    advancement: &str,
) -> Option<(bool, Vec<String>, Vec<String>)> {
    match s.ask(Question::AdvancementProgress {
        player: player.id,
        advancement: advancement.to_owned(),
    }) {
        Answer::AdvancementProgress(view) => {
            Some((view.done, view.criteria_done, view.criteria_remaining))
        }
        Answer::NotFound => None,
        other => panic!("unexpected {other:?}"),
    }
}

fn done(s: &mut Scene, player: &Player) -> Vec<String> {
    match s.ask(Question::DoneAdvancements(player.id)) {
        Answer::Advancements(ids) => ids,
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn granting_completes_every_criterion_and_shows_in_the_progress() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    let (finished, got, left) = progress(&mut s, &alice, ROOT).unwrap();
    assert!(!finished && got.is_empty() && !left.is_empty());
    s.act(to(&alice, PlayerOrder::GrantAdvancement(ROOT.to_owned())));
    let (finished, got, left) = progress(&mut s, &alice, ROOT).unwrap();
    assert!(finished && left.is_empty());
    assert!(!got.is_empty());
    assert!(done(&mut s, &alice).contains(&ROOT.to_owned()));
}

#[test]
fn a_name_without_namespace_is_a_minecraft_advancement() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    s.act(to(
        &alice,
        PlayerOrder::GrantAdvancement("story/root".to_owned()),
    ));
    assert!(progress(&mut s, &alice, "story/root").unwrap().0);
}

#[test]
fn revoking_takes_every_criterion_back() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    s.act(to(
        &alice,
        PlayerOrder::GrantAdvancement(MINE_STONE.to_owned()),
    ));
    assert!(progress(&mut s, &alice, MINE_STONE).unwrap().0);
    s.act(to(
        &alice,
        PlayerOrder::RevokeAdvancement(MINE_STONE.to_owned()),
    ));
    let (finished, got, left) = progress(&mut s, &alice, MINE_STONE).unwrap();
    assert!(!finished && got.is_empty() && !left.is_empty());
    assert!(!done(&mut s, &alice).contains(&MINE_STONE.to_owned()));
}

#[test]
fn an_unknown_advancement_is_a_warning_and_a_missing_answer() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    s.act(to(
        &alice,
        PlayerOrder::GrantAdvancement("minecraft:nope/x".to_owned()),
    ));
    s.act(to(
        &alice,
        PlayerOrder::RevokeAdvancement("minecraft:nope/x".to_owned()),
    ));
    assert_eq!(s.warnings().len(), 2);
    assert!(progress(&mut s, &alice, "minecraft:nope/x").is_none());
}

#[test]
fn the_progress_of_an_offline_player_is_not_found() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    s.server.handle(leave_of(&alice));
    s.server.deliver();
    assert!(progress(&mut s, &alice, ROOT).is_none());
}
