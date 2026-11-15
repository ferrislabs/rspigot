mod common;
#[path = "common/screen.rs"]
mod screen;

use common::*;
use rspigot_core::events::{ClientEvent, PlayerInput};
use rspigot_core::plugins::events::{PlayerAdvancementCriterionGrant, PlayerAdvancementDone};
use rspigot_core::plugins::{EventKind, EventPayload};
use rspigot_core::ports::ProgressFile;
use screen::*;

const ROOT: &str = "minecraft:story/root";
const TABLE_RECIPE: &str = "minecraft:crafting_table";

fn updates(player: &Player) -> Vec<ClientEvent> {
    player
        .link
        .take()
        .into_iter()
        .filter(|event| matches!(event, ClientEvent::Advancements { .. }))
        .collect()
}

fn with_table(s: &mut Scene, player: &Player) {
    give(s, player, 0, "minecraft:crafting_table", 1);
    s.tick(2);
    let _ = player;
}

#[test]
fn a_new_player_gets_the_tab_roots_and_their_first_children_without_a_toast() {
    let mut s = scene(&["Alice"]);
    let alice = arrive(&mut s.server, "Bob");
    let first = updates(&alice);
    let ClientEvent::Advancements {
        reset,
        added,
        progress,
        show,
        ..
    } = &first[0]
    else {
        panic!("an update");
    };
    assert!(*reset && !*show && progress.is_empty());
    assert!(added.iter().any(|found| found.id == ROOT));
    assert!(added.iter().all(|found| found.display.is_some()));
    assert!(!added.is_empty() && added.len() < 50);
}

#[test]
fn a_crafting_table_in_the_inventory_completes_the_root_and_unlocks_its_recipe() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    with_table(&mut s, &alice);
    assert!(s.server.done_advancements(alice.id).contains(&ROOT));
    assert!(s.server.known_recipes(alice.id).contains(&TABLE_RECIPE));
    let events = alice.link.take();
    let sent = events.iter().any(|event| {
        matches!(event, ClientEvent::Advancements { reset: false, show: true, progress, .. }
            if progress.iter().any(|(id, criteria)| *id == ROOT
                && criteria.iter().all(|(_, obtained)| obtained.is_some())))
    });
    assert!(sent);
}

#[test]
fn player_advancement_done_carries_the_advancement_key() {
    let mut s = scene_with(
        &["Alice"],
        vec![observe(1, EventKind::PlayerAdvancementDone)],
    );
    let alice = s.players[0].clone();
    with_table(&mut s, &alice);
    let events = fired(&s.plugin);
    let EventPayload::PlayerAdvancementDone(PlayerAdvancementDone { advancement, .. }) =
        &events[0].payload
    else {
        panic!("a completion");
    };
    assert!(events.iter().any(|event| matches!(&event.payload,
        EventPayload::PlayerAdvancementDone(done) if done.advancement == ROOT)));
    assert!(advancement.starts_with("minecraft:"));
}

#[test]
fn a_criterion_grant_is_asked_and_a_cancel_leaves_the_advancement_open() {
    let mut s = scene_with(
        &["Alice"],
        vec![deciding(1, EventKind::PlayerAdvancementCriterionGrant)],
    );
    let alice = s.players[0].clone();
    s.server.award_advancement(alice.id, ROOT);
    let asked = last_asked(&s.plugin, EventKind::PlayerAdvancementCriterionGrant);
    let EventPayload::PlayerAdvancementCriterionGrant(PlayerAdvancementCriterionGrant {
        advancement,
        criterion,
        ..
    }) = &asked.payload
    else {
        panic!("a grant");
    };
    assert_eq!(
        (advancement.as_str(), criterion.as_str()),
        (ROOT, "crafting_table")
    );
    rule_on(&mut s.server, 1, asked.id, true, None);
    assert!(s.server.done_advancements(alice.id).is_empty());
}

#[test]
fn an_allowed_criterion_grant_completes_once_the_plugin_answers() {
    let mut s = scene_with(
        &["Alice"],
        vec![deciding(1, EventKind::PlayerAdvancementCriterionGrant)],
    );
    let alice = s.players[0].clone();
    s.server.award_advancement(alice.id, ROOT);
    let asked = last_asked(&s.plugin, EventKind::PlayerAdvancementCriterionGrant);
    rule_on(&mut s.server, 1, asked.id, false, None);
    assert_eq!(s.server.done_advancements(alice.id), vec![ROOT]);
}

#[test]
fn completing_an_advancement_shows_its_children_and_announces_it_in_chat() {
    let mut s = scene(&["Alice", "Bob"]);
    let alice = s.players[0].clone();
    let bob = s.players[1].clone();
    updates(&alice);
    bob.link.take();
    assert!(
        s.server
            .award_advancement(alice.id, "minecraft:story/mine_stone")
    );
    s.server.deliver();
    let update = updates(&alice);
    let ClientEvent::Advancements {
        added, progress, ..
    } = &update[0]
    else {
        panic!("an update");
    };
    assert!(!added.is_empty());
    assert!(
        progress
            .iter()
            .any(|(id, _)| *id == "minecraft:story/mine_stone")
    );
    let bob_events = bob.link.take();
    let announced = bob_events.iter().any(|event| {
        matches!(event, ClientEvent::SystemChat { message, .. }
            if message.plain_text().contains("advancement"))
    });
    assert!(announced);
}

#[test]
fn an_unknown_advancement_is_refused() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    assert!(!s.server.award_advancement(alice.id, "minecraft:nope"));
}

#[test]
fn opening_a_tab_selects_it_and_closing_the_screen_clears_it() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    alice.link.take();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::AdvancementTab(Some(ROOT.to_owned())),
    );
    assert!(
        alice
            .link
            .take()
            .contains(&ClientEvent::SelectAdvancementsTab(Some(ROOT)))
    );
    send_input(&mut s.server, &alice, PlayerInput::AdvancementTab(None));
    assert!(
        alice
            .link
            .take()
            .contains(&ClientEvent::SelectAdvancementsTab(None))
    );
}

#[test]
fn progress_is_saved_in_the_advancements_file_and_restored_at_login() {
    let (mut s, saves) = saving_scene(&["Alice"], Vec::new());
    let alice = s.players[0].clone();
    s.server.award_advancement(alice.id, ROOT);
    s.server.handle(leave_of(&alice));
    s.server.deliver();
    let text = saves
        .progress(alice.id, ProgressFile::Advancements)
        .expect("an advancements file");
    assert!(text.contains(ROOT) && text.contains("\"done\":true"));
    let back = arrive(&mut s.server, "Alice");
    assert_eq!(s.server.done_advancements(back.id), vec![ROOT]);
    let first = updates(&back);
    let ClientEvent::Advancements { progress, .. } = &first[0] else {
        panic!("an update");
    };
    assert!(progress.iter().any(|(id, _)| *id == ROOT));
}
