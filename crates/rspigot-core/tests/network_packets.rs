mod common;

use common::*;
use rspigot_core::events::{ClientEvent, Inbound, PlayerInput};
use rspigot_core::server::Server;
use rspigot_domain::EntityId;
use rspigot_entities::EntityKind as Kind;

fn console(server: &mut Server<Link>, line: &str) {
    server.handle(Inbound::Console(line.to_owned()));
    server.tick();
    server.deliver();
}

fn spectate(server: &mut Server<Link>, player: &Player, target: EntityId) -> Vec<ClientEvent> {
    player.link.take();
    server.handle(Inbound::Input {
        player: player.id,
        input: PlayerInput::SpectatorAction(Some(target)),
    });
    server.deliver();
    player.link.take()
}

#[test]
fn a_new_fireball_is_announced_with_its_acceleration_power() {
    let mut s = scene_on(configured(|_| {}), &["Alice"], Vec::new());
    stream_chunk_of(&mut s, rspigot_domain::BlockPos::new(1, 64, 1));
    s.players[0].link.take();
    s.server.handle(Inbound::Console(
        "summon minecraft:fireball 1 64 3".to_owned(),
    ));
    s.tick(5);
    let power = s.players[0]
        .link
        .take()
        .into_iter()
        .find_map(|event| match event {
            ClientEvent::ProjectilePower { power, .. } => Some(power),
            _ => None,
        });
    assert!(power.is_some_and(|power| power > 0.0));
}

#[test]
fn a_spectator_action_sets_the_camera_on_a_pickable_entity_in_reach_only() {
    let mut server = configured(|_| {});
    let alice = join(&mut server, "Alice");
    settle_arrival(&mut server, &alice);
    console(&mut server, "summon minecraft:cow 1 64 1");
    let cow = server.entities_of(Kind::Cow)[0].id;
    let survival = spectate(&mut server, &alice, cow);
    assert!(!survival.iter().any(|e| matches!(e, ClientEvent::Camera(_))));
    console(&mut server, "gamemode spectator Alice");
    let events = spectate(&mut server, &alice, cow);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, ClientEvent::Camera(id) if *id == cow))
    );
    console(&mut server, "summon minecraft:cow 100 64 100");
    let far = server
        .entities_of(Kind::Cow)
        .iter()
        .map(|e| e.id)
        .find(|id| *id != cow)
        .unwrap();
    let events = spectate(&mut server, &alice, far);
    assert!(!events.iter().any(|e| matches!(e, ClientEvent::Camera(_))));
}

#[test]
fn a_light_changing_edit_sends_one_light_update_for_its_chunk() {
    let mut s = scene_on(configured(|_| {}), &["Alice"], Vec::new());
    stream_chunk_of(&mut s, rspigot_domain::BlockPos::new(1, 64, 1));
    s.players[0].link.take();
    s.server.handle(Inbound::Console(
        "setblock 1 70 1 minecraft:glowstone".to_owned(),
    ));
    s.server.handle(Inbound::Console(
        "setblock 2 70 1 minecraft:glowstone".to_owned(),
    ));
    s.tick(2);
    let updates = s.players[0]
        .link
        .take()
        .into_iter()
        .filter(|event| matches!(event, ClientEvent::LightUpdate { .. }))
        .count();
    assert_eq!(updates, 1);
}

#[test]
fn a_minecart_that_moves_is_tracked_with_minecart_steps() {
    let mut s = scene_on(configured(|_| {}), &["Alice"], Vec::new());
    stream_chunk_of(&mut s, rspigot_domain::BlockPos::new(1, 64, 1));
    s.server.handle(Inbound::Console(
        "summon minecraft:minecart 1 75 3".to_owned(),
    ));
    s.tick(2);
    s.players[0].link.take();
    s.tick(10);
    let events = s.players[0].link.take();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, ClientEvent::MinecartMove { steps, .. } if !steps.is_empty()))
    );
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, ClientEvent::EntityMove { .. }))
    );
}

#[test]
fn several_block_changes_in_one_section_arrive_as_one_section_update() {
    let mut s = scene_on(configured(|_| {}), &["Alice"], Vec::new());
    stream_chunk_of(&mut s, rspigot_domain::BlockPos::new(1, 64, 1));
    s.players[0].link.take_raw();
    for x in 1..4 {
        s.server.handle(Inbound::Console(format!(
            "setblock {x} 66 1 minecraft:stone"
        )));
    }
    s.tick(1);
    let events = s.players[0].link.take_raw();
    let sections: Vec<usize> = events
        .iter()
        .filter_map(|e| match e {
            ClientEvent::SectionBlocks { changes, .. } => Some(changes.len()),
            _ => None,
        })
        .collect();
    assert_eq!(sections, [3]);
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, ClientEvent::BlockUpdate { .. }))
    );
}
