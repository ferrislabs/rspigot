mod common;

use std::sync::Arc;

use common::*;
use rspigot_core::events::ClientEvent;
use rspigot_core::plugins::events::{
    PlayerChangedWorld, PlayerPortalPatch, PlayerTeleportCause, PortalCreateReason,
};
use rspigot_core::plugins::{BlockOrder, EventKind, EventPatch, EventPayload, Location, Order};
use rspigot_domain::world::Dimension;
use rspigot_domain::{Identifier, Rotation, Vec3};

fn nether() -> Identifier {
    Identifier::minecraft("the_nether")
}

fn end() -> Identifier {
    Identifier::minecraft("the_end")
}

fn scene_with_dimensions(handlers: Vec<rspigot_core::plugins::HandlerSpec>) -> Scene {
    let mut server = configured(|_| {});
    for (key, dimension) in [(nether(), Dimension::nether()), (end(), Dimension::end())] {
        server.load_dimension(key, dimension, Arc::new(Floor), Arc::new(NoStore));
    }
    scene_on(server, &["Alice"], handlers)
}

fn travel(s: &mut Scene, to: Identifier) {
    let alice = s.players[0].clone();
    s.server.change_dimension(
        alice.id,
        to,
        Vec3::new(8.5, 70.0, 8.5),
        None,
        PlayerTeleportCause::NetherPortal,
    );
    s.server.deliver();
}

#[test]
fn a_portal_cancelled_by_a_plugin_keeps_the_player_home() {
    let mut s = scene_with_dimensions(vec![deciding(1, EventKind::PlayerPortal)]);
    let alice = s.players[0].clone();
    travel(&mut s, nether());
    let asked = last_asked(&s.plugin, EventKind::PlayerPortal);
    rule_on(&mut s.server, 1, asked.id, true, None);
    assert_eq!(
        s.server.player_dimension(alice.id),
        Some(Identifier::minecraft("overworld"))
    );
    assert!(
        !alice
            .link
            .take()
            .iter()
            .any(|e| matches!(e, ClientEvent::Respawn { .. }))
    );
}

#[test]
fn a_portal_destination_changed_by_a_plugin_applies_and_announces_the_new_world() {
    let mut s = scene_with_dimensions(vec![
        deciding(1, EventKind::PlayerPortal),
        observe(2, EventKind::PlayerChangedWorld),
    ]);
    let alice = s.players[0].clone();
    travel(&mut s, nether());
    let asked = last_asked(&s.plugin, EventKind::PlayerPortal);
    rule_on(
        &mut s.server,
        1,
        asked.id,
        false,
        Some(EventPatch::PlayerPortal(PlayerPortalPatch {
            to: Some(Location {
                world: end().to_string(),
                position: Vec3::new(100.5, 49.0, 0.5),
                rotation: Rotation::default(),
            }),
            ..PlayerPortalPatch::default()
        })),
    );
    assert_eq!(s.server.player_dimension(alice.id), Some(end()));
    assert_eq!(
        s.server.player_location(alice.id),
        Some(Vec3::new(100.5, 49.0, 0.5))
    );
    let changed = settled_for(&s.plugin, EventKind::PlayerChangedWorld);
    let EventPayload::PlayerChangedWorld(PlayerChangedWorld { from, player }) =
        &changed.last().expect("announced").payload
    else {
        panic!("expected a world change");
    };
    assert_eq!(from, "minecraft:overworld");
    assert_eq!(player.world, end().to_string());
}

#[test]
fn a_plugin_can_cancel_a_cow_going_through_a_portal_and_sees_it_before_it_leaves() {
    let mut s = scene_with_dimensions(vec![deciding(1, EventKind::EntityPortal)]);
    s.server.handle(rspigot_core::events::Inbound::Console(
        "summon minecraft:cow 8.5 64 8.5 {NoAI:1b}".to_owned(),
    ));
    s.server.deliver();
    let cow = s.server.entities_of(rspigot_entities::EntityKind::Cow)[0].id;
    s.server
        .change_entity_dimension(cow, nether(), Vec3::new(8.5, 70.0, 8.5), None);
    s.server.deliver();
    let asked = last_asked(&s.plugin, EventKind::EntityPortal);
    let EventPayload::EntityPortal(portal) = &asked.payload else {
        panic!("an entity portal");
    };
    assert_eq!(portal.entity.id, cow);
    assert_eq!(portal.from.world, "minecraft:overworld");
    assert_eq!(portal.to.as_ref().unwrap().world, nether().to_string());
    rule_on(&mut s.server, 1, asked.id, true, None);
    assert_eq!(
        s.server
            .entities_of(rspigot_entities::EntityKind::Cow)
            .len(),
        1,
        "the cancelled cow stayed"
    );
}

fn portal_block(s: &mut Scene, x: i32) {
    s.act(Order::Block {
        pos: rspigot_domain::BlockPos::new(x, 64, 3),
        order: BlockOrder::Set("minecraft:end_portal".to_owned()),
    });
}

