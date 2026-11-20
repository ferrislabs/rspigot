mod common;

use common::*;
use rspigot_core::commands::{CommandOutcome, CommandSender, execute};
use rspigot_core::plugins::events::{PlayerAdvancementDone, PlayerRecipeDiscover};
use rspigot_core::plugins::{EventKind, EventPayload};

const STICK: &str = "minecraft:stick";
const ROOT: &str = "minecraft:story/root";

#[test]
fn recipe_give_fires_the_recipe_discover_event_for_each_recipe() {
    let mut s = scene_with(
        &["Alice"],
        vec![observe(1, EventKind::PlayerRecipeDiscover)],
    );
    let alice = s.players[0].clone();
    let outcome = execute(
        &mut s.server,
        CommandSender::Console,
        "recipe give Alice minecraft:stick",
    );
    assert_eq!(outcome, CommandOutcome::Success);
    let events = fired(&s.plugin);
    let EventPayload::PlayerRecipeDiscover(PlayerRecipeDiscover { recipe, .. }) =
        &events[0].payload
    else {
        panic!("a discovery");
    };
    assert_eq!(recipe, STICK);
    assert_eq!(s.server.known_recipes(alice.id), vec![STICK]);
}

#[test]
fn advancement_grant_fires_the_done_event_and_revoke_reopens_it() {
    let mut s = scene_with(
        &["Alice"],
        vec![observe(1, EventKind::PlayerAdvancementDone)],
    );
    let alice = s.players[0].clone();
    let line = format!("advancement grant Alice only {ROOT}");
    assert_eq!(
        execute(&mut s.server, CommandSender::Console, &line),
        CommandOutcome::Success
    );
    let events = fired(&s.plugin);
    assert!(events.iter().any(|event| matches!(&event.payload,
        EventPayload::PlayerAdvancementDone(PlayerAdvancementDone { advancement, .. })
            if advancement == ROOT)));
    assert_eq!(s.server.done_advancements(alice.id), vec![ROOT]);
    let line = format!("advancement revoke Alice only {ROOT}");
    assert_eq!(
        execute(&mut s.server, CommandSender::Console, &line),
        CommandOutcome::Success
    );
    assert!(s.server.done_advancements(alice.id).is_empty());
}
