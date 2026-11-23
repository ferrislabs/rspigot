#[path = "blocks_common/mod.rs"]
mod blocks_common;
mod common;
#[path = "common/screen.rs"]
mod screen;

use blocks_common::*;
use common::*;
use rspigot_blocks::BlockKind;
use rspigot_core::dimension_rules::{GameplayRules, register_gameplay_rules};
use rspigot_core::events::{ClientEvent, PlayerInput};
use rspigot_domain::{BlockPos, Identifier, Vec3};
use screen::use_block;

const ANCHOR: BlockPos = BlockPos::new(12, 64, 12);

fn rig(charges: &str) -> Scene {
    register_gameplay_rules(
        &Identifier::minecraft("overworld"),
        GameplayRules {
            respawn_anchor_works: true,
            ..GameplayRules::default()
        },
    );
    let cells = Cells::default().state(ANCHOR, BlockKind::RespawnAnchor, &[("charges", charges)]);
    let mut s = scene_of(cells, 5, 0);
    stream_chunk_of(&mut s, ANCHOR);
    s.tick(2);
    s
}

fn die_and_respawn(s: &mut Scene) -> Vec3 {
    let alice = s.players[0].clone();
    console(s, "kill Alice");
    s.tick(30);
    alice.link.take();
    send_input(&mut s.server, &alice, PlayerInput::Respawn);
    let mut spot = None;
    for event in alice.link.take() {
        if let ClientEvent::Teleport { id, position, .. } = event {
            spot = Some(position);
            send_input(&mut s.server, &alice, PlayerInput::TeleportConfirm(id));
        }
    }
    send_input(&mut s.server, &alice, PlayerInput::Loaded);
    s.tick(2);
    spot.expect("a respawn teleport")
}

#[test]
fn using_a_charged_anchor_in_a_dimension_where_it_works_sets_the_spawn_and_the_next_death_uses_a_charge()
 {
    let mut s = rig("2");
    let alice = s.players[0].clone();
    use_block(&mut s, &alice, ANCHOR);
    assert_eq!(kind_at(&s, ANCHOR), Some(BlockKind::RespawnAnchor));
    let spot = die_and_respawn(&mut s);
    assert_eq!(spot, Vec3::new(12.5, 64.0, 11.5));
    assert_eq!(property_at(&s, ANCHOR, "charges").as_deref(), Some("1"));
}

#[test]
fn an_anchor_with_its_last_charge_spent_sends_the_player_back_to_the_world_spawn() {
    let mut s = rig("1");
    let alice = s.players[0].clone();
    use_block(&mut s, &alice, ANCHOR);
    let first = die_and_respawn(&mut s);
    assert_eq!(first, Vec3::new(12.5, 64.0, 11.5));
    assert_eq!(property_at(&s, ANCHOR, "charges").as_deref(), Some("0"));
    let second = die_and_respawn(&mut s);
    assert_ne!(second, Vec3::new(12.5, 64.0, 11.5));
}

#[test]
fn a_full_anchor_refuses_glowstone_but_still_sets_the_spawn() {
    let mut s = rig("4");
    console(&mut s, "give Alice minecraft:glowstone 1");
    s.tick(1);
    let alice = s.players[0].clone();
    use_block(&mut s, &alice, ANCHOR);
    assert_eq!(property_at(&s, ANCHOR, "charges").as_deref(), Some("4"));
    assert_eq!(kind_at(&s, ANCHOR), Some(BlockKind::RespawnAnchor));
    assert_eq!(die_and_respawn(&mut s), Vec3::new(12.5, 64.0, 11.5));
    assert_eq!(property_at(&s, ANCHOR, "charges").as_deref(), Some("3"));
}
