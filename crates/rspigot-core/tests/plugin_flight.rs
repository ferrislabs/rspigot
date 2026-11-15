mod common;

use std::sync::Arc;
use std::time::Duration;

use common::*;
use rspigot_core::config::{GameConfig, LevelData, PluginSettings};
use rspigot_core::events::{Direction, Hand, Inbound, MovementAction, PlayerInput};
use rspigot_core::plugins::{
    Answer, EventKind, EventPayload, HandlerSpec, InventoryOrder, ItemSpec, Order, Question,
};
use rspigot_core::ports::ChunkGenerator;
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::world::Dimension;
use rspigot_domain::{BiomeId, BlockPos, BlockStateId, ChunkPos, Rotation, Vec3};
use rspigot_entities::EntityKind as Kind;

struct Lake;

impl ChunkGenerator for Lake {
    fn generate(&self, dimension: &Dimension, pos: ChunkPos) -> ChunkColumn {
        let biome = rspigot_data::registries::biome_id("minecraft:plains").unwrap();
        let mut chunk = ChunkColumn::empty(pos, dimension.height, BiomeId(biome));
        for x in 0..16 {
            for z in 0..16 {
                for y in 60..=61 {
                    chunk.set_block(
                        BlockPos::new(pos.min_block_x() + x, y, pos.min_block_z() + z),
                        BlockStateId(1),
                    );
                }
                let lake = pos.x == 0 && pos.z == 0 && z >= 4;
                for y in 62..=63 {
                    let state = if lake {
                        rspigot_blocks::BlockKind::Water.default_state()
                    } else {
                        BlockStateId(1)
                    };
                    chunk.set_block(
                        BlockPos::new(pos.min_block_x() + x, y, pos.min_block_z() + z),
                        state,
                    );
                }
            }
        }
        chunk
    }

    fn spawn_point(&self, _: &Dimension) -> Vec3 {
        Vec3::new(0.5, 64.0, 0.5)
    }
}

fn plains_server(adjust: impl FnOnce(&mut GameConfig)) -> Server<Link> {
    let mut config = GameConfig {
        view_distance: 3,
        chunk_workers: 2,
        spawn_protection: 0,
        plugins: PluginSettings {
            decision_ticks: 1_000,
            ..PluginSettings::default()
        },
        ..GameConfig::default()
    };
    adjust(&mut config);
    let data = LevelData {
        spawn_mobs: false,
        day_time: 18_000,
        daylight_cycle: false,
        ..LevelData::default()
    };
    let mut server = Server::new(
        config,
        data,
        Arc::new(NoStore),
        Arc::new(Lake),
        StatusBoard::new(20),
    )
    .with_rng_seed(7);
    server.wait_for_spawn(Duration::from_secs(10));
    server
}

fn ready(handlers: Vec<HandlerSpec>) -> Scene {
    ready_with(handlers, |_| {})
}

fn ready_with(handlers: Vec<HandlerSpec>, adjust: impl FnOnce(&mut GameConfig)) -> Scene {
    let mut s = scene_on(plains_server(adjust), &["Alice"], handlers);
    stream_chunk_of(&mut s, BlockPos::new(1, 64, 1));
    s
}

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn wear_elytra(s: &mut Scene) {
    let alice = s.players[0].id;
    s.act(Order::Inventory {
        player: alice,
        order: InventoryOrder::Set {
            slot: 38,
            item: Some(ItemSpec::of("minecraft:elytra", 1)),
        },
    });
}

fn leap(s: &mut Scene) {
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Move {
            position: Some(Vec3::new(0.5, 70.0, 0.5)),
            rotation: Some(Rotation::new(0.0, 0.0)),
            on_ground: false,
        },
    );
    s.tick(1);
}

fn glide(s: &mut Scene) {
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Movement(MovementAction::StartFallFlying),
    );
    s.tick(1);
}

