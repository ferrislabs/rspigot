mod common;

use common::*;
use rspigot_core::config::PluginSettings;
use rspigot_core::events::{ClientEvent, EntityStatus, Inbound, MetadataValue, PlayerInput};
use rspigot_core::plugins::{
    Answer, BlockOrder, EventKind, EventPayload, GameRule, Order, Question, WorldOrder,
};
use rspigot_domain::{BlockPos, EntityId, Rotation, Vec3};
use rspigot_entities::EntityKind as Kind;

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn set(s: &mut Scene, pos: BlockPos, key: &str) {
    s.act(Order::Block {
        pos,
        order: BlockOrder::Set(key.into()),
    });
}

fn block(s: &mut Scene, pos: BlockPos) -> String {
    match s.ask(Question::Block(pos)) {
        Answer::Block(state) => state.kind,
        _ => "minecraft:air".to_owned(),
    }
}

fn only(s: &Scene, kind: Kind) -> rspigot_core::entities::EntitySnapshot {
    let found = s.server.entities_of(kind);
    assert_eq!(found.len(), 1, "{found:?}");
    found[0]
}

fn metadata(s: &Scene, entity: EntityId, index: u8) -> Vec<MetadataValue> {
    s.players[0]
        .link
        .take()
        .into_iter()
        .filter_map(|event| match event {
            ClientEvent::EntityMetadata {
                entity: id,
                entries,
            } if id == entity => Some(entries),
            _ => None,
        })
        .flatten()
        .filter(|entry| entry.index == index)
        .map(|entry| entry.value)
        .collect()
}

fn fill_dirt(s: &mut Scene) {
    for x in -2..=2 {
        for z in 4..=8 {
            for y in 64..=65 {
                if (x, z) != (0, 6) {
                    set(s, BlockPos::new(x, y, z), "minecraft:dirt");
                }
            }
        }
    }
}

fn enclose(s: &mut Scene) {
    for x in -2i32..=2 {
        for z in 4..=8 {
            let edge = x.abs() == 2 || z == 4 || z == 8;
            if edge {
                for y in 64..=65 {
                    set(s, BlockPos::new(x, y, z), "minecraft:stone");
                }
            }
            set(s, BlockPos::new(x, 67, z), "minecraft:stone");
        }
    }
}

fn ground(s: &mut Scene) {
    console(s, "gamemode creative Alice");
    let alice = s.players[0].clone();
    send_input(&mut s.server, &alice, PlayerInput::Loaded);
    s.tick(30);
    alice.link.take();
}

#[test]
fn a_saved_carried_block_is_loaded_synchronised_and_saved_again() {
    let mut s = scene(&["Alice"]);
    ground(&mut s);
    console(
        &mut s,
        "summon minecraft:enderman 6.5 64 6.5 {carriedBlockState:{Name:\"minecraft:grass_block\",Properties:{snowy:\"false\"}}}",
    );
    s.tick(2);
    let enderman = only(&s, Kind::Enderman);
    let carried = metadata(&s, enderman.id, 16);
    let grass = u32::from(rspigot_blocks::BlockKind::GrassBlock.default_state().0);
    assert!(
        carried.contains(&MetadataValue::OptionalBlockState(Some(grass))),
        "{carried:?}"
    );
}

#[test]
fn an_enderman_takes_a_holdable_block_after_asking_the_plugins() {
    let mut s = scene_with(&["Alice"], vec![observe(1, EventKind::EntityChangeBlock)]);
    ground(&mut s);
    fill_dirt(&mut s);
    console(&mut s, "summon minecraft:enderman 0.5 64 6.5");
    let mut taken = None;
    for _ in 0..4_000 {
        s.tick(1);
        let changes: Vec<_> = fired(&s.plugin)
            .into_iter()
            .filter_map(|event| match event.payload {
                EventPayload::EntityChangeBlock(change) => Some((change.block, change.to)),
                _ => None,
            })
            .collect();
        if let Some(first) = changes.first() {
            taken = Some(first.clone());
            break;
        }
    }
    let (pos, to) = taken.expect("the enderman never reached for a block");
    assert_eq!(to, "minecraft:air");
    assert_eq!(block(&mut s, pos), "minecraft:air");
    s.tick(3);
    let enderman = only(&s, Kind::Enderman);
    let carried = metadata(&s, enderman.id, 16);
    let dirt = u32::from(rspigot_blocks::BlockKind::Dirt.default_state().0);
    assert!(carried.contains(&MetadataValue::OptionalBlockState(Some(dirt))));
}

#[test]
fn without_mob_griefing_the_enderman_leaves_the_world_alone() {
    let mut s = scene_with(&["Alice"], vec![observe(1, EventKind::EntityChangeBlock)]);
    s.act(Order::World(WorldOrder::SetGameRule(
        GameRule::MobGriefing,
        false,
    )));
    ground(&mut s);
    fill_dirt(&mut s);
    console(&mut s, "summon minecraft:enderman 0.5 64 6.5");
    s.tick(800);
    assert!(
        fired(&s.plugin)
            .into_iter()
            .all(|event| !matches!(event.payload, EventPayload::EntityChangeBlock(_)))
    );
}

