mod common;

use common::*;
use rspigot_core::config::PluginSettings;
use rspigot_core::events::{ClientEvent, DigAction, Direction, EntityStatus, Inbound, PlayerInput};
use rspigot_core::plugins::events::{CreatureSpawnReason, EntityRemoveCause};
use rspigot_core::plugins::{
    Answer, BlockOrder, EntityOrder, EventKind, EventPayload, GameRule, Order, Question, WorldOrder,
};
use rspigot_domain::{BlockPos, EntityId, Vec3};
use rspigot_entities::EntityKind as Kind;

const INFESTED: &str = "minecraft:infested_stone";
const STONE: &str = "minecraft:stone";
const AIR: &str = "minecraft:air";

fn waiting_scene(handlers: Vec<rspigot_core::plugins::HandlerSpec>) -> Scene {
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

fn set(s: &mut Scene, pos: BlockPos, key: &str) {
    s.act(Order::Block {
        pos,
        order: BlockOrder::Set(key.into()),
    });
}

fn block(s: &mut Scene, pos: BlockPos) -> String {
    match s.ask(Question::Block(pos)) {
        Answer::Block(state) => state.kind,
        _ => AIR.to_owned(),
    }
}

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn silverfish_count(s: &Scene) -> usize {
    s.server.entities_of(Kind::Silverfish).len()
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

fn spawn_reasons(s: &Scene) -> Vec<CreatureSpawnReason> {
    fired(&s.plugin)
        .into_iter()
        .filter_map(|event| match event.payload {
            EventPayload::CreatureSpawn(spawn) => Some(spawn.spawn_reason),
            _ => None,
        })
        .collect()
}

fn poofs(s: &Scene) -> usize {
    s.players[0]
        .link
        .take()
        .into_iter()
        .filter(|event| {
            matches!(
                event,
                ClientEvent::EntityStatus {
                    status: EntityStatus::Event(20),
                    ..
                }
            )
        })
        .count()
}

fn removals(s: &Scene) -> Vec<EntityRemoveCause> {
    fired(&s.plugin)
        .into_iter()
        .filter_map(|event| match event.payload {
            EventPayload::EntityRemove(removed) => Some(removed.cause),
            _ => None,
        })
        .collect()
}

fn infested_scene() -> (Scene, BlockPos) {
    let mut s = scene_with(
        &["Alice"],
        vec![
            observe(1, EventKind::CreatureSpawn),
            observe(2, EventKind::EntityChangeBlock),
        ],
    );
    let pos = BlockPos::new(3, 64, 3);
    stream_chunk_of(&mut s, pos);
    s.act(Order::World(WorldOrder::SetGameRule(
        GameRule::MobGriefing,
        false,
    )));
    set(&mut s, pos, INFESTED);
    (s, pos)
}

#[test]
fn breaking_an_infested_block_naturally_releases_a_silverfish_with_a_poof() {
    let (mut s, pos) = infested_scene();
    s.players[0].link.take();
    s.act(Order::Block {
        pos,
        order: BlockOrder::BreakNaturally,
    });
    s.tick(2);
    assert_eq!(block(&mut s, pos), AIR);
    let found = s.server.entities_of(Kind::Silverfish);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].position, Vec3::new(3.5, 64.0, 3.5));
    assert_eq!(found[0].health, Some(8.0));
    assert_eq!(spawn_reasons(&s), [CreatureSpawnReason::SilverfishBlock]);
    assert_eq!(poofs(&s), 1);
}

#[test]
fn plain_stone_releases_nothing() {
    let (mut s, pos) = infested_scene();
    set(&mut s, pos, STONE);
    s.act(Order::Block {
        pos,
        order: BlockOrder::BreakNaturally,
    });
    s.tick(2);
    assert_eq!(silverfish_count(&s), 0);
}

