mod common;

use common::*;
use rspigot_core::events::ClientEvent;
use rspigot_core::plugins::{DialogButtonSpec, DialogInputSpec, DialogSpec, PlayerOrder};

fn spec() -> DialogSpec {
    DialogSpec {
        title: "§6Task".to_owned(),
        body: vec!["Describe it".to_owned()],
        inputs: vec![DialogInputSpec {
            key: "task".to_owned(),
            label: "What".to_owned(),
            initial: String::new(),
            max_length: 0,
            multiline: true,
        }],
        buttons: vec![DialogButtonSpec {
            label: "Run".to_owned(),
            id: "agent:run".to_owned(),
        }],
        can_close_with_escape: true,
    }
}

fn shown(alice: &Player) -> Vec<rspigot_domain::dialog::Dialog> {
    alice
        .link
        .take()
        .into_iter()
        .filter_map(|event| match event {
            ClientEvent::ShowDialog(dialog) => Some(*dialog),
            _ => None,
        })
        .collect()
}

#[test]
fn a_dialog_order_shows_the_dialog_to_that_player_only() {
    let mut s = scene(&["Alice", "Bob"]);
    let (alice, bob) = (s.players[0].clone(), s.players[1].clone());
    s.act(to(&alice, PlayerOrder::ShowDialog(spec())));
    let dialogs = shown(&alice);
    assert_eq!(dialogs.len(), 1);
    let dialog = &dialogs[0];
    assert_eq!(dialog.title.plain_text(), "Task");
    assert_eq!(dialog.body[0].plain_text(), "Describe it");
    assert_eq!(dialog.inputs[0].key, "task");
    assert_eq!(dialog.inputs[0].max_length, 256);
    assert!(dialog.inputs[0].multiline);
    assert_eq!(dialog.buttons[0].id.to_string(), "agent:run");
    assert!(shown(&bob).is_empty());
    assert!(s.warnings().is_empty());
}

#[test]
fn an_invalid_dialog_warns_and_shows_nothing() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();

    let mut no_button = spec();
    no_button.buttons.clear();
    let mut bad_id = spec();
    bad_id.buttons[0].id = "Bad Id".to_owned();
    let mut bad_key = spec();
    bad_key.inputs[0].key = "bad key".to_owned();
    for broken in [no_button, bad_id, bad_key] {
        s.act(to(&alice, PlayerOrder::ShowDialog(broken)));
    }
    assert!(shown(&alice).is_empty());
    assert_eq!(s.warnings().len(), 3);
}

#[test]
fn clearing_a_dialog_reaches_the_player() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    s.act(to(&alice, PlayerOrder::ClearDialog));
    assert!(alice.link.take().contains(&ClientEvent::ClearDialog));
}
