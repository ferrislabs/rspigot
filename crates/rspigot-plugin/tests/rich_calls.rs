mod fake_host;

use fake_host::*;
use rspigot_plugin::{Click, Part, RichMessage};
use rspigot_plugin_protocol::v1::{self, from_plugin, player_order, rich_click};

#[tokio::test]
async fn a_rich_message_goes_on_the_wire_with_its_events() {
    let mut called = calling(|player| {
        Box::pin(async move {
            player.message.send_rich(
                RichMessage::new().text("Task ").part(
                    Part::new("[Cancel]")
                        .hover("Stops it")
                        .click(Click::Custom("agent:cancel".to_owned())),
                ),
            );
        })
    })
    .await;
    assert_eq!(
        called.session.hear().await,
        from_plugin::Body::Player(v1::PlayerOrder {
            player: STEVE.to_owned(),
            order: Some(player_order::Order::SendRichMessage(v1::RichMessage {
                parts: vec![
                    v1::RichPart {
                        text: "Task ".to_owned(),
                        hover: None,
                        click: None,
                    },
                    v1::RichPart {
                        text: "[Cancel]".to_owned(),
                        hover: Some("Stops it".to_owned()),
                        click: Some(v1::RichClick {
                            action: Some(rich_click::Action::Custom("agent:cancel".to_owned())),
                        }),
                    },
                ],
            })),
        })
    );
    called.finished().await;
}
