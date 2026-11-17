use rspigot_plugin_protocol::v1::{self, from_plugin};
use tokio::sync::mpsc;

use crate::link::Link;
use crate::{PlayerInventoryHandle, PlayerSoundHandle};

#[test]
fn a_generated_handle_sends_the_flat_order_for_its_target() {
    let link = Link::default();
    let (outbound, mut sent) = mpsc::unbounded_channel();
    let _attached = link.attach(outbound);

    PlayerInventoryHandle::new("abc".into(), link.clone()).close();
    PlayerSoundHandle::new("abc".into(), link).play(v1::Sound::default());

    let first = sent.try_recv().unwrap();
    assert_eq!(
        first.body,
        Some(from_plugin::Body::Player(v1::PlayerOrder {
            player: "abc".into(),
            order: Some(v1::player_order::Order::CloseInventory(v1::Empty {})),
        }))
    );
    let second = sent.try_recv().unwrap();
    assert!(matches!(
        second.body,
        Some(from_plugin::Body::Player(v1::PlayerOrder {
            order: Some(v1::player_order::Order::PlaySound(_)),
            ..
        }))
    ));
}