#[test]
fn an_entity_touching_an_end_portal_tells_the_plugins_which_portal_block_it_is_in() {
    let mut s = scene_with_dimensions(vec![observe(1, EventKind::EntityPortalEnter)]);
    portal_block(&mut s, 5);
    s.server.handle(rspigot_core::events::Inbound::Console(
        "summon minecraft:cow 5.5 64 3.5 {NoAI:1b}".to_owned(),
    ));
    s.server.deliver();
    let cow = s.server.entities_of(rspigot_entities::EntityKind::Cow)[0].id;
    s.tick(2);
    let entered: Vec<_> = fired(&s.plugin)
        .into_iter()
        .filter_map(|event| match event.payload {
            EventPayload::EntityPortalEnter(enter) if enter.entity.id == cow => Some(enter),
            _ => None,
        })
        .collect();
    assert!(!entered.is_empty());
    assert_eq!(entered[0].location.world, "minecraft:overworld");
    assert_eq!(entered[0].location.position, Vec3::new(5.0, 64.0, 3.0));
    assert_eq!(entered[0].entity_type, "minecraft:cow");
}

#[test]
fn a_player_standing_in_an_end_portal_tells_the_plugins_too() {
    let mut s = scene_with_dimensions(vec![observe(1, EventKind::EntityPortalEnter)]);
    portal_block(&mut s, 7);
    s.server.handle(rspigot_core::events::Inbound::Console(
        "tp Alice 7.5 64 3.5".to_owned(),
    ));
    s.server.deliver();
    s.tick(2);
    let kinds: Vec<String> = fired(&s.plugin)
        .into_iter()
        .filter_map(|event| match event.payload {
            EventPayload::EntityPortalEnter(enter) => Some(enter.entity_type),
            _ => None,
        })
        .collect();
    assert!(
        kinds.iter().any(|kind| kind == "minecraft:player"),
        "{kinds:?}"
    );
}

fn enter_end(s: &mut Scene) {
    let alice = s.players[0].clone();
    s.server.change_dimension(
        alice.id,
        end(),
        Vec3::new(8.5, 70.0, 8.5),
        None,
        PlayerTeleportCause::EndPortal,
    );
    s.server.deliver();
}

fn platform_block(s: &mut Scene) -> Option<rspigot_domain::BlockStateId> {
    s.tick(30);
    s.players[0]
        .link
        .take()
        .iter()
        .filter_map(|event| match event {
            ClientEvent::Chunk { dimension, chunk } if *dimension == end() => {
                chunk.block(rspigot_domain::BlockPos::new(100, 48, 0))
            }
            _ => None,
        })
        .next_back()
}

#[test]
fn the_end_platform_is_announced_with_the_obsidian_it_will_place_and_built() {
    let mut s = scene_with_dimensions(vec![observe(1, EventKind::PortalCreate)]);
    let alice = s.players[0].clone();
    enter_end(&mut s);
    let created: Vec<_> = fired(&s.plugin)
        .into_iter()
        .filter_map(|event| match event.payload {
            EventPayload::PortalCreate(create) => Some(create),
            _ => None,
        })
        .collect();
    assert_eq!(created.len(), 1);
    let create = &created[0];
    assert_eq!(create.reason, PortalCreateReason::EndPlatform);
    assert_eq!(create.world, end().to_string());
    assert_eq!(create.blocks.len(), 25);
    assert!(
        create
            .blocks
            .iter()
            .all(|block| block.state.kind == "minecraft:obsidian" && block.pos.y == 48)
    );
    assert_eq!(
        create.entity.as_ref().map(|entity| entity.kind.as_str()),
        Some("minecraft:player")
    );
    assert_eq!(s.server.player_dimension(alice.id), Some(end()));
    let obsidian = rspigot_blocks::BlockKind::Obsidian.default_state();
    assert_eq!(platform_block(&mut s), Some(obsidian));
}

#[test]
fn a_cancelled_end_platform_still_lets_the_player_arrive_but_places_nothing() {
    let mut s = scene_with_dimensions(vec![deciding(1, EventKind::PortalCreate)]);
    let alice = s.players[0].clone();
    enter_end(&mut s);
    assert_eq!(
        s.server.player_dimension(alice.id),
        Some(Identifier::minecraft("overworld"))
    );
    let asked = last_asked(&s.plugin, EventKind::PortalCreate);
    rule_on(&mut s.server, 1, asked.id, true, None);
    assert_eq!(s.server.player_dimension(alice.id), Some(end()));
    let obsidian = rspigot_blocks::BlockKind::Obsidian.default_state();
    assert_ne!(platform_block(&mut s), Some(obsidian));
}

#[test]
fn an_allowed_end_platform_is_built_when_the_decision_settles() {
    let mut s = scene_with_dimensions(vec![deciding(1, EventKind::PortalCreate)]);
    let alice = s.players[0].clone();
    enter_end(&mut s);
    let asked = last_asked(&s.plugin, EventKind::PortalCreate);
    rule_on(&mut s.server, 1, asked.id, false, None);
    assert_eq!(s.server.player_dimension(alice.id), Some(end()));
    let obsidian = rspigot_blocks::BlockKind::Obsidian.default_state();
    assert_eq!(platform_block(&mut s), Some(obsidian));
}
