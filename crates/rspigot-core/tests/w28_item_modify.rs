mod common;

use common::*;
use rspigot_core::events::Inbound;
use rspigot_core::plugins::{Answer, Question};

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn hotbar_zero(s: &mut Scene) -> rspigot_core::plugins::ItemSpec {
    let alice = s.players[0].id;
    let Answer::Inventory(state) = s.ask(Question::Inventory(alice)) else {
        panic!("an inventory");
    };
    state
        .slots
        .into_iter()
        .flatten()
        .next()
        .expect("a held item")
}

fn ready() -> Scene {
    let mut s = scene(&["Alice"]);
    console(&mut s, "scoreboard objectives add n dummy");
    console(&mut s, "scoreboard players set Alice n 7");
    console(&mut s, "give Alice minecraft:stone 1");
    s
}

#[test]
fn item_modify_set_name_resolves_a_score_against_the_executor() {
    let mut s = ready();
    console(
        &mut s,
        r#"item modify entity Alice hotbar.0 {"type":"minecraft:set_name","entity":"this","name":{"score":{"name":"Alice","objective":"n"}}}"#,
    );
    let name = hotbar_zero(&mut s).name.expect("a custom name");
    assert!(name.contains('7'), "{name}");
    assert!(!name.contains("objective"), "{name}");
}

#[test]
fn item_modify_set_lore_resolves_every_line() {
    let mut s = ready();
    console(
        &mut s,
        r#"item modify entity Alice hotbar.0 {"type":"minecraft:set_lore","entity":"this","lore":[{"score":{"name":"Alice","objective":"n"}},"plain"]}"#,
    );
    let lore = hotbar_zero(&mut s).lore;
    assert_eq!(lore.len(), 2);
    assert!(lore[0].contains('7'), "{lore:?}");
    assert!(!lore[0].contains("objective"), "{lore:?}");
}
