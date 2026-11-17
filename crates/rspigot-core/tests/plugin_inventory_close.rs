mod common;

use common::*;
use rspigot_core::events::ClientEvent;
use rspigot_core::plugins::PlayerOrder;

#[test]
fn closing_the_personal_inventory_sends_a_container_close() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    alice.link.take();
    s.act(to(&alice, PlayerOrder::CloseInventory));
    assert!(
        alice
            .link
            .take()
            .iter()
            .any(|event| matches!(event, ClientEvent::ContainerClose(0)))
    );
    assert!(s.warnings().is_empty());
}
