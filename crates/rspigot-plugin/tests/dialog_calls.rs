mod fake_host;

use fake_host::*;
use rspigot_plugin::{Dialog, DialogButton, DialogInput};
use rspigot_plugin_protocol::v1::{self, from_plugin, player_order};

fn ordered(order: player_order::Order) -> from_plugin::Body {
    from_plugin::Body::Player(v1::PlayerOrder {
        player: STEVE.to_owned(),
        order: Some(order),
    })
}

#[tokio::test]
async fn a_dialog_and_its_clearing_go_on_the_wire() {
    let mut called = calling(|player| {
        Box::pin(async move {
            player.dialog.show(
                Dialog::new("Task")
                    .line("Describe it")
                    .input(DialogInput::new("task", "What").initial("x").max_length(80))
                    .button(DialogButton::new("Run", "agent:run"))
                    .locked(),
            );
            player.dialog.clear();
        })
    })
    .await;
    assert_eq!(
        called.session.hear().await,
        ordered(player_order::Order::ShowDialog(v1::Dialog {
            title: "Task".to_owned(),
            body: vec!["Describe it".to_owned()],
            inputs: vec![v1::DialogInput {
                key: "task".to_owned(),
                label: "What".to_owned(),
                initial: "x".to_owned(),
                max_length: 80,
                multiline: false,
            }],
            buttons: vec![v1::DialogButton {
                label: "Run".to_owned(),
                id: "agent:run".to_owned(),
            }],
            can_close_with_escape: false,
        }))
    );
    assert_eq!(
        called.session.hear().await,
        ordered(player_order::Order::ClearDialog(v1::Empty {}))
    );
    called.finished().await;
}
