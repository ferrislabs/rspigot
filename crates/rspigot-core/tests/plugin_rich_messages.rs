mod common;

use common::*;
use rspigot_core::events::ClientEvent;
use rspigot_core::plugins::{ClickSpec, PlayerOrder, RichPartSpec};
use rspigot_domain::text::ClickEvent;

fn part(text: &str, hover: Option<&str>, click: Option<ClickSpec>) -> RichPartSpec {
    RichPartSpec {
        text: text.to_owned(),
        hover: hover.map(str::to_owned),
        click,
    }
}

fn messages(alice: &Player) -> Vec<rspigot_domain::text::Component> {
    alice
        .link
        .take()
        .into_iter()
        .filter_map(|event| match event {
            ClientEvent::SystemChat {
                message,
                overlay: false,
            } => Some(message),
            _ => None,
        })
        .collect()
}

#[test]
fn a_rich_message_carries_hover_and_click_on_each_part() {
    let mut s = scene(&["Alice", "Bob"]);
    let (alice, bob) = (s.players[0].clone(), s.players[1].clone());
    s.act(to(
        &alice,
        PlayerOrder::SendRichMessage(vec![
            part("Task ", None, None),
            part(
                "[Cancel]",
                Some("Stops the task"),
                Some(ClickSpec::Custom("agent:cancel".to_owned())),
            ),
        ]),
    ));
    let sent = messages(&alice);
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].plain_text(), "Task [Cancel]");
    let (first, second) = (&sent[0].extra[0], &sent[0].extra[1]);
    assert_eq!(first.style.click, None);
    assert_eq!(
        second.style.click,
        Some(Box::new(ClickEvent::Custom("agent:cancel".to_owned())))
    );
    assert_eq!(
        second.style.hover.as_ref().map(|hover| hover.plain_text()),
        Some("Stops the task".to_owned())
    );
    assert!(messages(&bob).is_empty());
    assert!(s.warnings().is_empty());
}

#[test]
fn an_unsafe_or_oversized_part_is_refused() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    let refused = [
        vec![part(
            "x",
            None,
            Some(ClickSpec::OpenUrl("file:///etc/passwd".to_owned())),
        )],
        vec![part(
            "x",
            None,
            Some(ClickSpec::OpenUrl("javascript:1".to_owned())),
        )],
        vec![part(
            "x",
            None,
            Some(ClickSpec::Custom("Bad Id".to_owned())),
        )],
        vec![part(
            "x",
            None,
            Some(ClickSpec::RunCommand("a".repeat(257))),
        )],
        vec![part("x", None, None); 65],
    ];
    for parts in &refused {
        s.act(to(&alice, PlayerOrder::SendRichMessage(parts.clone())));
    }
    assert!(messages(&alice).is_empty());
    assert_eq!(s.warnings().len(), refused.len());
}

#[test]
fn http_and_https_links_and_commands_are_accepted() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    s.act(to(
        &alice,
        PlayerOrder::SendRichMessage(vec![
            part(
                "a",
                None,
                Some(ClickSpec::OpenUrl("https://example.org".to_owned())),
            ),
            part(
                "b",
                None,
                Some(ClickSpec::OpenUrl("http://example.org".to_owned())),
            ),
            part("c", None, Some(ClickSpec::RunCommand("/spawn".to_owned()))),
            part(
                "d",
                None,
                Some(ClickSpec::SuggestCommand("/msg ".to_owned())),
            ),
            part(
                "e",
                None,
                Some(ClickSpec::CopyToClipboard("id-7".to_owned())),
            ),
        ]),
    ));
    assert_eq!(messages(&alice)[0].extra.len(), 5);
    assert!(s.warnings().is_empty());
}