fn toggles(s: &Scene, kind: EventKind) -> Vec<(u64, bool)> {
    asked_for(&s.plugin, kind)
        .into_iter()
        .filter_map(|asked| match asked.payload {
            EventPayload::EntityToggleGlide(event) => Some((asked.id, event.gliding)),
            _ => None,
        })
        .collect()
}

#[test]
fn an_airborne_player_with_elytra_starts_gliding_after_the_plugin_agrees() {
    let mut s = ready(vec![deciding(1, EventKind::EntityToggleGlide)]);
    wear_elytra(&mut s);
    leap(&mut s);
    glide(&mut s);
    let asked = last_asked(&s.plugin, EventKind::EntityToggleGlide);
    let EventPayload::EntityToggleGlide(event) = &asked.payload else {
        panic!("expected a glide event");
    };
    assert!(event.gliding);
    assert_eq!(event.entity_type, "minecraft:player");
    rule_on(&mut s.server, 1, asked.id, false, None);
    s.tick(1);
    let alice = s.players[0].clone();
    let flag = alice.link.take().into_iter().any(|event| {
        matches!(
            event,
            rspigot_core::events::ClientEvent::EntityMetadata { .. }
        )
    });
    assert!(flag);
}

#[test]
fn a_cancelled_glide_never_starts_and_is_asked_again_on_the_next_attempt() {
    let mut s = ready(vec![deciding(1, EventKind::EntityToggleGlide)]);
    wear_elytra(&mut s);
    leap(&mut s);
    glide(&mut s);
    let asked = last_asked(&s.plugin, EventKind::EntityToggleGlide);
    rule_on(&mut s.server, 1, asked.id, true, None);
    s.tick(25);
    assert_eq!(toggles(&s, EventKind::EntityToggleGlide).len(), 1);
    glide(&mut s);
    assert_eq!(toggles(&s, EventKind::EntityToggleGlide).len(), 2);
}

#[test]
fn without_elytra_or_in_the_air_nothing_is_asked() {
    let mut s = ready(vec![deciding(1, EventKind::EntityToggleGlide)]);
    leap(&mut s);
    glide(&mut s);
    assert!(toggles(&s, EventKind::EntityToggleGlide).is_empty());
    wear_elytra(&mut s);
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Move {
            position: Some(Vec3::new(0.5, 64.0, 0.5)),
            rotation: None,
            on_ground: true,
        },
    );
    glide(&mut s);
    assert!(toggles(&s, EventKind::EntityToggleGlide).is_empty());
}

#[test]
fn landing_asks_to_stop_gliding_and_a_cancel_keeps_the_player_in_the_air() {
    let mut s = ready(vec![deciding(1, EventKind::EntityToggleGlide)]);
    wear_elytra(&mut s);
    leap(&mut s);
    glide(&mut s);
    let started = last_asked(&s.plugin, EventKind::EntityToggleGlide);
    rule_on(&mut s.server, 1, started.id, false, None);
    s.tick(2);
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Move {
            position: Some(Vec3::new(0.5, 64.0, 0.5)),
            rotation: None,
            on_ground: true,
        },
    );
    s.tick(2);
    let stops: Vec<_> = toggles(&s, EventKind::EntityToggleGlide)
        .into_iter()
        .filter(|(_, gliding)| !gliding)
        .collect();
    assert_eq!(stops.len(), 1);
    rule_on(&mut s.server, 1, stops[0].0, true, None);
    s.tick(2);
    assert!(toggles(&s, EventKind::EntityToggleGlide).len() >= 3);
}

const EXPLOSIVE: &str = "{LifeTime:2,FireworksItem:{id:\"minecraft:firework_rocket\",count:1,components:{\"minecraft:fireworks\":{explosions:[{shape:\"small_ball\",colors:[I;16711680]}]}}}}";