#[test]
fn without_block_drops_no_silverfish_is_released() {
    let (mut s, pos) = infested_scene();
    s.act(Order::World(WorldOrder::SetGameRule(
        GameRule::BlockDrops,
        false,
    )));
    s.act(Order::Block {
        pos,
        order: BlockOrder::BreakNaturally,
    });
    s.tick(2);
    assert_eq!(block(&mut s, pos), AIR);
    assert_eq!(silverfish_count(&s), 0);
}

fn mine(s: &mut Scene, pos: BlockPos, ticks: usize) {
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Move {
            position: None,
            rotation: None,
            on_ground: true,
        },
    );
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Dig {
            action: DigAction::Start,
            pos,
            face: Direction::Up,
            sequence: 1,
        },
    );
    for _ in 0..ticks {
        s.tick(1);
    }
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Dig {
            action: DigAction::Finish,
            pos,
            face: Direction::Up,
            sequence: 2,
        },
    );
    s.tick(2);
}

#[test]
fn a_survival_player_mining_infested_stone_releases_a_silverfish() {
    let (mut s, _) = infested_scene();
    let pos = BlockPos::new(1, 64, 0);
    set(&mut s, pos, INFESTED);
    s.players[0].link.take();
    mine(&mut s, pos, 40);
    assert_eq!(block(&mut s, pos), AIR);
    assert_eq!(silverfish_count(&s), 1);
    assert_eq!(spawn_reasons(&s), [CreatureSpawnReason::SilverfishBlock]);
}

#[test]
fn a_creative_player_breaking_infested_stone_releases_nothing() {
    let (mut s, _) = infested_scene();
    console(&mut s, "gamemode creative Alice");
    let pos = BlockPos::new(1, 64, 0);
    set(&mut s, pos, INFESTED);
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Dig {
            action: DigAction::Start,
            pos,
            face: Direction::Up,
            sequence: 1,
        },
    );
    s.tick(2);
    assert_eq!(block(&mut s, pos), AIR);
    assert_eq!(silverfish_count(&s), 0);
}

#[test]
fn an_explosion_releases_the_silverfish_of_the_blocks_it_destroys() {
    let (mut s, _) = infested_scene();
    s.act(Order::World(WorldOrder::SetGameRule(
        GameRule::MobGriefing,
        true,
    )));
    let under = BlockPos::new(8, 63, 8);
    set(&mut s, under, INFESTED);
    console(
        &mut s,
        "summon minecraft:creeper 8.5 64 8.5 {NoAI:1b,ignited:1b,Fuse:2s}",
    );
    s.tick(10);
    assert_eq!(block(&mut s, under), AIR);
    assert!(spawn_reasons(&s).contains(&CreatureSpawnReason::SilverfishBlock));
}

struct Wake {
    first: BlockPos,
    second: BlockPos,
    fish: EntityId,
}

fn wake_scene(handlers: Vec<rspigot_core::plugins::HandlerSpec>) -> (Scene, Wake) {
    let mut s = waiting_scene(handlers);
    let origin = BlockPos::new(1, 64, 0);
    stream_chunk_of(&mut s, origin);
    for x in -4..=6 {
        for z in -4..=6 {
            set(&mut s, BlockPos::new(x, 63, z), "minecraft:dirt");
        }
    }
    let first = origin.offset(0, 0, 4);
    let second = origin.offset(2, 0, 0);
    set(&mut s, first, INFESTED);
    set(&mut s, second, INFESTED);
    console(&mut s, "summon minecraft:silverfish 1.5 64 0.5");
    let fish = appeared(&mut s);
    s.tick(6);
    (
        s,
        Wake {
            first,
            second,
            fish,
        },
    )
}

fn appeared(s: &mut Scene) -> EntityId {
    for _ in 0..20 {
        s.tick(1);
        if let Some(fish) = s.server.entities_of(Kind::Silverfish).first() {
            return fish.id;
        }
    }
    panic!("the silverfish never appeared");
}

fn punch(s: &mut Scene, fish: EntityId) {
    let alice = s.players[0].clone();
    send_input(&mut s.server, &alice, PlayerInput::Attack(fish));
}

