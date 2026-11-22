mod common;

use std::time::{Duration, SystemTime};

use common::*;
use rspigot_core::events::Inbound;
use rspigot_core::management::{
    ListAction, ManagementCall, ManagementReply, ManagementRequest, PlayerRef,
};
use rspigot_core::user_cache::{CachedProfile, UserCache};
use uuid::Uuid;

fn manage(
    server: &mut rspigot_core::server::Server<Link>,
    request: ManagementRequest,
) -> ManagementReply {
    let (reply, answer) = crossbeam_channel::bounded(1);
    server.handle(Inbound::Management(ManagementCall { request, reply }));
    answer.recv().expect("answered")
}

#[test]
fn a_login_is_written_to_the_user_cache() {
    let (sink, published) = crossbeam_channel::unbounded();
    let mut server = configured(|config| {
        config.user_cache = UserCache::default().with_sink(sink);
    });
    join(&mut server, "Alice");
    let written = published.try_iter().last().expect("published");
    assert_eq!(written.len(), 1);
    assert_eq!(written[0].name, "Alice");
}

#[test]
fn an_offline_player_is_resolved_through_the_user_cache() {
    let known = Uuid::from_u128(0x1234);
    let expires = SystemTime::now() + Duration::from_secs(3600 * 24);
    let mut server = configured(|config| {
        config.user_cache = UserCache::new(
            vec![CachedProfile {
                name: "Ghost".to_owned(),
                uuid: known,
                expires,
            }],
            1000,
        );
    });
    let reply = manage(
        &mut server,
        ManagementRequest::Allowlist(ListAction::Add(vec![PlayerRef::named("ghost")])),
    );
    assert_eq!(
        reply,
        ManagementReply::Allowlist(vec![PlayerRef {
            id: Some(known),
            name: Some("Ghost".to_owned()),
        }])
    );
}

#[test]
fn op_of_an_offline_name_uses_the_cached_uuid() {
    use rspigot_core::plugins::{Order, ServerOrder};
    let known = Uuid::from_u128(0x99);
    let (sink, published) = crossbeam_channel::unbounded();
    let server = configured(|config| {
        config.user_cache = UserCache::new(
            vec![CachedProfile {
                name: "Ghost".to_owned(),
                uuid: known,
                expires: SystemTime::now() + Duration::from_secs(3600),
            }],
            1000,
        );
    })
    .with_admin_sink(sink);
    let mut scene = scene_on(server, &[], Vec::new());
    scene.act(Order::Server(ServerOrder::OperatorAdd("Ghost".into())));
    let lists = published.try_iter().last().expect("published");
    assert_eq!(lists.operators[0].uuid, Some(known));
}