#[test]
fn a_rocket_at_the_end_of_its_flight_asks_to_explode_and_a_cancel_keeps_it_flying() {
    let mut s = ready(vec![deciding(1, EventKind::FireworkExplode)]);
    console(
        &mut s,
        &format!("summon minecraft:firework_rocket 0.5 64 0.5 {EXPLOSIVE}"),
    );
    s.tick(6);
    let asked = last_asked(&s.plugin, EventKind::FireworkExplode);
    let EventPayload::FireworkExplode(event) = &asked.payload else {
        panic!("expected a firework event");
    };
    assert_eq!(event.entity_type, "minecraft:firework_rocket");
    rule_on(&mut s.server, 1, asked.id, true, None);
    s.tick(1);
    assert_eq!(s.server.entities_of(Kind::FireworkRocket).len(), 1);
    let again = last_asked(&s.plugin, EventKind::FireworkExplode);
    assert_ne!(again.id, asked.id);
    rule_on(&mut s.server, 1, again.id, false, None);
    s.tick(1);
    assert!(s.server.entities_of(Kind::FireworkRocket).is_empty());
}

#[test]
fn an_exploding_rocket_hurts_the_mobs_next_to_it() {
    let mut s = ready(Vec::new());
    console(
        &mut s,
        "summon minecraft:cow 1.5 64 0.5 {NoAI:1b,Silent:1b}",
    );
    let before = s.server.entities_of(Kind::Cow)[0].health.unwrap();
    console(
        &mut s,
        &format!("summon minecraft:firework_rocket 0.5 64 0.5 {EXPLOSIVE}"),
    );
    s.tick(8);
    assert!(s.server.entities_of(Kind::FireworkRocket).is_empty());
    let after = s.server.entities_of(Kind::Cow)[0].health;
    assert!(
        after.is_none_or(|health| health < before),
        "{before} {after:?}"
    );
}

#[test]
fn a_gliding_player_boosts_with_a_rocket_that_is_spent_and_attached() {
    let mut s = ready(Vec::new());
    console(&mut s, "give Alice firework_rocket 3");
    wear_elytra(&mut s);
    leap(&mut s);
    glide(&mut s);
    use_item(&mut s);
    s.tick(1);
    let rockets = s.server.entities_of(Kind::FireworkRocket);
    assert_eq!(rockets.len(), 1);
    assert_eq!(first_slot(&mut s).map(|item| item.count), Some(2));
}

#[test]
fn a_rocket_used_on_the_ground_stands_where_it_was_clicked() {
    let mut s = ready(Vec::new());
    console(&mut s, "give Alice firework_rocket 3");
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::UseItemOn {
            hand: Hand::Main,
            pos: BlockPos::new(2, 63, 2),
            face: Direction::Up,
            cursor: Vec3::new(0.5, 1.0, 0.5),
            inside_block: false,
            sequence: 1,
        },
    );
    s.tick(1);
    let rockets = s.server.entities_of(Kind::FireworkRocket);
    assert_eq!(rockets.len(), 1);
    assert!((rockets[0].position.x - 2.5).abs() < 0.5);
    assert_eq!(first_slot(&mut s).map(|item| item.count), Some(2));
}

fn use_item(s: &mut Scene) {
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::UseItem {
            hand: Hand::Main,
            sequence: 1,
            rotation: Rotation::new(0.0, 0.0),
        },
    );
}

fn first_slot(s: &mut Scene) -> Option<ItemSpec> {
    let alice = s.players[0].id;
    match s.ask(Question::Inventory(alice)) {
        Answer::Inventory(inventory) => inventory.slots[0].clone(),
        other => panic!("expected the inventory, got {other:?}"),
    }
}

#[test]
fn gliding_wears_the_elytra_every_twenty_ticks() {
    let mut s = ready(vec![observe(1, EventKind::PlayerItemDamage)]);
    wear_elytra(&mut s);
    leap(&mut s);
    glide(&mut s);
    s.tick(25);
    let worn = settled_for(&s.plugin, EventKind::PlayerItemDamage);
    assert_eq!(worn.len(), 1);
    let EventPayload::PlayerItemDamage(event) = &worn[0].payload else {
        panic!("expected an item damage event");
    };
    assert_eq!(event.item.key, "minecraft:elytra");
    assert_eq!(event.damage, 1);
}