fn state_of(s: &mut Scene, wake: &Wake) -> [String; 2] {
    [block(s, wake.first), block(s, wake.second)]
}

#[test]
fn a_punched_silverfish_wakes_the_infested_blocks_around_it() {
    let (mut s, wake) = wake_scene(vec![observe(1, EventKind::EntityChangeBlock)]);
    punch(&mut s, wake.fish);
    assert!(changes(&s).is_empty());
    s.tick(30);
    let heard = changes(&s);
    assert!(matches!(heard.len(), 1 | 2), "{heard:?}");
    let mut woken: Vec<BlockPos> = heard.iter().map(|(pos, _, _)| *pos).collect();
    for (pos, to, cancelled) in &heard {
        assert!([wake.first, wake.second].contains(pos));
        assert_eq!((to.as_str(), *cancelled), (AIR, false));
    }
    woken.sort_by_key(|pos| (pos.x, pos.z));
    woken.dedup();
    assert_eq!(woken.len(), heard.len());
    let [first, second] = state_of(&mut s, &wake);
    let air = [&first, &second].into_iter().filter(|b| *b == AIR).count();
    assert_eq!(air, heard.len());
    assert_eq!(silverfish_count(&s), 1 + heard.len());
}

#[test]
fn the_wake_up_scan_waits_about_twenty_ticks_after_the_hit() {
    let (mut s, wake) = wake_scene(vec![observe(1, EventKind::EntityChangeBlock)]);
    punch(&mut s, wake.fish);
    s.tick(15);
    assert_eq!(state_of(&mut s, &wake), [INFESTED, INFESTED]);
    s.tick(10);
    assert!(state_of(&mut s, &wake).contains(&AIR.to_owned()));
}

#[test]
fn a_hit_without_an_attacker_does_not_wake_anything() {
    let (mut s, wake) = wake_scene(vec![observe(1, EventKind::EntityChangeBlock)]);
    s.act(Order::Entity {
        entity: wake.fish,
        order: EntityOrder::Damage(1.0),
    });
    s.tick(40);
    assert_eq!(block(&mut s, wake.first), INFESTED);
    assert_eq!(block(&mut s, wake.second), INFESTED);
}

#[test]
fn cancelled_wake_up_events_keep_every_block_and_the_scan_goes_on() {
    let (mut s, wake) = wake_scene(vec![deciding(1, EventKind::EntityChangeBlock)]);
    punch(&mut s, wake.fish);
    for _ in 0..40 {
        s.tick(1);
        if !asked_for(&s.plugin, EventKind::EntityChangeBlock).is_empty() {
            break;
        }
    }
    let asked = asked_for(&s.plugin, EventKind::EntityChangeBlock);
    assert_eq!(asked.len(), 1);
    assert_eq!(block(&mut s, wake.first), INFESTED);
    rule_on(&mut s.server, 1, asked[0].id, true, None);
    s.tick(1);
    let asked_again = asked_for(&s.plugin, EventKind::EntityChangeBlock);
    assert_eq!(asked_again.len(), 2);
    rule_on(&mut s.server, 1, asked_again[1].id, true, None);
    s.tick(5);
    assert_eq!(asked_for(&s.plugin, EventKind::EntityChangeBlock).len(), 2);
    assert_eq!(block(&mut s, wake.first), INFESTED);
    assert_eq!(block(&mut s, wake.second), INFESTED);
    assert_eq!(silverfish_count(&s), 1);
}

#[test]
fn without_mob_griefing_the_blocks_become_plain_stone_again() {
    let (mut s, wake) = wake_scene(vec![observe(1, EventKind::EntityChangeBlock)]);
    s.act(Order::World(WorldOrder::SetGameRule(
        GameRule::MobGriefing,
        false,
    )));
    punch(&mut s, wake.fish);
    s.tick(40);
    let heard = changes(&s);
    assert!(!heard.is_empty());
    for (pos, to, cancelled) in &heard {
        assert_eq!((to.as_str(), *cancelled), (AIR, false));
        assert_eq!(block(&mut s, *pos), STONE);
    }
    assert_eq!(silverfish_count(&s), 1);
}

