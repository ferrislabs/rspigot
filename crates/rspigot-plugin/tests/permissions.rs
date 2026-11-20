mod fake_host;

use fake_host::*;
use rspigot_plugin_protocol::v1::{self, from_plugin};

#[tokio::test]
async fn has_reads_the_rights_carried_by_the_player() {
    let mut called = calling(|player| {
        Box::pin(async move {
            let permissions = player.permissions;
            assert!(permissions.has("kit.use"));
            assert!(permissions.has("chat.color"));
            assert!(permissions.has("kit.other"));
            assert!(!permissions.has("kit.ban"));
            assert!(!permissions.has("world.edit"));
        })
    })
    .await;
    called.finished().await;
}

fn ordered(order: v1::player_order::Order) -> from_plugin::Body {
    from_plugin::Body::Player(v1::PlayerOrder {
        player: STEVE.to_owned(),
        order: Some(order),
    })
}

#[tokio::test]
async fn writing_a_right_sends_one_order_each() {
    let mut called = calling(|player| {
        Box::pin(async move {
            player.permissions.set("shop.use", true);
            player.permissions.set("shop.ban", false);
            player.permissions.unset("shop.use");
            player.permissions.add_group("vip");
            player.permissions.remove_group("vip");
            player.command.refresh();
        })
    })
    .await;
    use v1::player_order::Order;
    for expected in [
        Order::SetPermission(v1::PermissionSet {
            node: "shop.use".to_owned(),
            value: true,
        }),
        Order::SetPermission(v1::PermissionSet {
            node: "shop.ban".to_owned(),
            value: false,
        }),
        Order::UnsetPermission("shop.use".to_owned()),
        Order::AddGroup("vip".to_owned()),
        Order::RemoveGroup("vip".to_owned()),
        Order::RefreshCommands(v1::Empty {}),
    ] {
        assert_eq!(called.session.hear().await, ordered(expected));
    }
    called.finished().await;
}

#[tokio::test]
async fn the_groups_of_the_server_are_listed() {
    let mut called = calling(|player| {
        Box::pin(async move {
            let groups = player.server().permissions.get_groups().await.unwrap();
            assert_eq!(groups.len(), 1);
            assert_eq!(groups[0].name, "vip");
            assert_eq!(groups[0].permissions, ["kit.use"]);
            assert_eq!(groups[0].inherits, ["default"]);
        })
    })
    .await;
    let (request, asked) = called.session.asked().await;
    assert_eq!(
        asked,
        v1::question::Question::PermissionGroups(v1::Empty {})
    );
    called.session.say(answer(
        request,
        v1::answer::Body::PermissionGroups(v1::PermissionGroups {
            groups: vec![v1::PermissionGroup {
                name: "vip".to_owned(),
                permissions: vec!["kit.use".to_owned()],
                inherits: vec!["default".to_owned()],
            }],
        }),
    ));
    called.finished().await;
}

#[tokio::test]
async fn advancements_grant_revoke_and_report_progress() {
    let mut called = calling(|player| {
        Box::pin(async move {
            player.advancements.grant("story/root");
            player.advancements.revoke("minecraft:story/root");
            let progress = player
                .advancements
                .get_progress("story/root")
                .await
                .unwrap();
            assert!(progress.done);
            assert_eq!(progress.criteria_done, ["crafting_table"]);
            assert!(progress.criteria_remaining.is_empty());
            let done = player.advancements.get_done().await.unwrap();
            assert_eq!(done, ["minecraft:story/root"]);
        })
    })
    .await;
    use v1::player_order::Order;
    assert_eq!(
        called.session.hear().await,
        ordered(Order::GrantAdvancement("story/root".to_owned()))
    );
    assert_eq!(
        called.session.hear().await,
        ordered(Order::RevokeAdvancement("minecraft:story/root".to_owned()))
    );
    let (request, asked) = called.session.asked().await;
    assert_eq!(
        asked,
        v1::question::Question::AdvancementProgress(v1::AdvancementQuery {
            player: STEVE.to_owned(),
            advancement: "story/root".to_owned(),
        })
    );
    called.session.say(answer(
        request,
        v1::answer::Body::AdvancementProgress(v1::AdvancementProgress {
            done: true,
            criteria_done: vec!["crafting_table".to_owned()],
            criteria_remaining: Vec::new(),
        }),
    ));
    let (request, asked) = called.session.asked().await;
    assert_eq!(
        asked,
        v1::question::Question::DoneAdvancements(STEVE.to_owned())
    );
    called.session.say(answer(
        request,
        v1::answer::Body::Advancements(v1::AdvancementList {
            ids: vec!["minecraft:story/root".to_owned()],
        }),
    ));
    called.finished().await;
}