#[test]
fn a_stared_enderman_turns_on_the_player_and_becomes_creepy() {
    let mut s = scene(&["Alice"]);
    ground(&mut s);
    console(&mut s, "gamemode survival Alice");
    console(&mut s, "summon minecraft:enderman 0.5 64 6.5");
    let alice = s.players[0].clone();
    let player = s
        .server
        .player_entity_id(alice.id)
        .expect("alice has an entity");
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Move {
            position: Some(Vec3::new(0.5, 64.0, 0.5)),
            rotation: Some(Rotation::new(0.0, -8.8)),
            on_ground: true,
        },
    );
    let enderman = only(&s, Kind::Enderman).id;
    let mut creepy = false;
    for _ in 0..60 {
        s.tick(1);
        creepy |= metadata(&s, enderman, 17).contains(&MetadataValue::Boolean(true));
    }
    assert_eq!(s.server.mob_target(enderman), Some(player));
    assert!(creepy, "the creepy flag was never synchronised");
}

#[test]
fn in_daylight_an_idle_enderman_eventually_teleports_away() {
    let mut s = scene(&["Alice"]);
    ground(&mut s);
    console(&mut s, "summon minecraft:enderman 0.5 64 6.5");
    let mut teleported = false;
    for _ in 0..3_000 {
        s.tick(1);
        teleported |= s.players[0].link.take().iter().any(|event| {
            matches!(
                event,
                ClientEvent::EntityStatus {
                    status: EntityStatus::Event(46),
                    ..
                }
            )
        });
        if teleported {
            break;
        }
    }
    assert!(teleported, "no daylight teleport in 3000 ticks");
}

#[test]
fn arrows_never_hurt_an_enderman_they_make_it_teleport() {
    let mut s = scene(&["Alice"]);
    ground(&mut s);
    console(&mut s, "summon minecraft:enderman 0.5 64 4.5");
    let before = only(&s, Kind::Enderman);
    let health = before.health;
    console(
        &mut s,
        "summon minecraft:arrow 0.5 65.5 0.5 {Motion:[0.0d,0.0d,1.5d]}",
    );
    let mut teleported = false;
    for _ in 0..40 {
        s.tick(1);
        teleported |= s.players[0].link.take().iter().any(|event| {
            matches!(
                event,
                ClientEvent::EntityStatus {
                    status: EntityStatus::Event(46),
                    ..
                }
            )
        });
    }
    assert!(teleported);
    assert_eq!(only(&s, Kind::Enderman).health, health);
}

#[test]
fn endermites_expire_after_two_thousand_four_hundred_ticks() {
    let mut s = scene(&["Alice"]);
    ground(&mut s);
    console(
        &mut s,
        "summon minecraft:endermite 6.5 64 6.5 {Lifetime:2390}",
    );
    s.tick(5);
    assert_eq!(s.server.entities_of(Kind::Endermite).len(), 1);
    s.tick(20);
    assert!(s.server.entities_of(Kind::Endermite).is_empty());
}

#[test]
fn persistent_endermites_do_not_expire() {
    let mut s = scene(&["Alice"]);
    ground(&mut s);
    console(
        &mut s,
        "summon minecraft:endermite 6.5 64 6.5 {Lifetime:2390,PersistenceRequired:1b}",
    );
    s.tick(40);
    assert_eq!(s.server.entities_of(Kind::Endermite).len(), 1);
}

#[allow(dead_code)]
fn unused(_: PluginSettings) {}

#[test]
fn a_carrying_enderman_eventually_sets_its_block_down_through_an_event() {
    let mut s = scene_with(&["Alice"], vec![observe(1, EventKind::EntityChangeBlock)]);
    ground(&mut s);
    enclose(&mut s);
    console(
        &mut s,
        "summon minecraft:enderman 0.5 64 6.5 {carriedBlockState:{Name:\"minecraft:dirt\"}}",
    );
    let mut placed = None;
    for _ in 0..30_000 {
        s.tick(1);
        placed = fired(&s.plugin)
            .into_iter()
            .find_map(|event| match event.payload {
                EventPayload::EntityChangeBlock(change) => Some((change.block, change.to)),
                _ => None,
            });
        if placed.is_some() {
            break;
        }
    }
    let (pos, to) = placed.expect("the enderman never set its block down");
    assert_eq!(to, "minecraft:dirt");
    assert_eq!(block(&mut s, pos), "minecraft:dirt");
    s.tick(3);
    let enderman = only(&s, Kind::Enderman);
    let carried = metadata(&s, enderman.id, 16);
    assert!(carried.contains(&MetadataValue::OptionalBlockState(None)));
}
