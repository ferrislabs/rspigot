mod fake_host;

use fake_host::*;

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
