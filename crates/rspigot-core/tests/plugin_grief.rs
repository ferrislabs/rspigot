mod common;

use common::*;
use rspigot_core::config::PluginSettings;
use rspigot_core::events::{ClientEvent, Inbound};
use rspigot_core::plugins::events::EntityRemoveCause;
use rspigot_core::plugins::{
    Answer, BlockOrder, EventKind, EventPayload, GameRule, HandlerSpec, Order, Question, WorldOrder,
};
use rspigot_domain::BlockPos;

fn waiting_scene(handlers: Vec<HandlerSpec>) -> Scene {
    let mut server = server(PluginSettings {
        decision_ticks: 1_000,
        ..PluginSettings::default()
    });
    let plugin = connect_with(&mut server, 1, "tester", handlers);
    let alice = join(&mut server, "Alice");
    server.deliver();
    alice.link.take();
    Scene {
        server,
        plugin,
        players: vec![alice],
    }
}

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

fn changes(s: &Scene) -> Vec<(BlockPos, String, bool)> {
    fired(&s.plugin)
        .into_iter()
        .filter_map(|event| match event.payload {
            EventPayload::EntityChangeBlock(change) => {
                Some((change.block, change.to, event.cancelled))
            }
            _ => None,
        })
        .collect()
}

#[test]
fn a_falling_block_asks_to_leave_then_spawns_and_asks_to_land() {
    let mut s = scene_with(
        &["Alice"],
        vec![
            observe(1, EventKind::EntityChangeBlock),
            observe(2, EventKind::EntitySpawn),
        ],
    );
    let top = BlockPos::new(3, 70, 3);
    stream_chunk_of(&mut s, top);
    set(&mut s, top, "minecraft:sand");
    s.tick(60);
    let kinds: Vec<EventKind> = fired(&s.plugin)
        .into_iter()
        .map(|event| event.payload.kind())
        .collect();
    assert_eq!(
        kinds,
        [
            EventKind::EntityChangeBlock,
            EventKind::EntitySpawn,
            EventKind::EntityChangeBlock
        ]
    );
    assert_eq!(
        changes(&s),
        [
            (top, "minecraft:air".to_owned(), false),
            (BlockPos::new(3, 64, 3), "minecraft:sand".to_owned(), false),
        ]
    );
    assert_eq!(block(&mut s, BlockPos::new(3, 64, 3)), "minecraft:sand");
}

#[test]
fn a_cancelled_fall_keeps_the_block_and_a_cancelled_landing_drops_nothing() {
    let mut s = waiting_scene(vec![
        deciding(1, EventKind::EntityChangeBlock),
        observe(2, EventKind::EntityRemove),
    ]);
    let top = BlockPos::new(3, 70, 3);
    stream_chunk_of(&mut s, top);
    set(&mut s, top, "minecraft:sand");
    s.tick(2);
    let asked = asked_for(&s.plugin, EventKind::EntityChangeBlock);
    assert_eq!(asked.len(), 1);
    assert_eq!(block(&mut s, top), "minecraft:sand");
    rule_on(&mut s.server, 1, asked[0].id, true, None);
    assert_eq!(block(&mut s, top), "minecraft:sand");
    assert_eq!(s.server.falling_blocks(), 0);

    set(&mut s, top, "minecraft:air");
    set(&mut s, top, "minecraft:gravel");
    s.tick(2);
    let start = last_asked(&s.plugin, EventKind::EntityChangeBlock);
    rule_on(&mut s.server, 1, start.id, false, None);
    assert_eq!(block(&mut s, top), "minecraft:air");
    assert_eq!(s.server.falling_blocks(), 1);
    s.tick(60);
    let landing = last_asked(&s.plugin, EventKind::EntityChangeBlock);
    assert_ne!(landing.id, start.id);
    assert_eq!(s.server.falling_blocks(), 1, "the block waits in the air");
    rule_on(&mut s.server, 1, landing.id, true, None);
    assert_eq!(s.server.falling_blocks(), 0);
    assert_eq!(block(&mut s, BlockPos::new(3, 64, 3)), "minecraft:air");
    assert_eq!(s.server.items_on_ground(), 0);
    let removed: Vec<EntityRemoveCause> = fired(&s.plugin)
        .into_iter()
        .filter_map(|event| match event.payload {
            EventPayload::EntityRemove(removed) => Some(removed.cause),
            _ => None,
        })
        .collect();
    assert_eq!(removed, [EntityRemoveCause::Despawn]);
}

fn graze(s: &mut Scene) {
    let grass = BlockPos::new(4, 63, 4);
    stream_chunk_of(s, grass);
    set(s, grass, "minecraft:grass_block");
    for x in 3..=5 {
        for z in 3..=5 {
            set(s, BlockPos::new(x, 64, z), "minecraft:glass");
        }
    }
    set(s, BlockPos::new(4, 64, 4), "minecraft:air");
    for x in 3..=5 {
        for z in 3..=5 {
            set(s, BlockPos::new(x, 65, z), "minecraft:glass");
        }
    }
    set(s, BlockPos::new(4, 65, 4), "minecraft:air");
    console(
        s,
        "summon minecraft:sheep 4.5 64 4.5 {Age:-24000,Sheared:1b}",
    );
    for _ in 0..1_500 {
        s.tick(1);
        if !settled_for(&s.plugin, EventKind::SheepRegrowWool).is_empty() {
            return;
        }
    }
    panic!("the sheep never grazed");
}