fn far_silverfish(handlers: Vec<rspigot_core::plugins::HandlerSpec>, griefing: bool) -> Scene {
    let mut s = waiting_scene(handlers);
    s.act(Order::World(WorldOrder::SetGameRule(
        GameRule::MobGriefing,
        griefing,
    )));
    let spot = BlockPos::new(2, 64, 10);
    stream_chunk_of(&mut s, spot);
    console(&mut s, "gamemode spectator Alice");
    console(&mut s, "summon minecraft:silverfish 2.5 64 10.5");
    s.tick(1);
    s
}

fn run_until_asked(s: &mut Scene, kind: EventKind, limit: u32) -> bool {
    for _ in 0..limit {
        s.tick(1);
        if !asked_for(&s.plugin, kind).is_empty() {
            return true;
        }
    }
    false
}

#[test]
fn an_idle_silverfish_merges_into_stone_and_vanishes() {
    let mut s = far_silverfish(
        vec![
            observe(1, EventKind::EntityChangeBlock),
            observe(2, EventKind::EntityRemove),
        ],
        true,
    );
    for _ in 0..4_000 {
        s.tick(1);
        if silverfish_count(&s) == 0 {
            break;
        }
    }
    assert_eq!(silverfish_count(&s), 0);
    let heard = changes(&s);
    assert_eq!(heard.len(), 1, "{heard:?}");
    let (pos, to, cancelled) = &heard[0];
    assert_eq!((to.as_str(), *cancelled), (INFESTED, false));
    assert_eq!(block(&mut s, *pos), INFESTED);
    assert_eq!(removals(&s), [EntityRemoveCause::EnterBlock]);
    assert!(poofs(&s) <= 1);
}

#[test]
fn a_cancelled_merge_leaves_the_silverfish_and_the_block_alone() {
    let mut s = far_silverfish(vec![deciding(1, EventKind::EntityChangeBlock)], true);
    assert!(run_until_asked(&mut s, EventKind::EntityChangeBlock, 4_000));
    let asked = last_asked(&s.plugin, EventKind::EntityChangeBlock);
    let EventPayload::EntityChangeBlock(change) = &asked.payload else {
        panic!("a block change");
    };
    assert_eq!(change.to, INFESTED);
    assert_eq!(block(&mut s, change.block), STONE);
    rule_on(&mut s.server, 1, asked.id, true, None);
    s.tick(2);
    assert_eq!(silverfish_count(&s), 1);
    assert_eq!(block(&mut s, change.block), STONE);
}

#[test]
fn without_mob_griefing_a_silverfish_never_merges() {
    let mut s = far_silverfish(vec![observe(1, EventKind::EntityChangeBlock)], false);
    s.tick(900);
    assert_eq!(silverfish_count(&s), 1);
    assert!(changes(&s).is_empty());
}

#[test]
fn a_silverfish_with_a_target_does_not_merge() {
    let mut s = waiting_scene(vec![observe(1, EventKind::EntityChangeBlock)]);
    let pad: Vec<BlockPos> = (0..=12)
        .flat_map(|x| (-4..=4).map(move |z| BlockPos::new(x, 63, z)))
        .collect();
    for pos in &pad {
        set(&mut s, *pos, "minecraft:dirt");
    }
    console(&mut s, "summon minecraft:silverfish 6.5 64 0.5");
    let fish = loop {
        s.tick(1);
        let found = s.server.entities_of(Kind::Silverfish);
        if let Some(fish) = found.first()
            && s.server.mob_target(fish.id).is_some()
        {
            break fish.id;
        }
    };
    for pos in &pad {
        set(&mut s, *pos, STONE);
    }
    s.tick(150);
    assert!(s.server.mob_target(fish).is_some());
    assert!(changes(&s).is_empty());
    assert_eq!(silverfish_count(&s), 1);
}