#[test]
fn a_grazing_sheep_asks_to_eat_the_grass_then_to_regrow() {
    let mut s = scene_with(
        &["Alice"],
        vec![
            observe(1, EventKind::EntityChangeBlock),
            observe(2, EventKind::SheepRegrowWool),
        ],
    );
    graze(&mut s);
    assert_eq!(
        changes(&s),
        [(BlockPos::new(4, 63, 4), "minecraft:air".to_owned(), false)]
    );
    assert_eq!(block(&mut s, BlockPos::new(4, 63, 4)), "minecraft:dirt");
    let kinds: Vec<EventKind> = fired(&s.plugin)
        .into_iter()
        .map(|event| event.payload.kind())
        .collect();
    assert_eq!(
        kinds,
        [EventKind::EntityChangeBlock, EventKind::SheepRegrowWool]
    );
}

#[test]
fn without_mob_griefing_the_grazing_is_born_cancelled_but_the_wool_still_regrows() {
    let mut s = scene_with(
        &["Alice"],
        vec![
            deciding(1, EventKind::EntityChangeBlock),
            observe(2, EventKind::EntityChangeBlock),
            observe(3, EventKind::SheepRegrowWool),
        ],
    );
    s.act(Order::World(WorldOrder::SetGameRule(
        GameRule::MobGriefing,
        false,
    )));
    graze(&mut s);
    assert!(asked_for(&s.plugin, EventKind::EntityChangeBlock).is_empty());
    assert_eq!(
        changes(&s),
        [(BlockPos::new(4, 63, 4), "minecraft:air".to_owned(), true)]
    );
    assert_eq!(
        block(&mut s, BlockPos::new(4, 63, 4)),
        "minecraft:grass_block"
    );
}

#[test]
fn a_zombie_asks_to_interact_before_it_breaks_a_turtle_egg() {
    let mut s = waiting_scene(vec![deciding(1, EventKind::EntityInteract)]);
    s.act(Order::World(WorldOrder::SetTime(18_000)));
    let egg = BlockPos::new(5, 64, 5);
    stream_chunk_of(&mut s, egg);
    set(&mut s, egg, "minecraft:turtle_egg");
    console(&mut s, "gamemode spectator Alice");
    console(&mut s, "summon minecraft:zombie 2.5 64 2.5");
    for _ in 0..3_000 {
        s.tick(1);
        if !asked_for(&s.plugin, EventKind::EntityInteract).is_empty() {
            break;
        }
    }
    let asked = last_asked(&s.plugin, EventKind::EntityInteract);
    let EventPayload::EntityInteract(interact) = &asked.payload else {
        panic!("an entity interact");
    };
    assert_eq!(interact.block, egg);
    assert_eq!(interact.entity_type, "minecraft:zombie");
    assert_eq!(block(&mut s, egg), "minecraft:turtle_egg");
    rule_on(&mut s.server, 1, asked.id, true, None);
    assert_eq!(block(&mut s, egg), "minecraft:turtle_egg");
}

fn poofs(player: &Player) -> usize {
    player
        .link
        .take()
        .iter()
        .filter(|event| {
            matches!(event, ClientEvent::Particle(particle) if particle.kind == "minecraft:poof")
        })
        .count()
}

#[test]
fn a_cancelled_turtle_egg_break_sends_no_poof_and_an_allowed_one_does() {
    let mut s = waiting_scene(vec![deciding(1, EventKind::EntityInteract)]);
    let alice = s.players[0].clone();
    s.act(Order::World(WorldOrder::SetTime(18_000)));
    let egg = BlockPos::new(5, 64, 5);
    stream_chunk_of(&mut s, egg);
    set(&mut s, egg, "minecraft:turtle_egg");
    console(&mut s, "gamemode spectator Alice");
    console(&mut s, "summon minecraft:zombie 2.5 64 2.5");
    for _ in 0..3_000 {
        s.tick(1);
        if !asked_for(&s.plugin, EventKind::EntityInteract).is_empty() {
            break;
        }
    }
    s.server.deliver();
    assert_eq!(poofs(&alice), 0);
    let asked = last_asked(&s.plugin, EventKind::EntityInteract);
    rule_on(&mut s.server, 1, asked.id, true, None);
    s.server.deliver();
    assert_eq!(poofs(&alice), 0);
    for _ in 0..600 {
        s.tick(1);
        if last_asked(&s.plugin, EventKind::EntityInteract).id != asked.id {
            break;
        }
    }
    let again = last_asked(&s.plugin, EventKind::EntityInteract);
    assert_ne!(again.id, asked.id);
    s.server.deliver();
    assert_eq!(poofs(&alice), 0);
    rule_on(&mut s.server, 1, again.id, false, None);
    s.server.deliver();
    assert_eq!(poofs(&alice), 20);
    assert_eq!(block(&mut s, egg), "minecraft:air");
}
